//! The two faces a business document is set in, and how wide text is in them.
//!
//! Deliberately two, not a family: a document that needs italic, condensed and
//! three weights is a design, and an invoice is a form. Regular carries
//! everything; bold carries the handful of things that must not be misread —
//! the title, the column labels, and the total.
//!
//! Both are **standard-14** fonts, present in every conforming PDF reader, so
//! nothing is embedded and the file stays a few kilobytes. The cost of that
//! choice is the character repertoire ([`crate::encoding`]).

use crate::encoding;
use crate::metrics::{HELVETICA, HELVETICA_BOLD, Metrics};

/// One of the two faces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Font {
    /// Helvetica — body text, addresses, lines, amounts.
    #[default]
    Regular,
    /// Helvetica-Bold — the title, the column headings, the total.
    Bold,
}

impl Font {
    /// The character metrics of this face.
    #[must_use]
    pub fn metrics(self) -> &'static Metrics {
        match self {
            Self::Regular => &HELVETICA,
            Self::Bold => &HELVETICA_BOLD,
        }
    }

    /// The name this face is referenced by inside a content stream, and
    /// declared under in every page's `/Resources`.
    #[must_use]
    pub fn resource(self) -> &'static str {
        match self {
            Self::Regular => "F1",
            Self::Bold => "F2",
        }
    }

    /// Every face a document can use, in the order they are written into the
    /// file — so a resource name always means the same face.
    pub const ALL: [Self; 2] = [Self::Regular, Self::Bold];

    /// The width of `text` at `size` points, as it will actually be drawn:
    /// measured on the **encoded** bytes, so a folded character
    /// ([`encoding::fold`]) is measured as the glyph that reaches the page and
    /// not as the one that was asked for.
    #[must_use]
    pub fn text_width(self, text: &str, size: f64) -> f64 {
        let metrics = self.metrics();
        let units: u32 = encoding::encode(text)
            .into_iter()
            .map(|byte| u32::from(metrics.width(byte)))
            .sum();
        f64::from(units) * size / 1000.0
    }

    /// How many glyphs `text` becomes on the page — the multiplier for
    /// character spacing, which PDF applies once per glyph.
    #[must_use]
    pub fn glyph_count(self, text: &str) -> usize {
        encoding::encode(text).len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_measured_in_the_points_it_will_occupy() {
        // "1234" is four digits at 556/1000 em, at 10 pt: 22.24 pt.
        assert!((Font::Regular.text_width("1234", 10.0) - 22.24).abs() < 1e-9);
        // Bold is wider for the same words, which is why the two are measured
        // separately rather than one standing in for the other.
        assert!(Font::Bold.text_width("Total", 10.0) > Font::Regular.text_width("Total", 10.0));
        // Size scales linearly, and nothing measures negative.
        let at_10 = Font::Regular.text_width("Invoice", 10.0);
        assert!((Font::Regular.text_width("Invoice", 20.0) - at_10 * 2.0).abs() < 1e-9);
        assert!(Font::Regular.text_width("", 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn a_folded_character_is_measured_as_the_glyph_that_reaches_the_page() {
        // 'Ł' is printed as 'L', so it must measure as 'L' — otherwise every
        // Polish name would push the column beside it out of line.
        assert!(
            (Font::Regular.text_width("Łukasz", 9.5) - Font::Regular.text_width("Lukasz", 9.5))
                .abs()
                < 1e-9
        );
        // …and a character cp1252 does have is measured as itself.
        assert!(Font::Regular.text_width("Söhne", 9.5) > 0.0);
        assert_eq!(Font::Regular.glyph_count("Łukasz"), 6);
        assert_eq!(Font::Regular.glyph_count("Ĳ"), 2, "one char, two glyphs");
    }

    #[test]
    fn the_two_faces_keep_their_own_resource_names() {
        assert_eq!(Font::Regular.resource(), "F1");
        assert_eq!(Font::Bold.resource(), "F2");
        assert_eq!(Font::Regular.metrics().base_font, "Helvetica");
        assert_eq!(Font::Bold.metrics().base_font, "Helvetica-Bold");
        assert_eq!(Font::ALL.len(), 2);
        assert_eq!(Font::default(), Font::Regular);
    }
}
