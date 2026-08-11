//! The org chart and the own door (alo HR, ADR 0035, wave B6.02b) — over
//! [`alo_store::hr_org`].
//!
//! Two routes that look unrelated and are the same decision read from both
//! ends: **what a person may know about the company, and what the company must
//! show a person about themselves.**
//!
//! - `GET /hr/org` is the one HR read every member gets. A company where you
//!   cannot find out who your colleague's manager is has an org chart in a
//!   filing cabinet, and we are replacing filing cabinets. It carries the
//!   public fields only — and structurally so: the store folds it from
//!   [`alo_store::DirectoryEntry`], a type that has no home address on it to
//!   leak.
//! - `GET /hr/me` is the own door, and it answers with **everything** the
//!   employer holds about the caller — the private fields included. An answer
//!   that omitted the address we keep would be a worse answer than none; this
//!   is the subject-access read as much as it is a screen.
//!
//! Neither route logs a name, an address or a pay figure.

use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use serde_json::{Value, json};

use alo_store::{HrEmployeeId, OrgNode};

use crate::billing::map_store_err;
use crate::error::Problem;
use crate::hr_employees::{employee_json, employment_json};
use crate::state::{AppState, authenticate};

/// One person in the chart and the people beneath them.
///
/// Recursion is bounded by the data: the store refuses a reporting line that
/// would close a cycle, and its fold terminates whatever the rows say
/// (`alo_store::hr_org::fold_org_chart`), so this walk cannot run away even on
/// data repaired by hand.
fn node_json(node: &OrgNode) -> Value {
    json!({
        "id": node.id.as_str(),
        "name": node.name,
        "jobTitle": node.job_title,
        "team": node.team,
        "managerId": node.manager_id.as_ref().map(HrEmployeeId::as_str),
        "reports": node.reports.iter().map(node_json).collect::<Vec<_>>(),
    })
}

/// `GET /hr/org` → `{"chart":[…]}` — the reporting tree of this tenant's active
/// people, roots first, each with their reports beneath them.
///
/// **Every member gets it.** Somebody whose manager has left the directory is a
/// root rather than an absence, so the chart never silently drops a branch.
///
/// # Errors
/// `401` without a valid bearer token.
pub async fn org_chart(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let chart = account.acc.hr_org_chart().await.map_err(map_store_err)?;
    Ok(Json(json!({
        "chart": chart.iter().map(node_json).collect::<Vec<_>>(),
    })))
}

/// `GET /hr/me` → `{"employee":{…}|null, "employments":[…], "isHr":bool}` — the
/// caller's own record and the terms they are employed on.
///
/// `employee` is `null` for somebody with a login and no employee record — a
/// contractor with a mailbox, an admin who is not on the payroll — which is an
/// ordinary answer and not an error: the client shows the rest of the workspace
/// and no HR screens.
///
/// There is no argument by which this route could ask about a colleague: the
/// store statement behind it carries the caller's own user id, so another
/// person's record is unrepresentable rather than refused.
///
/// `isHr` tells a client which doors to draw, and is not itself the gate —
/// every HR route checks the role for itself.
///
/// # Errors
/// `401` without a valid bearer token.
pub async fn me(State(state): State<AppState>, headers: HeaderMap) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let employee = account.acc.my_hr_employee().await.map_err(map_store_err)?;
    let employments = account
        .acc
        .my_hr_employments()
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "employee": employee.as_ref().map(employee_json),
        "employments": employments.iter().map(employment_json).collect::<Vec<_>>(),
        "isHr": account.require_hr().is_ok(),
    })))
}
