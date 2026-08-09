//! The expense-claim HTTP surface (alo Finance, ADR 0035, wave B4.05) — what a
//! person spent, and the two verbs by which it stops being theirs alone — over
//! [`alo_store::fin_expenses`].
//!
//! `/finance` is a **new top-level prefix**: the production Caddyfile needs it
//! added at the next deploy (the standing human action `/billing`, `/crm`,
//! `/audit`, `/insights` and `/projects` already carry), and it joins
//! `API_PATHS` in `web/vite.config.ts` here so a browser call reaches the API
//! instead of the dev SPA — the lesson S1.11, BI1.04 and B3.04 each paid for
//! once (`docs/design/finance.md` § Routes).
//!
//! It shares [`crate::billing`]'s conventions — the account door, `Problem`
//! errors, no validation duplicated from the store, timestamps as RFC 3339 and
//! days as `YYYY-MM-DD` — and adds three of its own, each decided in the design
//! note rather than here:
//!
//! - **Every route here is the caller's own.** There is no `userId` anywhere in
//!   this module: a receipt names a restaurant, a pharmacy, a city on a date,
//!   and the store has no function on this door that takes somebody else's id.
//!   The cross-user reads and the decisions live in
//!   [`crate::finance_approvals`], behind the admin gate, which is why they are
//!   a different file — mixing them would put the module's one privileged read
//!   in the file whose whole premise is that there isn't one.
//! - **The day is the claimant's, in the claimant's zone.** `spentOn` is a
//!   plain day and never derived from the server's clock: a purchase made at
//!   23:40 in Berlin belongs to that day, not to the one a container in UTC
//!   thinks it is, and it is what every VAT period boundary uses.
//! - **Nothing here computes money.** The gross and the VAT are what the
//!   receipt says; `netCents` is the store's subtraction of two stored
//!   integers, and no float appears on this path.
//!
//! Merchant, description and note are personal data — they can name a person,
//! a clinic or an occasion — so nothing in this module logs one.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use time::Date;

use alo_store::{
    DriveNodeId, Expense, ExpenseMethod, ExpenseStatus, FinCategoryId, FinExpenseId, NewExpense,
    ProjectId,
};

use crate::billing::{
    absent_or_null, blank_to_none, iso, iso_date, map_store_err, parse_body, parse_iso_date,
};
use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// The longest period one read of a person's claims may ask for.
///
/// A year of expenses is what an employee or an accountant looks at; past it
/// the caller wants paging, which this route does not offer. Refused rather
/// than truncated: a silently shortened period is a list that is quietly
/// missing a claim.
const MAX_PERIOD_DAYS: i64 = 366;

/// One claim as JSON.
///
/// `netCents` is included although it is derivable, because it is the figure
/// that books to the expense account and the client must never be the thing
/// that computes it. `editable` says whether the claim is still the claimant's
/// own, so a UI can grey out a form instead of discovering the rule by being
/// refused.
///
/// `userId` is deliberately absent: every claim this module answers with is the
/// caller's. The approvals inbox, which genuinely shows people, adds it there.
pub(crate) fn expense_json(e: &Expense) -> Value {
    json!({
        "id": e.id.as_str(),
        "spentOn": iso_date(e.spent_on),
        "categoryId": e.category_id.as_ref().map(FinCategoryId::as_str),
        "merchant": e.merchant,
        "description": e.description,
        "grossCents": e.gross_cents,
        "vatCents": e.vat_cents,
        "netCents": e.net_cents(),
        "vatRateBp": e.vat_rate_bp,
        "currency": e.currency,
        "method": e.method.as_str(),
        "projectId": e.project_id.as_ref().map(ProjectId::as_str),
        "receiptNodeId": e.receipt_node_id.as_ref().map(DriveNodeId::as_str),
        "status": e.status.as_str(),
        "editable": e.is_editable(),
        "owesTheEmployee": e.method.owes_the_employee(),
        "submittedAt": e.submitted_at.map(iso),
        "decidedBy": e.decided_by.as_ref().map(alo_store::UserId::as_str),
        "decidedAt": e.decided_at.map(iso),
        "decisionNote": e.decision_note,
        "reimbursedOn": e.reimbursed_on.map(iso_date),
        "createdAt": iso(e.created_at),
        "updatedAt": iso(e.updated_at),
    })
}

/// The period and the optional status one read of a person's claims asks for.
#[derive(Deserialize)]
pub struct ExpensesQuery {
    #[serde(default)]
    from: Option<String>,
    #[serde(default)]
    to: Option<String>,
    #[serde(default)]
    status: Option<String>,
}

/// The writable shape of a claim.
///
/// The same body serves `POST` (merged onto the three facts a claim cannot be
/// created without) and `PATCH` (merged onto the stored record), so a field can
/// never mean one thing on create and another on edit. The three nullable links
/// and the VAT rate distinguish *absent* from an explicit `null`: without that,
/// a category attached by mistake could never be taken off again.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExpenseBody {
    #[serde(default)]
    spent_on: Option<String>,
    #[serde(default, deserialize_with = "absent_or_null")]
    category_id: Option<Option<String>>,
    #[serde(default)]
    merchant: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    gross_cents: Option<i64>,
    #[serde(default)]
    vat_cents: Option<i64>,
    #[serde(default, deserialize_with = "absent_or_null")]
    vat_rate_bp: Option<Option<i32>>,
    #[serde(default)]
    currency: Option<String>,
    #[serde(default)]
    method: Option<String>,
    #[serde(default, deserialize_with = "absent_or_null")]
    project_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "absent_or_null")]
    receipt_node_id: Option<Option<String>>,
}

impl ExpenseBody {
    /// Merges the stated fields onto `base`, leaving the rest as they were.
    ///
    /// The money rules — the bounds, the VAT never exceeding the gross, a VAT
    /// amount always carrying its rate — are the store's and are not repeated
    /// here. What this owns is the *shape* of what arrived: a day is a day, and
    /// a payment method is one of three words.
    ///
    /// # Errors
    /// `422` when `spentOn` is not exactly `YYYY-MM-DD`, or `method` is not a
    /// word the store knows.
    fn apply(self, base: NewExpense) -> Result<NewExpense, Problem> {
        Ok(NewExpense {
            spent_on: match self.spent_on.as_deref() {
                None => base.spent_on,
                Some(raw) => stated_day("spentOn", raw)?,
            },
            category_id: match self.category_id {
                None => base.category_id,
                Some(stated) => blank_to_none(stated).map(FinCategoryId::new),
            },
            merchant: self.merchant.unwrap_or(base.merchant),
            description: self.description.unwrap_or(base.description),
            gross_cents: self.gross_cents.unwrap_or(base.gross_cents),
            vat_cents: self.vat_cents.unwrap_or(base.vat_cents),
            vat_rate_bp: match self.vat_rate_bp {
                None => base.vat_rate_bp,
                Some(stated) => stated,
            },
            currency: blank_to_none(self.currency).or(base.currency),
            method: match self.method.as_deref() {
                None => base.method,
                Some(raw) => ExpenseMethod::parse(raw).map_err(map_store_err)?,
            },
            project_id: match self.project_id {
                None => base.project_id,
                Some(stated) => blank_to_none(stated).map(ProjectId::new),
            },
            receipt_node_id: match self.receipt_node_id {
                None => base.receipt_node_id,
                Some(stated) => blank_to_none(stated).map(DriveNodeId::new),
            },
        })
    }
}

/// The stored claim as a writable record, so a `PATCH` states only what changes.
fn editable(claim: &Expense) -> NewExpense {
    NewExpense {
        spent_on: claim.spent_on,
        category_id: claim.category_id.clone(),
        merchant: claim.merchant.clone(),
        description: claim.description.clone(),
        gross_cents: claim.gross_cents,
        vat_cents: claim.vat_cents,
        vat_rate_bp: claim.vat_rate_bp,
        currency: Some(claim.currency.clone()),
        method: claim.method,
        project_id: claim.project_id.clone(),
        receipt_node_id: claim.receipt_node_id.clone(),
    }
}

/// Reads a day that was stated, refusing anything that is not exactly a day.
///
/// Never a silent default: a claim dated by a fallback nobody asked for lands
/// in the wrong VAT period, which is a correction somebody has to file.
pub(crate) fn stated_day(name: &str, raw: &str) -> Result<Date, Problem> {
    parse_iso_date(raw.trim()).ok_or_else(|| {
        Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("{name} must be a day of the form YYYY-MM-DD"),
        )
    })
}

/// Reads a required day, naming it in the refusal.
fn required_day(name: &str, raw: Option<&str>) -> Result<Date, Problem> {
    let stated = raw
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("{name} is required"),
            )
        })?;
    stated_day(name, stated)
}

/// The two ends of a read of somebody's own finance records, and the ceiling
/// only the route layer owns.
///
/// Shared with [`crate::finance_mileage`]: a journey and the claim it became are
/// read over the same period by the same person, and two different answers to
/// "how long a period may I ask for" would be a list that ends where its
/// neighbour does not.
pub(crate) fn period_bounds(from: Option<&str>, to: Option<&str>) -> Result<(Date, Date), Problem> {
    let from = required_day("from", from)?;
    let to = required_day("to", to)?;
    // The store owns "must not end before it starts"; this owns the ceiling.
    if (to - from).whole_days() >= MAX_PERIOD_DAYS {
        return Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("the period must be shorter than {MAX_PERIOD_DAYS} days"),
        ));
    }
    Ok((from, to))
}

/// The two ends of a claims read.
fn period(query: &ExpensesQuery) -> Result<(Date, Date), Problem> {
    period_bounds(query.from.as_deref(), query.to.as_deref())
}

/// The optional status filter. Absent means every status; a word the store does
/// not know is a `422` naming the five it does, rather than an empty list that
/// looks like "you have no claims".
fn status_filter(raw: Option<&str>) -> Result<Option<ExpenseStatus>, Problem> {
    match raw.map(str::trim).filter(|value| !value.is_empty()) {
        None => Ok(None),
        Some(stated) => ExpenseStatus::parse(stated)
            .map(Some)
            .map_err(map_store_err),
    }
}

/// `GET /finance/expenses?from&to&status` → `{"expenses": [ … ]}` — the
/// **caller's own** claims in a period, newest purchase first.
///
/// # Errors
/// `401` without a valid bearer token; `422` when an end of the period is
/// missing or malformed, the period ends before it starts or spans more than a
/// year, or the status filter is not a word this build knows.
pub async fn list_expenses(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ExpensesQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let (from, to) = period(&query)?;
    let status = status_filter(query.status.as_deref())?;
    let claims = account
        .acc
        .expenses(from, to, status)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "expenses": claims.iter().map(expense_json).collect::<Vec<_>>(),
    })))
}

/// `POST /finance/expenses` `{spentOn, grossCents, method, …}` →
/// `{"expense": {…}}` — record a claim of the caller's own.
///
/// It starts as a draft: nothing is in anybody's queue until the claimant hands
/// it in. Three facts are required because a claim without them is not a claim
/// — the day the money left, what the receipt totals, and whose money paid.
///
/// # Errors
/// `401` without a valid bearer token; `422` when one of the three is missing
/// or malformed, or a store rule refuses the amounts; `404` when the category,
/// the project or the receipt is not one the caller can reach — existence is
/// never disclosed.
pub async fn create_expense(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: ExpenseBody = parse_body(&body)?;
    let spent_on = required_day("spentOn", req.spent_on.as_deref())?;
    let gross_cents = req.gross_cents.ok_or_else(|| {
        Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "grossCents is required: a claim is an amount somebody spent",
        )
    })?;
    let method = match req
        .method
        .as_deref()
        .map(str::trim)
        .filter(|m| !m.is_empty())
    {
        Some(raw) => ExpenseMethod::parse(raw).map_err(map_store_err)?,
        None => {
            return Err(Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                "method is required: whose money paid decides what the approval books",
            ));
        }
    };
    let input = req.apply(NewExpense::spent(spent_on, gross_cents, method))?;
    let claim = account
        .acc
        .log_expense(&input)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "expense": expense_json(&claim) })))
}

/// `GET /finance/expenses/{id}` → `{"expense": {…}}` — one of the caller's own.
///
/// A colleague's claim reads exactly like another tenant's and like one that
/// never existed: `404`. Not a `403`, which would confirm that somebody claimed
/// something that day.
///
/// # Errors
/// `401` without a valid bearer token; `404` when the claim is not the
/// caller's own.
pub async fn get_expense(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let claim = account
        .acc
        .expense(&FinExpenseId::new(id))
        .await
        .map_err(map_store_err)?
        .ok_or_else(Problem::not_found)?;
    Ok(Json(json!({ "expense": expense_json(&claim) })))
}

/// `PATCH /finance/expenses/{id}` `{…}` → `{"expense": {…}}` — correct a claim
/// that is still the caller's own.
///
/// A sparse patch merged onto the stored record: a field left out keeps its
/// value, and an explicit `null` clears one of the three links or the VAT rate.
/// A claim somebody is deciding is frozen (`409`), and withdrawing it is the
/// way back.
///
/// # Errors
/// `401` without a valid bearer token; `404` when the claim, or a thing it
/// points at, is not one the caller can reach; `409` when it has been handed
/// in; `422` when a field breaks its rule.
pub async fn update_expense(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: ExpenseBody = parse_body(&body)?;
    let id = FinExpenseId::new(id);
    let stored = account
        .acc
        .expense(&id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(Problem::not_found)?;
    let input = req.apply(editable(&stored))?;
    let claim = account
        .acc
        .edit_expense(&id, &input)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "expense": expense_json(&claim) })))
}

/// `DELETE /finance/expenses/{id}` → `204` — remove a claim nobody has acted on.
///
/// A draft is the claimant's alone, and a rejected claim is one the company has
/// declined to pay — refusing to remove that too would leave a refused claim
/// stuck in somebody's list forever with no verb that clears it. Everything else
/// is a document in a queue or in the books (`409`).
///
/// # Errors
/// `401` without a valid bearer token; `404` when the claim is not the caller's
/// own; `409` when it has been handed in.
pub async fn delete_expense(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .delete_expense(&FinExpenseId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /finance/expenses/{id}/submit` → `{"expense": {…}}` — hand the claim
/// in for a decision, **freezing it**.
///
/// Handing a rejected claim in again clears the old decision: a refusal that no
/// longer stands must not still be displayed on the record, and the history of
/// it is in the audit log.
///
/// # Errors
/// `401` without a valid bearer token; `404` when the claim is not the caller's
/// own; `409` when it is already waiting, approved or paid back, naming what it
/// is.
pub async fn submit_expense(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let claim = account
        .acc
        .submit_expense(&FinExpenseId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "expense": expense_json(&claim) })))
}

/// `POST /finance/expenses/{id}/withdraw` → `{"expense": {…}}` — take the claim
/// back out of the queue, so it can be corrected.
///
/// Only one nobody has decided. An approved claim is not the claimant's to
/// unmake: the company owes money on it.
///
/// # Errors
/// `401` without a valid bearer token; `404` when the claim is not the caller's
/// own; `409` when it is not waiting for a decision, naming what it is.
pub async fn withdraw_expense(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let claim = account
        .acc
        .withdraw_expense(&FinExpenseId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "expense": expense_json(&claim) })))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn detail(problem: &Problem) -> String {
        problem.detail.clone().unwrap_or_default()
    }

    fn body(json: &str) -> ExpenseBody {
        serde_json::from_str(json).expect("a body this test wrote")
    }

    fn base() -> NewExpense {
        NewExpense {
            category_id: Some(FinCategoryId::new("cat-1".to_owned())),
            merchant: "Bahn".to_owned(),
            description: "Berlin → München".to_owned(),
            vat_cents: 1900,
            vat_rate_bp: Some(1900),
            currency: Some("EUR".to_owned()),
            project_id: Some(ProjectId::new("prj-1".to_owned())),
            receipt_node_id: Some(DriveNodeId::new("node-1".to_owned())),
            ..NewExpense::spent(
                Date::from_calendar_date(2026, time::Month::March, 14).unwrap(),
                11_900,
                ExpenseMethod::Personal,
            )
        }
    }

    fn query(from: &str, to: &str, status: Option<&str>) -> ExpensesQuery {
        ExpensesQuery {
            from: Some(from.to_owned()),
            to: Some(to.to_owned()),
            status: status.map(str::to_owned),
        }
    }

    #[test]
    fn an_empty_patch_changes_nothing_at_all() {
        let merged = body("{}").apply(base()).expect("an empty patch is legal");
        let before = base();
        assert_eq!(merged.spent_on, before.spent_on);
        assert_eq!(merged.gross_cents, before.gross_cents);
        assert_eq!(merged.vat_cents, before.vat_cents);
        assert_eq!(merged.vat_rate_bp, before.vat_rate_bp);
        assert_eq!(merged.merchant, before.merchant);
        assert_eq!(merged.description, before.description);
        assert_eq!(merged.currency, before.currency);
        assert_eq!(merged.method, before.method);
        assert_eq!(merged.category_id, before.category_id);
        assert_eq!(merged.project_id, before.project_id);
        assert_eq!(merged.receipt_node_id, before.receipt_node_id);
    }

    #[test]
    fn an_explicit_null_clears_a_link_and_an_absent_field_does_not() {
        let cleared = body(r#"{"categoryId":null,"projectId":null,"receiptNodeId":null}"#)
            .apply(base())
            .expect("clearing is legal");
        assert_eq!(cleared.category_id, None);
        assert_eq!(cleared.project_id, None);
        assert_eq!(cleared.receipt_node_id, None);
        // A blank string is what a form sends when a picker is emptied, and
        // means the same thing.
        let blanked = body(r#"{"categoryId":"  "}"#)
            .apply(base())
            .expect("a blank clears");
        assert_eq!(blanked.category_id, None);
        // Absent leaves it exactly where it was.
        assert_eq!(
            body(r#"{"merchant":"DB Fernverkehr"}"#)
                .apply(base())
                .expect("legal")
                .category_id,
            base().category_id
        );
    }

    #[test]
    fn a_vat_rate_can_be_taken_off_a_claim_that_no_longer_shows_one() {
        // The case a plain Option would make unreachable: a receipt re-read as
        // showing no tax at all.
        let corrected = body(r#"{"vatCents":0,"vatRateBp":null}"#)
            .apply(base())
            .expect("legal");
        assert_eq!(corrected.vat_cents, 0);
        assert_eq!(corrected.vat_rate_bp, None);
        // And 0 % is a rate somebody states, not the absence of one.
        let exempt = body(r#"{"vatCents":0,"vatRateBp":0}"#)
            .apply(base())
            .expect("legal");
        assert_eq!(exempt.vat_rate_bp, Some(0));
    }

    #[test]
    fn a_day_and_a_method_arrive_in_one_shape_or_are_refused() {
        assert_eq!(
            body(r#"{"spentOn":"2026-04-01"}"#)
                .apply(base())
                .expect("legal")
                .spent_on,
            Date::from_calendar_date(2026, time::Month::April, 1).unwrap()
        );
        for bad in [
            r#"{"spentOn":"01/04/2026"}"#,
            r#"{"spentOn":"2026-13-01"}"#,
            r#"{"spentOn":"2026-04-01T10:00:00Z"}"#,
            r#"{"spentOn":""}"#,
        ] {
            let problem = body(bad).apply(base()).expect_err("refused");
            assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}");
            assert!(detail(&problem).contains("spentOn"), "{bad}");
        }
        assert_eq!(
            body(r#"{"method":"card"}"#)
                .apply(base())
                .expect("legal")
                .method,
            ExpenseMethod::Card
        );
        // The wording of the refusal is the store's, so client and store never
        // disagree about which words are legal.
        let problem = body(r#"{"method":"credit"}"#)
            .apply(base())
            .expect_err("refused");
        assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(detail(&problem).contains("personal, card or cash"));
    }

    #[test]
    fn the_period_is_stated_at_both_ends_and_bounded() {
        let (from, to) = period(&query("2026-01-01", "2026-03-31", None)).expect("a real period");
        assert_eq!(iso_date(from), "2026-01-01");
        assert_eq!(iso_date(to), "2026-03-31");
        for (bad, named) in [
            (query("", "2026-03-31", None), "from"),
            (query("2026-01-01", "  ", None), "to"),
            (query("2026-01-01", "last friday", None), "to"),
        ] {
            let problem = period(&bad).expect_err("refused");
            assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY);
            assert!(detail(&problem).contains(named), "{}", detail(&problem));
        }
        let problem = period(&query("2020-01-01", "2026-03-31", None)).expect_err("refused");
        assert!(detail(&problem).contains("shorter than"));
    }

    #[test]
    fn the_status_filter_is_a_word_the_store_knows_or_a_refusal() {
        assert_eq!(status_filter(None).expect("legal"), None);
        assert_eq!(status_filter(Some("  ")).expect("legal"), None);
        assert_eq!(
            status_filter(Some("submitted")).expect("legal"),
            Some(ExpenseStatus::Submitted)
        );
        let problem = status_filter(Some("pending")).expect_err("refused");
        assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(detail(&problem).contains("expense status"));
    }
}
