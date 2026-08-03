//! Snooze endpoint: `POST /snooze` hides conversations until a chosen time.
//! Thin over `AccountStore::snooze`, which moves the messages to the account's
//! Snoozed mailbox and records the wake time; a background sweeper in `main`
//! returns them to the Inbox when due. Tenant/owner scoping is enforced by the
//! store's membership helpers.

use alo_store::{MailboxId, MessageId, StoreError};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::{Json, body::Bytes};
use serde_json::{Value, json};

use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// A single snooze request may not move more than this many messages.
pub const MAX_SNOOZE_IDS: usize = 200;

/// `POST /snooze` — `{ "ids": [...], "mailboxId": "...", "until": <unix seconds> }`.
pub async fn snooze(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;

    let ids: Vec<MessageId> = req
        .get("ids")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(MessageId::new)
                .collect()
        })
        .unwrap_or_default();
    let (Some(mailbox), Some(until)) = (
        req.get("mailboxId").and_then(Value::as_str),
        req.get("until").and_then(Value::as_i64),
    ) else {
        return Err(Problem::with(
            StatusCode::BAD_REQUEST,
            "mailboxId and until required",
        ));
    };
    if ids.is_empty() {
        return Err(Problem::with(StatusCode::BAD_REQUEST, "ids required"));
    }
    if ids.len() > MAX_SNOOZE_IDS {
        return Err(Problem::with(StatusCode::BAD_REQUEST, "too many ids"));
    }

    account
        .acc
        .snooze(&ids, &MailboxId::new(mailbox), until)
        .await
        .map_err(|e| match e {
            StoreError::NotFound => Problem::not_found(),
            _ => Problem::server_error(),
        })?;
    Ok(Json(json!({ "ok": true })))
}
