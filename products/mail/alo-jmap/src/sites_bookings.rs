//! The owner's side of what a site can be *booked* for (ADR 0036, S2.13a):
//! the bookable services of a site and the Agenda calendars they may be
//! attached to.
//!
//! A separate module from [`crate::sites`] for a separate reason to change:
//! these routes describe an appointment nobody has made yet — the service, the
//! week it is offered in, and the questions asked when it is taken. Taking one
//! is the public flow's job and lives elsewhere.
//!
//! Two rules shape the wire contract:
//!
//! * **The calendar is named, never described.** A body carries a
//!   `calendarId` and nothing else about it; what that calendar is called and
//!   whether an appointment can be written into it is resolved server-side
//!   through the Sites-owned Agenda seam. A calendar this account cannot see
//!   is a `404`, exactly like a site of another tenant — the two are
//!   indistinguishable on purpose.
//! * **A write replaces the whole service.** Hours, questions and source are
//!   one decision; there is no field-at-a-time verb that could leave a public
//!   page offering a week its owner never agreed to.
//!
//! Errors follow the `/sites/{id}` contract: `401` unauthenticated, `404` for
//! anything that does not resolve in the caller's tenant, `422` for a rule the
//! store names, `400` for a body that is not the shape.

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::{Json, body::Bytes};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::{
    CalendarId, SiteAvailabilitySource, SiteBooking, SiteBookingField, SiteBookingFieldKind,
    SiteBookingId, SiteBookingInput, SiteBookingWindow, SiteId,
};

use crate::billing::iso;
use crate::error::Problem;
use crate::sites::{map_store_err, require_site};
use crate::state::{Account, AppState, authenticate};

/// One booking service, with its availability source resolved.
///
/// `calendar` is `null` when the bound calendar can no longer be reached —
/// deleted in Agenda, or a share withdrawn. The editor needs that as a fact it
/// can show ("this service is no longer connected to a calendar"), not as an
/// empty week the owner discovers from a visitor's complaint.
fn booking_json(booking: &SiteBooking, source: Option<&SiteAvailabilitySource>) -> Value {
    json!({
        "id": booking.id.as_str(),
        "name": booking.name,
        "description": booking.description,
        "calendarId": booking.calendar.as_str(),
        "calendar": source.map(source_json),
        "timeZone": booking.time_zone,
        "durationMinutes": booking.duration_minutes,
        "bufferMinutes": booking.buffer_minutes,
        "noticeMinutes": booking.notice_minutes,
        "horizonDays": booking.horizon_days,
        "location": booking.location,
        "hours": booking.hours.iter().map(window_json).collect::<Vec<_>>(),
        "fields": booking.fields.iter().map(field_json).collect::<Vec<_>>(),
        "active": booking.active,
        "createdAt": iso(booking.created_at),
        "updatedAt": iso(booking.updated_at),
    })
}

fn source_json(source: &SiteAvailabilitySource) -> Value {
    json!({
        "id": source.calendar.as_str(),
        "name": source.name,
        "writable": source.writable,
    })
}

/// ISO weekday (1 = Monday) and minutes from midnight, exactly as stored.
fn window_json(window: &SiteBookingWindow) -> Value {
    json!({
        "weekday": window.weekday,
        "startMinute": window.start_minute,
        "endMinute": window.end_minute,
    })
}

fn field_json(field: &SiteBookingField) -> Value {
    json!({
        "key": field.key,
        "label": field.label,
        "kind": field.kind.as_str(),
        "required": field.required,
        "options": field.options,
    })
}

/// `GET /sites/:id/booking-sources` -> the calendars this account could attach
/// a booking service to, read-only shares included and marked as such: a
/// picker that hid them would be a puzzle, one that explains them is an answer.
pub async fn list_sources(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let site = SiteId::new(id);
    require_site(&account, &site).await?;
    let sources = account
        .acc
        .site_availability_sources()
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "sources": sources.iter().map(source_json).collect::<Vec<_>>()
    })))
}

/// `GET /sites/:id/bookings` -> every bookable service of the site, in
/// creation order, each with its source resolved.
pub async fn list_bookings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let site = SiteId::new(id);
    require_site(&account, &site).await?;
    let bookings = account
        .acc
        .site_bookings(&site)
        .await
        .map_err(map_store_err)?;
    // One read of the calendar list for the whole page, matched in memory: a
    // site with a dozen services must not cost a dozen calendar queries.
    let sources = account
        .acc
        .site_availability_sources()
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "bookings": bookings
            .iter()
            .map(|booking| booking_json(booking, source_of(&sources, booking)))
            .collect::<Vec<_>>()
    })))
}

/// The body of a booking create or replace.
///
/// Only the five decisions that have no honest default are required: what it
/// is called, which calendar it lives in, which clock its hours are written
/// on, how long it takes, and when it is offered. The rest carries the
/// defaults a first service can be created with and corrected later.
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct BookingBody {
    name: String,
    #[serde(default)]
    description: Option<String>,
    calendar_id: String,
    time_zone: String,
    duration_minutes: i32,
    #[serde(default)]
    buffer_minutes: i32,
    #[serde(default)]
    notice_minutes: i32,
    #[serde(default = "default_horizon_days")]
    horizon_days: i32,
    #[serde(default)]
    location: Option<String>,
    hours: Vec<WindowBody>,
    #[serde(default)]
    fields: Vec<FieldBody>,
    #[serde(default = "default_active")]
    active: bool,
}

/// Two months ahead: far enough that a visitor can plan, near enough that a
/// week nobody has thought about yet is not already promised.
fn default_horizon_days() -> i32 {
    60
}

/// A service is created to be booked; switching it off is a later decision.
fn default_active() -> bool {
    true
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct WindowBody {
    weekday: i32,
    start_minute: i32,
    end_minute: i32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct FieldBody {
    key: String,
    label: String,
    kind: String,
    #[serde(default)]
    required: bool,
    #[serde(default)]
    options: Vec<String>,
}

/// The owned, already-parsed values of one write. It exists so create and
/// replace read a body exactly once and in exactly the same way: the borrowed
/// [`SiteBookingInput`] the store takes cannot own its strings.
struct BookingWrite {
    calendar: CalendarId,
    hours: Vec<SiteBookingWindow>,
    fields: Vec<SiteBookingField>,
}

impl BookingWrite {
    fn read(req: &BookingBody) -> Result<Self, Problem> {
        Ok(Self {
            calendar: CalendarId::new(req.calendar_id.trim().to_owned()),
            hours: req
                .hours
                .iter()
                .map(|window| SiteBookingWindow {
                    weekday: window.weekday,
                    start_minute: window.start_minute,
                    end_minute: window.end_minute,
                })
                .collect(),
            fields: req
                .fields
                .iter()
                .map(|field| {
                    Ok(SiteBookingField {
                        key: field.key.clone(),
                        label: field.label.clone(),
                        kind: SiteBookingFieldKind::parse(field.kind.trim())
                            .map_err(map_store_err)?,
                        required: field.required,
                        options: field.options.clone(),
                    })
                })
                .collect::<Result<Vec<_>, Problem>>()?,
        })
    }

    fn input<'a>(&'a self, req: &'a BookingBody) -> SiteBookingInput<'a> {
        SiteBookingInput {
            name: &req.name,
            description: req.description.as_deref(),
            calendar: &self.calendar,
            time_zone: &req.time_zone,
            duration_minutes: req.duration_minutes,
            buffer_minutes: req.buffer_minutes,
            notice_minutes: req.notice_minutes,
            horizon_days: req.horizon_days,
            location: req.location.as_deref(),
            hours: &self.hours,
            fields: &self.fields,
            active: req.active,
        }
    }
}

/// `POST /sites/:id/bookings` -> the stored service.
pub async fn create_booking(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: BookingBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let site = SiteId::new(id);
    let write = BookingWrite::read(&req)?;
    let created = account
        .acc
        .create_site_booking(&site, &write.input(&req))
        .await
        .map_err(map_store_err)?;
    answer(&account, &site, &created).await
}

/// `GET /sites/:id/bookings/:booking` -> one service, source resolved.
pub async fn get_booking(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, booking)): Path<(String, String)>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    answer(&account, &SiteId::new(id), &SiteBookingId::new(booking)).await
}

/// `PUT /sites/:id/bookings/:booking` -> the service as it now stands.
pub async fn update_booking(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, booking)): Path<(String, String)>,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: BookingBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let site = SiteId::new(id);
    let booking = SiteBookingId::new(booking);
    let write = BookingWrite::read(&req)?;
    account
        .acc
        .update_site_booking(&site, &booking, &write.input(&req))
        .await
        .map_err(map_store_err)?;
    answer(&account, &site, &booking).await
}

/// `DELETE /sites/:id/bookings/:booking` -> `204`. The calendar it pointed at
/// is Agenda's and is left exactly as it was; appointments already in it are
/// appointments, and nothing here cancels one.
pub async fn delete_booking(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, booking)): Path<(String, String)>,
) -> Result<StatusCode, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .delete_site_booking(&SiteId::new(id), &SiteBookingId::new(booking))
        .await
        .map_err(map_store_err)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Reads one service back and answers with it — the single shape every verb
/// returns, so the editor never has to reconcile two.
async fn answer(
    account: &Account,
    site: &SiteId,
    booking: &SiteBookingId,
) -> Result<Json<Value>, Problem> {
    let stored = account
        .acc
        .site_booking(site, booking)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such booking service"))?;
    let source = account
        .acc
        .site_availability_source(&stored.calendar)
        .await
        .map_err(map_store_err)?;
    Ok(Json(booking_json(&stored, source.as_ref())))
}

fn source_of<'a>(
    sources: &'a [SiteAvailabilitySource],
    booking: &SiteBooking,
) -> Option<&'a SiteAvailabilitySource> {
    sources
        .iter()
        .find(|source| source.calendar.as_str() == booking.calendar.as_str())
}
