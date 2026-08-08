//! The template HTTP surface (alo Projects, ADR 0035, wave B3.09) — the boards
//! a tenant has marked reusable, and the copy that starts a new engagement from
//! one, over [`alo_store::project_templates`].
//!
//! It shares [`crate::projects_time`]'s conventions — the account door,
//! `Problem` errors, no validation duplicated from the store, days as
//! `YYYY-MM-DD` — and adds two of its own.
//!
//! - **A template is addressed by its project id**, because a template *is* a
//!   project (`docs/design/projects.md`, "Milestones and templates"). There is
//!   no template id to keep in step with a board id, and no second record to
//!   go stale when the board is edited.
//! - **Instantiating answers with what it copied**, not just with an id: a
//!   dialog that says "12 tasks and 3 milestones" is a dialog whose promise the
//!   user can check against the board that just opened. The counts come from
//!   the store's own copy, never from the browser recounting anything.
//!
//! The trail reads `projects.template.create` when a board is marked,
//! `projects.template.delete` when the mark is taken off, and
//! `projects.template.instantiate` against the template a copy came from —
//! the derivation (B2.13) reads all three off the routes themselves.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::{BillingCustomerId, ProjectId, ProjectTemplate, TemplateCopy, TemplateInstance};

use crate::billing::{iso, map_store_err, parse_body, parse_iso_date};
use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// One template as JSON.
///
/// `taskCount` is what a copy *would carry* — open work only — so the number in
/// the dialog is the number of cards that appear on the new board. A card left
/// in the `done` column of the template is not part of the shape of the next
/// project and is neither counted nor copied.
fn template_json(template: &ProjectTemplate) -> Value {
    json!({
        // The same id under both names, and deliberately: `projectId` is what a
        // template *is*, and `id` is what the audit trail reads off a create
        // (B2.13's `created_id`) so marking a board is filed against the board
        // rather than against nothing.
        "id": template.project_id.as_str(),
        "projectId": template.project_id.as_str(),
        "name": template.name,
        "color": template.color,
        "archived": template.archived,
        "taskCount": template.task_count,
        "milestoneCount": template.milestone_count,
        "createdBy": template.created_by,
        "createdAt": iso(template.created_at),
    })
}

/// What one instantiation produced.
fn copy_json(copy: &TemplateCopy) -> Value {
    json!({
        "projectId": copy.project_id.as_str(),
        "taskCount": copy.task_count,
        "milestoneCount": copy.milestone_count,
    })
}

/// The body of `POST /projects/templates`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MarkTemplateBody {
    #[serde(default)]
    project_id: Option<String>,
}

/// The body of `POST /projects/templates/{id}/instantiate`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstantiateBody {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    starts_on: Option<String>,
    #[serde(default)]
    customer_id: Option<String>,
}

/// Reads a required id from a body, naming it in the refusal.
fn required_id(name: &str, raw: Option<&str>) -> Result<String, Problem> {
    raw.map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| {
            Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("{name} is required"),
            )
        })
}

/// Reads an optional `YYYY-MM-DD` day, refusing anything else rather than
/// falling back to "no shift" — a start date the server quietly ignored would
/// hand back a plan on the template's own dates and look like a bug in the copy.
fn optional_day(name: &str, raw: Option<&str>) -> Result<Option<time::Date>, Problem> {
    let Some(stated) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    parse_iso_date(stated).map(Some).ok_or_else(|| {
        Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("{name} must be a date of the form YYYY-MM-DD"),
        )
    })
}

/// `GET /projects/templates` → `{"templates": [ … ]}` — the reusable boards of
/// this tenant, in the order they were marked.
///
/// Tenant-wide by construction: only a shared team board can be marked, so this
/// list never names a colleague's private work.
///
/// # Errors
/// `401` without a valid bearer token; `500` on a store failure.
pub async fn list_templates(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let templates = account.acc.templates().await.map_err(map_store_err)?;
    Ok(Json(json!({
        "templates": templates.iter().map(template_json).collect::<Vec<_>>(),
    })))
}

/// `POST /projects/templates` `{projectId}` → `{"template": {…}}` — marks a
/// board reusable.
///
/// Idempotent: marking twice leaves one template and keeps the first mark's
/// date, because "since when has this been a template" is a fact about the
/// first time somebody said so.
///
/// # Errors
/// `401` without a valid bearer token; `422` when `projectId` is missing, when
/// the board is the caller's own personal one, or when it is archived — each
/// naming the rule; `404` when the board is not one this caller can see; `500`
/// on a store failure.
pub async fn mark_template(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: MarkTemplateBody = parse_body(&body)?;
    let project = ProjectId::new(required_id("projectId", req.project_id.as_deref())?);
    let template = account
        .acc
        .mark_template(&project)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "template": template_json(&template) })))
}

/// `DELETE /projects/templates/{id}` → `{"deleted": true}` — takes the mark off
/// a board.
///
/// The board, its tasks and its plan are untouched: what is deleted is the
/// claim that it is reusable, never work.
///
/// # Errors
/// `401` without a valid bearer token; `404` when the board is not this
/// tenant's or was not marked — unmarking twice is a clean denial, not a silent
/// success; `500` on a store failure.
pub async fn unmark_template(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .unmark_template(&ProjectId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "deleted": true })))
}

/// `POST /projects/templates/{id}/instantiate` `{name, startsOn?, customerId?}`
/// → `{"copy": {…}}` — starts a new project from a template.
///
/// The copy carries the board's colour, its open tasks with their columns,
/// order, priorities, labels and checklists, its milestones and the
/// task→milestone links, with every date shifted so the template's earliest
/// milestone lands on `startsOn`. It carries no assignees, comments, history,
/// followers, attachments, dependencies, hours or finished cards — and never
/// the template's customer, which is why `customerId` is the caller's to state.
///
/// # Errors
/// `401` without a valid bearer token; `422` when `name` is missing or too
/// long, when `startsOn` is not a `YYYY-MM-DD` day, when the template carries
/// more than 500 copyable tasks, or when the customer is archived; `404` when
/// the template or the customer is not this tenant's; `500` on a store failure.
pub async fn instantiate_template(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: InstantiateBody = parse_body(&body)?;
    let instance = TemplateInstance {
        // The bound and the blank rule are the store's; what is checked here is
        // only that a name was stated at all.
        name: req.name.unwrap_or_default(),
        starts_on: optional_day("startsOn", req.starts_on.as_deref())?,
        customer_id: req
            .customer_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| BillingCustomerId::new(value.to_owned())),
    };
    let copy = account
        .acc
        .instantiate_template(&ProjectId::new(id), &instance)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "copy": copy_json(&copy) })))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    use time::{Month, OffsetDateTime};

    fn template() -> ProjectTemplate {
        ProjectTemplate {
            project_id: ProjectId::new("p1".to_owned()),
            name: "Website relaunch".to_owned(),
            color: Some("#4b83c4".to_owned()),
            archived: false,
            task_count: 12,
            milestone_count: 3,
            created_by: "u1".to_owned(),
            created_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn a_template_reports_the_board_and_what_a_copy_would_carry() {
        let value = template_json(&template());
        assert_eq!(value["projectId"], json!("p1"));
        assert_eq!(
            value["id"],
            json!("p1"),
            "and again as `id`, which is what the audit trail reads off a create"
        );
        assert_eq!(value["name"], json!("Website relaunch"));
        assert_eq!(value["archived"], json!(false));
        assert_eq!(value["taskCount"], json!(12));
        assert_eq!(value["milestoneCount"], json!(3));
    }

    #[test]
    fn a_copy_answers_with_what_it_copied() {
        let value = copy_json(&TemplateCopy {
            project_id: ProjectId::new("p2".to_owned()),
            task_count: 12,
            milestone_count: 3,
        });
        assert_eq!(value["projectId"], json!("p2"));
        assert_eq!(value["taskCount"], json!(12));
        assert_eq!(value["milestoneCount"], json!(3));
    }

    #[test]
    fn a_template_is_always_a_template_of_something() {
        for absent in [None, Some("  ")] {
            let problem = required_id("projectId", absent).expect_err("refused");
            assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY);
            assert!(
                problem
                    .detail
                    .as_deref()
                    .unwrap_or_default()
                    .contains("projectId")
            );
        }
    }

    #[test]
    fn a_start_date_is_optional_but_never_guessed_at() {
        assert_eq!(optional_day("startsOn", None).unwrap(), None);
        assert_eq!(optional_day("startsOn", Some("  ")).unwrap(), None);
        assert_eq!(
            optional_day("startsOn", Some("2026-10-01")).unwrap(),
            Some(time::Date::from_calendar_date(2026, Month::October, 1).unwrap())
        );
        let problem = optional_day("startsOn", Some("01/10/2026")).expect_err("refused");
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
