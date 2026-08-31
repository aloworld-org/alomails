//! Authenticated invoice presentation reads and draft-only writes.

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use serde_json::{Value, json};

use alo_store::{BillingInvoiceId, QUOTE_DESIGN_MAX_BYTES};

use crate::billing::{iso, map_store_err, parse_body};
use crate::error::Problem;
use crate::state::{AppState, authenticate};

pub const DESIGN_BODY_LIMIT: usize = QUOTE_DESIGN_MAX_BYTES + 64 * 1024;

pub async fn get_design(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let record = account
        .acc
        .billing_invoice_design(&BillingInvoiceId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(match record {
        Some(record) => json!({ "design": record.design, "updatedAt": iso(record.updated_at) }),
        None => json!({ "design": Value::Null, "updatedAt": Value::Null }),
    }))
}

pub async fn put_design(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let design: Value = parse_body(&body)?;
    let id = BillingInvoiceId::new(id);
    account
        .acc
        .set_billing_invoice_design(&id, &design)
        .await
        .map_err(map_store_err)?;
    let record = account
        .acc
        .billing_invoice_design(&id)
        .await
        .map_err(map_store_err)?;
    Ok(Json(match record {
        Some(record) => json!({ "design": record.design, "updatedAt": iso(record.updated_at) }),
        None => json!({ "design": design, "updatedAt": Value::Null }),
    }))
}
