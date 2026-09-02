//! The typed section schema for alo Sites pages (schema v1, ADR 0036).
//!
//! A page's `sections` JSON is the envelope [`SectionsEnvelope`] —
//! `{ "schema_version": 1, "sections": [ … ] }` — where every entry is one
//! variant of [`Section`], an internally-tagged serde enum with a closed
//! vocabulary of sixteen section types. The schema is **strict on write**:
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

use crate::id::{BlobId, SiteBookingId, SiteCatalogId, SiteCollectionId};
use crate::site_custom_code::CustomCodeSection;
use crate::site_layout::{ColumnSplit, GridColumns, ImageShape};

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

/// The whole width or height of a source image, in the basis points image
/// geometry is expressed in. Crop rectangles and focal points are stored as
/// ten-thousandths of the source dimension — never pixels (the same crop must
/// survive a re-upload at another resolution) and never floats (a stored
/// presentation value has to compare and round-trip exactly, the way
/// `vat_rate_bp` does).
pub const IMAGE_GEOMETRY_FULL_BP: u16 = 10_000;
/// The smallest crop this schema accepts on either axis, in basis points (1%
/// of the source). A crop below this is a degenerate rectangle no editor
/// produces, and it would ask the derivative pipeline to blow a handful of
/// pixels up to a full-width image.
pub const MIN_CROP_EXTENT_BP: u16 = 100;

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

/// A crop rectangle over the source image, in [basis
/// points](IMAGE_GEOMETRY_FULL_BP) of its width and height. The origin is the
/// top-left corner, so `{0, 0, 10000, 10000}` is the whole image.
///
/// The rectangle is *presentation*, not a destructive edit: the tenant's blob
/// keeps every pixel it was uploaded with, and re-framing a photo never loses
/// what was cropped away.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageCrop {
    /// Left edge, in basis points from the left of the source.
    pub x_bp: u16,
    /// Top edge, in basis points from the top of the source.
    pub y_bp: u16,
    /// Width, in basis points of the source width.
    pub width_bp: u16,
    /// Height, in basis points of the source height.
    pub height_bp: u16,
}

impl ImageCrop {
    /// The whole image — what an absent crop means.
    pub const fn full() -> Self {
        ImageCrop {
            x_bp: 0,
            y_bp: 0,
            width_bp: IMAGE_GEOMETRY_FULL_BP,
            height_bp: IMAGE_GEOMETRY_FULL_BP,
        }
    }

    /// Right edge in basis points (`x + width`), widened so a rectangle that
    /// overflows the source is caught rather than wrapping.
    const fn right_bp(self) -> u32 {
        self.x_bp as u32 + self.width_bp as u32
    }

    /// Bottom edge in basis points (`y + height`).
    const fn bottom_bp(self) -> u32 {
        self.y_bp as u32 + self.height_bp as u32
    }

    /// The rectangle's midpoint — what an absent focal point means. Saturating
    /// because this answers for any parsed value, including one that has not
    /// reached [`SectionsEnvelope::validate`] yet.
    pub const fn center(self) -> ImageFocalPoint {
        ImageFocalPoint {
            x_bp: self.x_bp.saturating_add(self.width_bp / 2),
            y_bp: self.y_bp.saturating_add(self.height_bp / 2),
        }
    }

    /// Whether a focal point lies on or inside this rectangle.
    pub const fn contains(self, focal: ImageFocalPoint) -> bool {
        focal.x_bp >= self.x_bp
            && (focal.x_bp as u32) <= self.right_bp()
            && focal.y_bp >= self.y_bp
            && (focal.y_bp as u32) <= self.bottom_bp()
    }
}

/// The point of an image that must stay visible when a layout has to crop it
/// further — a face, a product — in [basis points](IMAGE_GEOMETRY_FULL_BP) of
/// the source width and height, top-left origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageFocalPoint {
    /// Horizontal position, in basis points from the left of the source.
    pub x_bp: u16,
    /// Vertical position, in basis points from the top of the source.
    pub y_bp: u16,
}

/// An image reference: a tenant blob, its alt text, and how it is presented.
///
/// `alt` is a required prop — the renderer emits `alt` on every `<img>` — but
/// an empty `alt` alone does not say *why* it is empty. `decorative` is that
/// missing half: set, it means "this image carries no information, and a
/// screen reader should skip it"; unset with a blank `alt`, it means the alt
/// text has simply not been written yet, which is what
/// [`needs_alt_text`](Self::needs_alt_text) reports and what the editor asks
/// the owner (or an AI proposal) to fill in.
///
/// `crop` and `focal` are both optional and both additive: an image stored
/// before this schema gained them parses unchanged and means "the whole image,
/// centred" ([`crop_or_full`](Self::crop_or_full),
/// [`focal_or_center`](Self::focal_or_center)).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SiteImage {
    /// The tenant blob holding the image bytes.
    pub blob_id: BlobId,
    /// Alt text; blank means either decorative (with `decorative` set) or
    /// not yet written.
    pub alt: String,
    /// The visible rectangle of the source; absent means the whole image.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crop: Option<ImageCrop>,
    /// The point to keep in frame when a layout crops further; absent means
    /// the centre of the visible rectangle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focal: Option<ImageFocalPoint>,
    /// Marks a blank `alt` as deliberate: the image is presentational and
    /// assistive technology should skip it.
    #[serde(default, skip_serializing_if = "is_not_set")]
    pub decorative: bool,
    /// The frame the image is shown in — one of the shapes its section
    /// declares ([`crate::site_layout`]); absent means the picture's own
    /// proportions, which is how every image rendered before this schema
    /// gained the property.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape: Option<ImageShape>,
}

/// `skip_serializing_if` for a defaulted flag: absent stays absent, so stored
/// section JSON gains no key for an image nobody marked decorative.
fn is_not_set(flag: &bool) -> bool {
    !*flag
}

impl SiteImage {
    /// An image with no presentation overrides — the value every writer
    /// starts from.
    pub fn new(blob_id: BlobId, alt: impl Into<String>) -> Self {
        SiteImage {
            blob_id,
            alt: alt.into(),
            crop: None,
            focal: None,
            decorative: false,
            shape: None,
        }
    }

    /// The visible rectangle: the stored crop, or the whole image.
    pub fn crop_or_full(&self) -> ImageCrop {
        self.crop.unwrap_or_else(ImageCrop::full)
    }

    /// The point to keep in frame: the stored focal point, or the centre of
    /// the visible rectangle.
    pub fn focal_or_center(&self) -> ImageFocalPoint {
        self.focal.unwrap_or_else(|| self.crop_or_full().center())
    }

    /// Whether this image is still missing the alt text it needs — blank alt
    /// on an image nobody marked decorative. The editor drives its
    /// write-the-alt-text prompt off this, so "not written yet" and "nothing
    /// to say" never look the same.
    pub fn needs_alt_text(&self) -> bool {
        !self.decorative && self.alt.trim().is_empty()
    }
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

/// A reusable site-theme colour role. Sections store intent, not raw colour
/// values, so brand changes propagate consistently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeColorRole {
    Background,
    Text,
    Border,
    #[serde(rename = "accent_1")]
    Accent1,
    #[serde(rename = "accent_2")]
    Accent2,
    #[serde(rename = "accent_3")]
    Accent3,
    #[serde(rename = "accent_4")]
    Accent4,
    #[serde(rename = "accent_5")]
    Accent5,
}

/// Shared, responsive presentation choices available to every content block.
/// They store design intent and named theme roles—never raw CSS—so the same
/// section remains branded, accessible and portable across templates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionLayoutStyle {
    Clean,
    Cards,
    Minimal,
    Editorial,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionSpacing {
    Compact,
    Standard,
    Generous,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionWidth {
    Narrow,
    Balanced,
    Wide,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionAlignment {
    Left,
    Center,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionEntrance {
    None,
    FadeUp,
    SlideIn,
    ScaleIn,
    Reveal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SectionPresentation {
    pub layout: SectionLayoutStyle,
    pub spacing: SectionSpacing,
    pub width: SectionWidth,
    pub alignment: SectionAlignment,
    pub background: ThemeColorRole,
    pub text: ThemeColorRole,
    pub button: ThemeColorRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub button_text: Option<ThemeColorRole>,
    pub button_hover: ThemeColorRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub button_hover_text: Option<ThemeColorRole>,
    pub entrance: SectionEntrance,
    pub speed: TransitionSpeed,
}

/// Theme roles used by the navigation surface and its link states.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NavAppearance {
    pub background: ThemeColorRole,
    pub text: ThemeColorRole,
    pub hover: ThemeColorRole,
}

/// Theme roles scoped to one hero. Button foregrounds are derived by the
/// renderer, so authors choose brand intent without creating unreadable text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeroAppearance {
    pub background: ThemeColorRole,
    pub primary_button: ThemeColorRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_button_text: Option<ThemeColorRole>,
    pub primary_button_hover: ThemeColorRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_button_hover_text: Option<ThemeColorRole>,
    pub secondary_button: ThemeColorRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secondary_button_text: Option<ThemeColorRole>,
    pub secondary_button_hover: ThemeColorRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secondary_button_hover_text: Option<ThemeColorRole>,
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
    /// Optional scoped palette; absent keeps every colour from the site theme.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub appearance: Option<NavAppearance>,
}

/// The hero's visual composition. These are deliberately named layouts, not
/// free coordinates, so every choice remains responsive and accessible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeroLayout {
    Centered,
    SplitRight,
    SplitLeft,
    Background,
    VideoBackground,
    Editorial,
}

impl HeroLayout {
    pub const fn class(self) -> &'static str {
        match self {
            Self::Centered => "hero-centered",
            Self::SplitRight => "hero-split-right",
            Self::SplitLeft => "hero-split-left",
            Self::Background => "hero-background",
            Self::VideoBackground => "hero-video-background",
            Self::Editorial => "hero-editorial",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeroHeight {
    Compact,
    Standard,
    Tall,
}

impl HeroHeight {
    pub const fn class(self) -> &'static str {
        match self {
            Self::Compact => "hero-height-compact",
            Self::Standard => "hero-height-standard",
            Self::Tall => "hero-height-tall",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeroAlignment {
    Left,
    Center,
    Right,
}

impl HeroAlignment {
    pub const fn class(self) -> &'static str {
        match self {
            Self::Left => "hero-align-left",
            Self::Center => "hero-align-center",
            Self::Right => "hero-align-right",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeroContentWidth {
    Narrow,
    Balanced,
    Wide,
}

impl HeroContentWidth {
    pub const fn class(self) -> &'static str {
        match self {
            Self::Narrow => "hero-width-narrow",
            Self::Balanced => "hero-width-balanced",
            Self::Wide => "hero-width-wide",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeroTextAnimation {
    None,
    FadeUp,
    WordReveal,
    SlideIn,
}

impl HeroTextAnimation {
    pub const fn class(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::FadeUp => Some("hero-text-fade-up"),
            Self::WordReveal => Some("hero-text-word-reveal"),
            Self::SlideIn => Some("hero-text-slide-in"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeroMediaAnimation {
    None,
    FadeIn,
    SlideUp,
    SlowZoom,
}

impl HeroMediaAnimation {
    pub const fn class(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::FadeIn => Some("hero-media-fade-in"),
            Self::SlideUp => Some("hero-media-slide-up"),
            Self::SlowZoom => Some("hero-media-slow-zoom"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeroAnimationSpeed {
    Quick,
    Smooth,
    Relaxed,
}

impl HeroAnimationSpeed {
    pub const fn class(self) -> &'static str {
        match self {
            Self::Quick => "hero-motion-quick",
            Self::Smooth => "hero-motion-smooth",
            Self::Relaxed => "hero-motion-relaxed",
        }
    }
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
    /// Optional direct HTTPS MP4/WebM source. The image remains its poster and
    /// reduced-motion fallback.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub video_url: Option<String>,
    /// Primary call-to-action button.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_cta: Option<Link>,
    /// Secondary, quieter call-to-action.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secondary_cta: Option<Link>,
    /// Optional scoped palette; absent preserves the site's default hero.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub appearance: Option<HeroAppearance>,
    /// Named responsive composition; absent preserves the original hero.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<HeroLayout>,
    /// Vertical breathing room.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<HeroHeight>,
    /// Copy and action alignment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alignment: Option<HeroAlignment>,
    /// Maximum text measure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_width: Option<HeroContentWidth>,
    /// Entrance preset for the headline, supporting text and actions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_animation: Option<HeroTextAnimation>,
    /// Entrance preset for the image or background video.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_animation: Option<HeroMediaAnimation>,
    /// Shared pace keeps independently animated elements feeling coordinated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub animation_speed: Option<HeroAnimationSpeed>,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeaturesLayout {
    Grid,
    Bento,
    List,
    Steps,
    Spotlight,
}

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
    /// Cards per row on a wide screen ([`crate::site_layout`]); absent renders
    /// the fluid grid this section has always used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub columns: Option<GridColumns>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<FeaturesLayout>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation: Option<SectionPresentation>,
}

/// A text block alongside an image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextImageLayout {
    Split,
    Overlap,
    Framed,
    Editorial,
    FullBleed,
}

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
    /// How the row is divided between image and text where the two sit side
    /// by side ([`crate::site_layout`]); absent means equal columns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub split: Option<ColumnSplit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<TextImageLayout>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation: Option<SectionPresentation>,
}

/// An image gallery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GalleryLayout {
    Grid,
    Masonry,
    Collage,
    Filmstrip,
    Spotlight,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GallerySection {
    /// Section heading.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heading: Option<String>,
    /// The images; at least one.
    pub images: Vec<SiteImage>,
    /// Images per row on a wide screen ([`crate::site_layout`]); absent
    /// renders the fluid grid this section has always used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub columns: Option<GridColumns>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<GalleryLayout>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation: Option<SectionPresentation>,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestimonialsLayout {
    Cards,
    Featured,
    Editorial,
    Stacked,
    Carousel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestimonialsSection {
    /// Section heading.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heading: Option<String>,
    /// The quotes; at least one.
    pub items: Vec<Testimonial>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<TestimonialsLayout>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation: Option<SectionPresentation>,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PricingLayout {
    Cards,
    Comparison,
    Featured,
    Compact,
    Editorial,
}

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<PricingLayout>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation: Option<SectionPresentation>,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TeamLayout {
    Portraits,
    Cards,
    Roster,
    Spotlight,
    Compact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeamSection {
    /// Section heading.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heading: Option<String>,
    /// The members; at least one.
    pub members: Vec<TeamMember>,
    /// People per row on a wide screen ([`crate::site_layout`]); absent
    /// renders the fluid grid this section has always used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub columns: Option<GridColumns>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<TeamLayout>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation: Option<SectionPresentation>,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FaqLayout {
    Accordion,
    Divided,
    Cards,
    TwoColumn,
    Editorial,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FaqSection {
    /// Section heading.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heading: Option<String>,
    /// The Q/A pairs; at least one.
    pub items: Vec<FaqItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<FaqLayout>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation: Option<SectionPresentation>,
}

/// A standalone call-to-action banner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CtaLayout {
    Centered,
    Split,
    Banner,
    Card,
    TwoActions,
}

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secondary_button: Option<Link>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<CtaLayout>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation: Option<SectionPresentation>,
}

/// A contact form. The form itself (fields, submissions) is a separate store
/// object; this section points at it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContactFormLayout {
    Simple,
    Split,
    Card,
    Panel,
    Minimal,
}

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<ContactFormLayout>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation: Option<SectionPresentation>,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollectionLayout {
    Grid,
    Masonry,
    List,
    Editorial,
    Carousel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollectionSection {
    pub collection_id: SiteCollectionId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heading: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<CollectionLayout>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation: Option<SectionPresentation>,
}

/// A catalog of what the site offers — dishes, rooms, services, courses —
/// frozen from the tenant's own [catalog](crate::site_catalog) at publish
/// time. The section stores only the stable catalog id, an optional heading,
/// and an optional category handle to show one grouping instead of all of
/// them; prices, names and availability live in the frozen snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogLayout {
    Grid,
    Menu,
    List,
    Featured,
    Compact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogSection {
    pub catalog_id: SiteCatalogId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heading: Option<String>,
    /// Handle of the single category to show; absent shows every category.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<CatalogLayout>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation: Option<SectionPresentation>,
}

/// Something a visitor may book, frozen from the tenant's own
/// [booking service](crate::site_bookings) at publish time. The section stores
/// only the stable service id and an optional heading; what it is called, how
/// long it takes, when it is offered and what is asked when booking all live in
/// the frozen snapshot, and the free times are read live against the bound
/// Agenda calendar at the moment the visitor looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BookingLayout {
    Card,
    Split,
    Centered,
    Panel,
    Compact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BookingSection {
    pub booking_id: SiteBookingId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heading: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<BookingLayout>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation: Option<SectionPresentation>,
}

/// The door to the site's ticket shop (ADR 0041, item S3.04f). The section
/// stores presentation only — an optional heading and an optional line of the
/// owner's own words. What is on sale, its price and what is left are live
/// state read from the Billing seam on `/tix`, one navigation away, exactly
/// as a booking section defers its free times: a published page is cached
/// bytes, and a price or a seat count must never be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TicketsLayout {
    Card,
    Centered,
    Split,
    Banner,
    Compact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TicketsSection {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heading: Option<String>,
    /// The owner's own sentence above the link (what the events are, why to
    /// come).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<TicketsLayout>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation: Option<SectionPresentation>,
}

/// The door to the site's stock shop (ADR 0041, item S3.05a3) — the
/// [`TicketsSection`] trade made again for goods on a shelf: the section
/// stores presentation only, because what is for sale, its price and what is
/// left are the owning seams' live answers served on `/shop`, one navigation
/// away. A published page is cached bytes, and a price or a shelf count must
/// never be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShopLayout {
    Storefront,
    Centered,
    Split,
    Banner,
    Compact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShopSection {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heading: Option<String>,
    /// The owner's own sentence above the link (what the shop sells, why to
    /// buy here).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<ShopLayout>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation: Option<SectionPresentation>,
}

/// A motion boundary between two content sections. It owns no visible copy:
/// the renderer applies its preset to the next section as it enters and,
/// optionally, leaves the viewport. Reduced-motion visitors always get the
/// still document order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransitionEffect {
    Fade,
    Slide,
    Scale,
    Reveal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransitionDirection {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransitionSpeed {
    Quick,
    Smooth,
    Relaxed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransitionTrigger {
    Early,
    Balanced,
    Late,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransitionSection {
    pub effect: TransitionEffect,
    pub direction: TransitionDirection,
    pub speed: TransitionSpeed,
    pub trigger: TransitionTrigger,
    /// When true the next section reverses as it leaves the viewport.
    #[serde(default)]
    pub animate_out: bool,
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
    /// What the site offers, frozen from the tenant's own catalog at publish
    /// time.
    Catalog(CatalogSection),
    /// Something a visitor may book, against the service's Agenda calendar.
    Booking(BookingSection),
    /// The door to the site's live ticket shop (`/tix`).
    Tickets(TicketsSection),
    /// The door to the site's live stock shop (`/shop`).
    Shop(ShopSection),
    /// Scroll motion applied to the next content section.
    Transition(TransitionSection),
    /// The tenant's own HTML/CSS/JS, published inside a sandboxed frame.
    CustomCode(CustomCodeSection),
    /// Page footer.
    Footer(FooterSection),
}

/// Every section type this build speaks, in the order the editor offers them:
/// the page's frame first, then the blocks that carry content, then its foot.
///
/// This is the list a palette iterates and the list [`Section::kind`] answers
/// out of; the schema-fixture test asserts the two agree, so a variant added
/// without a fixture — or a fixture without a variant — fails the gate rather
/// than becoming a section type nothing offers.
pub const SECTION_KINDS: &[&str] = &[
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
    "catalog",
    "booking",
    "tickets",
    "shop",
    "transition",
    "custom_code",
    "footer",
];

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
            Section::Catalog(_) => "catalog",
            Section::Booking(_) => "booking",
            Section::Tickets(_) => "tickets",
            Section::Shop(_) => "shop",
            Section::Transition(_) => "transition",
            Section::CustomCode(_) => "custom_code",
            Section::Footer(_) => "footer",
        }
    }

    /// This section's images, in document order — blob reference, alt text
    /// and presentation together. This is the set renderers, the public image
    /// path and the derivative pipeline work from: everything a page can show
    /// is exactly what this returns (plus the theme's logo/favicon). The match
    /// is deliberately exhaustive — a new section variant fails to compile
    /// until it declares its images.
    pub fn images(&self) -> Vec<&SiteImage> {
        match self {
            Section::Hero(s) => s.image.iter().collect(),
            Section::TextImage(s) => vec![&s.image],
            Section::Gallery(s) => s.images.iter().collect(),
            Section::Team(s) => s.members.iter().filter_map(|m| m.photo.as_ref()).collect(),
            Section::Nav(_)
            | Section::Features(_)
            | Section::Testimonials(_)
            | Section::Pricing(_)
            | Section::Faq(_)
            | Section::Cta(_)
            | Section::ContactForm(_)
            | Section::Collection(_)
            | Section::Catalog(_)
            | Section::Booking(_)
            | Section::Tickets(_)
            | Section::Shop(_)
            | Section::Transition(_)
            // A custom-code block owns no tenant blob: it has no network, so
            // the only image it can show is one carried inline in its markup.
            | Section::CustomCode(_)
            | Section::Footer(_) => Vec::new(),
        }
    }

    /// The tenant blobs this section's images reference, in document order —
    /// [`images`](Self::images) reduced to the reference set the public image
    /// path authorizes against.
    pub fn image_blob_ids(&self) -> Vec<&BlobId> {
        self.images().into_iter().map(|i| &i.blob_id).collect()
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
                check_opt_link(kind, s.cta.as_ref())?;
                Ok(())
            }
            Section::Hero(s) => {
                check_short(kind, "heading", &s.heading)?;
                check_opt_short(kind, "subheading", s.subheading.as_deref())?;
                check_opt_image(kind, s.image.as_ref())?;
                check_opt_video_url(kind, s.video_url.as_deref())?;
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
            Section::Catalog(s) => {
                check_token(kind, "catalog_id", s.catalog_id.as_str())?;
                check_opt_short(kind, "heading", s.heading.as_deref())?;
                if let Some(category) = &s.category {
                    check_token(kind, "category", category)?;
                }
                Ok(())
            }
            Section::Booking(s) => {
                check_token(kind, "booking_id", s.booking_id.as_str())?;
                check_opt_short(kind, "heading", s.heading.as_deref())
            }
            Section::Tickets(s) => {
                check_opt_short(kind, "heading", s.heading.as_deref())?;
                check_opt_long(kind, "body", s.body.as_deref())
            }
            Section::Shop(s) => {
                check_opt_short(kind, "heading", s.heading.as_deref())?;
                check_opt_long(kind, "body", s.body.as_deref())
            }
            Section::Transition(_) => Ok(()),
            // The block's own rules — byte caps, the capability/script pairing,
            // and everything that would break the frame's document — live with
            // the sandbox contract they belong to.
            Section::CustomCode(s) => s.validate(),
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
pub(crate) fn check_short(
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

pub(crate) fn check_opt_short(
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

/// Background video is deliberately narrower than a general link: published
/// pages are HTTPS, so accepting an insecure source would create mixed-content
/// failures. Control characters and whitespace are rejected before the value
/// ever reaches a `src` attribute.
fn check_opt_video_url(
    section: &'static str,
    video_url: Option<&str>,
) -> Result<(), SectionSchemaError> {
    let Some(video_url) = video_url else {
        return Ok(());
    };
    if video_url.chars().count() > MAX_HREF_CHARS {
        return Err(invalid(
            section,
            format!("video URL must be at most {MAX_HREF_CHARS} characters"),
        ));
    }
    if !video_url.to_ascii_lowercase().starts_with("https://")
        || video_url.len() <= "https://".len()
        || video_url.chars().any(char::is_whitespace)
        || video_url.chars().any(char::is_control)
    {
        return Err(invalid(
            section,
            "video URL must be a direct HTTPS address without spaces".to_owned(),
        ));
    }
    Ok(())
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
    // Alt may be empty (decorative, or not yet written) but stays bounded.
    if image.alt.chars().count() > MAX_SHORT_TEXT_CHARS {
        return Err(invalid(
            section,
            format!("image alt must be at most {MAX_SHORT_TEXT_CHARS} characters"),
        ));
    }
    if image.decorative && !image.alt.trim().is_empty() {
        return Err(invalid(
            section,
            "a decorative image must have empty alt text".to_owned(),
        ));
    }
    check_image_geometry(section, image)
}

/// Crop and focal-point rules. Both are optional; when present they must
/// describe a rectangle that exists inside the source and a point that exists
/// inside that rectangle, so the two can never contradict each other.
fn check_image_geometry(
    section: &'static str,
    image: &SiteImage,
) -> Result<(), SectionSchemaError> {
    if let Some(crop) = image.crop {
        if crop.width_bp < MIN_CROP_EXTENT_BP || crop.height_bp < MIN_CROP_EXTENT_BP {
            return Err(invalid(
                section,
                format!(
                    "image crop width and height must each be at least {MIN_CROP_EXTENT_BP} basis points of the image"
                ),
            ));
        }
        if crop.right_bp() > u32::from(IMAGE_GEOMETRY_FULL_BP)
            || crop.bottom_bp() > u32::from(IMAGE_GEOMETRY_FULL_BP)
        {
            return Err(invalid(
                section,
                format!(
                    "image crop must stay inside the image (x + width and y + height may not exceed {IMAGE_GEOMETRY_FULL_BP} basis points)"
                ),
            ));
        }
    }
    if let Some(focal) = image.focal {
        if focal.x_bp > IMAGE_GEOMETRY_FULL_BP || focal.y_bp > IMAGE_GEOMETRY_FULL_BP {
            return Err(invalid(
                section,
                format!(
                    "image focal point must be within {IMAGE_GEOMETRY_FULL_BP} basis points on each axis"
                ),
            ));
        }
        if let Some(crop) = image.crop
            && !crop.contains(focal)
        {
            return Err(invalid(
                section,
                "image focal point must lie inside the crop".to_owned(),
            ));
        }
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
        let image = SiteImage::new(
            BlobId::new("9hK3vQ2mR8pT1xWz4bC5dg"),
            "Roasting drum mid-batch",
        );
        let link = |label: &str, href: &str| Link {
            label: label.to_owned(),
            href: href.to_owned(),
        };
        vec![
            Section::Nav(NavSection {
                links: vec![link("Home", "/"), link("Pricing", "/pricing")],
                cta: Some(link("Order beans", "/order")),
                appearance: None,
            }),
            Section::Hero(HeroSection {
                heading: "Coffee roasted the morning it ships".to_owned(),
                subheading: Some("Small-batch roastery on the harbour".to_owned()),
                image: Some(image.clone()),
                video_url: Some("https://media.example/roastery.webm".to_owned()),
                primary_cta: Some(link("Shop roasts", "/shop")),
                secondary_cta: Some(link("Our story", "/about")),
                appearance: Some(HeroAppearance {
                    background: ThemeColorRole::Accent3,
                    primary_button: ThemeColorRole::Accent1,
                    primary_button_text: Some(ThemeColorRole::Background),
                    primary_button_hover: ThemeColorRole::Accent2,
                    primary_button_hover_text: Some(ThemeColorRole::Text),
                    secondary_button: ThemeColorRole::Accent4,
                    secondary_button_text: Some(ThemeColorRole::Accent4),
                    secondary_button_hover: ThemeColorRole::Accent5,
                    secondary_button_hover_text: None,
                }),
                layout: Some(HeroLayout::SplitRight),
                height: Some(HeroHeight::Tall),
                alignment: Some(HeroAlignment::Left),
                content_width: Some(HeroContentWidth::Narrow),
                text_animation: Some(HeroTextAnimation::WordReveal),
                media_animation: Some(HeroMediaAnimation::SlowZoom),
                animation_speed: Some(HeroAnimationSpeed::Relaxed),
            }),
            Section::Features(FeaturesSection {
                heading: Some("Why Nordwind".to_owned()),
                intro: Some("Three promises on every bag.".to_owned()),
                items: vec![FeatureItem {
                    title: "Roasted to order".to_owned(),
                    body: "Your batch goes in the drum after you order.".to_owned(),
                    icon: Some("flame".to_owned()),
                }],
                columns: None,
                layout: None,
                presentation: None,
            }),
            Section::TextImage(TextImageSection {
                heading: Some("The roastery".to_owned()),
                body: "A 1962 Probat drum, rebuilt by hand.".to_owned(),
                image: image.clone(),
                image_side: ImageSide::Left,
                split: None,
                layout: None,
                presentation: None,
            }),
            Section::Gallery(GallerySection {
                heading: Some("Inside the roastery".to_owned()),
                images: vec![image.clone()],
                columns: None,
                layout: None,
                presentation: None,
            }),
            Section::Testimonials(TestimonialsSection {
                heading: Some("What cafés say".to_owned()),
                items: vec![Testimonial {
                    quote: "The freshest beans we've ever pulled shots with.".to_owned(),
                    author: "Mara Lindqvist".to_owned(),
                    role: Some("Head barista, Kaffebaren".to_owned()),
                }],
                layout: None,
                presentation: None,
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
                layout: None,
                presentation: None,
            }),
            Section::Team(TeamSection {
                heading: Some("The roasters".to_owned()),
                members: vec![TeamMember {
                    name: "Jonas Meer".to_owned(),
                    role: Some("Founder & head roaster".to_owned()),
                    photo: Some(image.clone()),
                    bio: Some("Twenty years at the drum.".to_owned()),
                }],
                columns: None,
                layout: None,
                presentation: None,
            }),
            Section::Faq(FaqSection {
                heading: Some("Questions".to_owned()),
                items: vec![FaqItem {
                    question: "How fresh is the coffee?".to_owned(),
                    answer: "It ships the day it is roasted.".to_owned(),
                }],
                layout: None,
                presentation: None,
            }),
            Section::Cta(CtaSection {
                heading: "Taste the difference".to_owned(),
                body: Some("First bag ships free.".to_owned()),
                button: link("Order now", "/order"),
                secondary_button: None,
                layout: None,
                presentation: None,
            }),
            Section::ContactForm(ContactFormSection {
                heading: Some("Wholesale enquiries".to_owned()),
                body: Some("We answer within one business day.".to_owned()),
                form_id: Some("f4K9sL2wN7qR5tYx8vB1cA".to_owned()),
                success_message: Some("Thanks — talk soon.".to_owned()),
                layout: None,
                presentation: None,
            }),
            Section::Collection(CollectionSection {
                collection_id: SiteCollectionId::new("seasonal-roasts"),
                heading: Some("Seasonal roasts".to_owned()),
                layout: None,
                presentation: None,
            }),
            Section::Catalog(CatalogSection {
                catalog_id: SiteCatalogId::new("house-menu"),
                heading: Some("On the counter".to_owned()),
                category: Some("espresso".to_owned()),
                layout: None,
                presentation: None,
            }),
            Section::Booking(BookingSection {
                booking_id: SiteBookingId::new("tasting-table"),
                heading: Some("Book the tasting table".to_owned()),
                layout: None,
                presentation: None,
            }),
            Section::Tickets(TicketsSection {
                heading: Some("Cupping evenings".to_owned()),
                body: Some("Six seats around the roaster, once a month.".to_owned()),
                layout: None,
                presentation: None,
            }),
            Section::Shop(ShopSection {
                heading: Some("The roastery shop".to_owned()),
                body: Some("Beans and brew gear, shipped from the roastery.".to_owned()),
                layout: None,
                presentation: None,
            }),
            Section::CustomCode(CustomCodeSection {
                heading: Some("Roast timer".to_owned()),
                title: "A timer counting down the current roast".to_owned(),
                html: "<p id=\"left\">12:00</p>".to_owned(),
                css: Some("#left { font-size: 3rem; }".to_owned()),
                js: Some("document.getElementById('left');".to_owned()),
                capabilities: crate::site_custom_code::CustomCodeCapabilities {
                    scripts: true,
                    inline_images: true,
                },
                height_px: 220,
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
        assert_eq!(before.sections.len(), 18, "corpus must cover all variants");
        before.validate().unwrap();
        let value = before.to_value().unwrap();
        let after = SectionsEnvelope::from_value(value).unwrap();
        assert_eq!(before, after);
    }

    #[test]
    fn navigation_appearance_accepts_only_named_theme_roles() {
        let valid = SectionsEnvelope::from_value(json!({
            "schema_version": SECTIONS_SCHEMA_VERSION,
            "sections": [{
                "type": "nav",
                "links": [],
                "appearance": {
                    "background": "background",
                    "text": "text",
                    "hover": "accent_1"
                }
            }]
        }));
        assert!(valid.is_ok());

        let error = SectionsEnvelope::from_value(json!({
            "schema_version": SECTIONS_SCHEMA_VERSION,
            "sections": [{"type": "nav", "links": [], "appearance": {
                "background": "#ffffff", "text": "text", "hover": "accent_1"
            }}]
        }))
        .unwrap_err();
        assert!(matches!(error, SectionSchemaError::Shape(_)));
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
                SiteImage::new(BlobId::new("first-blob"), ""),
                SiteImage::new(BlobId::new("second-blob"), ""),
            ],
            columns: None,
            layout: None,
            presentation: None,
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
        let image = SiteImage::new(BlobId::new("9hK3vQ2mR8pT1xWz4bC5dg"), "");
        let before = envelope(vec![
            Section::Nav(NavSection {
                links: vec![],
                cta: None,
                appearance: None,
            }),
            Section::Hero(HeroSection {
                heading: "Hello".to_owned(),
                subheading: None,
                image: None,
                video_url: None,
                primary_cta: None,
                secondary_cta: None,
                appearance: None,
                layout: None,
                height: None,
                alignment: None,
                content_width: None,
                text_animation: None,
                media_animation: None,
                animation_speed: None,
            }),
            Section::Gallery(GallerySection {
                heading: None,
                images: vec![image],
                columns: None,
                layout: None,
                presentation: None,
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
    fn wire_tags_are_the_published_snake_case_tokens() {
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
            "catalog",
            "booking",
            "tickets",
            "shop",
            "custom_code",
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
    fn hero_video_accepts_direct_https_and_rejects_unsafe_sources() {
        for accepted in [
            "https://media.example/hero.mp4",
            "HTTPS://cdn.example/hero.webm?version=2",
        ] {
            assert!(check_opt_video_url("hero", Some(accepted)).is_ok());
        }
        for rejected in [
            "",
            "http://media.example/hero.mp4",
            "//media.example/hero.mp4",
            "javascript:alert(1)",
            "https://media.example/hero video.mp4",
        ] {
            assert!(matches!(
                check_opt_video_url("hero", Some(rejected)),
                Err(SectionSchemaError::Invalid {
                    section: "hero",
                    ..
                })
            ));
        }
    }

    #[test]
    fn content_rules_reject_blank_over_cap_and_empty_lists() {
        let blank_heading = envelope(vec![Section::Hero(HeroSection {
            heading: "   ".to_owned(),
            subheading: None,
            image: None,
            video_url: None,
            primary_cta: None,
            secondary_cta: None,
            appearance: None,
            layout: None,
            height: None,
            alignment: None,
            content_width: None,
            text_animation: None,
            media_animation: None,
            animation_speed: None,
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
            secondary_button: None,
            layout: None,
            presentation: None,
        })]);
        assert!(matches!(
            over_cap.validate(),
            Err(SectionSchemaError::Invalid { section: "cta", .. })
        ));

        let empty_items = envelope(vec![Section::Faq(FaqSection {
            heading: None,
            items: vec![],
            layout: None,
            presentation: None,
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
                secondary_button: None,
                layout: None,
                presentation: None,
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
            images: vec![SiteImage::new(BlobId::new("not/a/token"), "")],
            columns: None,
            layout: None,
            presentation: None,
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
            columns: None,
            layout: None,
            presentation: None,
        })]);
        assert!(matches!(
            bad_icon.validate(),
            Err(SectionSchemaError::Invalid {
                section: "features",
                ..
            })
        ));
    }

    /// A gallery of one image, for exercising image rules through the same
    /// gate everything else goes through.
    fn gallery_of(image: SiteImage) -> SectionsEnvelope {
        envelope(vec![Section::Gallery(GallerySection {
            heading: None,
            images: vec![image],
            columns: None,
            layout: None,
            presentation: None,
        })])
    }

    fn framed(crop: Option<ImageCrop>, focal: Option<ImageFocalPoint>) -> SiteImage {
        SiteImage {
            crop,
            focal,
            ..SiteImage::new(BlobId::new("9hK3vQ2mR8pT1xWz4bC5dg"), "A drum roaster")
        }
    }

    #[test]
    fn absent_presentation_props_parse_and_stay_absent_on_the_wire() {
        // The shape stored before crop/focal/decorative existed: it parses,
        // it means whole-image-centred, and re-serializing it adds no keys —
        // an old snapshot is never rewritten just by being read.
        let legacy = json!({
            "schema_version": 1,
            "sections": [{
                "type": "gallery",
                "images": [{"blob_id": "9hK3vQ2mR8pT1xWz4bC5dg", "alt": "A drum roaster"}]
            }]
        });
        let parsed = SectionsEnvelope::from_value(legacy.clone()).unwrap();
        let image = parsed.sections[0].images()[0];
        assert_eq!(image.crop, None);
        assert_eq!(image.focal, None);
        assert!(!image.decorative);
        assert_eq!(image.crop_or_full(), ImageCrop::full());
        assert_eq!(
            image.focal_or_center(),
            ImageFocalPoint {
                x_bp: 5_000,
                y_bp: 5_000
            }
        );
        assert_eq!(parsed.to_value().unwrap(), legacy);
    }

    #[test]
    fn a_crop_defaults_the_focal_point_to_its_own_centre() {
        let image = framed(
            Some(ImageCrop {
                x_bp: 2_000,
                y_bp: 1_000,
                width_bp: 4_000,
                height_bp: 2_000,
            }),
            None,
        );
        assert_eq!(
            image.focal_or_center(),
            ImageFocalPoint {
                x_bp: 4_000,
                y_bp: 2_000
            },
            "the centre of the crop, not the centre of the source"
        );
    }

    #[test]
    fn crop_rules_reject_rectangles_that_leave_or_vanish_inside_the_image() {
        let whole = ImageCrop::full();
        gallery_of(framed(Some(whole), None)).validate().unwrap();
        // Flush against the right and bottom edges is exactly in bounds.
        gallery_of(framed(
            Some(ImageCrop {
                x_bp: 5_000,
                y_bp: 5_000,
                width_bp: 5_000,
                height_bp: 5_000,
            }),
            None,
        ))
        .validate()
        .unwrap();

        for bad in [
            // One basis point past the right edge.
            ImageCrop {
                x_bp: 5_001,
                y_bp: 0,
                width_bp: 5_000,
                height_bp: 10_000,
            },
            // One basis point past the bottom edge.
            ImageCrop {
                x_bp: 0,
                y_bp: 1,
                width_bp: 10_000,
                height_bp: 10_000,
            },
            // A rectangle with no area at all.
            ImageCrop {
                x_bp: 0,
                y_bp: 0,
                width_bp: 0,
                height_bp: 10_000,
            },
            // Narrower than the minimum extent.
            ImageCrop {
                x_bp: 0,
                y_bp: 0,
                width_bp: 10_000,
                height_bp: MIN_CROP_EXTENT_BP - 1,
            },
        ] {
            assert!(
                matches!(
                    gallery_of(framed(Some(bad), None)).validate(),
                    Err(SectionSchemaError::Invalid {
                        section: "gallery",
                        ..
                    })
                ),
                "expected rejected: {bad:?}"
            );
        }
    }

    #[test]
    fn a_focal_point_must_be_inside_the_image_and_inside_its_crop() {
        let crop = ImageCrop {
            x_bp: 2_000,
            y_bp: 2_000,
            width_bp: 3_000,
            height_bp: 3_000,
        };
        // On the crop's own boundary is inside it.
        gallery_of(framed(
            Some(crop),
            Some(ImageFocalPoint {
                x_bp: 5_000,
                y_bp: 2_000,
            }),
        ))
        .validate()
        .unwrap();
        // Without a crop, anywhere in the source is fine.
        gallery_of(framed(
            None,
            Some(ImageFocalPoint {
                x_bp: 10_000,
                y_bp: 0,
            }),
        ))
        .validate()
        .unwrap();

        for (crop, focal) in [
            // Off the source entirely.
            (
                None,
                ImageFocalPoint {
                    x_bp: 10_001,
                    y_bp: 0,
                },
            ),
            (
                None,
                ImageFocalPoint {
                    x_bp: 0,
                    y_bp: 10_001,
                },
            ),
            // Inside the source, but outside the crop it belongs to — the two
            // props would contradict each other.
            (
                Some(crop),
                ImageFocalPoint {
                    x_bp: 1_999,
                    y_bp: 3_000,
                },
            ),
            (
                Some(crop),
                ImageFocalPoint {
                    x_bp: 3_000,
                    y_bp: 5_001,
                },
            ),
        ] {
            assert!(
                matches!(
                    gallery_of(framed(crop, Some(focal))).validate(),
                    Err(SectionSchemaError::Invalid {
                        section: "gallery",
                        ..
                    })
                ),
                "expected rejected: {focal:?} in {crop:?}"
            );
        }
    }

    #[test]
    fn decorative_and_missing_alt_text_are_different_states() {
        let blob = BlobId::new("9hK3vQ2mR8pT1xWz4bC5dg");

        let written = SiteImage::new(blob.clone(), "A drum roaster");
        assert!(!written.needs_alt_text());

        let not_written = SiteImage::new(blob.clone(), "   ");
        assert!(
            not_written.needs_alt_text(),
            "whitespace is not alt text either"
        );
        gallery_of(not_written).validate().unwrap();

        let decorative = SiteImage {
            decorative: true,
            ..SiteImage::new(blob.clone(), "")
        };
        assert!(!decorative.needs_alt_text());
        gallery_of(decorative).validate().unwrap();

        // Both at once is a contradiction: the renderer emits `alt=""` for a
        // decorative image, so the alt text would silently disappear.
        let both = SiteImage {
            decorative: true,
            ..SiteImage::new(blob, "A drum roaster")
        };
        assert!(matches!(
            gallery_of(both).validate(),
            Err(SectionSchemaError::Invalid {
                section: "gallery",
                ..
            })
        ));
    }

    #[test]
    fn presentation_props_round_trip_through_every_image_bearing_variant() {
        let crop = ImageCrop {
            x_bp: 1_000,
            y_bp: 500,
            width_bp: 8_000,
            height_bp: 9_000,
        };
        let focal = ImageFocalPoint {
            x_bp: 4_000,
            y_bp: 3_000,
        };
        let image = framed(Some(crop), Some(focal));
        let before = envelope(vec![
            Section::Hero(HeroSection {
                heading: "Hello".to_owned(),
                subheading: None,
                image: Some(image.clone()),
                video_url: None,
                primary_cta: None,
                secondary_cta: None,
                appearance: None,
                layout: None,
                height: None,
                alignment: None,
                content_width: None,
                text_animation: None,
                media_animation: None,
                animation_speed: None,
            }),
            Section::TextImage(TextImageSection {
                heading: None,
                body: "Body".to_owned(),
                image: image.clone(),
                image_side: ImageSide::Left,
                split: None,
                layout: None,
                presentation: None,
            }),
            Section::Gallery(GallerySection {
                heading: None,
                images: vec![image.clone()],
                columns: None,
                layout: None,
                presentation: None,
            }),
            Section::Team(TeamSection {
                heading: None,
                members: vec![TeamMember {
                    name: "Jonas Meer".to_owned(),
                    role: None,
                    photo: Some(image),
                    bio: None,
                }],
                columns: None,
                layout: None,
                presentation: None,
            }),
        ]);
        before.validate().unwrap();
        let after = SectionsEnvelope::from_value(before.to_value().unwrap()).unwrap();
        assert_eq!(before, after);
        for section in &after.sections {
            let images = section.images();
            assert_eq!(images.len(), 1, "{} declared no image", section.kind());
            assert_eq!(images[0].crop, Some(crop));
            assert_eq!(images[0].focal, Some(focal));
        }
    }

    #[test]
    fn unknown_presentation_prop_is_rejected() {
        // The crop is a closed shape too: a writer that invents `zoom` finds
        // out on write, not by having it silently dropped.
        let value = json!({
            "schema_version": 1,
            "sections": [{
                "type": "gallery",
                "images": [{
                    "blob_id": "9hK3vQ2mR8pT1xWz4bC5dg",
                    "alt": "",
                    "crop": {"x_bp": 0, "y_bp": 0, "width_bp": 10000, "height_bp": 10000, "zoom": 2}
                }]
            }]
        });
        assert!(matches!(
            SectionsEnvelope::from_value(value),
            Err(SectionSchemaError::Shape(_))
        ));
    }

    #[test]
    fn images_lists_every_image_bearing_variant_and_nothing_else() {
        let sections = full_sections();
        let with_images: Vec<&'static str> = sections
            .iter()
            .filter(|section| !section.images().is_empty())
            .map(Section::kind)
            .collect();
        assert_eq!(with_images, ["hero", "text_image", "gallery", "team"]);
        // The blob-id view is exactly the same set, reduced.
        for section in &sections {
            let from_images: Vec<&str> = section
                .images()
                .into_iter()
                .map(|i| i.blob_id.as_str())
                .collect();
            let direct: Vec<&str> = section
                .image_blob_ids()
                .into_iter()
                .map(BlobId::as_str)
                .collect();
            assert_eq!(from_images, direct);
        }
    }

    #[test]
    fn new_envelope_is_current_version_and_valid() {
        let envelope = SectionsEnvelope::new();
        assert_eq!(envelope.schema_version, SECTIONS_SCHEMA_VERSION);
        assert!(envelope.sections.is_empty());
        envelope.validate().unwrap();
    }
}
