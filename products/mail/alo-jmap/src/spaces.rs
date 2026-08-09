//! Spaces HTTP surface (ADR 0026). Authenticated, tenant-scoped through the
//! account door: every handler resolves the caller with [`authenticate`] and
//! touches only Spaces they belong to. Membership is the permission model — a
//! non-member gets 404 (existence hidden), a member below the needed role gets
//! 403.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use time::format_description::well_known::Rfc3339;

use alo_store::{SpaceId, SpaceRole, StoreError, UserId};

use crate::error::Problem;
use crate::state::{AppState, authenticate};

fn map_err(e: StoreError) -> Problem {
    match e {
        StoreError::NotFound => Problem::with(StatusCode::NOT_FOUND, "not found"),
        StoreError::Forbidden => Problem::with(StatusCode::FORBIDDEN, "insufficient role"),
        StoreError::Conflict(msg) => Problem::with(StatusCode::CONFLICT, &msg),
        _ => Problem::server_error(),
    }
}

fn iso(t: time::OffsetDateTime) -> String {
    t.format(&Rfc3339).unwrap_or_default()
}

fn role_str(r: SpaceRole) -> &'static str {
    r.as_str()
}

/// `GET /spaces` → `{"spaces":[...]}` — the Spaces the caller belongs to.
pub async fn list_spaces(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let spaces = account.acc.spaces().await.map_err(map_err)?;
    Ok(Json(json!({
        "spaces": spaces.iter().map(|s| json!({
            "id": s.id.as_str(), "name": s.name, "archived": s.archived,
            "myRole": role_str(s.my_role), "createdAt": iso(s.created_at),
        })).collect::<Vec<_>>(),
    })))
}

#[derive(Deserialize)]
struct CreateBody {
    name: String,
}

/// `POST /spaces` `{name}` → `{"id":"..."}` — create a Space (caller becomes its
/// manager).
pub async fn create_space(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: CreateBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let name = req.name.trim();
    if name.is_empty() {
        return Err(Problem::with(StatusCode::BAD_REQUEST, "a name is required"));
    }
    let id = account.acc.create_space(name).await.map_err(map_err)?;
    Ok(Json(json!({ "id": id.as_str() })))
}

/// `GET /spaces/:id` → `{space, members, modules}` — the Space, its membership
/// (with emails), and its enabled modules. Members-only.
pub async fn get_space(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let sid = SpaceId::new(id);
    let Some(space) = account.acc.space(&sid).await.map_err(map_err)? else {
        return Err(Problem::with(StatusCode::NOT_FOUND, "no such space"));
    };
    let members = account.acc.space_members(&sid).await.map_err(map_err)?;
    let modules = account.acc.space_modules(&sid).await.map_err(map_err)?;
    let ts = state.store.for_tenant(account.tenant.clone());
    let mut member_json = Vec::with_capacity(members.len());
    for m in &members {
        let email = ts
            .email_of(&UserId::new(m.user_id.clone()))
            .await
            .ok()
            .flatten();
        member_json.push(json!({
            "userId": m.user_id, "email": email, "role": role_str(m.role),
            "addedAt": iso(m.added_at),
        }));
    }
    Ok(Json(json!({
        "space": {
            "id": space.id.as_str(), "name": space.name, "archived": space.archived,
            "myRole": role_str(space.my_role), "createdAt": iso(space.created_at),
        },
        "members": member_json,
        "modules": modules,
    })))
}

#[derive(Deserialize)]
struct UpdateBody {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    archived: Option<bool>,
}

/// `PUT /spaces/:id` `{name?, archived?}` → `{status:"ok"}` — rename and/or
/// (un)archive. Manager only.
pub async fn update_space(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: UpdateBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let sid = SpaceId::new(id);
    if let Some(name) = req.name.as_deref().map(str::trim) {
        if name.is_empty() {
            return Err(Problem::with(
                StatusCode::BAD_REQUEST,
                "name cannot be empty",
            ));
        }
        account
            .acc
            .rename_space(&sid, name)
            .await
            .map_err(map_err)?;
    }
    if let Some(archived) = req.archived {
        account
            .acc
            .set_space_archived(&sid, archived)
            .await
            .map_err(map_err)?;
    }
    Ok(Json(json!({ "status": "ok" })))
}

#[derive(Deserialize)]
struct MemberBody {
    email: String,
    role: String,
}

/// `POST /spaces/:id/members` `{email, role}` → `{status:"ok"}` — add or re-role
/// a member by email. Manager only.
pub async fn add_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: MemberBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let Some(role) = SpaceRole::parse(req.role.trim()) else {
        return Err(Problem::with(StatusCode::BAD_REQUEST, "invalid role"));
    };
    let ts = state.store.for_tenant(account.tenant.clone());
    let Ok(user) = ts.user_by_email(req.email.trim()).await else {
        // No such user in this tenant — a clean 404, never a hint about other
        // tenants' users.
        return Err(Problem::with(StatusCode::NOT_FOUND, "no such user"));
    };
    account
        .acc
        .add_space_member(&SpaceId::new(id), &user, role)
        .await
        .map_err(map_err)?;
    Ok(Json(json!({ "status": "ok" })))
}

/// `DELETE /spaces/:id/members/:uid` → `{status:"ok"}` — remove a member.
/// Manager only; refuses to remove the last manager.
pub async fn remove_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, uid)): Path<(String, String)>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .remove_space_member(&SpaceId::new(id), &UserId::new(uid))
        .await
        .map_err(map_err)?;
    Ok(Json(json!({ "status": "ok" })))
}

#[derive(Deserialize)]
struct ModuleBody {
    module: String,
    enabled: bool,
}

/// `POST /spaces/:id/modules` `{module, enabled}` → `{status:"ok"}` — enable or
/// disable a module on a Space. Manager only.
pub async fn set_module(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: ModuleBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let module = req.module.trim();
    if module.is_empty() {
        return Err(Problem::with(StatusCode::BAD_REQUEST, "module is required"));
    }
    account
        .acc
        .set_space_module(&SpaceId::new(id), module, req.enabled)
        .await
        .map_err(map_err)?;
    Ok(Json(json!({ "status": "ok" })))
}
