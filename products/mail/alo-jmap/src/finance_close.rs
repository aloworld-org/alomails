//! Server-owned accounting close readiness. A close screen must not infer
//! whether the books are safe to close from several browser requests: this
//! endpoint takes one reporting date and returns the checks made against one
//! consistent backend contract.

use axum::Json;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::{AgedSide, BankLineStatus};

use crate::billing::{iso_date, map_store_err};
use crate::error::Problem;
use crate::finance_reports::{day, reader};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct CloseQuery {
    #[serde(default)]
    on: Option<String>,
}

/// `GET /finance/close-readiness?on=YYYY-MM-DD` returns the blocking and
/// advisory checks for closing through `on`. Counts and balance assertions are
/// produced by the server; the client only presents them.
pub async fn close_readiness(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<CloseQuery>,
) -> Result<Json<Value>, Problem> {
    let account = reader(&state, &headers).await?;
    let on = day("on", query.on.as_deref())?;

    let unmatched = account
        .acc
        .bank_lines(None, Some(BankLineStatus::Unmatched))
        .await
        .map_err(map_store_err)?;
    let pending = state
        .store
        .for_tenant(account.tenant.clone())
        .pending_expenses()
        .await
        .map_err(map_store_err)?;
    let receivables = account
        .acc
        .fin_aged(on, AgedSide::Receivable)
        .await
        .map_err(map_store_err)?;
    let payables = account
        .acc
        .fin_aged(on, AgedSide::Payable)
        .await
        .map_err(map_store_err)?;
    let balance = account
        .acc
        .fin_balance_sheet(on)
        .await
        .map_err(map_store_err)?;

    let unmatched_count = i64::try_from(unmatched.len()).unwrap_or(i64::MAX);
    let pending_count = i64::try_from(pending.len()).unwrap_or(i64::MAX);
    let blocking_count = i64::from(unmatched_count > 0)
        + i64::from(pending_count > 0)
        + i64::from(!balance.balances());
    let warning_count =
        i64::from(receivables.unconverted_count > 0) + i64::from(payables.unconverted_count > 0);

    Ok(Json(json!({
        "readiness": {
            "on": iso_date(on),
            "ready": blocking_count == 0,
            "blockingCount": blocking_count,
            "warningCount": warning_count,
            "checks": [
                { "key": "bankReconciliation", "status": if unmatched_count == 0 { "passed" } else { "blocked" }, "count": unmatched_count },
                { "key": "expenseApprovals", "status": if pending_count == 0 { "passed" } else { "blocked" }, "count": pending_count },
                { "key": "balanceSheet", "status": if balance.balances() { "passed" } else { "blocked" }, "count": balance.difference_cents.abs() },
                { "key": "receivableFx", "status": if receivables.unconverted_count == 0 { "passed" } else { "warning" }, "count": receivables.unconverted_count },
                { "key": "payableFx", "status": if payables.unconverted_count == 0 { "passed" } else { "warning" }, "count": payables.unconverted_count }
            ]
        }
    })))
}
