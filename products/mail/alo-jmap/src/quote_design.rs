//! The quotation design as the server reads it — the parts of the studio's
//! saved JSON that the printed page and the PDF render.
//!
//! The **web client owns the design's shape** (`QuoteStudioDesign.ts`); the
//! store keeps the document whole (`alo_store::billing_quote_designs`). This
//! module reads it **leniently**: a field that is missing takes the studio's
//! own default, a block kind the server does not know is skipped rather than
//! failing the print, and every value that will reach markup — a colour, an
//! image source, an enum — is validated into a closed set here, once, so the
//! renderers never interpolate a stored string.
//!
//! What is read: the content blocks, the marker/divider/table colours, and
//! which columns of the price table are shown. Header styles, the logo, the
//! contact QR and the table's presentation options are the studio's on-screen
//! design and are recorded in `docs/design/billing.md` as not yet printed.

use base64::Engine;
use serde_json::Value;

use crate::quote_design_lists::ListStyle;

/// A colour the studio picked, validated to `#rgb` or `#rrggbb`.
fn color(value: Option<&Value>) -> Option<String> {
    let text = value?.as_str()?.trim();
    let hex = text.strip_prefix('#')?;
    if (hex.len() == 3 || hex.len() == 6) && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(text.to_ascii_lowercase())
    } else {
        None
    }
}

/// The colours the printed content uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Colors {
    /// Quote bars and dividers.
    pub accent: String,
    /// Bullet markers.
    pub bullet_marker: String,
    /// Numbering markers.
    pub number_marker: String,
    /// The heading row of an information table.
    pub table_header: String,
}

impl Default for Colors {
    fn default() -> Self {
        Self {
            accent: "#e76f51".to_owned(),
            bullet_marker: "#e76f51".to_owned(),
            number_marker: "#e76f51".to_owned(),
            table_header: "#f3f0ea".to_owned(),
        }
    }
}

impl Colors {
    fn parse(value: Option<&Value>) -> Self {
        let defaults = Self::default();
        let field = |name: &str| value.and_then(|v| v.get(name));
        Self {
            accent: color(field("accent")).unwrap_or(defaults.accent),
            bullet_marker: color(field("bulletMarker")).unwrap_or(defaults.bullet_marker),
            number_marker: color(field("numberMarker")).unwrap_or(defaults.number_marker),
            table_header: color(field("tableHeader")).unwrap_or(defaults.table_header),
        }
    }
}

/// Which columns of the price table are printed. The description is always
/// printed; the studio lets the rest be hidden.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColumnVisibility {
    /// The unit label beside the quantity.
    pub unit: bool,
    /// The quantity column.
    pub quantity: bool,
    /// The unit-price column.
    pub unit_price: bool,
    /// The VAT-rate column.
    pub vat: bool,
    /// The line-net column.
    pub net: bool,
}

impl Default for ColumnVisibility {
    fn default() -> Self {
        Self {
            unit: true,
            quantity: true,
            unit_price: true,
            vat: true,
            net: true,
        }
    }
}

impl ColumnVisibility {
    fn parse(value: Option<&Value>) -> Self {
        let defaults = Self::default();
        let flag = |name: &str, default: bool| {
            value
                .and_then(|v| v.get(name))
                .and_then(Value::as_bool)
                .unwrap_or(default)
        };
        Self {
            unit: flag("unit", defaults.unit),
            quantity: flag("quantity", defaults.quantity),
            unit_price: flag("unitPrice", defaults.unit_price),
            vat: flag("vat", defaults.vat),
            net: flag("net", defaults.net),
        }
    }

    /// How many numeric columns are printed beside the description.
    #[must_use]
    pub fn numeric_count(self) -> usize {
        usize::from(self.quantity)
            + usize::from(self.unit_price)
            + usize::from(self.vat)
            + usize::from(self.net)
    }
}

/// A divider's weight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Thickness {
    /// A hairline.
    Fine,
    /// A rule.
    Medium,
    /// A bar.
    Bold,
}

/// A divider's stroke.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineStyle {
    /// Unbroken.
    Solid,
    /// Dashes.
    Dashed,
    /// Dots.
    Dotted,
}

/// Where a picture sits relative to its text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// Across the whole column, text (if any) below.
    Full,
    /// Picture left, text right.
    Left,
    /// Text left, picture right.
    Right,
}

/// The frame a picture is shown in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aspect {
    /// The picture's own proportions.
    Natural,
    /// A wide 16:7 frame.
    Landscape,
    /// A square frame.
    Square,
}

/// How a picture meets its frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    /// Fills the frame; the overflow is cropped.
    Cover,
    /// Whole picture visible; spare frame left blank.
    Contain,
}

/// A picture as the studio stored it: a `data:` URL, validated to an image
/// type and the base64 alphabet, so it can be re-emitted verbatim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataImage {
    /// `image/jpeg`, `image/png`, `image/webp` or `image/gif`.
    pub mime: &'static str,
    /// The base64 payload, unchanged.
    pub base64: String,
}

impl DataImage {
    fn parse(src: &str) -> Option<Self> {
        // The development corpus stores a short path to this reusable,
        // repository-owned illustration. Browsers can request its SVG, while
        // a PDF must be self-contained and its writer accepts raster images
        // only. Resolve this one closed path at render time so no network
        // request or large base64 database field is involved.
        //
        // The crate carries its own copy of the raster. It used to reach four
        // levels up into `web/public/`, which built here and failed in the
        // container: the image copies `platform`, `products`, `suite` and
        // `migrate`, never `web`, so production could not be built at all
        // between 2026-08-28 and this line. A crate that embeds a file owns
        // that file; the browser's copy under `web/` stays where the browser
        // wants it.
        if src == "/demo/billing/workspace.svg" {
            return Some(Self {
                mime: "image/png",
                base64: base64::engine::general_purpose::STANDARD
                    .encode(include_bytes!("../assets/demo-billing-workspace.png")),
            });
        }
        let rest = src.strip_prefix("data:")?;
        let (mime, payload) = rest.split_once(";base64,")?;
        let mime = match mime.to_ascii_lowercase().as_str() {
            "image/jpeg" | "image/jpg" => "image/jpeg",
            "image/png" => "image/png",
            "image/webp" => "image/webp",
            "image/gif" => "image/gif",
            _ => return None,
        };
        let payload = payload.trim();
        if payload.is_empty()
            || !payload
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/' || b == b'=')
        {
            return None;
        }
        Some(Self {
            mime,
            base64: payload.to_owned(),
        })
    }

    /// The `data:` URL, rebuilt from the validated parts.
    #[must_use]
    pub fn data_url(&self) -> String {
        format!("data:{};base64,{}", self.mime, self.base64)
    }
}

/// One content block, in the studio's vocabulary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    /// A titled passage.
    Text {
        /// Inline rich text.
        heading: String,
        /// Rich text.
        body: String,
    },
    /// A section heading.
    Heading {
        /// 1, 2 or 3.
        level: u8,
        /// Inline rich text.
        text: String,
    },
    /// Running prose, optionally flowed into columns.
    Paragraph {
        /// Rich text.
        text: String,
        /// 1 to 3.
        columns: u8,
    },
    /// A pull quote.
    Quote {
        /// Rich text.
        text: String,
        /// Inline rich text; blank for none.
        attribution: String,
        /// 1 to 3.
        columns: u8,
    },
    /// A numbered or bulleted list.
    List {
        /// Numbered rather than bulleted.
        ordered: bool,
        /// Newline-separated items, a leading tab per nesting level.
        items: String,
        /// 1 to 3.
        columns: u8,
        /// The marker scheme.
        style: ListStyle,
    },
    /// A rule between sections.
    Divider {
        /// Weight.
        thickness: Thickness,
        /// Stroke.
        style: LineStyle,
        /// 25, 50, 75 or 100.
        width_percent: u8,
        /// A colour of its own, or `None` for the accent.
        color: Option<String>,
    },
    /// A picture, with a caption and text beside or below it.
    Image {
        /// The picture, or `None` when the block has none yet.
        src: Option<DataImage>,
        /// Rich text under the picture; blank for none.
        caption: String,
        /// Rich text beside or below the picture; blank for none.
        body: String,
        /// Where the picture sits.
        placement: Placement,
        /// Picture share of the width when placed left or right, in percent.
        picture_percent: u8,
        /// The frame.
        aspect: Aspect,
        /// How the picture meets the frame.
        fit: Fit,
    },
    /// Where the price table goes.
    Pricing,
    /// An information table.
    Table {
        /// Column headings, inline rich text.
        columns: Vec<String>,
        /// Rows of cells, rich text, one cell per column.
        rows: Vec<Vec<String>>,
    },
}

fn text(value: &Value, name: &str) -> String {
    value
        .get(name)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn columns(value: &Value) -> u8 {
    match value.get("columns").and_then(Value::as_u64) {
        Some(n @ 2..=3) => u8::try_from(n).unwrap_or(1),
        _ => 1,
    }
}

impl Block {
    /// Reads one block; `None` for a kind the server does not print.
    fn parse(value: &Value) -> Option<Self> {
        let kind = value.get("kind").and_then(Value::as_str)?;
        Some(match kind {
            "text" => Self::Text {
                heading: text(value, "heading"),
                body: text(value, "body"),
            },
            "heading" => Self::Heading {
                level: match value.get("level").and_then(Value::as_u64) {
                    Some(n @ 1..=3) => u8::try_from(n).unwrap_or(2),
                    _ => 2,
                },
                text: text(value, "text"),
            },
            "paragraph" => Self::Paragraph {
                text: text(value, "text"),
                columns: columns(value),
            },
            "quote" => Self::Quote {
                text: text(value, "text"),
                attribution: text(value, "attribution"),
                columns: columns(value),
            },
            "list" => {
                let ordered = value
                    .get("ordered")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                Self::List {
                    ordered,
                    items: text(value, "items"),
                    columns: columns(value),
                    style: ListStyle::resolve(value.get("style").and_then(Value::as_str), ordered),
                }
            }
            "divider" => Self::Divider {
                thickness: match value.get("thickness").and_then(Value::as_str) {
                    Some("medium") => Thickness::Medium,
                    Some("bold") => Thickness::Bold,
                    _ => Thickness::Fine,
                },
                style: match value.get("style").and_then(Value::as_str) {
                    Some("dashed") => LineStyle::Dashed,
                    Some("dotted") => LineStyle::Dotted,
                    _ => LineStyle::Solid,
                },
                width_percent: match value.get("width").and_then(Value::as_u64) {
                    Some(25) => 25,
                    Some(50) => 50,
                    Some(75) => 75,
                    _ => 100,
                },
                color: color(value.get("color")),
            },
            "image" => Self::Image {
                src: value
                    .get("src")
                    .and_then(Value::as_str)
                    .and_then(DataImage::parse),
                caption: text(value, "caption"),
                body: text(value, "body"),
                placement: match value.get("placement").and_then(Value::as_str) {
                    Some("left") => Placement::Left,
                    Some("right") => Placement::Right,
                    _ => Placement::Full,
                },
                picture_percent: match value.get("columnRatio").and_then(Value::as_str) {
                    Some("33-67") => 33,
                    Some("40-60") => 40,
                    Some("60-40") => 60,
                    Some("67-33") => 67,
                    _ => 50,
                },
                aspect: match value.get("aspect").and_then(Value::as_str) {
                    Some("natural") => Aspect::Natural,
                    Some("square") => Aspect::Square,
                    _ => Aspect::Landscape,
                },
                fit: match value.get("fit").and_then(Value::as_str) {
                    Some("contain") => Fit::Contain,
                    _ => Fit::Cover,
                },
            },
            "pricing" => Self::Pricing,
            "table" => {
                let columns: Vec<(String, String)> = value
                    .get("columns")
                    .and_then(Value::as_array)
                    .map(|cols| {
                        cols.iter()
                            .map(|c| (text(c, "id"), text(c, "label")))
                            .collect()
                    })
                    .unwrap_or_default();
                let rows: Vec<Vec<String>> = value
                    .get("rows")
                    .and_then(Value::as_array)
                    .map(|rows| {
                        rows.iter()
                            .map(|row| {
                                columns
                                    .iter()
                                    .map(|(id, _)| {
                                        row.get("cells")
                                            .and_then(|cells| cells.get(id))
                                            .and_then(Value::as_str)
                                            .unwrap_or_default()
                                            .to_owned()
                                    })
                                    .collect()
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                Self::Table {
                    columns: columns.into_iter().map(|(_, label)| label).collect(),
                    rows,
                }
            }
            _ => return None,
        })
    }

    /// Whether the block would print anything at all. The studio hides an
    /// empty block on the customer-facing view; so does the page.
    #[must_use]
    pub fn has_content(&self) -> bool {
        let filled = |s: &str| !s.trim().is_empty();
        match self {
            Self::Text { heading, body } => filled(heading) || filled(body),
            Self::Heading { text, .. } | Self::Paragraph { text, .. } => filled(text),
            Self::Quote {
                text, attribution, ..
            } => filled(text) || filled(attribution),
            Self::List { items, .. } => items.split('\n').any(filled),
            Self::Divider { .. } | Self::Pricing => true,
            Self::Image {
                src, caption, body, ..
            } => src.is_some() || filled(caption) || filled(body),
            Self::Table { rows, .. } => rows.iter().flatten().any(|cell| filled(cell)),
        }
    }
}

/// A design as the renderers see it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuoteDesign {
    /// The content blocks in document order, the price table among them.
    pub blocks: Vec<Block>,
    /// The colours the content uses.
    pub colors: Colors,
    /// Which price-table columns are printed.
    pub columns: ColumnVisibility,
}

impl QuoteDesign {
    /// Reads a stored design. Never fails: what cannot be read is defaulted
    /// or skipped, because a print must always render.
    #[must_use]
    pub fn parse(value: &Value) -> Self {
        let blocks = value
            .get("blocks")
            .and_then(Value::as_array)
            .map(|blocks| {
                blocks
                    .iter()
                    .filter_map(Block::parse)
                    .filter(Block::has_content)
                    .collect()
            })
            .unwrap_or_default();
        Self {
            blocks,
            colors: Colors::parse(value.get("colors")),
            columns: ColumnVisibility::parse(value.get("columns")),
        }
    }

    /// The blocks before the price table and the blocks after it. A design
    /// with no price-table block prints all its content above the table.
    #[must_use]
    pub fn around_pricing(&self) -> (&[Block], &[Block]) {
        match self.blocks.iter().position(|b| matches!(b, Block::Pricing)) {
            Some(at) => (&self.blocks[..at], &self.blocks[at + 1..]),
            None => (&self.blocks, &[]),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use serde_json::json;

    #[test]
    fn a_design_is_read_leniently_and_validated_where_it_reaches_markup() {
        let design = QuoteDesign::parse(&json!({
            "blocks": [
                { "id": "h", "kind": "heading", "level": 9, "text": "Scope" },
                { "id": "p", "kind": "paragraph", "text": "<p>Hi</p>", "columns": 3 },
                { "id": "empty", "kind": "paragraph", "text": "  " },
                { "id": "x", "kind": "hologram", "text": "future" },
                { "id": "pricing-table", "kind": "pricing" },
                { "id": "l", "kind": "list", "ordered": true, "items": "A\n\tB", "style": "roman" },
                { "id": "d", "kind": "divider", "thickness": "bold", "style": "dashed", "width": 50, "color": "red" },
                { "id": "i", "kind": "image", "src": "javascript:alert(1)", "caption": "c" },
                { "id": "j", "kind": "image", "src": "data:image/PNG;base64,AAAA", "placement": "left", "columnRatio": "33-67" },
                { "id": "t", "kind": "table", "columns": [{ "id": "a", "label": "Item" }, { "id": "b", "label": "Qty" }],
                  "rows": [{ "id": "r", "cells": { "a": "Bolt", "b": "4" } }, { "id": "s", "cells": {} }] }
            ],
            "colors": { "accent": "#ABC", "bulletMarker": "not a colour", "tableHeader": "#112233" },
            "columns": { "unit": false, "vat": false }
        }));
        let (before, after) = design.around_pricing();
        assert_eq!(
            before.len(),
            2,
            "the empty paragraph and the unknown kind are skipped"
        );
        assert_eq!(
            before[0],
            Block::Heading {
                level: 2,
                text: "Scope".into()
            },
            "an out-of-range level takes the default"
        );
        assert!(matches!(before[1], Block::Paragraph { columns: 3, .. }));
        assert_eq!(after.len(), 5);
        assert!(matches!(
            after[0],
            Block::List {
                style: ListStyle::Roman,
                ordered: true,
                ..
            }
        ));
        assert_eq!(
            after[1],
            Block::Divider {
                thickness: Thickness::Bold,
                style: LineStyle::Dashed,
                width_percent: 50,
                color: None
            },
            "a colour that is not hex is not a colour"
        );
        // A picture whose source is not a data image prints as no picture.
        assert!(matches!(&after[2], Block::Image { src: None, caption, .. } if caption == "c"));
        assert!(matches!(
            &after[3],
            Block::Image {
                src: Some(DataImage {
                    mime: "image/png",
                    ..
                }),
                placement: Placement::Left,
                picture_percent: 33,
                ..
            }
        ));
        assert_eq!(
            after[4],
            Block::Table {
                columns: vec!["Item".into(), "Qty".into()],
                rows: vec![
                    vec!["Bolt".into(), "4".into()],
                    vec![String::new(), String::new()]
                ],
            }
        );
        assert_eq!(design.colors.accent, "#abc");
        assert_eq!(design.colors.bullet_marker, Colors::default().bullet_marker);
        assert_eq!(design.colors.table_header, "#112233");
        assert!(!design.columns.unit && !design.columns.vat && design.columns.net);
        assert_eq!(design.columns.numeric_count(), 3);
    }

    #[test]
    fn a_design_without_a_price_table_prints_everything_above_it() {
        let design = QuoteDesign::parse(&json!({ "blocks": [{ "kind": "heading", "text": "x" }] }));
        let (before, after) = design.around_pricing();
        assert_eq!((before.len(), after.len()), (1, 0));
        // And nothing at all is a valid design.
        assert_eq!(QuoteDesign::parse(&json!({})), QuoteDesign::default());
        assert_eq!(QuoteDesign::parse(&json!("junk")), QuoteDesign::default());
    }

    #[test]
    fn a_data_image_is_rebuilt_only_from_validated_parts() {
        let image = DataImage::parse("data:image/jpg;base64,/9j/4AAQ==").unwrap();
        assert_eq!(image.data_url(), "data:image/jpeg;base64,/9j/4AAQ==");
        assert!(DataImage::parse("data:text/html;base64,PHNjcmlwdD4=").is_none());
        assert!(DataImage::parse("data:image/png;base64,<script>").is_none());
        assert!(DataImage::parse("https://example.test/a.png").is_none());
    }

    #[test]
    fn the_repository_demo_picture_is_embedded_for_print_and_pdf() {
        let image = DataImage::parse("/demo/billing/workspace.svg").unwrap();
        assert_eq!(image.mime, "image/png");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&image.base64)
            .unwrap();
        assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
    }
}
