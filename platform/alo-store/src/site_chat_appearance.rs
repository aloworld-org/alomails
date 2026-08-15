//! The visitor assistant's appearance and voice (schema v1, ADR 0040 §5,
//! item S3.02f).
//!
//! The widget already wears the site's preset palette, logo and favicon —
//! asking somebody to pick their brand colour a second time is precisely the
//! seam alo exists to remove. What a tenant changes about it is therefore
//! **content or a bounded choice**: the welcome message, the bot's name and
//! avatar, up to [`CHAT_SUGGESTED_MAX`] suggested opening questions, a tone
//! scale plus a free-text note about the business's voice, which corner the
//! launcher sits in and which icon it shows, whether the panel opens by
//! itself (off by default — an uninvited popup is the thing everyone hates),
//! and the offline message shown when the assistant cannot answer.
//!
//! Two boundaries hold by type, not by review:
//!
//! - **Colour is a choice among the site's own palette roles**
//!   ([`ChatWidgetAccent`]), never a picker. Every shipped preset palette is
//!   contrast-checked at build time, and each accent role names a fill/label
//!   *pair* from that checked set — a test below proves no storable accent on
//!   any shipped preset can produce failing contrast. No custom CSS, no
//!   custom fonts, no hex values anywhere in this model.
//! - **The tone note shapes the prompt, never the rules.** It is stored here
//!   as bounded text; `alo-ai`'s prompt assembly quotes it as style guidance
//!   *above* the absolute answering rules, and its own test proves no note
//!   can widen what ADR 0040 §1 and §2 allow.
//!
//! Like [`crate::site_theme`], the model is pure types + validation, stored
//! as a versioned JSON envelope (the `appearance` column of
//! `site_chat_settings`); a pristine `{}` reads as the defaults. The write
//! gate is [`SiteChatAppearance::from_value`]; readers that must never fail
//! (the public renderer) use [`SiteChatAppearance::from_stored`].

use serde::{Deserialize, Serialize};

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::{BlobId, SiteId};
use crate::site_model::valid_id_token;
use crate::site_public::{PublishedSite, SitePublicStore};

/// The current appearance schema version.
pub const CHAT_APPEARANCE_SCHEMA_VERSION: u64 = 1;

/// The most characters a bot name may carry (single line).
pub const CHAT_BOT_NAME_MAX_CHARS: usize = 60;
/// The most characters the welcome message may carry.
pub const CHAT_WELCOME_MAX_CHARS: usize = 400;
/// The most suggested opening questions a tenant may configure.
pub const CHAT_SUGGESTED_MAX: usize = 3;
/// The most characters one suggested question may carry (single line).
pub const CHAT_SUGGESTED_QUESTION_MAX_CHARS: usize = 160;
/// The most characters the tone note may carry.
pub const CHAT_TONE_NOTE_MAX_CHARS: usize = 500;
/// The most characters the offline message may carry.
pub const CHAT_OFFLINE_MESSAGE_MAX_CHARS: usize = 300;

/// Why an appearance value was rejected. Messages are field-level validation
/// details, safe to surface on the wire as a 422 — they never echo the
/// submitted value.
#[derive(Debug, thiserror::Error)]
pub enum ChatAppearanceError {
    /// The envelope declares a schema version this build does not speak.
    #[error(
        "unsupported assistant appearance schema_version {0} \
         (this build speaks {CHAT_APPEARANCE_SCHEMA_VERSION})"
    )]
    UnsupportedVersion(u64),
    /// The JSON does not fit the typed schema: unknown prop, or a
    /// wrong-typed value.
    #[error("assistant appearance does not match schema v{CHAT_APPEARANCE_SCHEMA_VERSION}: {0}")]
    Shape(#[from] serde_json::Error),
    /// Structurally well-typed but violating a content rule. The message
    /// names the field and the rule.
    #[error("assistant appearance: {0}")]
    Invalid(String),
}

impl From<ChatAppearanceError> for StoreError {
    fn from(error: ChatAppearanceError) -> Self {
        StoreError::Validation(error.to_string())
    }
}

/// The tone scale between formal and warm (ADR 0040 §5). Shapes the prompt's
/// style guidance only — never its rules.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatTone {
    Formal,
    #[default]
    Neutral,
    Warm,
}

/// Which corner of the page the launcher sits in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatLauncherCorner {
    #[default]
    Right,
    Left,
}

/// Which icon the launcher shows beside its label — a bounded set of shipped
/// glyphs, never an upload.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatLauncherIcon {
    #[default]
    Chat,
    Question,
    Sparkle,
}

/// The widget's accent, as a choice among the site's own palette roles.
/// Each role names a fill/label **pair** whose contrast every shipped preset
/// already proves at build time — which is what makes this enum storable
/// where a colour picker would not be (a bot bubble is small text on a
/// coloured field, the single worst place for #FFFF00 on white).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatWidgetAccent {
    /// Fill `primary`, label `on_primary` — the site's buttons and links.
    #[default]
    Primary,
    /// Fill `text`, label `background` — the site's ink, inverted.
    Text,
    /// Fill `surface`, label `text` — the site's cards, quiet.
    Surface,
}

impl ChatWidgetAccent {
    /// The palette-role pair this accent paints the widget with, as
    /// `(fill, label)` role names. The public renderer maps these onto the
    /// theme's CSS tokens; the contrast test below walks the same pairs over
    /// every shipped preset.
    #[must_use]
    pub fn role_pair(self) -> (&'static str, &'static str) {
        match self {
            ChatWidgetAccent::Primary => ("primary", "on_primary"),
            ChatWidgetAccent::Text => ("text", "background"),
            ChatWidgetAccent::Surface => ("surface", "text"),
        }
    }
}

/// The versioned value stored in `site_chat_settings.appearance`. Every
/// field is optional or defaulted: absence means "the widget wears the
/// site's theme and speaks our localized copy".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct SiteChatAppearance {
    /// Schema version of the appearance; this build speaks
    /// [`CHAT_APPEARANCE_SCHEMA_VERSION`]. `0` (the serde default for a
    /// pristine `{}`) reads as current.
    pub schema_version: u64,
    /// The bot's name — the dialog's heading and accessible name. Often
    /// deliberately not the company name: "Ask Marie" outperforms "Chat
    /// with us". Absent: the localized default heading.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bot_name: Option<String>,
    /// The bot's avatar (a tenant blob), shown in the dialog header — may be
    /// a person rather than the logo. Served publicly only while the
    /// assistant is on.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<BlobId>,
    /// The first thing the visitor reads — the single highest-value field on
    /// the screen. Absent: a localized default is written for them rather
    /// than left blank.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub welcome: Option<String>,
    /// Up to [`CHAT_SUGGESTED_MAX`] opening questions, offered as one-tap
    /// buttons until the visitor asks their own.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub suggested_questions: Vec<String>,
    /// The tone scale between formal and warm.
    pub tone: ChatTone,
    /// A free-text note about the business's voice. Style guidance only —
    /// the prompt quotes it below rules it can never change.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tone_note: Option<String>,
    /// Which corner the launcher sits in.
    pub launcher_corner: ChatLauncherCorner,
    /// Which shipped icon the launcher shows.
    pub launcher_icon: ChatLauncherIcon,
    /// Whether the panel opens by itself on page load. Off by default; when
    /// on, it opens without stealing focus.
    pub auto_open: bool,
    /// Shown when the assistant cannot answer (ceiling spent, no backend).
    /// Absent: the localized default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offline_message: Option<String>,
    /// The widget's accent among the site's palette roles.
    pub accent: ChatWidgetAccent,
}

impl Default for SiteChatAppearance {
    /// The defaults a pristine `{}` reads as — current schema version, the
    /// widget wearing the site's theme, every text field on our localized
    /// copy. (Manual so `schema_version` defaults to the version this build
    /// speaks, for serde's field fill-in too.)
    fn default() -> Self {
        SiteChatAppearance {
            schema_version: CHAT_APPEARANCE_SCHEMA_VERSION,
            bot_name: None,
            avatar: None,
            welcome: None,
            suggested_questions: Vec::new(),
            tone: ChatTone::default(),
            tone_note: None,
            launcher_corner: ChatLauncherCorner::default(),
            launcher_icon: ChatLauncherIcon::default(),
            auto_open: false,
            offline_message: None,
            accent: ChatWidgetAccent::default(),
        }
    }
}

/// One optional single-line text field: trimmed-non-empty when present,
/// within its cap, no control characters.
fn check_line(
    value: Option<&str>,
    field: &str,
    max_chars: usize,
) -> std::result::Result<(), ChatAppearanceError> {
    let Some(value) = value else { return Ok(()) };
    check_text(value, field, max_chars)?;
    if value.chars().any(char::is_control) {
        return Err(ChatAppearanceError::Invalid(format!(
            "{field} must be a single line without control characters"
        )));
    }
    Ok(())
}

/// One text field that may span lines: trimmed-non-empty, within its cap, no
/// control characters other than newlines.
fn check_text(
    value: &str,
    field: &str,
    max_chars: usize,
) -> std::result::Result<(), ChatAppearanceError> {
    if value.trim().is_empty() {
        return Err(ChatAppearanceError::Invalid(format!(
            "{field} must not be blank (omit it to use the default)"
        )));
    }
    if value.chars().count() > max_chars {
        return Err(ChatAppearanceError::Invalid(format!(
            "{field} must be at most {max_chars} characters"
        )));
    }
    if value.chars().any(|c| c.is_control() && c != '\n') {
        return Err(ChatAppearanceError::Invalid(format!(
            "{field} must not contain control characters"
        )));
    }
    Ok(())
}

impl SiteChatAppearance {
    /// Parses and fully validates a wire appearance value. This is the write
    /// gate: everything persisted goes through here first.
    ///
    /// # Errors
    /// [`ChatAppearanceError::UnsupportedVersion`] on a version this build
    /// does not speak (checked before shape, so a v2 payload gets the
    /// version error); [`ChatAppearanceError::Shape`] on unknown or mistyped
    /// props; [`ChatAppearanceError::Invalid`] on a violated content rule.
    pub fn from_value(value: serde_json::Value) -> std::result::Result<Self, ChatAppearanceError> {
        if let Some(version) = value
            .get("schema_version")
            .and_then(serde_json::Value::as_u64)
            && version != CHAT_APPEARANCE_SCHEMA_VERSION
        {
            return Err(ChatAppearanceError::UnsupportedVersion(version));
        }
        let appearance: Self = serde_json::from_value(value)?;
        appearance.validate()?;
        Ok(appearance)
    }

    /// The read-side spelling for callers that must never fail (the public
    /// renderer): a pristine `{}` — a site that never touched its
    /// assistant's appearance — reads as the defaults, and so does anything
    /// invalid, defensively.
    #[must_use]
    pub fn from_stored(value: serde_json::Value) -> Self {
        Self::from_value(value).unwrap_or_default()
    }

    /// Serializes back to the stored JSON shape.
    ///
    /// # Errors
    /// [`ChatAppearanceError::Shape`] — cannot occur for values built from
    /// these types, but serialization is fallible by signature.
    pub fn to_value(&self) -> std::result::Result<serde_json::Value, ChatAppearanceError> {
        Ok(serde_json::to_value(self)?)
    }

    /// Content-rule validation: caps, single-line fields, blob token shape,
    /// the suggested-question count. The bounded choices (tone, corner,
    /// icon, accent) need no checking — the type admits only shipped values.
    ///
    /// # Errors
    /// The specific [`ChatAppearanceError`] naming the violated rule.
    pub fn validate(&self) -> std::result::Result<(), ChatAppearanceError> {
        if self.schema_version != CHAT_APPEARANCE_SCHEMA_VERSION {
            return Err(ChatAppearanceError::UnsupportedVersion(self.schema_version));
        }
        check_line(
            self.bot_name.as_deref(),
            "bot_name",
            CHAT_BOT_NAME_MAX_CHARS,
        )?;
        if let Some(avatar) = &self.avatar
            && !valid_id_token(avatar.as_str())
        {
            return Err(ChatAppearanceError::Invalid(
                "avatar is not a valid blob id".to_owned(),
            ));
        }
        if let Some(welcome) = &self.welcome {
            check_text(welcome, "welcome", CHAT_WELCOME_MAX_CHARS)?;
        }
        if self.suggested_questions.len() > CHAT_SUGGESTED_MAX {
            return Err(ChatAppearanceError::Invalid(format!(
                "at most {CHAT_SUGGESTED_MAX} suggested questions are allowed"
            )));
        }
        for question in &self.suggested_questions {
            check_line(
                Some(question),
                "suggested_questions",
                CHAT_SUGGESTED_QUESTION_MAX_CHARS,
            )?;
        }
        if let Some(note) = &self.tone_note {
            check_text(note, "tone_note", CHAT_TONE_NOTE_MAX_CHARS)?;
        }
        check_line(
            self.offline_message.as_deref(),
            "offline_message",
            CHAT_OFFLINE_MESSAGE_MAX_CHARS,
        )?;
        Ok(())
    }
}

impl AccountStore {
    /// The site's assistant appearance. A site that never touched it — or
    /// has no settings row at all — reads as the defaults, never an error.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the site isn't the tenant's;
    /// [`StoreError::Db`] on backend failure.
    pub async fn site_chat_appearance(&self, site: &SiteId) -> Result<SiteChatAppearance> {
        let row = sqlx::query_as::<_, (Option<serde_json::Value>,)>(
            "SELECT st.appearance FROM sites s \
             LEFT JOIN site_chat_settings st \
               ON st.tenant_id = s.tenant_id AND st.site_id = s.id \
             WHERE s.tenant_id = $1 AND s.id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        let Some((appearance,)) = row else {
            return Err(StoreError::NotFound);
        };
        Ok(appearance
            .map(SiteChatAppearance::from_stored)
            .unwrap_or_default())
    }

    /// Sets the assistant's appearance in one write, creating the settings
    /// row (assistant off, default ceiling) when the site never saved
    /// settings — appearance and enablement are independent choices.
    ///
    /// # Errors
    /// [`StoreError::Validation`] naming the violated rule;
    /// [`StoreError::NotFound`] when the site isn't the tenant's;
    /// [`StoreError::Db`] on backend failure.
    pub async fn set_site_chat_appearance(
        &self,
        site: &SiteId,
        appearance: &SiteChatAppearance,
    ) -> Result<SiteChatAppearance> {
        appearance.validate()?;
        let value = appearance.to_value()?;
        let done = sqlx::query(
            "INSERT INTO site_chat_settings (tenant_id, site_id, appearance) \
             SELECT s.tenant_id, s.id, $3 FROM sites s \
             WHERE s.tenant_id = $1 AND s.id = $2 \
             ON CONFLICT (tenant_id, site_id) DO UPDATE \
                SET appearance = EXCLUDED.appearance, updated_at = now()",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(value)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        self.site_chat_appearance(site).await
    }
}

impl SitePublicStore {
    /// The resolved site's assistant appearance, for the widget the public
    /// service renders. Scoped by the resolved value's private tenant
    /// pairing, like every other read on this door; absence reads as the
    /// defaults.
    ///
    /// # Errors
    /// [`StoreError::Db`] on backend failure.
    pub async fn chat_appearance(&self, site: &PublishedSite) -> Result<SiteChatAppearance> {
        let row = sqlx::query_as::<_, (serde_json::Value,)>(
            "SELECT appearance FROM site_chat_settings \
             WHERE tenant_id = $1 AND site_id = $2",
        )
        .bind(site.tenant.as_str())
        .bind(site.site.as_str())
        .fetch_optional(self.pool())
        .await
        .map_err(StoreError::Db)?;
        Ok(row
            .map(|(value,)| SiteChatAppearance::from_stored(value))
            .unwrap_or_default())
    }

    /// Whether `blob_id` is the resolved site's assistant avatar **and** the
    /// assistant is switched on — the public image path's membership check
    /// for a blob the publish itself does not reference (the same posture as
    /// the blog-cover fallback: the widget rides live state, so its avatar's
    /// servability does too).
    ///
    /// # Errors
    /// [`StoreError::Db`] on backend failure.
    pub async fn chat_avatar_allows(&self, site: &PublishedSite, blob_id: &str) -> Result<bool> {
        let row = sqlx::query_as::<_, (bool, serde_json::Value)>(
            "SELECT enabled, appearance FROM site_chat_settings \
             WHERE tenant_id = $1 AND site_id = $2",
        )
        .bind(site.tenant.as_str())
        .bind(site.site.as_str())
        .fetch_optional(self.pool())
        .await
        .map_err(StoreError::Db)?;
        Ok(row.is_some_and(|(enabled, value)| {
            enabled
                && SiteChatAppearance::from_stored(value)
                    .avatar
                    .is_some_and(|avatar| avatar.as_str() == blob_id)
        }))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use serde_json::json;

    use super::*;
    use crate::site_theme::{Palette, THEME_PRESETS};

    #[test]
    fn a_pristine_value_reads_as_the_defaults() {
        let defaults = SiteChatAppearance::from_stored(json!({}));
        assert_eq!(defaults, SiteChatAppearance::default());
        assert_eq!(defaults.tone, ChatTone::Neutral);
        assert_eq!(defaults.launcher_corner, ChatLauncherCorner::Right);
        assert_eq!(defaults.launcher_icon, ChatLauncherIcon::Chat);
        assert_eq!(defaults.accent, ChatWidgetAccent::Primary);
        assert!(!defaults.auto_open);
        assert!(defaults.bot_name.is_none() && defaults.welcome.is_none());
        // And the defaults are themselves storable.
        SiteChatAppearance::from_value(json!({})).unwrap();
    }

    #[test]
    fn a_full_appearance_round_trips() {
        let full = SiteChatAppearance {
            schema_version: CHAT_APPEARANCE_SCHEMA_VERSION,
            bot_name: Some("Marie".to_owned()),
            avatar: Some(BlobId::new("9hK3vQ2mR8pT1xWz4bC5dg")),
            welcome: Some("Hi, I'm Marie.\nAsk me about our bread.".to_owned()),
            suggested_questions: vec![
                "When are you open?".to_owned(),
                "Do you deliver?".to_owned(),
            ],
            tone: ChatTone::Warm,
            tone_note: Some("Family bakery, plain words, no jargon.".to_owned()),
            launcher_corner: ChatLauncherCorner::Left,
            launcher_icon: ChatLauncherIcon::Question,
            auto_open: true,
            offline_message: Some("We answer by mail within a day.".to_owned()),
            accent: ChatWidgetAccent::Surface,
        };
        full.validate().unwrap();
        let value = full.to_value().unwrap();
        assert_eq!(SiteChatAppearance::from_value(value).unwrap(), full);
        // Absent options serialize as absent keys, not nulls.
        let minimal = SiteChatAppearance::default().to_value().unwrap();
        assert!(minimal.get("bot_name").is_none() && minimal.get("avatar").is_none());
        assert!(minimal.get("suggested_questions").is_none());
    }

    #[test]
    fn caps_blanks_and_control_characters_are_rejected() {
        let cases: Vec<(serde_json::Value, &str)> = vec![
            (json!({"bot_name": "   "}), "bot_name"),
            (json!({"bot_name": "a".repeat(61)}), "bot_name"),
            (json!({"bot_name": "two\nlines"}), "bot_name"),
            (json!({"welcome": "w".repeat(401)}), "welcome"),
            (json!({"welcome": "bell\u{7}"}), "welcome"),
            (
                json!({"suggested_questions": ["a?", "b?", "c?", "d?"]}),
                "suggested questions",
            ),
            (
                json!({"suggested_questions": ["q".repeat(161)]}),
                "suggested_questions",
            ),
            (json!({"suggested_questions": [" "]}), "suggested_questions"),
            (json!({"tone_note": "n".repeat(501)}), "tone_note"),
            (
                json!({"offline_message": "o".repeat(301)}),
                "offline_message",
            ),
            (json!({"offline_message": "two\nlines"}), "offline_message"),
            (json!({"avatar": "not/a/token"}), "avatar"),
        ];
        for (value, field) in cases {
            match SiteChatAppearance::from_value(value.clone()) {
                Err(ChatAppearanceError::Invalid(msg)) => assert!(
                    msg.contains(field.split(' ').next().unwrap()),
                    "error for {value} does not name {field}: {msg}"
                ),
                other => panic!("{value} should be Invalid, got {other:?}"),
            }
        }
    }

    #[test]
    fn unknown_props_bad_enums_and_future_versions_are_rejected() {
        assert!(matches!(
            SiteChatAppearance::from_value(json!({"custom_css": ".x{}"})),
            Err(ChatAppearanceError::Shape(_))
        ));
        assert!(matches!(
            SiteChatAppearance::from_value(json!({"accent": "#ffff00"})),
            Err(ChatAppearanceError::Shape(_))
        ));
        assert!(matches!(
            SiteChatAppearance::from_value(json!({"tone": "sarcastic"})),
            Err(ChatAppearanceError::Shape(_))
        ));
        assert!(matches!(
            SiteChatAppearance::from_value(json!({"schema_version": 2})),
            Err(ChatAppearanceError::UnsupportedVersion(2))
        ));
    }

    /// WCAG relative luminance of a `#rrggbb` colour (the same arithmetic as
    /// `site_theme`'s build-time check).
    fn luminance(hex: &str) -> f64 {
        let channel = |i: usize| {
            let raw = u8::from_str_radix(&hex[i..i + 2], 16).unwrap() as f64 / 255.0;
            if raw <= 0.03928 {
                raw / 12.92
            } else {
                ((raw + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(1) + 0.7152 * channel(3) + 0.0722 * channel(5)
    }

    fn contrast(a: &str, b: &str) -> f64 {
        let (la, lb) = (luminance(a), luminance(b));
        (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
    }

    fn role(palette: &Palette, name: &str) -> &'static str {
        match name {
            "background" => palette.background,
            "surface" => palette.surface,
            "text" => palette.text,
            "primary" => palette.primary,
            "on_primary" => palette.on_primary,
            other => panic!("accent role pair names unknown palette role {other}"),
        }
    }

    /// The queue item's mandate, provable at build time: **no storable
    /// accent, on any shipped preset, can produce failing contrast.** Every
    /// accent is a fill/label pair of palette roles, and every pair meets
    /// WCAG AA on every preset — which is the whole reason colour is a role
    /// choice rather than a picker.
    #[test]
    fn every_accent_role_pair_meets_wcag_aa_on_every_shipped_preset() {
        let accents = [
            ChatWidgetAccent::Primary,
            ChatWidgetAccent::Text,
            ChatWidgetAccent::Surface,
        ];
        for preset in THEME_PRESETS {
            for accent in accents {
                let (fill, label) = accent.role_pair();
                let ratio = contrast(role(&preset.palette, fill), role(&preset.palette, label));
                assert!(
                    ratio >= 4.5,
                    "{}: accent {accent:?} ({label} on {fill}) contrast {ratio:.2} < 4.5",
                    preset.id
                );
            }
        }
    }
}
