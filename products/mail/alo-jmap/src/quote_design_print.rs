//! The quotation's designed content on the printed page — the studio's
//! blocks as HTML, in the print stylesheet's idiom.
//!
//! This is the customer-facing rendering of what the studio shows on screen:
//! the same blocks in the same order, the same nesting and numbering
//! ([`crate::quote_design_lists`]), the same colours. It is written for A4,
//! not for the app — a block never breaks across a page where it can help it,
//! and columns are real print columns.
//!
//! The renderer's rules are the page's ([`crate::billing_print`]): every
//! character of text goes through the escaper or the rich-text allow-list
//! ([`crate::rich_text`]), and every value that reaches an attribute is one
//! of a closed set validated by [`crate::quote_design`] — a colour is hex, a
//! picture is a validated `data:` URL, a width is one of four numbers.

use crate::billing_print::esc;
use crate::quote_design::{Aspect, Block, Colors, Fit, LineStyle, Placement, Thickness};
use crate::quote_design_lists::{number_items, parse_items};
use crate::rich_text::{sanitize_inline, sanitize_rich};

/// The stylesheet the blocks need, appended to the page's own.
pub const DESIGN_STYLE: &str = "\
.blocks { margin: 4mm 0 2mm; }
.blk { margin: 0 0 4mm; page-break-inside: avoid; }
.blk-h1 { font-size: 16pt; font-weight: 600; line-height: 1.2; margin: 0 0 2mm; letter-spacing: -.2pt; }
.blk-h2 { font-size: 13pt; font-weight: 600; line-height: 1.25; margin: 0 0 2mm; }
.blk-h3 { font-size: 11pt; font-weight: 600; line-height: 1.3; margin: 0 0 1.5mm; }
.blk-text h3 { font-size: 11.5pt; font-weight: 600; margin: 0 0 1.5mm; }
.rich p { margin: 0 0 2mm; } .rich p:last-child { margin-bottom: 0; }
.rich h1 { font-size: 13pt; margin: 0 0 2mm; } .rich h2 { font-size: 11.5pt; margin: 0 0 1.5mm; } .rich h3 { font-size: 10.5pt; margin: 0 0 1.5mm; }
.rich ul, .rich ol { margin: 0 0 2mm; padding-left: 5mm; } .rich li { margin: 0 0 .8mm; }
.rich strong, .rich b { font-weight: 600; }
.cols-2 { column-count: 2; column-gap: 8mm; } .cols-3 { column-count: 3; column-gap: 8mm; }
.cols-2 > *, .cols-3 > * { break-inside: avoid; }
.blk-quote { margin: 0 0 4mm; padding: .5mm 0 .5mm 4mm; border-left: 1mm solid #e76f51; font-size: 11pt; font-style: italic; }
.blk-quote footer { margin-top: 1.5mm; font-size: 9pt; font-style: normal; color: #4a4f58; }
.blk-list { list-style: none; margin: 0 0 4mm; padding: 0; }
.blk-list li { display: flex; gap: 2.5mm; margin: 0 0 1.2mm; break-inside: avoid; }
.blk-list .mk { flex: none; min-width: 5mm; font-weight: 600; font-variant-numeric: tabular-nums; }
.blk-list .lvl1 { padding-left: 6mm; } .blk-list .lvl2 { padding-left: 12mm; }
.blk-hr { border: 0; border-top-style: solid; border-top-width: .25mm; margin: 4mm auto; }
.hr-medium { border-top-width: .6mm; } .hr-bold { border-top-width: 1.2mm; }
.hr-dashed { border-top-style: dashed; } .hr-dotted { border-top-style: dotted; }
.hr-25 { width: 25%; } .hr-50 { width: 50%; } .hr-75 { width: 75%; } .hr-100 { width: 100%; }
.blk-fig { margin: 0 0 4mm; }
.blk-fig .frame { overflow: hidden; border-radius: 2mm; background: #f3f0ea; }
.frame.landscape { aspect-ratio: 16 / 7; } .frame.square { aspect-ratio: 1 / 1; }
.frame img { display: block; width: 100%; height: 100%; object-fit: cover; }
.frame.contain img { object-fit: contain; }
.frame.natural { aspect-ratio: auto; } .frame.natural img { height: auto; max-height: 120mm; object-fit: contain; }
.blk-fig figcaption { margin-top: 1.5mm; font-size: 8pt; color: #6b7280; }
.blk-fig .copy { margin-top: 3mm; }
.blk-fig.side { display: flex; gap: 6mm; align-items: center; }
.blk-fig.side > * { min-width: 0; } .blk-fig.side .pic { flex: 0 0 var(--pic); } .blk-fig.side .copy { flex: 1 1 0; margin: 0; }
.blk-fig .empty-pic { display: flex; align-items: center; justify-content: center; min-height: 20mm; color: #6b7280; font-style: italic; font-size: 8pt; }
table.blk-table { width: 100%; border-collapse: collapse; margin: 0 0 4mm; }
.blk-table th, .blk-table td { padding: 1.8mm 2mm; text-align: left; vertical-align: top; border-bottom: .2mm solid #dcdfe5; }
.blk-table th { font-weight: 600; }
";

/// The blocks as HTML, or an empty string when there are none — so a
/// document without content adds nothing to the page, not an empty section.
#[must_use]
pub fn render_blocks(blocks: &[Block], colors: &Colors) -> String {
    if blocks.is_empty() {
        return String::new();
    }
    let mut out = String::from("<section class=\"blocks\">");
    for block in blocks {
        out.push_str(&render_block(block, colors));
    }
    out.push_str("</section>\n");
    out
}

fn columns_class(columns: u8) -> &'static str {
    match columns {
        2 => " cols-2",
        3 => " cols-3",
        _ => "",
    }
}

fn render_block(block: &Block, colors: &Colors) -> String {
    match block {
        Block::Text { heading, body } => format!(
            "<div class=\"blk blk-text\">{}{}</div>",
            if heading.trim().is_empty() {
                String::new()
            } else {
                format!("<h3>{}</h3>", sanitize_inline(heading))
            },
            if body.trim().is_empty() {
                String::new()
            } else {
                format!("<div class=\"rich body\">{}</div>", sanitize_rich(body))
            },
        ),
        Block::Heading { level, text } => {
            // The page's own `<h1>` is the document title; a designed heading
            // is a section heading whatever its size.
            let tag = match level {
                1 => "h2",
                2 => "h3",
                _ => "h4",
            };
            format!(
                "<{tag} class=\"blk blk-h{level}\">{}</{tag}>",
                sanitize_inline(text)
            )
        }
        Block::Paragraph { text, columns } => format!(
            "<div class=\"blk rich{}\">{}</div>",
            columns_class(*columns),
            sanitize_rich(text)
        ),
        Block::Quote {
            text,
            attribution,
            columns,
        } => format!(
            "<blockquote class=\"blk blk-quote\" style=\"border-left-color:{}\">\
             <div class=\"rich{}\">{}</div>{}</blockquote>",
            esc(&colors.accent),
            columns_class(*columns),
            sanitize_rich(text),
            if attribution.trim().is_empty() {
                String::new()
            } else {
                format!("<footer>{}</footer>", sanitize_inline(attribution))
            },
        ),
        Block::List {
            ordered,
            items,
            columns,
            style,
        } => {
            let marker_color = if *ordered {
                &colors.number_marker
            } else {
                &colors.bullet_marker
            };
            let tag = if *ordered { "ol" } else { "ul" };
            let mut out = format!("<{tag} class=\"blk blk-list{}\">", columns_class(*columns));
            for numbered in number_items(parse_items(items), *style) {
                out.push_str(&format!(
                    "<li class=\"lvl{}\"><span class=\"mk\" style=\"color:{}\">{}</span>\
                     <span>{}</span></li>",
                    numbered.item.level,
                    esc(marker_color),
                    esc(&numbered.marker),
                    sanitize_inline(&numbered.item.text),
                ));
            }
            out.push_str(&format!("</{tag}>"));
            out
        }
        Block::Divider {
            thickness,
            style,
            width_percent,
            color,
        } => format!(
            "<hr class=\"blk blk-hr hr-{} hr-{} hr-{width_percent}\" style=\"border-top-color:{}\">",
            match thickness {
                Thickness::Fine => "fine",
                Thickness::Medium => "medium",
                Thickness::Bold => "bold",
            },
            match style {
                LineStyle::Solid => "solid",
                LineStyle::Dashed => "dashed",
                LineStyle::Dotted => "dotted",
            },
            esc(color.as_deref().unwrap_or(&colors.accent)),
        ),
        Block::Image {
            src,
            caption,
            body,
            placement,
            picture_percent,
            aspect,
            fit,
        } => {
            let frame_class = format!(
                "frame {}{}",
                match aspect {
                    Aspect::Natural => "natural",
                    Aspect::Landscape => "landscape",
                    Aspect::Square => "square",
                },
                match fit {
                    Fit::Cover => "",
                    Fit::Contain => " contain",
                }
            );
            let picture = match src {
                Some(image) => format!(
                    "<div class=\"{frame_class}\"><img src=\"{}\" alt=\"{}\"></div>",
                    esc(&image.data_url()),
                    esc(&crate::rich_text::plain_inline(caption)),
                ),
                None => String::new(),
            };
            let figure = format!(
                "<figure class=\"pic\">{picture}{}</figure>",
                if caption.trim().is_empty() {
                    String::new()
                } else {
                    format!(
                        "<figcaption class=\"rich\">{}</figcaption>",
                        sanitize_rich(caption)
                    )
                }
            );
            let copy = if body.trim().is_empty() {
                String::new()
            } else {
                format!("<div class=\"copy rich\">{}</div>", sanitize_rich(body))
            };
            match placement {
                Placement::Full => format!("<div class=\"blk blk-fig\">{figure}{copy}</div>"),
                Placement::Left => format!(
                    "<div class=\"blk blk-fig side\" style=\"--pic:{picture_percent}%\">{figure}{copy}</div>"
                ),
                Placement::Right => format!(
                    "<div class=\"blk blk-fig side\" style=\"--pic:{picture_percent}%\">{copy}{figure}</div>"
                ),
            }
        }
        // The price table is the page's own; it is placed by the page.
        Block::Pricing => String::new(),
        Block::Table { columns, rows } => {
            let mut out = format!(
                "<table class=\"blk blk-table\"><thead><tr style=\"background:{}\">",
                esc(&colors.table_header)
            );
            for column in columns {
                out.push_str(&format!("<th>{}</th>", sanitize_inline(column)));
            }
            out.push_str("</tr></thead><tbody>");
            for row in rows {
                out.push_str("<tr>");
                for cell in row {
                    out.push_str(&format!("<td class=\"rich\">{}</td>", sanitize_rich(cell)));
                }
                out.push_str("</tr>");
            }
            out.push_str("</tbody></table>");
            out
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::quote_design::QuoteDesign;
    use serde_json::json;

    fn render(value: serde_json::Value) -> String {
        let design = QuoteDesign::parse(&value);
        let (before, after) = design.around_pricing();
        format!(
            "{}|{}",
            render_blocks(before, &design.colors),
            render_blocks(after, &design.colors)
        )
    }

    #[test]
    fn every_block_kind_prints_in_order_around_the_price_table() {
        let html = render(json!({
            "blocks": [
                { "kind": "heading", "level": 1, "text": "Our <em>offer</em>" },
                { "kind": "paragraph", "text": "<p>Two &amp; two</p>", "columns": 2 },
                { "kind": "pricing" },
                { "kind": "list", "ordered": true, "style": "outline", "items": "Design\n\tAPI\nBuild" },
                { "kind": "quote", "text": "<p>Well said</p>", "attribution": "A client" },
                { "kind": "divider", "thickness": "bold", "style": "dashed", "width": 50 },
                { "kind": "table", "columns": [{ "id": "a", "label": "Item" }], "rows": [{ "id": "r", "cells": { "a": "Bolt" } }] },
                { "kind": "text", "heading": "Terms", "body": "Net 30" }
            ],
            "colors": { "accent": "#123456", "numberMarker": "#abcdef", "tableHeader": "#eeeeee" }
        }));
        let (before, after) = html.split_once('|').unwrap();
        assert!(before.contains("<h2 class=\"blk blk-h1\">Our <em>offer</em></h2>"));
        assert!(before.contains("class=\"blk rich cols-2\"><p>Two &amp; two</p>"));
        assert!(!before.contains("1.1."), "the list is after the table");
        assert!(after.contains("<li class=\"lvl0\"><span class=\"mk\" style=\"color:#abcdef\">1.</span><span>Design</span></li>"));
        assert!(
            after.contains(
                "<li class=\"lvl1\"><span class=\"mk\" style=\"color:#abcdef\">1.1.</span>"
            )
        );
        assert!(
            after
                .contains("<span class=\"mk\" style=\"color:#abcdef\">2.</span><span>Build</span>")
        );
        assert!(
            after.contains(
                "<blockquote class=\"blk blk-quote\" style=\"border-left-color:#123456\">"
            )
        );
        assert!(after.contains("<footer>A client</footer>"));
        assert!(after.contains(
            "<hr class=\"blk blk-hr hr-bold hr-dashed hr-50\" style=\"border-top-color:#123456\">"
        ));
        assert!(after.contains("<tr style=\"background:#eeeeee\"><th>Item</th></tr>"));
        assert!(after.contains("<td class=\"rich\">Bolt</td>"));
        assert!(after.contains("<h3>Terms</h3><div class=\"rich body\">Net 30</div>"));
    }

    #[test]
    fn nothing_stored_can_become_markup_the_page_did_not_write() {
        let html = render(json!({
            "blocks": [
                { "kind": "heading", "text": "<script>x()</script><b onclick=\"y()\">Bold</b>" },
                { "kind": "image", "src": "data:image/png;base64,AAAA\" onerror=\"z()", "caption": "<img src=x onerror=w()>cap" },
                { "kind": "image", "src": "data:image/jpeg;base64,/9j/", "caption": "Photo", "body": "<p>Beside</p>", "placement": "right", "columnRatio": "40-60", "aspect": "square", "fit": "contain" }
            ]
        }));
        assert!(!html.contains("<script"));
        assert!(!html.contains("onclick"));
        assert!(!html.contains("onerror"));
        assert!(html.contains("<b>Bold</b>"));
        // The first picture's source failed validation, so there is no <img>
        // for it — only its caption, stripped to text.
        assert_eq!(html.matches("<img ").count(), 1);
        assert!(html.contains("<figcaption class=\"rich\">cap</figcaption>"));
        // The second is placed right at 40 %, in a square, contained.
        assert!(html.contains("class=\"blk blk-fig side\" style=\"--pic:40%\"><div class=\"copy rich\"><p>Beside</p></div><figure"));
        assert!(html.contains("<div class=\"frame square contain\"><img src=\"data:image/jpeg;base64,/9j/\" alt=\"Photo\">"));
    }

    #[test]
    fn a_design_with_no_content_adds_nothing_to_the_page() {
        assert_eq!(render(json!({ "blocks": [{ "kind": "pricing" }] })), "|");
        assert_eq!(render(json!({})), "|");
    }
}
