//! JMAP blob upload/download (RFC 8620 §6). Blob ids are the store's —
//! no second id space. Upload enforces the size ceiling; download is
//! tenant-scoped and serves the stored Content-Type with no sniffing.

use alo_store::{BlobId, StoreError};
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_TYPE};
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::{Json, http::HeaderMap};
use serde_json::{Value, json};

use crate::error::Problem;
use crate::state::{AppState, authenticate};

fn store_problem(e: StoreError) -> Problem {
    match e {
        StoreError::NotFound => Problem::not_found(),
        StoreError::TooLarge { .. } => Problem::too_large(),
        StoreError::OverQuota => {
            Problem::with(StatusCode::INSUFFICIENT_STORAGE, "storage quota exceeded")
        }
        _ => Problem::server_error(),
    }
}

/// `POST /jmap/upload/{accountId}` — content-address the body into the
/// store's blob layer and return its blob id.
pub async fn upload(
    State(state): State<AppState>,
    Path(account_id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    if account_id != account.account_id() {
        return Err(Problem::not_found());
    }
    if body.len() as u64 > state.limits.max_size_upload {
        return Err(Problem::too_large());
    }
    let content_type = headers
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let size = body.len();
    let blob_id = account
        .acc
        .put_blob(body, content_type.as_deref())
        .await
        .map_err(store_problem)?;
    Ok(Json(json!({
        "accountId": account.account_id(),
        "blobId": blob_id.as_str(),
        "type": content_type.unwrap_or_else(|| "application/octet-stream".to_owned()),
        "size": size
    })))
}

/// `GET /jmap/download/{accountId}/{blobId}/{name}` — the blob's bytes,
/// tenant-scoped, served as an attachment with the stored type and
/// `nosniff`.
pub async fn download(
    State(state): State<AppState>,
    Path((account_id, blob_id, name)): Path<(String, String, String)>,
    headers: HeaderMap,
) -> Result<Response, Problem> {
    let account = authenticate(&state, &headers).await?;
    if account_id != account.account_id() {
        return Err(Problem::not_found());
    }

    // Attachment parts are addressed as "{messageBlobId}~a{index}": load the
    // message blob (account-scoped, like any blob), MIME-parse it, and serve
    // the decoded part. The message blob id is the ownership boundary.
    if let Some((msg_blob, index)) = parse_attachment_id(&blob_id) {
        let raw = account
            .acc
            .blob_bytes(&BlobId::new(msg_blob.to_owned()))
            .await
            .map_err(store_problem)?;
        let (part_bytes, part_type, part_name) =
            crate::mime_read::attachment_bytes(&raw, index).ok_or_else(Problem::not_found)?;
        return Ok(serve_download(
            Bytes::from(part_bytes),
            &part_type,
            &part_name,
        ));
    }

    let id = BlobId::new(blob_id);
    // The account door scopes blob access to blobs referenced by one of
    // this account's messages: an unreferenced/foreign blob is NotFound
    // from blob()/blob_bytes() themselves — no separate ownership guard.
    let meta = account.acc.blob(&id).await.map_err(store_problem)?;
    let bytes = account.acc.blob_bytes(&id).await.map_err(store_problem)?;

    let ctype = meta
        .content_type
        .unwrap_or_else(|| "application/octet-stream".to_owned());
    Ok(serve_download(bytes, &ctype, &name))
}

/// Split a composite attachment blob id "{messageBlobId}~a{index}" into the
/// message blob id and the attachment's zero-based index. `None` for a plain
/// blob id (no `~a` marker or a non-numeric index).
fn parse_attachment_id(blob_id: &str) -> Option<(&str, usize)> {
    let (msg_blob, idx) = blob_id.rsplit_once("~a")?;
    if msg_blob.is_empty() {
        return None;
    }
    idx.parse::<usize>().ok().map(|i| (msg_blob, i))
}

/// Serve raw bytes as a downloadable attachment: the given content type,
/// `nosniff`, and a sanitized `Content-Disposition` filename. Shared with the
/// public share-download path ([`crate::share`]).
pub(crate) fn serve_download(bytes: Bytes, ctype: &str, filename: &str) -> Response {
    let mut resp = (StatusCode::OK, bytes).into_response();
    let h = resp.headers_mut();
    h.insert(
        CONTENT_TYPE,
        HeaderValue::from_str(ctype)
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    h.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    // Never let a shared proxy/CDN retain downloaded bytes (matters most for the
    // public share path, where the URL is a capability).
    h.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    let safe = filename.replace(['\r', '\n', '"', '\\'], "");
    h.insert(
        CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{safe}\""))
            .unwrap_or_else(|_| HeaderValue::from_static("attachment")),
    );
    resp
}
