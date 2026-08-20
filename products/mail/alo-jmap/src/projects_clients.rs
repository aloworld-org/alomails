//! The engagement HTTP surface (alo Projects, ADR 0035, wave B3.07) — the list
//! a business opens Projects to look at, one engagement, and the client facts
//! that make a board client work, over [`alo_store::project_clients`] and
//! [`alo_store::project_hours`].
//!
//! This is the module's front door, and it is the one the rest of B3 has been
//! waiting for: nothing before it could say *who a project is worked for*, so
//! a rate could not be set, a budget could not be drawn, and the hours the
//! timer writes had no engagement to be worth anything on.
//!
//! It shares [`crate::projects_time`]'s conventions — the account door,
//! `Problem` errors, no validation duplicated from the store, days as
//! `YYYY-MM-DD` — and adds two of its own.
//!
//! - **One project, two halves, one answer.** A project is a `task_projects`
//!   row (the board Tasks already shows) plus, when it is client work, a
//!   `project_clients` row beside it (`docs/design/projects.md`, "One project
//!   list, extended"). This surface zips them, so an internal project appears
//!   with `client: null` rather than not at all — absence is the answer, with
//!   no sentinel to misread.
//! - **The hours are the project's, never a person's.** `hours` is the
//!   project-grain aggregate: everybody's minutes, with no per-person
//!   breakdown anywhere in the shape. That is the one cross-person read the
//!   design note allows, and [`alo_store::project_hours`] is where the reason
//!   is written down.
//!
//! **The client facts are addressed as `/projects/clients/{id}`**, not as
//! `/projects/{id}/client` as the design note first drew them. The audit
//! derivation (B2.13) reads the matched template mechanically and needs the
//! *collection* in the second segment — `/projects/{id}/client` resolves to no
//! audit action at all, and `tests/audit_routes.rs` fails the build for it.
//! Renaming the route is the cheap half of that trade; the other half would be
//! a hand-written exception in the one derivation whose whole value is that it
//! has none. The record addressed is still the project, so the trail reads
//! `projects.client.update` / `projects.client.delete` against the project's
//! own id.
//!
//! Nothing here computes money. A rate and two budgets are integers stored and
//! read back; the arithmetic that turns them into a figure is the profitability
//! report's (B3.08), server-side and never in a browser.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::{
    BillingCustomerId, NewProjectClient, ProjectClient, ProjectHours, ProjectId,
    ProjectWorkSummary, TaskProject, TaskProjectEdit,
};

use crate::billing::{blank_to_none, iso, iso_date, map_store_err, parse_body, parse_iso_date};
use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// One project's client facts as JSON, or `null` when it is internal work.
///
/// `rateCents` and both budgets are nullable and stay that way: an engagement
/// nobody has priced is legal and normal, and a rate reported as `0` would read
/// as "free" rather than "unstated" — the difference the handoff (B3.06)
/// refuses to guess at.
fn client_json(client: Option<&ProjectClient>) -> Value {
    match client {
        None => Value::Null,
        Some(c) => json!({
            "customerId": c.customer_id.as_str(),
            "currency": c.currency,
            "rateCents": c.rate_cents,
            "budgetMinutes": c.budget_minutes,
            "budgetCents": c.budget_cents,
            "startsOn": c.starts_on.map(iso_date),
            "createdAt": iso(c.created_at),
            "updatedAt": iso(c.updated_at),
        }),
    }
}

/// A project's hours to date as JSON — the aggregate, with nobody named.
///
/// `budgetConsumptionBp` is the bar's own figure, in basis points of the hours
/// budget (10 000 = the whole budget) and `null` when there is no budget to be
/// a proportion of. It is computed by the store rather than by the screen, for
/// the same reason a total is: two clients drawing the same bar must draw the
/// same bar, and a proportion is arithmetic over stored integers.
fn hours_json(hours: &ProjectHours, budget_minutes: Option<i64>) -> Value {
    json!({
        "minutes": hours.minutes,
        "billableMinutes": hours.billable_minutes,
        "billedMinutes": hours.billed_minutes,
        "lastWorkedOn": hours.last_worked_on.map(iso_date),
        "budgetConsumptionBp": hours.budget_consumption_bp(budget_minutes),
    })
}

/// One engagement as JSON: the board, its client facts (or `null`), and what it
/// has cost in hours.
fn project_json(
    project: &TaskProject,
    client: Option<&ProjectClient>,
    hours: &ProjectHours,
    work: Option<&ProjectWorkSummary>,
) -> Value {
    json!({
        "id": project.id.as_str(),
        "name": project.name,
        // `personal` boards can never be client work (the store's rule); the
        // kind is reported anyway so a screen can say why the client form is
        // not on offer instead of showing a control that always refuses.
        "kind": project.kind,
        "color": project.color,
        "ownerId": project.owner,
        "description": project.description,
        "status": project.status,
        "startsOn": project.starts_on.map(iso_date),
        "targetOn": project.target_on.map(iso_date),
        "createdAt": iso(project.created_at),
        "updatedAt": iso(project.updated_at),
        "client": client_json(client),
        "hours": hours_json(hours, client.and_then(|c| c.budget_minutes)),
        "work": {
            "openTasks": work.map_or(0, |w| w.open_tasks),
            "overdueTasks": work.map_or(0, |w| w.overdue_tasks),
            "blockedTasks": work.map_or(0, |w| w.blocked_tasks),
            "nextDueAt": work.and_then(|w| w.next_due_at).map(iso),
        },
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectBody {
    name: String,
    #[serde(default)]
    description: Option<String>,
    status: String,
    #[serde(default)]
    starts_on: Option<String>,
    #[serde(default)]
    target_on: Option<String>,
}

/// `PATCH /projects/{id}` replaces the editable lifecycle facts of a team
/// project. Client pricing remains a separate whole-record write.
pub async fn update_project(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: ProjectBody = parse_body(&body)?;
    let name = req.name.trim();
    if name.is_empty() {
        return Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "name is required",
        ));
    }
    if !matches!(
        req.status.as_str(),
        "planned" | "active" | "on_hold" | "completed" | "cancelled"
    ) {
        return Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "status is invalid",
        ));
    }
    let starts_on = optional_day("startsOn", req.starts_on.as_deref())?;
    let target_on = optional_day("targetOn", req.target_on.as_deref())?;
    if starts_on
        .zip(target_on)
        .is_some_and(|(start, target)| target < start)
    {
        return Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "targetOn must not be before startsOn",
        ));
    }
    let edit = TaskProjectEdit {
        name: name.to_owned(),
        description: blank_to_none(req.description),
        status: req.status,
        starts_on,
        target_on,
    };
    let project = account
        .acc
        .update_task_project(&ProjectId::new(id), &edit)
        .await
        .map_err(map_store_err)?;
    let client = account
        .acc
        .project_client(&project.id)
        .await
        .map_err(map_store_err)?;
    let hours = account
        .acc
        .project_hours_for(&project.id)
        .await
        .map_err(map_store_err)?;
    let work = account
        .acc
        .project_work_summaries()
        .await
        .map_err(map_store_err)?;
    let summary = work.iter().find(|summary| summary.project_id == project.id);
    Ok(Json(
        json!({ "project": project_json(&project, client.as_ref(), &hours, summary) }),
    ))
}

/// The body of `PUT /projects/clients/{id}` — the whole set of client facts,
/// stated together.
///
/// A `PUT` and not a `PATCH`, because the store's write is one idempotent
/// replacement: an engagement's client facts are a record that either applies
/// or does not, so a UI that saves one form makes one call. An unstated field
/// is therefore **cleared**, not kept — the contract a whole-record write has
/// to have if "save" is to mean what the form shows.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClientBody {
    #[serde(default)]
    customer_id: Option<String>,
    #[serde(default)]
    currency: Option<String>,
    #[serde(default)]
    rate_cents: Option<i64>,
    #[serde(default)]
    budget_minutes: Option<i64>,
    #[serde(default)]
    budget_cents: Option<i64>,
    #[serde(default)]
    starts_on: Option<String>,
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

/// Reads a day that may be absent. A blank string is absent; a malformed one is
/// a refusal, never a silent default — a start date invented by a fallback is a
/// date somebody will later read as a commitment.
fn optional_day(name: &str, raw: Option<&str>) -> Result<Option<time::Date>, Problem> {
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

/// `GET /projects` → `{"projects": [ … ]}` — every project this caller can see,
/// as client work: the board, its client facts (or `null`), and its hours.
///
/// Ordered as [`alo_store::AccountStore::task_projects`] orders boards — the
/// caller's own list first — rather than by activity: this is the same list
/// Tasks shows, seen through a second lens, and two modules disagreeing about
/// the order of one list is how a stranger concludes they are two lists.
///
/// # Errors
/// `401` without a valid bearer token; `500` on a store failure.
pub async fn list_projects(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let projects = account.acc.task_projects().await.map_err(map_store_err)?;
    let clients = account.acc.project_clients().await.map_err(map_store_err)?;
    let hours = account.acc.project_hours().await.map_err(map_store_err)?;
    let work = account
        .acc
        .project_work_summaries()
        .await
        .map_err(map_store_err)?;
    let listed: Vec<Value> = projects
        .iter()
        .map(|project| {
            let client = clients.iter().find(|c| c.project_id == project.id);
            // A board nobody has logged an hour against has no aggregate row;
            // the honest zero is the answer, not a gap in the list.
            let worked = hours
                .iter()
                .find(|h| h.project_id == project.id)
                .cloned()
                .unwrap_or_else(|| ProjectHours::none_yet(project.id.clone()));
            let summary = work.iter().find(|w| w.project_id == project.id);
            project_json(project, client, &worked, summary)
        })
        .collect();
    Ok(Json(json!({ "projects": listed })))
}

/// `GET /projects/{id}` → `{"project": {…}}` — one engagement.
///
/// # Errors
/// `401` without a valid bearer token; `404` when the project is not one this
/// caller can see — another tenant's, a colleague's private board, or one that
/// never existed, all the same answer; `500` on a store failure.
pub async fn get_project(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let project_id = ProjectId::new(id);
    // The hours read settles existence and visibility in one statement, so the
    // two answers cannot drift apart.
    let hours = account
        .acc
        .project_hours_for(&project_id)
        .await
        .map_err(map_store_err)?;
    let project = account
        .acc
        .task_projects()
        .await
        .map_err(map_store_err)?
        .into_iter()
        .find(|p| p.id == project_id)
        .ok_or_else(Problem::not_found)?;
    let client = account
        .acc
        .project_client(&project_id)
        .await
        .map_err(map_store_err)?;
    let work = account
        .acc
        .project_work_summaries()
        .await
        .map_err(map_store_err)?;
    let summary = work.iter().find(|w| w.project_id == project.id);
    Ok(Json(
        json!({ "project": project_json(&project, client.as_ref(), &hours, summary) }),
    ))
}

/// `PUT /projects/clients/{id}` `{customerId, currency?, rateCents?, …}` →
/// `{"client": {…}}` — makes a project client work, or replaces the facts that
/// already say so.
///
/// Idempotent: calling twice with the same body leaves the same record, and
/// `createdAt` survives a replacement so "when did this become client work"
/// stays answerable.
///
/// # Errors
/// `401` without a valid bearer token; `422` when `customerId` is missing,
/// `startsOn` is malformed, the project is the caller's own personal board, the
/// project or customer is archived, or a currency, rate or budget breaks its
/// rule — each naming the rule; `404` when the project or the customer is not
/// one this caller can see; `500` on a store failure.
pub async fn set_project_client(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: ClientBody = parse_body(&body)?;
    let facts = NewProjectClient {
        currency: blank_to_none(req.currency),
        rate_cents: req.rate_cents,
        budget_minutes: req.budget_minutes,
        budget_cents: req.budget_cents,
        starts_on: optional_day("startsOn", req.starts_on.as_deref())?,
        ..NewProjectClient::for_customer(BillingCustomerId::new(required_id(
            "customerId",
            req.customer_id.as_deref(),
        )?))
    };
    let client = account
        .acc
        .set_project_client(&ProjectId::new(id), &facts)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "client": client_json(Some(&client)) })))
}

/// `DELETE /projects/clients/{id}` → `{"cleared": true}` — makes a project
/// internal work again.
///
/// The hours stay. What is deleted is the *claim that they are billable to
/// somebody*: the board, its tasks and everything logged against it are
/// untouched, and hours already carried onto an invoice keep their link to the
/// document that carries them.
///
/// # Errors
/// `401` without a valid bearer token; `404` when the project is not this
/// tenant's or was not client work — detaching twice is a clean denial, not a
/// silent success; `500` on a store failure.
pub async fn clear_project_client(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .clear_project_client(&ProjectId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "cleared": true })))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    use time::{Month, OffsetDateTime};

    fn project() -> TaskProject {
        TaskProject {
            id: ProjectId::new("p1".to_owned()),
            name: "Portal rebuild".to_owned(),
            kind: "team".to_owned(),
            owner: "u1".to_owned(),
            color: None,
            description: Some("A useful engagement".to_owned()),
            status: "active".to_owned(),
            starts_on: None,
            target_on: None,
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    fn facts() -> ProjectClient {
        ProjectClient {
            project_id: ProjectId::new("p1".to_owned()),
            customer_id: BillingCustomerId::new("c1".to_owned()),
            currency: "EUR".to_owned(),
            rate_cents: Some(9_500),
            budget_minutes: Some(6_000),
            budget_cents: None,
            starts_on: time::Date::from_calendar_date(2026, Month::September, 1).ok(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn an_internal_project_says_so_by_absence() {
        let hours = ProjectHours::none_yet(ProjectId::new("p1".to_owned()));
        let value = project_json(&project(), None, &hours, None);
        assert!(value["client"].is_null(), "never a sentinel customer");
        assert_eq!(value["hours"]["minutes"], json!(0));
        assert!(
            value["hours"]["budgetConsumptionBp"].is_null(),
            "no budget is no proportion"
        );
        assert!(value["hours"]["lastWorkedOn"].is_null());
        assert_eq!(value["ownerId"], json!("u1"));
        assert_eq!(value["description"], json!("A useful engagement"));
        assert_eq!(value["status"], json!("active"));
        assert_eq!(value["createdAt"], json!("1970-01-01T00:00:00Z"));
    }

    #[test]
    fn a_project_summary_carries_work_that_needs_attention() {
        let hours = ProjectHours::none_yet(ProjectId::new("p1".to_owned()));
        let work = ProjectWorkSummary {
            project_id: ProjectId::new("p1".to_owned()),
            open_tasks: 7,
            overdue_tasks: 2,
            blocked_tasks: 1,
            next_due_at: Some(OffsetDateTime::UNIX_EPOCH),
        };
        let value = project_json(&project(), None, &hours, Some(&work));
        assert_eq!(value["work"]["openTasks"], json!(7));
        assert_eq!(value["work"]["overdueTasks"], json!(2));
        assert_eq!(value["work"]["blockedTasks"], json!(1));
        assert_eq!(value["work"]["nextDueAt"], json!("1970-01-01T00:00:00Z"));
    }

    #[test]
    fn an_unpriced_engagement_is_null_and_never_zero() {
        let unpriced = ProjectClient {
            rate_cents: None,
            budget_minutes: None,
            ..facts()
        };
        let value = client_json(Some(&unpriced));
        assert!(value["rateCents"].is_null(), "unstated, not free");
        assert!(value["budgetMinutes"].is_null());
        assert_eq!(value["currency"], json!("EUR"));
    }

    #[test]
    fn the_bar_reads_the_engagements_own_budget() {
        let hours = ProjectHours {
            minutes: 3_000,
            billable_minutes: 2_400,
            billed_minutes: 600,
            last_worked_on: time::Date::from_calendar_date(2026, Month::August, 5).ok(),
            ..ProjectHours::none_yet(ProjectId::new("p1".to_owned()))
        };
        let value = project_json(&project(), Some(&facts()), &hours, None);
        assert_eq!(value["hours"]["budgetConsumptionBp"], json!(5_000));
        assert_eq!(value["hours"]["billableMinutes"], json!(2_400));
        assert_eq!(value["hours"]["billedMinutes"], json!(600));
        assert_eq!(value["hours"]["lastWorkedOn"], json!("2026-08-05"));
        assert_eq!(value["client"]["customerId"], json!("c1"));
        assert_eq!(value["client"]["startsOn"], json!("2026-09-01"));
    }

    #[test]
    fn an_overrun_is_reported_rather_than_hidden() {
        let hours = ProjectHours {
            minutes: 9_000,
            ..ProjectHours::none_yet(ProjectId::new("p1".to_owned()))
        };
        let value = project_json(&project(), Some(&facts()), &hours, None);
        assert_eq!(value["hours"]["budgetConsumptionBp"], json!(15_000));
    }

    #[test]
    fn a_missing_customer_names_itself_in_the_refusal() {
        let problem = required_id("customerId", Some("  ")).expect_err("refused");
        assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(
            problem
                .detail
                .as_deref()
                .unwrap_or_default()
                .contains("customerId")
        );
    }

    #[test]
    fn a_start_date_is_absent_or_a_real_day_and_never_a_fallback() {
        assert_eq!(optional_day("startsOn", None).unwrap(), None);
        assert_eq!(optional_day("startsOn", Some("  ")).unwrap(), None);
        assert!(
            optional_day("startsOn", Some("2026-09-01"))
                .unwrap()
                .is_some()
        );
        let problem = optional_day("startsOn", Some("01/09/2026")).expect_err("refused");
        assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(
            problem
                .detail
                .as_deref()
                .unwrap_or_default()
                .contains("YYYY-MM-DD")
        );
    }
}
