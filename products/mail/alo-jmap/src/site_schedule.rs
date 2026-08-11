//! Publishing a website at a chosen moment (ADR 0036, S2.05b): the
//! authenticated `/sites/{id}/schedule` routes — say when the site should go
//! live, move that moment, call it off, and read what happened.
//!
//! Separate from [`crate::sites`] (the editing surface) and from
//! [`crate::site_versions`] (the history of what the internet served) because
//! it has its own reason to change: it is the *intention* surface. The model
//! behind it is [`alo_store::site_publish_schedule`]; the sweep that actually
//! runs a due intention is [`crate::site_publish_worker`].
//!
//! Two contracts this module holds:
//!
//! - **UTC on the wire, the reader's own clock on the screen.** `publishAt` is
//!   an RFC 3339 instant both ways. A caller may send any offset — `09:00` in
//!   Amsterdam and the same instant in UTC are the same moment — and every
//!   answer reports UTC, so a surface that renders it with the browser's zone
//!   shows the person the time they chose.
//! - **A schedule is not a publish.** Cancelling, moving, or failing an
//!   intention never touches what the internet is serving; only the worker's
//!   successful run appends a version.
//!
//! Error contract, identical to the rest of the sites surface: `401`
//! unauthenticated, `404` for anything that does not resolve in the caller's
//! tenant, `422` with the store's rule-naming sentence for a refusal.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use alo_store::{SiteId, SitePublishId, SitePublishSchedule, SitePublishScheduleId};

use crate::error::Problem;
use crate::sites::{map_store_err, require_site};
use crate::state::{AppState, authenticate};

/// Past schedules returned when the caller does not ask for a number. A site
/// is scheduled a handful of times, not hundreds; this is a screen's worth.
const DEFAULT_SCHEDULE_HISTORY_LIMIT: i64 = 20;

/// What a caller must send when `publishAt` is missing or unreadable. Names
/// the shape rather than echoing what arrived.
const BAD_PUBLISH_AT: &str =
    "publishAt must be a date and time with a time zone, for example 2026-09-01T09:00:00+02:00";

fn iso(t: OffsetDateTime) -> String {
    t.format(&Rfc3339).unwrap_or_default()
}

/// One scheduled publish as JSON. `publishAt` is always UTC; the surface that
/// shows it is what turns it back into the reader's own time.
fn schedule_json(schedule: &SitePublishSchedule) -> Value {
    json!({
        "id": schedule.id.as_str(),
        "siteId": schedule.site.as_str(),
        "publishAt": iso(schedule.publish_at),
        "status": schedule.status.as_str(),
        "requestedBy": schedule.requested_by.as_str(),
        "createdAt": iso(schedule.created_at),
        "updatedAt": iso(schedule.updated_at),
        "finishedAt": schedule.finished_at.map(iso),
        "attempts": schedule.attempts,
        "publishId": schedule.publish.as_ref().map(SitePublishId::as_str),
        "lastError": schedule.last_error,
    })
}

/// `?limit=` on the history read — a string the handler parses itself, so an
/// unreadable value falls back to the default instead of letting axum answer
/// a plain-text `400` where every other answer here is an RFC 9457 `Problem`.
#[derive(Deserialize)]
pub struct ScheduleHistoryQuery {
    limit: Option<String>,
}

/// `GET /sites/:id/schedule` → `{"schedule": …|null, "history": […]}` — the
/// intention that is still going to happen (`null` when there is none) plus
/// what previous ones did, newest moment first.
///
/// The pending schedule is repeated inside `history`: one read answers both
/// "when does this go live?" and "what happened last time?".
pub async fn get_schedule(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<ScheduleHistoryQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let site = SiteId::new(id);
    require_site(&account, &site).await?;
    let pending = account
        .acc
        .site_publish_schedule(&site)
        .await
        .map_err(map_store_err)?;
    let history = account
        .acc
        .site_publish_schedules(
            &site,
            query
                .limit
                .and_then(|limit| limit.trim().parse::<i64>().ok())
                .unwrap_or(DEFAULT_SCHEDULE_HISTORY_LIMIT),
        )
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "schedule": pending.as_ref().map_or(Value::Null, schedule_json),
        "history": history.iter().map(schedule_json).collect::<Vec<_>>(),
    })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScheduleBody {
    #[serde(rename = "publishAt")]
    publish_at: String,
}

/// `POST /sites/:id/schedule` `{"publishAt"}` → the scheduled publish.
///
/// The same call schedules and reschedules: a site with a pending intention
/// has that intention *moved*, keeping its id, so a surface watching one
/// schedule keeps watching it. Nothing about the site's content is checked
/// here — the author has until the chosen moment to finish it, and a publish
/// that then refuses records why on the row.
pub async fn set_schedule(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let site = SiteId::new(id);
    require_site(&account, &site).await?;
    let req: ScheduleBody = serde_json::from_slice(&body)
        .map_err(|_| Problem::with(StatusCode::UNPROCESSABLE_ENTITY, BAD_PUBLISH_AT))?;
    let at = OffsetDateTime::parse(req.publish_at.trim(), &Rfc3339)
        .map_err(|_| Problem::with(StatusCode::UNPROCESSABLE_ENTITY, BAD_PUBLISH_AT))?;
    let schedule = account
        .acc
        .schedule_site_publish(&site, at)
        .await
        .map_err(map_store_err)?;
    Ok(Json(schedule_json(&schedule)))
}

/// `DELETE /sites/:id/schedule/:schedule` → the cancelled schedule.
///
/// The row survives as `cancelled`: the tenant asked for something and then
/// changed their mind, and a surface that can say "you cancelled this" is
/// kinder than one where the entry silently disappears.
pub async fn cancel_schedule(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, schedule)): Path<(String, String)>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let site = SiteId::new(id);
    require_site(&account, &site).await?;
    let cancelled = account
        .acc
        .cancel_site_publish_schedule(&site, &SitePublishScheduleId::new(schedule))
        .await
        .map_err(|error| match error {
            alo_store::StoreError::NotFound => Problem::with(
                StatusCode::NOT_FOUND,
                "no such scheduled publish for this website",
            ),
            other => map_store_err(other),
        })?;
    Ok(Json(schedule_json(&cancelled)))
}
