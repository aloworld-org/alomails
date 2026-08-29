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

use alo_store::{
    BlobId, Calendar, CalendarEvent, CalendarId, EventId, OccurrenceOverride, StoreError, UserId,
};

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
    /// The IANA zone whose wall-clock a recurring event follows across DST
    /// (e.g. `Europe/Brussels`); empty/absent → occurrences repeat at fixed
    /// UTC instants. On a whole-series update, an *absent* field keeps the
    /// stored zone; an empty string clears it. An unknown name is refused.
    #[serde(default)]
    timezone: Option<String>,
    /// Guest email addresses to invite (iMIP invitations are mailed on save).
    #[serde(default)]
    attendees: Vec<String>,
    /// Which calendar to place the event on; absent → the personal calendar.
    #[serde(default, rename = "calendarId")]
    calendar_id: Option<String>,
    /// Reminder lead-time in minutes before the start, or absent for none.
    #[serde(default, rename = "reminderMinutes")]
    reminder_minutes: Option<i32>,
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
    let calendar_id = resolve_calendar(&account, &req).await?;
    let id = EventId::generate();
    let event = build_event(id.clone(), calendar_id, req)?;
    // Any room the guest list names is held first: a refusal (409) must leave
    // no half-made meeting behind, so the reservation is taken under the id the
    // event is about to be written at.
    let rooms = booked_resources(&account, &event).await?;
    account
        .acc
        .book_resources(&id, &event, &rooms)
        .await
        .map_err(Problem::from)?;
    // create_event denies a calendar the caller can't edit with NotFound → 404,
    // so map through the store-error translator rather than a blanket 500.
    if let Err(error) = account.acc.create_event_at(&id, &event).await {
        // The room was reserved for a meeting that never happened — give it back.
        let _ = account.acc.unbook_event(&id).await;
        return Err(map_store_err(error));
    }
    let saved = CalendarEvent { id, ..event };
    send_invitations(&state, &account, &saved).await;
    Ok(Json(event_json(&saved)))
}

#[derive(Deserialize)]
pub struct UpdateQuery {
    /// When present, edit only the single occurrence of a recurring series whose
    /// original start is this instant (RFC 3339) — a per-occurrence override
    /// (iCalendar `RECURRENCE-ID`). Absent → replace the whole event/series.
    occurrence: Option<String>,
}

/// `PUT /calendar/events/:id[?occurrence=<rfc3339>]` → `{status:"ok"}`.
///
/// - With `occurrence`: override just that instance of a recurring series (move
///   and/or re-title it) while the rest stays. The instance is addressed by its
///   ORIGINAL slot, so re-editing it is stable even after it was moved.
/// - Without it: replace the whole event/series (and re-issue invitations).
pub async fn update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(q): Query<UpdateQuery>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: EventBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let eid = EventId::new(id);
    if let Some(occ) = q.occurrence {
        let recurrence_id = parse_time(&occ)?;
        let ov = build_override(&req)?;
        account
            .acc
            .override_occurrence(&eid, recurrence_id, &ov)
            .await
            .map_err(map_store_err)?;
        // If the series has guests, tell them this one instance moved: a REQUEST
        // update carrying the same UID with a RECURRENCE-ID at the original slot.
        if let Ok(Some(master)) = account.acc.event(&eid).await
            && !master.attendees.is_empty()
        {
            let occurrence = CalendarEvent {
                summary: ov.summary.clone(),
                description: ov.description.clone(),
                location: ov.location.clone(),
                starts_at: ov.starts_at,
                ends_at: ov.ends_at,
                all_day: ov.all_day,
                recurrence: None,
                recurrence_id: Some(recurrence_id),
                exdates: Vec::new(),
                ..master
            };
            send_invitations(&state, &account, &occurrence).await;
        }
        return Ok(Json(json!({ "status": "ok", "scope": "occurrence" })));
    }
    let calendar_id = resolve_calendar(&account, &req).await?;
    let timezone_given = req.timezone.is_some();
    let mut event = build_event(eid.clone(), calendar_id, req)?;
    // The JSON body cannot express a series' exceptions (EXDATEs/RDATEs, which
    // arrive via DELETE?occurrence= and CalDAV) and usually omits the zone —
    // a whole-series edit carries them forward rather than resurrecting
    // cancelled instances or unpinning the wall-clock. An explicit `timezone`
    // (including `""` to clear) still wins.
    if let Ok(Some(prev)) = account.acc.event(&eid).await {
        event.exdates = prev.exdates;
        event.rdates = prev.rdates;
        if !timezone_given && !event.all_day {
            event.timezone = prev.timezone;
        }
    }
    // Re-hold the rooms the edited guest list names — against the new times, so
    // moving a meeting into an hour its room is taken is refused (409) before
    // anything is written. A room dropped from the list is released here.
    let rooms = booked_resources(&account, &event).await?;
    account
        .acc
        .book_resources(&eid, &event, &rooms)
        .await
        .map_err(Problem::from)?;
    if let Err(error) = account.acc.update_event(&eid, &event).await {
        // Not the caller's event after all: hold nothing on its behalf.
        let _ = account.acc.unbook_event(&eid).await;
        return Err(map_store_err(error));
    }
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
///   If the event has guests, each is emailed a one-instance `CANCEL` (same UID
///   with a `RECURRENCE-ID`) so that occurrence drops off their calendar too.
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
    let occurrence = match q.occurrence {
        Some(occ) => Some(parse_time(&occ)?),
        None => None,
    };
    cancel_core(&state, &account, &eid, occurrence).await
}

/// The delete/cancel itself, shared between the route above and the Agenda
/// agent's `cancel_event` executor (`crate::agenda_intents`) so "what
/// cancelling a meeting does" — the store write and the guests' `CANCEL`
/// mails — is decided in exactly one place.
pub(crate) async fn cancel_core(
    state: &AppState,
    account: &Account,
    eid: &EventId,
    occurrence: Option<OffsetDateTime>,
) -> Result<Json<Value>, Problem> {
    if let Some(when) = occurrence {
        // Read the event first, so we can tell guests just this instance is off.
        let event = account
            .acc
            .event(eid)
            .await
            .map_err(|_| Problem::server_error())?;
        account
            .acc
            .exclude_occurrence(eid, when)
            .await
            .map_err(map_store_err)?;
        if let Some(ev) = event
            && !ev.attendees.is_empty()
        {
            // A CANCEL for one instance: the same UID with a RECURRENCE-ID at the
            // cancelled slot and no RRULE, so clients drop only that one.
            let duration = ev.ends_at - ev.starts_at;
            let occurrence = CalendarEvent {
                starts_at: when,
                ends_at: when + duration,
                recurrence: None,
                recurrence_id: Some(when),
                exdates: Vec::new(),
                ..ev
            };
            send_cancellations(state, account, &occurrence).await;
        }
        return Ok(Json(json!({ "status": "ok", "scope": "occurrence" })));
    }
    // Read the event before deleting so we know whom to notify.
    let event = account
        .acc
        .event(eid)
        .await
        .map_err(|_| Problem::server_error())?;
    account.acc.delete_event(eid).await.map_err(map_store_err)?;
    if let Some(ev) = event {
        send_cancellations(state, account, &ev).await;
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
    // The blob load is the tenant/account ownership boundary: a foreign or
    // unreferenced blob is NotFound from the store itself.
    let raw = account
        .acc
        .blob_bytes(&BlobId::new(req.blob_id))
        .await
        .map_err(map_store_err)?;
    let (_, added, replied) = rsvp_core(&state, &account, &raw, &req.response).await?;
    Ok(Json(
        json!({ "status": "ok", "added": added, "replied": replied }),
    ))
}

/// The RSVP itself, given the invitation message's raw bytes: reads its iMIP
/// `REQUEST`, lands the event on the caller's personal calendar (unless
/// declining), and emails a `METHOD:REPLY` to the organizer. Returns the event
/// as read from the invitation, whether it was added, and whether the reply
/// was sent. Shared between the route above and the Agenda agent's
/// `respond_to_invitation` executor (`crate::agenda_intents`), so "what
/// answering an invitation does" is decided in exactly one place.
pub(crate) async fn rsvp_core(
    state: &AppState,
    account: &Account,
    raw: &[u8],
    response: &str,
) -> Result<(CalendarEvent, bool, bool), Problem> {
    let partstat = match response {
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
    let not_invite = || Problem::with(StatusCode::BAD_REQUEST, "this message is not an invitation");
    let ics_bytes = crate::mime_read::calendar_part(raw).ok_or_else(not_invite)?;
    let ics = String::from_utf8_lossy(&ics_bytes);
    if alo_store::ical::method_of(&ics).as_deref() != Some("REQUEST") {
        return Err(not_invite());
    }
    let mut event = alo_store::ical::from_ics(&ics, "").ok_or_else(|| {
        Problem::with(StatusCode::BAD_REQUEST, "the invitation could not be read")
    })?;
    // An accepted invitation lands on the recipient's personal calendar.
    event.calendar_id = account
        .acc
        .ensure_personal_calendar()
        .await
        .map_err(|_| Problem::server_error())?;
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
    let replied = send_reply(state, account, &event, partstat, organizer.as_deref()).await;
    Ok((event, added, replied))
}

#[derive(Deserialize)]
struct CancelBody {
    #[serde(rename = "blobId")]
    blob_id: String,
}

/// `POST /calendar/cancel` — apply an organizer's cancellation. Loads the
/// message (account-scoped), confirms it is a `METHOD:CANCEL`, and applies it
/// to the caller's calendar: a CANCEL naming a single instance (`RECURRENCE-ID`,
/// RFC 5546 §3.2.5) excludes just that occurrence of the stored series (an
/// `EXDATE` — the rest of the series stays), while one without removes the
/// whole event (by `UID`). Cancelling what isn't there (declined, or already
/// removed) is success with `removed:false` — the cancellation is honoured
/// either way; the response's `scope` says which shape was applied. Re-reading
/// the message server-side means a client can't ask to delete an arbitrary id:
/// only what a real cancellation the user received names is acted on.
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
    let not_cancel = || {
        Problem::with(
            StatusCode::BAD_REQUEST,
            "this message is not a cancellation",
        )
    };
    let ics_bytes = crate::mime_read::calendar_part(&raw).ok_or_else(not_cancel)?;
    let ics = String::from_utf8_lossy(&ics_bytes);
    if alo_store::ical::method_of(&ics).as_deref() != Some("CANCEL") {
        return Err(not_cancel());
    }
    let uid = alo_store::ical::uid_of(&ics).ok_or_else(not_cancel)?;
    // A CANCEL naming one instance excludes that occurrence; the series stays.
    if let Some(recurrence_id) = alo_store::ical::recurrence_id_of(&ics) {
        let removed = match account
            .acc
            .exclude_occurrence(&EventId::new(uid), recurrence_id)
            .await
        {
            Ok(()) => true,
            Err(StoreError::NotFound) => false,
            Err(_) => return Err(Problem::server_error()),
        };
        return Ok(Json(
            json!({ "status": "ok", "removed": removed, "scope": "occurrence" }),
        ));
    }
    let removed = match account.acc.delete_event(&EventId::new(uid)).await {
        Ok(()) => true,
        Err(StoreError::NotFound) => false,
        Err(_) => return Err(Problem::server_error()),
    };
    Ok(Json(
        json!({ "status": "ok", "removed": removed, "scope": "series" }),
    ))
}

/// `POST /calendar/apply-reply` — record a guest's reply on the organizer's
/// event. Loads the reply message (account-scoped), reads its iMIP `REPLY`
/// (the replying attendee + `PARTSTAT`), and merges that status onto the event
/// the caller organizes (matched by `UID`). Returns `{applied, email, status}`;
/// `applied:false` when the event isn't the caller's to update.
pub async fn apply_reply(
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
    let not_reply = || Problem::with(StatusCode::BAD_REQUEST, "this message is not a reply");
    let ics_bytes = crate::mime_read::calendar_part(&raw).ok_or_else(not_reply)?;
    let ics = String::from_utf8_lossy(&ics_bytes);
    if alo_store::ical::method_of(&ics).as_deref() != Some("REPLY") {
        return Err(not_reply());
    }
    let uid = alo_store::ical::uid_of(&ics).ok_or_else(not_reply)?;
    let (email, partstat) = alo_store::ical::reply_of(&ics).ok_or_else(not_reply)?;
    // Only updates an event the caller can edit (their organized event);
    // otherwise a clean `applied:false`, never another account's data.
    let applied = account
        .acc
        .set_attendee_status(&EventId::new(uid), &email, &partstat)
        .await
        .map_err(map_store_err)?;
    Ok(Json(
        json!({ "applied": applied, "email": email, "status": partstat }),
    ))
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

/// Validates and assembles a [`CalendarEvent`] from a request body. The
/// `calendar_id` is resolved by the caller (the request's or the personal one).
fn build_event(
    id: EventId,
    calendar_id: CalendarId,
    req: EventBody,
) -> Result<CalendarEvent, Problem> {
    let summary = req.summary.trim().to_owned();
    if summary.is_empty() {
        return Err(Problem::with(
            StatusCode::BAD_REQUEST,
            "a title is required",
        ));
    }
    let starts_at = parse_time(&req.starts_at)?;
    let ends_at = parse_time(&req.ends_at)?;
    if ends_at < starts_at {
        return Err(Problem::with(
            StatusCode::BAD_REQUEST,
            "the event ends before it starts",
        ));
    }
    // A zone must resolve in the IANA database, or expansion would silently
    // fall back to UTC — refuse it verbatim so the client can correct it
    // (Windows display names are the usual culprit).
    let timezone = match req
        .timezone
        .as_deref()
        .map(str::trim)
        .filter(|z| !z.is_empty())
    {
        Some(z) if !alo_store::tz::known(z) => {
            return Err(Problem::with(
                StatusCode::BAD_REQUEST,
                format!("unknown time zone: {z} (use an IANA name like Europe/Brussels)"),
            ));
        }
        other => other.map(str::to_owned).filter(|_| !req.all_day),
    };
    let clean = |s: Option<String>| s.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty());
    Ok(CalendarEvent {
        id,
        calendar_id,
        summary,
        description: clean(req.description),
        location: clean(req.location),
        starts_at,
        ends_at,
        all_day: req.all_day,
        recurrence: clean(req.recurrence),
        timezone,
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
        // separate action (DELETE with an `occurrence`), and RDATEs arrive
        // only over CalDAV. CalDAV PUTs preserve EXDATE/RDATE via
        // put_event/from_ics, not this create path; a whole-series update
        // carries the stored ones forward (see `update`).
        exdates: Vec::new(),
        rdates: Vec::new(),
        // Masters/one-offs have no recurrence-id; only expanded occurrences do.
        recurrence_id: None,
        // Clamp to a sane, non-negative lead time (0 = at start time).
        reminder_minutes: req.reminder_minutes.filter(|&m| (0..=40_320).contains(&m)),
        attendee_status: Vec::new(),
    })
}

/// Builds a per-occurrence override from an event body (the editable fields of a
/// single instance; placement + rule are not per-occurrence, so they're absent).
fn build_override(req: &EventBody) -> Result<OccurrenceOverride, Problem> {
    let summary = req.summary.trim().to_owned();
    if summary.is_empty() {
        return Err(Problem::with(
            StatusCode::BAD_REQUEST,
            "a title is required",
        ));
    }
    let starts_at = parse_time(&req.starts_at)?;
    let ends_at = parse_time(&req.ends_at)?;
    if ends_at < starts_at {
        return Err(Problem::with(
            StatusCode::BAD_REQUEST,
            "the event ends before it starts",
        ));
    }
    let clean = |s: &Option<String>| {
        s.as_ref()
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty())
    };
    Ok(OccurrenceOverride {
        summary,
        description: clean(&req.description),
        location: clean(&req.location),
        starts_at,
        ends_at,
        all_day: req.all_day,
    })
}

/// The calendar an event goes on: the request's `calendarId`, else the caller's
/// personal calendar (created on demand).
async fn resolve_calendar(account: &Account, req: &EventBody) -> Result<CalendarId, Problem> {
    match req.calendar_id.as_deref() {
        Some(c) if !c.is_empty() => Ok(CalendarId::new(c.to_owned())),
        _ => account
            .acc
            .ensure_personal_calendar()
            .await
            .map_err(|_| Problem::server_error()),
    }
}

#[derive(Deserialize)]
struct CalendarBody {
    name: String,
    #[serde(default)]
    color: Option<String>,
}

fn calendar_json(c: &Calendar) -> Value {
    json!({
        "id": c.id.as_str(),
        "name": c.name,
        "color": c.color,
        "kind": c.kind,
        // The viewer's access: "owner" | "editor" | "viewer".
        "role": c.role,
    })
}

/// `GET /calendar/calendars` → `{"calendars": [...]}` — the caller's calendars.
pub async fn list_calendars(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let cals = account
        .acc
        .calendars()
        .await
        .map_err(|_| Problem::server_error())?;
    let out: Vec<Value> = cals.iter().map(calendar_json).collect();
    Ok(Json(json!({ "calendars": out })))
}

/// `POST /calendar/calendars` → the created calendar.
pub async fn create_calendar(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: CalendarBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let name = req.name.trim();
    if name.is_empty() {
        return Err(Problem::with(
            StatusCode::BAD_REQUEST,
            "a calendar name is required",
        ));
    }
    let color = req
        .color
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty());
    let id = account
        .acc
        .create_calendar(name, color)
        .await
        .map_err(|_| Problem::server_error())?;
    Ok(Json(json!({
        "id": id.as_str(), "name": name, "color": color, "kind": "shared"
    })))
}

/// `PUT /calendar/calendars/:id` → `{status:"ok"}` (rename / recolour).
pub async fn rename_calendar(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: CalendarBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let name = req.name.trim();
    if name.is_empty() {
        return Err(Problem::with(
            StatusCode::BAD_REQUEST,
            "a calendar name is required",
        ));
    }
    let color = req
        .color
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty());
    account
        .acc
        .update_calendar(&CalendarId::new(id), name, color)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "status": "ok" })))
}

/// `DELETE /calendar/calendars/:id` → `{status:"ok"}`. Deletes the calendar and
/// its events; the personal calendar is protected.
pub async fn remove_calendar(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .delete_calendar(&CalendarId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "status": "ok" })))
}

// --- Sharing (Agenda slice 2) -----------------------------------------------

fn default_role() -> String {
    "viewer".to_owned()
}

/// Normalises a requested role, rejecting anything but `viewer`/`editor`.
fn valid_role(role: &str) -> Result<&str, Problem> {
    match role {
        "viewer" | "editor" => Ok(role),
        _ => Err(Problem::with(
            StatusCode::BAD_REQUEST,
            "role must be 'viewer' or 'editor'",
        )),
    }
}

/// Resolves a share subject to its stored `(kind, id)`. A `user` subject is an
/// email address (resolved within the caller's tenant); a `group` subject is a
/// group id, validated to exist in the tenant. Tenant-scoped throughout.
async fn resolve_subject(
    state: &AppState,
    account: &Account,
    kind: &str,
    subject: &str,
) -> Result<(&'static str, String), Problem> {
    let ts = state.store.for_tenant(account.tenant.clone());
    match kind {
        "user" => {
            let uid = ts
                .user_by_email(subject.trim())
                .await
                .map_err(|e| match e {
                    StoreError::NotFound => {
                        Problem::with(StatusCode::NOT_FOUND, "no user with that email address")
                    }
                    _ => Problem::server_error(),
                })?;
            Ok(("user", uid.as_str().to_owned()))
        }
        "group" => {
            let groups = ts
                .list_groups()
                .await
                .map_err(|_| Problem::server_error())?;
            let g = groups
                .into_iter()
                .find(|g| g.id == subject.trim())
                .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such group"))?;
            Ok(("group", g.id))
        }
        _ => Err(Problem::with(
            StatusCode::BAD_REQUEST,
            "kind must be 'user' or 'group'",
        )),
    }
}

#[derive(Deserialize)]
struct GrantBody {
    /// `"user"` (share with a person) or `"group"` (team access).
    kind: String,
    /// The person's email address, or the group's id.
    subject: String,
    #[serde(default = "default_role")]
    role: String,
}

/// `POST /calendar/calendars/:id/grants` → `{status:"ok"}`. Shares a calendar
/// the caller owns with a person (by email) or a group at viewer/editor. Only
/// the owner may share (enforced in the store).
pub async fn share_calendar(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: GrantBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let role = valid_role(req.role.trim())?;
    let (kind, subject_id) =
        resolve_subject(&state, &account, req.kind.trim(), &req.subject).await?;
    account
        .acc
        .grant_calendar(&CalendarId::new(id), kind, &subject_id, role)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "status": "ok" })))
}

#[derive(Deserialize)]
pub struct RevokeQuery {
    kind: String,
    subject: String,
}

/// `DELETE /calendar/calendars/:id/grants?kind=&subject=` → `{status:"ok"}`.
/// Removes a share from a calendar the caller owns.
pub async fn unshare_calendar(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(q): Query<RevokeQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let (kind, subject_id) = resolve_subject(&state, &account, q.kind.trim(), &q.subject).await?;
    account
        .acc
        .revoke_calendar(&CalendarId::new(id), kind, &subject_id)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "status": "ok" })))
}

/// `GET /calendar/calendars/:id/grants` → `{"grants":[...]}`. Who a calendar the
/// caller owns is shared with, each subject resolved to a human label.
pub async fn list_grants(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let grants = account
        .acc
        .calendar_grants(&CalendarId::new(id))
        .await
        .map_err(map_store_err)?;
    let ts = state.store.for_tenant(account.tenant.clone());
    let groups = ts
        .list_groups()
        .await
        .map_err(|_| Problem::server_error())?;
    let mut out = Vec::with_capacity(grants.len());
    for g in grants {
        // Resolve the subject to a display label; fall back to the raw id.
        let label = match g.subject_kind.as_str() {
            "user" => ts
                .email_of(&UserId::new(g.subject_id.clone()))
                .await
                .ok()
                .flatten()
                .unwrap_or_else(|| g.subject_id.clone()),
            "group" => groups
                .iter()
                .find(|x| x.id == g.subject_id)
                .map_or_else(|| g.subject_id.clone(), |x| x.name.clone()),
            _ => g.subject_id.clone(),
        };
        out.push(json!({
            "kind": g.subject_kind,
            "subject": g.subject_id,
            "label": label,
            "role": g.role,
        }));
    }
    Ok(Json(json!({ "grants": out })))
}

/// `GET /calendar/groups` → `{"groups":[{id,name}]}`. The tenant's groups, so
/// the share dialog can offer team/group sharing.
pub async fn list_shareable_groups(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let ts = state.store.for_tenant(account.tenant.clone());
    let groups = ts
        .list_groups()
        .await
        .map_err(|_| Problem::server_error())?;
    let out: Vec<Value> = groups
        .into_iter()
        .map(|g| json!({ "id": g.id, "name": g.name }))
        .collect();
    Ok(Json(json!({ "groups": out })))
}

// --- Free/busy (Agenda) -----------------------------------------------------

#[derive(Deserialize)]
struct FreeBusyBody {
    /// The people to check (email addresses in the caller's tenant).
    emails: Vec<String>,
    #[serde(rename = "from")]
    from: String,
    #[serde(rename = "to")]
    to: String,
}

/// `POST /calendar/freebusy` →
/// `{"freebusy":[{email,known,busy:[{start,end}],outsideHours:[{start,end}]}]}`.
/// Each person's **busy intervals** in `[from, to)` — merged, clamped to the
/// window, and carrying no event details (only busy/free) — plus, additively,
/// the spans **outside their working hours** (nights, weekends, non-working
/// days, in their schedule's zone). The two kinds are served side by side and
/// never merged: an existing client reading only `busy` sees exactly what it
/// always saw. Strictly within the caller's tenant: an email that isn't a
/// user there comes back `known:false`.
pub async fn free_busy(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: FreeBusyBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let from = parse_time(&req.from)?;
    let to = parse_time(&req.to)?;
    if to <= from || (to - from).whole_days() > MAX_RANGE_DAYS {
        return Err(Problem::with(
            StatusCode::BAD_REQUEST,
            "invalid or too-wide time range",
        ));
    }
    let ts = state.store.for_tenant(account.tenant.clone());
    let mut out = Vec::new();
    // Bound the fan-out; a scheduling UI checks a handful of people at a time.
    for email in req.emails.iter().take(50) {
        let email = email.trim();
        let Ok(uid) = ts.user_by_email(email).await else {
            // Not a person — a room answers the same question, in the same
            // currency, so the scheduling grid needs no second call for it.
            // A room keeps no working hours: it is free whenever nobody has it.
            if let Ok(Some(resource)) = account.acc.calendar_resource_by_email(email).await {
                let events = account
                    .acc
                    .resource_bookings_in_range(&resource.id, from, to)
                    .await
                    .map_err(|_| Problem::server_error())?;
                out.push(json!({
                    "email": email,
                    "known": true,
                    "kind": "resource",
                    "busy": spans_json(&alo_store::merged_busy_spans(&events, from, to)),
                    "outsideHours": [],
                }));
                continue;
            }
            out.push(json!({
                "email": email,
                "known": false,
                "kind": "unknown",
                "busy": [],
                "outsideHours": [],
            }));
            continue;
        };
        // Their own busy time (reusing the recurrence/override-aware expander),
        // tenant-scoped by construction — same tenant as the caller.
        let door = state.store.for_account(account.tenant.clone(), uid);
        let events = door
            .events_in_range(from, to)
            .await
            .map_err(|_| Problem::server_error())?;
        let busy = spans_json(&alo_store::merged_busy_spans(&events, from, to));
        // Their schedule's complement — the second span kind, beside busy,
        // never merged into it. The schedule's own zone wins; an unset one
        // follows the person's profile zone, else UTC.
        let hours = door
            .working_hours()
            .await
            .map_err(|_| Problem::server_error())?;
        let profile_zone = door
            .user_timezone()
            .await
            .map_err(|_| Problem::server_error())?;
        let outside = alo_store::outside_hours_spans(&hours, profile_zone.as_deref(), from, to);
        out.push(json!({
            "email": email,
            "known": true,
            "kind": "user",
            "busy": busy,
            "outsideHours": spans_json(&outside),
        }));
    }
    Ok(Json(json!({ "freebusy": out })))
}

/// The rooms an event's guest list names, in the order they appear and without
/// repeats. An attendee that is nobody's room is just a guest — the same list
/// carries both, which is what makes booking a room one act rather than two.
///
/// # Errors
/// [`Problem`] 500 when the lookup fails.
async fn booked_resources(
    account: &Account,
    event: &CalendarEvent,
) -> Result<Vec<CalendarId>, Problem> {
    let mut out: Vec<CalendarId> = Vec::new();
    for attendee in &event.attendees {
        let found = account
            .acc
            .calendar_resource_by_email(attendee)
            .await
            .map_err(|_| Problem::server_error())?;
        if let Some(resource) = found
            && !out.iter().any(|id| id.as_str() == resource.id.as_str())
        {
            out.push(resource.id);
        }
    }
    Ok(out)
}

/// Spans as the free/busy wire spells them: `[{start, end}]`, RFC 3339 UTC.
fn spans_json(spans: &[alo_store::CalendarBusySpan]) -> Vec<Value> {
    spans
        .iter()
        .map(|span| {
            json!({
                "start": span.from.format(&Rfc3339).unwrap_or_default(),
                "end": span.to.format(&Rfc3339).unwrap_or_default(),
            })
        })
        .collect()
}

fn parse_time(s: &str) -> Result<OffsetDateTime, Problem> {
    OffsetDateTime::parse(s, &Rfc3339)
        .map(|t| t.to_offset(time::UtcOffset::UTC))
        .map_err(|_| {
            Problem::with(
                StatusCode::BAD_REQUEST,
                "invalid date/time (expected RFC 3339)",
            )
        })
}

/// The record view every `/calendar/events` response serves — shared with the
/// Agenda agent's `event_lookup` executor, so the agent grounds in exactly
/// what the calendar itself shows.
pub(crate) fn event_json(e: &CalendarEvent) -> Value {
    json!({
        "id": e.id.as_str(),
        "summary": e.summary,
        "description": e.description,
        "location": e.location,
        "calendarId": e.calendar_id.as_str(),
        "startsAt": e.starts_at.format(&Rfc3339).unwrap_or_default(),
        "endsAt": e.ends_at.format(&Rfc3339).unwrap_or_default(),
        "allDay": e.all_day,
        "recurrence": e.recurrence,
        // The zone whose wall-clock a recurring event follows across DST
        // (IANA name), or null for fixed-UTC repetition.
        "timezone": e.timezone,
        "attendees": e.attendees,
        // The occurrence's original slot (its stable edit/skip handle); null on
        // a stored master or one-off. For a moved occurrence it differs from
        // startsAt, so the client edits/skips by this, not the displayed start.
        "recurrenceId": e.recurrence_id.and_then(|t| t.format(&Rfc3339).ok()),
        "reminderMinutes": e.reminder_minutes,
        // Who has responded (organizer's view): [{email, status}], as guests reply.
        "attendeeStatus": e.attendee_status.iter()
            .map(|(email, status)| json!({ "email": email, "status": status }))
            .collect::<Vec<_>>(),
    })
}

fn map_store_err(e: StoreError) -> Problem {
    match e {
        StoreError::NotFound => Problem::with(StatusCode::NOT_FOUND, "not found"),
        StoreError::Conflict(msg) => Problem::with(StatusCode::CONFLICT, &msg),
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
    // A room is an attendee, not a correspondent: it has no mailbox, and an
    // invitation addressed to one is a bounce. Its booking was already taken at
    // save time, which is the room's answer.
    let rooms: Vec<String> = account
        .acc
        .calendar_resources()
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|r| r.email.to_lowercase())
        .collect();
    let subject = crate::mime::encode_unstructured(&format!("{subject_prefix}: {}", event.summary));
    let plain_b64 = wrap76(&B64.encode(plain));
    let ics_b64 = wrap76(&B64.encode(alo_store::ical::to_imip(event, &organizer, method)));
    for attendee in event
        .attendees
        .iter()
        .filter(|a| !rooms.contains(&a.trim().to_lowercase()))
    {
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
