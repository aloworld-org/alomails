//! Tenancy proof for the bank import — statements and the lines they stage
//! (Law 1: isolation is tested, not assumed) — plus the two duplicate rules
//! that decide whether a bookkeeper's money gets counted twice.
//!
//! A bank statement is the **company's**, not one employee's: a colleague on
//! the same tenant reads the same imports, which is the point of importing it
//! once. What must never happen is one tenant reading another's, and the
//! interesting part is that the two tenants can hold *byte-identical files* —
//! two companies banking at the same institution in a quiet month — without
//! either one's import becoming an oracle for the other's. That is why both
//! uniqueness rules are per tenant, and it is what this suite ends on.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::bank_read::BankImportRequest;
use alo_store::{
    AccountStore, BankCsvDates, BankCsvMapping, BankLineStatus, BankSource, BankStatementId, Store,
    StoreError, TenantId,
};
use time::{Date, Month};

fn fixture(name: &str) -> Vec<u8> {
    let path = format!("{}/tests/fixtures/bank/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&path).unwrap_or_else(|error| panic!("read {path}: {error}"))
}

fn day(year: i32, month: Month, day: u8) -> Date {
    Date::from_calendar_date(year, month, day).unwrap()
}

/// A tenant with one user, on the account door the import uses.
async fn tenant(store: &Store, tag: &str) -> (AccountStore, TenantId) {
    let tenant = store.create_tenant(&format!("bank-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("u-{tag}@example.test"))
        .await
        .unwrap();
    (store.for_account(tenant.clone(), user), tenant)
}

fn assert_conflict<T: std::fmt::Debug>(result: Result<T, StoreError>, expect: &str) {
    match result {
        Err(StoreError::Conflict(message)) => assert!(
            message.contains(expect),
            "conflict {message:?} should name {expect:?}"
        ),
        other => panic!("expected Conflict naming {expect:?}, got: {other:?}"),
    }
}

fn assert_invalid<T: std::fmt::Debug>(result: Result<T, StoreError>, expect: &str) {
    match result {
        Err(StoreError::Validation(message)) => assert!(
            message.contains(expect),
            "validation {message:?} should name {expect:?}"
        ),
        other => panic!("expected Validation naming {expect:?}, got: {other:?}"),
    }
}

#[tokio::test]
async fn a_statement_is_staged_line_by_line_and_nothing_is_booked() {
    let store = common::test_store().await;
    let (acc, _) = tenant(&store, "stage").await;

    let report = acc
        .import_bank_camt053(&fixture("camt053_de_january.xml"))
        .await
        .unwrap();
    assert_eq!(report.staged, 4);
    assert_eq!(report.duplicates, 0);
    assert_eq!(
        report.unbooked, 1,
        "the pending card payment is reported, not staged"
    );

    let statement = &report.statement;
    assert_eq!(statement.source, BankSource::Camt);
    assert_eq!(statement.account_iban, "DE02120300000000202051");
    assert_eq!(statement.currency, "EUR");
    assert_eq!(statement.statement_ref, "2026/001");
    assert_eq!(statement.opening_balance_cents, Some(1_250_000));
    assert_eq!(statement.closing_balance_cents, Some(791_010));
    assert_eq!(statement.from_date, day(2026, Month::January, 1));
    assert_eq!(statement.to_date, day(2026, Month::January, 31));
    assert_eq!(statement.line_count, 4);
    assert_eq!(statement.file_sha256.len(), 64);

    let lines = acc.bank_lines(None, None).await.unwrap();
    assert_eq!(lines.len(), 4);
    assert!(
        lines
            .iter()
            .all(|line| line.status == BankLineStatus::Unmatched),
        "a staged line is not an event: nothing is matched and nothing is booked"
    );
    // Oldest first, numbered as the file listed them.
    assert_eq!(
        lines
            .iter()
            .map(|line| (line.line_no, line.amount_cents))
            .collect::<Vec<_>>(),
        vec![(1, 125_000), (2, -8_990), (3, -450_000), (4, -125_000)]
    );
    assert!(
        lines
            .iter()
            .all(|line| line.statement_id == statement.id && line.currency == "EUR")
    );

    // And the reads narrow the way the reconciliation screen will ask.
    let by_statement = acc.bank_lines(Some(&statement.id), None).await.unwrap();
    assert_eq!(by_statement.len(), 4);
    let matched = acc
        .bank_lines(None, Some(BankLineStatus::Matched))
        .await
        .unwrap();
    assert!(matched.is_empty(), "nothing is matched on import");
    let elsewhere = acc
        .bank_lines(Some(&BankStatementId::new("no-such".to_owned())), None)
        .await
        .unwrap();
    assert!(
        elsewhere.is_empty(),
        "a narrowing that matches nothing matches nothing"
    );
}

#[tokio::test]
async fn the_same_file_twice_is_refused_and_an_overlapping_one_adds_only_what_is_new() {
    let store = common::test_store().await;
    let (acc, _) = tenant(&store, "dupes").await;

    acc.import_bank_camt053(&fixture("camt053_de_january.xml"))
        .await
        .unwrap();

    // The same bytes: named, never swallowed.
    assert_conflict(
        acc.import_bank_camt053(&fixture("camt053_de_january.xml"))
            .await,
        "2026-01-01 to 2026-01-31",
    );

    // A different file covering the first week. Two of its three entries are
    // transactions already staged — one of them with its remittance spaced
    // differently, which is the same payment however the bank reformatted it.
    let week = acc
        .import_bank_camt053(&fixture("camt053_de_week1.xml"))
        .await
        .unwrap();
    assert_eq!(week.staged, 1);
    assert_eq!(week.duplicates, 2);
    assert_eq!(
        week.statement.line_count, 1,
        "the import honestly shows the one line it added"
    );

    let lines = acc.bank_lines(None, None).await.unwrap();
    assert_eq!(lines.len(), 5, "four from the month, one new from the week");
    assert_eq!(
        lines
            .iter()
            .filter(|line| line.amount_cents == -4_200)
            .count(),
        1
    );

    // Both imports are listed, most recent period first.
    let statements = acc.bank_statements().await.unwrap();
    assert_eq!(
        statements
            .iter()
            .map(|statement| statement.statement_ref.as_str())
            .collect::<Vec<_>>(),
        vec!["2026/001", "2026/001-w1"]
    );
}

#[tokio::test]
async fn a_quiet_month_imports_as_a_statement_with_no_lines() {
    let store = common::test_store().await;
    let (acc, _) = tenant(&store, "quiet").await;

    let report = acc
        .import_bank_camt053(&fixture("camt053_quiet_month.xml"))
        .await
        .unwrap();
    assert_eq!((report.staged, report.duplicates), (0, 0));
    assert_eq!(report.statement.line_count, 0);
    assert!(acc.bank_lines(None, None).await.unwrap().is_empty());
    assert_eq!(acc.bank_statements().await.unwrap().len(), 1);
}

#[tokio::test]
async fn a_file_we_cannot_read_stages_nothing_at_all() {
    let store = common::test_store().await;
    let (acc, _) = tenant(&store, "broken").await;

    assert_invalid(
        acc.import_bank_camt053(&fixture("camt053_no_direction.xml"))
            .await,
        "entry 2",
    );
    assert_invalid(acc.import_bank_camt053(b"not a file at all").await, "");
    assert_invalid(
        acc.import_bank_camt053(&vec![b'x'; 9 * 1024 * 1024]).await,
        "at most 8 MB",
    );

    assert!(
        acc.bank_statements().await.unwrap().is_empty(),
        "a refusal is never a partial import"
    );
    assert!(acc.bank_lines(None, None).await.unwrap().is_empty());
}

#[tokio::test]
async fn another_tenant_holding_the_identical_file_sees_none_of_ours() {
    let store = common::test_store().await;
    let (ours, _) = tenant(&store, "a").await;
    let (theirs, _) = tenant(&store, "b").await;

    let file = fixture("camt053_de_january.xml");
    let mine = ours.import_bank_camt053(&file).await.unwrap();

    // The same bytes, in another company. Both uniqueness rules are per
    // tenant, so this is an ordinary first import — not a conflict, and not a
    // way to learn that somebody else already holds this file.
    let yours = theirs.import_bank_camt053(&file).await.unwrap();
    assert_eq!(yours.staged, 4);
    assert_eq!(yours.duplicates, 0);
    assert_ne!(yours.statement.id, mine.statement.id);
    assert_eq!(
        yours.statement.file_sha256, mine.statement.file_sha256,
        "the same file has the same digest; only the tenant differs"
    );

    // And neither door reaches the other's rows.
    assert_eq!(ours.bank_statements().await.unwrap().len(), 1);
    assert_eq!(theirs.bank_statements().await.unwrap().len(), 1);
    assert!(
        theirs
            .bank_statement(&mine.statement.id)
            .await
            .unwrap()
            .is_none(),
        "another tenant's import is absent, never Forbidden — never an existence oracle"
    );
    assert!(
        ours.bank_statement(&yours.statement.id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        theirs
            .bank_lines(Some(&mine.statement.id), None)
            .await
            .unwrap()
            .is_empty(),
        "filtering by another tenant's statement yields our own nothing"
    );
    let theirs_lines = theirs.bank_lines(None, None).await.unwrap();
    assert_eq!(theirs_lines.len(), 4);
    assert!(
        theirs_lines
            .iter()
            .all(|line| line.statement_id == yours.statement.id)
    );
}

#[tokio::test]
async fn an_mt940_stages_through_exactly_the_same_rules_as_a_camt() {
    let store = common::test_store().await;
    let (acc, _) = tenant(&store, "swift").await;

    let report = acc
        .import_bank_mt940(&fixture("mt940_nl_february.sta"))
        .await
        .unwrap();
    assert_eq!(report.staged, 3);
    assert_eq!((report.duplicates, report.unbooked), (0, 0));
    assert_eq!(report.statement.source, BankSource::Mt940);
    assert_eq!(report.statement.account_iban, "NL91ABNA0417164300");
    assert_eq!(report.statement.statement_ref, "00002/001");
    assert_eq!(report.statement.closing_balance_cents, Some(-48_000));

    let lines = acc.bank_lines(None, None).await.unwrap();
    assert_eq!(
        lines
            .iter()
            .map(|line| (line.line_no, line.amount_cents))
            .collect::<Vec<_>>(),
        vec![(1, 50_000), (2, -120_000), (3, -3_000)]
    );
    assert!(
        lines
            .iter()
            .all(|line| line.status == BankLineStatus::Unmatched && line.currency == "EUR"),
        "a staged line is not an event, whichever parser read it"
    );

    // The same bytes twice is the same file, and the refusal names the period.
    assert_conflict(
        acc.import_bank_mt940(&fixture("mt940_nl_february.sta"))
            .await,
        "2026-02-01 to 2026-02-28",
    );

    // A file we cannot read stages nothing at all.
    assert_invalid(
        acc.import_bank_mt940(&fixture("mt940_two_statements.sta"))
            .await,
        "one at a time",
    );
    assert_invalid(
        acc.import_bank_mt940(&fixture("mt940_domestic_account.sta"))
            .await,
        "IBAN",
    );
    assert_invalid(acc.import_bank_mt940(b"a covering letter").await, "MT940");
    assert_eq!(
        acc.bank_statements().await.unwrap().len(),
        1,
        "a refusal is never a partial import"
    );
}

#[tokio::test]
async fn the_same_month_in_two_formats_is_the_same_month() {
    // Three parsers, one contract — and the line hash is of what the bank said
    // happened, not of how it spelled it. So a bookkeeper who downloads January
    // as CAMT and then again as MT940 does not book the month twice.
    let store = common::test_store().await;
    let (acc, _) = tenant(&store, "both").await;

    let camt = acc
        .import_bank_camt053(&fixture("camt053_de_january.xml"))
        .await
        .unwrap();
    assert_eq!(camt.staged, 4);

    let swift = acc
        .import_bank_mt940(&fixture("mt940_de_january.sta"))
        .await
        .unwrap();
    assert_eq!(
        (swift.staged, swift.duplicates),
        (0, 4),
        "the same four transactions, read out of another syntax"
    );
    assert_eq!(swift.statement.source, BankSource::Mt940);
    assert_eq!(
        swift.statement.line_count, 0,
        "the import honestly shows that it added nothing"
    );

    let lines = acc.bank_lines(None, None).await.unwrap();
    assert_eq!(lines.len(), 4, "the month is staged once, not twice");
    assert!(
        lines
            .iter()
            .all(|line| line.statement_id == camt.statement.id)
    );
}

#[tokio::test]
async fn another_tenant_holding_the_identical_mt940_sees_none_of_ours() {
    let store = common::test_store().await;
    let (ours, _) = tenant(&store, "swift-a").await;
    let (theirs, _) = tenant(&store, "swift-b").await;

    let file = fixture("mt940_de_january.sta");
    let mine = ours.import_bank_mt940(&file).await.unwrap();

    // Two companies banking at the same institution can hold byte-identical
    // files; neither import is an oracle for the other's.
    let yours = theirs.import_bank_mt940(&file).await.unwrap();
    assert_eq!((yours.staged, yours.duplicates), (4, 0));
    assert_ne!(yours.statement.id, mine.statement.id);

    assert!(
        theirs
            .bank_statement(&mine.statement.id)
            .await
            .unwrap()
            .is_none(),
        "another tenant's import is absent, never Forbidden"
    );
    assert!(
        theirs
            .bank_lines(Some(&mine.statement.id), None)
            .await
            .unwrap()
            .is_empty(),
        "filtering by another tenant's statement yields our own nothing"
    );
    assert_eq!(ours.bank_lines(None, None).await.unwrap().len(), 4);
    assert_eq!(theirs.bank_lines(None, None).await.unwrap().len(), 4);
}

#[tokio::test]
async fn a_colleague_on_the_same_tenant_reads_the_company_statement() {
    let store = common::test_store().await;
    let (acc, tenant_id) = tenant(&store, "shared").await;
    let colleague = store
        .for_tenant(tenant_id.clone())
        .create_user("colleague@example.test")
        .await
        .unwrap();
    let their_door = store.for_account(tenant_id, colleague);

    let report = acc
        .import_bank_camt053(&fixture("camt053_nl_february.xml"))
        .await
        .unwrap();

    let seen = their_door
        .bank_statement(&report.statement.id)
        .await
        .unwrap()
        .expect("the bank account is the company's, not the uploader's");
    assert_eq!(seen.account_iban, "NL91ABNA0417164300");
    assert_eq!(
        seen.imported_by, report.statement.imported_by,
        "who uploaded it is recorded, and it is not the reader"
    );
    assert_eq!(their_door.bank_lines(None, None).await.unwrap().len(), 3);

    // The colleague uploading the same file again is still the same file.
    assert_conflict(
        their_door
            .import_bank_camt053(&fixture("camt053_nl_february.xml"))
            .await,
        "already been imported",
    );
}

// ---- the CSV wizard (B4.08c) -------------------------------------------------

/// A request naming only the account, which is all a CSV needs when its header
/// is one the wizard knows.
fn csv_for(account: &str) -> BankImportRequest {
    BankImportRequest {
        account_iban: account.to_owned(),
        ..BankImportRequest::default()
    }
}

#[tokio::test]
async fn a_mapped_spreadsheet_stages_through_exactly_the_same_rules() {
    let store = common::test_store().await;
    let (acc, _) = tenant(&store, "sheet").await;

    let import = acc
        .import_bank_file(
            &csv_for("GB33BUKB20201555555555"),
            &fixture("csv_uk_february.csv"),
        )
        .await
        .unwrap();
    let report = import.imported.expect("a staged statement");
    assert_eq!(
        (report.staged, report.duplicates, report.unbooked),
        (3, 0, 0)
    );
    assert_eq!(report.statement.source, BankSource::Csv);
    assert_eq!(report.statement.account_iban, "GB33BUKB20201555555555");
    assert_eq!(
        (
            report.statement.statement_ref.as_str(),
            report.statement.opening_balance_cents
        ),
        ("", None),
        "a spreadsheet names no statement and states no balance"
    );
    assert_eq!(
        import.reading.skipped,
        vec![5],
        "the footer row is reported, so a person told '3 of 4' can find the fourth"
    );

    let lines = acc.bank_lines(None, None).await.unwrap();
    assert_eq!(
        lines
            .iter()
            .map(|line| (line.line_no, line.amount_cents))
            .collect::<Vec<_>>(),
        vec![(1, -340), (2, 120_000), (3, -80_000)]
    );
    assert!(
        lines
            .iter()
            .all(|line| line.status == BankLineStatus::Unmatched),
        "a staged line is not an event, whichever parser read it"
    );

    // The same bytes twice is the same file, whichever door they arrive at.
    let repeat = acc
        .import_bank_file(
            &csv_for("GB33BUKB20201555555555"),
            &fixture("csv_uk_february.csv"),
        )
        .await;
    assert_conflict(repeat, "2026-02-03 to 2026-02-27");
}

#[tokio::test]
async fn one_unreadable_row_writes_nothing_at_all() {
    let store = common::test_store().await;
    let (acc, _) = tenant(&store, "half").await;

    let import = acc
        .import_bank_file(
            &csv_for("DE02120300000000202051"),
            &fixture("csv_broken_rows.csv"),
        )
        .await
        .unwrap();
    assert!(
        import.imported.is_none(),
        "nothing is imported halfway: the readable row is not staged either"
    );
    assert_eq!(import.reading.errors.len(), 2);
    assert!(
        acc.bank_statements().await.unwrap().is_empty(),
        "and no statement header is left behind"
    );
    assert!(acc.bank_lines(None, None).await.unwrap().is_empty());

    // The same file, uploaded again after the rows are fixed, is an ordinary
    // first import: a refusal reserves nothing.
    let fixed = String::from_utf8(fixture("csv_broken_rows.csv"))
        .unwrap()
        .replace("2026-03-XX", "2026-03-03")
        .replace("twenty euros", "20.00");
    let import = acc
        .import_bank_file(&csv_for("DE02120300000000202051"), fixed.as_bytes())
        .await
        .unwrap();
    assert_eq!(import.imported.expect("staged").staged, 3);
}

#[tokio::test]
async fn the_same_month_as_a_spreadsheet_is_still_the_same_month() {
    // The third format, and the same promise the other two keep to each other:
    // the line hash is of what the bank said happened, not of how it spelled
    // it. A bookkeeper who downloads January as CAMT and then as a spreadsheet
    // does not book it twice.
    let store = common::test_store().await;
    let (acc, _) = tenant(&store, "three").await;

    let camt = acc
        .import_bank_camt053(&fixture("camt053_de_january.xml"))
        .await
        .unwrap();
    assert_eq!(camt.staged, 4);

    let sheet = acc
        .import_bank_file(
            &csv_for("DE02120300000000202051"),
            &fixture("csv_de_january.csv"),
        )
        .await
        .unwrap()
        .imported
        .expect("a staged statement");
    assert_eq!(
        (sheet.staged, sheet.duplicates),
        (0, 4),
        "the same four transactions, read out of a third format"
    );
    assert_eq!(sheet.statement.source, BankSource::Csv);
    assert_eq!(sheet.statement.line_count, 0);
    assert_eq!(
        acc.bank_lines(None, None).await.unwrap().len(),
        4,
        "the month is staged once, not three times"
    );
}

#[tokio::test]
async fn another_tenant_holding_the_identical_spreadsheet_sees_none_of_ours() {
    let store = common::test_store().await;
    let (ours, _) = tenant(&store, "sheet-a").await;
    let (theirs, _) = tenant(&store, "sheet-b").await;

    // Two companies exporting from the same portal in the same quiet month can
    // hold byte-identical spreadsheets — and, unlike the other two formats,
    // they can also import them **for different accounts**, because on a CSV
    // the account is the uploader's word. Neither is an oracle for the other.
    let file = fixture("csv_uk_february.csv");
    let mine = ours
        .import_bank_file(&csv_for("GB33BUKB20201555555555"), &file)
        .await
        .unwrap()
        .imported
        .expect("ours");
    let yours = theirs
        .import_bank_file(&csv_for("DE02120300000000202051"), &file)
        .await
        .unwrap()
        .imported
        .expect("theirs");

    assert_eq!((yours.staged, yours.duplicates), (3, 0));
    assert_ne!(yours.statement.id, mine.statement.id);
    assert_eq!(
        yours.statement.file_sha256, mine.statement.file_sha256,
        "the same file has the same digest; only the tenant differs"
    );
    assert!(
        theirs
            .bank_statement(&mine.statement.id)
            .await
            .unwrap()
            .is_none(),
        "another tenant's import is absent, never Forbidden"
    );
    assert!(
        theirs
            .bank_lines(Some(&mine.statement.id), None)
            .await
            .unwrap()
            .is_empty(),
        "filtering by another tenant's statement yields our own nothing"
    );
    assert_eq!(ours.bank_lines(None, None).await.unwrap().len(), 3);
    assert_eq!(theirs.bank_lines(None, None).await.unwrap().len(), 3);
}

#[tokio::test]
async fn a_file_the_wizard_cannot_be_told_how_to_read_stages_nothing() {
    let store = common::test_store().await;
    let (acc, _) = tenant(&store, "wizard").await;

    let account = "DE02120300000000202051";
    // No account stated, on the one format that cannot state its own.
    assert_invalid(
        acc.import_bank_file(
            &BankImportRequest::default(),
            b"Date,Amount\n2026-01-05,10.00\n",
        )
        .await,
        "state the account's IBAN",
    );
    // A mapping pointing at a column the file has not got.
    assert_invalid(
        acc.import_bank_file(
            &BankImportRequest {
                mapping: BankCsvMapping {
                    booked_on: Some("Date".to_owned()),
                    amount: Some("Montant".to_owned()),
                    ..BankCsvMapping::default()
                },
                ..csv_for(account)
            },
            b"Date,Amount\n2026-01-05,10.00\n",
        )
        .await,
        "no column mapped to the amount",
    );
    // Dates that could be either way round, with nothing in the file to settle
    // it — refused, never read one of the two ways.
    assert_invalid(
        acc.import_bank_file(
            &csv_for(account),
            b"Date,Amount\n03/04/2026,10.00\n05/06/2026,20.00\n",
        )
        .await,
        "state the date order",
    );
    // And the same file, told which way round it is, imports.
    let told = acc
        .import_bank_file(
            &BankImportRequest {
                dates: BankCsvDates::Dmy,
                ..csv_for(account)
            },
            b"Date,Amount\n03/04/2026,10.00\n05/06/2026,20.00\n",
        )
        .await
        .unwrap()
        .imported
        .expect("staged");
    assert_eq!(told.staged, 2);
    assert_eq!(told.statement.from_date, day(2026, Month::April, 3));

    assert_eq!(
        acc.bank_statements().await.unwrap().len(),
        1,
        "a refusal is never a partial import"
    );
}
