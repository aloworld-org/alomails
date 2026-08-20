//! The week HTTP surface (alo Projects, ADR 0035, wave B3.05) — the submit a
//! person makes, the decision an approver records, and the lock both put on the
//! hours in between — over [`alo_store::time_weeks`].
//!
//! [`crate::projects_time`] is the hours themselves and is deliberately a
//! different file: it holds one person's own record of work and has no `userId`
//! anywhere in it, while half of this module is a **cross-user** surface that
//! exists only for approvers. Mixing the two would put the module's one
//! privileged read in the file whose whole premise is that there isn't one.
//!
//! Four decisions, each taken in `docs/design/projects.md` rather than here:
//!
//! - **The personal door names a week by its Monday**
//!   (`/projects/weeks/2026-08-03/submit`), because a week nobody has submitted
//!   has no row and therefore no id. A day that is not a Monday is a `422`
//!   naming the Monday that was probably meant — never rounded to the containing
//!   week, because silently submitting a different week than the one asked for
//!   is the worst bug this module could ship.
//! - **The admin door names a week by its id** (`/projects/approvals/{id}/…`).
//!   An approver is always looking at a row that exists, and spelling a
//!   colleague's week as (person, date) in a URL would put an employee's
//!   identity into every access log between here and the browser.
//! - **`require_admin` gates the inbox and every decision**, the same gate
//!   `/admin/*` uses. An admin may decide their own week — a one-person tenant
//!   has nobody else — and the audit entry records that they did.
//! - **Refusals are conflicts, not silence.** Submitting a week that is already
//!   in somebody's inbox, deciding one nobody submitted, or reopening one whose
//!   hours are already on an invoice each answer `409` saying what the week
//!   actually is. A week that is not this tenant's is `404` and never a
//!   conflict, so no refusal is an existence oracle.
//!
//! A decision note can name a person or a case, so nothing here logs one.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use time::Date;

use alo_store::time_weeks::{PendingWeek, TimesheetWeek, WeekDecision};
use alo_store::{TimeWeekId, UserId};

use crate::billing::{iso, iso_date, map_store_err, parse_body, parse_iso_date};
use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// The longest span one read of the caller's own weeks may ask for.
///
/// Five years of Mondays is every week an employment relationship is likely to
/// hold and still a bounded answer; past it the caller wants paging, which this
/// route does not offer. Refused rather than truncated, exactly as the week
/// grid's own period is.
const MAX_WEEKS_SPAN_DAYS: i64 = 5 * 366;

/// One week as JSON.
///
/// `decidedBy` is a user id and not an address: the person reading their own
/// week already knows their approver, and turning ids into addresses on this
/// route would hand every employee a directory lookup they were not given. The
/// approvals inbox, which genuinely needs to show people, resolves the address
/// on the admin door and nowhere else.
fn week_json(week: &TimesheetWeek) -> Value {
    json!({
        "id": week.id.as_str(),
        "weekStart": iso_date(week.week_start),
        "weekEnd": iso_date(week.week_end()),
        "status": week.status.as_str(),
        "locked": week.status.is_locked(),
        "submittedAt": week.submitted_at.map(iso),
        "decidedBy": week.decided_by.as_ref().map(UserId::as_str),
        "decidedAt": week.decided_at.map(iso),
        "decisionNote": week.decision_note,
        "createdAt": iso(week.created_at),
        "updatedAt": iso(week.updated_at),
    })
}

/// One waiting week as the inbox shows it: the week, the person, and what it
/// adds up to. Minutes, never money — pricing a timesheet is B3.06's job.
fn pending_json(pending: &PendingWeek) -> Value {
    let mut value = week_json(&pending.week);
    if let Some(object) = value.as_object_mut() {
        object.insert("userId".to_owned(), json!(pending.week.user_id.as_str()));
        object.insert("userEmail".to_owned(), json!(pending.user_email));
        object.insert("minutes".to_owned(), json!(pending.minutes));
        object.insert(
            "billableMinutes".to_owned(),
            json!(pending.billable_minutes),
        );
        object.insert(
            "projects".to_owned(),
            json!(
                pending
                    .projects
                    .iter()
                    .map(|project| json!({
                        "projectId": project.project_id,
                        "projectName": project.project_name,
                        "minutes": project.minutes,
                        "billableMinutes": project.billable_minutes,
                    }))
                    .collect::<Vec<_>>()
            ),
        );
    }
    value
}

/// The period a read of the caller's own weeks asks for. Both ends are required
/// and inclusive, and may be any day — each is resolved to the week it falls in.
#[derive(Deserialize)]
pub struct WeeksQuery {
    #[serde(default)]
    from: Option<String>,
    #[serde(default)]
    to: Option<String>,
}

/// The body of a decision. A rejection without a reason is legal but unkind, so
/// the field exists on both verbs and neither requires it.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DecisionBody {
    #[serde(default)]
    note: Option<String>,
}

/// Reads a required day, naming it in the refusal.
fn required_day(name: &str, raw: Option<&str>) -> Result<Date, Problem> {
    let stated = raw
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| {
            Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("{name} is required"),
            )
        })?;
    parse_iso_date(stated).ok_or_else(|| {
        Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("{name} must be a date of the form YYYY-MM-DD"),
        )
    })
}

/// The two ends of a weeks read, each already resolved to its Monday so the
/// store's `week_start >= from` compares like with like.
fn week_period(query: &WeeksQuery) -> Result<(Date, Date), Problem> {
    let from = alo_store::week_start(required_day("from", query.from.as_deref())?);
    let to = alo_store::week_start(required_day("to", query.to.as_deref())?);
    if to < from {
        return Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "the end of the period must not be before its start",
        ));
    }
    if (to - from).whole_days() >= MAX_WEEKS_SPAN_DAYS {
        return Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("the period must be shorter than {MAX_WEEKS_SPAN_DAYS} days"),
        ));
    }
    Ok((from, to))
}

/// The Monday a `/projects/weeks/{monday}/…` route addresses.
///
/// The *weekday* rule is the store's and is left to it, so the refusal a caller
/// reads is the one sentence the store authored; what is checked here is only
/// that the segment is a date at all.
fn addressed_monday(raw: &str) -> Result<Date, Problem> {
    parse_iso_date(raw.trim()).ok_or_else(|| {
        Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "a week is addressed by its Monday, of the form YYYY-MM-DD",
        )
    })
}

/// The note a decision carries. Bounded by the store; blanked to empty here so
/// an absent field and an explicit `null` mean the same thing.
fn decision_note(body: &axum::body::Bytes) -> Result<String, Problem> {
    if body.is_empty() {
        return Ok(String::new());
    }
    let req: DecisionBody = parse_body(body)?;
    Ok(req.note.unwrap_or_default())
}

/// `GET /projects/weeks?from&to` → `{"weeks": [ … ]}` — the caller's **own**
/// weeks that have a status, oldest first.
///
/// A week the answer does not mention is open: it has no row, which is what open
/// means. Synthesising one per Monday in the period would invent records that do
/// not exist and ids for them too.
///
/// # Errors
/// `401` without a valid bearer token; `422` when an end of the period is
/// missing, malformed, ends before it starts, or spans more than five years.
pub async fn list_weeks(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<WeeksQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let (from, to) = week_period(&query)?;
    let weeks = account
        .acc
        .timesheet_weeks(from, to)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "weeks": weeks.iter().map(week_json).collect::<Vec<_>>(),
    })))
}

/// `POST /projects/weeks/{monday}/submit` → `{"week": {…}}` — hand the caller's
/// own week in for approval, **locking its hours**.
///
/// An empty week may be submitted: "I worked nothing this week" is a real
/// statement, and refusing it would leave a person no way to make it.
///
/// # Errors
/// `401` without a valid bearer token; `422` when the segment is not a date, or
/// not a Monday; `409` when the week is already submitted or already approved,
/// naming what it is.
pub async fn submit_week(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(monday): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let week = account
        .acc
        .submit_week(addressed_monday(&monday)?)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "week": week_json(&week) })))
}

/// `POST /projects/weeks/{monday}/withdraw` → `{"week": {…}}` — take a
/// submitted week back, unlocking its hours.
///
/// Only a week nobody has decided yet. An approved week is not the person's to
/// reopen — its hours may already be on a document — and an open or rejected one
/// is unlocked already.
///
/// # Errors
/// `401` without a valid bearer token; `422` when the segment is not a Monday;
/// `409` when the week is not currently submitted, naming what it is.
pub async fn withdraw_week(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(monday): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let week = account
        .acc
        .withdraw_week(addressed_monday(&monday)?)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "week": week_json(&week) })))
}

/// `GET /projects/approvals` → `{"weeks": [ … ]}` — **admin only**: every week
/// of this tenant awaiting a decision, oldest submission first.
///
/// The narrowest cross-user read the module has: submitted weeks, their owners'
/// addresses, and their minute totals. No notes and no entries — an approver
/// needs to know how much somebody handed in, not what they wrote about it.
///
/// # Errors
/// `401` without a valid bearer token; `403` when the caller is not a tenant
/// admin.
pub async fn list_approvals(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_admin()?;
    let weeks = state
        .store
        .for_tenant(account.tenant.clone())
        .pending_weeks()
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "weeks": weeks.iter().map(pending_json).collect::<Vec<_>>(),
    })))
}

/// `POST /projects/approvals/{id}/approve` `{note?}` → `{"week": {…}}` —
/// **admin only**: the week is approved and stays locked, and its billable hours
/// become eligible for an invoice draft (B3.06).
///
/// # Errors
/// `401`/`403` as above; `404` when the week is not this tenant's; `409` when it
/// is not awaiting a decision; `422` when the note is too long.
pub async fn approve_week(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    decide(state, headers, id, WeekDecision::Approve, &body).await
}

/// `POST /projects/approvals/{id}/reject` `{note?}` → `{"week": {…}}` — **admin
/// only**: the week is rejected and **unlocks**, because the point of a
/// rejection is that the person fixes it and submits again.
///
/// # Errors
/// `401`/`403` as above; `404` when the week is not this tenant's; `409` when it
/// is not awaiting a decision; `422` when the note is too long.
pub async fn reject_week(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    decide(state, headers, id, WeekDecision::Reject, &body).await
}

/// The one implementation behind approve and reject. They are two routes because
/// they are two acts an approver takes and two entries in the audit log, and one
/// function because the only thing that differs between them is the state the
/// week lands in.
async fn decide(
    state: AppState,
    headers: HeaderMap,
    id: String,
    decision: WeekDecision,
    body: &axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_admin()?;
    let note = decision_note(body)?;
    let week = state
        .store
        .for_tenant(account.tenant.clone())
        .decide_week(&TimeWeekId::new(id), decision, &account.user, &note)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "week": week_json(&week) })))
}

/// `POST /projects/approvals/{id}/reopen` → `{"week": {…}}` — **admin only**:
/// take a decision back. The week returns to open and its hours unlock.
///
/// Reopening an approved week whose hours are already on a document is a `409`
/// saying how many and which invoice: the hours have left this module and are on
/// paper a customer has read, and the way back is to void or credit that
/// document, not to edit history underneath it.
///
/// # Errors
/// `401`/`403` as above; `404` when the week is not this tenant's; `409` when it
/// has no decision to take back, or carries billed hours.
pub async fn reopen_week(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_admin()?;
    let week = state
        .store
        .for_tenant(account.tenant.clone())
        .reopen_week(&TimeWeekId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "week": week_json(&week) })))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn query(from: &str, to: &str) -> WeeksQuery {
        WeeksQuery {
            from: Some(from.to_owned()),
            to: Some(to.to_owned()),
        }
    }

    fn detail(problem: &Problem) -> String {
        problem.detail.clone().unwrap_or_default()
    }

    #[test]
    fn either_end_of_the_period_may_be_any_day_and_becomes_its_monday() {
        // A client that sends "the last four weeks" as two arbitrary days gets
        // the four weeks, not an empty answer because neither day was a Monday.
        let (from, to) = week_period(&query("2026-07-15", "2026-08-09")).expect("a real period");
        assert_eq!(iso_date(from), "2026-07-13");
        assert_eq!(iso_date(to), "2026-08-03");
    }

    #[test]
    fn a_period_is_stated_at_both_ends_and_never_runs_backwards() {
        for (bad, wrong) in [
            (query("", "2026-08-09"), "from"),
            (query("2026-08-03", "  "), "to"),
            (query("2026-08-03", "yesterday"), "to"),
        ] {
            let problem = week_period(&bad).expect_err("refused");
            assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY);
            assert!(detail(&problem).contains(wrong), "{}", detail(&problem));
        }
        let problem = week_period(&query("2026-08-10", "2026-08-03")).expect_err("refused");
        assert!(detail(&problem).contains("must not be before its start"));
    }

    #[test]
    fn a_period_longer_than_an_employment_is_a_paging_question() {
        assert!(week_period(&query("2022-01-03", "2026-08-03")).is_ok());
        let problem = week_period(&query("2000-01-03", "2026-08-03")).expect_err("refused");
        assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(detail(&problem).contains("shorter than"));
    }

    #[test]
    fn the_addressed_week_must_be_a_date_and_the_weekday_rule_is_the_stores() {
        assert_eq!(
            addressed_monday(" 2026-08-03 ").map(iso_date).ok(),
            Some("2026-08-03".to_owned())
        );
        // Not a Monday, but a real date: it reaches the store, which authors the
        // one refusal naming the Monday that was probably meant.
        assert!(addressed_monday("2026-08-05").is_ok());
        for bad in ["", "this-week", "2026-13-01", "2026-08-03T00:00:00Z"] {
            let problem = addressed_monday(bad).expect_err("accepted a non-date");
            assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}");
        }
    }

    #[test]
    fn a_decision_may_arrive_with_no_body_no_note_or_a_note() {
        assert_eq!(decision_note(&axum::body::Bytes::new()).unwrap(), "");
        let empty = axum::body::Bytes::from_static(b"{}");
        assert_eq!(decision_note(&empty).unwrap(), "");
        let stated = axum::body::Bytes::from_static(br#"{"note":"Thursday looks doubled"}"#);
        assert_eq!(decision_note(&stated).unwrap(), "Thursday looks doubled");
        let malformed = axum::body::Bytes::from_static(b"{");
        assert_eq!(
            decision_note(&malformed).expect_err("refused").status,
            StatusCode::BAD_REQUEST
        );
    }
}
