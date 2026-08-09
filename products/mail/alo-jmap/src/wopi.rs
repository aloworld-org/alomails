//! WOPI host (ADR 0010) — the Collabora office editor's back-channel. Collabora
//! loads and saves a Drive file's bytes by calling these endpoints with an
//! `access_token` we minted; the token is a short-lived, HMAC-signed, stateless
//! grant encoding (node, tenant, user, can-write, expiry), so no server state is
//! kept. The token endpoint (`/drive/nodes/:id/office`) is bearer-authed and
//! checks the caller's Drive access before minting; the `/wopi/*` endpoints
//! authenticate ONLY via that token (Collabora does not carry our bearer).

use std::sync::OnceLock;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hmac::{Hmac, Mac};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::Sha256;

use alo_store::{BlobId, DriveNodeId, TenantId, UserId};

use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// The signing secret; WOPI/office editing is disabled when unset.
fn secret() -> Option<&'static str> {
    static S: OnceLock<Option<String>> = OnceLock::new();
    S.get_or_init(|| {
        std::env::var("ALO_JMAP_WOPI_SECRET")
            .ok()
            .filter(|s| !s.is_empty())
    })
    .as_deref()
}

/// A verified WOPI grant.
struct WopiToken {
    node: String,
    tenant: String,
    user: String,
    can_write: bool,
}

fn sign_bytes(payload: &str, key: &str) -> Vec<u8> {
    // HMAC accepts any key length, so this never errors; on the impossible error
    // path an empty MAC simply never verifies.
    match Hmac::<Sha256>::new_from_slice(key.as_bytes()) {
        Ok(mut mac) => {
            mac.update(payload.as_bytes());
            mac.finalize().into_bytes().to_vec()
        }
        Err(_) => Vec::new(),
    }
}

/// Mints a token valid for `ttl_secs` for (node, tenant, user, can_write).
fn mint(
    node: &str,
    tenant: &str,
    user: &str,
    can_write: bool,
    now: i64,
    ttl_secs: i64,
) -> Option<String> {
    let key = secret()?;
    let exp = now + ttl_secs;
    let payload = format!(
        "{node}|{tenant}|{user}|{}|{exp}",
        if can_write { 1 } else { 0 }
    );
    let p = URL_SAFE_NO_PAD.encode(payload.as_bytes());
    Some(format!(
        "{p}.{}",
        URL_SAFE_NO_PAD.encode(sign_bytes(&p, key))
    ))
}

/// Verifies a token: HMAC (constant-time via the mac crate), format, and expiry.
fn verify(token: &str, now: i64) -> Option<WopiToken> {
    let key = secret()?;
    let (p, mac_b64) = token.split_once('.')?;
    let mac_bytes = URL_SAFE_NO_PAD.decode(mac_b64).ok()?;
    let mut mac = Hmac::<Sha256>::new_from_slice(key.as_bytes()).ok()?;
    mac.update(p.as_bytes());
    mac.verify_slice(&mac_bytes).ok()?; // constant-time; None on mismatch
    let raw = URL_SAFE_NO_PAD.decode(p).ok()?;
    let payload = String::from_utf8(raw).ok()?;
    let parts: Vec<&str> = payload.split('|').collect();
    if parts.len() != 5 {
        return None;
    }
    let exp: i64 = parts[4].parse().ok()?;
    if exp <= now {
        return None;
    }
    Some(WopiToken {
        node: parts[0].to_owned(),
        tenant: parts[1].to_owned(),
        user: parts[2].to_owned(),
        can_write: parts[3] == "1",
    })
}

fn now_unix() -> i64 {
    // time is already a dependency; use its clock.
    time::OffsetDateTime::now_utc().unix_timestamp()
}

#[derive(Deserialize)]
pub struct AccessTokenQuery {
    access_token: String,
}

// ---- token mint (bearer-authed) ---------------------------------------------

/// `GET /drive/nodes/:id/office` → `{"token":"..."}` — mint a WOPI token for the
/// Collabora editor after checking the caller's Drive access. The frontend
/// combines this with the same-origin `/hosting/discovery` to build the editor
/// iframe URL.
pub async fn office_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    if secret().is_none() {
        return Err(Problem::with(
            StatusCode::NOT_FOUND,
            "office editing not configured",
        ));
    }
    let account = authenticate(&state, &headers).await?;
    let node = DriveNodeId::new(id);
    // Readable? (drive_writable errors NotFound if not visible.)
    let can_write = account
        .acc
        .drive_writable(&node)
        .await
        .map_err(|_| Problem::with(StatusCode::NOT_FOUND, "no such file"))?;
    let token = mint(
        node.as_str(),
        account.tenant.as_str(),
        account.user.as_str(),
        can_write,
        now_unix(),
        12 * 3600,
    )
    .ok_or_else(Problem::server_error)?;
    Ok(Json(json!({ "token": token })))
}

// ---- WOPI host endpoints (token-authed) -------------------------------------

/// Resolves a WOPI token + the node it grants, or a 401/404 Problem.
async fn resolve(
    state: &AppState,
    id: &str,
    token: &str,
) -> Result<(alo_store::AccountStore, alo_store::DriveNode, bool), Problem> {
    let t = verify(token, now_unix())
        .ok_or_else(|| Problem::with(StatusCode::UNAUTHORIZED, "invalid token"))?;
    if t.node != id {
        return Err(Problem::with(
            StatusCode::UNAUTHORIZED,
            "token/file mismatch",
        ));
    }
    let acc = state
        .store
        .for_account(TenantId::new(t.tenant), UserId::new(t.user));
    let node = acc
        .drive_node(&DriveNodeId::new(id.to_owned()))
        .await
        .map_err(|_| Problem::server_error())?
        .ok_or_else(Problem::not_found)?;
    Ok((acc, node, t.can_write))
}

/// `GET /wopi/files/:id` — CheckFileInfo (RFC/WOPI): the metadata Collabora needs.
pub async fn check_file_info(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<AccessTokenQuery>,
) -> Result<Json<Value>, Problem> {
    let (_acc, node, can_write) = resolve(&state, &id, &q.access_token).await?;
    Ok(Json(json!({
        "BaseFileName": node.name,
        "Size": node.size,
        "OwnerId": node.created_by,
        "UserId": node.created_by,
        "UserCanWrite": can_write,
        "Version": node.updated_at.unix_timestamp().to_string(),
        "PostMessageOrigin": state.base_url,
        "SupportsUpdate": true,
        "SupportsLocks": false,
        "DisablePrint": false,
    })))
}

/// `GET /wopi/files/:id/contents` — the file's current bytes.
pub async fn get_file(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<AccessTokenQuery>,
) -> Result<Response, Problem> {
    let (acc, node, _w) = resolve(&state, &id, &q.access_token).await?;
    let Some(blob) = node.blob_id else {
        return Err(Problem::with(StatusCode::CONFLICT, "not a file"));
    };
    let bytes = acc
        .blob_bytes_for_send(&BlobId::new(blob))
        .await
        .map_err(|_| Problem::server_error())?;
    Ok((
        [(axum::http::header::CONTENT_TYPE, "application/octet-stream")],
        bytes,
    )
        .into_response())
}

/// `POST /wopi/files/:id/contents` — save new bytes as a new Drive version.
pub async fn put_file(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<AccessTokenQuery>,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let (acc, node, can_write) = resolve(&state, &id, &q.access_token).await?;
    if !can_write {
        return Err(Problem::with(StatusCode::FORBIDDEN, "read-only"));
    }
    let size = body.len() as i64;
    let blob = acc
        .put_blob(body, node.content_type.as_deref())
        .await
        .map_err(|_| Problem::server_error())?;
    acc.drive_add_version(&DriveNodeId::new(id), blob.as_str(), size)
        .await
        .map_err(|_| Problem::server_error())?;
    Ok(Json(json!({ "status": "ok" })))
}
