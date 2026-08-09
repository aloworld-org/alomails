//! Writing CSV, the way every export in alo writes it (RFC 4180).
//!
//! One responsibility: **a CSV export, as bytes and as a download** — turning
//! fields into a line of a comma-separated file that a spreadsheet, a `csv`
//! library and a bookkeeper's own script all read back as the fields that went
//! in, and serving the finished file under the headers that make it one
//! ([`attachment`]). The first caller is the VAT summary (B1.20); the reports
//! that follow (B2.08, B3.08, B4.11) share both rather than each re-deriving
//! the quoting rules or the headers.
//!
//! Three rules, and they are the whole of RFC 4180 that matters here:
//!
//! - A field is quoted when it contains a comma, a double quote, a carriage
//!   return or a line feed — and left bare otherwise, because a file of
//!   needlessly quoted numbers is harder for a human to read and no easier for
//!   a parser.
//! - A double quote inside a quoted field is written twice.
//! - Records end with **CRLF**, which the RFC specifies and Excel expects; every
//!   reader accepts it, including the ones that would also have accepted LF.
//!
//! **Formula injection is the caller's rule, not this module's.** A field whose
//! text begins with `=`, `+`, `-` or `@` is evaluated as a formula by some
//! spreadsheets, so an export carrying *user-authored text* must neutralise it
//! before it gets here. Doing it in this module would corrupt the one thing our
//! exports are made of — a negative amount begins with `-` and must stay a
//! number. No caller emits user text yet; the one that does will state its own
//! rule where the text is chosen.

use axum::http::header;
use axum::response::{IntoResponse, Response};

/// The record separator RFC 4180 specifies.
const CRLF: &str = "\r\n";

/// The characters that force a field to be quoted.
const MUST_QUOTE: [char; 4] = [',', '"', '\r', '\n'];

/// One field, quoted only when it has to be.
fn field(value: &str) -> String {
    if value.contains(MUST_QUOTE) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

/// One record, CRLF-terminated.
///
/// An empty slice writes an empty record — a blank line — which is what a
/// caller asking for one means; it is never used to mean "no record".
pub fn row(fields: &[&str]) -> String {
    let mut line = String::new();
    for (at, value) in fields.iter().enumerate() {
        if at > 0 {
            line.push(',');
        }
        line.push_str(&field(value));
    }
    line.push_str(CRLF);
    line
}

/// Serves a finished file as a download, with the four headers every alo
/// export carries.
///
/// **`attachment`**, because an export exists to be saved and handed to
/// somebody rather than rendered in our origin; **`nosniff`**, so nothing
/// re-interprets the bytes as something executable; **`no-store`**, because a
/// tenant's figures are not a cacheable asset; and a **stated charset**,
/// because a CSV without one is guessed at, and a guess turns an accented
/// customer name into mojibake in a spreadsheet.
///
/// Every report route serves its file through this one function, so a header
/// added for one export cannot be missing from another.
pub fn attachment(body: String, file_name: &str) -> Response {
    (
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8".to_owned()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{file_name}\""),
            ),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_owned()),
            (header::CACHE_CONTROL, "no-store".to_owned()),
        ],
        body,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_fields_are_written_bare_and_the_record_ends_crlf() {
        assert_eq!(
            row(&["rate", "EUR", "21.00", "-1029.97"]),
            "rate,EUR,21.00,-1029.97\r\n",
            "a negative amount is a number, never quoted or neutralised"
        );
        assert_eq!(row(&["one"]), "one\r\n");
        assert_eq!(row(&[]), "\r\n", "an empty record is a blank line");
        assert_eq!(row(&["", ""]), ",\r\n", "two empty fields, not one");
    }

    #[test]
    fn a_field_that_would_break_the_grammar_is_quoted() {
        assert_eq!(row(&["Acme, GmbH"]), "\"Acme, GmbH\"\r\n");
        assert_eq!(row(&["say \"hi\""]), "\"say \"\"hi\"\"\"\r\n");
        assert_eq!(row(&["two\nlines"]), "\"two\nlines\"\r\n");
        assert_eq!(row(&["carriage\rreturn"]), "\"carriage\rreturn\"\r\n");
        // A field that is only a quote still round-trips.
        assert_eq!(row(&["\""]), "\"\"\"\"\r\n");
    }

    #[test]
    fn a_file_is_served_as_a_download_that_is_never_cached() {
        let response = attachment(row(&["net"]), "vat-2026-01-01-to-2026-03-31.csv");
        let headers = response.headers();
        for (name, expected) in [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"vat-2026-01-01-to-2026-03-31.csv\"",
            ),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (header::CACHE_CONTROL, "no-store"),
        ] {
            assert_eq!(
                headers.get(&name).and_then(|value| value.to_str().ok()),
                Some(expected),
                "{name}"
            );
        }
    }

    #[test]
    fn quoting_is_decided_per_field_not_per_record() {
        assert_eq!(
            row(&["plain", "with, comma", "plain again"]),
            "plain,\"with, comma\",plain again\r\n"
        );
    }
}
