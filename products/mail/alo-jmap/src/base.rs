//! alo Base HTTP surface (ADR 0032). Authenticated; every read/write is gated in
//! the store through the Base's Drive node (a non-member gets 404, a space viewer
//! writing gets 403). Paths live under `/drive/*` (already proxied) and use
//! distinct literal prefixes (`base`, `base-tables`, `base-records`) so a node id
//! param never collides with a `tables`/`records` literal in the router.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::{Base, BaseRecordId, BaseTableId, DriveLocation, DriveNodeId, SpaceId, StoreError};

use crate::error::Problem;
use crate::state::{AppState, authenticate};

fn map_err(e: StoreError) -> Problem {
    match e {
        StoreError::NotFound => Problem::with(StatusCode::NOT_FOUND, "not found"),
        StoreError::Forbidden => Problem::with(StatusCode::FORBIDDEN, "insufficient role"),
        StoreError::Conflict(msg) => Problem::with(StatusCode::CONFLICT, &msg),
        _ => Problem::server_error(),
    }
}

fn location_of(space: Option<&str>) -> DriveLocation {
    match space {
        Some(s) if !s.trim().is_empty() => DriveLocation::Space(SpaceId::new(s.trim().to_owned())),
        _ => DriveLocation::Personal,
    }
}

fn parent_of(parent: Option<&str>) -> Option<DriveNodeId> {
    parent
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| DriveNodeId::new(p.to_owned()))
}

fn base_json(base: &Base) -> Value {
    json!({
        "nodeId": base.node_id.as_str(),
        "tables": base.tables.iter().map(|t| json!({
            "id": t.id.as_str(),
            "name": t.name,
            "fields": t.fields.iter().map(|f| json!({
                "id": f.id.as_str(), "name": f.name, "type": f.field_type, "options": f.options,
            })).collect::<Vec<_>>(),
            "views": t.views.iter().map(|v| json!({
                "id": v.id.as_str(), "kind": v.kind, "name": v.name, "config": v.config,
            })).collect::<Vec<_>>(),
            "records": t.records.iter().map(|r| json!({
                "id": r.id.as_str(), "cells": r.cells,
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    })
}

#[derive(Deserialize)]
pub struct CreateBaseBody {
    #[serde(default)]
    space: Option<String>,
    #[serde(default)]
    parent: Option<String>,
    name: String,
}

/// `POST /drive/base` `{space?, parent?, name}` → `{"nodeId":"..."}` — create a
/// Base (its node + a default table).
pub async fn create_base(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: CreateBaseBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let name = req.name.trim();
    if name.is_empty() {
        return Err(Problem::with(StatusCode::BAD_REQUEST, "a name is required"));
    }
    let node = account
        .acc
        .create_base(
            &location_of(req.space.as_deref()),
            parent_of(req.parent.as_deref()).as_ref(),
            name,
        )
        .await
        .map_err(map_err)?;
    Ok(Json(json!({ "nodeId": node.as_str() })))
}

/// `GET /drive/base/:nodeId` → the whole Base (tables, fields, records, views).
pub async fn get_base(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(node): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let Some(base) = account
        .acc
        .base(&DriveNodeId::new(node))
        .await
        .map_err(map_err)?
    else {
        return Err(Problem::with(StatusCode::NOT_FOUND, "no such base"));
    };
    Ok(Json(base_json(&base)))
}

#[derive(Deserialize)]
pub struct NameBody {
    name: String,
}

/// `POST /drive/base/:nodeId/tables` `{name}` → `{"id":"..."}`.
pub async fn add_table(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(node): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: NameBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    if req.name.trim().is_empty() {
        return Err(Problem::with(StatusCode::BAD_REQUEST, "a name is required"));
    }
    let id = account
        .acc
        .base_add_table(&DriveNodeId::new(node), req.name.trim())
        .await
        .map_err(map_err)?;
    Ok(Json(json!({ "id": id.as_str() })))
}

#[derive(Deserialize)]
pub struct FieldBody {
    name: String,
    #[serde(rename = "type")]
    field_type: String,
    #[serde(default)]
    options: Option<Value>,
}

/// `POST /drive/base-tables/:tableId/fields` `{name, type, options?}` → `{"id"}`.
pub async fn add_field(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(table): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: FieldBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    if req.name.trim().is_empty() {
        return Err(Problem::with(StatusCode::BAD_REQUEST, "a name is required"));
    }
    let id = account
        .acc
        .base_add_field(
            &BaseTableId::new(table),
            req.name.trim(),
            req.field_type.trim(),
            &req.options.unwrap_or_else(|| json!({})),
        )
        .await
        .map_err(map_err)?;
    Ok(Json(json!({ "id": id.as_str() })))
}

#[derive(Deserialize)]
pub struct RecordBody {
    #[serde(default)]
    cells: Option<Value>,
}

/// `POST /drive/base-tables/:tableId/records` `{cells?}` → `{"id"}`.
pub async fn add_record(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(table): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: RecordBody = serde_json::from_slice(&body).unwrap_or(RecordBody { cells: None });
    let id = account
        .acc
        .base_add_record(
            &BaseTableId::new(table),
            &req.cells.unwrap_or_else(|| json!({})),
        )
        .await
        .map_err(map_err)?;
    Ok(Json(json!({ "id": id.as_str() })))
}

/// `PUT /drive/base-records/:recordId` `{cells}` → `{status:"ok"}`.
pub async fn update_record(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(record): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: RecordBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    account
        .acc
        .base_update_record(
            &BaseRecordId::new(record),
            &req.cells.unwrap_or_else(|| json!({})),
        )
        .await
        .map_err(map_err)?;
    Ok(Json(json!({ "status": "ok" })))
}

/// `DELETE /drive/base-records/:recordId` → `{status:"ok"}`.
pub async fn delete_record(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(record): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .base_delete_record(&BaseRecordId::new(record))
        .await
        .map_err(map_err)?;
    Ok(Json(json!({ "status": "ok" })))
}

#[derive(Deserialize)]
pub struct ViewBody {
    kind: String,
    name: String,
    #[serde(default)]
    config: Option<Value>,
}

/// `POST /drive/base-tables/:tableId/views` `{kind, name, config?}` → `{"id"}`.
pub async fn add_view(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(table): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: ViewBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    if req.name.trim().is_empty() {
        return Err(Problem::with(StatusCode::BAD_REQUEST, "a name is required"));
    }
    let id = account
        .acc
        .base_add_view(
            &BaseTableId::new(table),
            req.kind.trim(),
            req.name.trim(),
            &req.config.unwrap_or_else(|| json!({})),
        )
        .await
        .map_err(map_err)?;
    Ok(Json(json!({ "id": id.as_str() })))
}
