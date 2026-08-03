//! User self-service mailbox sharing (ADR 0017): a user manages who can access
//! THEIR OWN mailbox — Gmail-style "grant access to your account", no admin
//! needed. The owner is always the signed-in user, so a caller can only ever
//! share their own mailbox, and only with people in their own tenant.

use alo_store::{StoreError, UserId};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::{Json, body::Bytes};
use serde_json::{Value, json};

use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// `GET /jmap/delegates` — who can access the signed-in user's mailbox:
/// `{ delegates: [{ id, email, canWrite, sendMode }] }`.
pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let ts = state.store.for_tenant(account.tenant.clone());
    let list = ts
        .delegates_of(&account.user)
        .await
        .map_err(|_| Problem::server_error())?;
    let mut delegates = Vec::with_capacity(list.len());
    for (id, email, can_write, send_mode) in list {
        // Per-folder restriction (ADR 0017): empty = whole mailbox.
        let folders = ts
            .delegate_folders(&account.user, &UserId::new(&id))
            .await
            .unwrap_or_default();
        delegates.push(json!({
            "id": id, "email": email, "canWrite": can_write,
            "sendMode": send_mode, "folders": folders,
        }));
    }
    Ok(Json(json!({ "delegates": delegates })))
}

/// `POST /jmap/delegates` — grant a person access to your mailbox. Body
/// `{ email, canWrite?, sendMode? }`. The person is looked up by email within
/// your tenant (you can't enumerate users; you name one).
pub async fn grant(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let v: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let email = v
        .get("email")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|e| e.contains('@') && e.len() <= 320)
        .ok_or_else(|| Problem::with(StatusCode::BAD_REQUEST, "a valid email is required"))?;
    let can_write = v.get("canWrite").and_then(Value::as_bool).unwrap_or(true);
    let send_mode = v
        .get("sendMode")
        .and_then(Value::as_str)
        .unwrap_or("none")
        .to_owned();

    let ts = state.store.for_tenant(account.tenant.clone());
    let delegate = ts
        .user_by_email(email)
        .await
        .map_err(|_| Problem::with(StatusCode::NOT_FOUND, "no such person in your organization"))?;
    ts.grant_delegate(&account.user, &delegate, can_write, &send_mode)
        .await
        .map_err(delegate_err)?;
    // Optional per-folder restriction (ADR 0017): present → set it (empty array
    // clears back to whole-mailbox); absent → leave any existing one unchanged.
    if let Some(arr) = v.get("folders").and_then(Value::as_array) {
        let folders: Vec<String> = arr
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect();
        ts.set_delegate_folders(&account.user, &delegate, &folders)
            .await
            .map_err(|_| Problem::server_error())?;
    }
    // The delegate's shared-mailbox set changed — notify their live stream so it
    // mounts the mailbox and goes live without a refresh (ADR 0017).
    crate::push::notify_delegation_change(&state, &account.tenant, delegate.as_str()).await;
    Ok(Json(json!({ "ok": true })))
}

/// `POST /jmap/delegates/remove` — revoke a person's access to your mailbox.
/// Body `{ delegateId }`.
pub async fn revoke(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let v: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let delegate = v
        .get("delegateId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| Problem::with(StatusCode::BAD_REQUEST, "delegateId is required"))?;
    state
        .store
        .for_tenant(account.tenant.clone())
        .revoke_delegate(&account.user, &UserId::new(delegate))
        .await
        .map_err(|_| Problem::server_error())?;
    crate::push::notify_delegation_change(&state, &account.tenant, delegate).await;
    Ok(Json(json!({ "ok": true })))
}

/// A store error while granting maps to a client-safe reason (e.g. sharing with
/// yourself, or a malformed send mode).
fn delegate_err(e: StoreError) -> Problem {
    match e {
        StoreError::Conflict(_) => {
            Problem::with(StatusCode::BAD_REQUEST, "that isn't a valid share")
        }
        StoreError::NotFound => {
            Problem::with(StatusCode::NOT_FOUND, "no such person in your organization")
        }
        _ => Problem::server_error(),
    }
}
