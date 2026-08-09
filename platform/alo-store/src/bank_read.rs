//! One door for three bank formats (alo Finance, ADR 0035, wave B4.08c;
//! `docs/design/finance.md`, "The bank and reconciliation").
//!
//! A person has a file. They do not have a format — they have whatever their
//! bank's portal offered on the day, and half the time they could not tell you
//! which of the three it is. So this module answers the question for them:
//! [`sniff_bank_source`] reads the first bytes, [`read_bank_file`] produces the
//! same [`BankFileReading`] whichever parser ran, and
//! [`AccountStore::import_bank_file`] stages it.
//!
//! # The reading is the preview, and the preview writes nothing
//!
//! [`read_bank_file`] is a **pure function**. It takes no store, touches no
//! database and therefore cannot write — which is the strongest form the
//! promise "the preview writes nothing" can take: not a rule somebody has to
//! keep, but a thing that has no way to happen. The commit is the same reading
//! plus one staging call, so a preview cannot promise a statement the commit
//! then reads differently.
//!
//! # Nothing is imported halfway
//!
//! A row that cannot be read is a [`RowError`] naming its line, and one of
//! those means the whole file stages nothing ([`BankFileImport::imported`] is
//! `None`). A statement half in the books is worse than one not in them: the
//! missing transactions are invisible, while a refused file is a thing a person
//! can see and fix. The report names every broken line at once, so the fix is
//! one pass rather than a row at a time.
//!
//! The counts a person actually reads live in [`BankImport`] — how many lines
//! were staged, and how many were already there from an overlapping file.

use crate::account::AccountStore;
use crate::bank_csv::{BankCsvDates, BankCsvDecimal, BankCsvMapping, read_csv_statement};
use crate::bank_import::{
    BankImport, BankSource, MAX_BANK_FILE_BYTES, ParsedStatement, STATEMENT_LINES_MAX, sha256_hex,
};
use crate::csv_read::{RowError, parse as parse_csv};
use crate::error::{Result, StoreError};

/// The bytes a sniff looks at. A CAMT file's root element and an MT940's first
/// tag are both within the first few lines; a covering note above them is not
/// longer than this in any file we have seen.
const SNIFF_BYTES: usize = 4_096;

/// What a caller states about the file it is uploading.
///
/// Everything here is about the file *as a whole*, and everything is optional
/// but the account — the two conventions and the mapping only mean anything for
/// a CSV, and a CAMT or MT940 file states all of it itself.
#[derive(Debug, Clone, Default)]
pub struct BankImportRequest {
    /// Which parser to use, or `None` to read it from the file.
    pub source: Option<BankSource>,
    /// The account this file is the statement of. Required for a CSV, which
    /// names no account; a **guard** for the other two, which do — a file that
    /// names a different account is refused rather than filed under this one.
    pub account_iban: String,
    /// The statement's currency; the tenant's default when unstated. Only a CSV
    /// needs it.
    pub currency: Option<String>,
    /// How this file writes dates (CSV only).
    pub dates: BankCsvDates,
    /// How this file writes decimals (CSV only).
    pub decimal: BankCsvDecimal,
    /// Which column is which (CSV only). Empty means "guess from the header".
    pub mapping: BankCsvMapping,
}

/// What a file says, before anything is written.
#[derive(Debug, Clone)]
pub struct BankFileReading {
    /// Which parser read it.
    pub source: BankSource,
    /// How the bytes were decoded, when the format is text a person exported
    /// (CSV). `None` for the two formats whose own reader owns the question.
    pub encoding: Option<&'static str>,
    /// The delimiter that was sniffed (CSV).
    pub delimiter: Option<char>,
    /// The header, in file order — what a wizard builds its column picker from.
    pub columns: Vec<String>,
    /// The mapping actually used: the caller's, or the header's own guess.
    pub mapping: BankCsvMapping,
    /// The date order actually used.
    pub dates: BankCsvDates,
    /// The decimal convention used.
    pub decimal: BankCsvDecimal,
    /// How many rows (or entries) the file stated.
    pub total_rows: usize,
    /// The file line each transaction came from, parallel to the statement's
    /// lines. Empty for the two formats that are not line-per-row.
    pub at: Vec<usize>,
    /// Rows that were blank in every mapped column — counted, because a person
    /// told "11 of 12" must be able to find the twelfth.
    pub skipped: Vec<usize>,
    /// Rows that cannot be read. One of these and the file stages nothing.
    pub errors: Vec<RowError>,
    /// The statement these rows make, or `None` when a row could not be read.
    pub statement: Option<ParsedStatement>,
}

impl BankFileReading {
    /// The reading of a file whose parser answered on its own — CAMT.053 or
    /// MT940, neither of which has columns, a mapping or a convention to state.
    fn from_parsed(source: BankSource, statement: ParsedStatement) -> Self {
        Self {
            source,
            encoding: None,
            delimiter: None,
            columns: Vec::new(),
            mapping: BankCsvMapping::default(),
            dates: BankCsvDates::Ymd,
            decimal: BankCsvDecimal::Auto,
            total_rows: statement.lines.len() + statement.unbooked,
            at: Vec::new(),
            skipped: Vec::new(),
            errors: Vec::new(),
            statement: Some(statement),
        }
    }
}

/// What an import did, or would have done.
#[derive(Debug, Clone)]
pub struct BankFileImport {
    /// What the file said.
    pub reading: BankFileReading,
    /// The staged statement and its counts, or `None` when a row could not be
    /// read and therefore **nothing** was written.
    pub imported: Option<BankImport>,
}

/// Which parser a file wants, read from its first bytes.
///
/// Deliberately three cheap questions rather than a validation: a file that
/// sniffs as CAMT and is not one gets the CAMT reader's own refusal, which
/// names the element that is missing — a better answer than "this is not a
/// bank file".
#[must_use]
pub fn sniff_bank_source(file: &[u8]) -> BankSource {
    let head = &file[..file.len().min(SNIFF_BYTES)];
    let text = String::from_utf8_lossy(head);
    let body = text.trim_start_matches('\u{feff}');
    if body.trim_start().starts_with('<') {
        return BankSource::Camt;
    }
    // An MT940 statement is a sequence of `:nn:` tags, sometimes inside SWIFT's
    // `{1:}{2:}{4:}` transport blocks and often below a bank's covering prose.
    // A CSV row does not begin with one.
    if body.trim_start().starts_with("{1:") || body.lines().any(is_mt940_tag) {
        return BankSource::Mt940;
    }
    BankSource::Csv
}

/// Whether a line begins with an MT940 field tag (`:20:`, `:61:`, `:62F:`).
fn is_mt940_tag(line: &str) -> bool {
    let rest = match line.trim_start().strip_prefix(':') {
        Some(rest) => rest,
        None => return false,
    };
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    if digits.len() != 2 {
        return false;
    }
    let tail = &rest[digits.len()..];
    let tail = tail
        .strip_prefix(|c: char| c.is_ascii_uppercase())
        .unwrap_or(tail);
    tail.starts_with(':')
}

/// Reads an uploaded statement file, whichever of the three formats it is.
///
/// Pure: it writes nothing and can write nothing, which is what makes it the
/// preview as well as the first half of the import.
///
/// # Errors
/// [`StoreError::Validation`] when the file is empty, larger than
/// [`MAX_BANK_FILE_BYTES`], not readable as the format it claims to be, the
/// statement of a **different account** than the caller named, or (for a CSV)
/// mapped to a column it has not got. A row that is merely unreadable is a
/// [`RowError`] in the reading rather than an error here.
pub fn read_bank_file(request: &BankImportRequest, file: &[u8]) -> Result<BankFileReading> {
    if file.is_empty() {
        return Err(StoreError::Validation("the file is empty".to_owned()));
    }
    if file.len() > MAX_BANK_FILE_BYTES {
        return Err(StoreError::Validation(format!(
            "a statement file may be at most {} MiB; split it by period",
            MAX_BANK_FILE_BYTES / (1024 * 1024)
        )));
    }
    let source = request.source.unwrap_or_else(|| sniff_bank_source(file));
    let reading = match source {
        BankSource::Camt => {
            BankFileReading::from_parsed(source, crate::bank_camt::parse_camt053(file)?)
        }
        BankSource::Mt940 => {
            BankFileReading::from_parsed(source, crate::bank_mt940::parse_mt940(file)?)
        }
        BankSource::Csv => read_csv(request, file)?,
    };
    guard_account(request, &reading)?;
    Ok(reading)
}

/// The CSV half: the file as a table, then the mapping over it.
fn read_csv(request: &BankImportRequest, file: &[u8]) -> Result<BankFileReading> {
    if crate::iban::canonicalize(&request.account_iban)
        .ok()
        .flatten()
        .is_none()
    {
        return Err(StoreError::Validation(
            "a CSV export does not say which account it is of, so state the account's IBAN with \
             the file"
                .to_owned(),
        ));
    }
    let table = parse_csv(file, STATEMENT_LINES_MAX)?;
    let reading = read_csv_statement(&table, request)?;
    Ok(BankFileReading {
        source: BankSource::Csv,
        encoding: Some(table.encoding.as_str()),
        delimiter: Some(table.delimiter),
        columns: table.header,
        mapping: reading.mapping,
        dates: reading.dates,
        decimal: reading.decimal,
        total_rows: table.rows.len(),
        at: reading.at,
        skipped: reading.skipped,
        errors: reading.errors,
        statement: reading.statement,
    })
}

/// Refuses a file that is the statement of an account other than the one the
/// caller named.
///
/// The wrong file uploaded to the right screen is an ordinary mistake, and it
/// is one that survives: the lines stage cleanly, reconcile against nothing,
/// and are found weeks later. A CAMT or MT940 file names its own account, so
/// the check costs one comparison — and is skipped when the caller named
/// nothing, because for those two formats the account is not required.
fn guard_account(request: &BankImportRequest, reading: &BankFileReading) -> Result<()> {
    let (Some(statement), Ok(Some(stated))) = (
        reading.statement.as_ref(),
        crate::iban::canonicalize(&request.account_iban),
    ) else {
        return Ok(());
    };
    let named = crate::iban::canonicalize(&statement.account_iban)
        .ok()
        .flatten();
    if named.is_some_and(|named| named != stated) {
        return Err(StoreError::Validation(
            "this file is the statement of a different account than the one it was uploaded for"
                .to_owned(),
        ));
    }
    Ok(())
}

impl AccountStore {
    /// Imports an uploaded statement file of any of the three formats.
    ///
    /// The single door a route uses: sniffing, parsing, the mapping, validation,
    /// the duplicate rules and the write are one call, so no caller can perform
    /// half of them. A file with an unreadable row writes **nothing** and comes
    /// back with [`BankFileImport::imported`] as `None` and every broken line
    /// named.
    ///
    /// # Errors
    /// As [`read_bank_file`]; [`StoreError::Conflict`] when these exact bytes
    /// have already been imported; [`StoreError::Db`] on failure.
    pub async fn import_bank_file(
        &self,
        request: &BankImportRequest,
        file: &[u8],
    ) -> Result<BankFileImport> {
        let reading = read_bank_file(request, file)?;
        let Some(statement) = reading
            .statement
            .as_ref()
            .filter(|_| reading.errors.is_empty())
        else {
            return Ok(BankFileImport {
                reading,
                imported: None,
            });
        };
        let imported = self
            .stage_bank_statement(statement, &sha256_hex(file))
            .await?;
        Ok(BankFileImport {
            reading,
            imported: Some(imported),
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    fn request() -> BankImportRequest {
        BankImportRequest {
            account_iban: "DE02120300000000202051".to_owned(),
            ..BankImportRequest::default()
        }
    }

    #[test]
    fn the_three_formats_are_told_apart_by_their_first_bytes() {
        assert_eq!(
            sniff_bank_source(b"<?xml version=\"1.0\"?><Document/>"),
            BankSource::Camt
        );
        assert_eq!(
            sniff_bank_source("\u{feff}<Document/>".as_bytes()),
            BankSource::Camt,
            "a byte-order mark is not a column"
        );
        assert_eq!(
            sniff_bank_source(b"{1:F01BANKDEFF}{2:O940}{4:\r\n:20:STMT\r\n"),
            BankSource::Mt940
        );
        assert_eq!(
            sniff_bank_source(b"Kontoauszug Januar 2026\r\n:20:STMT-1\r\n:25:DE02\r\n"),
            BankSource::Mt940,
            "a bank's covering note is above the first tag, not instead of it"
        );
        assert_eq!(
            sniff_bank_source(b"Date;Amount;Description\n2026-01-05;10,00;Kaffee\n"),
            BankSource::Csv
        );
        assert_eq!(
            sniff_bank_source(b"Time;Amount\n12:30:00;10.00\n"),
            BankSource::Csv,
            "a clock in a cell is not a field tag"
        );
    }

    #[test]
    fn an_empty_or_oversized_file_is_refused_before_it_is_parsed() {
        let refused = read_bank_file(&request(), b"").expect_err("empty");
        assert!(matches!(refused, StoreError::Validation(_)), "{refused:?}");
        let big = vec![b'a'; MAX_BANK_FILE_BYTES + 1];
        let refused = read_bank_file(&request(), &big).expect_err("oversized");
        assert!(
            matches!(&refused, StoreError::Validation(message) if message.contains("MiB")),
            "{refused:?}"
        );
    }

    #[test]
    fn a_csv_without_an_account_is_refused_with_what_to_state() {
        let file = b"Date,Amount\n2026-01-05,10.00\n";
        let refused = read_bank_file(&BankImportRequest::default(), file).expect_err("no account");
        assert!(
            matches!(&refused, StoreError::Validation(message)
                if message.contains("state the account's IBAN")),
            "{refused:?}"
        );
    }

    #[test]
    fn a_csv_reads_into_the_same_shape_the_other_two_produce() {
        let file = b"Date,Amount,Description\n2026-01-05,10.00,Kaffee\n";
        let reading = read_bank_file(&request(), file).expect("a reading");
        assert_eq!(reading.source, BankSource::Csv);
        assert_eq!(reading.encoding, Some("utf-8"));
        assert_eq!(reading.delimiter, Some(','));
        assert_eq!(reading.columns.len(), 3);
        assert_eq!(reading.total_rows, 1);
        assert_eq!(reading.at, vec![2]);
        let statement = reading.statement.expect("a statement");
        assert_eq!(statement.account_iban, "DE02120300000000202051");
        assert_eq!(statement.lines[0].amount_cents, 1_000);
    }
}
