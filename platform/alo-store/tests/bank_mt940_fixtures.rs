//! Golden files for the MT940 reader (alo Finance, ADR 0035, wave B4.08b).
//!
//! Each fixture in `tests/fixtures/bank/` is a statement written in the shape
//! one country's banks emit MT940 in — structured `?`-subfields or free text,
//! bare or wrapped in SWIFT's transport blocks, one page or two — and this
//! suite states, line by line, exactly what alo reads out of it. When a future
//! change to the reader alters what a bank statement means, this is the file
//! that fails.
//!
//! Two of them close the loop the way a real statement does: **opening balance
//! plus every transaction equals the closing balance**. That assertion is what
//! makes them golden rather than merely parseable — it would catch a sign read
//! backwards, a transaction dropped, or a reversal counted the right way up,
//! none of which a field-by-field comparison written from the same misreading
//! would notice.
//!
//! `a_month_reads_the_same_in_either_format` is the one that earns the wave's
//! claim of three parsers and one contract: the German January exists as both a
//! CAMT.053 and an MT940, and the two must produce the same transactions.
//!
//! Pure parsing, no database.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alo_store::{BankSource, ParsedStatement, StoreError, parse_camt053, parse_mt940};
use time::{Date, Month};

/// One of the fixture statements, read from disk exactly as an upload would
/// arrive.
fn fixture(name: &str) -> Vec<u8> {
    let path = format!("{}/tests/fixtures/bank/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&path).unwrap_or_else(|error| panic!("read {path}: {error}"))
}

fn parsed(name: &str) -> ParsedStatement {
    match parse_mt940(&fixture(name)) {
        Ok(statement) => statement,
        Err(error) => panic!("{name} should read as a statement, got {error:?}"),
    }
}

fn refused(name: &str) -> String {
    match parse_mt940(&fixture(name)) {
        Err(StoreError::Validation(message)) => message,
        other => panic!("{name} should be refused, got {other:?}"),
    }
}

fn day(year: i32, month: Month, day: u8) -> Date {
    Date::from_calendar_date(year, month, day).unwrap()
}

/// Opening balance + every transaction = closing balance. The property a
/// statement has by construction, and the one a misread sign breaks.
fn reconciles(statement: &ParsedStatement) {
    let opening = statement
        .opening_balance_cents
        .expect("this fixture states an opening balance");
    let closing = statement
        .closing_balance_cents
        .expect("this fixture states a closing balance");
    let moved: i64 = statement.lines.iter().map(|line| line.amount_cents).sum();
    assert_eq!(
        opening + moved,
        closing,
        "the statement must close where its transactions leave it"
    );
}

#[test]
fn a_german_month_reads_transaction_by_transaction() {
    let statement = parsed("mt940_de_january.sta");

    assert_eq!(statement.source, BankSource::Mt940);
    assert_eq!(statement.account_iban, "DE02120300000000202051");
    assert_eq!(statement.currency, "EUR");
    assert_eq!(statement.statement_ref, "00001/001");
    assert_eq!(statement.from_date, day(2026, Month::January, 1));
    assert_eq!(statement.to_date, day(2026, Month::January, 31));
    assert_eq!(statement.opening_balance_cents, Some(1_250_000));
    assert_eq!(statement.closing_balance_cents, Some(791_010));
    assert_eq!(
        statement.unbooked, 0,
        "MT940 states what the bank booked; nothing is pending and nothing is skipped"
    );
    assert_eq!(
        statement.lines.len(),
        4,
        "the bank's covering text above the first tag is not a transaction"
    );
    reconciles(&statement);

    let paid = &statement.lines[0];
    assert_eq!(paid.amount_cents, 125_000);
    assert_eq!(
        paid.value_on,
        day(2026, Month::January, 4),
        ":61: opens with the value date"
    );
    assert_eq!(
        paid.booked_on,
        day(2026, Month::January, 5),
        "the entry date is the day the books use"
    );
    assert_eq!(paid.currency, "EUR");
    assert_eq!(paid.counterparty_name, "Kaffeehaus Berlin GmbH");
    assert_eq!(paid.counterparty_iban, "NL91ABNA0417164300");
    assert_eq!(paid.remittance, "Rechnung INV-2026-00007 vielen Dank");
    assert_eq!(
        paid.bank_ref, "DE-2026-0105-0001",
        "what follows // is the bank's own reference, and it is preferred to the payer's"
    );

    let utility = &statement.lines[1];
    assert_eq!(utility.amount_cents, -8_990);
    assert_eq!(utility.counterparty_name, "Stadtwerke Muenchen GmbH");
    assert_eq!(utility.counterparty_iban, "DE89370400440532013000");
    assert_eq!(
        utility.remittance, "RF18539007547034",
        "a structured creditor reference is what was written on the payment"
    );

    let payroll = &statement.lines[2];
    assert_eq!(payroll.amount_cents, -450_000);
    assert_eq!(
        payroll.remittance, "Sammelueberweisung Gehaltslauf Januar 2026",
        "the ?2n chunks are one string: the bank split 'Gehaltslauf' across two of them"
    );
    assert_eq!(
        payroll.counterparty_name, "",
        "a batch names no counterparty, and the reader invents none"
    );
    assert_eq!(payroll.bank_ref, "DE-2026-0128-0011");

    let returned = &statement.lines[3];
    assert_eq!(
        returned.amount_cents, -125_000,
        "RC is a reversed credit, which is money leaving"
    );
    assert_eq!(
        returned.remittance, "Rueckbuchung Rechnung INV-2026-00007",
        "an invoice number split across two chunks is put back together"
    );
    assert_eq!(returned.counterparty_name, "Kaffeehaus Berlin GmbH");
}

#[test]
fn a_month_reads_the_same_in_either_format() {
    // The wave's claim, stated as a test: three parsers, one contract. The same
    // January as a CAMT.053 and as an MT940 — different syntax, different
    // silences, the same four movements of money.
    let camt = match parse_camt053(&fixture("camt053_de_january.xml")) {
        Ok(statement) => statement,
        Err(error) => panic!("the CAMT fixture should read, got {error:?}"),
    };
    let mt940 = parsed("mt940_de_january.sta");

    assert_eq!(camt.account_iban, mt940.account_iban);
    assert_eq!(camt.currency, mt940.currency);
    assert_eq!(camt.from_date, mt940.from_date);
    assert_eq!(camt.to_date, mt940.to_date);
    assert_eq!(camt.opening_balance_cents, mt940.opening_balance_cents);
    assert_eq!(camt.closing_balance_cents, mt940.closing_balance_cents);

    // Every field the line hash is built from — which is what makes the two
    // imports de-duplicate against each other in the store.
    for (from_xml, from_swift) in camt.lines.iter().zip(&mt940.lines) {
        assert_eq!(from_xml.booked_on, from_swift.booked_on);
        assert_eq!(from_xml.amount_cents, from_swift.amount_cents);
        assert_eq!(from_xml.currency, from_swift.currency);
        assert_eq!(from_xml.bank_ref, from_swift.bank_ref);
        assert_eq!(from_xml.counterparty_iban, from_swift.counterparty_iban);
        assert_eq!(from_xml.remittance, from_swift.remittance);
    }
    assert_eq!(camt.lines.len(), mt940.lines.len());
}

#[test]
fn a_dutch_month_reads_the_same_standard_written_differently() {
    let statement = parsed("mt940_nl_february.sta");

    assert_eq!(
        statement.account_iban, "NL91ABNA0417164300",
        "the currency appended to the account is not part of the account"
    );
    assert_eq!(statement.currency, "EUR");
    assert_eq!(statement.statement_ref, "00002/001");
    assert_eq!(statement.from_date, day(2026, Month::February, 1));
    assert_eq!(statement.to_date, day(2026, Month::February, 28));
    assert_eq!(statement.opening_balance_cents, Some(25_000));
    assert_eq!(
        statement.closing_balance_cents,
        Some(-48_000),
        "an overdrawn account closes on a debit balance"
    );
    assert_eq!(statement.lines.len(), 3);
    reconciles(&statement);

    let received = &statement.lines[0];
    assert_eq!(received.amount_cents, 50_000);
    assert_eq!(
        received.remittance, "Factuur INV-2026-00011 betaling februari",
        "a free-text field the bank wrapped is one remittance"
    );
    assert_eq!(
        received.counterparty_name, "",
        "free text names no party, and a blank field is the honest answer"
    );
    assert_eq!(received.bank_ref, "NL-2026-0203-0007");

    let supplier = &statement.lines[1];
    assert_eq!(supplier.amount_cents, -120_000);
    assert_eq!(
        supplier.remittance, "Leverancier BV maandtermijn",
        "a transaction with no :86: falls back to its own supplementary line"
    );
    assert_eq!(
        supplier.bank_ref, "",
        "NONREF is MT940's way of saying nothing, not a reference"
    );

    let charges = &statement.lines[2];
    assert_eq!(charges.amount_cents, -3_000);
    assert_eq!(charges.remittance, "Kosten betalingsverkeer februari");
    assert_ne!(
        charges.remittance, "Vanaf 1 maart wijzigen onze tarieven",
        "the bank's note after the closing balance belongs to no transaction"
    );
}

#[test]
fn a_long_month_sent_as_two_pages_is_one_statement() {
    let statement = parsed("mt940_paged.sta");
    assert_eq!(statement.lines.len(), 2, "two pages, one statement");
    assert_eq!(statement.statement_ref, "00004/001");
    assert_eq!(
        statement.opening_balance_cents,
        Some(100_000),
        "the period opens where the first page did, not where the second reopened"
    );
    assert_eq!(statement.closing_balance_cents, Some(130_000));
    assert_eq!(statement.from_date, day(2026, Month::April, 1));
    assert_eq!(statement.to_date, day(2026, Month::April, 30));
    reconciles(&statement);
    assert_eq!(statement.lines[0].counterparty_name, "Bauhaus Leipzig GmbH");
    assert_eq!(statement.lines[1].counterparty_name, "Allianz SE");
}

#[test]
fn a_month_in_which_nothing_happened_is_still_a_statement() {
    let statement = parsed("mt940_quiet_month.sta");
    assert!(statement.lines.is_empty());
    assert_eq!(statement.from_date, day(2026, Month::March, 1));
    assert_eq!(statement.to_date, day(2026, Month::March, 31));
    assert_eq!(statement.opening_balance_cents, Some(791_010));
    assert_eq!(statement.closing_balance_cents, Some(791_010));
    reconciles(&statement);
}

#[test]
fn a_file_holding_two_statements_is_refused_whole() {
    assert!(
        refused("mt940_two_statements.sta").contains("one at a time"),
        "two accounts in one upload are not one import"
    );
}

#[test]
fn a_statement_of_a_domestic_account_says_what_to_ask_the_bank_for() {
    let message = refused("mt940_domestic_account.sta");
    assert!(message.contains(":25:"), "names the field: {message}");
    assert!(message.contains("IBAN"), "names what is missing: {message}");
    assert!(
        !message.contains("12030000"),
        "an error never quotes the tenant's bank data: {message}"
    );
}
