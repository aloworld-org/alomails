//! What a section may be resized to (ADR 0042, S3.01c) — the closed vocabulary
//! of shapes and ratios, and the per-section-type declaration of which of them
//! that type offers.
//!
//! ADR 0042 allows resizing *within a section's own constraints*: "a two-column
//! split moves between its allowed ratios; an image picks from its allowed
//! shapes". This module is where those constraints are written down, once:
//!
//! - The three enums ([`ColumnSplit`], [`GridColumns`], [`ImageShape`]) are the
//!   only values a stored section can hold. They are words, never numbers — a
//!   gesture cannot land between two of them, so free positioning is not
//!   something the editor has to refuse: it is something the schema cannot
//!   express.
//! - [`layout_controls`] declares, per section *type*, which properties are
//!   resizable, the JSON pointer each one lives at, the values it may take **in
//!   order** (narrowest first, so stepping through them with a key is
//!   monotonic), and what an absent value means.
//!
//! Everything that offers a resize reads that declaration rather than repeating
//! it: the editor (over `GET /sites/config`), the preview document's keyboard
//! gesture, and the renderer's own class names. A new resizable property is one
//! entry here plus the CSS that honours it — and a variant added to an enum
//! without being declared fails a test in this module rather than silently
//! becoming a value no editor ever offers.
//!
//! Absent means "as it has always rendered": every field is `Option`, every
//! `None` renders exactly the page a site stored before this schema gained
//! them, and nothing is written until somebody resizes something.

use serde::{Deserialize, Serialize};

/// How a `text_image` section divides its row between the image and the text,
/// at the width where the two sit side by side. Ordered by how much room the
/// **text** gets, so stepping forward always widens the words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColumnSplit {
    /// The image takes the larger share of the row.
    WideImage,
    /// Equal columns — what an absent split renders as.
    Half,
    /// The text takes the larger share of the row.
    WideText,
}

impl ColumnSplit {
    /// Every split, narrowest text first.
    pub const ALL: &'static [ColumnSplit] = &[
        ColumnSplit::WideImage,
        ColumnSplit::Half,
        ColumnSplit::WideText,
    ];

    /// The wire word — exactly what serde reads and writes.
    pub const fn as_str(self) -> &'static str {
        match self {
            ColumnSplit::WideImage => "wide_image",
            ColumnSplit::Half => "half",
            ColumnSplit::WideText => "wide_text",
        }
    }

    /// The class the renderer puts on the section for this split.
    pub const fn class(self) -> &'static str {
        match self {
            ColumnSplit::WideImage => "split-wide-image",
            ColumnSplit::Half => "split-half",
            ColumnSplit::WideText => "split-wide-text",
        }
    }
}

/// How many cards a grid section shows per row on a wide screen. A phone
/// always gets one column and a tablet at most two, whichever of these is
/// chosen — the choice is a ceiling, never a promise to squeeze four cards
/// onto a 360-pixel screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GridColumns {
    /// Two cards per row.
    Two,
    /// Three cards per row.
    Three,
    /// Four cards per row.
    Four,
}

impl GridColumns {
    /// Every column count, fewest first.
    pub const ALL: &'static [GridColumns] =
        &[GridColumns::Two, GridColumns::Three, GridColumns::Four];

    /// The wire word.
    pub const fn as_str(self) -> &'static str {
        match self {
            GridColumns::Two => "two",
            GridColumns::Three => "three",
            GridColumns::Four => "four",
        }
    }

    /// The class the renderer puts on the section for this count.
    pub const fn class(self) -> &'static str {
        match self {
            GridColumns::Two => "cols-2",
            GridColumns::Three => "cols-3",
            GridColumns::Four => "cols-4",
        }
    }
}

/// The frame an image is shown in. `Natural` keeps the picture's own
/// proportions; the others hold a fixed ratio and crop what does not fit,
/// around the image's focal point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageShape {
    /// The image's own proportions — what an absent shape renders as.
    Natural,
    /// A wide banner (16:9).
    Wide,
    /// A square (1:1).
    Square,
    /// An upright portrait (3:4).
    Tall,
}

impl ImageShape {
    /// Every shape, widest first.
    pub const ALL: &'static [ImageShape] = &[
        ImageShape::Wide,
        ImageShape::Natural,
        ImageShape::Square,
        ImageShape::Tall,
    ];

    /// The wire word.
    pub const fn as_str(self) -> &'static str {
        match self {
            ImageShape::Natural => "natural",
            ImageShape::Wide => "wide",
            ImageShape::Square => "square",
            ImageShape::Tall => "tall",
        }
    }

    /// The class the renderer puts on the image's `<figure>`; `None` for the
    /// natural shape, which is the page as it renders with no shape at all.
    pub const fn class(self) -> Option<&'static str> {
        match self {
            ImageShape::Natural => None,
            ImageShape::Wide => Some("shape-wide"),
            ImageShape::Square => Some("shape-square"),
            ImageShape::Tall => Some("shape-tall"),
        }
    }
}

/// One resizable property of one section type: what it is called on the wire,
/// where it lives inside the section, and the complete list of values it may
/// take — in the order a stepping gesture walks them.
///
/// The pointer is what makes a resize the *same* change shape as every other
/// edit: it is applied as a `set_prop` operation through the reviewed edit
/// door, so a person dragging and a model proposing produce one diff, one
/// undo entry and one stored envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutControl {
    /// The control's name, stable on the wire (`split`, `columns`, `shape`).
    pub key: &'static str,
    /// RFC 6901 pointer to the property inside the section
    /// (`/split`, `/image/shape`).
    pub pointer: &'static str,
    /// Every value this control offers, in order.
    pub values: &'static [&'static str],
    /// What an absent value renders as — the value a control starts on.
    pub default_value: &'static str,
}

impl LayoutControl {
    /// Whether `value` is one this control offers. The one question every
    /// door asks before writing a resize.
    pub fn offers(&self, value: &str) -> bool {
        self.values.contains(&value)
    }
}

/// The declaration for `text_image`.
const TEXT_IMAGE_CONTROLS: &[LayoutControl] = &[
    LayoutControl {
        key: "split",
        pointer: "/split",
        values: &["wide_image", "half", "wide_text"],
        default_value: "half",
    },
    LayoutControl {
        key: "shape",
        pointer: "/image/shape",
        values: &["wide", "natural", "square", "tall"],
        default_value: "natural",
    },
];

/// The declaration for `hero` — its image, when it has one.
const HERO_CONTROLS: &[LayoutControl] = &[LayoutControl {
    key: "shape",
    pointer: "/image/shape",
    values: &["wide", "natural", "square", "tall"],
    default_value: "natural",
}];

/// The declaration for a card grid of at most three across (`features`).
const THREE_COLUMN_CONTROLS: &[LayoutControl] = &[LayoutControl {
    key: "columns",
    pointer: "/columns",
    values: &["two", "three"],
    default_value: "three",
}];

/// The declaration for a card grid that may go four across (`gallery`,
/// `team`).
const FOUR_COLUMN_CONTROLS: &[LayoutControl] = &[LayoutControl {
    key: "columns",
    pointer: "/columns",
    values: &["two", "three", "four"],
    default_value: "three",
}];

/// Nothing to resize.
const NO_CONTROLS: &[LayoutControl] = &[];

/// What the section type `kind` (the wire tag: `hero`, `text_image`, …) may be
/// resized to. An unknown or non-resizable type answers with an empty slice,
/// which is what "this section has no handles" means everywhere.
pub fn layout_controls(kind: &str) -> &'static [LayoutControl] {
    match kind {
        "hero" => HERO_CONTROLS,
        "text_image" => TEXT_IMAGE_CONTROLS,
        "features" => THREE_COLUMN_CONTROLS,
        "gallery" | "team" => FOUR_COLUMN_CONTROLS,
        _ => NO_CONTROLS,
    }
}

/// The one control named `key` on section type `kind`, or `None` — the lookup
/// a write path does before it turns a gesture into a `set_prop`.
pub fn layout_control(kind: &str, key: &str) -> Option<&'static LayoutControl> {
    layout_controls(kind).iter().find(|c| c.key == key)
}

/// Every section type that offers at least one control, in declaration order —
/// what `GET /sites/config` publishes to the editor.
pub const RESIZABLE_SECTION_KINDS: &[&str] = &["hero", "text_image", "features", "gallery", "team"];

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::site_model::{Section, SectionsEnvelope};

    /// Every declared value must be a word the schema actually parses — the
    /// declaration and the enum are two spellings of one vocabulary, and a
    /// third spelling in a test would only prove the test.
    #[test]
    fn every_declared_value_is_a_value_the_schema_speaks() {
        for kind in RESIZABLE_SECTION_KINDS {
            for control in layout_controls(kind) {
                for value in control.values {
                    let parsed: Result<serde_json::Value, _> =
                        serde_json::to_value(value).map_err(|e| e.to_string());
                    assert!(parsed.is_ok());
                    match control.key {
                        "split" => {
                            serde_json::from_value::<ColumnSplit>(serde_json::json!(value))
                                .unwrap_or_else(|e| panic!("{kind}/{value}: {e}"));
                        }
                        "columns" => {
                            serde_json::from_value::<GridColumns>(serde_json::json!(value))
                                .unwrap_or_else(|e| panic!("{kind}/{value}: {e}"));
                        }
                        "shape" => {
                            serde_json::from_value::<ImageShape>(serde_json::json!(value))
                                .unwrap_or_else(|e| panic!("{kind}/{value}: {e}"));
                        }
                        other => panic!("undeclared control key {other}"),
                    }
                }
                assert!(
                    control.offers(control.default_value),
                    "{kind}/{}: default is not one of the offered values",
                    control.key
                );
            }
        }
    }

    /// A variant that exists but is never offered is a value the editor cannot
    /// reach and a page could still be stored with. Both enums with a single
    /// meaning per section type are checked whole.
    #[test]
    fn every_variant_of_every_enum_is_declared_somewhere() {
        let declared = |key: &str| -> Vec<&'static str> {
            RESIZABLE_SECTION_KINDS
                .iter()
                .flat_map(|kind| layout_controls(kind))
                .filter(|c| c.key == key)
                .flat_map(|c| c.values.iter().copied())
                .collect()
        };
        for split in ColumnSplit::ALL {
            assert!(declared("split").contains(&split.as_str()), "{split:?}");
        }
        for shape in ImageShape::ALL {
            assert!(declared("shape").contains(&shape.as_str()), "{shape:?}");
        }
        // A grid section may legitimately cap its own count (a feature card is
        // wider than a photo), so each declared list is a subset — but every
        // variant has to be offered by at least one section type.
        for columns in GridColumns::ALL {
            assert!(
                declared("columns").contains(&columns.as_str()),
                "{columns:?}"
            );
        }
    }

    /// The declaration is only useful if its pointers hit real properties.
    /// Each is applied to a section of its own kind and the result must parse
    /// and validate — the same gate a resize goes through on the wire.
    #[test]
    fn every_declared_pointer_addresses_a_property_the_schema_accepts() {
        for kind in RESIZABLE_SECTION_KINDS {
            for control in layout_controls(kind) {
                for value in control.values {
                    let mut section = sample(kind);
                    set_pointer(&mut section, control.pointer, serde_json::json!(value));
                    let envelope = serde_json::json!({
                        "schema_version": 1,
                        "sections": [section],
                    });
                    SectionsEnvelope::from_value(envelope)
                        .unwrap_or_else(|e| panic!("{kind} {} = {value}: {e}", control.pointer));
                }
            }
        }
    }

    /// The property that makes ADR 0042's promise keepable: nothing between
    /// the declared words is a value. A percentage, a pixel count, a fraction
    /// — each is refused by the schema itself, so "no gesture can produce free
    /// positioning" does not depend on any editor behaving.
    #[test]
    fn a_free_value_is_not_expressible() {
        for free in [
            serde_json::json!(0.37),
            serde_json::json!(37),
            serde_json::json!("37%"),
            serde_json::json!("1.5fr"),
            serde_json::json!("half "),
            serde_json::json!({ "left_bp": 3700 }),
        ] {
            for kind in RESIZABLE_SECTION_KINDS {
                for control in layout_controls(kind) {
                    let mut section = sample(kind);
                    set_pointer(&mut section, control.pointer, free.clone());
                    let envelope = serde_json::json!({
                        "schema_version": 1,
                        "sections": [section],
                    });
                    assert!(
                        SectionsEnvelope::from_value(envelope).is_err(),
                        "{kind} {} accepted {free}",
                        control.pointer
                    );
                }
            }
            assert!(
                !layout_controls("text_image")
                    .iter()
                    .any(|c| c.offers(&free.to_string())),
            );
        }
    }

    /// A section of `kind` with everything its declared pointers need, as
    /// JSON: the fixture the two tests above resize.
    fn sample(kind: &str) -> serde_json::Value {
        let image = serde_json::json!({ "blob_id": "blob-1", "alt": "A shop front" });
        match kind {
            "hero" => serde_json::json!({
                "type": "hero", "heading": "Welcome", "image": image,
            }),
            "text_image" => serde_json::json!({
                "type": "text_image", "body": "We bake bread.",
                "image": image, "image_side": "left",
            }),
            "features" => serde_json::json!({
                "type": "features",
                "items": [{ "title": "Fresh", "body": "Daily." }],
            }),
            "gallery" => serde_json::json!({ "type": "gallery", "images": [image] }),
            "team" => serde_json::json!({
                "type": "team", "members": [{ "name": "Ada" }],
            }),
            other => panic!("no fixture for {other}"),
        }
    }

    /// Writes `value` at an RFC 6901 pointer of the shape the declaration
    /// uses (`/a` or `/a/b`), creating nothing: every pointer declared here
    /// addresses a property of a section that exists.
    fn set_pointer(section: &mut serde_json::Value, pointer: &str, value: serde_json::Value) {
        let mut cursor = section;
        let tokens: Vec<&str> = pointer.split('/').skip(1).collect();
        let (last, parents) = tokens.split_last().expect("pointer with a leaf");
        for token in parents {
            cursor = cursor.get_mut(*token).expect("pointer parent exists");
        }
        cursor
            .as_object_mut()
            .expect("pointer parent is an object")
            .insert((*last).to_string(), value);
    }

    /// The renderer keys its CSS off these, so two variants sharing a class
    /// would be two arrangements that look like one.
    #[test]
    fn every_class_name_is_distinct() {
        let mut classes: Vec<&str> = Vec::new();
        classes.extend(ColumnSplit::ALL.iter().map(|v| v.class()));
        classes.extend(GridColumns::ALL.iter().map(|v| v.class()));
        classes.extend(ImageShape::ALL.iter().filter_map(|v| v.class()));
        let mut sorted = classes.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), classes.len(), "{classes:?}");
    }

    /// Sections nobody can resize answer with nothing, rather than with a
    /// control whose pointer would 422 on write.
    #[test]
    fn a_section_with_no_handles_declares_none() {
        for kind in [
            "nav",
            "footer",
            "faq",
            "cta",
            "custom_code",
            "not-a-section",
        ] {
            assert!(layout_controls(kind).is_empty(), "{kind}");
            assert!(layout_control(kind, "columns").is_none(), "{kind}");
        }
        // …and the resizable list is exactly the kinds that declare something.
        for section in [
            serde_json::json!({ "type": "faq" }),
            serde_json::json!({ "type": "cta" }),
        ] {
            let kind = section["type"].as_str().expect("a tag");
            assert!(!RESIZABLE_SECTION_KINDS.contains(&kind));
        }
    }

    /// Every kind the declaration names is a section type this build has.
    #[test]
    fn every_declared_kind_is_a_real_section_type() {
        for kind in RESIZABLE_SECTION_KINDS {
            let section: Section = serde_json::from_value(sample(kind)).expect("a section");
            assert_eq!(&section.kind(), kind);
        }
    }
}
