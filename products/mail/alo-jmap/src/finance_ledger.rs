//! Account-ledger drill-down behind report figures.

use crate::billing::{iso_date, map_store_err};
use crate::error::Problem;
use crate::finance_reports::{day, reader};
use crate::state::AppState;
use alo_store::FinAccountId;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
pub struct LedgerQuery {
    from: Option<String>,
    to: Option<String>,
}

pub async fn account_ledger(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<LedgerQuery>,
) -> Result<Json<Value>, Problem> {
    let account = reader(&state, &headers).await?;
    let from = match query.from.as_deref() {
        Some(value) => Some(day("from", Some(value))?),
        None => None,
    };
    let to = match query.to.as_deref() {
        Some(value) => Some(day("to", Some(value))?),
        None => None,
    };
    let ledger = account
        .acc
        .fin_account_ledger(&FinAccountId::new(id), from, to, 2_000)
        .await
        .map_err(map_store_err)?;
    Ok(Json(
        json!({"ledger":{"accountId":ledger.account_id.as_str(),"openingCents":ledger.opening_cents,"closingCents":ledger.closing_cents,"truncated":ledger.truncated,"lines":ledger.lines.iter().map(|line|json!({"id":line.posting_id.as_str(),"entryId":line.entry_id.as_str(),"date":iso_date(line.entry_date),"kind":line.kind.as_str(),"entryMemo":line.entry_memo,"memo":line.memo,"currency":line.currency,"amountCents":line.amount_cents,"baseCents":line.base_cents,"runningCents":line.running_cents,"projectId":line.project_id})).collect::<Vec<_>>()}}),
    ))
}
