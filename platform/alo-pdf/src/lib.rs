//! # alo-pdf — a minimal PDF 1.7 writer
//!
//! Enough of the format to produce a **business document**: pages of text,
//! rules, filled and stroked boxes, in the fourteen fonts every PDF reader is
//! required to have, and JPEG pictures placed as they are ([`image`]) — a
//! quotation carries a product photo. No transparency, no forms, no
//! scripting — none of which belongs on an invoice.
//!
//! It is in `platform/` because a PDF is not a billing concept. Billing is the
//! first caller (`alo-jmap/src/billing_pdf.rs`, wave B1), and Drive exports and
//! Docs are the next ones.
//!
//! ## What it is not
//!
//! **It is not an HTML renderer.** `docs/design/billing.md` (B1.17) records the
//! decision: the printed page and the PDF are two renderers over one document
//! model, so there is nothing to parse back and nothing that can drift. A
//! caller that wants CSS wants a browser, and a browser is an engine we
//! deliberately do not run on the invoice path.
//!
//! ## Coordinates
//!
//! **Points, from the top-left corner, y downwards** — the direction a layout
//! is written in. PDF's own space is bottom-left and y-up; the flip happens
//! once, in [`Canvas`], so no caller ever computes `height - y`. One point is
//! 1/72 inch; [`mm`] converts from the unit A4 is defined in.
//!
//! ## Text
//!
//! Every string is encoded to **WinAnsi** (cp1252) because that is what the
//! standard-14 fonts can address. Characters outside it are folded to their
//! base Latin form rather than dropped ([`encoding`]). A font file — which
//! removes the limit entirely, and which PDF/A-3 requires — is a licensed
//! binary and a human's choice, recorded as open in `docs/design/billing.md`.
//!
//! ## Attachments
//!
//! A document can carry files inside it ([`attachment`]): the bytes, the name
//! a receiving system looks them up by, and the relationship that says what
//! they are. Factur-X (B1.22) is the first caller — one document that is both
//! the page a human reads and the invoice a bookkeeping system imports.
//!
//! ```
//! use alo_pdf::{Align, Color, Canvas, Font, Pdf, PdfDate, TextStyle, mm};
//!
//! let mut page = Canvas::a4();
//! page.text(
//!     mm(15.0),
//!     mm(16.0),
//!     Align::Left,
//!     &TextStyle::new(Font::Bold, 17.0),
//!     "Invoice INV-2026-00001",
//! );
//! let mut pdf = Pdf::new("Invoice INV-2026-00001", PdfDate::new(2026, 8, 7, 9, 30, 0));
//! pdf.add_page(page);
//! let bytes = pdf.finish();
//! assert!(bytes.starts_with(b"%PDF-1.7"));
//! # let _ = (Color::BLACK, mm(1.0));
//! ```

pub mod attachment;
pub mod canvas;
pub mod color;
pub mod encoding;
pub mod font;
pub mod image;
pub mod metrics;
pub mod text;
pub mod writer;

pub use attachment::{Attachment, Relationship};
pub use canvas::{Canvas, Rect};
pub use color::Color;
pub use font::Font;
pub use image::{ImageError, ImageFit, JpegImage};
pub use text::{Align, TextStyle};
pub use writer::{Pdf, PdfDate};

/// Millimetres as PDF points (1 pt = 1/72 in, 1 in = 25.4 mm).
///
/// A4 and every margin in a document specification is stated in millimetres;
/// PDF only speaks points. Converting at the edge keeps the layout readable
/// against the paper it describes.
#[must_use]
pub fn mm(value: f64) -> f64 {
    value * 72.0 / 25.4
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_millimetre_is_the_same_millimetre_a_ruler_measures() {
        // 210 mm — the width of A4 — is the 595.28 pt every PDF reader shows.
        assert!((mm(210.0) - 595.2756).abs() < 0.001);
        assert!((mm(297.0) - 841.8898).abs() < 0.001);
        assert!((mm(25.4) - 72.0).abs() < 1e-9);
        assert!((mm(0.0)).abs() < 1e-9);
    }
}
