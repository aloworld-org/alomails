//! How a run of text is set, and where it breaks.
//!
//! A PDF places glyphs; it does not lay out paragraphs. Anything a caller
//! wants that a word processor would do — a right-aligned column, a
//! description that wraps inside its cell — is arithmetic done here, before a
//! single byte reaches a content stream.

use crate::color::Color;
use crate::font::Font;

/// Which point of a run of text the caller's `x` names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    /// `x` is the left edge — the default, and every body paragraph.
    #[default]
    Left,
    /// `x` is the right edge — every column of money.
    Right,
    /// `x` is the centre — a banner, a monogram.
    Center,
}

/// Everything about how a run of text looks, in one value.
///
/// One struct rather than five arguments on every drawing call: a document
/// re-uses a handful of styles many times, and passing them by name is what
/// keeps a layout readable.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStyle {
    /// Which face.
    pub font: Font,
    /// Size in points.
    pub size: f64,
    /// Ink.
    pub color: Color,
    /// Extra space after every glyph, in points — the PDF `Tc` operator. The
    /// letter-spacing a small-caps label wants; zero everywhere else.
    pub char_spacing: f64,
}

impl TextStyle {
    /// A style in the default ink and with no letter-spacing.
    #[must_use]
    pub fn new(font: Font, size: f64) -> Self {
        Self {
            font,
            size,
            color: Color::BLACK,
            char_spacing: 0.0,
        }
    }

    /// The same style in another ink.
    #[must_use]
    pub fn inked(self, color: Color) -> Self {
        Self { color, ..self }
    }

    /// The same style, letter-spaced.
    #[must_use]
    pub fn tracked(self, char_spacing: f64) -> Self {
        Self {
            char_spacing,
            ..self
        }
    }

    /// The width `text` occupies in this style, letter-spacing included.
    ///
    /// PDF applies `Tc` after **every** glyph, the last one included, so the
    /// spacing multiplies the glyph count rather than the gaps between them —
    /// which is what a right-aligned run has to subtract.
    #[must_use]
    pub fn width_of(&self, text: &str) -> f64 {
        self.font.text_width(text, self.size)
            + self.char_spacing * self.font.glyph_count(text) as f64
    }

    /// Where a run of text starts, given the point `x` names under `align`.
    #[must_use]
    pub fn origin(&self, x: f64, align: Align, text: &str) -> f64 {
        match align {
            Align::Left => x,
            Align::Right => x - self.width_of(text),
            Align::Center => x - self.width_of(text) / 2.0,
        }
    }

    /// Breaks `text` into lines that each fit inside `max_width`.
    ///
    /// - **Existing line breaks are kept.** A note typed over three lines
    ///   prints over three lines; an empty line stays an empty line.
    /// - **Words break only on a space or a tab**, never on a no-break space,
    ///   so a grouped amount (`1 234.56`, whose separator is a narrow no-break
    ///   space) is never split across two lines.
    /// - **A word too long for the width is broken by character**, because
    ///   the alternative — one over-wide line — silently draws a customer's
    ///   40-character product code across the column beside it.
    /// - The result always contains at least one line, and no line is ever
    ///   empty because of the algorithm rather than the text.
    #[must_use]
    pub fn wrap(&self, text: &str, max_width: f64) -> Vec<String> {
        let mut lines = Vec::new();
        for paragraph in text.split('\n') {
            self.wrap_paragraph(paragraph.trim_end_matches('\r'), max_width, &mut lines);
        }
        if lines.is_empty() {
            lines.push(String::new());
        }
        lines
    }

    /// One paragraph of [`Self::wrap`], appended to `lines`.
    fn wrap_paragraph(&self, paragraph: &str, max_width: f64, lines: &mut Vec<String>) {
        let mut current = String::new();
        for word in paragraph.split([' ', '\t']).filter(|w| !w.is_empty()) {
            let candidate = if current.is_empty() {
                word.to_owned()
            } else {
                format!("{current} {word}")
            };
            if self.width_of(&candidate) <= max_width {
                current = candidate;
                continue;
            }
            if !current.is_empty() {
                lines.push(std::mem::take(&mut current));
            }
            current = if self.width_of(word) <= max_width {
                word.to_owned()
            } else {
                // The word alone still does not fit: break it by character.
                self.break_word(word, max_width, lines)
            };
        }
        if !current.is_empty() || paragraph.trim().is_empty() {
            lines.push(current);
        }
    }

    /// Pushes all but the last fitting chunk of an over-long word, returning
    /// the remainder to carry on with.
    fn break_word(&self, word: &str, max_width: f64, lines: &mut Vec<String>) -> String {
        let mut chunk = String::new();
        for c in word.chars() {
            let mut candidate = chunk.clone();
            candidate.push(c);
            // A single character always goes on the line, however narrow the
            // column: otherwise this loop would never terminate.
            if !chunk.is_empty() && self.width_of(&candidate) > max_width {
                lines.push(std::mem::take(&mut chunk));
            }
            chunk.push(c);
        }
        chunk
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style() -> TextStyle {
        TextStyle::new(Font::Regular, 10.0)
    }

    #[test]
    fn a_right_aligned_run_ends_where_it_was_asked_to() {
        let s = style();
        let width = s.width_of("1 234.56");
        assert!((s.origin(500.0, Align::Right, "1 234.56") + width - 500.0).abs() < 1e-9);
        assert!((s.origin(500.0, Align::Left, "x") - 500.0).abs() < 1e-9);
        assert!((s.origin(100.0, Align::Center, "x") + s.width_of("x") / 2.0 - 100.0).abs() < 1e-9);
    }

    #[test]
    fn letter_spacing_is_counted_once_per_glyph() {
        let plain = style();
        let tracked = plain.tracked(2.0);
        // Five glyphs, five extra units of tracking.
        assert!((tracked.width_of("Total") - plain.width_of("Total") - 10.0).abs() < 1e-9);
        assert!((tracked.width_of("") - plain.width_of("")).abs() < f64::EPSILON);
    }

    #[test]
    fn a_sentence_wraps_at_words_and_every_line_fits() {
        let s = style();
        let sentence = "Payable by 2026-08-21 to the account below, quoting the invoice number.";
        let lines = s.wrap(sentence, 120.0);
        assert!(lines.len() > 1, "a long sentence must wrap");
        for line in &lines {
            assert!(
                s.width_of(line) <= 120.0,
                "line wider than the column: {line:?} ({})",
                s.width_of(line)
            );
        }
        // Nothing is lost and nothing is invented.
        assert_eq!(lines.join(" "), sentence);
    }

    #[test]
    fn typed_line_breaks_are_the_authors_and_are_kept() {
        let s = style();
        let lines = s.wrap("first\n\nthird", 500.0);
        assert_eq!(lines, vec!["first", "", "third"]);
        // A short text is one line, never zero.
        assert_eq!(s.wrap("", 500.0), vec![String::new()]);
        assert_eq!(s.wrap("short", 500.0), vec!["short"]);
    }

    #[test]
    fn a_word_wider_than_the_column_is_broken_rather_than_left_to_overflow() {
        let s = style();
        let code = "PRODUCTCODE1234567890ABCDEFGHIJ";
        let lines = s.wrap(code, 40.0);
        assert!(lines.len() > 1);
        for line in &lines {
            assert!(!line.is_empty());
            assert!(s.width_of(line) <= 40.0, "{line:?} overflows");
        }
        assert_eq!(lines.concat(), code);
    }

    #[test]
    fn a_column_narrower_than_one_character_still_terminates() {
        // Pathological, but a layout bug upstream must not hang the server.
        let s = style();
        let lines = s.wrap("abc def", 0.1);
        assert_eq!(lines.concat().replace(' ', ""), "abcdef");
        assert!(lines.iter().all(|l| !l.is_empty()));
    }

    #[test]
    fn an_amount_is_never_split_across_two_lines() {
        // The group separator is a narrow no-break space; it must not be a
        // break opportunity, or "EUR 1 234.56" could print as "1" and "234.56"
        // on separate lines of a cell.
        let s = style();
        let amount = "EUR 1\u{202f}234\u{202f}567.89";
        let lines = s.wrap(amount, s.width_of("EUR 1\u{202f}234\u{202f}567.89") - 1.0);
        assert_eq!(lines.len(), 2, "only the ordinary space is a break");
        assert_eq!(lines[0], "EUR");
        assert_eq!(lines[1], "1\u{202f}234\u{202f}567.89");
    }
}
