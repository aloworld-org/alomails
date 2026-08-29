//! Onboarding and offboarding checklists, over HTTP (alo HR, ADR 0035, wave
//! B6.05) — over [`alo_store::hr_checklists`].
//!
//! Three decisions this file makes, each of them about a door rather than about
//! a field:
//!
//! - **Templates are HR's, both ways.** What a company does when somebody
//!   arrives is a company-wide shape, and — unlike a leave policy, which staff
//!   must read to ask for time off — a template is of no use to anybody who
//!   cannot run it. So the whole `/hr/checklist-templates` surface is behind
//!   [`crate::state::Account::require_hr`].
//! - **Running one is HR's; reading one back is HR's or the person's own.** A
//!   newcomer looking at "my checklist" on their first morning is the design
//!   note's own `GET /hr/me` promise, and the resolution reuses
//!   [`LeaveDoor`] — the module's single answer to "whose record is this?" —
//!   rather than spelling a second version of it here. A manager sees their
//!   reports' checklists for the same reason they see their leave.
//! - **A refusal about somebody else's record is a `404`, never a `403`.** A
//!   `403` would confirm the record exists and whose it is.
//!
//! The tasks a run creates are ordinary tasks on an ordinary shared board, so
//! everything after this route — reassigning a step, ticking it, commenting on
//! it — happens in the Tasks module with no HR surface involved. That is the
//! design's central bet, and it is why this file is short.
//!
//! **What is not here: provisioning.** "Create the mailbox" is a step somebody
//! does and ticks. No route in this file can create an account, and none will
//! (`docs/design/hr.md`, "Cuts").

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use time::Date;

use alo_store::hr_checklists::{
    ChecklistKind, ChecklistOwners, ChecklistProgress, ChecklistRun, ChecklistTemplate,
    NewChecklistRun, NewChecklistStep, NewChecklistTemplate, PlannedStep, StepOwner,
};
use alo_store::{HrChecklistTemplateId, HrEmployeeId, TenantStore, UserId};

use crate::billing::{iso, iso_date, map_store_err, parse_body, parse_iso_date};
use crate::error::Problem;
use crate::hr_leave_door::LeaveDoor;
use crate::state::{AppState, authenticate};

/// One template as JSON, steps included: a template *is* its steps, and a list
/// that showed only names would need a second round trip per row to be useful.
fn template_json(template: &ChecklistTemplate) -> Value {
    json!({
        "id": template.id.as_str(),
        "name": template.name,
        "kind": template.kind.as_str(),
        "steps": template.steps.iter().map(|step| json!({
            "id": step.id.as_str(),
            "title": step.title,
            "detail": step.detail,
            "owner": step.owner.as_str(),
            "dayOffset": step.day_offset,
        })).collect::<Vec<_>>(),
        "createdBy": template.created_by,
        "createdAt": iso(template.created_at),
        "updatedAt": iso(template.updated_at),
    })
}

/// A run as JSON — including who each step resolved to, so the person who drew
/// the checklist sees the assignment rather than discovering it on somebody
/// else's board.
fn run_json(run: &ChecklistRun) -> Value {
    json!({
        "projectId": run.project_id.as_str(),
        "name": run.name,
        "templateId": run.template_id.as_str(),
        "kind": run.kind.as_str(),
        "anchorOn": iso_date(run.anchor_on),
        "steps": run.steps.iter().map(step_json).collect::<Vec<_>>(),
    })
}

fn step_json(step: &PlannedStep) -> Value {
    json!({
        "taskId": step.task_id.as_str(),
        "title": step.title,
        "owner": step.owner.as_str(),
        "assignee": step.assignee.as_str(),
        "dueOn": iso_date(step.due_on),
    })
}

/// One running checklist as JSON, folded from its own tasks. Shared with the
/// agent's `open_checklists` ([`crate::hr_intents`]), so the screen and the
/// agent state a checklist's progress in one shape.
pub(crate) fn progress_json(progress: &ChecklistProgress) -> Value {
    json!({
        "projectId": progress.project_id.as_str(),
        "name": progress.name,
        "total": progress.total,
        "done": progress.done,
        "complete": progress.is_complete(),
        "firstDueOn": progress.first_due_on.map(iso_date),
        "lastDueOn": progress.last_due_on.map(iso_date),
    })
}

/// The writable shape of one step.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StepBody {
    title: String,
    #[serde(default)]
    detail: Option<String>,
    owner: String,
    #[serde(default)]
    day_offset: Option<i32>,
}

impl StepBody {
    fn read(self) -> Result<NewChecklistStep, Problem> {
        Ok(NewChecklistStep {
            title: self.title,
            detail: self.detail.unwrap_or_default(),
            owner: StepOwner::parse(&self.owner).map_err(map_store_err)?,
            day_offset: self.day_offset.unwrap_or_default(),
        })
    }
}

/// The writable shape of a template.
///
/// Steps are stated whole or not at all: a checklist is a short ordered list,
/// and a per-step diff would be a reordering protocol between two screens to
/// save writing sixty rows nobody is racing over. An absent `steps` on a `PATCH`
/// keeps the stored ones.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TemplateBody {
    #[serde(default)]
    name: Option<String>,
    /// Required on create; **ignored on edit** — turning an onboarding into an
    /// offboarding silently reverses what every offset in it means.
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    steps: Option<Vec<StepBody>>,
}

impl TemplateBody {
    /// Merges the stated fields onto `base`.
    fn apply(self, base: NewChecklistTemplate) -> Result<NewChecklistTemplate, Problem> {
        let steps = match self.steps {
            None => base.steps,
            Some(stated) => stated
                .into_iter()
                .map(StepBody::read)
                .collect::<Result<Vec<_>, Problem>>()?,
        };
        Ok(NewChecklistTemplate {
            name: self.name.unwrap_or(base.name),
            kind: match self.kind.as_deref() {
                None => base.kind,
                Some(word) => ChecklistKind::parse(word).map_err(map_store_err)?,
            },
            steps,
        })
    }
}

/// The stored template as writable input — the base a `PATCH` merges onto.
fn editable(template: &ChecklistTemplate) -> NewChecklistTemplate {
    NewChecklistTemplate {
        name: template.name.clone(),
        kind: template.kind,
        steps: template
            .steps
            .iter()
            .map(|step| NewChecklistStep {
                title: step.title.clone(),
                detail: step.detail.clone(),
                owner: step.owner,
                day_offset: step.day_offset,
            })
            .collect(),
    }
}

/// The default a create merges onto: an onboarding with no steps, which
/// [`alo_store::hr_checklists`] refuses — so a body that states nothing is
/// answered by the rule rather than by an empty template.
fn blank() -> NewChecklistTemplate {
    NewChecklistTemplate {
        name: String::new(),
        kind: ChecklistKind::Onboarding,
        steps: Vec::new(),
    }
}

/// Loads one of the tenant's templates, or the `404` an id from another tenant
/// gets.
async fn load(hr: &TenantStore, id: &HrChecklistTemplateId) -> Result<ChecklistTemplate, Problem> {
    hr.hr_checklist_template(id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such checklist template"))
}

/// `GET /hr/checklist-templates` → `{"templates":[…]}` — **HR only**: the shapes
/// this company runs when somebody arrives or leaves, each with its steps.
///
/// # Errors
/// `401`/`403` per the HR door.
pub async fn list_templates(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_hr()?;
    let templates = state
        .store
        .for_tenant(account.tenant.clone())
        .hr_checklist_templates()
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "templates": templates.iter().map(template_json).collect::<Vec<_>>(),
    })))
}

/// `POST /hr/checklist-templates` `{name, kind, steps:[…]}` →
/// `{"template":{…}}` — **HR only**.
///
/// # Errors
/// `401`/`403` per the HR door; `409` when a template of this kind already has
/// the name; `422` on a blank name, no steps, or a step the caller can fix.
pub async fn create_template(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_hr()?;
    let req: TemplateBody = parse_body(&body)?;
    let input = req.apply(blank())?;
    let hr = state.store.for_tenant(account.tenant.clone());
    let id = hr
        .create_hr_checklist_template(&input, &account.user)
        .await
        .map_err(map_store_err)?;
    Ok(Json(
        json!({ "template": template_json(&load(&hr, &id).await?) }),
    ))
}

/// `GET /hr/checklist-templates/{id}` → `{"template":{…}}` — **HR only**.
///
/// # Errors
/// `401`/`403` per the HR door; `404` when the id is not this tenant's.
pub async fn get_template(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_hr()?;
    let hr = state.store.for_tenant(account.tenant.clone());
    let template = load(&hr, &HrChecklistTemplateId::new(id)).await?;
    Ok(Json(json!({ "template": template_json(&template) })))
}

/// `PATCH /hr/checklist-templates/{id}` `{name?, steps?}` →
/// `{"template":{…}}` — **HR only**.
///
/// Stating `steps` replaces every step; omitting it keeps them. Checklists
/// already running are untouched either way: an instance is a copy.
///
/// # Errors
/// `401`/`403` per the HR door; `404` when the template is not this tenant's;
/// `409` when another template of the same kind has the name; `422` on a step
/// the caller can fix.
pub async fn update_template(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_hr()?;
    let req: TemplateBody = parse_body(&body)?;
    let hr = state.store.for_tenant(account.tenant.clone());
    let id = HrChecklistTemplateId::new(id);
    let stored = load(&hr, &id).await?;
    let input = req.apply(editable(&stored))?;
    hr.update_hr_checklist_template(&id, &input)
        .await
        .map_err(map_store_err)?;
    Ok(Json(
        json!({ "template": template_json(&load(&hr, &id).await?) }),
    ))
}

/// `DELETE /hr/checklist-templates/{id}` → `{"deleted":true}` — **HR only**.
///
/// Deletion rather than archiving, and it is honest: a checklist already running
/// is a copy on its own board, so nothing anybody is working through depends on
/// this row.
///
/// # Errors
/// `401`/`403` per the HR door; `404` when the template is not this tenant's or
/// is already gone.
pub async fn delete_template(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_hr()?;
    state
        .store
        .for_tenant(account.tenant.clone())
        .delete_hr_checklist_template(&HrChecklistTemplateId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "deleted": true })))
}

/// The body of a run.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunBody {
    template_id: String,
    /// The day every step is dated from: the first day for an onboarding, the
    /// last day for an offboarding.
    anchor_on: String,
    /// The board's name. Absent takes the template's name and the person's.
    #[serde(default)]
    name: Option<String>,
    /// Who fills the roles this run. Every one optional.
    #[serde(default)]
    owners: Option<OwnersBody>,
}

/// The people a caller names for the roles a template mentions.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct OwnersBody {
    #[serde(default)]
    hr: Option<String>,
    #[serde(default)]
    manager: Option<String>,
    #[serde(default)]
    it: Option<String>,
    #[serde(default)]
    employee: Option<String>,
}

impl OwnersBody {
    /// A blank string is "not stated", not "a user whose id is empty": an
    /// unfilled picker sends `""` more often than it sends `null`.
    fn read(self) -> ChecklistOwners {
        fn user(raw: Option<String>) -> Option<UserId> {
            raw.map(|id| id.trim().to_owned())
                .filter(|id| !id.is_empty())
                .map(UserId::new)
        }
        ChecklistOwners {
            hr: user(self.hr),
            manager: user(self.manager),
            it: user(self.it),
            employee: user(self.employee),
        }
    }
}

/// A day the caller stated, or the `422` that names the format.
fn stated_day(raw: &str, field: &str) -> Result<Date, Problem> {
    parse_iso_date(raw.trim()).ok_or_else(|| {
        Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("{field} must be a date written YYYY-MM-DD"),
        )
    })
}

/// `POST /hr/employees/{id}/checklists`
/// `{templateId, anchorOn, name?, owners?}` → `{"run":{…}}` — **HR only**:
/// draw a checklist for this person.
///
/// What lands is a real task board: the template's steps as tasks, each assigned
/// to the person its role resolves to and dated from `anchorOn`. Everything
/// afterwards — reassigning a step, ticking it, commenting on it — is the Tasks
/// module, because a checklist step that arrives as a task arrives where its
/// owner already looks.
///
/// # Errors
/// `401`/`403` per the HR door; `404` when the template, the person, or a named
/// user is not this tenant's; `422` when the person's record is archived, the
/// date is not a date, or the anchor moves a step off the calendar.
pub async fn run_checklist(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_hr()?;
    let req: RunBody = parse_body(&body)?;
    let run = NewChecklistRun {
        template_id: HrChecklistTemplateId::new(req.template_id),
        anchor_on: stated_day(&req.anchor_on, "anchorOn")?,
        name: req.name.unwrap_or_default(),
        owners: req.owners.unwrap_or_default().read(),
    };
    let landed = account
        .acc
        .instantiate_hr_checklist(&HrEmployeeId::new(id), &run)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "run": run_json(&landed) })))
}

/// `GET /hr/employees/{id}/checklists` → `{"checklists":[…]}` — the checklists
/// ever drawn for this person, newest first, each with how far through it is.
///
/// **HR, their manager, or the person themselves.** A newcomer looking at their
/// own first week is the point of the read; the same door as their leave decides
/// it, and a stranger's `404` does not say whether the record exists.
///
/// # Errors
/// `401` without a valid bearer token; `404` when the person is not one the
/// caller may read.
pub async fn list_checklists(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let employee = HrEmployeeId::new(id);
    let door = LeaveDoor::resolve(&account).await?;
    if !door.may_read(&employee) {
        return Err(Problem::with(StatusCode::NOT_FOUND, "no such employee"));
    }
    let checklists = account
        .acc
        .hr_employee_checklists(&employee)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "checklists": checklists.iter().map(progress_json).collect::<Vec<_>>(),
        "hr": door.is_hr,
    })))
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    fn template_body(value: Value) -> TemplateBody {
        serde_json::from_value(value).unwrap_or_else(|e| panic!("body rejected: {e}"))
    }

    fn run_body(value: Value) -> RunBody {
        serde_json::from_value(value).unwrap_or_else(|e| panic!("body rejected: {e}"))
    }

    fn stored() -> NewChecklistTemplate {
        NewChecklistTemplate {
            name: "Nieuwe collega".to_owned(),
            kind: ChecklistKind::Onboarding,
            steps: vec![NewChecklistStep {
                title: "Order the laptop".to_owned(),
                detail: "Standard machine.".to_owned(),
                owner: StepOwner::It,
                day_offset: -5,
            }],
        }
    }

    #[test]
    fn an_empty_patch_changes_nothing() {
        let merged = template_body(json!({}))
            .apply(stored())
            .unwrap_or_else(|e| panic!("rejected: {e:?}"));
        assert_eq!(merged.name, "Nieuwe collega");
        assert_eq!(merged.kind, ChecklistKind::Onboarding);
        assert_eq!(merged.steps.len(), 1);
        assert_eq!(merged.steps[0].day_offset, -5);
        assert_eq!(merged.steps[0].detail, "Standard machine.");
    }

    #[test]
    fn stated_steps_replace_the_lot_and_default_where_a_field_is_absent() {
        let merged = template_body(json!({
            "steps": [
                { "title": "Welcome lunch", "owner": "manager" },
                { "title": "Handbook", "owner": "employee", "dayOffset": 2, "detail": "" },
            ]
        }))
        .apply(stored())
        .unwrap_or_else(|e| panic!("rejected: {e:?}"));
        assert_eq!(merged.steps.len(), 2);
        assert_eq!(merged.steps[0].owner, StepOwner::Manager);
        assert_eq!(
            merged.steps[0].day_offset, 0,
            "an unstated offset is the anchor day itself"
        );
        assert_eq!(merged.steps[0].detail, "");
        assert_eq!(merged.steps[1].day_offset, 2);
        // An explicitly empty list reaches the store, which is what refuses it —
        // the rule lives in one place.
        let emptied = template_body(json!({ "steps": [] }))
            .apply(stored())
            .unwrap_or_else(|e| panic!("rejected: {e:?}"));
        assert!(emptied.steps.is_empty());
    }

    #[test]
    fn a_word_this_build_does_not_know_is_refused() {
        assert!(
            template_body(json!({ "kind": "probation" }))
                .apply(stored())
                .is_err()
        );
        assert!(
            template_body(json!({ "steps": [{ "title": "x", "owner": "facilities" }] }))
                .apply(stored())
                .is_err()
        );
    }

    #[test]
    fn an_unfilled_owner_picker_is_not_a_user_called_nothing() {
        let owners = run_body(json!({
            "templateId": "t-1",
            "anchorOn": "2026-09-01",
            "owners": { "it": "u-it", "manager": "  ", "hr": null }
        }))
        .owners
        .unwrap_or_default()
        .read();
        assert_eq!(owners.it.as_ref().map(UserId::as_str), Some("u-it"));
        assert!(owners.manager.is_none(), "a blank string is not a user");
        assert!(owners.hr.is_none());
        assert!(owners.employee.is_none());
    }

    #[test]
    fn a_date_that_is_not_a_date_names_the_format() {
        assert!(stated_day("2026-09-01", "anchorOn").is_ok());
        let refused =
            stated_day("1 September 2026", "anchorOn").expect_err("a sentence is not a date");
        assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
        let detail = refused.detail.unwrap_or_default();
        assert!(detail.contains("YYYY-MM-DD"), "got: {detail}");
    }
}
