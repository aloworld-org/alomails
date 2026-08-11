//! Admin console endpoints (tenant-admin only). The first surface is AI
//! provider management (ADR 0011, extended): configure OpenAI-compatible
//! backends (self-hosted Ollama, OpenAI, a custom endpoint), pick the default,
//! and test connectivity. Every handler gates on `Account::require_admin`.
//!
//! Secrets never leave the server: a provider's API key is stored but only its
//! presence (`hasKey`) is returned, and it is never logged.

use alo_store::{
    ALL_MODULES, AiProviderRow, AppModule, GroupId, Page, StoreError, TenantRole, UserId,
};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::{Json, body::Bytes};
use serde_json::{Value, json};

use crate::error::Problem;
use crate::jtypes::utc_date;
use crate::state::{Account, AppState, authenticate};

/// Record an audit entry for an admin mutation, best-effort: a failed audit
/// write is logged but never fails the action it describes (ADR 0012). Never
/// carries a secret or a message body — an actor, a verb, and a target id.
async fn audit(
    state: &AppState,
    account: &Account,
    action: &str,
    target: Option<&str>,
    detail: Option<&str>,
) {
    if let Err(error) = state
        .store
        .record_audit(
            &account.tenant,
            Some(&account.user),
            None,
            action,
            target,
            detail,
        )
        .await
    {
        tracing::warn!(%error, action, "audit write failed");
    }
}

/// Map a store error to a client problem (admin writes): conflicts (e.g. a
/// duplicate email) are 409, everything else a 500 with no leaked detail.
fn store_admin_err(e: StoreError) -> Problem {
    match e {
        StoreError::Conflict(_) => Problem::with(StatusCode::CONFLICT, "already exists"),
        StoreError::NotFound => Problem::not_found(),
        _ => Problem::server_error(),
    }
}

/// A stored provider as JSON — the API key is reduced to `hasKey`.
fn provider_json(p: &AiProviderRow) -> Value {
    json!({
        "id": p.id,
        "kind": p.kind,
        "label": p.label,
        "baseUrl": p.base_url,
        "model": p.model,
        "enabled": p.enabled,
        "isDefault": p.is_default,
        "hasKey": p.api_key.as_ref().is_some_and(|k| !k.is_empty()),
    })
}

/// `GET /admin/ai/providers` → `{ "providers": [...] }` (keys redacted).
pub async fn list_providers(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_admin()?;
    let providers = account
        .acc
        .list_ai_providers()
        .await
        .map_err(|_| Problem::server_error())?;
    let list: Vec<Value> = providers.iter().map(provider_json).collect();
    Ok(Json(json!({ "providers": list })))
}

/// `POST /admin/ai/providers` — create or update one provider. Body:
/// `{ id, kind, label, baseUrl, model, enabled, apiKey? }`. A `null`/absent
/// `apiKey` on update keeps the stored key. `id` is client-supplied (a UUID for
/// new providers); the store's tenant guard makes a foreign id a no-op.
pub async fn upsert_provider(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_admin()?;
    let v: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;

    let id = str_field(&v, "id").ok_or_else(|| bad("id required"))?;
    let kind = str_field(&v, "kind").ok_or_else(|| bad("kind required"))?;
    let label = str_field(&v, "label").unwrap_or_default();
    let base_url = str_field(&v, "baseUrl").unwrap_or_default();
    let model = str_field(&v, "model").unwrap_or_default();
    let enabled = v.get("enabled").and_then(Value::as_bool).unwrap_or(false);
    // Only overwrite the key when a non-empty one is supplied.
    let api_key = v
        .get("apiKey")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty());

    account
        .acc
        .upsert_ai_provider(&id, &kind, &label, &base_url, &model, api_key, enabled)
        .await
        .map_err(|_| Problem::server_error())?;
    Ok(Json(json!({ "id": id })))
}

/// `POST /admin/ai/providers/default` — make one provider the tenant default.
/// Body: `{ id }`.
pub async fn set_default(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_admin()?;
    let v: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let id = str_field(&v, "id").ok_or_else(|| bad("id required"))?;
    account
        .acc
        .set_default_ai_provider(&id)
        .await
        .map_err(store_admin_err)?;
    Ok(Json(json!({ "id": id })))
}

/// `DELETE /admin/ai/providers/{id}` — remove a provider.
pub async fn delete_provider(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_admin()?;
    account
        .acc
        .delete_ai_provider(&id)
        .await
        .map_err(|_| Problem::server_error())?;
    Ok(Json(json!({ "id": id })))
}

/// `POST /admin/ai/test` — test connectivity to a backend without saving it.
/// Body: `{ baseUrl, apiKey? }` → `{ ok, models }` or a 502/400.
pub async fn test_connection(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_admin()?;
    let v: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let base_url = str_field(&v, "baseUrl").ok_or_else(|| bad("baseUrl required"))?;
    let api_key = v.get("apiKey").and_then(Value::as_str);
    match alo_ai::check(&base_url, api_key).await {
        Ok(models) => Ok(Json(json!({ "ok": true, "models": models }))),
        Err(_) => Err(Problem::with(StatusCode::BAD_GATEWAY, "ai-backend")),
    }
}

fn str_field(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(Value::as_str)
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

fn bad(detail: &'static str) -> Problem {
    Problem::with(StatusCode::BAD_REQUEST, detail)
}

/// Whether tenant→domain ownership is enforced on address assignment (ADR
/// 0012). Off by default so a single-tenant deployment is unaffected; flipped
/// on when a deployment hosts mutually-distrusting tenants.
fn domain_ownership_enforced() -> bool {
    std::env::var("ALO_ENFORCE_DOMAIN_OWNERSHIP")
        .map(|v| matches!(v.trim(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false)
}

/// Reject assigning `address` unless the acting tenant owns a verified domain
/// matching its host (ADR 0012 security spine). Inert when enforcement is off
/// or no domains are registered (single-tenant/dev). The address must already
/// be shaped `local@domain`.
async fn require_domain_owned(
    state: &AppState,
    tenant: &alo_store::TenantId,
    address: &str,
) -> Result<(), Problem> {
    if !domain_ownership_enforced() {
        return Ok(());
    }
    if !state
        .store
        .any_domains_registered()
        .await
        .map_err(|_| Problem::server_error())?
    {
        return Ok(());
    }
    let domain = address
        .rsplit('@')
        .next()
        .unwrap_or("")
        .trim()
        .to_lowercase();
    if domain.is_empty() {
        return Err(bad("valid email required"));
    }
    if state
        .store
        .tenant_owns_verified_domain(tenant, &domain)
        .await
        .map_err(|_| Problem::server_error())?
    {
        Ok(())
    } else {
        Err(Problem::with(
            StatusCode::FORBIDDEN,
            "domain-not-owned-or-unverified",
        ))
    }
}

// ---- users & mailboxes -------------------------------------------------

const MIN_PASSWORD: usize = 8;

/// `GET /admin/users` → `{ users: [...] }` with per-user usage and aliases.
pub async fn list_users(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_admin()?;
    let ts = state.store.for_tenant(account.tenant.clone());
    let users = ts.list_users().await.map_err(|_| Problem::server_error())?;
    // Every role grant in the tenant in ONE read, not one per row: the console
    // lists a whole company, and a query per user is how a page of forty people
    // becomes forty round trips (B4.12).
    let grants = ts
        .role_grants()
        .await
        .map_err(|_| Problem::server_error())?;
    let mut list = Vec::with_capacity(users.len());
    for u in &users {
        let aliases = ts
            .aliases_of(&UserId::new(u.id.clone()))
            .await
            .unwrap_or_default();
        let roles: Vec<&str> = grants
            .iter()
            .filter(|(holder, _)| holder.as_str() == u.id)
            .map(|(_, role)| role.as_str())
            .collect();
        list.push(json!({
            "id": u.id,
            "email": u.email,
            "isAdmin": u.is_admin,
            "roles": roles,
            "createdAt": utc_date(u.created_at),
            "messageCount": u.message_count,
            "storageBytes": u.storage_bytes,
            "aliases": aliases,
        }));
    }
    Ok(Json(json!({ "users": list })))
}

/// `POST /admin/users` — create a user. Body `{ email, password }`. The new
/// user gets an inbox so they can receive mail immediately.
pub async fn create_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_admin()?;
    let v: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let email = str_field(&v, "email").ok_or_else(|| bad("email required"))?;
    let password = v.get("password").and_then(Value::as_str).unwrap_or("");
    if !email.contains('@') {
        return Err(bad("valid email required"));
    }
    if password.len() < MIN_PASSWORD {
        return Err(bad("password too short"));
    }
    require_domain_owned(&state, &account.tenant, &email).await?;
    let ts = state.store.for_tenant(account.tenant.clone());
    let user = ts.create_user(&email).await.map_err(store_admin_err)?;
    // Password + inbox are separate writes; if either fails, roll the user row
    // back so we never leave a half-created account that owns the email but
    // cannot log in or receive mail ("done means the full path works").
    let provisioned = async {
        state
            .identity
            .set_password(&account.tenant, &user, &email, password)
            .await
            .map_err(|_| Problem::server_error())?;
        state
            .store
            .for_account(account.tenant.clone(), user.clone())
            .inbox()
            .await
            .map_err(|_| Problem::server_error())?;
        Ok::<(), Problem>(())
    }
    .await;
    if let Err(e) = provisioned {
        if let Err(cleanup) = ts.delete_user(&user).await {
            tracing::warn!(%cleanup, "failed to roll back a half-created user");
        }
        return Err(e);
    }
    audit(&state, &account, "user.create", Some(&email), None).await;
    Ok(Json(json!({ "id": user.as_str() })))
}

/// `POST /admin/users/password` — reset a user's password. Body
/// `{ userId, password }`.
pub async fn reset_password(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_admin()?;
    let v: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let user_id = str_field(&v, "userId").ok_or_else(|| bad("userId required"))?;
    let password = v.get("password").and_then(Value::as_str).unwrap_or("");
    if password.len() < MIN_PASSWORD {
        return Err(bad("password too short"));
    }
    let ts = state.store.for_tenant(account.tenant.clone());
    let user = UserId::new(user_id);
    let email = ts
        .email_of(&user)
        .await
        .map_err(|_| Problem::server_error())?
        .ok_or_else(Problem::not_found)?;
    state
        .identity
        .set_password(&account.tenant, &user, &email, password)
        .await
        .map_err(|_| Problem::server_error())?;
    Ok(Json(json!({ "ok": true })))
}

/// `POST /admin/users/admin` — set/clear a user's admin flag. Body
/// `{ userId, isAdmin }`. An admin may not remove their own admin (self-lockout).
pub async fn set_user_admin(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_admin()?;
    let v: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let user_id = str_field(&v, "userId").ok_or_else(|| bad("userId required"))?;
    let is_admin = v.get("isAdmin").and_then(Value::as_bool).unwrap_or(false);
    if user_id == account.user.as_str() && !is_admin {
        return Err(Problem::with(
            StatusCode::CONFLICT,
            "cannot remove your own admin",
        ));
    }
    state
        .store
        .for_tenant(account.tenant.clone())
        .set_admin(&UserId::new(user_id.clone()), is_admin)
        .await
        .map_err(|_| Problem::server_error())?;
    audit(
        &state,
        &account,
        "user.admin",
        Some(&user_id),
        Some(if is_admin { "granted" } else { "revoked" }),
    )
    .await;
    Ok(Json(json!({ "ok": true })))
}

/// `POST /admin/users/roles` — grant or revoke a tenant-wide scoped role
/// (ADR 0035, B4.12). Body `{ userId, role, granted }`.
///
/// Separate from `/admin/users/admin` rather than a field beside `isAdmin`,
/// because they are different kinds of fact: the admin flag is the console, a
/// role is a scope. Granting is idempotent, and so is revoking — the caller's
/// intent is a state, not an event.
///
/// # Errors
/// `401` without a valid bearer token; `403` for a non-admin; `422` for a
/// missing `userId` or a role this build does not know; `404` when the user is
/// not a member of this tenant — including when they are a member of another
/// one, which is the same answer an id that was never issued gets.
pub async fn set_user_role(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_admin()?;
    let v: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let user_id = str_field(&v, "userId").ok_or_else(|| bad("userId required"))?;
    let role_name = str_field(&v, "role").ok_or_else(|| bad("role required"))?;
    // The store's own message names the accepted set, so the refusal tells the
    // caller what to send instead of that a word was wrong.
    let role = TenantRole::parse(&role_name)
        .map_err(|e| Problem::with(StatusCode::UNPROCESSABLE_ENTITY, e.to_string()))?;
    let granted = v.get("granted").and_then(Value::as_bool).unwrap_or(false);
    let ts = state.store.for_tenant(account.tenant.clone());
    let user = UserId::new(user_id.clone());
    if granted {
        ts.grant_role(&user, role, &account.user)
            .await
            .map_err(store_admin_err)?;
    } else {
        ts.revoke_role(&user, role).await.map_err(store_admin_err)?;
    }
    audit(
        &state,
        &account,
        "user.role",
        Some(&user_id),
        Some(&format!(
            "{} {role}",
            if granted { "granted" } else { "revoked" }
        )),
    )
    .await;
    Ok(Json(json!({ "ok": true })))
}

/// `GET /admin/users/{id}/modules` — which apps this person has.
///
/// Answers the whole picture rather than just the denials, because that is
/// what the console renders: every switchable module, and whether its switch
/// is on. A client that had to subtract one list from another to draw a row of
/// checkboxes would be reimplementing this endpoint, slightly differently.
///
/// Reports what was **stored**, not what applies. A tenant admin is never
/// denied at the gate, but their switches are shown as they were set — an
/// admin who is looking at their own row should see what they did, and
/// `may_open` is the thing that ignores it.
///
/// # Errors
/// `401` without a valid bearer token; `403` for a non-admin; `404` when the
/// user is not a member of this tenant.
pub async fn user_modules(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_admin()?;
    let denied = state
        .store
        .for_tenant(account.tenant.clone())
        .denied_modules(&UserId::new(id))
        .await
        .map_err(store_admin_err)?;
    Ok(Json(json!({
        "modules": ALL_MODULES
            .iter()
            .map(|module| json!({
                "id": module.as_str(),
                "allowed": !denied.contains(module),
            }))
            .collect::<Vec<_>>()
    })))
}

/// `POST /admin/users/modules` — switch one app on or off for one person.
/// Body `{ userId, module, allowed }`.
///
/// One switch per call rather than a whole set, so two administrators editing
/// the same person do not silently undo each other: a PUT of the full list
/// would make the last writer's snapshot win, including the switches they
/// never touched. Idempotent in both directions — the caller's intent is a
/// state, not an event.
///
/// # Errors
/// `401` without a valid bearer token; `403` for a non-admin; `422` for a
/// missing `userId` or a module this build does not know; `404` when the user
/// is not a member of this tenant.
pub async fn set_user_module(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_admin()?;
    let v: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let user_id = str_field(&v, "userId").ok_or_else(|| bad("userId required"))?;
    let module_name = str_field(&v, "module").ok_or_else(|| bad("module required"))?;
    // The store's own message names the accepted set, so the refusal tells the
    // caller what to send instead of that a word was wrong.
    let module = AppModule::parse(&module_name)
        .map_err(|e| Problem::with(StatusCode::UNPROCESSABLE_ENTITY, e.to_string()))?;
    let allowed = v.get("allowed").and_then(Value::as_bool).unwrap_or(true);
    state
        .store
        .for_tenant(account.tenant.clone())
        .set_module_access(
            &UserId::new(user_id.clone()),
            module,
            allowed,
            &account.user,
        )
        .await
        .map_err(store_admin_err)?;
    audit(
        &state,
        &account,
        "user.module",
        Some(&user_id),
        Some(&format!(
            "{} {module}",
            if allowed { "granted" } else { "removed" }
        )),
    )
    .await;
    Ok(Json(json!({ "ok": true })))
}

/// `DELETE /admin/users/{id}` — delete a user. An admin cannot delete themself.
pub async fn delete_user(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_admin()?;
    if id == account.user.as_str() {
        return Err(Problem::with(
            StatusCode::CONFLICT,
            "cannot delete yourself",
        ));
    }
    state
        .store
        .for_tenant(account.tenant.clone())
        .delete_user(&UserId::new(id.clone()))
        .await
        .map_err(|_| Problem::server_error())?;
    audit(&state, &account, "user.delete", Some(&id), None).await;
    Ok(Json(json!({ "ok": true })))
}

/// `POST /admin/users/alias` — add an alias to a user. Body `{ userId, address }`.
pub async fn add_alias(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_admin()?;
    let v: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let user_id = str_field(&v, "userId").ok_or_else(|| bad("userId required"))?;
    let address = str_field(&v, "address").ok_or_else(|| bad("address required"))?;
    if !address.contains('@') {
        return Err(bad("valid address required"));
    }
    require_domain_owned(&state, &account.tenant, &address).await?;
    state
        .store
        .for_tenant(account.tenant.clone())
        .add_alias(&UserId::new(user_id), &address)
        .await
        .map_err(store_admin_err)?;
    audit(&state, &account, "alias.add", Some(&address), None).await;
    Ok(Json(json!({ "ok": true })))
}

/// `POST /admin/users/alias/remove` — remove an alias. Body `{ address }`.
pub async fn remove_alias(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_admin()?;
    let v: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let address = str_field(&v, "address").ok_or_else(|| bad("address required"))?;
    state
        .store
        .for_tenant(account.tenant.clone())
        .remove_alias(&address)
        .await
        .map_err(|_| Problem::server_error())?;
    audit(&state, &account, "alias.remove", Some(&address), None).await;
    Ok(Json(json!({ "ok": true })))
}

// ---- groups & lists ----------------------------------------------------

/// `GET /admin/groups` → `{ groups: [...] }` with each group's members and
/// optional distribution-list address.
pub async fn list_groups(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_admin()?;
    let ts = state.store.for_tenant(account.tenant.clone());
    let groups = ts
        .list_groups()
        .await
        .map_err(|_| Problem::server_error())?;
    let mut list = Vec::with_capacity(groups.len());
    for g in &groups {
        let members = ts
            .group_members_detailed(&GroupId::new(g.id.clone()))
            .await
            .unwrap_or_default();
        list.push(json!({
            "id": g.id,
            "name": g.name,
            "address": g.address,
            "memberCount": g.member_count,
            "members": members
                .into_iter()
                .map(|(id, email)| json!({ "id": id, "email": email }))
                .collect::<Vec<_>>(),
        }));
    }
    Ok(Json(json!({ "groups": list })))
}

/// `POST /admin/groups` — create a group. Body `{ name }`.
pub async fn create_group(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_admin()?;
    let v: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let name = str_field(&v, "name").ok_or_else(|| bad("name required"))?;
    let id = state
        .store
        .for_tenant(account.tenant.clone())
        .create_group(&name)
        .await
        .map_err(store_admin_err)?;
    audit(&state, &account, "group.create", Some(&name), None).await;
    Ok(Json(json!({ "id": id.as_str() })))
}

/// `DELETE /admin/groups/{id}` — delete a group (its memberships cascade).
pub async fn delete_group(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_admin()?;
    state
        .store
        .for_tenant(account.tenant.clone())
        .delete_group(&GroupId::new(id.clone()))
        .await
        .map_err(|_| Problem::server_error())?;
    audit(&state, &account, "group.delete", Some(&id), None).await;
    Ok(Json(json!({ "ok": true })))
}

/// `POST /admin/groups/name` — rename a group. Body `{ groupId, name }`.
pub async fn rename_group(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_admin()?;
    let v: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let group_id = str_field(&v, "groupId").ok_or_else(|| bad("groupId required"))?;
    let name = str_field(&v, "name").ok_or_else(|| bad("name required"))?;
    state
        .store
        .for_tenant(account.tenant.clone())
        .rename_group(&GroupId::new(group_id.clone()), &name)
        .await
        .map_err(store_admin_err)?;
    audit(
        &state,
        &account,
        "group.rename",
        Some(&group_id),
        Some(&name),
    )
    .await;
    Ok(Json(json!({ "ok": true })))
}

/// `POST /admin/groups/address` — set or clear a group's list address. Body
/// `{ groupId, address? }`. An empty/absent address turns the list off.
pub async fn set_group_address(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_admin()?;
    let v: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let group_id = str_field(&v, "groupId").ok_or_else(|| bad("groupId required"))?;
    let address = str_field(&v, "address");
    if let Some(a) = &address {
        if !a.contains('@') {
            return Err(bad("valid address required"));
        }
        require_domain_owned(&state, &account.tenant, a).await?;
    }
    state
        .store
        .for_tenant(account.tenant.clone())
        .set_group_address(&GroupId::new(group_id.clone()), address.as_deref())
        .await
        .map_err(store_admin_err)?;
    audit(
        &state,
        &account,
        "group.address",
        Some(&group_id),
        Some(address.as_deref().unwrap_or("cleared")),
    )
    .await;
    Ok(Json(json!({ "ok": true })))
}

/// `POST /admin/groups/members` — add a user to a group. Body `{ groupId, userId }`.
pub async fn add_group_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_admin()?;
    let v: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let group_id = str_field(&v, "groupId").ok_or_else(|| bad("groupId required"))?;
    let user_id = str_field(&v, "userId").ok_or_else(|| bad("userId required"))?;
    state
        .store
        .for_tenant(account.tenant.clone())
        .add_group_member(&GroupId::new(group_id), &UserId::new(user_id))
        .await
        .map_err(store_admin_err)?;
    Ok(Json(json!({ "ok": true })))
}

/// `POST /admin/groups/members/remove` — remove a user from a group. Body
/// `{ groupId, userId }`.
pub async fn remove_group_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_admin()?;
    let v: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let group_id = str_field(&v, "groupId").ok_or_else(|| bad("groupId required"))?;
    let user_id = str_field(&v, "userId").ok_or_else(|| bad("userId required"))?;
    state
        .store
        .for_tenant(account.tenant.clone())
        .remove_group_member(&GroupId::new(group_id), &UserId::new(user_id))
        .await
        .map_err(|_| Problem::server_error())?;
    Ok(Json(json!({ "ok": true })))
}

// ---- mailbox delegation (ADR 0017) --------------------------------------

/// `GET /admin/delegates/{ownerId}` — the users granted access to that user's
/// mailbox: `{ delegates: [{ id, email, canSend }] }`.
pub async fn list_delegates(
    State(state): State<AppState>,
    Path(owner_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_admin()?;
    let owner = UserId::new(owner_id);
    let ts = state.store.for_tenant(account.tenant.clone());
    let list = ts.delegates_of(&owner).await.map_err(store_admin_err)?;
    let mut delegates = Vec::with_capacity(list.len());
    for (id, email, can_write, send_mode) in list {
        let folders = ts
            .delegate_folders(&owner, &UserId::new(&id))
            .await
            .unwrap_or_default();
        delegates.push(json!({
            "id": id, "email": email, "canWrite": can_write,
            "sendMode": send_mode, "folders": folders,
        }));
    }
    Ok(Json(json!({ "delegates": delegates })))
}

/// `GET /admin/users/{id}/mailboxes` — a user's folders (id + name), for the
/// admin per-folder delegation picker. Admin-only; tenant-scoped, so it can
/// only ever list folders of a user in the admin's own tenant.
pub async fn user_mailboxes(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_admin()?;
    let acc = state
        .store
        .for_account(account.tenant.clone(), UserId::new(user_id));
    let boxes = acc
        .mailboxes(Page::first(alo_store::MAX_PAGE))
        .await
        .map_err(store_admin_err)?;
    let mailboxes: Vec<Value> = boxes
        .iter()
        .map(|m| {
            json!({
                "id": m.id.as_str(),
                "name": m.name,
                "role": m.role,
                "parentId": m.parent_id.as_ref().map(|p| p.as_str()),
            })
        })
        .collect();
    Ok(Json(json!({ "mailboxes": mailboxes })))
}

/// `POST /admin/delegates` — grant a delegate access to an owner's mailbox.
/// Body `{ ownerId, delegateId, canSend? }`. Idempotent (re-grant updates the
/// send flag).
pub async fn grant_delegate(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_admin()?;
    let v: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let owner = str_field(&v, "ownerId").ok_or_else(|| bad("ownerId required"))?;
    let delegate = str_field(&v, "delegateId").ok_or_else(|| bad("delegateId required"))?;
    let can_write = v.get("canWrite").and_then(Value::as_bool).unwrap_or(true);
    let send_mode = str_field(&v, "sendMode").unwrap_or_else(|| "none".to_owned());
    let ts = state.store.for_tenant(account.tenant.clone());
    ts.grant_delegate(
        &UserId::new(owner.clone()),
        &UserId::new(delegate.clone()),
        can_write,
        &send_mode,
    )
    .await
    .map_err(store_admin_err)?;
    // Optional per-folder restriction (ADR 0017): present → set (empty clears).
    if let Some(arr) = v.get("folders").and_then(Value::as_array) {
        let folders: Vec<String> = arr
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect();
        ts.set_delegate_folders(
            &UserId::new(owner.clone()),
            &UserId::new(delegate.clone()),
            &folders,
        )
        .await
        .map_err(store_admin_err)?;
    }
    crate::push::notify_delegation_change(&state, &account.tenant, &delegate).await;
    audit(
        &state,
        &account,
        "delegate.grant",
        Some(&owner),
        Some(&delegate),
    )
    .await;
    Ok(Json(json!({ "ok": true })))
}

/// `POST /admin/delegates/remove` — revoke a delegate's access. Body
/// `{ ownerId, delegateId }`.
pub async fn revoke_delegate(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_admin()?;
    let v: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let owner = str_field(&v, "ownerId").ok_or_else(|| bad("ownerId required"))?;
    let delegate = str_field(&v, "delegateId").ok_or_else(|| bad("delegateId required"))?;
    state
        .store
        .for_tenant(account.tenant.clone())
        .revoke_delegate(&UserId::new(owner.clone()), &UserId::new(delegate.clone()))
        .await
        .map_err(|_| Problem::server_error())?;
    crate::push::notify_delegation_change(&state, &account.tenant, &delegate).await;
    audit(
        &state,
        &account,
        "delegate.revoke",
        Some(&owner),
        Some(&delegate),
    )
    .await;
    Ok(Json(json!({ "ok": true })))
}

// ---- domains (tenant-admin; ADR 0012) ----------------------------------

/// The DNS label under a domain where the ownership token is published.
const VERIFY_PREFIX: &str = "_alo-verify";

/// A domain row as JSON for the tenant-admin domains page. `verifyRecord` is
/// exactly the DNS TXT record to publish to prove ownership.
fn domain_json(d: &alo_store::DomainRow) -> Value {
    json!({
        "domain": d.domain,
        "tenantId": d.tenant_id,
        "verified": d.verified_at.is_some(),
        "verifiedAt": d.verified_at.map(utc_date),
        "verifyRecord": {
            "name": format!("{VERIFY_PREFIX}.{}", d.domain),
            "type": "TXT",
            "value": d.verify_token,
        },
        "createdAt": utc_date(d.created_at),
    })
}

/// Ensure a verified domain has an active DKIM signing key (ADR 0014).
/// Best-effort: a keygen or store failure is logged, never fails the verify.
async fn ensure_dkim_key(state: &AppState, tenant: &alo_store::TenantId, domain: &str) {
    match state.store.active_dkim_material(domain).await {
        Ok(Some(_)) => return, // already has an active key
        Ok(None) => {}
        Err(_) => return,
    }
    let Some(key) = alo_auth_mail::dkim::keystore::generate_ed25519_key() else {
        tracing::warn!(domain, "DKIM key generation failed");
        return;
    };
    if let Err(error) = state
        .store
        .install_active_dkim_key(
            tenant,
            domain,
            &key.selector,
            key.seed.as_ref(),
            &key.public_raw,
        )
        .await
    {
        tracing::warn!(%error, domain, "DKIM key install failed");
    }
}

/// The active DKIM DNS record for `domain` (ADR 0014), or `null` if the domain
/// has no key yet: `<selector>._domainkey.<domain>` TXT = the Ed25519 key.
async fn dkim_record_json(ts: &alo_store::TenantStore, domain: &str) -> Value {
    let keys = ts.list_dkim_keys(domain).await.unwrap_or_default();
    match keys.iter().find(|k| k.active) {
        Some(k) => json!({
            "name": format!("{}._domainkey.{}", k.selector, domain),
            "type": "TXT",
            "value": alo_auth_mail::dkim::keystore::ed25519_txt_record(&k.public_raw),
            "selector": k.selector,
        }),
        None => Value::Null,
    }
}

/// `GET /admin/domains` → `{ domains: [...] }` — this tenant's own domains, each
/// with its active DKIM record to publish.
pub async fn list_domains(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_admin()?;
    let ts = state.store.for_tenant(account.tenant.clone());
    let domains = ts
        .list_domains()
        .await
        .map_err(|_| Problem::server_error())?;
    let mut list = Vec::with_capacity(domains.len());
    for d in &domains {
        let mut obj = domain_json(d);
        obj["dkim"] = dkim_record_json(&ts, &d.domain).await;
        list.push(obj);
    }
    Ok(Json(json!({ "domains": list })))
}

/// `POST /admin/domains` — register a domain to this tenant (unverified). Body
/// `{ domain }`. Returns the DNS record to publish for verification.
pub async fn create_domain(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_admin()?;
    let v: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let domain = str_field(&v, "domain").ok_or_else(|| bad("domain required"))?;
    if !domain.contains('.') {
        return Err(bad("valid domain required"));
    }
    let row = state
        .store
        .create_domain(&account.tenant, &domain)
        .await
        .map_err(store_admin_err)?;
    audit(&state, &account, "domain.register", Some(&domain), None).await;
    Ok(Json(domain_json(&row)))
}

/// `POST /admin/domains/verify` — check the DNS TXT proof for one of this
/// tenant's domains and mark it verified if present. Body `{ domain }`.
pub async fn verify_domain(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_admin()?;
    let v: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let domain = str_field(&v, "domain").ok_or_else(|| bad("domain required"))?;
    // The domain must belong to THIS tenant — never verify another tenant's.
    let record = state
        .store
        .domain_record(&domain)
        .await
        .map_err(|_| Problem::server_error())?
        .filter(|r| r.tenant_id == account.tenant.as_str())
        .ok_or_else(Problem::not_found)?;

    let name = format!("{VERIFY_PREFIX}.{}", record.domain);
    let resolver = crate::security::build_resolver().ok_or_else(Problem::server_error)?;
    let found = crate::security::txt_records(&resolver, &name)
        .await
        .iter()
        .any(|r| r.trim() == record.verify_token);
    if !found {
        return Ok(Json(json!({ "domain": record.domain, "verified": false })));
    }
    state
        .store
        .set_domain_verified(&record.domain)
        .await
        .map_err(store_admin_err)?;
    // A verified domain gets its own DKIM signing key (ADR 0014).
    ensure_dkim_key(&state, &account.tenant, &record.domain).await;
    audit(
        &state,
        &account,
        "domain.verify",
        Some(&record.domain),
        None,
    )
    .await;
    Ok(Json(json!({ "domain": record.domain, "verified": true })))
}

/// `POST /admin/domains/delete` — remove one of this tenant's domains. Body
/// `{ domain }`. Scoped: a tenant admin cannot remove another tenant's domain.
pub async fn delete_domain(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_admin()?;
    let v: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let domain = str_field(&v, "domain").ok_or_else(|| bad("domain required"))?;
    // Confirm ownership before deleting (no cross-tenant delete / existence oracle).
    let owned = state
        .store
        .domain_record(&domain)
        .await
        .map_err(|_| Problem::server_error())?
        .is_some_and(|r| r.tenant_id == account.tenant.as_str());
    if !owned {
        return Err(Problem::not_found());
    }
    state
        .store
        .delete_domain(&domain)
        .await
        .map_err(store_admin_err)?;
    // Remove the domain's DKIM keys too (dkim_keys references the tenant, not
    // the domain, so it does not cascade).
    if let Err(error) = state
        .store
        .for_tenant(account.tenant.clone())
        .delete_dkim_keys(&domain)
        .await
    {
        tracing::warn!(%error, "failed to remove DKIM keys for deleted domain");
    }
    audit(&state, &account, "domain.delete", Some(&domain), None).await;
    Ok(Json(json!({ "ok": true })))
}

/// `POST /admin/domains/dkim/rotate` — generate a fresh active DKIM key for one
/// of this tenant's domains (selector rollover, ADR 0014). Body `{ domain }`.
/// The previous key stays published (inactive) until the operator removes it,
/// so in-flight mail still verifies.
pub async fn rotate_dkim(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_admin()?;
    let v: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let domain = str_field(&v, "domain").ok_or_else(|| bad("domain required"))?;
    let owned = state
        .store
        .domain_record(&domain)
        .await
        .map_err(|_| Problem::server_error())?
        .is_some_and(|r| r.tenant_id == account.tenant.as_str());
    if !owned {
        return Err(Problem::not_found());
    }
    let key =
        alo_auth_mail::dkim::keystore::generate_ed25519_key().ok_or_else(Problem::server_error)?;
    state
        .store
        .install_active_dkim_key(
            &account.tenant,
            &domain,
            &key.selector,
            key.seed.as_ref(),
            &key.public_raw,
        )
        .await
        .map_err(store_admin_err)?;
    audit(&state, &account, "dkim.rotate", Some(&domain), None).await;
    let ts = state.store.for_tenant(account.tenant.clone());
    Ok(Json(
        json!({ "domain": domain, "dkim": dkim_record_json(&ts, &domain).await }),
    ))
}

// ---- audit log (tenant-admin; ADR 0012) --------------------------------

/// `GET /admin/audit` → `{ entries: [...] }` — this tenant's recent
/// administrative actions, newest first.
pub async fn list_audit(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_admin()?;
    let entries = state
        .store
        .for_tenant(account.tenant.clone())
        .list_audit(200)
        .await
        .map_err(|_| Problem::server_error())?;
    let list: Vec<Value> = entries
        .iter()
        .map(|e| {
            json!({
                "id": e.id,
                "actor": e.actor,
                "action": e.action,
                "target": e.target,
                "detail": e.detail,
                "entityType": e.entity_type,
                "entityId": e.entity_id,
                "at": utc_date(e.created_at),
            })
        })
        .collect();
    Ok(Json(json!({ "entries": list })))
}
