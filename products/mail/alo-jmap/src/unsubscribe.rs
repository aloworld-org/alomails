//! One-click unsubscribe (RFC 8058). The reading pane offers an Unsubscribe
//! action when a message carries `List-Unsubscribe` with one-click support; for
//! those, this route performs the single POST **server-side**. That is required
//! for two reasons: a browser cannot POST cross-origin to the sender's endpoint
//! (CORS), and the target URL comes from an untrusted email, so the request must
//! go through the egress SSRF guard rather than the user's browser.
//!
//! The URL is re-derived from the stored message here — never taken from the
//! client — so a caller cannot use this route as an open POST proxy. mailto: and
//! plain browsing-link unsubscribes are handled in the client (compose / open
//! tab); only the RFC 8058 one-click POST needs the server.

use std::time::Duration;

use alo_ai::egress::{self, EgressError};
use alo_store::{MessageId, StoreError};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::{Json, body::Bytes};
use serde_json::{Value, json};

use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// How long to wait on the sender's unsubscribe endpoint before giving up.
const UNSUB_TIMEOUT: Duration = Duration::from_secs(10);

/// `POST /jmap/unsubscribe` — body `{"emailId": "..."}`. Performs the RFC 8058
/// one-click unsubscribe for that message, if it supports it.
pub async fn unsubscribe(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let email_id = req
        .get("emailId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| Problem::with(StatusCode::BAD_REQUEST, "emailId is required"))?;

    // Re-read the message through the account door (a foreign/absent id is
    // NotFound) and re-parse its List-Unsubscribe — never trust a client URL.
    let raw = match account.acc.message_bytes(&MessageId::new(email_id)).await {
        Ok(bytes) => bytes,
        Err(StoreError::NotFound) => {
            return Err(Problem::with(StatusCode::NOT_FOUND, "no such message"));
        }
        Err(_) => return Err(Problem::server_error()),
    };
    let unsub = crate::mime_read::parse(&raw).unsubscribe;
    let url = match unsub {
        Some(u) if u.one_click => u.http.filter(|h| h.starts_with("https://")),
        _ => None,
    };
    let Some(url) = url else {
        return Err(Problem::with(
            StatusCode::BAD_REQUEST,
            "this message has no one-click unsubscribe",
        ));
    };

    // Guarded egress: https only, resolved host must be public, address pinned,
    // redirects disabled. A blocked/unreachable target is a plain bad-gateway —
    // never an oracle into what is internally reachable.
    let client = egress::guarded_client(&url, UNSUB_TIMEOUT)
        .await
        .map_err(egress_problem)?;
    let resp = client
        .post(&url)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body("List-Unsubscribe=One-Click")
        .send()
        .await
        .map_err(|_| Problem::with(StatusCode::BAD_GATEWAY, "the unsubscribe request failed"))?;

    if resp.status().is_success() {
        Ok(Json(json!({ "unsubscribed": true })))
    } else {
        Err(Problem::with(
            StatusCode::BAD_GATEWAY,
            "the sender's server rejected the unsubscribe",
        ))
    }
}

/// Map an egress refusal to a bad-gateway (both variants read the same to the
/// caller — no signal about internal reachability).
fn egress_problem(_e: EgressError) -> Problem {
    Problem::with(
        StatusCode::BAD_GATEWAY,
        "the unsubscribe target is unreachable",
    )
}
