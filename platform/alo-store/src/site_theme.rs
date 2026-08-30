//! The typed theme model for alo Sites (schema v1, ADR 0036).
//!
//! A site's `theme` column holds the envelope [`SiteTheme`] —
//! `{ "schema_version": 1, "preset": "north", … }` — a shipped preset id plus
//! optional brand-colour and image overrides. Presets remain the safe starting
//! point; custom colours are validated at the write gate and become reusable
//! site-wide tokens rather than one-off values stored inside sections.
//!
//! Presets carry their palette and typography as static tokens; the
//! stylesheet generator (S1.07) reads them from here, so the editor UI, the
//! renderer, and the stored JSON can never disagree about what a preset
//! means. Font stacks are **system fonts only** — a published site loads no
//! third-party font (or any other cross-origin resource); that is part of the
//! product's privacy promise, not an optimization.
//!
//! Like [`crate::site_model`], this module is pure types + validation — no
//! persistence. The store validates through [`SiteTheme::from_value`] on
//! every write; readers that must never fail (the renderer) use
//! [`SiteTheme::from_stored`], which falls back to the default theme.

use serde::{Deserialize, Serialize};

use crate::id::BlobId;
use crate::site_model::valid_id_token;

/// The current theme schema version. Bumps ship a pure upgrade function
/// applied on read; stored JSON is rewritten lazily on the next save.
pub const THEME_SCHEMA_VERSION: u64 = 1;

/// The preset a site starts with (and the fallback for a pristine `{}` theme).
pub const DEFAULT_THEME_PRESET: &str = "north";

/// Why a theme value was rejected. Messages are field-level validation
/// details, safe to surface on the wire as a 422 — they never echo the
/// submitted value.
#[derive(Debug, thiserror::Error)]
pub enum ThemeSchemaError {
    /// The envelope declares a schema version this build does not speak.
    #[error("unsupported theme schema_version {0} (this build speaks {THEME_SCHEMA_VERSION})")]
    UnsupportedVersion(u64),
    /// The JSON does not fit the typed schema: unknown prop, missing required
    /// prop, or a wrong-typed value.
    #[error("theme JSON does not match schema v{THEME_SCHEMA_VERSION}: {0}")]
    Shape(#[from] serde_json::Error),
    /// Structurally well-typed but violating a content rule (unknown preset,
    /// malformed blob ref). The message names the field and the rule.
    #[error("theme: {0}")]
    Invalid(String),
}

/// The versioned value stored in a site's `theme` column.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SiteTheme {
    /// Schema version of the theme; this build speaks
    /// [`THEME_SCHEMA_VERSION`].
    pub schema_version: u64,
    /// Id of a shipped [`ThemePreset`] — the only palette/typography source
    /// in v1.
    pub preset: String,
    /// The site's logo (a tenant blob). The renderer uses the site name as
    /// the logo's alt text; without a logo, the nav shows the name as text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logo: Option<BlobId>,
    /// The site's favicon (a tenant blob); browsers fall back to none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub favicon: Option<BlobId>,
    /// Optional site-wide brand palette. Sections refer to these named roles;
    /// they never persist arbitrary colour values of their own.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub colors: Option<BrandColors>,
}

/// Tenant-editable brand tokens. Base colours define the canvas; accents are
/// reusable roles offered by section editors as `Accent 1` … `Accent 5`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrandColors {
    pub background: String,
    pub text: String,
    pub border: String,
    pub accent_1: String,
    pub accent_2: String,
    pub accent_3: String,
    pub accent_4: String,
    pub accent_5: String,
}

impl SiteTheme {
    /// The default theme — [`DEFAULT_THEME_PRESET`], no logo, no favicon.
    /// Also what a pristine `{}` theme column reads as.
    pub fn new() -> Self {
        SiteTheme {
            schema_version: THEME_SCHEMA_VERSION,
            preset: DEFAULT_THEME_PRESET.to_owned(),
            logo: None,
            favicon: None,
            colors: None,
        }
    }

    /// Parses and fully validates a wire theme value. This is the write gate:
    /// everything persisted goes through here first.
    ///
    /// # Errors
    /// [`ThemeSchemaError::UnsupportedVersion`] on a version this build does
    /// not speak (checked before shape, so a v2 payload gets the version
    /// error, not a confusing shape error); [`ThemeSchemaError::Shape`] on
    /// unknown/missing/mistyped props; [`ThemeSchemaError::Invalid`] on an
    /// unknown preset or malformed blob ref.
    pub fn from_value(value: serde_json::Value) -> Result<Self, ThemeSchemaError> {
        if let Some(version) = value
            .get("schema_version")
            .and_then(serde_json::Value::as_u64)
            && version != THEME_SCHEMA_VERSION
        {
            return Err(ThemeSchemaError::UnsupportedVersion(version));
        }
        let theme: Self = serde_json::from_value(value)?;
        theme.validate()?;
        Ok(theme)
    }

    /// The read-side spelling for callers that must never fail (the
    /// renderer): a pristine `{}` column — a site that never set a theme —
    /// reads as [`SiteTheme::new`]. Anything else invalid also falls back to
    /// the default, defensively; the write gate makes that path unreachable
    /// for values this build stored.
    pub fn from_stored(value: serde_json::Value) -> Self {
        Self::from_value(value).unwrap_or_default()
    }

    /// Serializes back to the stored JSON shape.
    ///
    /// # Errors
    /// [`ThemeSchemaError::Shape`] — cannot occur for values built from these
    /// types, but serialization is fallible by signature.
    pub fn to_value(&self) -> Result<serde_json::Value, ThemeSchemaError> {
        Ok(serde_json::to_value(self)?)
    }

    /// Content-rule validation: version, shipped preset, blob token shapes.
    ///
    /// # Errors
    /// The specific [`ThemeSchemaError`] variant naming the violated rule.
    pub fn validate(&self) -> Result<(), ThemeSchemaError> {
        if self.schema_version != THEME_SCHEMA_VERSION {
            return Err(ThemeSchemaError::UnsupportedVersion(self.schema_version));
        }
        if theme_preset(&self.preset).is_none() {
            return Err(ThemeSchemaError::Invalid(
                "preset is not a shipped theme preset".to_owned(),
            ));
        }
        if let Some(logo) = &self.logo
            && !valid_id_token(logo.as_str())
        {
            return Err(ThemeSchemaError::Invalid(
                "logo is not a valid blob id".to_owned(),
            ));
        }
        if let Some(favicon) = &self.favicon
            && !valid_id_token(favicon.as_str())
        {
            return Err(ThemeSchemaError::Invalid(
                "favicon is not a valid blob id".to_owned(),
            ));
        }
        if let Some(colors) = &self.colors {
            for (field, value) in [
                ("background", &colors.background),
                ("text", &colors.text),
                ("border", &colors.border),
                ("accent_1", &colors.accent_1),
                ("accent_2", &colors.accent_2),
                ("accent_3", &colors.accent_3),
                ("accent_4", &colors.accent_4),
                ("accent_5", &colors.accent_5),
            ] {
                if !is_hex_colour(value) {
                    return Err(ThemeSchemaError::Invalid(format!(
                        "colors.{field} must be a six-digit hex colour such as #1d4ed8"
                    )));
                }
            }
            if contrast(&colors.background, &colors.text) < 4.5 {
                return Err(ThemeSchemaError::Invalid(
                    "colors.text must have at least 4.5:1 contrast against colors.background"
                        .to_owned(),
                ));
            }
        }
        Ok(())
    }

    /// The shipped preset this theme points at.
    pub fn resolved_preset(&self) -> &'static ThemePreset {
        // A validated theme always resolves; the default covers the
        // defensive read path.
        theme_preset(&self.preset).unwrap_or(&THEME_PRESETS[0])
    }
}

fn is_hex_colour(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
}

fn contrast(first: &str, second: &str) -> f64 {
    fn luminance(hex: &str) -> f64 {
        let channel = |index: usize| {
            let raw = u8::from_str_radix(&hex[index..index + 2], 16).unwrap_or(0) as f64 / 255.0;
            if raw <= 0.03928 {
                raw / 12.92
            } else {
                ((raw + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(1) + 0.7152 * channel(3) + 0.0722 * channel(5)
    }
    let (a, b) = (luminance(first), luminance(second));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

impl Default for SiteTheme {
    fn default() -> Self {
        Self::new()
    }
}

/// Color tokens of a preset, as `#rrggbb` hex. Every pairing the renderer
/// puts text on (text/background, text/surface, `on_primary`/`primary`,
/// `muted_text`/background) meets WCAG AA (≥ 4.5:1), enforced by a unit test
/// — which is why v1 ships presets instead of free-form colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Palette {
    /// Page background.
    pub background: &'static str,
    /// Card/alternate-band background.
    pub surface: &'static str,
    /// Body text (on background and surface).
    pub text: &'static str,
    /// Secondary text (on background and surface).
    pub muted_text: &'static str,
    /// Buttons, links, accents.
    pub primary: &'static str,
    /// Text on a `primary` fill.
    pub on_primary: &'static str,
    /// Hairlines and card borders (decorative; no contrast requirement).
    pub border: &'static str,
}

/// Typography tokens of a preset. Families are CSS `font-family` stacks of
/// system fonts only — a published site never loads a third-party font.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Typography {
    /// Heading font stack.
    pub heading_family: &'static str,
    /// Body font stack.
    pub body_family: &'static str,
    /// Heading weight (400–900 in hundreds).
    pub heading_weight: u16,
}

/// One shipped palette+typography preset. The `name` is a product proper noun
/// (like a paint color), shown as-is in every locale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ThemePreset {
    /// Stable id stored in site themes (`[a-z0-9-]`).
    pub id: &'static str,
    /// Display name (proper noun, not translated).
    pub name: &'static str,
    /// Color tokens.
    pub palette: Palette,
    /// Type tokens.
    pub typography: Typography,
}

const SANS: &str =
    "system-ui, -apple-system, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif";
const SERIF: &str = "'Iowan Old Style', 'Palatino Linotype', Palatino, Georgia, serif";
const SERIF_TEXT: &str = "Georgia, 'Times New Roman', Times, serif";
const HUMANIST: &str = "Seravek, 'Gill Sans Nova', Ubuntu, Calibri, 'DejaVu Sans', sans-serif";
const MONO: &str = "ui-monospace, 'Cascadia Code', Menlo, Consolas, 'Liberation Mono', monospace";

/// The shipped presets, picker order. The first entry is the default
/// ([`DEFAULT_THEME_PRESET`]).
pub const THEME_PRESETS: &[ThemePreset] = &[
    ThemePreset {
        id: "north",
        name: "North",
        palette: Palette {
            background: "#ffffff",
            surface: "#f2f5f8",
            text: "#17212b",
            muted_text: "#4c5866",
            primary: "#1d4ed8",
            on_primary: "#ffffff",
            border: "#dde3e9",
        },
        typography: Typography {
            heading_family: SANS,
            body_family: SANS,
            heading_weight: 700,
        },
    },
    ThemePreset {
        id: "ink",
        name: "Ink",
        palette: Palette {
            background: "#12161c",
            surface: "#1a2029",
            text: "#e7ebf0",
            muted_text: "#9aa5b1",
            primary: "#8ab4ff",
            on_primary: "#0b1a33",
            border: "#2a323d",
        },
        typography: Typography {
            heading_family: SANS,
            body_family: SANS,
            heading_weight: 600,
        },
    },
    ThemePreset {
        id: "terra",
        name: "Terra",
        palette: Palette {
            background: "#faf6ef",
            surface: "#f2eadd",
            text: "#38291d",
            muted_text: "#6e5844",
            primary: "#9c3d1e",
            on_primary: "#ffffff",
            border: "#e4d8c6",
        },
        typography: Typography {
            heading_family: SERIF_TEXT,
            body_family: SANS,
            heading_weight: 700,
        },
    },
    ThemePreset {
        id: "fern",
        name: "Fern",
        palette: Palette {
            background: "#f6faf6",
            surface: "#eaf2ea",
            text: "#1c2a1f",
            muted_text: "#526456",
            primary: "#216a45",
            on_primary: "#ffffff",
            border: "#d8e4d9",
        },
        typography: Typography {
            heading_family: HUMANIST,
            body_family: HUMANIST,
            heading_weight: 600,
        },
    },
    ThemePreset {
        id: "plum",
        name: "Plum",
        palette: Palette {
            background: "#fbf8fc",
            surface: "#f3ecf6",
            text: "#2b2130",
            muted_text: "#6b5a73",
            primary: "#71279e",
            on_primary: "#ffffff",
            border: "#e5dbea",
        },
        typography: Typography {
            heading_family: SERIF,
            body_family: SERIF_TEXT,
            heading_weight: 700,
        },
    },
    ThemePreset {
        id: "carbon",
        name: "Carbon",
        palette: Palette {
            background: "#ffffff",
            surface: "#f5f5f5",
            text: "#141414",
            muted_text: "#525252",
            primary: "#141414",
            on_primary: "#ffffff",
            border: "#e2e2e2",
        },
        typography: Typography {
            heading_family: MONO,
            body_family: SANS,
            heading_weight: 700,
        },
    },
    ThemePreset {
        id: "midnight",
        name: "Midnight",
        palette: Palette {
            background: "#0f1a2b",
            surface: "#182740",
            text: "#e8edf6",
            muted_text: "#9fabc4",
            primary: "#f0b653",
            on_primary: "#26180a",
            border: "#263c5e",
        },
        typography: Typography {
            heading_family: SANS,
            body_family: SANS,
            heading_weight: 700,
        },
    },
];

/// Looks up a shipped preset by id.
pub fn theme_preset(id: &str) -> Option<&'static ThemePreset> {
    THEME_PRESETS.iter().find(|preset| preset.id == id)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use serde_json::json;

    use super::*;

    #[test]
    fn at_least_six_presets_ship_with_wellformed_unique_ids() {
        assert!(
            THEME_PRESETS.len() >= 6,
            "the queue requires at least six shipped presets"
        );
        let mut seen = std::collections::HashSet::new();
        for preset in THEME_PRESETS {
            assert!(
                !preset.id.is_empty()
                    && preset
                        .id
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
                "preset id not a lowercase token: {}",
                preset.id
            );
            assert!(seen.insert(preset.id), "duplicate preset id: {}", preset.id);
            assert!(!preset.name.trim().is_empty());
            let weight = preset.typography.heading_weight;
            assert!(
                (400..=900).contains(&weight) && weight % 100 == 0,
                "odd heading weight on {}: {weight}",
                preset.id
            );
        }
        assert_eq!(THEME_PRESETS[0].id, DEFAULT_THEME_PRESET);
        assert!(theme_preset(DEFAULT_THEME_PRESET).is_some());
        assert!(theme_preset("no-such-preset").is_none());
    }

    /// WCAG relative luminance of a `#rrggbb` color.
    fn luminance(hex: &str) -> f64 {
        assert!(
            hex.len() == 7
                && hex.starts_with('#')
                && hex[1..]
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "not lowercase #rrggbb hex: {hex}"
        );
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

    /// The reason presets exist instead of free-form colors: every text
    /// pairing the renderer produces meets WCAG AA, provable at build time.
    #[test]
    fn every_preset_palette_is_hex_and_meets_wcag_aa_contrast() {
        for preset in THEME_PRESETS {
            let p = preset.palette;
            // luminance() also asserts the hex format of every token it sees;
            // border is decorative but must still be well-formed.
            luminance(p.border);
            for (fg, bg, pair) in [
                (p.text, p.background, "text/background"),
                (p.text, p.surface, "text/surface"),
                (p.muted_text, p.background, "muted_text/background"),
                (p.muted_text, p.surface, "muted_text/surface"),
                (p.on_primary, p.primary, "on_primary/primary"),
                // The stylesheet colors links and secondary buttons
                // `primary` on both page and card backgrounds.
                (p.primary, p.background, "primary/background"),
                (p.primary, p.surface, "primary/surface"),
            ] {
                let ratio = contrast(fg, bg);
                assert!(
                    ratio >= 4.5,
                    "{}: {pair} contrast {ratio:.2} < 4.5",
                    preset.id
                );
            }
        }
    }

    #[test]
    fn full_and_minimal_themes_round_trip() {
        let full = SiteTheme {
            schema_version: THEME_SCHEMA_VERSION,
            preset: "terra".to_owned(),
            logo: Some(BlobId::new("9hK3vQ2mR8pT1xWz4bC5dg")),
            favicon: Some(BlobId::new("f4K9sL2wN7qR5tYx8vB1cA")),
            colors: Some(BrandColors {
                background: "#ffffff".to_owned(),
                text: "#17212b".to_owned(),
                border: "#dde3e9".to_owned(),
                accent_1: "#1d4ed8".to_owned(),
                accent_2: "#216a45".to_owned(),
                accent_3: "#71279e".to_owned(),
                accent_4: "#9c3d1e".to_owned(),
                accent_5: "#4c5866".to_owned(),
            }),
        };
        full.validate().unwrap();
        assert_eq!(
            SiteTheme::from_value(full.to_value().unwrap()).unwrap(),
            full
        );

        let minimal = SiteTheme::new();
        let value = minimal.to_value().unwrap();
        // Absent options serialize as absent keys, not nulls.
        assert!(value.get("logo").is_none() && value.get("favicon").is_none());
        assert_eq!(SiteTheme::from_value(value).unwrap(), minimal);
    }

    #[test]
    fn default_theme_is_current_version_and_valid() {
        let theme = SiteTheme::new();
        assert_eq!(theme.schema_version, THEME_SCHEMA_VERSION);
        theme.validate().unwrap();
        assert_eq!(theme.resolved_preset().id, DEFAULT_THEME_PRESET);
    }

    #[test]
    fn custom_brand_colours_are_hex_and_keep_base_text_readable() {
        let valid = json!({
            "schema_version": 1,
            "preset": "north",
            "colors": {
                "background": "#ffffff", "text": "#17212b", "border": "#dde3e9",
                "accent_1": "#1d4ed8", "accent_2": "#216a45", "accent_3": "#71279e",
                "accent_4": "#9c3d1e", "accent_5": "#4c5866"
            }
        });
        assert!(SiteTheme::from_value(valid).is_ok());
        for invalid in [
            json!({
                "schema_version": 1, "preset": "north",
                "colors": {"background": "white", "text": "#17212b", "border": "#dde3e9", "accent_1": "#1d4ed8", "accent_2": "#216a45", "accent_3": "#71279e", "accent_4": "#9c3d1e", "accent_5": "#4c5866"}
            }),
            json!({
                "schema_version": 1, "preset": "north",
                "colors": {"background": "#ffffff", "text": "#eeeeee", "border": "#dde3e9", "accent_1": "#1d4ed8", "accent_2": "#216a45", "accent_3": "#71279e", "accent_4": "#9c3d1e", "accent_5": "#4c5866"}
            }),
        ] {
            assert!(matches!(
                SiteTheme::from_value(invalid),
                Err(ThemeSchemaError::Invalid(_))
            ));
        }
    }

    #[test]
    fn future_schema_version_gets_the_version_error_not_a_shape_error() {
        let value = json!({"schema_version": 2, "palette": {"custom": true}});
        assert!(matches!(
            SiteTheme::from_value(value),
            Err(ThemeSchemaError::UnsupportedVersion(2))
        ));
    }

    #[test]
    fn unknown_prop_missing_prop_and_bad_types_are_rejected() {
        for value in [
            json!({"schema_version": 1, "preset": "north", "bogus": true}),
            json!({"schema_version": 1}),
            json!({"preset": "north"}),
            json!({"schema_version": 1, "preset": 7}),
        ] {
            assert!(matches!(
                SiteTheme::from_value(value),
                Err(ThemeSchemaError::Shape(_))
            ));
        }
    }

    #[test]
    fn unknown_preset_and_malformed_blob_refs_are_rejected() {
        let unknown = json!({"schema_version": 1, "preset": "vaporwave"});
        assert!(matches!(
            SiteTheme::from_value(unknown),
            Err(ThemeSchemaError::Invalid(msg)) if msg.contains("preset")
        ));
        let bad_logo = json!({"schema_version": 1, "preset": "north", "logo": "not/a/token"});
        assert!(matches!(
            SiteTheme::from_value(bad_logo),
            Err(ThemeSchemaError::Invalid(msg)) if msg.contains("logo")
        ));
        let bad_favicon = json!({"schema_version": 1, "preset": "north", "favicon": ""});
        assert!(matches!(
            SiteTheme::from_value(bad_favicon),
            Err(ThemeSchemaError::Invalid(msg)) if msg.contains("favicon")
        ));
    }

    #[test]
    fn from_stored_reads_pristine_and_defensive_paths_as_the_default() {
        // A site that never set a theme stores the column default `{}`.
        assert_eq!(SiteTheme::from_stored(json!({})), SiteTheme::new());
        // Defensive: unreachable for values this build stored, but the
        // renderer must never fail on a theme read.
        assert_eq!(
            SiteTheme::from_stored(json!({"schema_version": 99})),
            SiteTheme::new()
        );
        let stored = json!({"schema_version": 1, "preset": "plum"});
        assert_eq!(SiteTheme::from_stored(stored).preset, "plum");
    }
}
