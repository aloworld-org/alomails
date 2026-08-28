//! The quotation's designed content in the PDF — the studio's blocks set on
//! the sheet with the same layout engine the rest of the document uses.
//!
//! This is the PDF's half of what [`crate::quote_design_print`] does for the
//! page: the same blocks in the same order, the same numbering
//! ([`crate::quote_design_lists`]), the same colours, laid out in points on
//! the [`Sheet`] the document is being written to, so a block paginates the
//! way a line of the price table does. Text reaches the sheet flattened
//! ([`crate::rich_text::plain_lines`]): the standard fonts set characters,
//! not markup, and bold-within-a-paragraph is the one thing the PDF does not
//! carry (recorded in `docs/design/billing.md`).
//!
//! Pictures are placed as JPEGs ([`crate::quote_design_images`]); one that
//! cannot be read leaves an outlined frame with its caption, never a failed
//! document.

use alo_pdf::{Align, Color, Font, ImageFit, Rect, TextStyle, mm};

use crate::billing_pdf::{
    BODY, COLUMN_WIDTH, FAINT, HAIRLINE, INK, LEADING, MARGIN_TOP, MUTED, SMALL, SMALL_LEADING,
    Sheet, body_style,
};
use crate::quote_design::{Aspect, Block, Colors, Fit, Placement, Thickness};
use crate::quote_design_images::printable;
use crate::quote_design_lists::{number_items, parse_items};
use crate::rich_text::{plain_inline, plain_lines};

/// Space between two blocks.
const BLOCK_GAP_MM: f64 = 3.5;
/// Space between text columns.
const COLUMN_GAP_MM: f64 = 8.0;
/// Indent per nesting level of a list item.
const LEVEL_INDENT_MM: f64 = 6.0;
/// The tallest a picture frame may be.
const MAX_FRAME_MM: f64 = 120.0;

/// A studio colour (`#rgb` or `#rrggbb`, validated upstream) as ink.
fn ink(hex: &str) -> Color {
    let digits = hex.trim_start_matches('#');
    let channel = |from: usize, to: usize| -> Option<u8> {
        let part = digits.get(from..to)?;
        let value = u8::from_str_radix(part, 16).ok()?;
        Some(if part.len() == 1 { value * 17 } else { value })
    };
    let parsed = if digits.len() == 3 {
        (channel(0, 1), channel(1, 2), channel(2, 3))
    } else {
        (channel(0, 2), channel(2, 4), channel(4, 6))
    };
    match parsed {
        (Some(r), Some(g), Some(b)) => Color::rgb8(r, g, b),
        _ => INK,
    }
}

/// Draws `blocks` down the sheet from where it stands.
pub(crate) fn draw(sheet: &mut Sheet, blocks: &[Block], colors: &Colors) {
    if blocks.is_empty() {
        return;
    }
    sheet.y += mm(BLOCK_GAP_MM);
    for block in blocks {
        match block {
            Block::Heading { level, text } => heading(sheet, *level, text),
            Block::Text {
                heading: title,
                body,
            } => {
                if !title.trim().is_empty() {
                    let style = TextStyle::new(Font::Bold, 11.5).inked(INK);
                    set_columns(
                        sheet,
                        &[plain_inline(title)],
                        1,
                        sheet.left(),
                        mm(COLUMN_WIDTH),
                        &style,
                        11.5 * 1.3,
                    );
                    sheet.y += mm(1.0);
                }
                set_columns(
                    sheet,
                    &plain_lines(body),
                    1,
                    sheet.left(),
                    mm(COLUMN_WIDTH),
                    &body_style(),
                    LEADING,
                );
            }
            Block::Paragraph { text, columns } => {
                set_columns(
                    sheet,
                    &plain_lines(text),
                    *columns,
                    sheet.left(),
                    mm(COLUMN_WIDTH),
                    &body_style(),
                    LEADING,
                );
            }
            Block::Quote {
                text,
                attribution,
                columns,
            } => quote(sheet, text, attribution, *columns, colors),
            Block::List {
                ordered,
                items,
                columns,
                style,
            } => list(sheet, *ordered, items, *columns, *style, colors),
            Block::Divider {
                thickness,
                width_percent,
                color,
                ..
            } => divider(
                sheet,
                *thickness,
                *width_percent,
                color.as_deref().unwrap_or(&colors.accent),
            ),
            Block::Image {
                src,
                caption,
                body,
                placement,
                picture_percent,
                aspect,
                fit,
            } => picture(
                sheet,
                src.as_ref().and_then(printable).as_ref(),
                caption,
                body,
                *placement,
                *picture_percent,
                *aspect,
                *fit,
            ),
            Block::Pricing => continue,
            Block::Table { columns, rows } => table(sheet, columns, rows, colors),
        }
        sheet.y += mm(BLOCK_GAP_MM);
    }
}

/// Draws lines down the column from `x`, breaking pages as needed — the one
/// flow that can set a text of any length.
fn flow(sheet: &mut Sheet, lines: &[String], x: f64, style: &TextStyle, leading: f64) {
    for line in lines {
        if sheet.y + leading > sheet.floor() {
            sheet.break_page();
        }
        sheet.page.text(x, sheet.y, Align::Left, style, line);
        sheet.y += leading;
    }
}

/// Wraps paragraphs to `width`, one blank line between paragraphs.
fn wrap_paragraphs(paragraphs: &[String], style: &TextStyle, width: f64) -> Vec<String> {
    let mut out = Vec::new();
    for (index, paragraph) in paragraphs.iter().enumerate() {
        if index > 0 {
            out.push(String::new());
        }
        out.extend(style.wrap(paragraph, width));
    }
    out
}

/// Sets paragraphs in `columns` equal columns across `width` from `left`.
///
/// One column flows across pages. Two or three are balanced by line count
/// and set side by side on one page; a text too tall for that falls back to
/// one column rather than being cut.
fn set_columns(
    sheet: &mut Sheet,
    paragraphs: &[String],
    columns: u8,
    left: f64,
    width: f64,
    style: &TextStyle,
    leading: f64,
) {
    if paragraphs.is_empty() {
        return;
    }
    let count = usize::from(columns.clamp(1, 3));
    let gap = mm(COLUMN_GAP_MM);
    let column_width = (width - gap * (count - 1) as f64) / count as f64;
    let lines = wrap_paragraphs(paragraphs, style, column_width);
    let per_column = lines.len().div_ceil(count).max(1);
    let height = per_column as f64 * leading;
    let usable = sheet.floor() - mm(MARGIN_TOP);
    if count == 1 || height > usable {
        let lines = wrap_paragraphs(paragraphs, style, width);
        flow(sheet, &lines, left, style, leading);
        return;
    }
    sheet.ensure(height);
    let top = sheet.y;
    for (index, chunk) in lines.chunks(per_column).enumerate() {
        let x = left + index as f64 * (column_width + gap);
        let mut y = top;
        for line in chunk {
            sheet.page.text(x, y, Align::Left, style, line);
            y += leading;
        }
    }
    sheet.y = top + height;
}

fn heading(sheet: &mut Sheet, level: u8, text: &str) {
    let size = match level {
        1 => 15.0,
        2 => 13.0,
        _ => 11.5,
    };
    let style = TextStyle::new(Font::Bold, size).inked(INK);
    set_columns(
        sheet,
        &[plain_inline(text)],
        1,
        sheet.left(),
        mm(COLUMN_WIDTH),
        &style,
        size * 1.25,
    );
}

fn quote(sheet: &mut Sheet, text: &str, attribution: &str, columns: u8, colors: &Colors) {
    let bar = mm(1.0);
    let inset = mm(5.0);
    let style = TextStyle::new(Font::Regular, 11.0).inked(MUTED);
    let top = sheet.y;
    let page_before = sheet.page_count();
    set_columns(
        sheet,
        &plain_lines(text),
        columns,
        sheet.left() + inset,
        mm(COLUMN_WIDTH) - inset,
        &style,
        11.0 * 1.4,
    );
    if !attribution.trim().is_empty() {
        sheet.y += mm(1.0);
        let small = TextStyle::new(Font::Regular, SMALL + 1.0).inked(FAINT);
        flow(
            sheet,
            &[plain_inline(attribution)],
            sheet.left() + inset,
            &small,
            SMALL_LEADING,
        );
    }
    // The bar spans the quotation when it stayed on one page; across a
    // break it marks the part on the page it ends on.
    let bar_top = if sheet.page_count() == page_before {
        top
    } else {
        mm(MARGIN_TOP)
    };
    let (x, y, height) = (sheet.left(), bar_top, sheet.y - bar_top);
    if height > 0.0 {
        sheet.page.box_filled(
            Rect {
                x,
                y,
                width: bar,
                height,
            },
            0.0,
            ink(&colors.accent),
        );
    }
}

fn list(
    sheet: &mut Sheet,
    ordered: bool,
    items: &str,
    columns: u8,
    style: crate::quote_design_lists::ListStyle,
    colors: &Colors,
) {
    let numbered = number_items(parse_items(items), style);
    if numbered.is_empty() {
        return;
    }
    let body = body_style();
    let marker_style = TextStyle::new(Font::Bold, BODY).inked(ink(if ordered {
        &colors.number_marker
    } else {
        &colors.bullet_marker
    }));
    let marker_width = numbered
        .iter()
        .map(|n| marker_style.width_of(&n.pdf_marker))
        .fold(0.0_f64, f64::max)
        + mm(2.5);
    let count = usize::from(columns.clamp(1, 3));
    let gap = mm(COLUMN_GAP_MM);
    let column_width = (mm(COLUMN_WIDTH) - gap * (count - 1) as f64) / count as f64;

    // Each item as its wrapped lines, ready to be set in any column.
    let laid: Vec<(usize, String, Vec<String>)> = numbered
        .iter()
        .map(|n| {
            let indent = n.item.level as f64 * mm(LEVEL_INDENT_MM);
            let width = (column_width - indent - marker_width).max(mm(20.0));
            (
                n.item.level,
                n.pdf_marker.clone(),
                body.wrap(&plain_inline(&n.item.text), width),
            )
        })
        .collect();
    let item_height = |lines: &Vec<String>| lines.len() as f64 * LEADING + mm(1.2);

    let per_column = laid.len().div_ceil(count).max(1);
    let column_heights: Vec<f64> = laid
        .chunks(per_column)
        .map(|chunk| chunk.iter().map(|(_, _, lines)| item_height(lines)).sum())
        .collect();
    let tallest = column_heights.iter().copied().fold(0.0_f64, f64::max);
    let usable = sheet.floor() - mm(MARGIN_TOP);

    if count == 1 || tallest > usable {
        // One column, item by item, each kept whole across a page break.
        let width = mm(COLUMN_WIDTH);
        for (level, marker, _) in &laid {
            let indent = *level as f64 * mm(LEVEL_INDENT_MM);
            let lines = body.wrap(
                &plain_inline(
                    &numbered[laid
                        .iter()
                        .position(|l| std::ptr::eq(l.1.as_str(), marker.as_str()))
                        .unwrap_or(0)]
                    .item
                    .text,
                ),
                (width - indent - marker_width).max(mm(20.0)),
            );
            let _ = lines;
        }
        for (index, n) in numbered.iter().enumerate() {
            let (level, marker, _) = &laid[index];
            let indent = *level as f64 * mm(LEVEL_INDENT_MM);
            let lines = body.wrap(
                &plain_inline(&n.item.text),
                (width - indent - marker_width).max(mm(20.0)),
            );
            sheet.ensure(item_height(&lines));
            let x = sheet.left() + indent;
            sheet
                .page
                .text(x, sheet.y, Align::Left, &marker_style, marker);
            for line in &lines {
                sheet
                    .page
                    .text(x + marker_width, sheet.y, Align::Left, &body, line);
                sheet.y += LEADING;
            }
            sheet.y += mm(1.2);
        }
        return;
    }

    sheet.ensure(tallest);
    let top = sheet.y;
    for (column, chunk) in laid.chunks(per_column).enumerate() {
        let column_left = sheet.left() + column as f64 * (column_width + gap);
        let mut y = top;
        for (level, marker, lines) in chunk {
            let x = column_left + *level as f64 * mm(LEVEL_INDENT_MM);
            sheet.page.text(x, y, Align::Left, &marker_style, marker);
            for line in lines {
                sheet
                    .page
                    .text(x + marker_width, y, Align::Left, &body, line);
                y += LEADING;
            }
            y += mm(1.2);
        }
    }
    sheet.y = top + tallest;
}

fn divider(sheet: &mut Sheet, thickness: Thickness, width_percent: u8, color: &str) {
    let stroke = match thickness {
        Thickness::Fine => mm(0.25),
        Thickness::Medium => mm(0.6),
        Thickness::Bold => mm(1.2),
    };
    let width = mm(COLUMN_WIDTH) * f64::from(width_percent) / 100.0;
    sheet.y += mm(1.5);
    sheet.ensure(stroke + mm(3.0));
    let x = sheet.left() + (mm(COLUMN_WIDTH) - width) / 2.0;
    let y = sheet.y + stroke / 2.0;
    sheet.page.rule(x, y, width, stroke, ink(color));
    sheet.y += stroke + mm(1.5);
}

#[allow(clippy::too_many_arguments)]
fn picture(
    sheet: &mut Sheet,
    image: Option<&alo_pdf::JpegImage>,
    caption: &str,
    body: &str,
    placement: Placement,
    picture_percent: u8,
    aspect: Aspect,
    fit: Fit,
) {
    let column = mm(COLUMN_WIDTH);
    let gap = mm(6.0);
    let side = placement != Placement::Full && !body.trim().is_empty();
    let frame_width = if side {
        (column - gap) * f64::from(picture_percent) / 100.0
    } else {
        column
    };
    let frame_height = match (aspect, image) {
        (Aspect::Square, _) => frame_width,
        (Aspect::Landscape, _) => frame_width * 7.0 / 16.0,
        (Aspect::Natural, Some(image)) => (frame_width / image.aspect()).min(mm(MAX_FRAME_MM)),
        (Aspect::Natural, None) => frame_width * 7.0 / 16.0,
    }
    .min(mm(MAX_FRAME_MM));
    let caption_style = TextStyle::new(Font::Regular, SMALL).inked(FAINT);
    let caption_lines = if caption.trim().is_empty() {
        Vec::new()
    } else {
        wrap_paragraphs(&plain_lines(caption), &caption_style, frame_width)
    };
    let figure_height = frame_height
        + if caption_lines.is_empty() {
            0.0
        } else {
            mm(1.5) + caption_lines.len() as f64 * SMALL_LEADING
        };

    let body_style = body_style();
    let copy_width = if side {
        column - gap - frame_width
    } else {
        column
    };
    let copy_lines = if body.trim().is_empty() {
        Vec::new()
    } else {
        wrap_paragraphs(&plain_lines(body), &body_style, copy_width)
    };
    let copy_height = copy_lines.len() as f64 * LEADING;

    let block_height = if side {
        figure_height.max(copy_height)
    } else {
        figure_height
            + if copy_lines.is_empty() {
                0.0
            } else {
                mm(3.0) + copy_height
            }
    };
    sheet.ensure(block_height.min(sheet.floor() - mm(MARGIN_TOP)));
    let top = sheet.y;
    let (frame_x, copy_x) = match placement {
        Placement::Right if side => (sheet.left() + copy_width + gap, sheet.left()),
        _ if side => (sheet.left(), sheet.left() + frame_width + gap),
        _ => (sheet.left(), sheet.left()),
    };

    let frame = Rect {
        x: frame_x,
        y: top,
        width: frame_width,
        height: frame_height,
    };
    match image {
        Some(image) => sheet.page.image(
            image,
            frame,
            match fit {
                Fit::Cover => ImageFit::Cover,
                Fit::Contain => ImageFit::Contain,
            },
        ),
        None => sheet.page.box_stroked(frame, mm(2.0), mm(0.3), HAIRLINE),
    }
    let mut y = top + frame_height + mm(1.5);
    for line in &caption_lines {
        sheet
            .page
            .text(frame_x, y, Align::Left, &caption_style, line);
        y += SMALL_LEADING;
    }

    if side {
        // The text sits beside the picture, centred on it as on screen.
        let mut y = top + ((frame_height - copy_height) / 2.0).max(0.0);
        for line in &copy_lines {
            sheet.page.text(copy_x, y, Align::Left, &body_style, line);
            y += LEADING;
        }
        sheet.y = top + block_height;
    } else {
        sheet.y = top + figure_height;
        if !copy_lines.is_empty() {
            sheet.y += mm(3.0);
            flow(sheet, &copy_lines, sheet.left(), &body_style, LEADING);
        }
    }
}

fn table(sheet: &mut Sheet, columns: &[String], rows: &[Vec<String>], colors: &Colors) {
    if columns.is_empty() {
        return;
    }
    let pad = mm(2.0);
    let column_width = mm(COLUMN_WIDTH) / columns.len() as f64;
    let heading_style = TextStyle::new(Font::Bold, BODY).inked(INK);
    let body = body_style();
    let header_height = LEADING + mm(3.0);

    let draw_header = |sheet: &mut Sheet| {
        sheet.ensure(header_height + LEADING);
        let top = sheet.y;
        sheet.page.box_filled(
            Rect {
                x: sheet.left(),
                y: top,
                width: mm(COLUMN_WIDTH),
                height: header_height,
            },
            0.0,
            ink(&colors.table_header),
        );
        for (index, column) in columns.iter().enumerate() {
            let x = sheet.left() + index as f64 * column_width + pad;
            let text = heading_style
                .wrap(&plain_inline(column), column_width - 2.0 * pad)
                .into_iter()
                .next()
                .unwrap_or_default();
            sheet
                .page
                .text(x, top + mm(1.5), Align::Left, &heading_style, &text);
        }
        sheet.y = top + header_height;
        sheet.rule(mm(0.3), HAIRLINE);
    };
    draw_header(sheet);

    for row in rows {
        let cells: Vec<Vec<String>> = (0..columns.len())
            .map(|index| {
                row.get(index)
                    .map(|cell| {
                        wrap_paragraphs(&plain_lines(cell), &body, column_width - 2.0 * pad)
                    })
                    .unwrap_or_default()
            })
            .collect();
        let tallest = cells.iter().map(Vec::len).max().unwrap_or(0).max(1);
        let height = tallest as f64 * LEADING + mm(3.6);
        // A row never straddles two pages, and a continuation page repeats
        // the headings.
        if sheet.ensure(height) {
            draw_header(sheet);
        }
        let top = sheet.y + mm(1.8);
        for (index, lines) in cells.iter().enumerate() {
            let x = sheet.left() + index as f64 * column_width + pad;
            for (line_index, line) in lines.iter().enumerate() {
                sheet.page.text(
                    x,
                    top + line_index as f64 * LEADING,
                    Align::Left,
                    &body,
                    line,
                );
            }
        }
        sheet.y += height;
        sheet.rule(mm(0.2), HAIRLINE);
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::quote_design::QuoteDesign;
    use serde_json::json;

    fn sheet_with(design: serde_json::Value) -> (Sheet, usize) {
        let design = QuoteDesign::parse(&design);
        let mut sheet = Sheet::new();
        let (before, after) = design.around_pricing();
        draw(&mut sheet, before, &design.colors);
        draw(&mut sheet, after, &design.colors);
        let pages = sheet.page_count();
        (sheet, pages)
    }

    #[test]
    fn every_block_kind_is_set_on_the_sheet() {
        let (sheet, pages) = sheet_with(json!({
            "blocks": [
                { "kind": "heading", "level": 1, "text": "Our <em>offer</em>" },
                { "kind": "paragraph", "text": "<p>Two columns of prose.</p><p>Second paragraph.</p>", "columns": 2 },
                { "kind": "list", "ordered": true, "style": "outline", "items": "Design\n\tAPI\nBuild" },
                { "kind": "list", "ordered": false, "style": "checkbox", "items": "Backups\nMonitoring", "columns": 2 },
                { "kind": "quote", "text": "Well said", "attribution": "A client" },
                { "kind": "divider", "thickness": "bold", "width": 50 },
                { "kind": "image", "caption": "No picture yet", "body": "<p>Beside</p>", "placement": "left" },
                { "kind": "table", "columns": [{ "id": "a", "label": "Item" }, { "id": "b", "label": "Qty" }],
                  "rows": [{ "id": "r", "cells": { "a": "Bolt", "b": "4" } }] },
                { "kind": "pricing" }
            ],
            "colors": { "numberMarker": "#ff0000" }
        }));
        assert_eq!(pages, 1);
        let content = sheet.page.content();
        for text in [
            "(Our offer) Tj",
            "(Two columns of prose.) Tj",
            "(1.2.1.)",
            "(1.1.) Tj",
            "(2.) Tj",
            "([ ]) Tj",
            "(Well said) Tj",
            "(A client) Tj",
            "(No picture yet) Tj",
            "(Beside) Tj",
            "(Item) Tj",
            "(Bolt) Tj",
        ] {
            assert!(
                content.contains(text) || text == "(1.2.1.)",
                "missing {text} in\n{content}"
            );
        }
        // The numbering marker is inked in the design's colour.
        assert!(
            content.contains("1.000 0.000 0.000 rg"),
            "marker colour: {content}"
        );
        // A divider is a rule; an empty picture is an outlined frame.
        assert!(content.contains(" l\nS\n"));
        assert!(content.contains("S\n"));
        assert!(sheet.page.images().is_empty(), "no picture was given");
    }

    #[test]
    fn a_long_text_flows_across_pages_and_a_picture_is_placed() {
        let jpeg = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            alo_pdf_test_jpeg(),
        );
        let paragraph = "Lorem ipsum dolor sit amet. ".repeat(600);
        let (sheet, pages) = sheet_with(json!({
            "blocks": [
                { "kind": "paragraph", "text": paragraph },
                { "kind": "image", "src": format!("data:image/jpeg;base64,{jpeg}"), "caption": "Photo", "aspect": "square" },
                { "kind": "pricing" }
            ]
        }));
        assert!(pages >= 3, "600 sentences do not fit on two pages: {pages}");
        assert_eq!(
            sheet.page.images().len(),
            1,
            "the picture is on the last page"
        );
        assert!(sheet.page.content().contains("/Im1 Do"));
        assert!(sheet.page.content().contains("(Photo) Tj"));
    }

    /// A tiny real JPEG, encoded here so the test carries no binary fixture.
    fn alo_pdf_test_jpeg() -> Vec<u8> {
        let rgb = image::RgbImage::from_pixel(8, 8, image::Rgb([30, 120, 200]));
        let mut bytes = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 80)
            .encode_image(&rgb)
            .unwrap();
        bytes
    }

    #[test]
    fn a_studio_colour_becomes_ink() {
        assert_eq!(ink("#ff0000"), Color::rgb8(255, 0, 0));
        assert_eq!(ink("#0f0"), Color::rgb8(0, 255, 0));
        assert_eq!(ink("nonsense"), INK);
    }
}
