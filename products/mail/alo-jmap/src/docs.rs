//! alo Docs HTTP API (ADR 0015): tenant- and owner-scoped CRUD for
//! technical-authoring documents. Thin handlers over `AccountStore`'s document
//! methods, which bake in the `(tenant, owner)` predicate — a foreign or
//! non-existent id is an indistinguishable 404, never another user's document.
//!
//! - `GET    /docs`        → list the caller's documents (metadata only)
//! - `POST   /docs`        → create `{title}` → the new document
//! - `GET    /docs/{id}`   → one document with its blocks
//! - `PUT    /docs/{id}`   → save `{title, blocks}`
//! - `DELETE /docs/{id}`   → delete

use alo_store::{Document, DocumentSummary, StoreError};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::{Json, body::Bytes};
use serde_json::{Value, json};

use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// Cap the saved document (bytes). Blocks can hold code and tables, but a
/// document is not a file store; this bounds a single save.
pub const MAX_DOC_BYTES: usize = 2 * 1024 * 1024;

/// Cap the title length (characters) to a sane single line.
const MAX_TITLE: usize = 200;

fn summary_json(d: &DocumentSummary) -> Value {
    json!({ "id": d.id, "title": d.title, "updatedAt": d.updated_at })
}

fn document_json(d: &Document) -> Value {
    // `blocks` is stored as JSON text; re-parse so the client receives a real
    // array, not a string. A corrupt row (should never happen) degrades to [].
    let blocks: Value = serde_json::from_str(&d.blocks).unwrap_or_else(|_| json!([]));
    json!({ "id": d.id, "title": d.title, "blocks": blocks, "updatedAt": d.updated_at })
}

/// `GET /docs` — the caller's documents, newest-first.
pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let docs = account
        .acc
        .list_documents()
        .await
        .map_err(|_| Problem::server_error())?;
    let out: Vec<Value> = docs.iter().map(summary_json).collect();
    Ok(Json(json!({ "documents": out })))
}

/// `POST /docs` — create a document. Body: `{"title": "..."}` (blank allowed;
/// defaults to an untitled document).
pub async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let request: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let title = clean_title(request.get("title").and_then(Value::as_str).unwrap_or(""));
    let doc = account
        .acc
        .create_document(&title)
        .await
        .map_err(|_| Problem::server_error())?;
    Ok(Json(document_json(&doc)))
}

/// `GET /docs/{id}` — one document with its blocks, or 404.
pub async fn get(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let doc = account
        .acc
        .get_document(&id)
        .await
        .map_err(|_| Problem::server_error())?
        .ok_or_else(Problem::not_found)?;
    Ok(Json(document_json(&doc)))
}

/// `PUT /docs/{id}` — save the title and blocks. Body: `{"title", "blocks": [...]}`.
pub async fn save(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    if body.len() > MAX_DOC_BYTES {
        return Err(Problem::with(
            StatusCode::PAYLOAD_TOO_LARGE,
            "document too large",
        ));
    }
    let request: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let title = clean_title(request.get("title").and_then(Value::as_str).unwrap_or(""));
    let blocks = request.get("blocks").cloned().unwrap_or_else(|| json!([]));
    if !blocks.is_array() {
        return Err(Problem::with(
            StatusCode::BAD_REQUEST,
            "blocks must be an array",
        ));
    }
    let blocks_str = serde_json::to_string(&blocks).map_err(|_| Problem::server_error())?;
    account
        .acc
        .save_document(&id, &title, &blocks_str)
        .await
        .map_err(store_err)?;
    Ok(Json(json!({ "ok": true })))
}

/// `DELETE /docs/{id}` — delete the caller's document.
pub async fn delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.acc.delete_document(&id).await.map_err(store_err)?;
    Ok(Json(json!({ "ok": true })))
}

/// Trim a title to a single sane line, bounded in length.
fn clean_title(raw: &str) -> String {
    let cleaned: String = raw
        .replace(['\r', '\n', '\t'], " ")
        .trim()
        .chars()
        .take(MAX_TITLE)
        .collect();
    if cleaned.is_empty() {
        "Untitled document".to_owned()
    } else {
        cleaned
    }
}

/// Map a store error to a client problem: a missing/foreign document is a 404,
/// anything else a coarse 500.
fn store_err(err: StoreError) -> Problem {
    match err {
        StoreError::NotFound => Problem::not_found(),
        _ => Problem::server_error(),
    }
}
