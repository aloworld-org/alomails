//! The plan HTTP surface (alo Projects, ADR 0035, wave B3.09) — the milestones
//! of a project and where each task sits among them, over
//! [`alo_store::project_milestones`].
//!
//! This is the timeline's one read. `GET /projects/milestones?projectId=…`
//! answers with the plan **and** the placements together, because a timeline
//! that fetched them separately would draw a bar before it knew what was under
//! it, and two round trips to render one screen is how a "simple view" becomes
//! a loading state.
//!
//! It shares [`crate::projects_time`]'s conventions — the account door,
//! `Problem` errors, no validation duplicated from the store, days as
//! `YYYY-MM-DD` — and adds two of its own.
//!
//! - **`late` is judged against the server's date**, like an invoice's
//!   `overdue` ([`crate::billing_document::today`]): whether a deadline has
//!   passed is a fact about the plan, not about the reader's clock, and a
//!   browser with a wrong date must not be able to clear its own late list.
//! - **Reaching a milestone is its own route**, not a field on the edit. A
//!   `PATCH` that could quietly close a deliverable while fixing a typo is a
//!   `PATCH` whose audit line lies about what happened; `POST …/done` files
//!   `projects.milestone.done` against the milestone and says what it did.
//!
//! **The plan is addressed as `/projects/milestones/{id}`** rather than the
//! design note's `/projects/{id}/milestones`, for the reason
//! [`crate::projects_clients`] gives in full: the audit derivation (B2.13) reads
//! the matched template mechanically and needs the *collection* in the second
//! segment. The project is stated as `projectId` — a query parameter on the
//! read, a body field on the create — and the trail reads
//! `projects.milestone.create` / `.update` / `.done` / `.delete` against the
//! milestone's own id, and `projects.task.milestone.update` / `.delete` against
//! the task whose place in the plan changed.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use time::Date;

use alo_store::{
    Milestone, MilestoneEdit, NewMilestone, ProjectId, ProjectMilestoneId, TaskId, TaskPlacement,
};

use crate::billing::{iso, iso_date, map_store_err, parse_body, parse_iso_date};
use crate::billing_document::today;
use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// One milestone as JSON, with the two facts a timeline draws beside it.
///
/// `taskCount`/`taskDoneCount` are information beside `done`, never the thing
/// itself: a milestone whose tasks are all closed is still not reached until
/// somebody says it is (`docs/design/projects.md`, "Milestones and templates").
fn milestone_json(m: &Milestone, today: Date) -> Value {
    json!({
        "id": m.id.as_str(),
        "projectId": m.project_id.as_str(),
        "name": m.name,
        "dueOn": iso_date(m.due_on),
        "done": m.is_done(),
        "doneAt": m.done_at.map(iso),
        "late": m.is_late(today),
        "taskCount": m.task_count,
        "taskDoneCount": m.task_done_count,
        "createdAt": iso(m.created_at),
        "updatedAt": iso(m.updated_at),
    })
}

/// One task's place in the plan as JSON.
fn placement_json(p: &TaskPlacement) -> Value {
    json!({
        "taskId": p.task_id.as_str(),
        "milestoneId": p.milestone_id.as_str(),
    })
}

/// The project a plan read is about.
#[derive(Deserialize)]
pub struct PlanQuery {
    #[serde(default, rename = "projectId")]
    project_id: Option<String>,
}

/// The body of `POST /projects/milestones`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NewMilestoneBody {
    #[serde(default)]
    project_id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    due_on: Option<String>,
}

/// The body of `PATCH /projects/milestones/{id}` — the two facts a milestone
/// is, stated together.
///
/// A whole-record `PATCH` rather than a sparse one, like the time entry's
/// (B3.04) and for the same reason: the store's [`MilestoneEdit`] is a whole
/// record, "the milestone now says this", so a form that shows two fields saves
/// two fields.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EditMilestoneBody {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    due_on: Option<String>,
}

/// The body of `POST /projects/milestones/{id}/done`. Absent means reached —
/// the button's ordinary call — and `{"done": false}` puts it back ahead of us.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DoneBody {
    #[serde(default)]
    done: Option<bool>,
}

/// The body of `PUT /projects/tasks/{task_id}/milestone`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlacementBody {
    #[serde(default)]
    milestone_id: Option<String>,
}

/// Reads a required id from a body or a query, naming it in the refusal.
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

/// Reads the day a milestone falls on. Required and never defaulted: a
/// milestone without a date is a label, and a date invented by a fallback is a
/// deadline somebody will later read as a commitment.
fn required_day(name: &str, raw: Option<&str>) -> Result<Date, Problem> {
    let stated = raw
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| {
            Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("{name} is required: a milestone is a named date"),
            )
        })?;
    parse_iso_date(stated).ok_or_else(|| {
        Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("{name} must be a date of the form YYYY-MM-DD"),
        )
    })
}

/// `GET /projects/milestones?projectId=…` →
/// `{"milestones": [ … ], "placements": [ … ]}` — one project's plan and where
/// its tasks sit in it, the timeline's single read.
///
/// A project with no plan answers with two empty lists, and so does a project
/// this caller cannot see: existence is never disclosed by the shape of a list.
///
/// # Errors
/// `401` without a valid bearer token; `422` when `projectId` is missing — a
/// plan is always a plan *of something*; `500` on a store failure.
pub async fn list_plan(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PlanQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let project = ProjectId::new(required_id("projectId", query.project_id.as_deref())?);
    let milestones = account
        .acc
        .milestones(&project)
        .await
        .map_err(map_store_err)?;
    let placements = account
        .acc
        .task_placements(&project)
        .await
        .map_err(map_store_err)?;
    let today = today();
    Ok(Json(json!({
        "milestones": milestones.iter().map(|m| milestone_json(m, today)).collect::<Vec<_>>(),
        "placements": placements.iter().map(placement_json).collect::<Vec<_>>(),
    })))
}

/// `GET /projects/milestones/{id}` → `{"milestone": {…}}` — one milestone.
///
/// # Errors
/// `401` without a valid bearer token; `404` when the milestone is not one this
/// caller can see — another tenant's, one on a colleague's private board, or
/// one that never existed, all the same answer; `500` on a store failure.
pub async fn get_milestone(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let milestone = account
        .acc
        .milestone(&ProjectMilestoneId::new(id))
        .await
        .map_err(map_store_err)?
        .ok_or_else(Problem::not_found)?;
    Ok(Json(
        json!({ "milestone": milestone_json(&milestone, today()) }),
    ))
}

/// `POST /projects/milestones` `{projectId, name, dueOn}` →
/// `{"milestone": {…}}` — plans a date.
///
/// # Errors
/// `401` without a valid bearer token; `422` when `projectId`, `name` or
/// `dueOn` is missing or malformed, when the project is archived, or when the
/// plan is already at its 200-milestone ceiling — each naming the rule; `404`
/// when the project is not one this caller can see; `500` on a store failure.
pub async fn create_milestone(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: NewMilestoneBody = parse_body(&body)?;
    let project = ProjectId::new(required_id("projectId", req.project_id.as_deref())?);
    let new = NewMilestone {
        // The bound and the blank rule are the store's; what is checked here is
        // only that a name was stated at all.
        name: req.name.unwrap_or_default(),
        due_on: required_day("dueOn", req.due_on.as_deref())?,
    };
    let milestone = account
        .acc
        .create_milestone(&project, &new)
        .await
        .map_err(map_store_err)?;
    Ok(Json(
        json!({ "milestone": milestone_json(&milestone, today()) }),
    ))
}

/// `PATCH /projects/milestones/{id}` `{name, dueOn}` → `{"milestone": {…}}` —
/// renames a milestone or moves its date.
///
/// # Errors
/// `401` without a valid bearer token; `422` when `dueOn` is missing or
/// malformed or the name breaks its rule; `404` when the milestone is not one
/// this caller can see; `500` on a store failure.
pub async fn update_milestone(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: EditMilestoneBody = parse_body(&body)?;
    let edit = MilestoneEdit {
        name: req.name.unwrap_or_default(),
        due_on: required_day("dueOn", req.due_on.as_deref())?,
    };
    let milestone = account
        .acc
        .update_milestone(&ProjectMilestoneId::new(id), &edit)
        .await
        .map_err(map_store_err)?;
    Ok(Json(
        json!({ "milestone": milestone_json(&milestone, today()) }),
    ))
}

/// `POST /projects/milestones/{id}/done` `{done?}` → `{"milestone": {…}}` —
/// marks a milestone reached, or puts it back ahead of us.
///
/// Idempotent in both directions, and a second click does not restamp
/// `doneAt`: it answers "when was this reached", and a button pressed twice is
/// not two events.
///
/// # Errors
/// `401` without a valid bearer token; `404` when the milestone is not one this
/// caller can see; `500` on a store failure.
pub async fn set_milestone_done(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    // A bodyless call from a one-button control is the ordinary case.
    let req: DoneBody = if body.is_empty() {
        DoneBody { done: None }
    } else {
        parse_body(&body)?
    };
    let milestone = account
        .acc
        .set_milestone_done(&ProjectMilestoneId::new(id), req.done.unwrap_or(true))
        .await
        .map_err(map_store_err)?;
    Ok(Json(
        json!({ "milestone": milestone_json(&milestone, today()) }),
    ))
}

/// `DELETE /projects/milestones/{id}` → `{"deleted": true}` — takes a date out
/// of the plan.
///
/// The tasks under it stay exactly where they are on the board; what is deleted
/// is the date they were placed against. Deleting a plan never deletes work.
///
/// # Errors
/// `401` without a valid bearer token; `404` when the milestone is not one this
/// caller can see, or was already deleted — deleting twice is a clean denial,
/// not a silent success; `500` on a store failure.
pub async fn delete_milestone(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .delete_milestone(&ProjectMilestoneId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "deleted": true })))
}

/// `PUT /projects/tasks/{task_id}/milestone` `{milestoneId}` →
/// `{"placement": {…}}` — puts a task under a milestone, or moves it to
/// another one, which is the same call because a task has one place in a plan.
///
/// # Errors
/// `401` without a valid bearer token; `422` when `milestoneId` is missing or
/// the task and the milestone belong to different projects — a plan does not
/// reach across boards; `404` when either is not one this caller can see;
/// `500` on a store failure.
pub async fn place_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: PlacementBody = parse_body(&body)?;
    let task = TaskId::new(task_id);
    let milestone =
        ProjectMilestoneId::new(required_id("milestoneId", req.milestone_id.as_deref())?);
    account
        .acc
        .set_task_milestone(&task, &milestone)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "placement": placement_json(&TaskPlacement { task_id: task, milestone_id: milestone }),
    })))
}

/// `DELETE /projects/tasks/{task_id}/milestone` → `{"cleared": true}` — takes a
/// task out of the plan, leaving it on the board.
///
/// # Errors
/// `401` without a valid bearer token; `404` when the task is not one this
/// caller can see or was not placed under any milestone; `500` on a store
/// failure.
pub async fn unplace_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .clear_task_milestone(&TaskId::new(task_id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "cleared": true })))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    use time::{Month, OffsetDateTime};

    fn day(day: u8) -> Date {
        Date::from_calendar_date(2026, Month::September, day).unwrap()
    }

    fn milestone() -> Milestone {
        Milestone {
            id: ProjectMilestoneId::new("m1".to_owned()),
            project_id: ProjectId::new("p1".to_owned()),
            name: "Design signed off".to_owned(),
            due_on: day(30),
            done_at: None,
            position: 0,
            created_by: "u1".to_owned(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
            task_count: 4,
            task_done_count: 4,
        }
    }

    #[test]
    fn a_milestone_reports_its_day_its_state_and_its_work() {
        let value = milestone_json(&milestone(), day(1));
        assert_eq!(value["dueOn"], json!("2026-09-30"));
        assert_eq!(value["done"], json!(false));
        assert!(value["doneAt"].is_null());
        assert_eq!(value["late"], json!(false));
        assert_eq!(value["taskCount"], json!(4));
        assert_eq!(value["taskDoneCount"], json!(4));
    }

    #[test]
    fn every_task_closed_is_not_the_milestone_reached() {
        let value = milestone_json(&milestone(), day(1));
        assert_eq!(
            value["taskCount"], value["taskDoneCount"],
            "all its work is done…"
        );
        assert_eq!(value["done"], json!(false), "…and it is still not reached");
    }

    #[test]
    fn late_is_the_servers_judgement_and_a_reached_milestone_is_never_late() {
        let value = milestone_json(&milestone(), day(30));
        assert_eq!(value["late"], json!(false), "today is not late");
        let value = milestone_json(
            &milestone(),
            Date::from_calendar_date(2026, Month::October, 1).unwrap(),
        );
        assert_eq!(value["late"], json!(true));
        let reached = Milestone {
            done_at: Some(OffsetDateTime::UNIX_EPOCH),
            ..milestone()
        };
        let value = milestone_json(
            &reached,
            Date::from_calendar_date(2026, Month::December, 1).unwrap(),
        );
        assert_eq!(value["late"], json!(false), "however late it was reached");
        assert_eq!(value["done"], json!(true));
        assert!(!value["doneAt"].is_null());
    }

    #[test]
    fn a_plan_is_always_a_plan_of_something() {
        let problem = required_id("projectId", Some("  ")).expect_err("refused");
        assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(
            problem
                .detail
                .as_deref()
                .unwrap_or_default()
                .contains("projectId")
        );
    }

    #[test]
    fn a_date_is_required_and_never_invented() {
        for absent in [None, Some("  ")] {
            let problem = required_day("dueOn", absent).expect_err("refused");
            assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY);
            assert!(
                problem
                    .detail
                    .as_deref()
                    .unwrap_or_default()
                    .contains("named date"),
                "the refusal says why a milestone needs one"
            );
        }
        let problem = required_day("dueOn", Some("30/09/2026")).expect_err("refused");
        assert!(
            problem
                .detail
                .as_deref()
                .unwrap_or_default()
                .contains("YYYY-MM-DD")
        );
        assert_eq!(required_day("dueOn", Some("2026-09-30")).unwrap(), day(30));
    }

    #[test]
    fn a_placement_names_both_ends() {
        let value = placement_json(&TaskPlacement {
            task_id: TaskId::new("t1".to_owned()),
            milestone_id: ProjectMilestoneId::new("m1".to_owned()),
        });
        assert_eq!(value["taskId"], json!("t1"));
        assert_eq!(value["milestoneId"], json!("m1"));
    }
}
