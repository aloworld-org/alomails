//! The hours HTTP surface (alo Projects, ADR 0035, wave B3.04) — the running
//! timer, the manual entry, and the week a person reads back — over
//! [`alo_store::time_timer`] and [`alo_store::time_entries`].
//!
//! `/projects` is a **new top-level prefix**: the production Caddyfile needs it
//! added at the next deploy, the standing human action `/billing`, `/crm`,
//! `/audit` and `/insights` already carry, and it is in the vite dev proxy list
//! so a browser call reaches the API instead of the SPA
//! (`docs/design/projects.md` § Routes).
//!
//! It shares [`crate::billing`]'s conventions — the account door, `Problem`
//! errors, no validation duplicated from the store, timestamps as RFC 3339 and
//! days as `YYYY-MM-DD` — and adds four of its own, each of them a decision
//! taken in the design note rather than here.
//!
//! - **Every route is the caller's own.** There is no `userId` anywhere in this
//!   surface: a person's hours are personal data, and the store has no function
//!   that takes somebody else's id. The cross-user reads arrive on the admin
//!   door with the approvals inbox (B3.05).
//! - **The day is the client's, in the client's zone.** `workDate` is a plain
//!   day and never derived from the server's clock: an entry stopped at 00:30 in
//!   Berlin belongs to the previous working day, and a week boundary that moves
//!   with the server's zone is one an employee will dispute.
//! - **Starting a timer while one runs is a `409` carrying the running timer**,
//!   not an implicit stop. Stopping writes a billable fact with a duration, and
//!   a write nobody asked for is not a convenience — so the client is told what
//!   is running and decides. Its one button makes two calls, and both are
//!   audited.
//! - **Nothing here computes money.** Minutes in, minutes out; the rate on an
//!   entry is the snapshot the store took, and the fold into an invoice line is
//!   B3.06's single pure function. No float appears on this path.
//!
//! Notes are personal data — they can name a client, a colleague or a case — so
//! nothing in this module logs one.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use time::Date;

use alo_store::time_entries::{TimeTotals, week_totals};
use alo_store::time_timer::{RunningTimer, StoppedTimer};
use alo_store::{
    NewTimeEntry, ProjectId, StartTimer, TaskId, TimeEntry, TimeEntryEdit, TimeEntryId,
};

use crate::billing::{blank_to_none, iso, iso_date, map_store_err, parse_body, parse_iso_date};
use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// The longest period one read of the week grid may ask for.
///
/// A timesheet screen asks for a week and a report for a quarter; anything past
/// a year is a client asking for its whole history one call at a time, which is
/// a paging question and not this route's. Refused rather than truncated: a
/// silently shortened period is a total that is quietly wrong.
const MAX_PERIOD_DAYS: i64 = 366;

/// One entry as JSON.
///
/// `workDate` is the day the work belongs to and `startedAt` is provenance —
/// when a timer or a calendar event produced it. A reader can see both rather
/// than being told a story about one, exactly as an activity's `happenedAt` and
/// `createdAt` are both shown.
pub(crate) fn entry_json(e: &TimeEntry, task_title: Option<&str>) -> Value {
    json!({
        "id": e.id.as_str(),
        "projectId": e.project_id.as_str(),
        "taskId": e.task_id.as_ref().map(TaskId::as_str),
        "taskTitle": task_title,
        "workDate": iso_date(e.work_date),
        "startedAt": e.started_at.map(iso),
        "minutes": e.minutes,
        "billable": e.billable,
        // The snapshot the store took, never a figure computed here.
        "rateCents": e.rate_cents,
        "currency": e.currency,
        "note": e.note,
        "proposed": e.is_proposed(),
        "billed": e.is_billed(),
        "invoiceId": e.invoice_id.as_ref().map(alo_store::BillingInvoiceId::as_str),
        "createdAt": iso(e.created_at),
        "updatedAt": iso(e.updated_at),
    })
}

/// A running clock as JSON, or `null` when none is.
fn timer_json(t: Option<&RunningTimer>) -> Value {
    match t {
        None => Value::Null,
        Some(t) => json!({
            "projectId": t.project_id.as_str(),
            "taskId": t.task_id.as_ref().map(TaskId::as_str),
            "startedAt": iso(t.started_at),
            "billable": t.billable,
            "note": t.note,
        }),
    }
}

/// A period's minute totals as JSON. Minutes, never money.
pub(crate) fn totals_json(t: TimeTotals) -> Value {
    json!({
        "minutes": t.minutes,
        "billableMinutes": t.billable_minutes,
        "proposedMinutes": t.proposed_minutes,
    })
}

/// The body of `POST /projects/timer/start`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartBody {
    #[serde(default)]
    project_id: Option<String>,
    #[serde(default)]
    task_id: Option<String>,
    /// Absent means chargeable: a client project's hours are, unless the person
    /// starting the clock says otherwise.
    #[serde(default)]
    billable: Option<bool>,
    #[serde(default)]
    note: Option<String>,
}

/// The body of `POST /projects/timer/stop`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StopBody {
    /// The day the work belongs to, in the worker's own zone. Absent falls back
    /// to the day the clock started — see [`stop_timer`].
    #[serde(default)]
    work_date: Option<String>,
}

/// The body of `POST /projects/time`, a manual entry.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EntryBody {
    #[serde(default)]
    project_id: Option<String>,
    #[serde(default)]
    task_id: Option<String>,
    #[serde(default)]
    work_date: Option<String>,
    #[serde(default)]
    minutes: Option<i64>,
    #[serde(default)]
    billable: Option<bool>,
    #[serde(default)]
    rate_cents: Option<i64>,
    #[serde(default)]
    currency: Option<String>,
    #[serde(default)]
    note: Option<String>,
}

/// The body of `PATCH /projects/time/{id}` — the correctable facts, all of them
/// stated together.
///
/// A whole-record `PATCH` rather than a sparse one, because the store's
/// [`TimeEntryEdit`] is a whole record: "the entry now says this". The two
/// fields deliberately absent from it — the project and the rate — are absent
/// here too, and for the store's reasons, not the edge's.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EditBody {
    #[serde(default)]
    work_date: Option<String>,
    #[serde(default)]
    task_id: Option<String>,
    #[serde(default)]
    minutes: Option<i64>,
    #[serde(default)]
    billable: Option<bool>,
    #[serde(default)]
    note: Option<String>,
}

/// The period a week read asks for. Both ends are required and inclusive.
#[derive(Deserialize)]
pub struct WeekQuery {
    #[serde(default)]
    from: Option<String>,
    #[serde(default)]
    to: Option<String>,
    #[serde(default, rename = "projectId")]
    project_id: Option<String>,
}

/// Reads a required id from a body, naming it in the refusal.
fn required_id(name: &str, raw: Option<&str>) -> Result<String, Problem> {
    raw.map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| {
            Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("{name} is required"),
            )
        })
}

/// Reads a required day, naming it in the refusal.
fn required_day(name: &str, raw: Option<&str>) -> Result<Date, Problem> {
    optional_day(name, raw)?.ok_or_else(|| {
        Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("{name} is required: an hour always belongs to a stated day"),
        )
    })
}

/// Reads a day that may be absent. A blank string is absent; a malformed one is
/// a refusal, never a silent default — a timesheet dated by a fallback nobody
/// asked for is the one thing an employee will dispute.
fn optional_day(name: &str, raw: Option<&str>) -> Result<Option<Date>, Problem> {
    let Some(stated) = raw.map(str::trim).filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    parse_iso_date(stated).map(Some).ok_or_else(|| {
        Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("{name} must be a date of the form YYYY-MM-DD"),
        )
    })
}

/// Reads a required duration in minutes. The *range* is the store's rule and is
/// left to it; what is checked here is only that a number was stated at all.
fn required_minutes(raw: Option<i64>) -> Result<i64, Problem> {
    raw.ok_or_else(|| {
        Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "minutes is required: an entry is a duration",
        )
    })
}

/// The two ends of a week read, and the rule about the pair that only this
/// route has: a period long enough to be a paging question instead of a read.
fn period(query: &WeekQuery) -> Result<(Date, Date), Problem> {
    let from = required_day("from", query.from.as_deref())?;
    let to = required_day("to", query.to.as_deref())?;
    // The store owns "must not end before it starts"; this owns the ceiling.
    if (to - from).whole_days() >= MAX_PERIOD_DAYS {
        return Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("the period must be shorter than {MAX_PERIOD_DAYS} days"),
        ));
    }
    Ok((from, to))
}

/// `GET /projects/timer` → `{"timer": {…} | null}` — the caller's own running
/// clock.
///
/// Answers `null` rather than `404` when nothing runs: "no timer" is the
/// ordinary state of a workspace, and a widget that polls this should not have
/// to read a refusal as an answer.
///
/// # Errors
/// `401` without a valid bearer token; `500` on a store failure.
pub async fn get_timer(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let running = account.acc.running_timer().await.map_err(map_store_err)?;
    Ok(Json(json!({ "timer": timer_json(running.as_ref()) })))
}

/// `POST /projects/timer/start` `{projectId, taskId?, billable?, note?}` →
/// `{"timer": {…}}`.
///
/// # Errors
/// `401` without a valid bearer token; `422` when `projectId` is missing, the
/// note is too long, or the task is on another project; `404` when the project
/// or task is not one the caller can see; **`409` when a timer is already
/// running**, carrying it in the body under `timer` so the client can offer to
/// stop that one rather than ask the user what happened.
pub async fn start_timer(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: StartBody = parse_body(&body)?;
    let start = StartTimer {
        task_id: blank_to_none(req.task_id).map(TaskId::new),
        billable: req.billable.unwrap_or(true),
        note: req.note.unwrap_or_default(),
        ..StartTimer::on(ProjectId::new(required_id(
            "projectId",
            req.project_id.as_deref(),
        )?))
    };
    match account.acc.start_timer(&start).await {
        Ok(running) => Ok(Json(json!({ "timer": timer_json(Some(&running)) }))),
        Err(alo_store::StoreError::Conflict(message)) => {
            // The running timer is what the client needs to act, and reading it
            // back is a second call it would make anyway. A read that fails
            // leaves the refusal intact and unadorned — a 409 that turned into
            // a 500 because the extra context could not be fetched would be a
            // worse answer than the plain one.
            let running = account.acc.running_timer().await.unwrap_or(None);
            Err(
                Problem::with(StatusCode::CONFLICT, message).with_extra(json!({
                    "timer": timer_json(running.as_ref()),
                })),
            )
        }
        Err(other) => Err(map_store_err(other)),
    }
}

/// `POST /projects/timer/stop` `{workDate?}` →
/// `{"entry": {…}, "elapsedMinutes": n, "cappedAtDayLimit": bool}`.
///
/// `workDate` is the day in the worker's own zone; absent, the store falls back
/// to the day the clock **started**. `cappedAtDayLimit` says a clock ran past a
/// full day and the entry was written at the ceiling — somebody went home
/// without stopping it, and `elapsedMinutes` says how long it really ran so the
/// person can correct the entry rather than discover a 22-hour invoice line.
///
/// The answer deliberately names three things rather than one, so the audit
/// layer files this as the timer event it is and not as the creation of a
/// record it does not address.
///
/// # Errors
/// `401` without a valid bearer token; `404` when no timer is running; `422`
/// when `workDate` is malformed or the engagement's rate cannot be expressed.
pub async fn stop_timer(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    // A stop with no body at all is the ordinary call from a one-button widget.
    let req: StopBody = if body.is_empty() {
        StopBody { work_date: None }
    } else {
        parse_body(&body)?
    };
    let work_date = optional_day("workDate", req.work_date.as_deref())?;
    let StoppedTimer {
        entry,
        elapsed_minutes,
        capped,
    } = account
        .acc
        .stop_timer(work_date)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "entry": entry_json(&entry, None),
        "elapsedMinutes": elapsed_minutes,
        "cappedAtDayLimit": capped,
    })))
}

/// `GET /projects/time?from&to[&projectId]` →
/// `{"entries": [ … ], "totals": {…}}` — the caller's own hours in a period,
/// the week grid's one read.
///
/// Proposals come back alongside real entries, each saying which it is, and are
/// counted only in `totals.proposedMinutes`: the screen that offers a
/// suggestion for acceptance is the screen that shows the week, and a
/// suggestion that is invisibly already in a total is not a suggestion.
///
/// # Errors
/// `401` without a valid bearer token; `422` when an end of the period is
/// missing, malformed, ends before it starts, or spans more than a year.
pub async fn list_time(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<WeekQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let (from, to) = period(&query)?;
    let project = blank_to_none(query.project_id).map(ProjectId::new);
    let entries = account
        .acc
        .time_entries(from, to, project.as_ref())
        .await
        .map_err(map_store_err)?;
    let task_ids = entries
        .iter()
        .filter_map(|entry| entry.task_id.as_ref().map(|id| id.as_str().to_owned()))
        .collect::<Vec<_>>();
    let task_titles = account
        .acc
        .task_titles(&task_ids)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "entries": entries.iter().map(|entry| {
            let title = entry.task_id.as_ref().and_then(|id| task_titles.get(id.as_str()));
            entry_json(entry, title.map(String::as_str))
        }).collect::<Vec<_>>(),
        "totals": totals_json(week_totals(&entries)),
    })))
}

/// `POST /projects/time` `{projectId, workDate, minutes, …}` →
/// `{"entry": {…}}` — a manual entry, for work done away from the clock.
///
/// The answer carries the **stored** record rather than an echo of the request,
/// the same contract every billing write holds: the rate on it is the snapshot
/// the store resolved, which is the thing the caller could not have computed.
///
/// # Errors
/// `401` without a valid bearer token; `422` when a required field is missing
/// or a value breaks its rule; `404` when the project or task is not one the
/// caller can see.
pub async fn create_time(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: EntryBody = parse_body(&body)?;
    let project = ProjectId::new(required_id("projectId", req.project_id.as_deref())?);
    let work_date = required_day("workDate", req.work_date.as_deref())?;
    let minutes = required_minutes(req.minutes)?;
    let new = NewTimeEntry {
        task_id: blank_to_none(req.task_id).map(TaskId::new),
        billable: req.billable.unwrap_or(true),
        rate_cents: req.rate_cents,
        currency: blank_to_none(req.currency),
        note: req.note.unwrap_or_default(),
        ..NewTimeEntry::worked(project, work_date, minutes)
    };
    let entry = account.acc.log_time(&new).await.map_err(map_store_err)?;
    Ok(Json(json!({ "entry": entry_json(&entry, None) })))
}

/// `GET /projects/time/{id}` → `{"entry": {…}}` — one of the caller's own
/// entries.
///
/// A colleague's entry inside the same tenant reads exactly like another
/// tenant's and like one that never existed: `404`. Not a `403`, which would
/// confirm that somebody worked that day.
///
/// # Errors
/// `401` without a valid bearer token; `404` when the entry is not the
/// caller's own.
pub async fn get_time(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let entry = account
        .acc
        .time_entry(&TimeEntryId::new(id))
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such entry"))?;
    Ok(Json(json!({ "entry": entry_json(&entry, None) })))
}

/// `PATCH /projects/time/{id}` `{workDate, minutes, taskId?, billable?, note?}`
/// → `{"entry": {…}}` — correct one of the caller's own entries.
///
/// Neither the project nor the rate can be corrected, by the store's design:
/// moving an hour to another engagement changes who is billed for it, which is
/// a new record; and the rate is a snapshot of what was agreed when the work was
/// written down, so repricing it is not a correction of what happened.
///
/// # Errors
/// `401` without a valid bearer token; `404` when the entry is not the caller's
/// own; `409` when it is already on a document — void or credit that document
/// to release it; `422` when a value breaks its rule.
pub async fn update_time(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: EditBody = parse_body(&body)?;
    let id = TimeEntryId::new(id);
    let current = account
        .acc
        .time_entry(&id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such entry"))?;
    // An absent field means "unchanged", read from the stored record rather
    // than from a default — a PATCH that silently blanked a note the caller did
    // not mention would be a correction nobody made.
    let edit = TimeEntryEdit {
        work_date: optional_day("workDate", req.work_date.as_deref())?.unwrap_or(current.work_date),
        task_id: match req.task_id {
            None => current.task_id.clone(),
            // An explicitly empty string detaches the task; the store's own
            // `None` means the same thing, so the two agree.
            Some(stated) => blank_to_none(Some(stated)).map(TaskId::new),
        },
        minutes: req.minutes.unwrap_or(current.minutes),
        billable: req.billable.unwrap_or(current.billable),
        note: req.note.unwrap_or_else(|| current.note.clone()),
    };
    let entry = account
        .acc
        .edit_time_entry(&id, &edit)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "entry": entry_json(&entry, None) })))
}

/// `DELETE /projects/time/{id}` → `{"deleted": true}` — remove one of the
/// caller's own entries.
///
/// # Errors
/// `401` without a valid bearer token; `404` when the entry is not the caller's
/// own; `409` when it is already on a document.
pub async fn delete_time(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .delete_time_entry(&TimeEntryId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "deleted": true })))
}

/// `GET /projects/time/proposals` → `{"entries": [ … ]}` — the caller's own
/// pending suggestions (ADR 0023), newest first.
///
/// A separate read from the week grid rather than a filter on it, because it
/// answers a different question: the grid asks "what did I do that week", and
/// this asks "what is waiting for me to say yes or no", which has no period at
/// all. Proposals still appear in the grid's own period, flagged and counted
/// only in `totals.proposedMinutes` — see [`list_time`].
///
/// # Errors
/// `401` without a valid bearer token; `500` on a store failure.
pub async fn list_proposals(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let entries = account
        .acc
        .time_entry_proposals()
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "entries": entries.iter().map(|entry| entry_json(entry, None)).collect::<Vec<_>>(),
    })))
}

/// `POST /projects/time/{id}/accept` → `{"entry": {…}}` — the human "yes" that
/// turns one of the caller's own suggestions into real work.
///
/// Accepting is what puts the hour into the week's totals, so it is a write
/// like any other: **the rate is resolved now**, from the engagement's facts as
/// they stand today, and the week lock applies. The answer carries the stored
/// entry, so the client sees the price it just acquired rather than an echo.
///
/// # Errors
/// `401` without a valid bearer token; `404` when the entry is not the caller's
/// own pending proposal — an already-accepted one included, so a double accept
/// can never reprice an hour; `409` when its week is submitted or approved;
/// `422` when the engagement's rate can no longer be expressed.
pub async fn accept_time(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let entry = account
        .acc
        .accept_time_entry(&TimeEntryId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "entry": entry_json(&entry, None) })))
}

/// `POST /projects/time/{id}/reject` → `{"rejected": true}` — the human "no",
/// which discards the suggestion.
///
/// A suggestion nobody accepted is not a record of anything, so it is deleted
/// rather than kept as a rejected row. The week lock deliberately does not
/// apply (the store's own reasoning): a proposal is in no total, so discarding
/// one changes nothing an approver ever saw, and refusing it would strand a
/// draft the lock arrived after.
///
/// # Errors
/// `401` without a valid bearer token; `404` when the entry is not the caller's
/// own pending proposal.
pub async fn reject_time(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .reject_time_entry(&TimeEntryId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "rejected": true })))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use time::{Month, OffsetDateTime};

    fn query(from: &str, to: &str) -> WeekQuery {
        WeekQuery {
            from: Some(from.to_owned()),
            to: Some(to.to_owned()),
            project_id: None,
        }
    }

    #[test]
    fn a_period_is_stated_at_both_ends_or_refused() {
        let (from, to) = period(&query("2026-08-03", "2026-08-09")).expect("a plain week");
        assert_eq!(iso_date(from), "2026-08-03");
        assert_eq!(iso_date(to), "2026-08-09");

        for (bad, wrong_end) in [
            (query("", "2026-08-09"), "from"),
            (query("2026-08-03", " "), "to"),
        ] {
            let problem = period(&bad).expect_err("refused");
            assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY);
            assert!(
                problem
                    .detail
                    .as_deref()
                    .unwrap_or_default()
                    .contains(wrong_end),
                "the refusal must name the end that is wrong"
            );
        }
        let problem = period(&query("03/08/2026", "2026-08-09")).expect_err("refused");
        assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(
            problem
                .detail
                .as_deref()
                .unwrap_or_default()
                .contains("YYYY-MM-DD")
        );
    }

    #[test]
    fn a_period_longer_than_a_year_is_a_paging_question_and_is_refused() {
        // A full year is fine; the day past the ceiling is not, and it is
        // refused rather than truncated.
        assert!(period(&query("2026-01-01", "2026-12-31")).is_ok());
        let problem = period(&query("2026-01-01", "2027-01-02")).expect_err("refused");
        assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(
            problem
                .detail
                .as_deref()
                .unwrap_or_default()
                .contains("366 days")
        );
    }

    #[test]
    fn a_day_may_be_absent_but_never_malformed() {
        assert_eq!(optional_day("workDate", None).unwrap(), None);
        assert_eq!(optional_day("workDate", Some("  ")).unwrap(), None);
        assert_eq!(
            optional_day("workDate", Some(" 2026-08-03 "))
                .unwrap()
                .map(iso_date),
            Some("2026-08-03".to_owned())
        );
        for bad in ["yesterday", "2026-13-01", "2026-08-03T09:00:00Z"] {
            let problem =
                optional_day("workDate", Some(bad)).expect_err("accepted a malformed day");
            assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}");
        }
    }

    #[test]
    fn a_required_field_names_itself_in_the_refusal() {
        let problem = required_id("projectId", Some("  ")).expect_err("refused");
        assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(
            problem
                .detail
                .as_deref()
                .unwrap_or_default()
                .contains("projectId")
        );
        assert_eq!(required_id("projectId", Some(" p1 ")).unwrap(), "p1");

        let problem = required_minutes(None).expect_err("refused");
        assert!(
            problem
                .detail
                .as_deref()
                .unwrap_or_default()
                .contains("minutes")
        );
        // The range itself is the store's rule, not this layer's: a zero
        // reaches the store, which names the bound it broke.
        assert_eq!(required_minutes(Some(0)).unwrap(), 0);
    }

    #[test]
    fn a_timer_that_is_not_running_is_null_not_an_empty_object() {
        assert_eq!(timer_json(None), Value::Null);
    }

    #[test]
    fn a_time_entry_names_its_linked_task() {
        let now = OffsetDateTime::UNIX_EPOCH;
        let entry = TimeEntry {
            id: TimeEntryId::new("time-1"),
            user_id: alo_store::UserId::new("user-1"),
            project_id: ProjectId::new("project-1"),
            task_id: Some(TaskId::new("task-1")),
            work_date: Date::from_calendar_date(2026, Month::August, 20).expect("valid work date"),
            started_at: Some(now),
            minutes: 45,
            billable: true,
            rate_cents: Some(12_500),
            currency: Some("EUR".to_owned()),
            note: "Reviewed the launch checklist".to_owned(),
            state: "active".to_owned(),
            source_kind: None,
            source_id: None,
            invoice_id: None,
            billed_at: None,
            created_at: now,
            updated_at: now,
        };

        let value = entry_json(&entry, Some("Launch the website"));
        assert_eq!(value["taskId"], "task-1");
        assert_eq!(value["taskTitle"], "Launch the website");
    }
}
