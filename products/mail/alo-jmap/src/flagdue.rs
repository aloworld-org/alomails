//! Flag follow-up due-dates. A flagged message can carry a date the user means
//! to act by (Outlook-style "flag with reminder"). This is a plain per-message
//! timestamp, set here and surfaced as `alo:flagDue` on the Email object; the
//! client renders it and marks it overdue. There is no reminder/sweeper — the
//! date is displayed, not enforced.

use alo_store::{MessageId, StoreError};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::{Json, body::Bytes};
use serde_json::{Value, json};

use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// `POST /jmap/flag-due` — body `{"emailId": "...", "dueAt": <epoch>|null}`.
/// Sets (or clears, with null) the message's flag due-date. Setting a date also
/// flags the message; clearing the flag elsewhere is what removes it.
pub async fn set_flag_due(
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

    // `dueAt`: a Unix epoch (seconds) to set, or null/absent to clear. Reject a
    // present-but-non-integer value rather than silently clearing.
    let due = match req.get("dueAt") {
        None | Some(Value::Null) => None,
        Some(v) => Some(
            v.as_i64()
                .filter(|s| *s > 0)
                .ok_or_else(|| Problem::with(StatusCode::BAD_REQUEST, "dueAt must be an epoch"))?,
        ),
    };

    let mid = MessageId::new(email_id);
    match account.acc.set_flag_due(&mid, due).await {
        Ok(()) => {}
        Err(StoreError::NotFound) => {
            return Err(Problem::with(StatusCode::NOT_FOUND, "no such message"));
        }
        Err(_) => return Err(Problem::server_error()),
    }
    // A due-date implies the message is flagged; keep the two consistent.
    if due.is_some()
        && let Err(e) = account.acc.set_keyword(&mid, "$flagged", true).await
        && !matches!(e, StoreError::NotFound)
    {
        return Err(Problem::server_error());
    }
    Ok(Json(json!({ "ok": true })))
}
