//! Reading CSV — the file a spreadsheet exports, turned into a header and
//! rows, with the line numbers a user can find again (alo, ADR 0035).
//!
//! One responsibility: **decoding and parsing**. It knows nothing about leads,
//! deals, bank lines or anything else a column might mean; the caller maps the
//! columns it recognises ([`crate::crm_lead_import`] is the first, B2.09; the
//! bank-statement import of B4.08 is the next). It is the reading half of the
//! dialect `alo-jmap/src/csv.rs` writes, and it reads back everything that
//! module writes.
//!
//! Four decisions, each of them about a file a real person actually has.
//!
//! - **The encoding is detected, never assumed.** A byte-order mark decides
//!   first (UTF-8, UTF-16 either way — the second is what "Unicode Text" from
//!   Excel is); otherwise valid UTF-8 is UTF-8; otherwise the bytes are
//!   Windows-1252, which is what Excel on Windows writes as "CSV (Comma
//!   delimited)" and which cannot fail to decode. Refusing a file for being
//!   CP1252 would refuse half the CSVs in Europe, and guessing *silently*
//!   would put `SociÃ©tÃ©` in a customer record — so the encoding used is
//!   reported back ([`CsvTable::encoding`]) and a human can see it.
//! - **The delimiter is sniffed from the header.** A German or French Excel
//!   writes `;` because the comma is its decimal point; a "Unicode Text" export
//!   writes tabs. The candidate that yields the most header fields wins, and a
//!   tie goes to the comma — the dialect the RFC names.
//! - **A blank line is not a record.** RFC 4180 has no opinion worth following
//!   here: spreadsheets end files with them and put them between blocks, and
//!   nobody means "import an empty lead".
//! - **A row wider than its header is refused, a narrower one is padded.**
//!   Extra fields mean the file was misread (most often a decimal comma under a
//!   comma delimiter) and importing it would silently shift every value one
//!   column left; missing trailing fields are just a spreadsheet trimming
//!   empties.
//!
//! Every refusal is a [`StoreError::Validation`] naming the rule and the
//! **line**, and never quoting the file: the content may be somebody's customer
//! list (law 1), and "line 12 has more fields than the header" is what a person
//! needs anyway.

use encoding_rs::{UTF_16BE, UTF_16LE, WINDOWS_1252};

use crate::error::{Result, StoreError};

/// The most columns a file may have. A lead list has a dozen; sixty-four is a
/// misread file, not an ambitious one.
pub const CSV_MAX_COLUMNS: usize = 64;

/// The most characters one field may hold. Long enough for any address block
/// pasted into a cell, short enough that a binary file mistaken for CSV is
/// refused before it is turned into a `String` per field.
pub const CSV_MAX_FIELD_CHARS: usize = 10_000;

/// The delimiters sniffed, in the order a tie is broken.
const CANDIDATE_DELIMITERS: [char; 3] = [',', ';', '\t'];

/// How the bytes were read as text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CsvEncoding {
    /// UTF-8, with or without a byte-order mark.
    Utf8,
    /// UTF-16, little-endian (a "Unicode Text" export on Windows).
    Utf16Le,
    /// UTF-16, big-endian.
    Utf16Be,
    /// Windows-1252 — the fallback for bytes that are not valid UTF-8.
    Windows1252,
}

impl CsvEncoding {
    /// The name a report shows, so a person can tell whether their accented
    /// characters survived.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Utf8 => "utf-8",
            Self::Utf16Le => "utf-16le",
            Self::Utf16Be => "utf-16be",
            Self::Windows1252 => "windows-1252",
        }
    }
}

/// One data row, with the line of the file it came from.
#[derive(Debug, Clone)]
pub struct CsvRow {
    /// 1-based line number **in the file**, counting the header — the number a
    /// spreadsheet shows in its own gutter, so a reported row can be found.
    pub line: usize,
    /// The fields, padded to the header's width.
    pub fields: Vec<String>,
}

impl CsvRow {
    /// The trimmed field at `column`, or `""` when the column is absent — a
    /// caller mapping an optional column never has to bounds-check.
    #[must_use]
    pub fn field(&self, column: Option<usize>) -> &str {
        column
            .and_then(|at| self.fields.get(at))
            .map_or("", |value| value.trim())
    }

    /// Whether every field of the row is blank.
    #[must_use]
    pub fn is_blank(&self) -> bool {
        self.fields.iter().all(|f| f.trim().is_empty())
    }
}

/// A parsed file: how it was read, what its columns are called, and its rows.
#[derive(Debug, Clone)]
pub struct CsvTable {
    /// How the bytes were decoded.
    pub encoding: CsvEncoding,
    /// The delimiter that was sniffed.
    pub delimiter: char,
    /// The header names, trimmed, in file order.
    pub header: Vec<String>,
    /// Every non-blank data row.
    pub rows: Vec<CsvRow>,
}

impl CsvTable {
    /// The index of the column called `name`, matched **case- and
    /// space-insensitively** — a header a human typed is `E-Mail` as often as
    /// `email`, and no import should turn on that.
    #[must_use]
    pub fn column(&self, name: &str) -> Option<usize> {
        let wanted = fold(name);
        self.header.iter().position(|got| fold(got) == wanted)
    }
}

/// The comparison key for a header name: lowercased, with spaces, hyphens,
/// underscores and dots removed. `E-mail address` and `email_address` are one
/// column.
fn fold(name: &str) -> String {
    name.chars()
        .filter(|c| !matches!(c, ' ' | '-' | '_' | '.' | '\u{a0}'))
        .flat_map(char::to_lowercase)
        .collect()
}

/// Reads `bytes` as CSV, refusing a file that is not one.
///
/// `max_rows` bounds the **data** rows; the header does not count against it.
///
/// # Errors
/// [`StoreError::Validation`] when the file holds no rows at all, has two
/// columns with the same name, has more than [`CSV_MAX_COLUMNS`] columns, holds
/// a field longer than [`CSV_MAX_FIELD_CHARS`], has a row wider than its
/// header, or holds more than `max_rows` rows.
pub fn parse(bytes: &[u8], max_rows: usize) -> Result<CsvTable> {
    let (text, encoding) = decode(bytes);
    let delimiter = sniff_delimiter(&text);
    // One past the cap, so the cap is reported as the cap and not as a file
    // that happens to end there.
    let records = records(&text, delimiter, max_rows.saturating_add(2))?;
    let mut records = records.into_iter();
    let header_record = records.next().ok_or_else(|| {
        StoreError::Validation("the file holds no rows; the first row must be a header".to_owned())
    })?;
    let header: Vec<String> = header_record
        .fields
        .iter()
        .map(|name| name.trim().to_owned())
        .collect();
    if header.len() > CSV_MAX_COLUMNS {
        return Err(StoreError::Validation(format!(
            "the file has more than {CSV_MAX_COLUMNS} columns; check that it is a table"
        )));
    }
    check_unique(&header)?;
    let mut rows = Vec::new();
    for mut record in records {
        if record.fields.len() > header.len() {
            return Err(StoreError::Validation(format!(
                "line {} has more fields than the header; check the file's delimiter",
                record.line
            )));
        }
        record.fields.resize(header.len(), String::new());
        rows.push(record);
        if rows.len() > max_rows {
            return Err(StoreError::Validation(format!(
                "the file holds more than {max_rows} rows; split it"
            )));
        }
    }
    Ok(CsvTable {
        encoding,
        delimiter,
        header,
        rows,
    })
}

/// Refuses two columns a mapping could not tell apart. Blank names are not
/// compared: an export often ends in an unnamed empty column, and two of those
/// name nothing at all.
fn check_unique(header: &[String]) -> Result<()> {
    let mut seen: Vec<String> = Vec::with_capacity(header.len());
    for name in header.iter().filter(|name| !name.is_empty()) {
        let key = fold(name);
        if seen.contains(&key) {
            return Err(StoreError::Validation(
                "two columns have the same name; rename one before importing".to_owned(),
            ));
        }
        seen.push(key);
    }
    Ok(())
}

/// Decodes the bytes, reporting which way it read them.
///
/// Shared with [`crate::bank_mt940`]: an MT940 file is not CSV, but it arrives
/// from the same portals in the same encodings, and one notion of "how European
/// bookkeeping software writes a text file" is better than two.
pub(crate) fn decode(bytes: &[u8]) -> (String, CsvEncoding) {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return (
            String::from_utf8_lossy(rest).into_owned(),
            CsvEncoding::Utf8,
        );
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        let (text, _) = UTF_16LE.decode_without_bom_handling(rest);
        return (text.into_owned(), CsvEncoding::Utf16Le);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        let (text, _) = UTF_16BE.decode_without_bom_handling(rest);
        return (text.into_owned(), CsvEncoding::Utf16Be);
    }
    match std::str::from_utf8(bytes) {
        Ok(text) => (text.to_owned(), CsvEncoding::Utf8),
        Err(_) => {
            let (text, _) = WINDOWS_1252.decode_without_bom_handling(bytes);
            (text.into_owned(), CsvEncoding::Windows1252)
        }
    }
}

/// The delimiter that reads the header as the most fields.
///
/// Sniffed by parsing the first record with each candidate rather than by
/// counting characters, so a delimiter inside a quoted header name ("Company,
/// legal") does not vote. Only that first record is parsed, three times — not
/// the file.
fn sniff_delimiter(text: &str) -> char {
    let mut best = (CANDIDATE_DELIMITERS[0], 0);
    for candidate in CANDIDATE_DELIMITERS {
        let fields = records(text, candidate, 1)
            .ok()
            .and_then(|records| records.into_iter().next())
            .map_or(0, |first| first.fields.len());
        if fields > best.1 {
            best = (candidate, fields);
        }
    }
    best.0
}

/// The refusal a field over [`CSV_MAX_FIELD_CHARS`] gets, from either of the
/// two places that can see it.
fn field_too_long(line: usize) -> StoreError {
    StoreError::Validation(format!(
        "line {line} holds a field longer than {CSV_MAX_FIELD_CHARS} characters; check that this \
         is a text file"
    ))
}

/// Ends a field, holding it to [`CSV_MAX_FIELD_CHARS`].
fn take_field(field: &mut String, line: usize) -> Result<String> {
    if field.chars().count() > CSV_MAX_FIELD_CHARS {
        return Err(field_too_long(line));
    }
    Ok(std::mem::take(field))
}

/// The state machine: RFC 4180 records, with CRLF, LF or CR as the terminator
/// and blank lines skipped. Stops once `limit` records have been read, so the
/// delimiter sniff costs one record and not one file.
fn records(text: &str, delimiter: char, limit: usize) -> Result<Vec<CsvRow>> {
    let mut out: Vec<CsvRow> = Vec::new();
    let mut fields: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut line = 1_usize;
    let mut record_line = 1_usize;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            match c {
                '"' => {
                    if chars.peek() == Some(&'"') {
                        chars.next();
                        field.push('"');
                    } else {
                        quoted = false;
                    }
                }
                _ => {
                    if c == '\n' {
                        line += 1;
                    }
                    field.push(c);
                }
            }
        } else if c == '"' && field.is_empty() {
            // A quote opens a field only at its start; one in the middle of
            // bare text is a character somebody typed.
            quoted = true;
        } else if c == delimiter {
            let done = take_field(&mut field, record_line)?;
            fields.push(done);
        } else if c == '\r' || c == '\n' {
            if c == '\r' && chars.peek() == Some(&'\n') {
                chars.next();
            }
            let done = take_field(&mut field, record_line)?;
            fields.push(done);
            end_record(&mut out, &mut fields, record_line);
            line += 1;
            record_line = line;
            if out.len() >= limit {
                return Ok(out);
            }
        } else {
            field.push(c);
        }
        // A cheap upper bound on the exact check `take_field` makes: a string
        // of `n` characters is at most `4n` bytes, so this never refuses a
        // field that would have been accepted — it only stops a binary file
        // from being accumulated into one enormous `String` before it is.
        if field.len() > CSV_MAX_FIELD_CHARS * 4 {
            return Err(field_too_long(record_line));
        }
    }
    if !field.is_empty() || !fields.is_empty() {
        let done = take_field(&mut field, record_line)?;
        fields.push(done);
        end_record(&mut out, &mut fields, record_line);
    }
    Ok(out)
}

/// Ends a record, dropping it when every field of it is blank.
fn end_record(out: &mut Vec<CsvRow>, fields: &mut Vec<String>, line: usize) {
    let row = CsvRow {
        line,
        fields: std::mem::take(fields),
    };
    if !row.is_blank() {
        out.push(row);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn table(text: &str) -> CsvTable {
        parse(text.as_bytes(), 1_000).expect("readable CSV")
    }

    fn rule(text: &str) -> String {
        match parse(text.as_bytes(), 1_000) {
            Err(StoreError::Validation(rule)) => rule,
            other => panic!("expected a validation refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_plain_comma_file_reads_back_as_its_fields() {
        let table = table("Company,Email\r\nAcme GmbH,ada@acme.example\r\n");
        assert_eq!(table.delimiter, ',');
        assert_eq!(table.encoding, CsvEncoding::Utf8);
        assert_eq!(table.header, ["Company", "Email"]);
        assert_eq!(table.rows.len(), 1);
        assert_eq!(table.rows[0].line, 2, "the line a spreadsheet shows");
        assert_eq!(table.rows[0].fields, ["Acme GmbH", "ada@acme.example"]);
    }

    #[test]
    fn quoting_survives_the_round_trip_our_own_writer_makes() {
        // Exactly what `alo-jmap/src/csv.rs` would have written.
        let table = table("Name,Note\r\n\"Acme, GmbH\",\"say \"\"hi\"\"\"\r\n");
        assert_eq!(table.rows[0].fields, ["Acme, GmbH", "say \"hi\""]);
    }

    #[test]
    fn a_newline_inside_a_quoted_field_is_part_of_the_field() {
        let table = table("Name,Address\r\nAcme,\"One Street\r\nBerlin\"\r\nBeta,Two Street\r\n");
        assert_eq!(table.rows.len(), 2);
        assert_eq!(table.rows[0].fields[1], "One Street\r\nBerlin");
        assert_eq!(
            table.rows[1].line, 4,
            "the wrapped field moved the next line down"
        );
    }

    #[test]
    fn the_delimiter_is_sniffed_and_a_tie_goes_to_the_comma() {
        assert_eq!(table("a;b;c\r\n1;2;3\r\n").delimiter, ';');
        assert_eq!(table("a\tb\tc\n1\t2\t3\n").delimiter, '\t');
        assert_eq!(table("a,b\n1,2\n").delimiter, ',');
        // One field either way: nothing to tell them apart, so the RFC's
        // dialect wins.
        assert_eq!(table("Company\nAcme\n").delimiter, ',');
        // A delimiter inside a quoted header name does not vote.
        assert_eq!(
            table("\"Company, legal\";Email\nAcme;a@b.example\n").delimiter,
            ';'
        );
    }

    #[test]
    fn every_terminator_a_spreadsheet_writes_ends_a_record() {
        for text in [
            "a,b\r\n1,2\r\n",
            "a,b\n1,2\n",
            "a,b\r1,2\r",
            "a,b\n1,2",
            "a,b\n\n\n1,2\n\n",
        ] {
            let table = table(text);
            assert_eq!(table.rows.len(), 1, "{text:?}");
            assert_eq!(table.rows[0].fields, ["1", "2"], "{text:?}");
        }
    }

    #[test]
    fn a_short_row_is_padded_and_a_wide_one_is_refused() {
        let table = table("a,b,c\n1,2\n");
        assert_eq!(table.rows[0].fields, ["1", "2", ""]);
        // The classic misread: a decimal comma under a comma delimiter.
        assert!(rule("a,b\n1,2,3\n").contains("line 2"));
        assert!(rule("a,b\n1,2,3\n").contains("more fields than the header"));
    }

    #[test]
    fn a_file_with_no_rows_at_all_is_refused() {
        assert!(rule("").contains("no rows"));
        assert!(rule("\n\n").contains("no rows"));
    }

    #[test]
    fn a_blank_first_line_is_skipped_and_the_next_one_is_the_header() {
        // A row of empty cells is a blank line however a spreadsheet wrote it,
        // so the header is the first line that says something. What we read as
        // the header is always reported back to the caller, which is what makes
        // this safe: a person sees the columns we found before anything is
        // imported under them.
        let table = table("\n,,\nCompany,Contact,Email\nAcme,Ada,ada@acme.example\n");
        assert_eq!(table.header, ["Company", "Contact", "Email"]);
        assert_eq!(table.rows.len(), 1);
        assert_eq!(table.rows[0].line, 4);
    }

    #[test]
    fn two_columns_with_one_name_are_refused_but_blank_ones_are_not() {
        assert!(rule("Email,E-Mail\na@b.example,c@d.example\n").contains("same name"));
        assert!(rule("Email,email\na,b\n").contains("same name"));
        let table = table("Company,,\nAcme,,\n");
        assert_eq!(table.header.len(), 3, "two unnamed columns name nothing");
    }

    #[test]
    fn the_caps_are_enforced() {
        let wide: String = (0..CSV_MAX_COLUMNS + 1)
            .map(|at| format!("c{at}"))
            .collect::<Vec<_>>()
            .join(",");
        assert!(rule(&format!("{wide}\n")).contains("more than"));
        let long = "x".repeat(CSV_MAX_FIELD_CHARS + 1);
        assert!(rule(&format!("a\n{long}\n")).contains("longer than"));
        let many = format!("a\n{}", "1\n".repeat(5));
        match parse(many.as_bytes(), 4) {
            Err(StoreError::Validation(rule)) => assert!(rule.contains("more than 4 rows")),
            other => panic!("expected the row cap, got {other:?}"),
        }
    }

    #[test]
    fn a_column_is_found_however_its_header_was_typed() {
        let table = table("Contact E-Mail,Company Name\na@b.example,Acme\n");
        assert_eq!(table.column("contactemail"), Some(0));
        assert_eq!(table.column("Company_name"), Some(1));
        assert_eq!(table.column("phone"), None);
        assert_eq!(
            table.rows[0].field(table.column("contactemail")),
            "a@b.example"
        );
        assert_eq!(
            table.rows[0].field(None),
            "",
            "an unmapped column reads blank"
        );
    }

    #[test]
    fn each_encoding_a_spreadsheet_writes_is_read_as_itself() {
        let mut utf8_bom = vec![0xEF, 0xBB, 0xBF];
        utf8_bom.extend_from_slice("Company\nSociété\n".as_bytes());
        let table = parse(&utf8_bom, 100).unwrap();
        assert_eq!(table.encoding, CsvEncoding::Utf8);
        assert_eq!(table.rows[0].fields[0], "Société");

        // Windows-1252: 0xE9 is é, and is not valid UTF-8 on its own.
        let cp1252 = b"Company\nSoci\xE9t\xE9\n";
        let table = parse(cp1252, 100).unwrap();
        assert_eq!(table.encoding, CsvEncoding::Windows1252);
        assert_eq!(table.rows[0].fields[0], "Société");

        let mut utf16 = vec![0xFF, 0xFE];
        for unit in "Company\nSociété\n".encode_utf16() {
            utf16.extend_from_slice(&unit.to_le_bytes());
        }
        let table = parse(&utf16, 100).unwrap();
        assert_eq!(table.encoding, CsvEncoding::Utf16Le);
        assert_eq!(table.rows[0].fields[0], "Société");
    }
}
