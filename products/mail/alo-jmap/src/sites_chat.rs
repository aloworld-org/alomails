//! The site assistant's settings routes (ADR 0040 §3 and §5, items S3.02c
//! and S3.02f): the one screen that switches the assistant on is the screen
//! that shows its monthly spending ceiling — defaulted rather than blank,
//! spend rather than tokens, integer cents only. `GET`/`PUT
//! /sites/:id/chat-settings` read and set switch + ceiling;
//! `GET`/`PUT /sites/:id/chat-appearance` read and set everything the tenant
//! may change about the widget itself — welcome message, bot name and
//! avatar, suggested questions, tone and tone note, launcher corner and
//! icon, offline message, and an accent chosen among the site's own palette
//! roles (never free-form colours or CSS; the typed model rejects anything
//! else).
//!
//! `GET /sites/:id/chat-actions` is the assistant's transcript (item
//! S3.03e): what the assistant did — each answer with the pages it cited,
//! each refusal, each booking offered and made, each lead raised — newest
//! first, bounded by the store's own per-site cap. It shows the act and the
//! tenant's own published facts, never the conversation: the store's type
//! has no field a question, an answer text or a visitor could travel in.
//!
//! Auth and tenancy follow the `/sites/*` family exactly: [`authenticate`],
//! then the account door, so a foreign site id is a clean 404 and a violated
//! content rule is a 422 naming the rule.
//!
//! **All these routes are the site owner's** (S3.02d, the same posture as
//! the domain-purchase money door): the ceiling is the tenant's money, the
//! switch makes the tenant's published content answerable by strangers, and
//! the appearance — including the tone note that reaches the public prompt —
//! is that same public voice; neither a restricted site editor nor an
//! uninvolved colleague may read or set them — only the person who made the
//! site, or an admin.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::{Value, json};
use time::OffsetDateTime;

use alo_sites::render::strings_for;
use alo_sites::stylesheet::stylesheet;
use alo_store::{
    BlobId, CHAT_BOT_NAME_MAX_CHARS, CHAT_OFFLINE_MESSAGE_MAX_CHARS, CHAT_SUGGESTED_MAX,
    CHAT_SUGGESTED_QUESTION_MAX_CHARS, CHAT_TONE_NOTE_MAX_CHARS, CHAT_WELCOME_MAX_CHARS,
    ChatLauncherCorner, ChatLauncherIcon, ChatTone, ChatWidgetAccent,
    DEFAULT_CHAT_MONTHLY_CEILING_CENTS, Site, SiteChatAppearance, SiteChatSettings, SiteId,
    SiteTheme, chat_month_key,
};

use crate::error::Problem;
use crate::sites::{map_store_err, require_site, require_site_manager};
use crate::state::{Account, AppState, authenticate};

/// The refusal a non-owner meets here, naming what only the owner may do.
const OWNER_ONLY: &str =
    "Only this website's owner can switch its assistant on or set its monthly budget.";

/// The site these settings belong to, provided the caller administers it.
async fn require_settings_site(account: &Account, site: &SiteId) -> Result<Site, Problem> {
    let site = require_site(account, site).await?;
    require_site_manager(account, &site)
        .map_err(|_| Problem::with(StatusCode::FORBIDDEN, OWNER_ONLY))?;
    Ok(site)
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

/// The appearance as wire JSON. `limits` rides along so the screen can bound
/// its fields to the same caps the store enforces, instead of re-inventing
/// them; `defaults` carries the written copy the widget falls back on — in
/// the **site's** default language, which is what the visitor would read —
/// so the screen can pre-fill a written welcome instead of an empty box
/// (S3.02g) without keeping a second copy of the renderer's strings.
fn appearance_json(appearance: &SiteChatAppearance, site: &Site) -> Value {
    let strings = strings_for(&site.default_locale);
    json!({
        "botName": appearance.bot_name,
        "avatarBlobId": appearance.avatar.as_ref().map(BlobId::as_str),
        "welcome": appearance.welcome,
        "suggestedQuestions": appearance.suggested_questions,
        "tone": appearance.tone,
        "toneNote": appearance.tone_note,
        "launcherCorner": appearance.launcher_corner,
        "launcherIcon": appearance.launcher_icon,
        "autoOpen": appearance.auto_open,
        "offlineMessage": appearance.offline_message,
        "accent": appearance.accent,
        "limits": {
            "botNameChars": CHAT_BOT_NAME_MAX_CHARS,
            "welcomeChars": CHAT_WELCOME_MAX_CHARS,
            "suggestedQuestions": CHAT_SUGGESTED_MAX,
            "suggestedQuestionChars": CHAT_SUGGESTED_QUESTION_MAX_CHARS,
            "toneNoteChars": CHAT_TONE_NOTE_MAX_CHARS,
            "offlineMessageChars": CHAT_OFFLINE_MESSAGE_MAX_CHARS,
        },
        "defaults": {
            "botName": strings.chat_title,
            "welcome": strings.chat_welcome,
            "offlineMessage": strings.chat_unavailable,
        },
    })
}

/// `GET /sites/:id/chat-appearance` → the assistant's appearance and voice.
/// Never blank: a site that never touched it reads as the defaults (the
/// widget wears the site's theme and speaks our localized copy).
pub async fn get_chat_appearance(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let site = SiteId::new(id);
    let record = require_settings_site(&account, &site).await?;
    let appearance = account
        .acc
        .site_chat_appearance(&site)
        .await
        .map_err(map_store_err)?;
    Ok(Json(appearance_json(&appearance, &record)))
}

/// `GET /sites/:id/chat-actions` → the transcript of what the assistant did,
/// newest first. Owner-only like the rest of this screen: the transcript
/// says which published facts the tenant's public voice is handing out.
pub async fn get_chat_actions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let site = SiteId::new(id);
    require_settings_site(&account, &site).await?;
    let actions = account
        .acc
        .site_chat_actions(&site)
        .await
        .map_err(map_store_err)?;
    let rfc3339 = |t: time::OffsetDateTime| {
        t.format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default()
    };
    let actions: Vec<Value> = actions
        .into_iter()
        .map(|action| {
            json!({
                "id": action.id,
                "kind": action.kind.as_str(),
                "fact": action.fact,
                "slotAt": action.slot_at.map(rfc3339),
                "citations": action
                    .citations
                    .iter()
                    .map(|c| json!({"title": c.title, "path": c.path}))
                    .collect::<Vec<Value>>(),
                "occurredAt": rfc3339(action.occurred_at),
            })
        })
        .collect();
    Ok(Json(json!({"actions": actions})))
}

/// The wire shape of a `PUT /sites/:id/chat-appearance` body: the complete
/// appearance, every field optional or defaulted — omitting a field returns
/// it to its default (the screen always sends the whole picture). The
/// bounded choices deserialize through the store's own enums, so an
/// out-of-range value is a shape error naming the allowed variants.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
struct PutChatAppearanceBody {
    bot_name: Option<String>,
    avatar_blob_id: Option<String>,
    welcome: Option<String>,
    suggested_questions: Vec<String>,
    tone: ChatTone,
    tone_note: Option<String>,
    launcher_corner: ChatLauncherCorner,
    launcher_icon: ChatLauncherIcon,
    auto_open: bool,
    offline_message: Option<String>,
    accent: ChatWidgetAccent,
}

impl Default for PutChatAppearanceBody {
    fn default() -> Self {
        let defaults = SiteChatAppearance::default();
        PutChatAppearanceBody {
            bot_name: None,
            avatar_blob_id: None,
            welcome: None,
            suggested_questions: Vec::new(),
            tone: defaults.tone,
            tone_note: None,
            launcher_corner: defaults.launcher_corner,
            launcher_icon: defaults.launcher_icon,
            auto_open: defaults.auto_open,
            offline_message: None,
            accent: defaults.accent,
        }
    }
}

/// `PUT /sites/:id/chat-appearance` → validates and stores the appearance in
/// one write and returns the resulting view — the same shape `GET` serves.
/// A violated content rule (a cap, a blank, a malformed blob id) is a 422
/// whose detail names the field and the rule, verbatim from the model.
pub async fn put_chat_appearance(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let value: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let req: PutChatAppearanceBody = serde_json::from_value(value)
        .map_err(|error| Problem::with(StatusCode::UNPROCESSABLE_ENTITY, error.to_string()))?;
    let appearance = SiteChatAppearance {
        bot_name: req.bot_name,
        avatar: req.avatar_blob_id.map(BlobId::new),
        welcome: req.welcome,
        suggested_questions: req.suggested_questions,
        tone: req.tone,
        tone_note: req.tone_note,
        launcher_corner: req.launcher_corner,
        launcher_icon: req.launcher_icon,
        auto_open: req.auto_open,
        offline_message: req.offline_message,
        accent: req.accent,
        ..SiteChatAppearance::default()
    };
    let site = SiteId::new(id);
    let record = require_settings_site(&account, &site).await?;
    let stored = account
        .acc
        .set_site_chat_appearance(&site, &appearance)
        .await
        .map_err(map_store_err)?;
    Ok(Json(appearance_json(&stored, &record)))
}

/// The largest avatar the preview inlines as a `data:` URI — the same bound
/// as the page preview's images, and for the same reason: public asset paths
/// do not resolve on the edit origin, and the preview document stays bounded.
const PREVIEW_AVATAR_MAX_BYTES: usize = 4 * 1024 * 1024;

/// `POST /sites/:id/chat-appearance/preview` → the widget exactly as the
/// public service would inject it, wearing the site's own theme, as one
/// complete self-contained HTML document (`text/html`) — the live preview
/// beside the appearance fields (S3.02g). The body is the same shape `PUT`
/// takes and is validated by the same model, so what previews clean saves
/// clean; **nothing is stored**. The panel renders open and without its
/// behavior script — the editor shows the document in a sandboxed iframe.
/// Owner-only like the rest of the appearance family, and `no-store`: a
/// draft has no cache life.
pub async fn preview_chat_appearance(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Response, Problem> {
    let account = authenticate(&state, &headers).await?;
    let value: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let req: PutChatAppearanceBody = serde_json::from_value(value)
        .map_err(|error| Problem::with(StatusCode::UNPROCESSABLE_ENTITY, error.to_string()))?;
    let appearance = SiteChatAppearance {
        bot_name: req.bot_name,
        avatar: req.avatar_blob_id.map(BlobId::new),
        welcome: req.welcome,
        suggested_questions: req.suggested_questions,
        tone: req.tone,
        tone_note: req.tone_note,
        launcher_corner: req.launcher_corner,
        launcher_icon: req.launcher_icon,
        auto_open: req.auto_open,
        offline_message: req.offline_message,
        accent: req.accent,
        ..SiteChatAppearance::default()
    };
    let site = SiteId::new(id);
    let record = require_settings_site(&account, &site).await?;
    // The same gate a save goes through, so the 422 wording never differs
    // between previewing a value and storing it.
    appearance
        .validate()
        .map_err(|error| map_store_err(error.into()))?;
    let avatar_src = match &appearance.avatar {
        None => None,
        Some(blob) => {
            use base64::Engine;
            match account.acc.site_image(blob).await.map_err(map_store_err)? {
                Some(image) if image.bytes.len() <= PREVIEW_AVATAR_MAX_BYTES => Some(format!(
                    "data:{};base64,{}",
                    image.content_type,
                    base64::engine::general_purpose::STANDARD.encode(&image.bytes)
                )),
                // Absent, foreign, non-image or oversized: the preview simply
                // shows no avatar rather than a broken image.
                _ => None,
            }
        }
    };
    let theme = SiteTheme::from_stored(record.theme.clone());
    let css = stylesheet(&theme);
    let html = alo_sites::serve::widget::preview_document(
        strings_for(&record.default_locale),
        &css,
        &appearance,
        avatar_src.as_deref(),
    );
    Ok((
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        html,
    )
        .into_response())
}
