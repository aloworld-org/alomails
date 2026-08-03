//! `POST /import/imap` — the import wizard's backend: pull the user's
//! recent mail from a remote IMAP host into their alo Inbox. The heavy
//! lifting (SSRF-guarded connect, verified TLS, IMAP session, dedup +
//! ingest) is in [`crate::imap_import`]; this is the thin HTTP edge.

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::error::Problem;
use crate::imap_import::{self, ImapConfig, ImportError};
use crate::state::{AppState, authenticate};

/// The wizard's request body.
#[derive(Deserialize)]
struct ImportRequest {
    host: String,
    /// Defaults to 993 (implicit TLS) when absent.
    port: Option<u16>,
    username: String,
    password: String,
}

/// `POST /import/imap`. Runs a synchronous, bounded import and returns
/// `{imported, skipped, failed}`. The password is read from the body and
/// never logged or echoed.
pub async fn import(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: ImportRequest = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;

    let host = req.host.trim();
    if host.is_empty() || req.username.trim().is_empty() || req.password.is_empty() {
        return Err(Problem::with(
            StatusCode::BAD_REQUEST,
            "a server, username, and password are required",
        ));
    }
    let config = ImapConfig {
        host,
        port: req.port.unwrap_or(993),
        username: req.username.trim(),
        password: &req.password,
    };

    match imap_import::import(&account.acc, &config).await {
        Ok(outcome) => Ok(Json(json!({
            "imported": outcome.imported,
            "skipped": outcome.skipped,
            "failed": outcome.failed,
        }))),
        // Map the cause to an actionable status; never leak host internals.
        Err(err) => Err(match err {
            ImportError::Auth => Problem::with(
                StatusCode::UNAUTHORIZED,
                "The username or password was not accepted. For Gmail/Outlook, use an app password.",
            ),
            ImportError::Host => Problem::with(
                StatusCode::BAD_REQUEST,
                "That mail server address could not be reached.",
            ),
            ImportError::Connect | ImportError::Protocol => Problem::with(
                StatusCode::BAD_GATEWAY,
                "Could not complete the import from that mail server.",
            ),
        }),
    }
}
