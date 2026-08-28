//! The design of a quotation over HTTP — where the studio's layout is kept so
//! the printed document and the PDF can carry it (alo Billing, ADR 0035).
//!
//! Two routes on the quote's own path:
//!
//! - `GET /billing/quotes/{id}/design` → `{"design": …|null, "updatedAt"}`.
//!   `null` is a quote that has never been designed; the studio then starts
//!   from its blank design.
//! - `PUT /billing/quotes/{id}/design` with the design as the body → the same
//!   answer. The whole design is replaced every time — the studio saves the
//!   document it is looking at, not a patch stream — and the store decides
//!   what it accepts: a JSON object of bounded size, on a **draft**. A sent
//!   offer answers `409`: the paper the customer holds does not change after
//!   the fact.
//!
//! The body limit is the store's ceiling plus a little framing, set on this
//! route alone: pictures travel inside the design, and the router's default
//! limit is sized for JSON that carries none.

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use serde_json::{Value, json};

use alo_store::{BillingQuoteId, QUOTE_DESIGN_MAX_BYTES};

use crate::billing::{iso, map_store_err, parse_body};
use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// The most the `PUT` accepts on the wire: the stored ceiling plus framing.
pub const DESIGN_BODY_LIMIT: usize = QUOTE_DESIGN_MAX_BYTES + 64 * 1024;
// The wire limit is the stored ceiling plus framing only — never a second,
// larger ceiling of its own.
const _: () = assert!(DESIGN_BODY_LIMIT - QUOTE_DESIGN_MAX_BYTES <= 128 * 1024);

/// `GET /billing/quotes/{id}/design`.
pub async fn get_design(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let record = account
        .acc
        .billing_quote_design(&BillingQuoteId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(match record {
        Some(record) => json!({ "design": record.design, "updatedAt": iso(record.updated_at) }),
        None => json!({ "design": Value::Null, "updatedAt": Value::Null }),
    }))
}

/// `PUT /billing/quotes/{id}/design`.
pub async fn put_design(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let design: Value = parse_body(&body)?;
    let id = BillingQuoteId::new(id);
    account
        .acc
        .set_billing_quote_design(&id, &design)
        .await
        .map_err(map_store_err)?;
    let record = account
        .acc
        .billing_quote_design(&id)
        .await
        .map_err(map_store_err)?;
    Ok(Json(match record {
        Some(record) => json!({ "design": record.design, "updatedAt": iso(record.updated_at) }),
        None => json!({ "design": design, "updatedAt": Value::Null }),
    }))
}
