//! Self-service app-password routes: the signed-in user creates, lists and
//! revokes their own app-specific passwords (mail M1.3). The `(tenant, user)`
//! every operation is scoped to comes from the bearer token and nowhere else,
//! so the routes cannot address another user's credentials at all. The secret
//! appears exactly once — in the create response — and is never logged,
//! stored, or retrievable again; everything after that is `alo-identity`'s
//! contract (argon2id hash at rest, constant-time verify at the legacy seam).

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::{Json, body::Bytes};
use serde_json::{Value, json};
use time::format_description::well_known::Rfc3339;

use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// An identity failure as the problem it is on the wire: store refusals keep
/// their meaning (404/409/422 through the one `StoreError` table), and a
/// crypto or config failure is ours — a bare 500 that leaks nothing.
fn identity_problem(error: alo_identity::IdentityError) -> Problem {
    match error {
        alo_identity::IdentityError::Store(e) => e.into(),
        _ => Problem::server_error(),
    }
}

/// An instant as the wire carries it, RFC 3339 in UTC; `null` for "never".
fn stamp(t: Option<time::OffsetDateTime>) -> Value {
    match t.map(|t| t.format(&Rfc3339)) {
        Some(Ok(s)) => json!(s),
        _ => Value::Null,
    }
}

/// `GET /settings/app-passwords` → `{ appPasswords: [{ id, name, createdAt,
/// lastUsedAt }] }` — the caller's own records, oldest first. Never a hash,
/// never a secret.
pub async fn list_app_passwords(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let rows = state
        .identity
        .list_app_passwords(&account.tenant, &account.user)
        .await
        .map_err(identity_problem)?;
    let list: Vec<Value> = rows
        .into_iter()
        .map(|r| {
            json!({
                "id": r.id.as_str(),
                "name": r.name,
                "createdAt": stamp(Some(r.created_at)),
                "lastUsedAt": stamp(r.last_used_at),
            })
        })
        .collect();
    Ok(Json(json!({ "appPasswords": list })))
}

/// `POST /settings/app-passwords` — create one. Body `{ name }` (the device
/// label, e.g. "Thunderbird on the desk machine"). Response `{ id, name,
/// secret }` — the only response the secret ever rides in; it is shown once
/// and cannot be fetched again. An empty or overlong name is a 422 and the
/// per-user cap a 409, both worded by the store.
pub async fn create_app_password(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let v: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let name = v.get("name").and_then(Value::as_str).unwrap_or("").trim();
    let (id, secret) = state
        .identity
        .create_app_password(&account.tenant, &account.user, name)
        .await
        .map_err(identity_problem)?;
    Ok(Json(json!({
        "id": id.as_str(),
        "name": name,
        "secret": secret.reveal(),
    })))
}

/// `DELETE /settings/app-passwords/{id}` — revoke one, immediately: the row
/// and its hash are gone, and the credential fails on the next connection.
/// A foreign or unknown id gets the same clean 404 — no oracle for "exists,
/// but not yours".
pub async fn revoke_app_password(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    state
        .identity
        .revoke_app_password(
            &account.tenant,
            &account.user,
            &alo_store::AppPasswordId::new(id),
        )
        .await
        .map_err(identity_problem)?;
    Ok(Json(json!({ "ok": true })))
}
