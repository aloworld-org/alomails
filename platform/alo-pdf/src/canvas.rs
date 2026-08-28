//! One page, and the marks on it.
//!
//! A canvas accumulates a PDF **content stream**: the operators that draw text
//! and rules. It is deliberately small — text, straight lines, filled and
//! stroked boxes — because that is the whole vocabulary of a business
//! document, and every operator this crate cannot emit is one that cannot go
//! wrong on an invoice.
//!
//! ## The flip
//!
//! Callers work in **points from the top-left corner, y downwards**, the
//! direction a layout is written and read in. PDF's own space is bottom-left
//! and y-upwards. The conversion happens here, once ([`Canvas::flip`]), so no
//! layout ever computes `height - y` and no page ends up mirrored because one
//! call forgot to.

use crate::color::Color;
use crate::encoding;
use crate::image::{ImageFit, JpegImage};
use crate::text::{Align, TextStyle};

/// A4 portrait, the paper every European invoice is written on.
const A4_WIDTH_MM: f64 = 210.0;
/// A4 portrait height.
const A4_HEIGHT_MM: f64 = 297.0;

/// One page and the operators drawn on it.
pub struct Canvas {
    /// Page width in points.
    width: f64,
    /// Page height in points.
    height: f64,
    /// The content stream so far.
    ops: String,
    /// The pictures placed on this page, in the order the stream names them
    /// (`/Im1`, `/Im2`, …).
    images: Vec<JpegImage>,
}

impl Canvas {
    /// A blank page of the given size in points.
    #[must_use]
    pub fn new(width: f64, height: f64) -> Self {
        Self {
            width,
            height,
            ops: String::new(),
            images: Vec::new(),
        }
    }

    /// A blank A4 portrait page — 595.28 × 841.89 pt.
    #[must_use]
    pub fn a4() -> Self {
        Self::new(crate::mm(A4_WIDTH_MM), crate::mm(A4_HEIGHT_MM))
    }

    /// The page width in points.
    #[must_use]
    pub fn width(&self) -> f64 {
        self.width
    }

    /// The page height in points.
    #[must_use]
    pub fn height(&self) -> f64 {
        self.height
    }

    /// A caller's downward `y` as PDF's upward one.
    fn flip(&self, y: f64) -> f64 {
        self.height - y
    }

    /// Draws one line of text.
    ///
    /// `y` is the **top** of the line, not its baseline: a layout advances
    /// down a page in line heights, and asking it to also know where a
    /// baseline sits inside each one is how blocks end up a few points out of
    /// alignment with the rules drawn beside them. The baseline is placed at
    /// the face's own ascent below `y`.
    ///
    /// `x` names the left, right or centre of the run, per `align` — a column
    /// of amounts is drawn by naming its right edge, never by measuring at the
    /// call site.
    pub fn text(&mut self, x: f64, y: f64, align: Align, style: &TextStyle, text: &str) {
        if text.is_empty() {
            return;
        }
        let ascent = f64::from(style.font.metrics().ascent) / 1000.0;
        let baseline = self.flip(y + ascent * style.size);
        let origin = style.origin(x, align, text);
        self.ops.push_str(&format!(
            "BT\n/{} {} Tf\n{} rg\n{} Tc\n1 0 0 1 {} {} Tm\n{} Tj\nET\n",
            style.font.resource(),
            num(style.size),
            style.color.operands(),
            num(style.char_spacing),
            num(origin),
            num(baseline),
            encoding::pdf_string(text),
        ));
    }

    /// Draws a straight line of `thickness` points between two points.
    pub fn line(&mut self, from: (f64, f64), to: (f64, f64), thickness: f64, color: Color) {
        self.ops.push_str(&format!(
            "{} RG\n{} w\n{} {} m\n{} {} l\nS\n",
            color.operands(),
            num(thickness),
            num(from.0),
            num(self.flip(from.1)),
            num(to.0),
            num(self.flip(to.1)),
        ));
    }

    /// Draws a horizontal rule from `x` for `width` points — the border under
    /// a table heading, the line above a total.
    pub fn rule(&mut self, x: f64, y: f64, width: f64, thickness: f64, color: Color) {
        self.line((x, y), (x + width, y), thickness, color);
    }

    /// Fills a box, optionally with rounded corners (`radius` in points, `0.0`
    /// for square).
    pub fn box_filled(&mut self, rect: Rect, radius: f64, color: Color) {
        let path = self.rect_path(rect, radius);
        self.ops
            .push_str(&format!("{} rg\n{path}f\n", color.operands()));
    }

    /// Strokes the outline of a box.
    pub fn box_stroked(&mut self, rect: Rect, radius: f64, thickness: f64, color: Color) {
        let path = self.rect_path(rect, radius);
        self.ops.push_str(&format!(
            "{} RG\n{} w\n{path}S\n",
            color.operands(),
            num(thickness),
        ));
    }

    /// The path operators for a box, in PDF space.
    ///
    /// A radius of zero is the single `re` operator; a rounded box is four
    /// straight sides and four Bézier corners, with the usual circle-from-
    /// cubics constant (`4/3·(√2−1)`).
    fn rect_path(&self, rect: Rect, radius: f64) -> String {
        let (left, right) = (rect.x, rect.x + rect.width);
        let (top, bottom) = (self.flip(rect.y), self.flip(rect.y + rect.height));
        if radius <= 0.0 {
            return format!(
                "{} {} {} {} re\n",
                num(left),
                num(bottom),
                num(rect.width),
                num(rect.height)
            );
        }
        let r = radius.min(rect.width / 2.0).min(rect.height / 2.0);
        let k = r * 0.552_284_749_83;
        let mut path = String::new();
        path.push_str(&format!("{} {} m\n", num(left + r), num(bottom)));
        path.push_str(&format!("{} {} l\n", num(right - r), num(bottom)));
        path.push_str(&curve(
            (right - r + k, bottom),
            (right, bottom + r - k),
            (right, bottom + r),
        ));
        path.push_str(&format!("{} {} l\n", num(right), num(top - r)));
        path.push_str(&curve(
            (right, top - r + k),
            (right - r + k, top),
            (right - r, top),
        ));
        path.push_str(&format!("{} {} l\n", num(left + r), num(top)));
        path.push_str(&curve(
            (left + r - k, top),
            (left, top - r + k),
            (left, top - r),
        ));
        path.push_str(&format!("{} {} l\n", num(left), num(bottom + r)));
        path.push_str(&curve(
            (left, bottom + r - k),
            (left + r - k, bottom),
            (left + r, bottom),
        ));
        path.push_str("h\n");
        path
    }

    /// Places a picture in `frame`, scaled per `fit` and centred.
    ///
    /// The frame is also the clip: with [`ImageFit::Cover`] the picture is
    /// scaled up until it fills the frame and the overflow is cut off at the
    /// frame's edges, never drawn over the text beside it.
    pub fn image(&mut self, image: &JpegImage, frame: Rect, fit: ImageFit) {
        if frame.width <= 0.0 || frame.height <= 0.0 {
            return;
        }
        let aspect = image.aspect();
        let wider_than_frame = aspect >= frame.width / frame.height;
        let (width, height) = match (fit, wider_than_frame) {
            (ImageFit::Contain, true) | (ImageFit::Cover, false) => {
                (frame.width, frame.width / aspect)
            }
            (ImageFit::Contain, false) | (ImageFit::Cover, true) => {
                (frame.height * aspect, frame.height)
            }
        };
        let x = frame.x + (frame.width - width) / 2.0;
        let y = frame.y + (frame.height - height) / 2.0;
        self.images.push(image.clone());
        let name = self.images.len();
        // `q … Q` keeps the clip and the transform to this one picture.
        self.ops.push_str(&format!(
            "q\n{} {} {} {} re W n\n{} 0 0 {} {} {} cm\n/Im{name} Do\nQ\n",
            num(frame.x),
            num(self.flip(frame.y + frame.height)),
            num(frame.width),
            num(frame.height),
            num(width),
            num(height),
            num(x),
            num(self.flip(y + height)),
        ));
    }

    /// The content stream this canvas has accumulated — the operators as
    /// they will be written, which is what a caller's test reads.
    #[must_use]
    pub fn content(&self) -> &str {
        &self.ops
    }

    /// The pictures the stream names, `/Im1` first.
    #[must_use]
    pub fn images(&self) -> &[JpegImage] {
        &self.images
    }
}

/// A box in caller space: top-left corner, width and height, all in points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    /// Left edge.
    pub x: f64,
    /// Top edge.
    pub y: f64,
    /// Width.
    pub width: f64,
    /// Height.
    pub height: f64,
}

/// One cubic Bézier segment.
fn curve(c1: (f64, f64), c2: (f64, f64), to: (f64, f64)) -> String {
    format!(
        "{} {} {} {} {} {} c\n",
        num(c1.0),
        num(c1.1),
        num(c2.0),
        num(c2.1),
        num(to.0),
        num(to.1)
    )
}

/// A number as a PDF operand.
///
/// Three decimals — finer than any output device resolves — and never
/// exponential notation, which PDF does not accept. `-0` is normalised to `0`
/// because a negative zero in a coordinate is noise in every diff of a golden
/// file.
fn num(value: f64) -> String {
    let rounded = (value * 1000.0).round() / 1000.0;
    let rounded = if rounded == 0.0 { 0.0 } else { rounded };
    let text = format!("{rounded:.3}");
    let trimmed = text.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() { "0" } else { trimmed }.to_owned()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::font::Font;
    use crate::mm;

    #[test]
    fn a_page_is_a4_in_points() {
        let page = Canvas::a4();
        assert!((page.width() - 595.276).abs() < 0.01);
        assert!((page.height() - 841.89).abs() < 0.01);
        assert!(page.content().is_empty(), "a blank page draws nothing");
    }

    #[test]
    fn the_origin_is_the_top_left_corner() {
        let mut page = Canvas::a4();
        // A rule 16 mm from the top must sit 16 mm from the top, which in
        // PDF's own upward space is (297 − 16) mm.
        page.rule(0.0, mm(16.0), mm(10.0), 0.5, Color::BLACK);
        let expected = num(mm(297.0 - 16.0));
        assert!(
            page.content().contains(&format!("0 {expected} m")),
            "content was {}",
            page.content()
        );
    }

    #[test]
    fn text_is_placed_on_a_baseline_below_the_line_it_was_given() {
        let mut page = Canvas::new(200.0, 100.0);
        let style = TextStyle::new(Font::Regular, 10.0);
        page.text(20.0, 30.0, Align::Left, &style, "Invoice");
        // Baseline = 30 + 7.18 below the top, i.e. 100 − 37.18 up from the
        // bottom. The x is untouched for a left-aligned run.
        assert!(page.content().contains("1 0 0 1 20 62.82 Tm"));
        assert!(page.content().contains("/F1 10 Tf"));
        assert!(page.content().contains("(Invoice) Tj"));
    }

    #[test]
    fn a_right_aligned_run_ends_at_the_x_it_names() {
        let mut page = Canvas::new(200.0, 100.0);
        let style = TextStyle::new(Font::Regular, 10.0);
        page.text(180.0, 10.0, Align::Right, &style, "1 234.56");
        let start = 180.0 - style.width_of("1 234.56");
        assert!(
            page.content().contains(&format!("1 0 0 1 {} ", num(start))),
            "content was {}",
            page.content()
        );
    }

    #[test]
    fn nothing_a_customer_typed_can_become_pdf_structure() {
        // The one place customer data meets the file format. A name carrying
        // the string delimiters must not close the string it is written into.
        let mut page = Canvas::new(200.0, 100.0);
        let style = TextStyle::new(Font::Regular, 10.0);
        page.text(0.0, 0.0, Align::Left, &style, "Acme (Holdings) \\ Ltd");
        assert!(page.content().contains("(Acme \\(Holdings\\) \\\\ Ltd) Tj"));
        // …and a non-ASCII byte is octal, never raw.
        let mut page = Canvas::new(200.0, 100.0);
        page.text(0.0, 0.0, Align::Left, &style, "Söhne");
        assert!(page.content().contains("(S\\366hne) Tj"));
        assert!(page.content().is_ascii(), "the stream must stay 7-bit");
    }

    #[test]
    fn an_empty_run_draws_nothing_at_all() {
        let mut page = Canvas::new(200.0, 100.0);
        page.text(0.0, 0.0, Align::Left, &TextStyle::new(Font::Bold, 9.0), "");
        assert!(page.content().is_empty());
    }

    #[test]
    fn a_square_box_is_one_operator_and_a_rounded_one_is_a_closed_path() {
        let mut page = Canvas::new(200.0, 100.0);
        let rect = Rect {
            x: 10.0,
            y: 20.0,
            width: 30.0,
            height: 40.0,
        };
        page.box_filled(rect, 0.0, Color::BLACK);
        assert!(page.content().contains("10 40 30 40 re"));
        assert!(page.content().contains("f\n"));

        let mut page = Canvas::new(200.0, 100.0);
        page.box_stroked(rect, 5.0, 1.0, Color::WHITE);
        let content = page.content();
        assert_eq!(content.matches(" c\n").count(), 4, "four rounded corners");
        assert!(content.contains("h\n") && content.contains("S\n"));
        // The radius is clamped to what the box can hold, so an over-large
        // radius bulges rather than inverting the path.
        let mut page = Canvas::new(200.0, 100.0);
        page.box_filled(rect, 1000.0, Color::BLACK);
        assert_eq!(page.content().matches(" c\n").count(), 4);
    }

    #[test]
    fn a_picture_is_scaled_into_its_frame_and_clipped_to_it() {
        let image = JpegImage::new(crate::image::tests::skeleton(400, 200, 3)).unwrap();
        let frame = Rect {
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 100.0,
        };
        // Contain: a 2:1 picture in a square frame is 100 wide, 50 tall,
        // centred 25 down from the frame's top.
        let mut page = Canvas::new(200.0, 300.0);
        page.image(&image, frame, ImageFit::Contain);
        let content = page.content();
        assert!(content.contains("/Im1 Do"), "content was {content}");
        assert!(
            content.contains("100 0 0 50 10 "),
            "contain scales to width: {content}"
        );
        // The clip is the frame in PDF space: bottom-left (10, 300−120).
        assert!(content.contains("10 180 100 100 re W n"), "clip: {content}");
        assert_eq!(page.images().len(), 1);

        // Cover: the same picture fills the height (100 tall → 200 wide) and
        // is centred, so it starts 50 left of the frame; the clip trims it.
        let mut page = Canvas::new(200.0, 300.0);
        page.image(&image, frame, ImageFit::Cover);
        assert!(
            page.content().contains("200 0 0 100 -40 "),
            "cover: {}",
            page.content()
        );

        // A second picture is /Im2.
        page.image(&image, frame, ImageFit::Cover);
        assert!(page.content().contains("/Im2 Do"));
        assert_eq!(page.images().len(), 2);
    }

    #[test]
    fn numbers_are_written_the_way_pdf_can_read_them() {
        assert_eq!(num(0.0), "0");
        assert_eq!(num(-0.0), "0");
        assert_eq!(num(12.0), "12");
        assert_eq!(num(12.3456), "12.346");
        assert_eq!(num(-3.5), "-3.5");
        // Never exponential, whatever the magnitude.
        assert!(!num(0.000_001).contains('e'));
        assert!(!num(1e12).contains('e'));
    }
}
