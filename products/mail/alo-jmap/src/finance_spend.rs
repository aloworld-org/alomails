//! Tenant spend controls. Amounts are integer cents and `null` disables a rule.

use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::SpendPolicy;

use crate::billing::{iso, map_store_err, parse_body};
use crate::error::Problem;
use crate::state::{AppState, authenticate};

fn policy_json(policy: &SpendPolicy, currency: &str) -> Value {
    json!({
        "receiptRequiredAboveCents": policy.receipt_required_above_cents,
        "projectRequiredAboveCents": policy.project_required_above_cents,
        "secondApprovalAboveCents": policy.second_approval_above_cents,
        "updatedBy": policy.updated_by.as_ref().map(|user| user.as_str()),
        "updatedAt": policy.updated_at.map(iso),
        "currency": currency,
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PolicyBody {
    #[serde(default)]
    receipt_required_above_cents: Option<i64>,
    #[serde(default)]
    project_required_above_cents: Option<i64>,
    #[serde(default)]
    second_approval_above_cents: Option<i64>,
}

pub async fn get_policy(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_finance()?;
    let currency = account
        .acc
        .billing_base_currency()
        .await
        .map_err(map_store_err)?;
    let policy = state
        .store
        .for_tenant(account.tenant)
        .spend_policy()
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "policy": policy_json(&policy, &currency) })))
}

pub async fn put_policy(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_finance()?;
    let currency = account
        .acc
        .billing_base_currency()
        .await
        .map_err(map_store_err)?;
    let input: PolicyBody = parse_body(&body)?;
    let policy = state
        .store
        .for_tenant(account.tenant)
        .set_spend_policy(
            &SpendPolicy {
                receipt_required_above_cents: input.receipt_required_above_cents,
                project_required_above_cents: input.project_required_above_cents,
                second_approval_above_cents: input.second_approval_above_cents,
                updated_by: None,
                updated_at: None,
            },
            &account.user,
        )
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "policy": policy_json(&policy, &currency) })))
}
