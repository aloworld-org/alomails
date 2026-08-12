//! Pages the internet can reach but only with a password (ADR 0036, S2.06a):
//! the authenticated `/sites/{id}/pages/{pid}/password` routes — protect a
//! page, change its password, lift it, and read which pages carry one.
//!
//! Separate from [`crate::sites`] (the editing surface) because it has its own
//! reason to change: it is the only place a secret enters the sites API, and it
//! obeys two rules the editing routes do not.
//!
//! - **A password goes in and never comes out.** No read on this surface — or
//!   on any other — answers the password or its hash. `GET` says whether a page
//!   is protected and when that was last decided; an owner who has forgotten
//!   the password sets a new one.
//! - **Setting it is effective immediately, not at the next publish.** The
//!   model deliberately keeps protection out of the immutable publish
//!   ([`alo_store::site_page_protection`]), so a page can be closed — or a
//!   leaked password replaced — without republishing the site.
//!
//! Error contract, identical to the rest of the sites surface: `401`
//! unauthenticated, `404` for anything that does not resolve in the caller's
//! tenant, `422` with the store's rule-naming sentence for a refusal.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use alo_store::{SiteId, SitePageId, SitePageProtection};

use crate::error::Problem;
use crate::sites::{map_store_err, require_site};
use crate::state::{AppState, authenticate};

/// What a caller must send when the body carries no usable password.
const BAD_PASSWORD_BODY: &str = "password must be a string, for example {\"password\": \"…\"}";

fn iso(t: OffsetDateTime) -> String {
    t.format(&Rfc3339).unwrap_or_default()
}

/// One page's protection state. `protected: false` carries no timestamps —
/// there is nothing to date — so a surface can branch on one field.
fn protection_json(protection: Option<&SitePageProtection>) -> Value {
    match protection {
        Some(protection) => json!({
            "protected": true,
            "pageId": protection.page.as_str(),
            "createdAt": iso(protection.created_at),
            "updatedAt": iso(protection.updated_at),
        }),
        None => json!({ "protected": false }),
    }
}

/// `GET /sites/:id/passwords` → `{"pages":[…]}` — every protected page of the
/// site, so a page list can mark them in one read instead of one per page.
pub async fn list_page_passwords(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let site = SiteId::new(id);
    require_site(&account, &site).await?;
    let protections = account
        .acc
        .site_page_protections(&site)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "pages": protections
            .iter()
            .map(|protection| protection_json(Some(protection)))
            .collect::<Vec<_>>(),
    })))
}

/// `GET /sites/:id/pages/:pid/password` → whether this page is protected.
/// Never the password: a forgotten one is replaced, not recovered.
pub async fn get_page_password(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, page)): Path<(String, String)>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let site = SiteId::new(id);
    require_site(&account, &site).await?;
    let protection = account
        .acc
        .site_page_protection(&site, &SitePageId::new(page))
        .await
        .map_err(map_store_err)?;
    Ok(Json(protection_json(protection.as_ref())))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PasswordBody {
    password: String,
}

/// `PUT /sites/:id/pages/:pid/password` `{"password"}` → the page's protection.
///
/// The same call protects a page and changes its password, because from the
/// owner's side those are one decision. Both take effect on the next public
/// request, and both end every visitor session opened with the previous
/// password.
pub async fn set_page_password(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, page)): Path<(String, String)>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let site = SiteId::new(id);
    require_site(&account, &site).await?;
    // Parsed by hand so an unreadable body cannot be echoed back: this is the
    // one request on the sites surface whose body is a secret.
    let req: PasswordBody = serde_json::from_slice(&body)
        .map_err(|_| Problem::with(StatusCode::UNPROCESSABLE_ENTITY, BAD_PASSWORD_BODY))?;
    let protection = account
        .acc
        .set_site_page_password(&site, &SitePageId::new(page), &req.password)
        .await
        .map_err(map_store_err)?;
    Ok(Json(protection_json(Some(&protection))))
}

/// `DELETE /sites/:id/pages/:pid/password` → the page is public again on the
/// next request. Idempotent: a page that carries no password is already in the
/// asked-for state.
pub async fn remove_page_password(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, page)): Path<(String, String)>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let site = SiteId::new(id);
    require_site(&account, &site).await?;
    account
        .acc
        .remove_site_page_password(&site, &SitePageId::new(page))
        .await
        .map_err(map_store_err)?;
    Ok(Json(protection_json(None)))
}
