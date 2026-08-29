//! Authenticated HTTP surface for tenant-scoped Billing price connections.

use alo_store::billing_price_connections::{
    NewPriceConnection, PriceConnection, PriceConnectionDirection, PriceConnectionHealth,
};
use alo_store::{AccountStore, BillingPriceConnectionId, BillingProductId};
use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::billing::{iso, map_store_err, parse_body};
use crate::error::Problem;
use crate::state::{AppState, authenticate};

fn connection_json(connection: &PriceConnection) -> Value {
    json!({
        "id": connection.id.as_str(),
        "direction": connection.direction.as_str(),
        "company": connection.company,
        "catalogue": connection.catalogue,
        "health": connection.health.as_str(),
        "cadence": connection.cadence,
        "channel": connection.channel,
        "changesCount": connection.changes_count,
        "lastSyncedAt": connection.last_synced_at.map(iso),
        "expiresAt": connection.expires_at.map(iso),
        "productIds": connection
            .product_ids
            .iter()
            .map(BillingProductId::as_str)
            .collect::<Vec<_>>(),
        "itemCount": connection.product_ids.len(),
        "createdAt": iso(connection.created_at),
        "updatedAt": iso(connection.updated_at),
    })
}

async fn load(
    account: &AccountStore,
    id: &BillingPriceConnectionId,
) -> Result<PriceConnection, Problem> {
    account
        .billing_price_connections()
        .await
        .map_err(map_store_err)?
        .into_iter()
        .find(|connection| connection.id == *id)
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such price connection"))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateBody {
    direction: String,
    company: String,
    catalogue: String,
    cadence: String,
    channel: String,
    #[serde(default)]
    product_ids: Vec<String>,
}

fn direction(value: &str) -> Result<PriceConnectionDirection, Problem> {
    match value {
        "received" => Ok(PriceConnectionDirection::Received),
        "shared" => Ok(PriceConnectionDirection::Shared),
        _ => Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "unknown price connection direction",
        )),
    }
}

fn health(value: &str) -> Result<PriceConnectionHealth, Problem> {
    match value {
        "connected" => Ok(PriceConnectionHealth::Connected),
        "attention" => Ok(PriceConnectionHealth::Attention),
        "paused" => Ok(PriceConnectionHealth::Paused),
        "expired" => Ok(PriceConnectionHealth::Expired),
        _ => Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "unknown price connection health",
        )),
    }
}

/// Lists all durable connections for the authenticated tenant.
pub async fn list_connections(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let connections = account
        .acc
        .billing_price_connections()
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "connections": connections.iter().map(connection_json).collect::<Vec<_>>()
    })))
}

/// Creates a received or shared connection and its catalogue product links.
pub async fn create_connection(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let body: CreateBody = parse_body(&body)?;
    let input = NewPriceConnection {
        direction: direction(&body.direction)?,
        company: body.company,
        catalogue: body.catalogue,
        cadence: body.cadence,
        channel: body.channel,
        product_ids: body
            .product_ids
            .into_iter()
            .map(BillingProductId::new)
            .collect(),
    };
    let id = account
        .acc
        .create_billing_price_connection(&input)
        .await
        .map_err(map_store_err)?;
    let connection = load(&account.acc, &id).await?;
    Ok(Json(json!({ "connection": connection_json(&connection) })))
}

#[derive(Deserialize)]
struct HealthBody {
    health: String,
}

/// Changes the operational health of an existing tenant connection.
pub async fn update_connection_health(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let body: HealthBody = parse_body(&body)?;
    let id = BillingPriceConnectionId::new(id);
    account
        .acc
        .set_billing_price_connection_health(&id, health(&body.health)?)
        .await
        .map_err(map_store_err)?;
    let connection = load(&account.acc, &id).await?;
    Ok(Json(json!({ "connection": connection_json(&connection) })))
}

/// Runs the local connection sync transition and returns the refreshed record.
pub async fn sync_connection(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let id = BillingPriceConnectionId::new(id);
    account
        .acc
        .sync_billing_price_connection(&id)
        .await
        .map_err(map_store_err)?;
    let connection = load(&account.acc, &id).await?;
    Ok(Json(json!({ "connection": connection_json(&connection) })))
}

/// Permanently disconnects one of this tenant's price connections.
pub async fn delete_connection(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .delete_billing_price_connection(&BillingPriceConnectionId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "deleted": true })))
}
