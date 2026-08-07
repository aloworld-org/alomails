//! The machinery both e-invoice syntaxes are written with (alo Billing, wave
//! B1.23) — an emitter, the standard's number formats, and the response an XML
//! document leaves as.
//!
//! EN 16931 has two syntax bindings in law: UN/CEFACT CII, which Factur-X
//! carries ([`crate::billing_cii`]), and OASIS UBL, which XRechnung uses
//! ([`crate::billing_ubl`]). They disagree about almost everything visible —
//! element names, nesting, where the currency is stated, how a date is spelt —
//! and agree about everything underneath: an amount is two decimals with the
//! sign in front, a quantity has no fixed scale, a percentage has two decimals,
//! and no string a customer typed may ever open an element.
//!
//! So that agreement lives here, once. It is deliberately **not** an XML
//! library: both schemas are sequences, the order is the caller's
//! responsibility, and a library that could reorder attributes or drop
//! whitespace would break golden files for no gain.

use axum::http::header;
use axum::response::{IntoResponse, Response};

use crate::billing_print::{PrintDocument, Strings, file_stem};

// ---- formatting --------------------------------------------------------------

/// Integer cents as the standard's decimal: always two places, the sign in
/// front, and no grouping — this is a machine's number, not the paper's.
#[must_use]
pub fn amount(cents: i64) -> String {
    let value = i128::from(cents);
    let magnitude = value.unsigned_abs();
    format!(
        "{}{}.{:02}",
        if value < 0 { "-" } else { "" },
        magnitude / 100,
        magnitude % 100
    )
}

/// Milli-units as a decimal quantity, with no more decimals than it has.
///
/// `1.5`, not `1.500`: a quantity is not an amount, it has no fixed scale, and
/// trailing zeros invite a reader to think they mean precision.
#[must_use]
pub fn quantity(qty_milli: i64) -> String {
    let value = i128::from(qty_milli);
    let magnitude = value.unsigned_abs();
    let sign = if value < 0 { "-" } else { "" };
    let units = magnitude / 1_000;
    let thousandths = magnitude % 1_000;
    if thousandths == 0 {
        return format!("{sign}{units}");
    }
    let fraction = format!("{thousandths:03}");
    format!("{sign}{units}.{}", fraction.trim_end_matches('0'))
}

/// Basis points as a percentage with two decimals — `2100` is `21.00`.
#[must_use]
pub fn percent(rate_bp: i32) -> String {
    let magnitude = i64::from(rate_bp).unsigned_abs();
    format!(
        "{}{}.{:02}",
        if rate_bp < 0 { "-" } else { "" },
        magnitude / 100,
        magnitude % 100
    )
}

/// Escapes text for XML.
///
/// All five, in element text as well as in attributes, for the same reason the
/// HTML renderer escapes all five ([`crate::billing_print`]): one escaper that
/// is safe everywhere beats two that have to be chosen between. Control
/// characters that XML 1.0 cannot represent at all — a stray `\u{0}` from a
/// paste — are dropped rather than encoded, because there is no encoding of
/// them that a parser will accept.
#[must_use]
pub fn esc(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\t' | '\n' | '\r' => out.push(c),
            c if (c as u32) < 0x20 => {}
            c => out.push(c),
        }
    }
    out
}

// ---- the download ------------------------------------------------------------

/// The name an e-invoice is saved under when it is downloaded on its own.
///
/// The document's own heading plus the syntax — `Invoice-INV-2026-00001-
/// factur-x.xml`, `…-xrechnung.xml` — so an invoice's renderings sort next to
/// each other in the folder somebody archives them in, and the two syntaxes of
/// one document never collide. Never the name the file has *inside* a hybrid
/// PDF ([`crate::billing_cii::ATTACHMENT_NAME`]), which is fixed by the
/// standard and identical for every document there is.
#[must_use]
pub fn file_name(doc: &PrintDocument<'_>, s: &Strings, syntax: &str) -> String {
    let stem = file_stem(doc, s);
    if stem.is_empty() {
        format!("document-{syntax}.xml")
    } else {
        format!("{stem}-{syntax}.xml")
    }
}

/// Serves an e-invoice as a file.
///
/// The same three headers the PDF is served with, for the same reasons
/// ([`crate::billing_pdf::response`]): an **attachment**, never inline, so no
/// XML document is ever opened inside our origin; `nosniff`, so nothing
/// re-interprets the bytes; and `no-store`, because this is a customer's
/// invoice rather than a cacheable asset.
#[must_use]
pub fn response(xml: String, file_name: &str) -> Response {
    (
        [
            (
                header::CONTENT_TYPE,
                "application/xml; charset=utf-8".to_owned(),
            ),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{file_name}\""),
            ),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_owned()),
            (header::CACHE_CONTROL, "no-store".to_owned()),
        ],
        xml,
    )
        .into_response()
}

// ---- the emitter -------------------------------------------------------------

/// A tiny indented-XML emitter.
///
/// Not a general XML library: it writes elements in the order it is told to,
/// and the schema's sequence is the caller's responsibility. What it *does*
/// guarantee is that every element it opens is closed at the depth it was
/// opened, and that no text reaches the document unescaped.
pub struct Xml {
    out: String,
    depth: usize,
}

impl Xml {
    /// An empty document.
    #[must_use]
    pub fn new() -> Self {
        Self {
            out: String::with_capacity(4096),
            depth: 0,
        }
    }

    fn indent(&mut self) {
        for _ in 0..self.depth {
            self.out.push_str("  ");
        }
    }

    /// Writes text with no escaping and no indentation — the XML declaration,
    /// and nothing a document's data ever reaches.
    pub fn raw(&mut self, text: &str) {
        self.out.push_str(text);
    }

    /// Opens an element and descends into it.
    pub fn open(&mut self, tag: &str) {
        self.indent();
        self.out.push('<');
        self.out.push_str(tag);
        self.out.push_str(">\n");
        self.depth += 1;
    }

    /// Opens an element carrying the given attributes, already formatted.
    pub fn open_with(&mut self, tag: &str, attributes: &str) {
        self.indent();
        self.out.push('<');
        self.out.push_str(tag);
        self.out.push(' ');
        self.out.push_str(attributes);
        self.out.push_str(">\n");
        self.depth += 1;
    }

    /// Closes the element opened at this depth.
    pub fn close(&mut self, tag: &str) {
        self.depth = self.depth.saturating_sub(1);
        self.indent();
        self.out.push_str("</");
        self.out.push_str(tag);
        self.out.push_str(">\n");
    }

    /// Writes a self-closing element — one the schema requires and our
    /// documents have nothing to say in.
    pub fn empty(&mut self, tag: &str) {
        self.indent();
        self.out.push('<');
        self.out.push_str(tag);
        self.out.push_str("/>\n");
    }

    /// Writes an element with escaped text in it.
    pub fn leaf(&mut self, tag: &str, text: &str) {
        self.indent();
        self.out.push('<');
        self.out.push_str(tag);
        self.out.push('>');
        self.out.push_str(&esc(text));
        self.out.push_str("</");
        self.out.push_str(tag);
        self.out.push_str(">\n");
    }

    /// Writes an element with attributes and escaped text in it.
    pub fn leaf_with(&mut self, tag: &str, attributes: &str, text: &str) {
        self.indent();
        self.out.push('<');
        self.out.push_str(tag);
        self.out.push(' ');
        self.out.push_str(attributes);
        self.out.push('>');
        self.out.push_str(&esc(text));
        self.out.push_str("</");
        self.out.push_str(tag);
        self.out.push_str(">\n");
    }

    /// The finished document.
    #[must_use]
    pub fn finish(self) -> String {
        self.out
    }
}

impl Default for Xml {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_number_is_written_the_way_the_standard_reads_it() {
        assert_eq!(amount(0), "0.00");
        assert_eq!(amount(5), "0.05");
        assert_eq!(amount(22_688), "226.88");
        assert_eq!(amount(-22_688), "-226.88");
        assert_eq!(amount(i64::MIN), "-92233720368547758.08");

        assert_eq!(quantity(1_500), "1.5");
        assert_eq!(quantity(2_000), "2");
        assert_eq!(quantity(1), "0.001");
        assert_eq!(quantity(-1_250), "-1.25");
        assert_eq!(quantity(0), "0");

        assert_eq!(percent(2100), "21.00");
        assert_eq!(percent(0), "0.00");
        assert_eq!(percent(550), "5.50");
    }

    #[test]
    fn nothing_a_customer_typed_can_open_an_element() {
        assert_eq!(
            esc("Meier & Söhne <GmbH> \"one\" 'two'"),
            "Meier &amp; S\u{f6}hne &lt;GmbH&gt; &quot;one&quot; &apos;two&apos;"
        );
        // A control character XML 1.0 cannot represent is dropped, not encoded.
        assert_eq!(esc("a\u{0}b\tc"), "ab\tc");
        assert_eq!(esc("line\nbreak"), "line\nbreak");
    }

    #[test]
    fn the_emitter_closes_what_it_opens_at_the_depth_it_opened_it() {
        let mut xml = Xml::new();
        xml.open("a");
        xml.leaf_with("b", "c=\"1\"", "text & more");
        xml.empty("d");
        xml.close("a");
        assert_eq!(
            xml.finish(),
            "<a>\n  <b c=\"1\">text &amp; more</b>\n  <d/>\n</a>\n"
        );
    }
}
