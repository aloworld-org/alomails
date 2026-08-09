//! The CSV mapping wizard against whole files (B4.08c) — three exports in the
//! shapes European bank portals actually write, read with no database in sight.
//!
//! A golden file here is not "a file that parses". Two of the three assert
//! something a parser cannot fake:
//!
//! - `csv_de_january.csv` is the **same month** as `camt053_de_january.xml` and
//!   `mt940_de_january.sta`, transaction for transaction, in a third format —
//!   Windows-1252, semicolons, dotted dates, comma decimals, a trailing minus's
//!   European cousin. Every field the line hash is built from is asserted equal
//!   to the CAMT reading, which is what makes the store de-duplicate the three
//!   against each other (`bank_import_tenancy.rs` then proves it does).
//! - `csv_uk_february.csv` is the other half of the world: comma-delimited, ISO
//!   dates, a dot decimal, and money split across a paid-out and a paid-in
//!   column with a running balance the mapping ignores. Its last row is a
//!   footer — blank in every mapped column — and is skipped rather than
//!   refused.
//! - `csv_broken_rows.csv` is what a person actually uploads on a Tuesday: two
//!   rows nobody can read. It stages nothing at all and names both lines.
//!
//! All IBANs are the specifications' own test numbers. The files are
//! hand-authored to the published shapes rather than copied from a bank's own
//! export: a real statement is somebody's money.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alo_store::bank_read::{BankFileReading, BankImportRequest, read_bank_file};
use alo_store::{BankCsvDates, BankCsvDecimal, BankSource, ParsedStatement, parse_camt053};
use time::{Date, Month};

const GERMAN_ACCOUNT: &str = "DE02120300000000202051";
const BRITISH_ACCOUNT: &str = "GB33BUKB20201555555555";

fn fixture(name: &str) -> Vec<u8> {
    let path = format!("{}/tests/fixtures/bank/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&path).unwrap_or_else(|error| panic!("read {path}: {error}"))
}

fn day(year: i32, month: Month, day: u8) -> Date {
    Date::from_calendar_date(year, month, day).unwrap()
}

/// A request naming only the account — everything else is what the wizard
/// guesses, which is the case a person should never have to leave.
fn for_account(account: &str) -> BankImportRequest {
    BankImportRequest {
        account_iban: account.to_owned(),
        ..BankImportRequest::default()
    }
}

fn read(name: &str, request: &BankImportRequest) -> BankFileReading {
    read_bank_file(request, &fixture(name)).expect("a readable file")
}

#[test]
fn a_german_export_is_read_by_its_header_alone() {
    let reading = read("csv_de_january.csv", &for_account(GERMAN_ACCOUNT));

    assert_eq!(reading.source, BankSource::Csv, "sniffed, not stated");
    assert_eq!(
        reading.encoding,
        Some("windows-1252"),
        "half of Europe's portals still write it, umlauts and all"
    );
    assert_eq!(reading.delimiter, Some(';'));
    assert_eq!(
        reading.dates,
        BankCsvDates::Dmy,
        "a dot is never month-first"
    );
    assert_eq!(reading.decimal, BankCsvDecimal::Auto);
    assert_eq!(reading.total_rows, 4);
    assert!(reading.errors.is_empty(), "{:?}", reading.errors);
    assert!(reading.skipped.is_empty());
    assert_eq!(reading.at, vec![2, 3, 4, 5], "the spreadsheet's own gutter");

    // The guess, in the file's own words — what a wizard shows for correction.
    assert_eq!(reading.mapping.booked_on.as_deref(), Some("Buchungstag"));
    assert_eq!(reading.mapping.value_on.as_deref(), Some("Wertstellung"));
    assert_eq!(reading.mapping.amount.as_deref(), Some("Betrag"));
    assert_eq!(reading.mapping.currency.as_deref(), Some("Währung"));
    assert_eq!(
        reading.mapping.counterparty_name.as_deref(),
        Some("Empfänger"),
        "the umlaut survived the decoding, or this column would not match"
    );
    assert_eq!(reading.mapping.bank_ref.as_deref(), Some("Kundenreferenz"));
    assert_eq!(reading.mapping.debit, None, "one signed column, not two");

    let statement = reading.statement.expect("a statement");
    assert_eq!(statement.account_iban, GERMAN_ACCOUNT);
    assert_eq!(statement.currency, "EUR");
    assert_eq!(statement.from_date, day(2026, Month::January, 5));
    assert_eq!(statement.to_date, day(2026, Month::January, 29));
    assert_eq!(
        (
            statement.statement_ref.as_str(),
            statement.opening_balance_cents,
            statement.closing_balance_cents
        ),
        ("", None, None),
        "a CSV export names no statement and states no balances: absent, not zero"
    );

    let paid = &statement.lines[0];
    assert_eq!(paid.amount_cents, 125_000);
    assert_eq!(paid.booked_on, day(2026, Month::January, 5));
    assert_eq!(paid.value_on, day(2026, Month::January, 4));
    assert_eq!(paid.counterparty_name, "Kaffeehaus Berlin GmbH");
    assert_eq!(paid.counterparty_iban, "NL91ABNA0417164300");
    assert_eq!(paid.remittance, "Rechnung INV-2026-00007 vielen Dank");
    assert_eq!(paid.bank_ref, "DE-2026-0105-0001");

    let utility = &statement.lines[1];
    assert_eq!(utility.amount_cents, -8_990, "a leading minus is money out");

    let payroll = &statement.lines[2];
    assert_eq!(payroll.amount_cents, -450_000, "1.234,56 read exactly");
    assert_eq!(payroll.counterparty_iban, "", "a batch names no account");
}

#[test]
fn the_same_january_in_a_third_format_is_the_same_january() {
    let camt: ParsedStatement = parse_camt053(&fixture("camt053_de_january.xml")).unwrap();
    let csv = read("csv_de_january.csv", &for_account(GERMAN_ACCOUNT))
        .statement
        .expect("a statement");

    assert_eq!(camt.account_iban, csv.account_iban);
    assert_eq!(camt.currency, csv.currency);
    assert_eq!(camt.lines.len(), csv.lines.len());
    // Every field the line hash is built from. A spreadsheet states no
    // balances and no period of its own, so those are deliberately not
    // compared — the transactions are what must be identical, because they are
    // what the store de-duplicates on.
    for (from_xml, from_sheet) in camt.lines.iter().zip(&csv.lines) {
        assert_eq!(from_xml.booked_on, from_sheet.booked_on);
        assert_eq!(from_xml.amount_cents, from_sheet.amount_cents);
        assert_eq!(from_xml.currency, from_sheet.currency);
        assert_eq!(from_xml.bank_ref, from_sheet.bank_ref);
        assert_eq!(from_xml.counterparty_iban, from_sheet.counterparty_iban);
        assert_eq!(from_xml.remittance, from_sheet.remittance);
    }
}

#[test]
fn a_british_export_splits_its_money_over_two_columns() {
    let reading = read("csv_uk_february.csv", &for_account(BRITISH_ACCOUNT));

    assert_eq!(reading.encoding, Some("utf-8"));
    assert_eq!(reading.delimiter, Some(','));
    assert_eq!(reading.dates, BankCsvDates::Ymd);
    assert_eq!(reading.total_rows, 4, "the footer is a row of the file");
    assert!(reading.errors.is_empty(), "{:?}", reading.errors);
    assert_eq!(
        reading.skipped,
        vec![5],
        "a running-balance footer is blank in every mapped column, and is not a mistake"
    );
    assert_eq!(reading.mapping.debit.as_deref(), Some("Paid out"));
    assert_eq!(reading.mapping.credit.as_deref(), Some("Paid in"));
    assert_eq!(reading.mapping.amount, None, "there is no signed column");
    assert_eq!(
        reading.mapping.counterparty_iban.as_deref(),
        Some("Counterparty IBAN"),
        "the more specific column wins over the bare one"
    );

    let statement = reading.statement.expect("a statement");
    assert_eq!(statement.lines.len(), 3);
    assert_eq!(
        statement.lines[0].amount_cents, -340,
        "paid out is money out"
    );
    assert_eq!(
        statement.lines[1].amount_cents, 120_000,
        "paid in is money in"
    );
    assert_eq!(statement.lines[2].amount_cents, -80_000);
    assert_eq!(statement.from_date, day(2026, Month::February, 3));
    assert_eq!(statement.to_date, day(2026, Month::February, 27));
    assert_eq!(
        statement.currency, "EUR",
        "the tenant's default, because the caller stated none and the file has no currency column"
    );
    assert_eq!(
        statement.lines[1].remittance, "Invoice INV-2026-00011",
        "the description is what B4.09 will search our invoice numbers in"
    );
}

#[test]
fn a_file_with_rows_nobody_can_read_stages_none_of_it() {
    let reading = read("csv_broken_rows.csv", &for_account(GERMAN_ACCOUNT));

    assert!(
        reading.statement.is_none(),
        "one broken row and the file stages nothing: a statement half in the books hides the \
         half that is not"
    );
    assert_eq!(reading.errors.len(), 2);
    assert_eq!(reading.errors[0].line, 3);
    assert_eq!(reading.errors[1].line, 4);
    for error in &reading.errors {
        assert!(
            !error.rule.contains("twenty") && !error.rule.contains("nobody can read"),
            "a rule names the rule, never the row (Law 1): {}",
            error.rule
        );
    }
}

#[test]
fn a_csv_uploaded_for_the_wrong_account_is_still_that_accounts_statement() {
    // The account is the caller's word on a CSV — nothing in the file
    // contradicts it — so the guard has nothing to catch here. It catches the
    // formats that name their own account, which is asserted below.
    let reading = read("csv_de_january.csv", &for_account(BRITISH_ACCOUNT));
    assert_eq!(
        reading.statement.expect("a statement").account_iban,
        BRITISH_ACCOUNT
    );

    let refused = read_bank_file(
        &for_account(BRITISH_ACCOUNT),
        &fixture("camt053_de_january.xml"),
    )
    .expect_err("a German statement uploaded for a British account");
    assert!(
        format!("{refused}").contains("different account"),
        "{refused:?}"
    );
}
