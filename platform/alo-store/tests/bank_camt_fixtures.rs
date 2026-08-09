//! Golden files for the CAMT.053 reader (alo Finance, ADR 0035, wave B4.08a).
//!
//! Each fixture in `tests/fixtures/bank/` is a statement written in the shape
//! one country's banks publish their camt.053 examples in — different
//! prefixing, different balance codes, different silences — and this suite
//! states, line by line, exactly what alo reads out of it. When a future
//! change to the reader alters what a bank statement means, this is the file
//! that fails.
//!
//! Two of them close the loop the way a real statement does: **opening balance
//! plus every booked entry equals the closing balance**. That assertion is what
//! makes them golden rather than merely parseable — it would catch a sign read
//! backwards, an entry dropped, or a batch counted twice, none of which a
//! field-by-field comparison written from the same misreading would notice.
//!
//! Pure parsing, no database.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alo_store::{BankSource, ParsedStatement, StoreError, parse_camt053};
use time::{Date, Month};

/// One of the fixture statements, read from disk exactly as an upload would
/// arrive.
fn fixture(name: &str) -> Vec<u8> {
    let path = format!("{}/tests/fixtures/bank/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&path).unwrap_or_else(|error| panic!("read {path}: {error}"))
}

fn parsed(name: &str) -> ParsedStatement {
    match parse_camt053(&fixture(name)) {
        Ok(statement) => statement,
        Err(error) => panic!("{name} should read as a statement, got {error:?}"),
    }
}

fn day(year: i32, month: Month, day: u8) -> Date {
    Date::from_calendar_date(year, month, day).unwrap()
}

/// Opening balance + every line = closing balance. The property a statement
/// has by construction, and the one a misread sign breaks.
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
        "the statement must close where its entries leave it"
    );
}

#[test]
fn a_german_month_reads_entry_by_entry() {
    let statement = parsed("camt053_de_january.xml");

    assert_eq!(statement.source, BankSource::Camt);
    assert_eq!(statement.account_iban, "DE02120300000000202051");
    assert_eq!(statement.currency, "EUR");
    assert_eq!(statement.statement_ref, "2026/001");
    assert_eq!(statement.from_date, day(2026, Month::January, 1));
    assert_eq!(statement.to_date, day(2026, Month::January, 31));
    assert_eq!(statement.opening_balance_cents, Some(1_250_000));
    assert_eq!(statement.closing_balance_cents, Some(791_010));
    assert_eq!(
        statement.unbooked, 1,
        "the pending card payment is counted, not staged"
    );
    assert_eq!(statement.lines.len(), 4);
    reconciles(&statement);

    let paid = &statement.lines[0];
    assert_eq!(paid.amount_cents, 125_000);
    assert_eq!(paid.booked_on, day(2026, Month::January, 5));
    assert_eq!(paid.value_on, day(2026, Month::January, 4));
    assert_eq!(paid.currency, "EUR");
    assert_eq!(paid.counterparty_name, "Kaffeehaus Berlin GmbH");
    assert_eq!(paid.counterparty_iban, "NL91ABNA0417164300");
    assert_eq!(paid.remittance, "Rechnung INV-2026-00007 vielen Dank");
    assert_eq!(paid.bank_ref, "DE-2026-0105-0001");

    let utility = &statement.lines[1];
    assert_eq!(utility.amount_cents, -8_990);
    assert_eq!(utility.counterparty_name, "Stadtwerke Muenchen GmbH");
    assert_eq!(utility.counterparty_iban, "DE89370400440532013000");
    assert_eq!(
        utility.remittance, "RF18539007547034",
        "a structured creditor reference is what was written on the payment"
    );

    let payroll = &statement.lines[2];
    assert_eq!(payroll.amount_cents, -450_000, "the batch total, once");
    assert_eq!(
        payroll.counterparty_name, "",
        "neither of the two people is the counterparty of the entry"
    );
    assert_eq!(
        payroll.remittance,
        "Sammelueberweisung Gehaltslauf Januar 2026"
    );

    let returned = &statement.lines[3];
    assert_eq!(
        returned.amount_cents, -125_000,
        "a reversed credit is money leaving"
    );
    assert_eq!(returned.counterparty_name, "Kaffeehaus Berlin GmbH");
    assert_eq!(returned.bank_ref, "DE-2026-0129-0014");
}

#[test]
fn a_dutch_month_reads_the_same_standard_written_differently() {
    let statement = parsed("camt053_nl_february.xml");

    assert_eq!(
        statement.account_iban, "NL91ABNA0417164300",
        "a prefixed document is the same document"
    );
    assert_eq!(statement.statement_ref, "0000002");
    assert_eq!(statement.from_date, day(2026, Month::February, 1));
    assert_eq!(
        statement.to_date,
        day(2026, Month::February, 28),
        "a dateTime in a local offset states a day"
    );
    assert_eq!(
        statement.opening_balance_cents,
        Some(25_000),
        "PRCD stands in for a missing OPBD"
    );
    assert_eq!(
        statement.closing_balance_cents,
        Some(-48_000),
        "an overdrawn account closes on a debit balance"
    );
    assert_eq!(statement.unbooked, 0);
    assert_eq!(statement.lines.len(), 3);
    reconciles(&statement);

    let received = &statement.lines[0];
    assert_eq!(received.amount_cents, 50_000);
    assert_eq!(received.counterparty_name, "Gemeente Amsterdam");
    assert_eq!(
        received.remittance, "Factuur INV-2026-00011 betaling februari",
        "several unstructured lines are one remittance"
    );

    let supplier = &statement.lines[1];
    assert_eq!(supplier.amount_cents, -120_000);
    assert_eq!(supplier.counterparty_name, "Leverancier BV");
    assert_eq!(supplier.counterparty_iban, "BE68539007547034");
    assert_eq!(
        supplier.bank_ref, "",
        "NOTPROVIDED is the standard's way of saying nothing, not a reference"
    );

    let charges = &statement.lines[2];
    assert_eq!(charges.amount_cents, -3_000);
    assert_eq!(
        charges.counterparty_name, "",
        "bank charges have no counterparty but the bank"
    );
    assert_eq!(charges.remittance, "Kosten betalingsverkeer februari");
}

#[test]
fn a_month_in_which_nothing_happened_is_still_a_statement() {
    let statement = parsed("camt053_quiet_month.xml");
    assert!(statement.lines.is_empty());
    assert_eq!(statement.from_date, day(2026, Month::March, 1));
    assert_eq!(statement.to_date, day(2026, Month::March, 31));
    assert_eq!(statement.opening_balance_cents, Some(791_010));
    assert_eq!(statement.closing_balance_cents, Some(791_010));
    reconciles(&statement);
}

#[test]
fn an_entry_that_does_not_say_which_way_the_money_went_refuses_the_whole_file() {
    match parse_camt053(&fixture("camt053_no_direction.xml")) {
        Err(StoreError::Validation(message)) => {
            assert!(message.contains("entry 2"), "names the entry: {message}");
            assert!(
                message.contains("CdtDbtInd"),
                "names the element: {message}"
            );
        }
        other => panic!("expected a Validation refusal, got {other:?}"),
    }
}

#[test]
fn the_week_file_is_the_same_transactions_written_a_little_differently() {
    // Not a duplicate *file* — a different document, whose first two entries
    // are the January statement's first two. The store's line rule is what
    // notices; here we only prove the reader gives them the same content, so
    // that rule has something to work with.
    let week = parsed("camt053_de_week1.xml");
    let month = parsed("camt053_de_january.xml");
    assert_eq!(week.lines.len(), 3);
    for (a, b) in week.lines.iter().take(2).zip(&month.lines) {
        assert_eq!(a.amount_cents, b.amount_cents);
        assert_eq!(a.booked_on, b.booked_on);
        assert_eq!(a.bank_ref, b.bank_ref);
        assert_eq!(a.counterparty_iban, b.counterparty_iban);
    }
    assert_eq!(week.lines[2].amount_cents, -4_200);
    assert_eq!(week.lines[2].remittance, "Kartenzahlung Tankstelle");
}
