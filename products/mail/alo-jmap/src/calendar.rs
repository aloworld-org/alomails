//! Personal-calendar HTTP surface (Agenda slice 1). Authenticated, tenant/user-
//! scoped through the account door — every handler resolves the caller with
//! [`authenticate`] and touches only that account's events.
//!
//! - `GET  /calendar/events?from=&to=` — events overlapping `[from, to)`.
//! - `POST /calendar/events` — create; returns `{id}`.
//! - `PUT  /calendar/events/:id` — replace.
//! - `DELETE /calendar/events/:id` — delete.
//!
//! Times cross the wire as RFC 3339 (UTC), on the user's single implicit
//! calendar. Events may recur (an iCalendar `RRULE`) and carry guest addresses;
//! saving an event with guests mails each an iMIP `METHOD:REQUEST` invitation
//! from the owner's address (see [`send_invitations`]). CalDAV sync of the same
//! events lives in `carddav`. No event content is logged (Law 1).

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use serde::Deserialize;
use serde_json::{Value, json};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use alo_store::{BlobId, CalendarEvent, EventId, StoreError};

use crate::error::Problem;
use crate::state::{Account, AppState, authenticate};

/// Widest window a single range query may span (guards a pathological pull).
const MAX_RANGE_DAYS: i64 = 400;

#[derive(Deserialize)]
pub struct RangeQuery {
    from: String,
    to: String,
}

#[derive(Deserialize)]
struct EventBody {
    summary: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    location: Option<String>,
    #[serde(rename = "startsAt")]
    starts_at: String,
    #[serde(rename = "endsAt")]
    ends_at: String,
    #[serde(default, rename = "allDay")]
    all_day: bool,
    /// An iCalendar RRULE (e.g. `FREQ=WEEKLY`) or empty/absent for a one-off.
    #[serde(default)]
    recurrence: Option<String>,
    /// Guest email addresses to invite (iMIP invitations are mailed on save).
    #[serde(default)]
    attendees: Vec<String>,
}

/// `GET /calendar/events?from=&to=` → `{"events": [...]}`.
pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<RangeQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let from = parse_time(&q.from)?;
    let to = parse_time(&q.to)?;
    if to <= from || (to - from).whole_days() > MAX_RANGE_DAYS {
        return Err(Problem::with(
            StatusCode::BAD_REQUEST,
            "invalid or too-wide time range",
        ));
    }
    let events = account
        .acc
        .events_in_range(from, to)
        .await
        .map_err(|_| Problem::server_error())?;
    let out: Vec<Value> = events.iter().map(event_json).collect();
    Ok(Json(json!({ "events": out })))
}

/// `GET /calendar/events/:id` → the stored (unexpanded) event. Used by the
/// editor to load a recurring event's series template, since the list returns
/// expanded occurrences that share the master's id.
pub async fn get_one(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    match account
        .acc
        .event(&EventId::new(id))
        .await
        .map_err(|_| Problem::server_error())?
    {
        Some(e) => Ok(Json(event_json(&e))),
        None => Err(Problem::with(StatusCode::NOT_FOUND, "no such event")),
    }
}

/// `POST /calendar/events` → `{id, ...}` (the created event).
pub async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: EventBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let event = build_event(EventId::generate(), req)?;
    let id = account
        .acc
        .create_event(&event)
        .await
        .map_err(|_| Problem::server_error())?;
    let saved = CalendarEvent { id, ..event };
    send_invitations(&state, &account, &saved).await;
    Ok(Json(event_json(&saved)))
}

/// `PUT /calendar/events/:id` → `{status:"ok"}`.
pub async fn update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: EventBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let event = build_event(EventId::new(id.clone()), req)?;
    account
        .acc
        .update_event(&EventId::new(id), &event)
        .await
        .map_err(map_store_err)?;
    send_invitations(&state, &account, &event).await;
    Ok(Json(json!({ "status": "ok" })))
}

#[derive(Deserialize)]
pub struct DeleteQuery {
    /// When present, delete only this one occurrence of a recurring series (its
    /// start-instant, RFC 3339) by adding an `EXDATE` — the rest of the series
    /// stays. Absent → delete the whole event/series.
    occurrence: Option<String>,
}

/// `DELETE /calendar/events/:id[?occurrence=<rfc3339>]`.
///
/// - With `occurrence`: skip just that instance of a recurring series (the
///   series and every other instance remain); syncs to phones as an `EXDATE`.
///   Guests are not (yet) emailed a per-occurrence cancellation — that rides
///   with the edit-one-occurrence slice.
/// - Without it: delete the whole event; if it had guests, each is emailed a
///   cancellation so their calendar removes it too.
pub async fn delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(q): Query<DeleteQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let eid = EventId::new(id);
    if let Some(occ) = q.occurrence {
        let when = parse_time(&occ)?;
        account
            .acc
            .exclude_occurrence(&eid, when)
            .await
            .map_err(map_store_err)?;
        return Ok(Json(json!({ "status": "ok", "scope": "occurrence" })));
    }
    // Read the event before deleting so we know whom to notify.
    let event = account
        .acc
        .event(&eid)
        .await
        .map_err(|_| Problem::server_error())?;
    account.acc.delete_event(&eid).await.map_err(map_store_err)?;
    if let Some(ev) = event {
        send_cancellations(&state, &account, &ev).await;
    }
    Ok(Json(json!({ "status": "ok", "scope": "series" })))
}

#[derive(Deserialize)]
struct RsvpBody {
    /// The invitation message's blob id (the ownership boundary — loading it is
    /// scoped to the caller's account).
    #[serde(rename = "blobId")]
    blob_id: String,
    /// `accepted`, `declined`, or `tentative`.
    response: String,
}

/// `POST /calendar/rsvp` — respond to an inbound invitation. Loads the message
/// (account-scoped), reads its iMIP `REQUEST`, adds the event to the caller's
/// calendar (unless declining), and emails a `METHOD:REPLY` to the organizer
/// carrying the chosen status. Returns `{status, added, replied}`.
pub async fn rsvp(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: RsvpBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let partstat = match req.response.as_str() {
        "accepted" => "ACCEPTED",
        "declined" => "DECLINED",
        "tentative" => "TENTATIVE",
        _ => {
            return Err(Problem::with(
                StatusCode::BAD_REQUEST,
                "response must be accepted, declined, or tentative",
            ));
        }
    };
    // The blob load is the tenant/account ownership boundary: a foreign or
    // unreferenced blob is NotFound from the store itself.
    let raw = account
        .acc
        .blob_bytes(&BlobId::new(req.blob_id))
        .await
        .map_err(map_store_err)?;
    let not_invite = || Problem::with(StatusCode::BAD_REQUEST, "this message is not an invitation");
    let ics_bytes = crate::mime_read::calendar_part(&raw).ok_or_else(not_invite)?;
    let ics = String::from_utf8_lossy(&ics_bytes);
    if alo_store::ical::method_of(&ics).as_deref() != Some("REQUEST") {
        return Err(not_invite());
    }
    let event = alo_store::ical::from_ics(&ics, "").ok_or_else(|| {
        Problem::with(StatusCode::BAD_REQUEST, "the invitation could not be read")
    })?;
    // Declining doesn't clutter the calendar; accept/tentative land the event
    // (upsert keyed on the organizer's UID, so a re-RSVP updates in place).
    let added = partstat != "DECLINED";
    if added {
        account
            .acc
            .put_event(&event.id, &event)
            .await
            .map_err(|_| Problem::server_error())?;
    }
    let organizer = alo_store::ical::organizer_of(&ics);
    let replied = send_reply(&state, &account, &event, partstat, organizer.as_deref()).await;
    Ok(Json(json!({ "status": "ok", "added": added, "replied": replied })))
}

#[derive(Deserialize)]
struct CancelBody {
    #[serde(rename = "blobId")]
    blob_id: String,
}

/// `POST /calendar/cancel` — apply an organizer's cancellation. Loads the
/// message (account-scoped), confirms it is a `METHOD:CANCEL`, and removes the
/// matching event (by `UID`) from the caller's calendar. Removing an event that
/// isn't there (declined, or already removed) is success with `removed:false` —
/// the cancellation is honoured either way. Re-reading the message server-side
/// means a client can't ask to delete an arbitrary id: only the `UID` named by
/// a real cancellation the user received is acted on.
pub async fn cancel(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: CancelBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let raw = account
        .acc
        .blob_bytes(&BlobId::new(req.blob_id))
        .await
        .map_err(map_store_err)?;
    let not_cancel =
        || Problem::with(StatusCode::BAD_REQUEST, "this message is not a cancellation");
    let ics_bytes = crate::mime_read::calendar_part(&raw).ok_or_else(not_cancel)?;
    let ics = String::from_utf8_lossy(&ics_bytes);
    if alo_store::ical::method_of(&ics).as_deref() != Some("CANCEL") {
        return Err(not_cancel());
    }
    let uid = alo_store::ical::uid_of(&ics).ok_or_else(not_cancel)?;
    let removed = match account.acc.delete_event(&EventId::new(uid)).await {
        Ok(()) => true,
        Err(StoreError::NotFound) => false,
        Err(_) => return Err(Problem::server_error()),
    };
    Ok(Json(json!({ "status": "ok", "removed": removed })))
}

/// Emails a `METHOD:REPLY` to the invitation's organizer, from the responder's
/// own address, carrying their participation status. Best-effort: the RSVP is
/// already recorded on the calendar, so a missing organizer or send failure is
/// logged — never with addresses or content (Law 1) — and reported as
/// `replied: false` rather than failing the request.
async fn send_reply(
    state: &AppState,
    account: &Account,
    event: &CalendarEvent,
    partstat: &str,
    organizer: Option<&str>,
) -> bool {
    let Some(organizer) = organizer else {
        tracing::warn!("calendar: invitation has no organizer; no reply sent");
        return false;
    };
    let Some(addr) = state.submission_addr.as_deref() else {
        return false;
    };
    let responder = match state
        .store
        .for_tenant(account.tenant.clone())
        .email_of(&account.user)
        .await
    {
        Ok(Some(email)) => email,
        _ => {
            tracing::warn!("calendar: responder address unknown; no reply sent");
            return false;
        }
    };
    let ics = alo_store::ical::to_reply(event, organizer, &responder, partstat);
    let verb = match partstat {
        "ACCEPTED" => "Accepted",
        "DECLINED" => "Declined",
        _ => "Tentative",
    };
    let subject = crate::mime::encode_unstructured(&format!("{verb}: {}", event.summary));
    let plain_b64 = wrap76(&B64.encode(format!("{responder} responded: {partstat}.")));
    let ics_b64 = wrap76(&B64.encode(ics));
    let message = format!(
        "From: {responder}\r\n\
         To: {organizer}\r\n\
         Subject: {subject}\r\n\
         MIME-Version: 1.0\r\n\
         Content-Type: multipart/alternative; boundary=\"{IMIP_BOUNDARY}\"\r\n\
         \r\n\
         --{IMIP_BOUNDARY}\r\n\
         Content-Type: text/plain; charset=utf-8\r\n\
         Content-Transfer-Encoding: base64\r\n\
         \r\n\
         {plain_b64}\r\n\
         --{IMIP_BOUNDARY}\r\n\
         Content-Type: text/calendar; charset=utf-8; method=REPLY\r\n\
         Content-Transfer-Encoding: base64\r\n\
         \r\n\
         {ics_b64}\r\n\
         --{IMIP_BOUNDARY}--\r\n"
    );
    let rcpts = [organizer.to_owned()];
    match crate::submission::submit(addr, &responder, &rcpts, message.as_bytes()).await {
        Ok(()) => true,
        Err(reason) => {
            tracing::warn!(reason = %reason, "calendar: could not send the RSVP reply");
            false
        }
    }
}

/// Validates and assembles a [`CalendarEvent`] from a request body.
fn build_event(id: EventId, req: EventBody) -> Result<CalendarEvent, Problem> {
    let summary = req.summary.trim().to_owned();
    if summary.is_empty() {
        return Err(Problem::with(StatusCode::BAD_REQUEST, "a title is required"));
    }
    let starts_at = parse_time(&req.starts_at)?;
    let ends_at = parse_time(&req.ends_at)?;
    if ends_at < starts_at {
        return Err(Problem::with(
            StatusCode::BAD_REQUEST,
            "the event ends before it starts",
        ));
    }
    let clean = |s: Option<String>| s.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty());
    Ok(CalendarEvent {
        id,
        summary,
        description: clean(req.description),
        location: clean(req.location),
        starts_at,
        ends_at,
        all_day: req.all_day,
        recurrence: clean(req.recurrence),
        attendees: req
            .attendees
            .into_iter()
            .map(|a| a.trim().to_owned())
            // A plausible address, and no whitespace/control chars — the latter
            // would let a crafted address inject headers into the invitation.
            .filter(|a| {
                a.contains('@')
                    && a.len() <= 254
                    && !a.contains(|c: char| c.is_whitespace() || c.is_control())
            })
            .collect(),
        // A new event carries no exceptions; excluding an occurrence is a
        // separate action (DELETE with an `occurrence`). CalDAV PUTs preserve
        // any EXDATE via put_event/from_ics, not this create path.
        exdates: Vec::new(),
    })
}

fn parse_time(s: &str) -> Result<OffsetDateTime, Problem> {
    OffsetDateTime::parse(s, &Rfc3339)
        .map(|t| t.to_offset(time::UtcOffset::UTC))
        .map_err(|_| Problem::with(StatusCode::BAD_REQUEST, "invalid date/time (expected RFC 3339)"))
}

fn event_json(e: &CalendarEvent) -> Value {
    json!({
        "id": e.id.as_str(),
        "summary": e.summary,
        "description": e.description,
        "location": e.location,
        "startsAt": e.starts_at.format(&Rfc3339).unwrap_or_default(),
        "endsAt": e.ends_at.format(&Rfc3339).unwrap_or_default(),
        "allDay": e.all_day,
        "recurrence": e.recurrence,
        "attendees": e.attendees,
    })
}

fn map_store_err(e: StoreError) -> Problem {
    match e {
        StoreError::NotFound => Problem::with(StatusCode::NOT_FOUND, "no such event"),
        _ => Problem::server_error(),
    }
}

/// MIME boundary for the invitation's `multipart/alternative`. A fixed token is
/// safe: it never occurs in a base64-encoded part body.
const IMIP_BOUNDARY: &str = "=_alo_imip_iX9q2p";

/// Mails each guest an iMIP invitation (`METHOD:REQUEST`) from the organizer's
/// address, so recipients on any calendar (Gmail/Outlook/Apple) get a real,
/// RSVP-able invite. A save on an existing event re-issues the request (same
/// `UID`), which those clients treat as an update.
async fn send_invitations(state: &AppState, account: &Account, event: &CalendarEvent) {
    notify_attendees(
        state,
        account,
        event,
        "REQUEST",
        "Invitation",
        &format!("You're invited to {}.", event.summary),
    )
    .await;
}

/// Mails each guest an iMIP cancellation (`METHOD:CANCEL`) when the owner
/// deletes an event, so their calendar (and alomails') removes it. Same `UID`,
/// so clients match it to the original.
async fn send_cancellations(state: &AppState, account: &Account, event: &CalendarEvent) {
    notify_attendees(
        state,
        account,
        event,
        "CANCEL",
        "Cancelled",
        &format!("{} has been cancelled.", event.summary),
    )
    .await;
}

/// Mails every guest an iMIP scheduling message for `event` from the owner's
/// address: a `multipart/alternative` of a short note and a `text/calendar;
/// method={method}` part. Best-effort — the calendar write already happened, so
/// a missing listener/organizer or a send failure is logged (never with
/// addresses or content — Law 1) rather than failing the request.
async fn notify_attendees(
    state: &AppState,
    account: &Account,
    event: &CalendarEvent,
    method: &str,
    subject_prefix: &str,
    plain: &str,
) {
    if event.attendees.is_empty() {
        return;
    }
    let Some(addr) = state.submission_addr.as_deref() else {
        tracing::warn!("calendar: no submission listener; {method} not sent");
        return;
    };
    let organizer = match state
        .store
        .for_tenant(account.tenant.clone())
        .email_of(&account.user)
        .await
    {
        Ok(Some(email)) => email,
        _ => {
            tracing::warn!("calendar: organizer address unknown; {method} not sent");
            return;
        }
    };
    let subject =
        crate::mime::encode_unstructured(&format!("{subject_prefix}: {}", event.summary));
    let plain_b64 = wrap76(&B64.encode(plain));
    let ics_b64 = wrap76(&B64.encode(alo_store::ical::to_imip(event, &organizer, method)));
    for attendee in &event.attendees {
        let message = format!(
            "From: {organizer}\r\n\
             To: {attendee}\r\n\
             Subject: {subject}\r\n\
             MIME-Version: 1.0\r\n\
             Content-Type: multipart/alternative; boundary=\"{IMIP_BOUNDARY}\"\r\n\
             \r\n\
             --{IMIP_BOUNDARY}\r\n\
             Content-Type: text/plain; charset=utf-8\r\n\
             Content-Transfer-Encoding: base64\r\n\
             \r\n\
             {plain_b64}\r\n\
             --{IMIP_BOUNDARY}\r\n\
             Content-Type: text/calendar; charset=utf-8; method={method}\r\n\
             Content-Transfer-Encoding: base64\r\n\
             \r\n\
             {ics_b64}\r\n\
             --{IMIP_BOUNDARY}--\r\n"
        );
        if let Err(reason) = crate::submission::submit(
            addr,
            &organizer,
            std::slice::from_ref(attendee),
            message.as_bytes(),
        )
        .await
        {
            tracing::warn!(reason = %reason, "calendar: could not send a {method}");
        }
    }
}

/// Wraps a base64 string to 76-column lines (RFC 2045) with CRLF separators.
fn wrap76(b64: &str) -> String {
    b64.as_bytes()
        .chunks(76)
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect::<Vec<_>>()
        .join("\r\n")
}
