//! Large-file transfer (alo Transfer): upload a file too big to attach and
//! get back a private, expiring **public** download link that rides the message
//! in place of an inline attachment. There is no size limit — the upload and the
//! download are both streamed, so a large file is never buffered whole — and the
//! sender chooses how long the link lives.
//!
//! - `POST /share/upload?name=<file>&days=<n>` (authenticated) streams the bytes
//!   to storage and mints a link, returning `{url, filename, size, expiresAt}`.
//! - `GET /share/{token}` (PUBLIC — the recipient may be anyone) streams the
//!   file as a download if the token is live, else a plain 404.
//!
//! Security: the token is 256-bit and stored hashed at rest; the download is
//! always `Content-Disposition: attachment` + `nosniff` + `no-store`, so a shared
//! file is never rendered inline. The link is a capability URL — holding it is
//! the only authorization, exactly the WeTransfer model.

use std::time::{SystemTime, UNIX_EPOCH};

use alo_store::ShareStream;
use axum::Json;
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_TYPE};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use futures::StreamExt;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// Cap on the stored filename length.
const MAX_FILENAME_LEN: usize = 255;
/// Default link lifetime when the caller doesn't choose one (days).
const DEFAULT_TTL_DAYS: i64 = 7;
/// The longest a link may live (days) — an upper bound on the pick.
const MAX_TTL_DAYS: i64 = 365;

#[derive(Deserialize)]
pub struct UploadQuery {
    /// The original filename (URL-encoded by the client; axum decodes it).
    name: Option<String>,
    /// How many days the link should live (clamped to `1..=MAX_TTL_DAYS`).
    days: Option<i64>,
}

/// `POST /share/upload?name=<file>&days=<n>` — authenticated. The request body is
/// the raw file (streamed, unbounded); the `Content-Type` header is its media
/// type. Returns the share link.
pub async fn upload(
    State(state): State<AppState>,
    Query(q): Query<UploadQuery>,
    headers: HeaderMap,
    body: Body,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let filename = sanitize_filename(q.name.as_deref());
    let content_type = sanitize_content_type(
        headers
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or(""),
    );
    let days = q.days.unwrap_or(DEFAULT_TTL_DAYS).clamp(1, MAX_TTL_DAYS);
    let expires = now_epoch() + days * 24 * 60 * 60;

    // Stream the request body straight into storage — never buffered whole.
    let content = body
        .into_data_stream()
        .map(|chunk| chunk.map_err(std::io::Error::other))
        .boxed();
    let created = account
        .acc
        .create_share(content, &filename, &content_type, expires)
        .await
        .map_err(|_| Problem::server_error())?;

    if created.size == 0 {
        return Err(Problem::with(StatusCode::BAD_REQUEST, "empty file"));
    }

    let base = state.base_url.trim_end_matches('/');
    Ok(Json(json!({
        "url": format!("{base}/share/{}", created.token),
        "filename": filename,
        "size": created.size,
        "expiresAt": created.expires_at_epoch,
    })))
}

/// `GET /share/{token}` — PUBLIC. Streams the file as a download if the token is
/// live, otherwise a plain 404. No authentication: the unguessable token is the
/// capability.
pub async fn download(State(state): State<AppState>, Path(token): Path<String>) -> Response {
    let target = match state.store.resolve_share(&token).await {
        Ok(Some(t)) => t,
        _ => return not_found(),
    };
    match state.store.open_share(&target).await {
        Ok(stream) => serve_stream(stream, &target.content_type, &target.filename),
        Err(_) => not_found(),
    }
}

/// Stream a share to the client as a forced download with the safety headers.
fn serve_stream(share: ShareStream, ctype: &str, filename: &str) -> Response {
    let mut resp = Body::from_stream(share.content).into_response();
    let h = resp.headers_mut();
    h.insert(
        CONTENT_TYPE,
        HeaderValue::from_str(ctype)
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    h.insert(CONTENT_LENGTH, HeaderValue::from(share.size));
    h.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    h.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    let safe = filename.replace(['\r', '\n', '"', '\\'], "");
    h.insert(
        CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{safe}\""))
            .unwrap_or_else(|_| HeaderValue::from_static("attachment")),
    );
    resp
}

/// A minimal 404 for an unknown or expired link (never reveals which).
fn not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        Html("<!doctype html><meta charset=utf-8><title>Link expired</title><body style=\"font-family:system-ui;padding:3rem;color:#333\"><h1>This link has expired</h1><p>Ask the sender to share the file again.</p></body>"),
    )
        .into_response()
}

/// Sanitize a filename for storage + `Content-Disposition`: strip any path and
/// control/quote characters, cap the length, and never allow an empty name.
fn sanitize_filename(name: Option<&str>) -> String {
    let raw = name.unwrap_or("").trim();
    let base = raw.rsplit(['/', '\\']).next().unwrap_or(raw);
    let cleaned: String = base
        .chars()
        .filter(|c| !c.is_control() && *c != '"' && *c != '\\')
        .take(MAX_FILENAME_LEN)
        .collect();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        "file".to_owned()
    } else {
        cleaned.to_owned()
    }
}

/// A safe media type: a plain `type/subtype`-ish token, else octet-stream. The
/// download is served as an attachment regardless, so this is belt-and-braces.
fn sanitize_content_type(raw: &str) -> String {
    let ct = raw.split(';').next().unwrap_or("").trim();
    let ok = !ct.is_empty()
        && ct.len() <= 128
        && ct.contains('/')
        && ct
            .bytes()
            .all(|b| b.is_ascii_graphic() && b != b'"' && b != b'\\');
    if ok {
        ct.to_ascii_lowercase()
    } else {
        "application/octet-stream".to_owned()
    }
}

fn now_epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::{sanitize_content_type, sanitize_filename};

    #[test]
    fn filename_strips_path_and_control_chars() {
        assert_eq!(sanitize_filename(Some("../../etc/passwd")), "passwd");
        assert_eq!(sanitize_filename(Some("a\r\nb\"c.pdf")), "abc.pdf");
        assert_eq!(sanitize_filename(Some("  ")), "file");
        assert_eq!(sanitize_filename(None), "file");
        assert_eq!(sanitize_filename(Some("report.xlsx")), "report.xlsx");
    }

    #[test]
    fn content_type_falls_back_when_junk() {
        assert_eq!(sanitize_content_type("image/png"), "image/png");
        assert_eq!(
            sanitize_content_type("application/pdf; charset=x"),
            "application/pdf"
        );
        assert_eq!(
            sanitize_content_type("not a type"),
            "application/octet-stream"
        );
        assert_eq!(sanitize_content_type(""), "application/octet-stream");
    }
}
