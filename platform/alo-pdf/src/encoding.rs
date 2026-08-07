//! Text → **WinAnsi** (cp1252), the encoding the standard-14 fonts speak.
//!
//! The fourteen fonts every PDF reader is required to have can address 218
//! characters, and that set is Western European. It covers French, German,
//! Spanish, Italian, Dutch, Portuguese, the Nordic languages, Icelandic — and
//! it does **not** cover Polish, Czech, Slovak, Hungarian, Romanian, the Baltic
//! languages, Greek or Cyrillic.
//!
//! So a character it cannot address is **folded to its base Latin form**
//! rather than dropped or replaced: `Łukasz` prints as `Lukasz`, `Ștefan` as
//! `Stefan`, `Erdős` as `Erdos`. That is a lossy rendering of somebody's name
//! and we are not pretending otherwise — it is chosen only because the
//! alternative on the same page is `?ukasz`, and because a name a reader can
//! still recognise is worth more than a row of question marks. A script with
//! no Latin base at all (Greek, Cyrillic) reaches the last resort, `?`.
//!
//! **This ends at B1.22.** PDF/A-3 — which Factur-X requires — forbids
//! non-embedded fonts, so an embedded font file lands there and takes the
//! whole limitation with it: nothing outside this module and
//! [`crate::metrics`] knows the repertoire is restricted.

/// What an unrepresentable character becomes when it has no Latin base.
const REPLACEMENT: u8 = b'?';

/// Encodes text as WinAnsi bytes, folding what the encoding cannot represent.
///
/// Total: every `&str` produces bytes, and no byte is a control code, so the
/// result is always safe to place in a PDF string.
#[must_use]
pub fn encode(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    for c in text.chars() {
        match win_ansi(c) {
            Some(byte) => out.push(byte),
            // `chars`, not `bytes`: a fold may itself be a non-ASCII
            // character (a no-break space), and its UTF-8 bytes are not its
            // WinAnsi code.
            None => out.extend(fold(c).chars().map(|f| win_ansi(f).unwrap_or(REPLACEMENT))),
        }
    }
    out
}

/// Text as a **PDF literal string**, parentheses included.
///
/// The one place text meets the file format, so it is written once and both
/// the page content ([`crate::canvas`]) and the document information
/// dictionary ([`crate::writer`]) go through it. `\`, `(` and `)` are escaped
/// because they delimit the string — a customer called `Acme (Holdings)` must
/// not be able to close it — and every byte outside printable ASCII is written
/// as an octal escape, so the file stays 7-bit clean and no byte of somebody's
/// name can be mistaken for structure.
#[must_use]
pub fn pdf_string(text: &str) -> String {
    let mut out = String::from("(");
    for byte in encode(text) {
        match byte {
            b'\\' => out.push_str("\\\\"),
            b'(' => out.push_str("\\("),
            b')' => out.push_str("\\)"),
            0x20..=0x7E => out.push(char::from(byte)),
            other => out.push_str(&format!("\\{other:03o}")),
        }
    }
    out.push(')');
    out
}

/// The WinAnsi code for one character, or `None` when the encoding has none.
///
/// Whitespace other than the space itself (tab, newline) is deliberately
/// *not* encodable: a PDF string is one line of text on one baseline, and a
/// caller that has newlines in its hands must split them into lines before
/// asking for bytes. [`encode`] folds them to a space so that a caller which
/// forgets cannot smuggle a control byte into a content stream.
#[must_use]
pub fn win_ansi(c: char) -> Option<u8> {
    let code = u32::from(c);
    // ASCII printable, and Latin-1's upper half, are WinAnsi unchanged.
    if (0x20..0x7F).contains(&code) || (0xA0..=0xFF).contains(&code) {
        return u8::try_from(code).ok();
    }
    // The 0x80..0x9F block, where cp1252 differs from Latin-1: typography and
    // the six letters (Š š Ž ž Œ œ Ÿ) that make it more than ISO 8859-1.
    Some(match c {
        '\u{20AC}' => 0x80, // €
        '\u{201A}' => 0x82, // ‚
        '\u{0192}' => 0x83, // ƒ
        '\u{201E}' => 0x84, // „
        '\u{2026}' => 0x85, // …
        '\u{2020}' => 0x86, // †
        '\u{2021}' => 0x87, // ‡
        '\u{02C6}' => 0x88, // ˆ
        '\u{2030}' => 0x89, // ‰
        '\u{0160}' => 0x8A, // Š
        '\u{2039}' => 0x8B, // ‹
        '\u{0152}' => 0x8C, // Œ
        '\u{017D}' => 0x8E, // Ž
        '\u{2018}' => 0x91, // '
        '\u{2019}' => 0x92, // '
        '\u{201C}' => 0x93, // "
        '\u{201D}' => 0x94, // "
        '\u{2022}' => 0x95, // •
        '\u{2013}' => 0x96, // –
        '\u{2014}' => 0x97, // —
        '\u{02DC}' => 0x98, // ˜
        '\u{2122}' => 0x99, // ™
        '\u{0161}' => 0x9A, // š
        '\u{203A}' => 0x9B, // ›
        '\u{0153}' => 0x9C, // œ
        '\u{017E}' => 0x9E, // ž
        '\u{0178}' => 0x9F, // Ÿ
        _ => return None,
    })
}

/// The closest printable Latin form of a character WinAnsi cannot represent.
///
/// Covers Latin Extended-A (the Central European, Baltic and Turkish letters)
/// and the Romanian comma-below pair from Extended-B, plus the typographic
/// characters our own document formatters emit. Everything else — including
/// every non-Latin script — is `?`.
#[must_use]
pub fn fold(c: char) -> &'static str {
    match c {
        // -- Latin Extended-A: the letters cp1252 is missing -----------------
        'Ā' | 'Ă' | 'Ą' => "A",
        'ā' | 'ă' | 'ą' => "a",
        'Ć' | 'Ĉ' | 'Ċ' | 'Č' => "C",
        'ć' | 'ĉ' | 'ċ' | 'č' => "c",
        'Ď' | 'Đ' => "D",
        'ď' | 'đ' => "d",
        'Ē' | 'Ĕ' | 'Ė' | 'Ę' | 'Ě' => "E",
        'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => "e",
        'Ĝ' | 'Ğ' | 'Ġ' | 'Ģ' => "G",
        'ĝ' | 'ğ' | 'ġ' | 'ģ' => "g",
        'Ĥ' | 'Ħ' => "H",
        'ĥ' | 'ħ' => "h",
        'Ĩ' | 'Ī' | 'Ĭ' | 'Į' | 'İ' => "I",
        'ĩ' | 'ī' | 'ĭ' | 'į' | 'ı' => "i",
        'Ĳ' => "IJ",
        'ĳ' => "ij",
        'Ĵ' => "J",
        'ĵ' => "j",
        'Ķ' => "K",
        'ķ' | 'ĸ' => "k",
        'Ĺ' | 'Ļ' | 'Ľ' | 'Ŀ' | 'Ł' => "L",
        'ĺ' | 'ļ' | 'ľ' | 'ŀ' | 'ł' => "l",
        'Ń' | 'Ņ' | 'Ň' | 'Ŋ' => "N",
        'ń' | 'ņ' | 'ň' | 'ŋ' => "n",
        'ŉ' => "'n",
        'Ō' | 'Ŏ' | 'Ő' => "O",
        'ō' | 'ŏ' | 'ő' => "o",
        'Ŕ' | 'Ŗ' | 'Ř' => "R",
        'ŕ' | 'ŗ' | 'ř' => "r",
        'Ś' | 'Ŝ' | 'Ş' | 'Ș' => "S",
        'ś' | 'ŝ' | 'ş' | 'ș' => "s",
        'Ţ' | 'Ť' | 'Ŧ' | 'Ț' => "T",
        'ţ' | 'ť' | 'ŧ' | 'ț' => "t",
        'Ũ' | 'Ū' | 'Ŭ' | 'Ů' | 'Ű' | 'Ų' => "U",
        'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => "u",
        'Ŵ' => "W",
        'ŵ' => "w",
        'Ŷ' => "Y",
        'ŷ' => "y",
        'Ź' | 'Ż' => "Z",
        'ź' | 'ż' => "z",
        'ſ' => "s",
        // -- typography our own formatters and stored text produce -----------
        // A no-break space, not a space: an amount's digit groups and a
        // wrapped sentence must not be broken apart by [`crate::font::wrap`].
        '\u{2009}' | '\u{202F}' => "\u{a0}",
        '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2212}' => "-",
        '\u{2032}' => "'",
        '\u{2033}' => "\"",
        // Line and paragraph separators, and the whitespace a caller should
        // have split on: a PDF string is one baseline, never two.
        '\t' | '\n' | '\r' | '\u{2028}' | '\u{2029}' => " ",
        _ => "?",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> String {
        encode(s).iter().map(|b| char::from(*b)).collect()
    }

    #[test]
    fn western_europe_is_carried_exactly() {
        // Latin-1 letters are one byte each and keep their code point, so a
        // German or French name is printed, not approximated.
        assert_eq!(encode("Kunde & Söhne"), b"Kunde & S\xf6hne");
        assert_eq!(encode("Ærø"), b"\xc6r\xf8");
        assert_eq!(encode("Citroën"), b"Citro\xebn");
        assert_eq!(encode("Straße"), b"Stra\xdfe");
        // …including the six letters cp1252 adds above Latin-1.
        assert_eq!(encode("Škoda"), b"\x8akoda");
        assert_eq!(encode("Žofia œuvre"), b"\x8eofia \x9cuvre");
    }

    #[test]
    fn a_name_we_cannot_print_stays_legible_rather_than_becoming_noise() {
        // The eight member states the standard-14 fonts cannot spell. Each
        // fold is a real name, so a regression reads as a wrong name.
        assert_eq!(text("Łukasz Wójcik"), "Lukasz W\u{f3}jcik"); // ó is cp1252
        assert_eq!(text("Václav Havlíček"), "V\u{e1}clav Havl\u{ed}cek");
        assert_eq!(text("Erdős Ferenc"), "Erdos Ferenc");
        assert_eq!(text("Ștefan Țării"), "Stefan Tarii");
        assert_eq!(text("Jonas Žilinskas"), "Jonas \u{8e}ilinskas"); // Ž is cp1252
        assert_eq!(text("Đorđe"), "Dorde");
    }

    #[test]
    fn a_script_with_no_latin_base_is_marked_not_guessed() {
        // Transliterating Greek or Cyrillic is a language decision, not an
        // encoding one; the document says it could not print them.
        assert_eq!(text("Αθήνα"), "?????");
        assert_eq!(text("София"), "?????");
        assert_eq!(text("東京"), "??");
        // But nothing is ever *lost*: one character in, one glyph out.
        assert_eq!(encode("Αθήνα").len(), 5);
    }

    #[test]
    fn the_formatters_own_typography_survives() {
        // What billing_print's amount()/quantity() actually emit.
        assert_eq!(encode("1\u{202f}234.56"), b"1\xa0234.56");
        assert_eq!(text("\u{2212}226.88"), "-226.88");
        assert_eq!(encode("a \u{2013} b"), b"a \x96 b");
        assert_eq!(encode("Alo \u{b7} ABN"), b"Alo \xb7 ABN");
        assert_eq!(encode("EUR \u{20ac}"), b"EUR \x80");
    }

    #[test]
    fn no_control_byte_can_reach_a_content_stream() {
        // A caller that forgets to split its lines gets spaces, never a byte
        // that would end the PDF string it is being written into.
        let bytes = encode("first\nsecond\tthird\r\n");
        assert_eq!(bytes, b"first second third  ");
        for byte in encode("\u{0}\u{1}\u{7}\u{1b}\u{7f}\u{9d}") {
            assert!(byte >= 0x20, "control byte {byte:#04x} escaped the encoder");
        }
    }

    #[test]
    fn every_encodable_character_round_trips_to_its_own_code() {
        // The two ranges that are the identity mapping really are one.
        for code in (0x20..0x7Fu32).chain(0xA0..=0xFF) {
            let c = char::from_u32(code).unwrap_or('?');
            assert_eq!(win_ansi(c), u8::try_from(code).ok(), "{c:?}");
        }
        // …and the unassigned WinAnsi slots are refused, so the width tables
        // are never asked for a glyph that does not exist.
        for code in [0x7Fu32, 0x81, 0x8D, 0x8F, 0x90, 0x9D] {
            let c = char::from_u32(code).unwrap_or('?');
            assert_eq!(win_ansi(c), None, "{code:#04x} is unassigned in WinAnsi");
        }
    }

    #[test]
    fn the_empty_string_encodes_to_nothing() {
        assert!(encode("").is_empty());
        assert_eq!(pdf_string(""), "()");
        assert_eq!(fold('ø'), "?", "a character cp1252 has is never folded");
    }

    #[test]
    fn nothing_a_customer_typed_can_close_the_string_it_is_written_into() {
        assert_eq!(
            pdf_string("Acme (Holdings) \\ Ltd"),
            "(Acme \\(Holdings\\) \\\\ Ltd)"
        );
        // Non-ASCII is octal, never a raw byte.
        assert_eq!(pdf_string("Söhne"), "(S\\366hne)");
        assert!(pdf_string("Αθήνα Łukasz").is_ascii());
    }
}
