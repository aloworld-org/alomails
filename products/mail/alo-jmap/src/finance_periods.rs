//! The fiscal-period HTTP surface (alo Finance, ADR 0035, wave B4.10) — the
//! periods a tenant reports on, and the soft close that shuts the books behind
//! them — over [`alo_store::fin_periods`].
//!
//! Four routes and three decisions this file makes rather than the store:
//!
//! - **Everybody reads, only an admin writes.** A bookkeeper about to date an
//!   expense into last quarter needs to know the quarter is closed *before* the
//!   journal refuses them, so the list is open to any authenticated member of
//!   the tenant. Defining, closing and reopening are `require_admin` — the same
//!   gate the approvals inbox and the mileage rate table use — because closing
//!   the books is the act that makes a report final. (B4.12's accountant role
//!   widens the write gate additively; nothing decided here moves.)
//! - **The lock date travels with the list**, computed by the store from the
//!   closed periods. A client never derives it: the journal enforces one
//!   number, and a screen that computed its own would eventually disagree with
//!   the only one that matters.
//! - **Close and reopen are two named acts, not a settable `status`.** They
//!   have different consequences (one shuts the books, the other admits a
//!   reported period is being changed and demands a reason for it), and the
//!   audit trail (B2.13) records them under their own names —
//!   `finance.period.close` and `finance.period.reopen`.
//!
//! Nothing here is personal data: a period is two dates and a state. `closedBy`
//! is a user id, which the same surface already returns for a decided expense.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::{FinPeriod, FinPeriodId};

use crate::billing::{iso, iso_date, map_store_err, parse_body};
use crate::error::Problem;
use crate::finance_expenses::stated_day;
use crate::state::{AppState, authenticate};

/// One period as JSON. `note` is the note of the state it is in — what the
/// closer said, or why it was reopened.
fn period_json(period: &FinPeriod) -> Value {
    json!({
        "id": period.id.as_str(),
        "fromDate": iso_date(period.from_date),
        "toDate": iso_date(period.to_date),
        "status": period.status.as_str(),
        "closedBy": period.closed_by.as_ref().map(alo_store::UserId::as_str),
        "closedAt": period.closed_at.map(iso),
        "note": period.note,
        "createdAt": iso(period.created_at),
    })
}

/// The body of a period definition: the two days it runs between, inclusive.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PeriodBody {
    #[serde(default)]
    from_date: Option<String>,
    #[serde(default)]
    to_date: Option<String>,
}

/// The body of a close or a reopen. Optional on the close (a sentence about
/// what was filed), required on the reopen — enforced by the store, so the two
/// doors cannot drift apart.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NoteBody {
    #[serde(default)]
    note: Option<String>,
}

/// Reads the note out of a body that may be absent altogether, so an empty
/// `POST`, `{}` and an explicit `null` all mean "nothing said".
fn stated_note(body: &axum::body::Bytes) -> Result<String, Problem> {
    if body.is_empty() {
        return Ok(String::new());
    }
    let req: NoteBody = parse_body(body)?;
    Ok(req.note.unwrap_or_default())
}

/// Reads a required day, naming the field in the refusal.
fn required_day(name: &str, raw: Option<&str>) -> Result<time::Date, Problem> {
    let stated = raw
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("{name} is required: a fiscal period is two days"),
            )
        })?;
    stated_day(name, stated)
}

/// `GET /finance/periods` → `{"periods": [ … ], "lockDate": "YYYY-MM-DD"|null}`
/// — the tenant's periods oldest first, and the day the books are shut through.
///
/// Readable by any member of the tenant: knowing a quarter is closed before
/// typing a date into it is what stops the journal having to say no.
///
/// # Errors
/// `401` without a valid bearer token; `500` on a store failure.
pub async fn list_periods(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let periods = account.acc.fin_periods().await.map_err(map_store_err)?;
    let lock_date = account.acc.fin_lock_date().await.map_err(map_store_err)?;
    Ok(Json(json!({
        "periods": periods.iter().map(period_json).collect::<Vec<_>>(),
        "lockDate": lock_date.map(iso_date),
    })))
}

/// `POST /finance/periods` `{fromDate, toDate}` → `{"period": {…}}` — **admin
/// only**: defines a period. It starts open; closing it is a separate act on a
/// separate day.
///
/// # Errors
/// `401`/`403` as above; `422` when a day is missing, malformed, the wrong way
/// round or spans more than a year; `409` when it overlaps a period that
/// exists, would sit inside closed books, or the tenant is at its ceiling.
pub async fn create_period(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_admin()?;
    let req: PeriodBody = parse_body(&body)?;
    let from_date = required_day("fromDate", req.from_date.as_deref())?;
    let to_date = required_day("toDate", req.to_date.as_deref())?;
    let period = account
        .acc
        .create_fin_period(from_date, to_date)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "period": period_json(&period) })))
}

/// `POST /finance/periods/{id}/close` `{note?}` → `{"period": {…}}` — **admin
/// only**: the books are shut through this period's last day, and every entry
/// dated on or before it is refused until somebody reopens it.
///
/// # Errors
/// `401`/`403` as above; `404` when the period is not this tenant's; `409` when
/// it is already closed or an earlier period is still open; `422` when the note
/// is too long.
pub async fn close_period(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_admin()?;
    let note = stated_note(&body)?;
    let period = account
        .acc
        .close_fin_period(&FinPeriodId::new(id), &note)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "period": period_json(&period) })))
}

/// `POST /finance/periods/{id}/reopen` `{note}` → `{"period": {…}}` — **admin
/// only**: a reported period is opened again, with the reason it had to be.
///
/// The reason is required. It replaces the closing note, because a period
/// carries the note of the state it is in; the audit trail carries the history.
///
/// # Errors
/// `401`/`403` as above; `404` when the period is not this tenant's; `409` when
/// it is not closed or a later period is still closed; `422` when the reason is
/// blank or too long.
pub async fn reopen_period(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_admin()?;
    let note = stated_note(&body)?;
    let period = account
        .acc
        .reopen_fin_period(&FinPeriodId::new(id), &note)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "period": period_json(&period) })))
}
