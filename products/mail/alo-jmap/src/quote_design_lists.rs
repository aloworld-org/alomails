//! The list schemes of the quotation studio, as the server numbers them.
//!
//! The studio's catalogue (`web/src/billing/quote-studio/listStyles.ts`) is
//! the source of truth for what a marker looks like; this is the same
//! catalogue written a second time so the printed page and the PDF number a
//! list exactly as the screen did. The two are kept in step by the shared
//! fixtures in their tests — a scheme added on one side without the other is
//! a failing test, not a silent difference.
//!
//! Items travel as one newline-separated string with a leading tab per
//! nesting level (`listItems.ts`), which is what a saved design carries.

/// Nesting depth of a list item: three levels, like the library it mirrors.
pub const MAX_LEVEL: usize = 2;

/// One of the studio's numbering or bullet schemes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListStyle {
    /// `1.` / `a.` / `i.`
    Decimal,
    /// `1)` / `a)` / `i)`
    Parenthesis,
    /// `1.` / `1.1.` / `1.2.1.`
    Outline,
    /// `A.` / `a.` / `i.`
    UpperAlpha,
    /// `I.` / `A.` / `1.`
    Roman,
    /// `01.` / `a.` / `i.`
    LeadingZero,
    /// `●` / `○` / `■`
    Disc,
    /// `❖` / `➢` / `■`
    Diamond,
    /// `❑` at every level.
    Square,
    /// `➔` / `◆` / `●`
    Arrow,
    /// `★` / `○` / `■`
    Star,
    /// `➢` / `○` / `■`
    Chevron,
    /// `☐` at every level.
    Checkbox,
}

impl ListStyle {
    /// The scheme a saved design names, or what a list looked like before
    /// schemes existed — plain numbers, round bullets. A bullet scheme on a
    /// numbered list (or the reverse) is treated as absent, as the studio
    /// itself does (`resolveListStyle`).
    #[must_use]
    pub fn resolve(id: Option<&str>, ordered: bool) -> Self {
        let style = match id {
            Some("decimal") => Some(Self::Decimal),
            Some("parenthesis") => Some(Self::Parenthesis),
            Some("outline") => Some(Self::Outline),
            Some("upper-alpha") => Some(Self::UpperAlpha),
            Some("roman") => Some(Self::Roman),
            Some("leading-zero") => Some(Self::LeadingZero),
            Some("disc") => Some(Self::Disc),
            Some("diamond") => Some(Self::Diamond),
            Some("square") => Some(Self::Square),
            Some("arrow") => Some(Self::Arrow),
            Some("star") => Some(Self::Star),
            Some("chevron") => Some(Self::Chevron),
            Some("checkbox") => Some(Self::Checkbox),
            _ => None,
        };
        match style {
            Some(style) if style.is_numbering() == ordered => style,
            _ if ordered => Self::Decimal,
            _ => Self::Disc,
        }
    }

    /// Whether the scheme counts (as opposed to bullets).
    #[must_use]
    pub fn is_numbering(self) -> bool {
        matches!(
            self,
            Self::Decimal
                | Self::Parenthesis
                | Self::Outline
                | Self::UpperAlpha
                | Self::Roman
                | Self::LeadingZero
        )
    }

    /// The bullet glyphs per level, as the screen shows them.
    fn glyphs(self) -> [&'static str; 3] {
        match self {
            Self::Disc => ["\u{25cf}", "\u{25cb}", "\u{25a0}"],
            Self::Diamond => ["\u{2756}", "\u{27a2}", "\u{25a0}"],
            Self::Square => ["\u{2751}", "\u{2751}", "\u{2751}"],
            Self::Arrow => ["\u{2794}", "\u{25c6}", "\u{25cf}"],
            Self::Star => ["\u{2605}", "\u{25cb}", "\u{25a0}"],
            Self::Chevron => ["\u{27a2}", "\u{25cb}", "\u{25a0}"],
            Self::Checkbox => ["\u{2610}", "\u{2610}", "\u{2610}"],
            _ => ["", "", ""],
        }
    }

    /// The bullet glyphs per level in the characters the PDF's standard fonts
    /// can set (WinAnsi has a bullet, a dash and a middle dot, and nothing
    /// like a star). The scheme is kept legible rather than exact: a marker
    /// the reader's font cannot draw would print as a box.
    fn pdf_glyphs(self) -> [&'static str; 3] {
        match self {
            Self::Checkbox => ["[ ]", "[ ]", "[ ]"],
            Self::Arrow | Self::Chevron => [">", "\u{2013}", "\u{b7}"],
            Self::Square => ["\u{a7}", "\u{a7}", "\u{a7}"],
            _ => ["\u{2022}", "\u{2013}", "\u{b7}"],
        }
    }
}

/// One item of a list: how deep it sits and what it says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListItem {
    /// 0 for a top-level item, up to [`MAX_LEVEL`].
    pub level: usize,
    /// The item's text, in the studio's inline rich text.
    pub text: String,
}

/// An item with the marker it prints under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NumberedItem {
    /// The item.
    pub item: ListItem,
    /// The marker as the screen shows it — `1.`, `b)`, `1.2.1.`, `●` …
    pub marker: String,
    /// The marker in characters the PDF's fonts can set.
    pub pdf_marker: String,
}

/// The stored string → items, empty lines dropped (as the screen drops
/// them). Tabs beyond the deepest level are treated as the deepest.
#[must_use]
pub fn parse_items(items: &str) -> Vec<ListItem> {
    items
        .split('\n')
        .map(|line| {
            let text = line.trim_start_matches('\t');
            ListItem {
                level: (line.len() - text.len()).min(MAX_LEVEL),
                text: text.to_owned(),
            }
        })
        .filter(|item| !item.text.trim().is_empty())
        .collect()
}

/// Numbers every item in document order. A counter restarts when a shallower
/// item appears, so `1. / a. / b. / 2. / a.` counts the way a reader expects.
#[must_use]
pub fn number_items(items: Vec<ListItem>, style: ListStyle) -> Vec<NumberedItem> {
    let mut counters = [0usize; MAX_LEVEL + 1];
    items
        .into_iter()
        .map(|item| {
            let level = item.level.min(MAX_LEVEL);
            counters[level] += 1;
            for deeper in counters.iter_mut().skip(level + 1) {
                *deeper = 0;
            }
            let (marker, pdf_marker) = if style.is_numbering() {
                let marker = numbering(style, level, &counters);
                (marker.clone(), marker)
            } else {
                (
                    style.glyphs()[level].to_owned(),
                    style.pdf_glyphs()[level].to_owned(),
                )
            };
            NumberedItem {
                item,
                marker,
                pdf_marker,
            }
        })
        .collect()
}

fn numbering(style: ListStyle, level: usize, c: &[usize; MAX_LEVEL + 1]) -> String {
    match (style, level) {
        (ListStyle::Decimal, 0) => format!("{}.", c[0]),
        (ListStyle::Decimal, 1) => format!("{}.", lower_alpha(c[1])),
        (ListStyle::Decimal, _) => format!("{}.", lower_roman(c[2])),
        (ListStyle::Parenthesis, 0) => format!("{})", c[0]),
        (ListStyle::Parenthesis, 1) => format!("{})", lower_alpha(c[1])),
        (ListStyle::Parenthesis, _) => format!("{})", lower_roman(c[2])),
        (ListStyle::Outline, 0) => format!("{}.", c[0]),
        (ListStyle::Outline, 1) => format!("{}.{}.", c[0], c[1]),
        (ListStyle::Outline, _) => format!("{}.{}.{}.", c[0], c[1], c[2]),
        (ListStyle::UpperAlpha, 0) => format!("{}.", upper_alpha(c[0])),
        (ListStyle::UpperAlpha, 1) => format!("{}.", lower_alpha(c[1])),
        (ListStyle::UpperAlpha, _) => format!("{}.", lower_roman(c[2])),
        (ListStyle::Roman, 0) => format!("{}.", upper_roman(c[0])),
        (ListStyle::Roman, 1) => format!("{}.", upper_alpha(c[1])),
        (ListStyle::Roman, _) => format!("{}.", c[2]),
        (ListStyle::LeadingZero, 0) => format!("{:02}.", c[0]),
        (ListStyle::LeadingZero, 1) => format!("{}.", lower_alpha(c[1])),
        (ListStyle::LeadingZero, _) => format!("{}.", lower_roman(c[2])),
        _ => String::new(),
    }
}

/// 1 → A, 26 → Z, 27 → AA — the spreadsheet column sequence.
fn upper_alpha(n: usize) -> String {
    if n == 0 {
        return "0".to_owned();
    }
    let mut out = Vec::new();
    let mut rest = n;
    while rest > 0 {
        let index = (rest - 1) % 26;
        out.push(char::from(b'A' + u8::try_from(index).unwrap_or(0)));
        rest = (rest - 1) / 26;
    }
    out.iter().rev().collect()
}

fn lower_alpha(n: usize) -> String {
    upper_alpha(n).to_lowercase()
}

const ROMAN: [(usize, &str); 13] = [
    (1000, "M"),
    (900, "CM"),
    (500, "D"),
    (400, "CD"),
    (100, "C"),
    (90, "XC"),
    (50, "L"),
    (40, "XL"),
    (10, "X"),
    (9, "IX"),
    (5, "V"),
    (4, "IV"),
    (1, "I"),
];

fn upper_roman(n: usize) -> String {
    if n == 0 || n > 3999 {
        return n.to_string();
    }
    let mut out = String::new();
    let mut rest = n;
    for (value, numeral) in ROMAN {
        while rest >= value {
            out.push_str(numeral);
            rest -= value;
        }
    }
    out
}

fn lower_roman(n: usize) -> String {
    upper_roman(n).to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn markers(items: &str, style: ListStyle) -> Vec<String> {
        number_items(parse_items(items), style)
            .into_iter()
            .map(|n| n.marker)
            .collect()
    }

    #[test]
    fn the_numbering_library_matches_the_screen_scheme_for_scheme() {
        // The same fixtures as `listItems.test.ts` / `listStyles.test.ts`.
        let outline = "A\n\tB\n\tC\n\t\tD\nE\n\tF";
        assert_eq!(
            markers(outline, ListStyle::Decimal),
            ["1.", "a.", "b.", "i.", "2.", "a."]
        );
        assert_eq!(
            markers("A\n\tB\n\tC\n\t\tD\nE", ListStyle::Outline),
            ["1.", "1.1.", "1.2.", "1.2.1.", "2."]
        );
        assert_eq!(
            markers(outline, ListStyle::Parenthesis),
            ["1)", "a)", "b)", "i)", "2)", "a)"]
        );
        assert_eq!(markers("A\nB", ListStyle::UpperAlpha), ["A.", "B."]);
        assert_eq!(
            markers("A\n\tB\n\t\tC\nD\nE\nF\nG", ListStyle::Roman),
            ["I.", "A.", "1.", "II.", "III.", "IV.", "V."]
        );
        assert_eq!(
            markers("A\nB\n\tC", ListStyle::LeadingZero),
            ["01.", "02.", "a."]
        );
    }

    #[test]
    fn counting_continues_past_the_alphabet_and_into_roman_tens() {
        assert_eq!(upper_alpha(27), "AA");
        assert_eq!(upper_alpha(52), "AZ");
        assert_eq!(upper_roman(14), "XIV");
        assert_eq!(upper_roman(1994), "MCMXCIV");
    }

    #[test]
    fn bullets_carry_the_scheme_glyph_and_a_printable_stand_in() {
        let numbered = number_items(parse_items("A\n\tB\n\t\tC"), ListStyle::Diamond);
        let glyphs: Vec<&str> = numbered.iter().map(|n| n.marker.as_str()).collect();
        assert_eq!(glyphs, ["\u{2756}", "\u{27a2}", "\u{25a0}"]);
        for item in &numbered {
            assert!(
                item.pdf_marker.chars().all(|c| u32::from(c) < 0x2023),
                "PDF marker must be WinAnsi-settable: {}",
                item.pdf_marker
            );
        }
        let boxes = number_items(parse_items("A"), ListStyle::Checkbox);
        assert_eq!(boxes[0].marker, "\u{2610}");
        assert_eq!(boxes[0].pdf_marker, "[ ]");
    }

    #[test]
    fn a_saved_scheme_is_resolved_against_the_kind_of_list() {
        assert_eq!(ListStyle::resolve(None, true), ListStyle::Decimal);
        assert_eq!(ListStyle::resolve(None, false), ListStyle::Disc);
        assert_eq!(ListStyle::resolve(Some("roman"), true), ListStyle::Roman);
        assert_eq!(ListStyle::resolve(Some("roman"), false), ListStyle::Disc);
        assert_eq!(ListStyle::resolve(Some("star"), false), ListStyle::Star);
        assert_eq!(ListStyle::resolve(Some("star"), true), ListStyle::Decimal);
        assert_eq!(ListStyle::resolve(Some("fancy"), true), ListStyle::Decimal);
    }

    #[test]
    fn items_drop_blank_lines_and_clamp_depth() {
        let items = parse_items("A\n\n\tB\n\t\t\t\tC\n   ");
        assert_eq!(items.len(), 3);
        assert_eq!(items[1].level, 1);
        assert_eq!(items[2].level, MAX_LEVEL);
        assert_eq!(items[2].text, "C");
    }
}
