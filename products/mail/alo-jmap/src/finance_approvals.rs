//! The approver's side of an expense claim (alo Finance, ADR 0035, wave
//! B4.05b) — the queue of what is waiting, and the three decisions that empty
//! it — over [`alo_store::fin_expenses`]' tenant door.
//!
//! [`crate::finance_expenses`] is the claimant's own record of what they spent
//! and has no `userId` anywhere in it. This is deliberately a different file
//! because every route here is a **cross-user** surface that exists only for
//! approvers: putting the module's one privileged read in the file whose whole
//! premise is that there isn't one is how such a read stops being noticed.
//!
//! Four decisions, each taken in `docs/design/finance.md` rather than here:
//!
//! - **`require_finance` gates every route** — a tenant admin, or the
//!   accountant role B4.12 added. Deriving an approver from something smaller
//!   — a project owner, a team lead — was rejected for hours in
//!   `docs/design/projects.md` and is rejected here for the same reason: a
//!   claim is a *person's*, not a project's. The accountant was added because
//!   deciding what the company reimburses is bookkeeping, which is the whole
//!   of what that role is for. When B6.02 brings the org chart the check
//!   widens additively again and nothing already decided moves.
//! - **An admin may decide their own claim.** A one-person tenant has nobody
//!   else, and the audit entry records who it was.
//! - **The queue is the narrowest cross-user read the module has**: the claims
//!   awaiting a decision, who made each, and the category each books to.
//!   Nothing about anybody's other claims, and no totals per person — an
//!   approvals inbox is not a spending report about employees.
//! - **Refusals are conflicts, not silence.** Deciding a claim nobody handed
//!   in, or reimbursing one the company's own card paid, answers `409` saying
//!   what the claim actually is. A claim that is not this tenant's is `404` and
//!   never a conflict, so no refusal is an existence oracle.
//!
//! A decision note can name a person or an occasion, so nothing here logs one.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::{ExpenseDecision, FinExpenseId, PendingExpense};

use crate::billing::{map_store_err, parse_body};
use crate::error::Problem;
use crate::finance_expenses::{expense_json, stated_day};
use crate::state::{AppState, authenticate};

/// One waiting claim as the inbox shows it: the claim, the person who made it,
/// and the word that says where it books. Crate-visible so the Finance agent's
/// `expenses_awaiting` (`crate::finance_intents`) shows the queue in exactly
/// these rows.
pub(crate) fn pending_json(pending: &PendingExpense) -> Value {
    let mut value = expense_json(&pending.expense);
    if let Some(object) = value.as_object_mut() {
        object.insert("userId".to_owned(), json!(pending.expense.user_id.as_str()));
        object.insert("userEmail".to_owned(), json!(pending.user_email));
        object.insert("categoryName".to_owned(), json!(pending.category_name));
    }
    value
}

/// The body of a decision. A refusal without a reason is legal but unkind, so
/// the field exists on both verbs and neither requires it.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DecisionBody {
    #[serde(default)]
    note: Option<String>,
}

/// The body of the reimbursement: the day the money actually moved.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReimburseBody {
    #[serde(default)]
    reimbursed_on: Option<String>,
}

/// The note a decision carries. Bounded by the store; blanked to empty here so
/// an absent field, an empty body and an explicit `null` all mean the same.
fn decision_note(body: &axum::body::Bytes) -> Result<String, Problem> {
    if body.is_empty() {
        return Ok(String::new());
    }
    let req: DecisionBody = parse_body(body)?;
    Ok(req.note.unwrap_or_default())
}

/// `GET /finance/expenses/pending` → `{"expenses": [ … ]}` — **admin or
/// accountant**: every claim of this tenant awaiting a decision, oldest
/// purchase first.
///
/// # Errors
/// `401` without a valid bearer token; `403` when the caller is neither a
/// tenant admin nor an accountant.
pub async fn list_pending_expenses(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_finance()?;
    let waiting = state
        .store
        .for_tenant(account.tenant.clone())
        .pending_expenses()
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "expenses": waiting.iter().map(pending_json).collect::<Vec<_>>(),
    })))
}

/// `GET /finance/expenses/reimbursable` → `{"expenses": [ … ]}` — **admin or
/// accountant**: every claim this tenant has approved and still owes the
/// employee for, oldest decision first.
///
/// Its own route rather than a status filter on the queue above, because it is
/// not the same list read differently: an approved claim a company card paid is
/// approved and is *not* reimbursable, and a payer's queue that listed it would
/// be a queue with a line in it nobody can clear (the store's
/// `reimbursable_expenses`, and the `409` [`reimburse_expense`] answers).
///
/// # Errors
/// `401` without a valid bearer token; `403` when the caller is neither a
/// tenant admin nor an accountant.
pub async fn list_reimbursable_expenses(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_finance()?;
    let owed = state
        .store
        .for_tenant(account.tenant.clone())
        .reimbursable_expenses()
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "expenses": owed.iter().map(pending_json).collect::<Vec<_>>(),
    })))
}

/// `POST /finance/expenses/{id}/approve` `{note?}` → `{"expense": {…}}` —
/// **admin or accountant**: the cost is the company's, and (when the
/// employee's own money paid) so is the debt to them.
///
/// # Errors
/// `401`/`403` as above; `404` when the claim is not this tenant's; `409` when
/// it is not awaiting a decision; `422` when the note is too long.
pub async fn approve_expense(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    decide(state, headers, id, ExpenseDecision::Approve, &body).await
}

/// `POST /finance/expenses/{id}/reject` `{note?}` → `{"expense": {…}}` —
/// **admin or accountant**: the claim goes back to its claimant, editable, so
/// they can correct it and hand it in again.
///
/// # Errors
/// `401`/`403` as above; `404` when the claim is not this tenant's; `409` when
/// it is not awaiting a decision; `422` when the note is too long.
pub async fn reject_expense(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    decide(state, headers, id, ExpenseDecision::Reject, &body).await
}

/// The one implementation behind approve and reject. Two routes because they
/// are two acts an approver takes and two entries in the audit log; one
/// function because the only thing that differs is the state the claim lands in.
async fn decide(
    state: AppState,
    headers: HeaderMap,
    id: String,
    decision: ExpenseDecision,
    body: &axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_finance()?;
    let note = decision_note(body)?;
    let claim = state
        .store
        .for_tenant(account.tenant.clone())
        .decide_expense(&FinExpenseId::new(id), decision, &account.user, &note)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "expense": expense_json(&claim) })))
}

/// `POST /finance/expenses/{id}/reimburse` `{reimbursedOn}` →
/// `{"expense": {…}}` — **admin or accountant**: the money has been paid back.
///
/// The day is required and is the payer's, never the server's clock: it is the
/// date the reimbursement books on, and a day chosen by whichever zone a
/// container runs in is a posting in the wrong period.
///
/// Only an **approved** claim, and only one the **employee's own money** paid: a
/// company card left nobody owed anything, and recording a repayment against one
/// would book money out of the bank twice. Both refusals are `409` naming which
/// rule stopped it.
///
/// # Errors
/// `401`/`403` as above; `404` when the claim is not this tenant's; `409` when
/// it is not approved or nobody is owed anything on it; `422` when
/// `reimbursedOn` is missing or is not a day.
pub async fn reimburse_expense(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_finance()?;
    let req: ReimburseBody = parse_body(&body)?;
    let stated = req
        .reimbursed_on
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                "reimbursedOn is required: the day the money moved is the day it books on",
            )
        })?;
    let claim = state
        .store
        .for_tenant(account.tenant.clone())
        .reimburse_expense(&FinExpenseId::new(id), stated_day("reimbursedOn", stated)?)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "expense": expense_json(&claim) })))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn a_decision_may_arrive_with_no_body_no_note_or_a_note() {
        assert_eq!(decision_note(&axum::body::Bytes::new()).unwrap(), "");
        let empty = axum::body::Bytes::from_static(b"{}");
        assert_eq!(decision_note(&empty).unwrap(), "");
        let stated = axum::body::Bytes::from_static(br#"{"note":"the receipt is missing"}"#);
        assert_eq!(decision_note(&stated).unwrap(), "the receipt is missing");
        let malformed = axum::body::Bytes::from_static(b"{");
        assert_eq!(
            decision_note(&malformed).expect_err("refused").status,
            StatusCode::BAD_REQUEST
        );
    }

    #[test]
    fn the_inbox_adds_the_person_to_the_claim_and_nothing_else() {
        use alo_store::{Expense, ExpenseMethod, ExpenseStatus, UserId};
        use time::{Date, Month, OffsetDateTime};

        let now = OffsetDateTime::UNIX_EPOCH;
        let claim = Expense {
            id: FinExpenseId::new("exp-1".to_owned()),
            user_id: UserId::new("usr-1".to_owned()),
            spent_on: Date::from_calendar_date(2026, Month::March, 14).unwrap(),
            category_id: None,
            merchant: "Bahn".to_owned(),
            description: String::new(),
            gross_cents: 11_900,
            vat_cents: 1900,
            vat_rate_bp: Some(1900),
            currency: "EUR".to_owned(),
            method: ExpenseMethod::Personal,
            project_id: None,
            receipt_node_id: None,
            status: ExpenseStatus::Submitted,
            submitted_at: Some(now),
            decided_by: None,
            decided_at: None,
            decision_note: String::new(),
            reimbursed_on: None,
            proposed_category_id: None,
            proposed_at: None,
            proposed_reason: String::new(),
            proposal_declined_at: None,
            created_at: now,
            updated_at: now,
        };
        let value = pending_json(&PendingExpense {
            expense: claim,
            user_email: "traveller@example.test".to_owned(),
            category_name: Some("Reisekosten".to_owned()),
        });
        assert_eq!(value["userEmail"], json!("traveller@example.test"));
        assert_eq!(value["userId"], json!("usr-1"));
        assert_eq!(value["categoryName"], json!("Reisekosten"));
        // The claim itself is rendered by exactly the same function the
        // claimant's own routes use, so the two can never drift apart.
        assert_eq!(value["netCents"], json!(10_000));
        assert_eq!(value["status"], json!("submitted"));
        assert_eq!(value["editable"], json!(false));
    }
}
