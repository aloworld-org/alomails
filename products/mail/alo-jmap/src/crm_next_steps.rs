//! A deal's next steps HTTP surface (alo CRM, ADR 0035, wave B2) — on top of
//! [`alo_store::crm_next_steps`], which owns no table of its own.
//!
//! **A next step is a Task**, created in the tasks module with ADR 0021's source
//! link (`sourceKind: "deal"`, `sourceId: <deal id>`) and read back through it
//! (`docs/design/crm.md`, "Activities and next steps"). Two consequences shape
//! this module:
//!
//! - **The answer is a task, in the tasks module's own JSON shape**
//!   ([`crate::tasks::task_json`]) — reused, not re-spelled, so a client renders
//!   one kind of card whether it met the task in Tasks, in Mail or in a deal
//!   drawer, and so a field added to a task never has to be added twice.
//! - **A next step lands where the person who will do it keeps their work** —
//!   the project they name, defaulting to their own personal project. A deal is
//!   tenant-wide and a personal project is not, so the list a colleague reads
//!   holds the next steps that are theirs to see: the ones on team projects,
//!   plus anything assigned to them. That asymmetry is deliberate and is the
//!   same one a linked conversation has.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::{CrmDealId, NewTask, ProjectId, TaskId};

use crate::billing::{map_store_err, parse_body, parse_rfc3339};
use crate::error::Problem;
use crate::state::{Account, AppState, authenticate};
use crate::tasks::{resolve_assignee, resolve_emails, task_json};

/// The body of the write route: what the next step is, when it is due, and
/// where it goes.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NextStepBody {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    description: Option<String>,
    /// RFC 3339. A task's due date is an instant, as it is everywhere else in
    /// Tasks — unlike a deal's `expectedClose`, which is a day.
    #[serde(default)]
    due_at: Option<String>,
    /// `none` | `low` | `medium` | `high`; the tasks module's own vocabulary.
    #[serde(default)]
    priority: Option<String>,
    /// Where to file it. Absent means the caller's own personal project.
    #[serde(default)]
    project_id: Option<String>,
    /// Who will do it, as an email address or a user id of this tenant.
    #[serde(default)]
    assignee: Option<String>,
    /// The board column it starts in; absent means the tasks module's default.
    #[serde(default)]
    status: Option<String>,
}

/// Trims a stated value and treats a blank one as absent — a form whose field
/// was cleared sends an empty string.
fn stated(raw: Option<&str>) -> Option<&str> {
    raw.map(str::trim).filter(|v| !v.is_empty())
}

impl NextStepBody {
    /// Turns the request into the task to create, resolving the assignee
    /// through this tenant's own directory.
    ///
    /// The source link is **not** read from the body: the store overwrites it
    /// with the deal in the path, so a "next step" always really points at the
    /// deal it was raised from.
    async fn into_task(
        self,
        state: &AppState,
        account: &Account,
    ) -> Result<(NewTask, Option<ProjectId>), Problem> {
        let title = stated(self.title.as_deref())
            .ok_or_else(|| Problem::with(StatusCode::UNPROCESSABLE_ENTITY, "title is required"))?
            .to_owned();
        let due_at = match stated(self.due_at.as_deref()) {
            None => None,
            Some(raw) => Some(parse_rfc3339(raw).ok_or_else(|| {
                Problem::with(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "dueAt must be an RFC 3339 timestamp",
                )
            })?),
        };
        let assignee = resolve_assignee(state, account, &self.assignee).await;
        let project = stated(self.project_id.as_deref()).map(|p| ProjectId::new(p.to_owned()));
        Ok((
            NewTask {
                title,
                description: stated(self.description.as_deref()).map(str::to_owned),
                status: stated(self.status.as_deref()).map(str::to_owned),
                assignee,
                due_at,
                priority: stated(self.priority.as_deref()).map(str::to_owned),
                // A person deciding, never the agent suggesting: the
                // propose-then-approve path (ADR 0023) stays in Tasks.
                state: None,
                source_kind: None,
                source_id: None,
            },
            project,
        ))
    }
}

/// `GET /crm/deals/{id}/next-steps` → `{"nextSteps":[…]}` — the deal's linked
/// tasks as **this** reader may see them, unfinished first and then by due
/// date.
///
/// A deal that is not this tenant's is the same `404` an id that never existed
/// gets, never an empty list.
pub async fn list_next_steps(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let tasks = account
        .acc
        .crm_deal_next_steps(&CrmDealId::new(id))
        .await
        .map_err(map_store_err)?;
    let ts = state.store.for_tenant(account.tenant.clone());
    let emails = resolve_emails(&ts, &tasks).await;
    Ok(Json(json!({
        "nextSteps": tasks.iter().map(|t| task_json(t, &emails)).collect::<Vec<_>>(),
    })))
}

/// `POST /crm/deals/{id}/next-steps` `{title, dueAt?, projectId?, assignee?, …}`
/// → `{"nextStep":{…}}` — agree what happens next, as a real task.
///
/// It is created on the project the caller names, or on their own personal
/// project when they name none, and it carries the source link back to the deal
/// so both records can see each other. A project the caller cannot see is the
/// same `404` a deal of another tenant gets.
pub async fn add_next_step(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: NextStepBody = parse_body(&body)?;
    let (new, project) = req.into_task(&state, &account).await?;
    let deal = CrmDealId::new(id);
    let created = account
        .acc
        .create_crm_deal_next_step(&deal, project.as_ref(), &new)
        .await
        .map_err(map_store_err)?;
    let task = one_task(&account, &created).await?;
    let ts = state.store.for_tenant(account.tenant.clone());
    let emails = resolve_emails(&ts, std::slice::from_ref(&task)).await;
    Ok(Json(json!({ "nextStep": task_json(&task, &emails) })))
}

/// Reads back the task just written, so the answer is the stored record rather
/// than an echo of the request — the contract every CRM and billing write
/// holds.
async fn one_task(account: &Account, id: &TaskId) -> Result<alo_store::Task, Problem> {
    account
        .acc
        .task(id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such task"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(json: Value) -> NextStepBody {
        serde_json::from_value(json).unwrap_or_else(|e| panic!("body rejected: {e}"))
    }

    #[test]
    fn a_next_step_states_what_it_is() {
        // The title is the one field a next step cannot do without; everything
        // else has a default the tasks module already owns.
        for absent in [json!({}), json!({ "title": "" }), json!({ "title": "  " })] {
            assert!(
                stated(body(absent.clone()).title.as_deref()).is_none(),
                "{absent}"
            );
        }
        assert_eq!(
            stated(body(json!({ "title": "  Call Ada  " })).title.as_deref()),
            Some("Call Ada")
        );
    }

    #[test]
    fn a_due_date_is_an_instant_or_a_refusal() {
        assert!(parse_rfc3339("2026-08-14T09:00:00Z").is_some());
        for bad in ["2026-08-14", "next tuesday", "14/08/2026"] {
            assert!(parse_rfc3339(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn a_source_link_in_the_body_is_ignored_not_obeyed() {
        // The deal in the path is the source, always: the body cannot point a
        // "next step" at somebody else's record. Unknown fields are ignored
        // exactly as they are on every other CRM write.
        let req = body(json!({
            "title": "Call Ada",
            "sourceKind": "email",
            "sourceId": "msg_1",
            "state": "proposed",
        }));
        assert_eq!(stated(req.title.as_deref()), Some("Call Ada"));
    }

    #[test]
    fn a_blank_project_means_the_callers_own() {
        for blank in [
            json!({}),
            json!({ "projectId": "" }),
            json!({ "projectId": " " }),
        ] {
            assert!(stated(body(blank).project_id.as_deref()).is_none());
        }
        assert_eq!(
            stated(body(json!({ "projectId": "proj_1" })).project_id.as_deref()),
            Some("proj_1")
        );
    }
}
