//! The site assistant's settings routes (ADR 0040 §3, item S3.02c): the one
//! screen that switches the assistant on is the screen that shows its
//! monthly spending ceiling — defaulted rather than blank, spend rather than
//! tokens, integer cents only. `GET` returns the effective settings even for
//! a site that never touched them (off, default ceiling) together with the
//! current month's spend, so the UI can say what is left before the
//! assistant pauses; `PUT` sets switch and ceiling in one write.
//!
//! Auth and tenancy follow the `/sites/*` family exactly: [`authenticate`],
//! then the account door, so a foreign site id is a clean 404 and a ceiling
//! outside the allowed range is a 422 naming the rule.
//!
//! **Both routes are the site owner's** (S3.02d, the same posture as the
//! domain-purchase money door): the ceiling is the tenant's money and the
//! switch makes the tenant's published content answerable by strangers, so
//! neither a restricted site editor nor an uninvolved colleague may read or
//! set them — only the person who made the site, or an admin.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use time::OffsetDateTime;

use alo_store::{DEFAULT_CHAT_MONTHLY_CEILING_CENTS, SiteChatSettings, SiteId, chat_month_key};

use crate::error::Problem;
use crate::sites::{map_store_err, require_site, require_site_manager};
use crate::state::{Account, AppState, authenticate};

/// The refusal a non-owner meets here, naming what only the owner may do.
const OWNER_ONLY: &str =
    "Only this website's owner can switch its assistant on or set its monthly budget.";

/// The site these settings belong to, provided the caller administers it.
async fn require_settings_site(account: &Account, site: &SiteId) -> Result<(), Problem> {
    let site = require_site(account, site).await?;
    require_site_manager(account, &site)
        .map_err(|_| Problem::with(StatusCode::FORBIDDEN, OWNER_ONLY))
}

/// The effective settings as JSON. `defaultCeilingCents` rides along so the
/// screen can label the pre-filled value as the default it is.
fn settings_json(settings: &SiteChatSettings) -> Value {
    json!({
        "enabled": settings.enabled,
        "monthlyCeilingCents": settings.monthly_ceiling_cents,
        "defaultCeilingCents": DEFAULT_CHAT_MONTHLY_CEILING_CENTS,
        "month": settings.month,
        "spentCents": settings.spent_cents,
        "ceilingHit": settings.ceiling_hit,
    })
}

/// `GET /sites/:id/chat-settings` → the effective assistant settings plus
/// this month's spend. Never blank: a site that never saved settings reads
/// as off with the default ceiling.
pub async fn get_chat_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let site = SiteId::new(id);
    require_settings_site(&account, &site).await?;
    let month = chat_month_key(OffsetDateTime::now_utc());
    let settings = account
        .acc
        .site_chat_settings(&site, &month)
        .await
        .map_err(map_store_err)?;
    Ok(Json(settings_json(&settings)))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PutChatSettingsBody {
    enabled: bool,
    monthly_ceiling_cents: i64,
}

/// `PUT /sites/:id/chat-settings` → sets the switch and the ceiling in one
/// write and returns the resulting view — the same shape `GET` serves.
pub async fn put_chat_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: PutChatSettingsBody =
        serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let site = SiteId::new(id);
    require_settings_site(&account, &site).await?;
    let month = chat_month_key(OffsetDateTime::now_utc());
    let settings = account
        .acc
        .set_site_chat_settings(&site, req.enabled, req.monthly_ceiling_cents, &month)
        .await
        .map_err(map_store_err)?;
    Ok(Json(settings_json(&settings)))
}
