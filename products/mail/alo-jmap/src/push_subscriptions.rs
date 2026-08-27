//! Self-service Web Push subscription routes (mail M5.3): the signed-in
//! user's browser registers, lists and removes its own push subscriptions.
//! The `(tenant, user)` every operation is scoped to comes from the bearer
//! token and nowhere else. The endpoint URL a browser hands us is a
//! capability — it is stored, shown back only to its owner, and never
//! logged; the key material goes in and never comes back out at all (the
//! dispatcher reads it store-side to encrypt toward the browser).

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::{Json, body::Bytes};
use serde_json::{Value, json};
use time::format_description::well_known::Rfc3339;

use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// `GET /settings/push-subscriptions` → `{ enabled, publicKey,
/// subscriptions: [{ id, endpoint, createdAt }] }`. `enabled` says whether
/// this deployment holds a VAPID key at all — the settings screen shows the
/// opt-in only when subscribing could work — and `publicKey` is what the
/// browser passes to `pushManager.subscribe` as `applicationServerKey`.
pub async fn list_push_subscriptions(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let rows = state
        .store
        .for_tenant(account.tenant.clone())
        .list_push_subscriptions(&account.user)
        .await
        .map_err(Problem::from)?;
    let list: Vec<Value> = rows
        .into_iter()
        .map(|r| {
            json!({
                "id": r.id.as_str(),
                "endpoint": r.endpoint,
                "createdAt": r.created_at.format(&Rfc3339).ok(),
            })
        })
        .collect();
    let public_key = state.web_push.as_ref().map(|wp| wp.public_key_b64());
    Ok(Json(json!({
        "enabled": public_key.is_some(),
        "publicKey": public_key,
        "subscriptions": list,
    })))
}

/// `POST /settings/push-subscriptions` — register (or refresh) this
/// browser's subscription. Body is the W3C `PushSubscription.toJSON()`
/// shape: `{ endpoint, keys: { p256dh, auth } }`. Response `{ id }`.
/// A non-HTTPS endpoint (loopback excepted, for local stacks) is a 422:
/// the server will POST to this URL later, and a URL it would refuse to
/// POST to should be refused now, not stored and silently dropped.
pub async fn create_push_subscription(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let v: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let endpoint = v
        .get("endpoint")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let keys = v.get("keys").cloned().unwrap_or(Value::Null);
    let p256dh = keys
        .get("p256dh")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let auth = keys
        .get("auth")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let parsed = reqwest::Url::parse(endpoint).map_err(|_| {
        Problem::with(
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            "the push endpoint is not a URL",
        )
    })?;
    if !crate::push_notify::endpoint_allowed(&parsed) {
        return Err(Problem::with(
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            "a push endpoint must be an https URL",
        ));
    }
    let id = state
        .store
        .for_tenant(account.tenant.clone())
        .create_push_subscription(&account.user, endpoint, p256dh, auth)
        .await
        .map_err(Problem::from)?;
    Ok(Json(json!({ "id": id.as_str() })))
}

/// `DELETE /settings/push-subscriptions/{id}` — remove one device,
/// immediately: pushes to it stop with the row. A foreign or unknown id
/// gets the same clean 404 — no oracle for "exists, but not yours".
pub async fn delete_push_subscription(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    state
        .store
        .for_tenant(account.tenant.clone())
        .delete_push_subscription(&account.user, &alo_store::PushSubscriptionId::new(id))
        .await
        .map_err(Problem::from)?;
    Ok(Json(json!({ "ok": true })))
}
