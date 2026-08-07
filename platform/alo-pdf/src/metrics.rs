//! Character metrics for the standard-14 fonts we use.
//!
//! A PDF has no layout engine: a producer that wants a column of amounts to
//! line up on the right must know how wide every character is before it places
//! it. These tables are that knowledge.
//!
//! ## Where the numbers come from
//!
//! They are **extracted from a real Helvetica, not remembered.** The `hmtx`
//! advance of every WinAnsi character was read out of
//! `/System/Library/Fonts/Helvetica.ttc` (both faces) and scaled from the
//! font's 2048 units/em to the 1000 units/em PDF works in. The extraction is a
//! ~90-line stdlib TrueType parse (`head` → units/em, `hhea` → number of
//! metrics, `hmtx` → advances, `cmap` format 4 → glyph ids); it is recorded
//! here rather than shipped as a build step, because these fourteen fonts have
//! not changed since 1997 and will not.
//!
//! For the repertoire a business document prints — letters, digits, space and
//! punctuation — the extraction is identical to Adobe's published AFM metrics,
//! which is what makes it trustworthy. A handful of **symbols** differ between
//! the two (`€` 744 here vs 556 in Adobe's AFM; `±`, `÷`, `µ` similarly). The
//! tables are shipped exactly as extracted rather than hand-patched towards
//! the AFM: a table that is "the measurement, except for three values somebody
//! remembered" is the one that is wrong in a way nobody can check. Nothing on
//! an invoice is affected — amounts print the ISO code (`EUR 1 234.56`), never
//! a currency symbol, by the design note's own rule — and the widths we
//! declare in the font dictionary ([`crate::writer`]) are these same numbers,
//! so a reader is told exactly what we measured.
//!
//! Digits all share one width in both faces (556), which is what makes a
//! column of amounts align at all; [`Metrics::width`] is the only reader.

/// The lowest character code these tables describe (`space`).
pub const FIRST_CHAR: u8 = 0x20;

/// The highest character code these tables describe.
pub const LAST_CHAR: u8 = 0xFF;

/// Number of entries in a width table — every code from [`FIRST_CHAR`] to
/// [`LAST_CHAR`] inclusive.
pub const WIDTH_COUNT: usize = (LAST_CHAR as usize - FIRST_CHAR as usize) + 1;

/// Everything a PDF font dictionary and a layout need to know about one face.
///
/// Glyph-space units, 1000 per em, as PDF requires: a width of 556 at 9.5 pt
/// occupies `556 / 1000 * 9.5` points.
pub struct Metrics {
    /// The `/BaseFont` name, one of the fourteen every reader must have.
    pub base_font: &'static str,
    /// Advance width per character code, from [`FIRST_CHAR`].
    ///
    /// Six codes are unassigned in WinAnsi (`0x7F`, `0x81`, `0x8D`, `0x8F`,
    /// `0x90`, `0x9D`) and carry `0`; [`crate::encoding`] never emits them.
    pub widths: [u16; WIDTH_COUNT],
    /// `/FontBBox`, from Adobe's AFM for this face.
    pub bbox: [i32; 4],
    /// `/Ascent` — the top of `b`, `d`, `h`.
    pub ascent: i32,
    /// `/Descent` — the bottom of `p`, `q`, `y` (negative).
    pub descent: i32,
    /// `/CapHeight` — the top of a capital.
    pub cap_height: i32,
    /// `/StemV` — nominal vertical stem width, the one descriptor entry that
    /// is an estimate rather than a measurement.
    pub stem_v: i32,
}

impl Metrics {
    /// The advance width of one character code, in glyph-space units.
    ///
    /// A code below [`FIRST_CHAR`] cannot be drawn and measures zero; nothing
    /// in [`crate::encoding`] produces one.
    #[must_use]
    pub fn width(&self, code: u8) -> u16 {
        if code < FIRST_CHAR {
            return 0;
        }
        self.widths[usize::from(code - FIRST_CHAR)]
    }
}

/// Helvetica — the body of every document this crate writes.
pub static HELVETICA: Metrics = Metrics {
    base_font: "Helvetica",
    widths: [
        278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, //
        556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556, //
        1015, 667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, 722, 778, //
        667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 278, 278, 278, 469, 556, //
        333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556, //
        556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584, 0, //
        744, 0, 222, 556, 333, 1000, 556, 556, 333, 1000, 667, 333, 1000, 0, 611, 0, //
        0, 222, 222, 333, 333, 350, 556, 1000, 333, 1000, 500, 333, 944, 0, 500, 667, //
        278, 333, 556, 556, 556, 556, 260, 556, 333, 737, 370, 556, 584, 333, 737, 333, //
        400, 549, 333, 333, 333, 576, 537, 278, 333, 333, 365, 556, 834, 834, 834, 611, //
        667, 667, 667, 667, 667, 667, 1000, 722, 667, 667, 667, 667, 278, 278, 278, 278, //
        722, 722, 778, 778, 778, 778, 778, 584, 778, 722, 722, 722, 722, 667, 667, 611, //
        556, 556, 556, 556, 556, 556, 889, 500, 556, 556, 556, 556, 278, 278, 278, 278, //
        556, 556, 556, 556, 556, 556, 556, 549, 611, 556, 556, 556, 556, 500, 556, 500,
    ],
    bbox: [-166, -225, 1000, 931],
    ascent: 718,
    descent: -207,
    cap_height: 718,
    stem_v: 88,
};

/// Helvetica-Bold — headings, column labels, and the one line of a document
/// that must not be misread: its total.
pub static HELVETICA_BOLD: Metrics = Metrics {
    base_font: "Helvetica-Bold",
    widths: [
        278, 333, 474, 556, 556, 889, 722, 238, 333, 333, 389, 584, 278, 333, 278, 278, //
        556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 333, 333, 584, 584, 584, 611, //
        975, 722, 722, 722, 722, 667, 611, 778, 722, 278, 556, 722, 611, 833, 722, 778, //
        667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 333, 278, 333, 584, 556, //
        333, 556, 611, 556, 611, 556, 333, 611, 611, 278, 278, 556, 278, 889, 611, 611, //
        611, 611, 389, 556, 333, 611, 556, 778, 556, 556, 500, 389, 280, 389, 584, 0, //
        744, 0, 278, 556, 500, 1000, 556, 556, 333, 1000, 667, 333, 1000, 0, 611, 0, //
        0, 278, 278, 500, 500, 350, 556, 1000, 333, 1000, 556, 333, 944, 0, 500, 667, //
        278, 333, 556, 556, 556, 556, 280, 556, 333, 737, 370, 556, 584, 333, 737, 333, //
        400, 549, 333, 333, 333, 576, 556, 278, 333, 333, 365, 556, 834, 834, 834, 611, //
        722, 722, 722, 722, 722, 722, 1000, 722, 667, 667, 667, 667, 278, 278, 278, 278, //
        722, 722, 778, 778, 778, 778, 778, 584, 778, 722, 722, 722, 722, 667, 667, 611, //
        556, 556, 556, 556, 556, 556, 889, 556, 556, 556, 556, 556, 278, 278, 278, 278, //
        611, 611, 611, 611, 611, 611, 611, 549, 611, 611, 611, 611, 611, 556, 611, 556,
    ],
    bbox: [-170, -228, 1003, 962],
    ascent: 718,
    descent: -207,
    cap_height: 718,
    stem_v: 140,
};

#[cfg(test)]
mod tests {
    use super::*;

    /// The six codes WinAnsi leaves unassigned. Nothing may be drawn at them.
    const UNASSIGNED: [u8; 6] = [0x7F, 0x81, 0x8D, 0x8F, 0x90, 0x9D];

    #[test]
    fn a_column_of_amounts_can_line_up() {
        // Every digit shares one width in both faces. If this ever stopped
        // being true, tabular figures would be impossible and every money
        // column in every document would go ragged.
        for face in [&HELVETICA, &HELVETICA_BOLD] {
            let zero = face.width(b'0');
            assert_eq!(zero, 556, "{}: digits are 556 wide", face.base_font);
            for digit in b'0'..=b'9' {
                assert_eq!(face.width(digit), zero, "{}", face.base_font);
            }
            // The characters an amount is built from besides its digits.
            assert_eq!(face.width(b' '), 278, "{}", face.base_font);
            assert_eq!(face.width(b'.'), 278, "{}", face.base_font);
        }
    }

    #[test]
    fn the_tables_are_the_shape_the_font_dictionary_declares() {
        assert_eq!(WIDTH_COUNT, 224);
        for face in [&HELVETICA, &HELVETICA_BOLD] {
            assert_eq!(face.widths.len(), WIDTH_COUNT);
            for code in FIRST_CHAR..=LAST_CHAR {
                let width = face.width(code);
                if UNASSIGNED.contains(&code) {
                    assert_eq!(width, 0, "{code:#04X} is unassigned in WinAnsi");
                } else {
                    assert!(width > 0, "{:#04X} has no width", code);
                    assert!(width <= 1015, "{code:#04X} is implausibly wide");
                }
            }
        }
    }

    #[test]
    fn the_faces_are_the_two_a_document_needs_and_differ_where_bold_differs() {
        assert_eq!(HELVETICA.base_font, "Helvetica");
        assert_eq!(HELVETICA_BOLD.base_font, "Helvetica-Bold");
        // Values a transcription slip would break, each independently known.
        assert_eq!(HELVETICA.width(b'A'), 667);
        assert_eq!(HELVETICA.width(b'W'), 944);
        assert_eq!(HELVETICA.width(b'i'), 222);
        assert_eq!(HELVETICA_BOLD.width(b'A'), 722);
        assert_eq!(HELVETICA_BOLD.width(b'i'), 278);
        // Bold is wider than regular for lowercase letters, never narrower.
        for code in b'a'..=b'z' {
            assert!(
                HELVETICA_BOLD.width(code) >= HELVETICA.width(code),
                "bold {} is narrower than regular",
                char::from(code)
            );
        }
    }

    #[test]
    fn a_code_below_the_table_measures_nothing_rather_than_panicking() {
        // Control codes cannot be drawn; the encoder never produces one, and
        // a lookup for one must still be total.
        assert_eq!(HELVETICA.width(0), 0);
        assert_eq!(HELVETICA.width(b'\n'), 0);
        assert_eq!(HELVETICA_BOLD.width(0x1F), 0);
    }
}
