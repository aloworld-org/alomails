//! The typed section schema for alo Sites pages (schema v1, ADR 0036).
//!
//! A page's `sections` JSON is the envelope [`SectionsEnvelope`] —
//! `{ "schema_version": 1, "sections": [ … ] }` — where every entry is one
//! variant of [`Section`], an internally-tagged serde enum with a closed
//! vocabulary of thirteen section types. The schema is **strict on write**:
//! unknown section types and unknown props are validation errors here, because
//! the only writers (the editor UI and the AI ops path) speak this schema
//! exactly. Read-side tolerance (skip-with-log on unknown sections, so an old
//! renderer never 500s on a newer snapshot mid-deploy) is the renderer's job,
//! not this module's (`docs/design/sites.md`).
//!
//! This module is pure types + validation — no persistence. The page store
//! validates against it on every write; the renderer consumes the same types,
//! so the schema cannot drift between the two.

use serde::{Deserialize, Serialize};

use crate::id::{BlobId, SiteCollectionId};

/// The current sections schema version. Version bumps ship an explicit pure
/// upgrade function (v1 → v2) applied on read; stored JSON is rewritten
/// lazily on the next save.
pub const SECTIONS_SCHEMA_VERSION: u64 = 1;

/// Maximum sections on one page.
pub const MAX_SECTIONS_PER_PAGE: usize = 50;
/// Maximum entries in any section's list (features, images, tiers, …).
pub const MAX_ITEMS_PER_SECTION: usize = 50;
/// Character cap for short display strings (headings, labels, names).
pub const MAX_SHORT_TEXT_CHARS: usize = 300;
/// Character cap for long-form strings (bodies, quotes, answers).
pub const MAX_LONG_TEXT_CHARS: usize = 5_000;
/// Character cap for link targets.
pub const MAX_HREF_CHARS: usize = 2_000;
/// Character cap for opaque id tokens (blob refs, form refs).
const MAX_TOKEN_CHARS: usize = 64;
/// Character cap for icon name tokens.
const MAX_ICON_CHARS: usize = 40;

/// Why a sections value was rejected. Messages are field-level validation
/// details, safe to surface on the wire as a 422 — they never echo stored
/// content beyond what the writer just sent.
#[derive(Debug, thiserror::Error)]
pub enum SectionSchemaError {
    /// The envelope declares a schema version this build does not speak.
    #[error(
        "unsupported sections schema_version {0} (this build speaks {SECTIONS_SCHEMA_VERSION})"
    )]
    UnsupportedVersion(u64),
    /// The JSON does not fit the typed schema: unknown section type, unknown
    /// prop, missing required prop, or a wrong-typed value.
    #[error("sections JSON does not match schema v{SECTIONS_SCHEMA_VERSION}: {0}")]
    Shape(#[from] serde_json::Error),
    /// More sections than a page may hold.
    #[error("a page may have at most {MAX_SECTIONS_PER_PAGE} sections")]
    TooManySections,
    /// A section's props are structurally well-typed but violate a content
    /// rule (blank required text, over-cap length, unsafe href, empty list).
    #[error("{section} section: {detail}")]
    Invalid {
        /// The section's wire tag (e.g. `hero`).
        section: &'static str,
        /// The violated rule, named for the UI.
        detail: String,
    },
}

/// A link: visible label + target. Targets are restricted to site-relative
/// paths, fragments, and `http(s)`/`mailto:`/`tel:` URLs — never script-able
/// schemes — so a stored href is always safe to render into an `href`
/// attribute.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Link {
    /// Visible link text.
    pub label: String,
    /// Link target (validated by [`SectionsEnvelope::validate`]).
    pub href: String,
}

/// An image reference: a tenant blob plus its alt text. `alt` is a required
/// prop (the renderer emits `alt` on every `<img>`); an empty string is the
/// deliberate spelling for a decorative image.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SiteImage {
    /// The tenant blob holding the image bytes.
    pub blob_id: BlobId,
    /// Alt text; empty means decorative.
    pub alt: String,
}

/// Which side of the text a `text_image` section puts its image on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageSide {
    /// Image left, text right.
    Left,
    /// Image right, text left.
    Right,
}

/// Top navigation bar. The logo comes from the site's theme, not from here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NavSection {
    /// Menu links, in order. May be empty (logo-only nav).
    pub links: Vec<Link>,
    /// Optional highlighted call-to-action button.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cta: Option<Link>,
}

/// The page's lead banner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeroSection {
    /// Main headline.
    pub heading: String,
    /// Supporting line under the headline.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subheading: Option<String>,
    /// Optional hero image.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<SiteImage>,
    /// Primary call-to-action button.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_cta: Option<Link>,
    /// Secondary, quieter call-to-action.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secondary_cta: Option<Link>,
}

/// One entry in a `features` grid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureItem {
    /// Feature name.
    pub title: String,
    /// One-or-two-sentence description.
    pub body: String,
    /// Optional named icon token (`[a-z0-9-]`); the renderer falls back to no
    /// icon on a token it doesn't ship.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

/// A grid of product/service features.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeaturesSection {
    /// Section heading.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heading: Option<String>,
    /// Short intro paragraph under the heading.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intro: Option<String>,
    /// The features; at least one.
    pub items: Vec<FeatureItem>,
}

/// A text block alongside an image.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextImageSection {
    /// Section heading.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heading: Option<String>,
    /// The paragraph text.
    pub body: String,
    /// The image.
    pub image: SiteImage,
    /// Which side the image sits on.
    pub image_side: ImageSide,
}

/// An image gallery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GallerySection {
    /// Section heading.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heading: Option<String>,
    /// The images; at least one.
    pub images: Vec<SiteImage>,
}

/// One customer quote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Testimonial {
    /// The quote text.
    pub quote: String,
    /// Who said it.
    pub author: String,
    /// Their role/company line.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

/// A row of customer quotes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestimonialsSection {
    /// Section heading.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heading: Option<String>,
    /// The quotes; at least one.
    pub items: Vec<Testimonial>,
}

/// One pricing tier. `price` is a **display string** ("€9/mo", "Sur mesure"),
/// not a money value — nothing computes on it, so the integer-cents law is
/// not in play here (`docs/design/sites.md`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PricingTier {
    /// Tier name.
    pub name: String,
    /// Display price string — never parsed, never computed on.
    pub price: String,
    /// Billing-period line under the price ("per month").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub period: Option<String>,
    /// One-line tier description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Included-feature bullet lines.
    pub features: Vec<String>,
    /// Tier call-to-action.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cta: Option<Link>,
    /// Visually emphasize this tier ("most popular").
    #[serde(default)]
    pub highlighted: bool,
}

/// A pricing table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PricingSection {
    /// Section heading.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heading: Option<String>,
    /// Short intro paragraph under the heading.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intro: Option<String>,
    /// The tiers; at least one.
    pub tiers: Vec<PricingTier>,
}

/// One person on a `team` section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeamMember {
    /// Display name.
    pub name: String,
    /// Role line.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Portrait image.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub photo: Option<SiteImage>,
    /// Short bio.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bio: Option<String>,
}

/// The people behind the business.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeamSection {
    /// Section heading.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heading: Option<String>,
    /// The members; at least one.
    pub members: Vec<TeamMember>,
}

/// One question/answer pair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FaqItem {
    /// The question.
    pub question: String,
    /// The answer.
    pub answer: String,
}

/// A frequently-asked-questions list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FaqSection {
    /// Section heading.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heading: Option<String>,
    /// The Q/A pairs; at least one.
    pub items: Vec<FaqItem>,
}

/// A standalone call-to-action banner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CtaSection {
    /// Banner headline.
    pub heading: String,
    /// Supporting line.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    /// The action button.
    pub button: Link,
}

/// A contact form. The form itself (fields, submissions) is a separate store
/// object; this section points at it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContactFormSection {
    /// Section heading.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heading: Option<String>,
    /// Line above the form.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    /// The tenant form this section submits to; `None` until the forms slice
    /// wires it (the renderer shows the section without a working submit).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub form_id: Option<String>,
    /// Message shown after a successful submit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub success_message: Option<String>,
}

/// The page footer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FooterSection {
    /// Footer line (e.g. the copyright notice).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Footer links (imprint, privacy, socials). May be empty.
    pub links: Vec<Link>,
}

/// A reusable card collection whose rows are frozen from alo Base at
/// publish time. The section stores only the stable binding id and optional
/// presentation heading; public rendering never reads the live Base.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollectionSection {
    pub collection_id: SiteCollectionId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heading: Option<String>,
}

/// One section of a page — the closed v1 vocabulary. The wire tag is the
/// `type` prop (`{"type": "hero", …}`); unknown tags and unknown props are
/// rejected on write.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Section {
    /// Top navigation bar.
    Nav(NavSection),
    /// Lead banner.
    Hero(HeroSection),
    /// Feature grid.
    Features(FeaturesSection),
    /// Text alongside an image.
    TextImage(TextImageSection),
    /// Image gallery.
    Gallery(GallerySection),
    /// Customer quotes.
    Testimonials(TestimonialsSection),
    /// Pricing table.
    Pricing(PricingSection),
    /// Team members.
    Team(TeamSection),
    /// Q/A list.
    Faq(FaqSection),
    /// Call-to-action banner.
    Cta(CtaSection),
    /// Contact form.
    ContactForm(ContactFormSection),
    /// Cards resolved from an alo Base collection at publish time.
    Collection(CollectionSection),
    /// Page footer.
    Footer(FooterSection),
}

impl Section {
    /// The section's wire tag — the exact string serde writes as `type`.
    pub fn kind(&self) -> &'static str {
        match self {
            Section::Nav(_) => "nav",
            Section::Hero(_) => "hero",
            Section::Features(_) => "features",
            Section::TextImage(_) => "text_image",
            Section::Gallery(_) => "gallery",
            Section::Testimonials(_) => "testimonials",
            Section::Pricing(_) => "pricing",
            Section::Team(_) => "team",
            Section::Faq(_) => "faq",
            Section::Cta(_) => "cta",
            Section::ContactForm(_) => "contact_form",
            Section::Collection(_) => "collection",
            Section::Footer(_) => "footer",
        }
    }

    /// The tenant blobs this section's images reference, in document order.
    /// This is the reference set renderers and the public image path work
    /// from: everything a page can show is exactly what this returns (plus
    /// the theme's logo/favicon). The match is deliberately exhaustive — a
    /// new section variant fails to compile until it declares its images.
    pub fn image_blob_ids(&self) -> Vec<&BlobId> {
        match self {
            Section::Hero(s) => s.image.iter().map(|i| &i.blob_id).collect(),
            Section::TextImage(s) => vec![&s.image.blob_id],
            Section::Gallery(s) => s.images.iter().map(|i| &i.blob_id).collect(),
            Section::Team(s) => s
                .members
                .iter()
                .filter_map(|m| m.photo.as_ref().map(|i| &i.blob_id))
                .collect(),
            Section::Nav(_)
            | Section::Features(_)
            | Section::Testimonials(_)
            | Section::Pricing(_)
            | Section::Faq(_)
            | Section::Cta(_)
            | Section::ContactForm(_)
            | Section::Collection(_)
            | Section::Footer(_) => Vec::new(),
        }
    }

    /// Content-rule validation for this section (structural typing is already
    /// guaranteed by serde at parse time).
    fn validate(&self) -> Result<(), SectionSchemaError> {
        let kind = self.kind();
        match self {
            Section::Nav(s) => {
                check_len(kind, "links", s.links.len(), false)?;
                for link in &s.links {
                    check_link(kind, link)?;
                }
                check_opt_link(kind, s.cta.as_ref())
            }
            Section::Hero(s) => {
                check_short(kind, "heading", &s.heading)?;
                check_opt_short(kind, "subheading", s.subheading.as_deref())?;
                check_opt_image(kind, s.image.as_ref())?;
                check_opt_link(kind, s.primary_cta.as_ref())?;
                check_opt_link(kind, s.secondary_cta.as_ref())
            }
            Section::Features(s) => {
                check_opt_short(kind, "heading", s.heading.as_deref())?;
                check_opt_long(kind, "intro", s.intro.as_deref())?;
                check_len(kind, "items", s.items.len(), true)?;
                for item in &s.items {
                    check_short(kind, "title", &item.title)?;
                    check_long(kind, "body", &item.body)?;
                    if let Some(icon) = &item.icon {
                        check_icon(kind, icon)?;
                    }
                }
                Ok(())
            }
            Section::TextImage(s) => {
                check_opt_short(kind, "heading", s.heading.as_deref())?;
                check_long(kind, "body", &s.body)?;
                check_image(kind, &s.image)
            }
            Section::Gallery(s) => {
                check_opt_short(kind, "heading", s.heading.as_deref())?;
                check_len(kind, "images", s.images.len(), true)?;
                for image in &s.images {
                    check_image(kind, image)?;
                }
                Ok(())
            }
            Section::Testimonials(s) => {
                check_opt_short(kind, "heading", s.heading.as_deref())?;
                check_len(kind, "items", s.items.len(), true)?;
                for item in &s.items {
                    check_long(kind, "quote", &item.quote)?;
                    check_short(kind, "author", &item.author)?;
                    check_opt_short(kind, "role", item.role.as_deref())?;
                }
                Ok(())
            }
            Section::Pricing(s) => {
                check_opt_short(kind, "heading", s.heading.as_deref())?;
                check_opt_long(kind, "intro", s.intro.as_deref())?;
                check_len(kind, "tiers", s.tiers.len(), true)?;
                for tier in &s.tiers {
                    check_short(kind, "name", &tier.name)?;
                    check_short(kind, "price", &tier.price)?;
                    check_opt_short(kind, "period", tier.period.as_deref())?;
                    check_opt_long(kind, "description", tier.description.as_deref())?;
                    check_len(kind, "features", tier.features.len(), false)?;
                    for feature in &tier.features {
                        check_short(kind, "features", feature)?;
                    }
                    check_opt_link(kind, tier.cta.as_ref())?;
                }
                Ok(())
            }
            Section::Team(s) => {
                check_opt_short(kind, "heading", s.heading.as_deref())?;
                check_len(kind, "members", s.members.len(), true)?;
                for member in &s.members {
                    check_short(kind, "name", &member.name)?;
                    check_opt_short(kind, "role", member.role.as_deref())?;
                    check_opt_image(kind, member.photo.as_ref())?;
                    check_opt_long(kind, "bio", member.bio.as_deref())?;
                }
                Ok(())
            }
            Section::Faq(s) => {
                check_opt_short(kind, "heading", s.heading.as_deref())?;
                check_len(kind, "items", s.items.len(), true)?;
                for item in &s.items {
                    check_short(kind, "question", &item.question)?;
                    check_long(kind, "answer", &item.answer)?;
                }
                Ok(())
            }
            Section::Cta(s) => {
                check_short(kind, "heading", &s.heading)?;
                check_opt_long(kind, "body", s.body.as_deref())?;
                check_link(kind, &s.button)
            }
            Section::ContactForm(s) => {
                check_opt_short(kind, "heading", s.heading.as_deref())?;
                check_opt_long(kind, "body", s.body.as_deref())?;
                if let Some(form_id) = &s.form_id {
                    check_token(kind, "form_id", form_id)?;
                }
                check_opt_short(kind, "success_message", s.success_message.as_deref())
            }
            Section::Collection(s) => {
                check_token(kind, "collection_id", s.collection_id.as_str())?;
                check_opt_short(kind, "heading", s.heading.as_deref())
            }
            Section::Footer(s) => {
                check_opt_short(kind, "text", s.text.as_deref())?;
                check_len(kind, "links", s.links.len(), false)?;
                for link in &s.links {
                    check_link(kind, link)?;
                }
                Ok(())
            }
        }
    }
}

/// The versioned value stored in a page's `sections` column.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SectionsEnvelope {
    /// Schema version of `sections`; this build speaks
    /// [`SECTIONS_SCHEMA_VERSION`].
    pub schema_version: u64,
    /// The page's sections, in render order.
    pub sections: Vec<Section>,
}

impl SectionsEnvelope {
    /// An empty current-version envelope — a new page's starting value.
    pub fn new() -> Self {
        SectionsEnvelope {
            schema_version: SECTIONS_SCHEMA_VERSION,
            sections: Vec::new(),
        }
    }

    /// Parses and fully validates a stored/wire sections value. This is the
    /// write gate: everything persisted goes through here first.
    ///
    /// # Errors
    /// [`SectionSchemaError::UnsupportedVersion`] on a version this build
    /// does not speak (checked before shape, so a v2 payload gets the version
    /// error, not a confusing shape error); [`SectionSchemaError::Shape`] on
    /// unknown types/props or missing/mistyped values; the content-rule
    /// variants from [`Self::validate`].
    pub fn from_value(value: serde_json::Value) -> Result<Self, SectionSchemaError> {
        if let Some(version) = value
            .get("schema_version")
            .and_then(serde_json::Value::as_u64)
            && version != SECTIONS_SCHEMA_VERSION
        {
            return Err(SectionSchemaError::UnsupportedVersion(version));
        }
        let envelope: Self = serde_json::from_value(value)?;
        envelope.validate()?;
        Ok(envelope)
    }

    /// Serializes back to the stored JSON shape.
    ///
    /// # Errors
    /// [`SectionSchemaError::Shape`] — cannot occur for values built from
    /// these types, but serialization is fallible by signature.
    pub fn to_value(&self) -> Result<serde_json::Value, SectionSchemaError> {
        Ok(serde_json::to_value(self)?)
    }

    /// Content-rule validation: version, section count, and every section's
    /// rules (text bounds, href safety, non-empty item lists).
    ///
    /// # Errors
    /// The specific [`SectionSchemaError`] variant naming the violated rule.
    pub fn validate(&self) -> Result<(), SectionSchemaError> {
        if self.schema_version != SECTIONS_SCHEMA_VERSION {
            return Err(SectionSchemaError::UnsupportedVersion(self.schema_version));
        }
        if self.sections.len() > MAX_SECTIONS_PER_PAGE {
            return Err(SectionSchemaError::TooManySections);
        }
        for section in &self.sections {
            section.validate()?;
        }
        Ok(())
    }
}

impl Default for SectionsEnvelope {
    fn default() -> Self {
        Self::new()
    }
}

// ---- content-rule helpers ---------------------------------------------------

fn invalid(section: &'static str, detail: String) -> SectionSchemaError {
    SectionSchemaError::Invalid { section, detail }
}

/// A required short string: non-blank, within [`MAX_SHORT_TEXT_CHARS`].
fn check_short(
    section: &'static str,
    field: &'static str,
    value: &str,
) -> Result<(), SectionSchemaError> {
    check_text(section, field, value, MAX_SHORT_TEXT_CHARS)
}

/// A required long-form string: non-blank, within [`MAX_LONG_TEXT_CHARS`].
fn check_long(
    section: &'static str,
    field: &'static str,
    value: &str,
) -> Result<(), SectionSchemaError> {
    check_text(section, field, value, MAX_LONG_TEXT_CHARS)
}

fn check_opt_short(
    section: &'static str,
    field: &'static str,
    value: Option<&str>,
) -> Result<(), SectionSchemaError> {
    value.map_or(Ok(()), |v| check_short(section, field, v))
}

fn check_opt_long(
    section: &'static str,
    field: &'static str,
    value: Option<&str>,
) -> Result<(), SectionSchemaError> {
    value.map_or(Ok(()), |v| check_long(section, field, v))
}

fn check_text(
    section: &'static str,
    field: &'static str,
    value: &str,
    max_chars: usize,
) -> Result<(), SectionSchemaError> {
    if value.trim().is_empty() {
        return Err(invalid(section, format!("{field} must not be blank")));
    }
    if value.chars().count() > max_chars {
        return Err(invalid(
            section,
            format!("{field} must be at most {max_chars} characters"),
        ));
    }
    Ok(())
}

/// List bounds: within [`MAX_ITEMS_PER_SECTION`], and non-empty where the
/// section is meaningless without entries.
fn check_len(
    section: &'static str,
    field: &'static str,
    len: usize,
    require_one: bool,
) -> Result<(), SectionSchemaError> {
    if require_one && len == 0 {
        return Err(invalid(
            section,
            format!("{field} must have at least one entry"),
        ));
    }
    if len > MAX_ITEMS_PER_SECTION {
        return Err(invalid(
            section,
            format!("{field} may have at most {MAX_ITEMS_PER_SECTION} entries"),
        ));
    }
    Ok(())
}

fn check_link(section: &'static str, link: &Link) -> Result<(), SectionSchemaError> {
    check_short(section, "link label", &link.label)?;
    check_href(section, &link.href)
}

fn check_opt_link(section: &'static str, link: Option<&Link>) -> Result<(), SectionSchemaError> {
    link.map_or(Ok(()), |l| check_link(section, l))
}

/// Href safety: site-relative paths (`/…` but not `//…`), fragments (`#…`),
/// and `http(s)`/`mailto:`/`tel:` targets only. Scheme matching is
/// case-insensitive so `JavaScript:` cannot slip past; everything not on the
/// allowlist is rejected.
fn check_href(section: &'static str, href: &str) -> Result<(), SectionSchemaError> {
    if href.is_empty() {
        return Err(invalid(section, "link href must not be empty".to_owned()));
    }
    if href.chars().count() > MAX_HREF_CHARS {
        return Err(invalid(
            section,
            format!("link href must be at most {MAX_HREF_CHARS} characters"),
        ));
    }
    if href.starts_with("//") {
        return Err(invalid(
            section,
            "link href may not be protocol-relative".to_owned(),
        ));
    }
    if href.starts_with('/') || href.starts_with('#') {
        return Ok(());
    }
    let lower = href.to_ascii_lowercase();
    if ["http://", "https://", "mailto:", "tel:"]
        .iter()
        .any(|scheme| lower.starts_with(scheme))
    {
        return Ok(());
    }
    Err(invalid(
        section,
        "link href must be a site path, #fragment, or http(s)/mailto/tel URL".to_owned(),
    ))
}

fn check_image(section: &'static str, image: &SiteImage) -> Result<(), SectionSchemaError> {
    check_token(section, "image blob_id", image.blob_id.as_str())?;
    // Alt may be empty (decorative) but stays bounded.
    if image.alt.chars().count() > MAX_SHORT_TEXT_CHARS {
        return Err(invalid(
            section,
            format!("image alt must be at most {MAX_SHORT_TEXT_CHARS} characters"),
        ));
    }
    Ok(())
}

fn check_opt_image(
    section: &'static str,
    image: Option<&SiteImage>,
) -> Result<(), SectionSchemaError> {
    image.map_or(Ok(()), |i| check_image(section, i))
}

/// An opaque id token: non-empty, bounded, URL-safe base64 charset — the shape
/// every store id has, safe to embed in URLs and HTML attributes. Shared with
/// the theme model ([`crate::site_theme`]) so "a valid id" means one thing
/// across the sites schema family.
pub(crate) fn valid_id_token(token: &str) -> bool {
    !token.is_empty()
        && token.len() <= MAX_TOKEN_CHARS
        && token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn check_token(
    section: &'static str,
    field: &'static str,
    token: &str,
) -> Result<(), SectionSchemaError> {
    if !valid_id_token(token) {
        return Err(invalid(section, format!("{field} is not a valid id")));
    }
    Ok(())
}

/// An icon name token: `[a-z0-9-]`, bounded.
fn check_icon(section: &'static str, icon: &str) -> Result<(), SectionSchemaError> {
    if icon.is_empty()
        || icon.len() > MAX_ICON_CHARS
        || !icon
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err(invalid(
            section,
            "icon must be a lowercase token of letters, digits, and hyphens".to_owned(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use serde_json::{Value, json};

    use super::*;

    /// One fully-populated instance of every section variant — the exhaustive
    /// round-trip corpus.
    fn full_sections() -> Vec<Section> {
        let image = SiteImage {
            blob_id: BlobId::new("9hK3vQ2mR8pT1xWz4bC5dg"),
            alt: "Roasting drum mid-batch".to_owned(),
        };
        let link = |label: &str, href: &str| Link {
            label: label.to_owned(),
            href: href.to_owned(),
        };
        vec![
            Section::Nav(NavSection {
                links: vec![link("Home", "/"), link("Pricing", "/pricing")],
                cta: Some(link("Order beans", "/order")),
            }),
            Section::Hero(HeroSection {
                heading: "Coffee roasted the morning it ships".to_owned(),
                subheading: Some("Small-batch roastery on the harbour".to_owned()),
                image: Some(image.clone()),
                primary_cta: Some(link("Shop roasts", "/shop")),
                secondary_cta: Some(link("Our story", "/about")),
            }),
            Section::Features(FeaturesSection {
                heading: Some("Why Nordwind".to_owned()),
                intro: Some("Three promises on every bag.".to_owned()),
                items: vec![FeatureItem {
                    title: "Roasted to order".to_owned(),
                    body: "Your batch goes in the drum after you order.".to_owned(),
                    icon: Some("flame".to_owned()),
                }],
            }),
            Section::TextImage(TextImageSection {
                heading: Some("The roastery".to_owned()),
                body: "A 1962 Probat drum, rebuilt by hand.".to_owned(),
                image: image.clone(),
                image_side: ImageSide::Left,
            }),
            Section::Gallery(GallerySection {
                heading: Some("Inside the roastery".to_owned()),
                images: vec![image.clone()],
            }),
            Section::Testimonials(TestimonialsSection {
                heading: Some("What cafés say".to_owned()),
                items: vec![Testimonial {
                    quote: "The freshest beans we've ever pulled shots with.".to_owned(),
                    author: "Mara Lindqvist".to_owned(),
                    role: Some("Head barista, Kaffebaren".to_owned()),
                }],
            }),
            Section::Pricing(PricingSection {
                heading: Some("Subscriptions".to_owned()),
                intro: Some("Pause or cancel any time.".to_owned()),
                tiers: vec![PricingTier {
                    name: "Weekly".to_owned(),
                    price: "€18/week".to_owned(),
                    period: Some("billed weekly".to_owned()),
                    description: Some("Two 250g bags every week.".to_owned()),
                    features: vec!["Free shipping".to_owned(), "Roast-day dispatch".to_owned()],
                    cta: Some(link("Start weekly", "/subscribe/weekly")),
                    highlighted: true,
                }],
            }),
            Section::Team(TeamSection {
                heading: Some("The roasters".to_owned()),
                members: vec![TeamMember {
                    name: "Jonas Meer".to_owned(),
                    role: Some("Founder & head roaster".to_owned()),
                    photo: Some(image.clone()),
                    bio: Some("Twenty years at the drum.".to_owned()),
                }],
            }),
            Section::Faq(FaqSection {
                heading: Some("Questions".to_owned()),
                items: vec![FaqItem {
                    question: "How fresh is the coffee?".to_owned(),
                    answer: "It ships the day it is roasted.".to_owned(),
                }],
            }),
            Section::Cta(CtaSection {
                heading: "Taste the difference".to_owned(),
                body: Some("First bag ships free.".to_owned()),
                button: link("Order now", "/order"),
            }),
            Section::ContactForm(ContactFormSection {
                heading: Some("Wholesale enquiries".to_owned()),
                body: Some("We answer within one business day.".to_owned()),
                form_id: Some("f4K9sL2wN7qR5tYx8vB1cA".to_owned()),
                success_message: Some("Thanks — talk soon.".to_owned()),
            }),
            Section::Collection(CollectionSection {
                collection_id: SiteCollectionId::new("seasonal-roasts"),
                heading: Some("Seasonal roasts".to_owned()),
            }),
            Section::Footer(FooterSection {
                text: Some("© Nordwind Coffee Roasters".to_owned()),
                links: vec![link("Imprint", "/imprint"), link("Privacy", "/privacy")],
            }),
        ]
    }

    fn envelope(sections: Vec<Section>) -> SectionsEnvelope {
        SectionsEnvelope {
            schema_version: SECTIONS_SCHEMA_VERSION,
            sections,
        }
    }

    #[test]
    fn every_variant_round_trips_fully_populated() {
        let before = envelope(full_sections());
        assert_eq!(before.sections.len(), 13, "corpus must cover all variants");
        before.validate().unwrap();
        let value = before.to_value().unwrap();
        let after = SectionsEnvelope::from_value(value).unwrap();
        assert_eq!(before, after);
    }

    /// The image-reference collector over the full corpus: exactly the four
    /// image-bearing variants (hero, text_image, gallery, team) declare
    /// their blobs; every other variant declares none. The public image
    /// path and the preview inliner both work from this set.
    #[test]
    fn image_blob_ids_cover_exactly_the_image_bearing_variants() {
        let sections = full_sections();
        let with_images: Vec<&'static str> = sections
            .iter()
            .filter(|section| !section.image_blob_ids().is_empty())
            .map(Section::kind)
            .collect();
        assert_eq!(with_images, ["hero", "text_image", "gallery", "team"]);
        for section in &sections {
            for blob in section.image_blob_ids() {
                assert_eq!(blob.as_str(), "9hK3vQ2mR8pT1xWz4bC5dg");
            }
        }
        // A gallery declares every image it shows, in order.
        let gallery = Section::Gallery(GallerySection {
            heading: None,
            images: vec![
                SiteImage {
                    blob_id: BlobId::new("first-blob"),
                    alt: String::new(),
                },
                SiteImage {
                    blob_id: BlobId::new("second-blob"),
                    alt: String::new(),
                },
            ],
        });
        let ids: Vec<&str> = gallery
            .image_blob_ids()
            .into_iter()
            .map(BlobId::as_str)
            .collect();
        assert_eq!(ids, ["first-blob", "second-blob"]);
    }

    #[test]
    fn minimal_variants_round_trip_with_options_absent() {
        let image = SiteImage {
            blob_id: BlobId::new("9hK3vQ2mR8pT1xWz4bC5dg"),
            alt: String::new(),
        };
        let before = envelope(vec![
            Section::Nav(NavSection {
                links: vec![],
                cta: None,
            }),
            Section::Hero(HeroSection {
                heading: "Hello".to_owned(),
                subheading: None,
                image: None,
                primary_cta: None,
                secondary_cta: None,
            }),
            Section::Gallery(GallerySection {
                heading: None,
                images: vec![image],
            }),
            Section::Footer(FooterSection {
                text: None,
                links: vec![],
            }),
        ]);
        before.validate().unwrap();
        let value = before.to_value().unwrap();
        // Absent options serialize as absent keys, not nulls — the stored
        // JSON stays minimal and re-parses to the same value.
        assert!(value["sections"][1].get("subheading").is_none());
        let after = SectionsEnvelope::from_value(value).unwrap();
        assert_eq!(before, after);
    }

    #[test]
    fn wire_tags_are_the_thirteen_snake_case_tokens() {
        let expected = [
            "nav",
            "hero",
            "features",
            "text_image",
            "gallery",
            "testimonials",
            "pricing",
            "team",
            "faq",
            "cta",
            "contact_form",
            "collection",
            "footer",
        ];
        for (section, expected) in full_sections().iter().zip(expected) {
            assert_eq!(section.kind(), expected);
            let value = serde_json::to_value(section).unwrap();
            assert_eq!(value["type"], Value::from(expected), "serde tag drifted");
        }
    }

    #[test]
    fn unknown_section_type_is_rejected() {
        let value = json!({
            "schema_version": 1,
            "sections": [{"type": "carousel", "images": []}]
        });
        assert!(matches!(
            SectionsEnvelope::from_value(value),
            Err(SectionSchemaError::Shape(_))
        ));
    }

    #[test]
    fn unknown_prop_is_rejected() {
        let value = json!({
            "schema_version": 1,
            "sections": [{"type": "hero", "heading": "Hi", "bogus": true}]
        });
        assert!(matches!(
            SectionsEnvelope::from_value(value),
            Err(SectionSchemaError::Shape(_))
        ));
    }

    #[test]
    fn unknown_envelope_prop_and_missing_version_are_rejected() {
        for value in [
            json!({"schema_version": 1, "sections": [], "extra": 1}),
            json!({"sections": []}),
        ] {
            assert!(matches!(
                SectionsEnvelope::from_value(value),
                Err(SectionSchemaError::Shape(_))
            ));
        }
    }

    #[test]
    fn future_schema_version_gets_the_version_error_not_a_shape_error() {
        // A v2 payload may contain sections this build can't parse — the
        // version must be reported, not a confusing shape failure.
        let value = json!({
            "schema_version": 2,
            "sections": [{"type": "carousel", "speed": "fast"}]
        });
        assert!(matches!(
            SectionsEnvelope::from_value(value),
            Err(SectionSchemaError::UnsupportedVersion(2))
        ));
    }

    #[test]
    fn href_allowlist_accepts_safe_targets_and_rejects_scriptable_ones() {
        for ok in [
            "/",
            "/about",
            "#contact",
            "https://example.org/menu",
            "http://example.org",
            "mailto:hello@example.org",
            "MAILTO:hello@example.org",
            "tel:+3212345678",
        ] {
            assert!(check_href("nav", ok).is_ok(), "expected accepted: {ok}");
        }
        for bad in [
            "",
            "javascript:alert(1)",
            "JavaScript:alert(1)",
            "data:text/html;base64,PGI+",
            "//evil.example",
            "ftp://example.org",
            "about:blank",
        ] {
            assert!(
                matches!(
                    check_href("nav", bad),
                    Err(SectionSchemaError::Invalid { .. })
                ),
                "expected rejected: {bad:?}"
            );
        }
    }

    #[test]
    fn content_rules_reject_blank_over_cap_and_empty_lists() {
        let blank_heading = envelope(vec![Section::Hero(HeroSection {
            heading: "   ".to_owned(),
            subheading: None,
            image: None,
            primary_cta: None,
            secondary_cta: None,
        })]);
        assert!(matches!(
            blank_heading.validate(),
            Err(SectionSchemaError::Invalid {
                section: "hero",
                ..
            })
        ));

        let over_cap = envelope(vec![Section::Cta(CtaSection {
            heading: "x".repeat(MAX_SHORT_TEXT_CHARS + 1),
            body: None,
            button: Link {
                label: "Go".to_owned(),
                href: "/go".to_owned(),
            },
        })]);
        assert!(matches!(
            over_cap.validate(),
            Err(SectionSchemaError::Invalid { section: "cta", .. })
        ));

        let empty_items = envelope(vec![Section::Faq(FaqSection {
            heading: None,
            items: vec![],
        })]);
        assert!(matches!(
            empty_items.validate(),
            Err(SectionSchemaError::Invalid { section: "faq", .. })
        ));

        let too_many = envelope(vec![
            Section::Cta(CtaSection {
                heading: "Go".to_owned(),
                body: None,
                button: Link {
                    label: "Go".to_owned(),
                    href: "/go".to_owned(),
                },
            });
            MAX_SECTIONS_PER_PAGE + 1
        ]);
        assert!(matches!(
            too_many.validate(),
            Err(SectionSchemaError::TooManySections)
        ));
    }

    #[test]
    fn token_and_icon_rules_hold() {
        let bad_blob = envelope(vec![Section::Gallery(GallerySection {
            heading: None,
            images: vec![SiteImage {
                blob_id: BlobId::new("not/a/token"),
                alt: String::new(),
            }],
        })]);
        assert!(matches!(
            bad_blob.validate(),
            Err(SectionSchemaError::Invalid {
                section: "gallery",
                ..
            })
        ));

        let bad_icon = envelope(vec![Section::Features(FeaturesSection {
            heading: None,
            intro: None,
            items: vec![FeatureItem {
                title: "T".to_owned(),
                body: "B".to_owned(),
                icon: Some("Flame!".to_owned()),
            }],
        })]);
        assert!(matches!(
            bad_icon.validate(),
            Err(SectionSchemaError::Invalid {
                section: "features",
                ..
            })
        ));
    }

    #[test]
    fn new_envelope_is_current_version_and_valid() {
        let envelope = SectionsEnvelope::new();
        assert_eq!(envelope.schema_version, SECTIONS_SCHEMA_VERSION);
        assert!(envelope.sections.is_empty());
        envelope.validate().unwrap();
    }
}
