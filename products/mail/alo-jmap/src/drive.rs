//! Drive HTTP surface (ADR 0027). Authenticated, tenant-scoped through the
//! account door. A location is either the caller's personal "My Files" (no
//! `space` given) or a Space they belong to (`space=<id>`); access follows
//! location, so a non-member gets 404 and a space viewer trying to write gets
//! 403 — enforced entirely in the store.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use serde::Deserialize;
use serde_json::{Value, json};
use time::format_description::well_known::Rfc3339;

use alo_store::{BlobId, DriveLocation, DriveNode, DriveNodeId, NewDriveFile, SpaceId, StoreError};

use crate::error::Problem;
use crate::state::{AppState, authenticate};

pub(crate) fn map_err(e: StoreError) -> Problem {
    match e {
        StoreError::NotFound => Problem::with(StatusCode::NOT_FOUND, "not found"),
        StoreError::Forbidden => Problem::with(StatusCode::FORBIDDEN, "insufficient role"),
        StoreError::Conflict(msg) => Problem::with(StatusCode::CONFLICT, &msg),
        _ => Problem::server_error(),
    }
}

fn iso(t: time::OffsetDateTime) -> String {
    t.format(&Rfc3339).unwrap_or_default()
}

/// Personal when `space` is absent/empty, else that Space.
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

pub(crate) fn node_json(n: &DriveNode) -> Value {
    json!({
        "id": n.id.as_str(),
        "parentId": n.parent_id.as_ref().map(|p| p.as_str()),
        "space": if n.location_kind == "space" { Some(n.location_id.clone()) } else { None },
        "kind": n.kind,
        "name": n.name,
        "blobId": n.blob_id,
        "size": n.size,
        "contentType": n.content_type,
        "trashed": n.trashed,
        "sourceKind": n.source_kind,
        "sourceId": n.source_id,
        "createdBy": n.created_by,
        "createdAt": iso(n.created_at),
        "updatedAt": iso(n.updated_at),
    })
}

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    space: Option<String>,
    #[serde(default)]
    parent: Option<String>,
}

/// `GET /drive/list?space=&parent=` → `{"nodes":[...]}` — a folder's live
/// contents (root when no parent).
pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ListQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let loc = location_of(q.space.as_deref());
    let parent = parent_of(q.parent.as_deref());
    let nodes = crate::drive_intents::node_list(&account, &loc, parent.as_ref()).await?;
    Ok(Json(json!({ "nodes": nodes })))
}

/// `GET /drive/trash?space=` → `{"nodes":[...]}` — the trashed nodes of a location.
pub async fn trash(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ListQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let loc = location_of(q.space.as_deref());
    let nodes = account.acc.drive_trash(&loc).await.map_err(map_err)?;
    Ok(Json(
        json!({ "nodes": nodes.iter().map(node_json).collect::<Vec<_>>() }),
    ))
}

#[derive(Deserialize)]
pub struct FolderBody {
    #[serde(default)]
    space: Option<String>,
    #[serde(default)]
    parent: Option<String>,
    name: String,
}

/// `POST /drive/folders` `{space?, parent?, name}` → `{"id":"..."}`.
pub async fn create_folder(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: FolderBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let name = req.name.trim();
    if name.is_empty() {
        return Err(Problem::with(StatusCode::BAD_REQUEST, "a name is required"));
    }
    let id = crate::drive_intents::create_folder(
        &account,
        &location_of(req.space.as_deref()),
        parent_of(req.parent.as_deref()).as_ref(),
        name,
    )
    .await?;
    Ok(Json(json!({ "id": id.as_str() })))
}

#[derive(Deserialize)]
pub struct FileBody {
    #[serde(default)]
    space: Option<String>,
    #[serde(default)]
    parent: Option<String>,
    name: String,
    #[serde(rename = "blobId")]
    blob_id: String,
    size: i64,
    #[serde(default, rename = "contentType")]
    content_type: Option<String>,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default, rename = "sourceKind")]
    source_kind: Option<String>,
    #[serde(default, rename = "sourceId")]
    source_id: Option<String>,
}

const FILE_KINDS: [&str; 4] = ["file", "doc", "sheet", "slides"];

/// `POST /drive/files` `{space?, parent?, name, blobId, size, ...}` →
/// `{"id":"..."}` — register an uploaded blob as a file/document.
pub async fn create_file(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: FileBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    if req.name.trim().is_empty() || req.blob_id.trim().is_empty() {
        return Err(Problem::with(
            StatusCode::BAD_REQUEST,
            "name and blobId are required",
        ));
    }
    let kind = req.kind.as_deref().unwrap_or("file");
    if !FILE_KINDS.contains(&kind) {
        return Err(Problem::with(StatusCode::BAD_REQUEST, "invalid file kind"));
    }
    let new = NewDriveFile {
        name: req.name.trim().to_owned(),
        blob_id: req.blob_id.trim().to_owned(),
        size: req.size,
        content_type: req.content_type,
        kind: Some(kind.to_owned()),
        source_kind: req.source_kind,
        source_id: req.source_id,
    };
    let id = account
        .acc
        .drive_create_file(
            &location_of(req.space.as_deref()),
            parent_of(req.parent.as_deref()).as_ref(),
            &new,
        )
        .await
        .map_err(map_err)?;
    Ok(Json(json!({ "id": id.as_str() })))
}

/// `GET /drive/nodes/:id` → `{node}` — a single node the caller can read.
pub async fn get_node(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let node = crate::drive_intents::node_record(&account, &DriveNodeId::new(id)).await?;
    Ok(Json(json!({ "node": node })))
}

#[derive(Deserialize)]
pub struct RenameBody {
    name: String,
}

/// `PUT /drive/nodes/:id` `{name}` → `{status:"ok"}` — rename.
pub async fn rename(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: RenameBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let name = req.name.trim();
    if name.is_empty() {
        return Err(Problem::with(
            StatusCode::BAD_REQUEST,
            "name cannot be empty",
        ));
    }
    crate::drive_intents::rename_node(&account, &DriveNodeId::new(id), name).await?;
    Ok(Json(json!({ "status": "ok" })))
}

#[derive(Deserialize)]
pub struct DestBody {
    #[serde(default)]
    space: Option<String>,
    #[serde(default)]
    parent: Option<String>,
}

/// `POST /drive/nodes/:id/move` `{space?, parent?}` → `{status:"ok"}` — move
/// (re-scopes access).
pub async fn move_node(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: DestBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    crate::drive_intents::move_node(
        &account,
        &DriveNodeId::new(id),
        &location_of(req.space.as_deref()),
        parent_of(req.parent.as_deref()).as_ref(),
    )
    .await?;
    Ok(Json(json!({ "status": "ok" })))
}

/// `POST /drive/nodes/:id/copy` `{space?, parent?}` → `{"id":"..."}` — copy.
pub async fn copy_node(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: DestBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let new = account
        .acc
        .drive_copy(
            &DriveNodeId::new(id),
            &location_of(req.space.as_deref()),
            parent_of(req.parent.as_deref()).as_ref(),
        )
        .await
        .map_err(map_err)?;
    Ok(Json(json!({ "id": new.as_str() })))
}

/// `POST /drive/nodes/:id/trash` → `{status:"ok"}`.
pub async fn trash_node(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .drive_trash_node(&DriveNodeId::new(id))
        .await
        .map_err(map_err)?;
    Ok(Json(json!({ "status": "ok" })))
}

/// `POST /drive/nodes/:id/restore` → `{status:"ok"}`.
pub async fn restore_node(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .drive_restore_node(&DriveNodeId::new(id))
        .await
        .map_err(map_err)?;
    Ok(Json(json!({ "status": "ok" })))
}

/// `DELETE /drive/nodes/:id` → `{status:"ok"}` — permanent delete (from trash).
pub async fn purge(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .drive_purge(&DriveNodeId::new(id))
        .await
        .map_err(map_err)?;
    Ok(Json(json!({ "status": "ok" })))
}

/// `GET /drive/nodes/:id/versions` → `{"versions":[...]}`.
pub async fn versions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let vs = account
        .acc
        .drive_versions(&DriveNodeId::new(id))
        .await
        .map_err(map_err)?;
    Ok(Json(json!({
        "versions": vs.iter().map(|v| json!({
            "versionNo": v.version_no, "blobId": v.blob_id, "size": v.size,
            "createdBy": v.created_by, "createdAt": iso(v.created_at),
        })).collect::<Vec<_>>(),
    })))
}

#[derive(Deserialize)]
pub struct VersionBody {
    #[serde(rename = "blobId")]
    blob_id: String,
    size: i64,
}

/// `POST /drive/nodes/:id/versions` `{blobId, size}` → `{"versionNo":n}` — a new
/// upload/save.
pub async fn add_version(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: VersionBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    if req.blob_id.trim().is_empty() {
        return Err(Problem::with(StatusCode::BAD_REQUEST, "blobId is required"));
    }
    let no = account
        .acc
        .drive_add_version(&DriveNodeId::new(id), req.blob_id.trim(), req.size)
        .await
        .map_err(map_err)?;
    Ok(Json(json!({ "versionNo": no })))
}

/// `POST /drive/nodes/:id/versions/:no/restore` → `{"versionNo":n}` — restore an
/// old version as a new current one.
pub async fn restore_version(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, no)): Path<(String, i32)>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let new_no = account
        .acc
        .drive_restore_version(&DriveNodeId::new(id), no)
        .await
        .map_err(map_err)?;
    Ok(Json(json!({ "versionNo": new_no })))
}

/// `GET /drive/nodes/:id/download` — stream a file's current bytes. Gated by
/// read access to the node's location; the blob is then served tenant-scoped.
pub async fn download(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Response, Problem> {
    let account = authenticate(&state, &headers).await?;
    let Some(node) = account
        .acc
        .drive_node(&DriveNodeId::new(id))
        .await
        .map_err(map_err)?
    else {
        return Err(Problem::not_found());
    };
    let Some(blob) = node.blob_id else {
        return Err(Problem::with(StatusCode::BAD_REQUEST, "not a file"));
    };
    let bytes = account
        .acc
        .blob_bytes_for_send(&BlobId::new(blob))
        .await
        .map_err(map_err)?;
    let ct = node
        .content_type
        .as_deref()
        .unwrap_or("application/octet-stream");
    Ok(crate::blob::serve_download(bytes, ct, &node.name))
}
