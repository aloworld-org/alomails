//! Personal-calendar HTTP surface (Agenda slice 1). Authenticated, tenant/user-
//! scoped through the account door — every handler resolves the caller with
//! [`authenticate`] and touches only that account's events.
//!
//! - `GET  /calendar/events?from=&to=` — events overlapping `[from, to)`.
//! - `POST /calendar/events` — create; returns `{id}`.
//! - `PUT  /calendar/events/:id` — replace.
//! - `DELETE /calendar/events/:id` — delete.
//!
//! Times cross the wire as RFC 3339 (UTC). Slice 1 is plain timed/all-day
//! events on the user's single implicit calendar; recurrence, attendees and
//! CalDAV sync are later slices. No event content is logged (Law 1).

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use alo_store::{CalendarEvent, EventId, StoreError};

use crate::error::Problem;
use crate::state::{AppState, authenticate};

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
    Ok(Json(event_json(&CalendarEvent { id, ..event })))
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
    Ok(Json(json!({ "status": "ok" })))
}

/// `DELETE /calendar/events/:id` → `{status:"ok"}`.
pub async fn delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .delete_event(&EventId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "status": "ok" })))
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
    })
}

fn map_store_err(e: StoreError) -> Problem {
    match e {
        StoreError::NotFound => Problem::with(StatusCode::NOT_FOUND, "no such event"),
        _ => Problem::server_error(),
    }
}
