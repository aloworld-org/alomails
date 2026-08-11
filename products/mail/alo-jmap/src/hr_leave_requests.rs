//! Asking for time off and deciding it, over HTTP (alo HR, ADR 0035, wave
//! B6.03b) — over [`alo_store::hr_leave_requests`], with
//! [`crate::hr_leave_door`] deciding who may.
//!
//! Six routes, one record. The division of labour with the store is worth
//! stating once, because it is what keeps both halves reviewable:
//!
//! - **the store** refuses on the record — a decided request is not decided
//!   twice, an approved one is not edited, days another live request covers are
//!   not booked twice, an overdraft is not approved;
//! - **this file** refuses on the caller — whose leave it is, who manages them,
//!   who holds the HR door, and the rule that nobody approves their own.
//!
//! Neither half can be bypassed by the other, and a route that forgot its door
//! would still be refused by the record's own rules for anything that is not a
//! read.
//!
//! **Nothing here is logged.** A leave note is "hospital appointment on the
//! Tuesday"; a decision note is a manager's sentence about a person. Both are
//! personal data of the sharpest kind, and the audit trail (B2.13, automatic
//! from the route) records *that* a decision happened and by whom, never what
//! was written in it.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use time::Date;

use alo_store::hr_leave_requests::{LeaveRequestQuery, LeaveStatus, NewLeaveRequest};
use alo_store::{HrLeavePolicyId, HrLeaveRequestId, LeaveRequest, TenantStore};

use crate::billing::{iso, iso_date, map_store_err, parse_body, parse_iso_date};
use crate::billing_document::today;
use crate::error::Problem;
use crate::hr_leave_door::{LeaveDoor, subject_of_write};
use crate::state::{AppState, authenticate};

/// One request as JSON.
///
/// `costMinutes` and `workingDays` are **the server's fold**, never the
/// client's arithmetic — the same rule money has everywhere in this suite. A
/// screen shows "5 working days" beside the dates and the minutes behind it on
/// hover, because a number nobody can reproduce is a number people distrust.
pub(crate) fn request_json(request: &LeaveRequest) -> Value {
    json!({
        "id": request.id.as_str(),
        "employeeId": request.employee_id.as_str(),
        "employeeName": request.employee_name,
        "policyId": request.policy_id.as_str(),
        "policyName": request.policy_name,
        "fromDay": iso_date(request.from_day),
        "toDay": iso_date(request.to_day),
        "status": request.status.as_str(),
        "note": request.note,
        "costMinutes": request.cost.minutes,
        "workingDays": request.cost.working_days,
        "decidedBy": request.decided_by,
        "decidedAt": request.decided_at.map(iso),
        "decisionNote": request.decision_note,
        "closedAt": request.closed_at.map(iso),
        "createdAt": iso(request.created_at),
        "updatedAt": iso(request.updated_at),
    })
}

/// Loads one request, or the `404` an id from another tenant gets.
async fn load(hr: &TenantStore, id: &HrLeaveRequestId) -> Result<LeaveRequest, Problem> {
    hr.hr_leave_request(id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such leave request"))
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

/// The body of a new request, and of an edit to one.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RequestBody {
    /// Whose leave. Absent means the caller's own; naming somebody else is the
    /// HR door.
    #[serde(default)]
    employee_id: Option<String>,
    /// Which policy. Required on create, ignored on edit — moving an absence to
    /// another policy is a cancel and a new request, so a balance's history
    /// stays readable.
    #[serde(default)]
    policy_id: Option<String>,
    from_day: String,
    to_day: String,
    #[serde(default)]
    note: Option<String>,
}

/// The body of a decision, a withdrawal or a cancellation. A refusal without a
/// reason is legal but unkind, so the field exists on every verb and none
/// requires it.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DecisionBody {
    #[serde(default)]
    note: Option<String>,
}

/// The note a verb carries: an absent field, an empty body and an explicit
/// `null` all mean the same.
fn decision_note(body: &axum::body::Bytes) -> Result<String, Problem> {
    if body.is_empty() {
        return Ok(String::new());
    }
    let req: DecisionBody = parse_body(body)?;
    Ok(req.note.unwrap_or_default())
}

/// Query string of the list route.
#[derive(Deserialize)]
pub struct ListQuery {
    /// `mine` (default), `team` — the people who report to me — or `all`, which
    /// is HR's.
    #[serde(default)]
    scope: Option<String>,
    /// `requested`, `approved`, … Repeatable as a comma-separated list; absent
    /// is every state.
    #[serde(default)]
    status: Option<String>,
    /// Only leave touching this day or later.
    #[serde(default)]
    from: Option<String>,
    /// Only leave touching this day or earlier.
    #[serde(default)]
    to: Option<String>,
}

/// `GET /hr/leave-requests?scope=mine|team|all&status=&from=&to=` →
/// `{"requests":[…], "scope":"…"}`.
///
/// The three scopes are three different questions, and each is answered by
/// naming the exact people it is about — never by reading everybody's and
/// filtering afterwards, which is the shape that leaks the day somebody edits
/// the filter.
///
/// # Errors
/// `401` without a valid bearer token; `403` when somebody who is not HR asks
/// for `all`; `422` on an unknown scope, status or date.
pub async fn list_requests(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ListQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let door = LeaveDoor::resolve(&account).await?;
    let employees = door.scope(q.scope.as_deref())?;
    let mut statuses = Vec::new();
    for word in q.status.as_deref().unwrap_or_default().split(',') {
        if !word.trim().is_empty() {
            statuses.push(LeaveStatus::parse(word).map_err(map_store_err)?);
        }
    }
    let query = LeaveRequestQuery {
        employees,
        statuses,
        from: q
            .from
            .as_deref()
            .map(|raw| stated_day(raw, "from"))
            .transpose()?,
        to: q
            .to
            .as_deref()
            .map(|raw| stated_day(raw, "to"))
            .transpose()?,
    };
    let requests = state
        .store
        .for_tenant(account.tenant.clone())
        .hr_leave_requests(&query)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "requests": requests.iter().map(request_json).collect::<Vec<_>>(),
        "scope": q.scope.unwrap_or_else(|| "mine".to_owned()),
    })))
}

/// `POST /hr/leave-requests` `{policyId, fromDay, toDay, note?, employeeId?}` →
/// `{"request":{…}}` — asking for time off.
///
/// A policy that requires no approval (a sick policy a tenant records rather
/// than decides) lands `approved` with the requester named as the decider; every
/// other policy lands `requested`.
///
/// # Errors
/// `401`; `403` when somebody who is not HR names another employee; `404` when
/// the person or the policy is not this tenant's; `409` on days another live
/// request covers, an archived policy, a person who has left, or an
/// auto-approval that would overdraw a policy; `422` on a range that ends before
/// it starts, reaches outside the employment, or costs nothing.
pub async fn create_request(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let door = LeaveDoor::resolve(&account).await?;
    let req: RequestBody = parse_body(&body)?;
    let hr = state.store.for_tenant(account.tenant.clone());
    let employee = subject_of_write(&hr, &door, req.employee_id.as_deref()).await?;
    let policy = req
        .policy_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| {
            Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                "policyId is required: leave is always on one of the tenant's policies",
            )
        })?;
    let input = NewLeaveRequest {
        employee_id: employee,
        policy_id: HrLeavePolicyId::new(policy.to_owned()),
        from_day: stated_day(&req.from_day, "fromDay")?,
        to_day: stated_day(&req.to_day, "toDay")?,
        note: req.note.unwrap_or_default(),
    };
    let id = hr
        .create_hr_leave_request(&input, &account.user, today())
        .await
        .map_err(map_store_err)?;
    Ok(Json(
        json!({ "request": request_json(&load(&hr, &id).await?) }),
    ))
}

/// `GET /hr/leave-requests/{id}` → `{"request":{…}}` — one request, to the
/// person it is about, their manager, or HR.
///
/// # Errors
/// `401`; `404` when the request is not this tenant's **or** not the caller's to
/// read — the same answer either way, so no refusal is an existence oracle.
pub async fn get_request(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let door = LeaveDoor::resolve(&account).await?;
    let hr = state.store.for_tenant(account.tenant.clone());
    let request = load(&hr, &HrLeaveRequestId::new(id)).await?;
    door.require_read(&request.employee_id)?;
    Ok(Json(json!({ "request": request_json(&request) })))
}

/// `PATCH /hr/leave-requests/{id}` `{fromDay, toDay, note?}` →
/// `{"request":{…}}` — different dates, before anybody has decided.
///
/// **Its owner and nobody else.** A manager who wants different dates rejects
/// with a note; editing somebody's request into a different request they did not
/// make is not a thing this module does, and that includes HR
/// (`docs/design/hr.md`, "The request, and its state machine").
///
/// # Errors
/// `401`; `404` when the request is not this tenant's or not the caller's own;
/// `409` when it has been decided or the new dates collide; `422` on a range the
/// caller can fix.
pub async fn update_request(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let door = LeaveDoor::resolve(&account).await?;
    let hr = state.store.for_tenant(account.tenant.clone());
    let id = HrLeaveRequestId::new(id);
    let stored = load(&hr, &id).await?;
    require_own(&door, &stored)?;
    let req: RequestBody = parse_body(&body)?;
    hr.update_hr_leave_request(
        &id,
        stated_day(&req.from_day, "fromDay")?,
        stated_day(&req.to_day, "toDay")?,
        req.note.as_deref().unwrap_or(&stored.note),
    )
    .await
    .map_err(map_store_err)?;
    Ok(Json(
        json!({ "request": request_json(&load(&hr, &id).await?) }),
    ))
}

/// `POST /hr/leave-requests/{id}/withdraw` → `{"request":{…}}` — taking back a
/// request nobody has decided. **Its owner and nobody else.**
///
/// # Errors
/// `401`; `404` as for the edit; `409` when it has already been decided.
pub async fn withdraw_request(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let door = LeaveDoor::resolve(&account).await?;
    let hr = state.store.for_tenant(account.tenant.clone());
    let id = HrLeaveRequestId::new(id);
    let stored = load(&hr, &id).await?;
    require_own(&door, &stored)?;
    hr.withdraw_hr_leave_request(&id, &account.user)
        .await
        .map_err(map_store_err)?;
    Ok(Json(
        json!({ "request": request_json(&load(&hr, &id).await?) }),
    ))
}

/// `POST /hr/leave-requests/{id}/approve` `{note?}` → `{"request":{…}}`.
///
/// # Errors
/// `401`; `404` when the request is not this tenant's or not the caller's to
/// decide; `409` when it is already decided, when the caller is the person
/// taking the leave (and not an admin), or when the approval would overdraw a
/// policy that does not allow it.
pub async fn approve_request(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    decide(state, headers, id, body, true).await
}

/// `POST /hr/leave-requests/{id}/reject` `{note?}` → `{"request":{…}}`.
///
/// A rejection never depends on the balance: saying no to leave somebody cannot
/// afford must always be possible.
///
/// # Errors
/// As [`approve_request`], minus the overdraft.
pub async fn reject_request(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    decide(state, headers, id, body, false).await
}

/// The shared body of the two decisions.
async fn decide(
    state: AppState,
    headers: HeaderMap,
    id: String,
    body: axum::body::Bytes,
    approve: bool,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let door = LeaveDoor::resolve(&account).await?;
    let hr = state.store.for_tenant(account.tenant.clone());
    let id = HrLeaveRequestId::new(id);
    let stored = load(&hr, &id).await?;
    door.require_decide(&stored.employee_id)?;
    let note = decision_note(&body)?;
    hr.decide_hr_leave_request(&id, approve, &account.user, &note, today())
        .await
        .map_err(map_store_err)?;
    Ok(Json(
        json!({ "request": request_json(&load(&hr, &id).await?) }),
    ))
}

/// `POST /hr/leave-requests/{id}/cancel` → `{"request":{…}}` — approved leave
/// that has not started, given back.
///
/// The person taking it, their manager, or HR. Leave already begun is refused
/// (`409`): the fact that somebody was absent last Tuesday is corrected by HR
/// with a reason, never erased by a button.
///
/// # Errors
/// `401`; `404` when the request is not this tenant's or not the caller's;
/// `409` when it is not approved, or has already started.
pub async fn cancel_request(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let door = LeaveDoor::resolve(&account).await?;
    let hr = state.store.for_tenant(account.tenant.clone());
    let id = HrLeaveRequestId::new(id);
    let stored = load(&hr, &id).await?;
    // Cancelling is the one verb all three relationships share: it gives the
    // balance back and takes nothing away from anybody.
    door.require_read(&stored.employee_id)?;
    hr.cancel_hr_leave_request(&id, &account.user, today())
        .await
        .map_err(map_store_err)?;
    Ok(Json(
        json!({ "request": request_json(&load(&hr, &id).await?) }),
    ))
}

/// The owner rule of the two verbs that belong to the person who asked.
fn require_own(door: &LeaveDoor, request: &LeaveRequest) -> Result<(), Problem> {
    if door.is_me(&request.employee_id) {
        return Ok(());
    }
    // A manager or HR may read it, so telling them it exists is no disclosure —
    // but the verb is not theirs, and saying so is more useful than a 404 they
    // can disprove by reading the record they just listed.
    if door.may_read(&request.employee_id) {
        return Err(Problem::with(
            StatusCode::FORBIDDEN,
            "only the person who asked may change or withdraw their request; reject it with a \
             note instead",
        ));
    }
    Err(Problem::with(
        StatusCode::NOT_FOUND,
        "no such leave request",
    ))
}
