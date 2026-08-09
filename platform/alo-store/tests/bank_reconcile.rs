//! **Reconciliation, the exact stage** (alo Finance, ADR 0035, wave B4.09a) —
//! a staged bank line becoming money in the books, on the real wire.
//!
//! `src/bank_match.rs` states the rule as arithmetic and proves it without a
//! database. This suite asserts the six things a pure test cannot:
//!
//! - the **arc**: import a statement, see one suggestion, confirm it, and find
//!   a payment, a settled invoice, a matched line and two journal entries that
//!   leave the receivable at exactly zero;
//! - **nothing is auto-confirmed** — an imported statement moves no money and
//!   posts nothing until a person says so (ADR 0023);
//! - **the books open here**: a confirmation books the invoice's issue when
//!   nothing else has, and says so, and never books it twice;
//! - **the rule is re-derived under the row locks**, so a payment somebody
//!   recorded by hand in the meantime refuses the confirmation instead of
//!   doubling the money — and leaves nothing behind when it does;
//! - **precision**: one cent short, the wrong number, money going the other way
//!   and a second line against a settled invoice are all refused, on the wire;
//! - **tenancy**: two tenants can hold the byte-identical statement and the
//!   same document number, and neither is ever an oracle or a target for the
//!   other (Law 1).
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::bank_read::BankImportRequest;
use alo_store::{
    AccountRole, AccountStore, BankCsvDates, BankCsvDecimal, BankCsvMapping, BankLineId,
    BankLineStatus, BankMatchTarget, BankSource, BillingCustomerId, BillingInvoiceId, CHART,
    ChartName, ChartSeed, EntrySource, InvoiceStatus, NewCustomer, NewInvoice, NewLine, NewPayment,
    PaymentState, SourceEvent, SourceKind, Store, StoreError, TenantId,
};
use time::{Date, Duration};

/// The account every statement in this suite is of — the IBAN specification's
/// own test number, never a real one.
const ACCOUNT: &str = "DE02120300000000202051";

/// €1 307.00: the two-rate document the posting rules' goldens are written for.
const GROSS_CENTS: i64 = 130_700;

fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::NotFound) => {}
        other => panic!("expected NotFound, got {other:?}"),
    }
}

fn refused<T: std::fmt::Debug>(result: Result<T, StoreError>, expect: &str) -> String {
    match result {
        Err(StoreError::Validation(message) | StoreError::Conflict(message)) => {
            assert!(
                message.contains(expect),
                "refusal {message:?} should name {expect:?}"
            );
            message
        }
        other => panic!("expected a refusal naming {expect:?}, got {other:?}"),
    }
}

/// The chart, named per tenant so a leak between two of them shows up as a name
/// from the wrong tenant rather than as a number that happens to match.
fn seed(tag: &str) -> ChartSeed {
    ChartSeed {
        names: CHART
            .iter()
            .map(|account| ChartName {
                code: account.code.to_owned(),
                name: format!("{tag} {}", account.code),
            })
            .collect(),
    }
}

async fn tenant_with_chart(store: &Store, tag: &str) -> (AccountStore, TenantId) {
    let tenant = store
        .create_tenant(&format!("reconcile-{tag}"))
        .await
        .unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@reconciling.test"))
        .await
        .unwrap();
    let account = store.for_account(tenant.clone(), user);
    account
        .fin_accounts_or_seed(&seed(tag), false)
        .await
        .unwrap();
    (account, tenant)
}

async fn customer(account: &AccountStore, tag: &str) -> BillingCustomerId {
    account
        .create_billing_customer(&NewCustomer {
            name: format!("Customer {tag}"),
            country: "NL".to_owned(),
            currency: "EUR".to_owned(),
            payment_terms_days: 30,
            ..Default::default()
        })
        .await
        .unwrap()
}

fn lines() -> Vec<NewLine> {
    [
        (10_000_i64, 10_000_i64, 2100_i32),
        (4_000, 5_000, 900),
        (-1_000, 10_000, 2100),
    ]
    .iter()
    .enumerate()
    .map(
        |(index, &(qty_milli, unit_price_cents, vat_rate_bp))| NewLine {
            description: format!("Line {index}"),
            unit: "hour".to_owned(),
            qty_milli,
            unit_price_cents,
            vat_rate_bp,
        },
    )
    .collect()
}

/// An issued invoice worth [`GROSS_CENTS`], with its number and its issue date
/// — the two facts a bank line has to agree with.
async fn issued_invoice(
    account: &AccountStore,
    customer: &BillingCustomerId,
) -> (BillingInvoiceId, String, Date) {
    let id = account
        .create_billing_invoice(&NewInvoice::for_customer(customer.clone()))
        .await
        .unwrap();
    account
        .set_billing_invoice_lines(&id, &lines())
        .await
        .unwrap();
    let document = account.issue_billing_invoice(&id).await.unwrap();
    assert_eq!(document.totals.gross_cents, GROSS_CENTS);
    let number = document.invoice.number.clone().expect("an issued number");
    let issued_on = document.invoice.issue_date.expect("an issue date");
    (id, number, issued_on)
}

/// A CSV statement of `rows` — `(booked on, signed cents, what the payer
/// wrote)` — in the shape a bank portal exports: ISO dates, a dot decimal, one
/// signed amount column.
fn statement(rows: &[(Date, i64, String)]) -> Vec<u8> {
    let mut csv = String::from("date,amount,description,reference\n");
    for (index, (booked_on, cents, remittance)) in rows.iter().enumerate() {
        let sign = if *cents < 0 { "-" } else { "" };
        csv.push_str(&format!(
            "{booked_on},{sign}{}.{:02},{remittance},REF{index}\n",
            cents.abs() / 100,
            cents.abs() % 100
        ));
    }
    csv.into_bytes()
}

fn csv_request() -> BankImportRequest {
    BankImportRequest {
        source: Some(BankSource::Csv),
        account_iban: ACCOUNT.to_owned(),
        currency: Some("EUR".to_owned()),
        dates: BankCsvDates::Ymd,
        decimal: BankCsvDecimal::Dot,
        mapping: BankCsvMapping {
            booked_on: Some("date".to_owned()),
            amount: Some("amount".to_owned()),
            remittance: Some("description".to_owned()),
            bank_ref: Some("reference".to_owned()),
            ..Default::default()
        },
    }
}

/// Imports `rows` as a statement and answers the staged lines, in file order.
async fn stage(account: &AccountStore, rows: &[(Date, i64, String)]) -> Vec<BankLineId> {
    let report = account
        .import_bank_file(&csv_request(), &statement(rows))
        .await
        .unwrap();
    let imported = report.imported.expect("the file imports");
    assert_eq!(imported.staged, rows.len(), "every row stages");
    let statement_id = imported.statement.id.clone();
    // The store answers a bookkeeper's order (oldest first); a test names its
    // rows by where they were in the file.
    let mut lines = account.bank_lines(Some(&statement_id), None).await.unwrap();
    lines.sort_by_key(|line| line.line_no);
    lines.into_iter().map(|line| line.id).collect()
}

/// What the tenant's ledger says the receivable is, in **both** money columns:
/// the accounting currency (the trial balance's figure) and the documents' own
/// (summed from the account's ledger lines). An account with no posting is
/// absent from both, which is a zero balance.
async fn receivable(account: &AccountStore) -> (i64, i64) {
    let Some(ar) = account.fin_account_for_role(AccountRole::Ar).await.unwrap() else {
        return (0, 0);
    };
    let base = account
        .fin_trial_balance(None, None)
        .await
        .unwrap()
        .accounts
        .iter()
        .find(|row| row.account_id == ar.id)
        .map_or(0, |row| row.balance_cents);
    let document: i64 = account
        .fin_account_ledger(&ar.id, None, None, 200)
        .await
        .unwrap()
        .lines
        .iter()
        .map(|line| line.amount_cents)
        .sum();
    (document, base)
}

#[tokio::test]
async fn a_quoted_number_and_the_exact_amount_become_a_payment_and_two_entries() {
    let store = common::test_store().await;
    let (acc, _) = tenant_with_chart(&store, "arc").await;
    let customer = customer(&acc, "arc").await;
    let (invoice, number, issued_on) = issued_invoice(&acc, &customer).await;

    // A statement is never from the future: the day the bank booked this is the
    // day the document was issued, which is the only day a test can have.
    let paid_on = issued_on;
    let lines = stage(
        &acc,
        &[
            (
                paid_on,
                GROSS_CENTS,
                format!("Rechnung {number} vielen Dank"),
            ),
            (paid_on, -8_990, "Stadtwerke Muenchen".to_owned()),
        ],
    )
    .await;

    // Nothing has been matched, nothing has been paid, nothing has been booked.
    let suggestions = acc.bank_match_suggestions(None).await.unwrap();
    assert!(!suggestions.numbers_capped);
    assert_eq!(suggestions.lines.len(), 2, "both lines are unmatched");
    let quoted = &suggestions.lines[0];
    assert_eq!(quoted.exact.len(), 1, "one document, quoted by number");
    assert_eq!(quoted.exact[0].invoice_id, invoice);
    assert_eq!(quoted.exact[0].number, number);
    assert_eq!(quoted.exact[0].amount_cents, GROSS_CENTS);
    assert_eq!(quoted.exact[0].days_after_issue, 0);
    assert!(
        suggestions.lines[1].exact.is_empty(),
        "the utility bill matches nothing"
    );
    assert_eq!(
        acc.billing_invoice(&invoice)
            .await
            .unwrap()
            .unwrap()
            .settlement()
            .state,
        PaymentState::Unpaid,
        "a suggestion is not a payment"
    );
    assert_eq!(acc.bank_match(&lines[0]).await.unwrap(), None);

    // The confirmation.
    let confirmed = acc
        .confirm_bank_match(&lines[0], &BankMatchTarget::Invoice(invoice.clone()))
        .await
        .unwrap();
    assert!(
        confirmed.invoice_booked_now,
        "nothing else books an issue yet, so the confirmation does"
    );
    assert_eq!(confirmed.matched.amount_cents, GROSS_CENTS);
    assert_eq!(
        confirmed.matched.target,
        BankMatchTarget::Invoice(invoice.clone())
    );

    // The payment, dated the day the bank booked it, quoting the bank's own
    // reference.
    let payments = acc.billing_payments(&invoice).await.unwrap();
    assert_eq!(payments.len(), 1);
    assert_eq!(payments[0].amount_cents, GROSS_CENTS);
    assert_eq!(payments[0].paid_on, paid_on);
    assert_eq!(payments[0].reference, "REF0");
    assert_eq!(confirmed.matched.payment_id.as_ref(), Some(&payments[0].id));

    // The document, settled — a projection of the payment, not of the match.
    let document = acc.billing_invoice(&invoice).await.unwrap().unwrap();
    assert_eq!(document.invoice.status, InvoiceStatus::Paid);
    assert_eq!(document.settlement().outstanding_cents, 0);

    // The line, matched, and the match readable through its own door.
    let line = acc.bank_line(&lines[0]).await.unwrap().unwrap();
    assert_eq!(line.status, BankLineStatus::Matched);
    let stored = acc.bank_match(&lines[0]).await.unwrap().unwrap();
    assert_eq!(stored.id, confirmed.matched.id);
    assert_eq!(stored.entry_id, confirmed.matched.entry_id);
    assert_eq!(stored.rule_id, None, "the exact stage needs no rule");

    // The books: the issue entry and the settlement, and a receivable that is
    // exactly zero in both columns (P5 on the wire).
    assert_eq!(
        acc.fin_invoice_entry(&invoice).await.unwrap(),
        Some(confirmed.invoice_entry_id.clone())
    );
    assert_eq!(
        acc.fin_entry_for_source(&EntrySource {
            kind: SourceKind::Payment,
            id: payments[0].id.as_str().to_owned(),
            event: SourceEvent::Settle,
        })
        .await
        .unwrap(),
        confirmed.matched.entry_id
    );
    assert_eq!(receivable(&acc).await, (0, 0));
    assert!(acc.fin_unbalanced_entries().await.unwrap().is_empty());

    // The other line is untouched by any of it.
    assert_eq!(
        acc.bank_line(&lines[1]).await.unwrap().unwrap().status,
        BankLineStatus::Unmatched
    );
}

#[tokio::test]
async fn an_invoice_already_in_the_books_is_not_booked_a_second_time() {
    let store = common::test_store().await;
    let (acc, _) = tenant_with_chart(&store, "booked").await;
    let customer = customer(&acc, "booked").await;
    let (invoice, number, issued_on) = issued_invoice(&acc, &customer).await;
    let issue_entry = acc.post_invoice_issue(&invoice).await.unwrap();

    let lines = stage(&acc, &[(issued_on, GROSS_CENTS, number)]).await;
    let confirmed = acc
        .confirm_bank_match(&lines[0], &BankMatchTarget::Invoice(invoice.clone()))
        .await
        .unwrap();
    assert!(!confirmed.invoice_booked_now, "it was already there");
    assert_eq!(confirmed.invoice_entry_id, issue_entry);
    assert_eq!(receivable(&acc).await, (0, 0));
}

#[tokio::test]
async fn the_rest_of_a_partly_paid_document_is_what_an_exact_match_moves() {
    let store = common::test_store().await;
    let (acc, _) = tenant_with_chart(&store, "partial").await;
    let customer = customer(&acc, "partial").await;
    let (invoice, number, issued_on) = issued_invoice(&acc, &customer).await;
    acc.post_invoice_issue(&invoice).await.unwrap();

    // €300.00 arrived by hand first, and was booked.
    let deposit = acc
        .record_billing_payment(
            &invoice,
            &NewPayment {
                paid_on: Some(issued_on),
                amount_cents: 30_000,
                method: "bank transfer".to_owned(),
                reference: "deposit".to_owned(),
            },
        )
        .await
        .unwrap();
    acc.post_payment_settle(&invoice, &deposit).await.unwrap();

    let rest = GROSS_CENTS - 30_000;
    let lines = stage(
        &acc,
        &[
            (issued_on, GROSS_CENTS, number.clone()),
            (issued_on, rest, number),
        ],
    )
    .await;

    let suggestions = acc.bank_match_suggestions(None).await.unwrap();
    assert!(
        suggestions.lines[0].exact.is_empty(),
        "the gross is no longer what the document owes"
    );
    assert_eq!(suggestions.lines[1].exact.len(), 1);
    assert_eq!(suggestions.lines[1].exact[0].amount_cents, rest);

    refused(
        acc.confirm_bank_match(&lines[0], &BankMatchTarget::Invoice(invoice.clone()))
            .await,
        "exactly what",
    );
    acc.confirm_bank_match(&lines[1], &BankMatchTarget::Invoice(invoice.clone()))
        .await
        .unwrap();

    let document = acc.billing_invoice(&invoice).await.unwrap().unwrap();
    assert_eq!(document.invoice.status, InvoiceStatus::Paid);
    assert_eq!(
        receivable(&acc).await,
        (0, 0),
        "the last payment carries the receivable to exactly zero"
    );
    assert_eq!(
        acc.bank_line(&lines[0]).await.unwrap().unwrap().status,
        BankLineStatus::Unmatched,
        "a refused confirmation leaves its line alone"
    );
}

#[tokio::test]
async fn money_recorded_in_the_meantime_refuses_the_confirmation_and_writes_nothing() {
    let store = common::test_store().await;
    let (acc, _) = tenant_with_chart(&store, "raced").await;
    let customer = customer(&acc, "raced").await;
    let (invoice, number, issued_on) = issued_invoice(&acc, &customer).await;
    let lines = stage(&acc, &[(issued_on, GROSS_CENTS, number)]).await;

    // The suggestion is real at the moment it is made.
    let suggestions = acc.bank_match_suggestions(None).await.unwrap();
    assert_eq!(suggestions.lines[0].exact.len(), 1);

    // A colleague keys the same money in by hand before anybody clicks it.
    acc.record_billing_payment(
        &invoice,
        &NewPayment {
            paid_on: Some(issued_on),
            amount_cents: GROSS_CENTS,
            method: "bank transfer".to_owned(),
            reference: "keyed in".to_owned(),
        },
    )
    .await
    .unwrap();

    refused(
        acc.confirm_bank_match(&lines[0], &BankMatchTarget::Invoice(invoice.clone()))
            .await,
        "already settled",
    );
    assert_eq!(
        acc.billing_payments(&invoice).await.unwrap().len(),
        1,
        "the money is not counted twice"
    );
    assert_eq!(acc.bank_match(&lines[0]).await.unwrap(), None);
    assert_eq!(
        acc.bank_line(&lines[0]).await.unwrap().unwrap().status,
        BankLineStatus::Unmatched
    );
    assert_eq!(
        acc.fin_invoice_entry(&invoice).await.unwrap(),
        None,
        "a refused confirmation books nothing at all"
    );
}

#[tokio::test]
async fn a_line_is_confirmed_once_and_a_settled_invoice_takes_no_second_line() {
    let store = common::test_store().await;
    let (acc, _) = tenant_with_chart(&store, "once").await;
    let customer = customer(&acc, "once").await;
    let (invoice, number, issued_on) = issued_invoice(&acc, &customer).await;
    let lines = stage(
        &acc,
        &[
            (issued_on, GROSS_CENTS, number.clone()),
            (issued_on, GROSS_CENTS, number),
        ],
    )
    .await;

    acc.confirm_bank_match(&lines[0], &BankMatchTarget::Invoice(invoice.clone()))
        .await
        .unwrap();
    // The same line again: the line itself refuses, whatever the arithmetic.
    refused(
        acc.confirm_bank_match(&lines[0], &BankMatchTarget::Invoice(invoice.clone()))
            .await,
        "already matched",
    );
    // A duplicate transfer the customer sent twice: the document refuses, and
    // the line stays for a person to decide about (a refund is not a payment).
    refused(
        acc.confirm_bank_match(&lines[1], &BankMatchTarget::Invoice(invoice.clone()))
            .await,
        "already settled",
    );
    assert_eq!(acc.billing_payments(&invoice).await.unwrap().len(), 1);
    assert_eq!(
        acc.bank_line(&lines[1]).await.unwrap().unwrap().status,
        BankLineStatus::Unmatched
    );
}

#[tokio::test]
async fn what_the_exact_stage_will_not_confirm() {
    let store = common::test_store().await;
    let (acc, _) = tenant_with_chart(&store, "precision").await;
    let customer = customer(&acc, "precision").await;
    let (invoice, number, issued_on) = issued_invoice(&acc, &customer).await;

    let lines = stage(
        &acc,
        &[
            // One cent short of what is owed.
            (issued_on, GROSS_CENTS - 1, number.clone()),
            // The right amount, quoting nothing.
            (issued_on, GROSS_CENTS, "vielen Dank".to_owned()),
            // The right amount and number, money going the other way.
            (issued_on, -GROSS_CENTS, number.clone()),
            // The right amount and number, before the document existed.
            (issued_on - Duration::days(1), GROSS_CENTS, number.clone()),
            // A number that is one digit longer than ours.
            (issued_on, GROSS_CENTS, format!("{number}1")),
        ],
    )
    .await;

    let suggestions = acc.bank_match_suggestions(None).await.unwrap();
    for (at, line) in suggestions.lines.iter().enumerate() {
        assert!(
            line.exact.is_empty(),
            "line {at} should suggest nothing, got {:?}",
            line.exact
        );
    }
    for (at, expect) in [
        (0, "exactly what"),
        (1, "does not quote"),
        (2, "money leaving"),
        (3, "before that invoice was issued"),
        (4, "does not quote"),
    ] {
        refused(
            acc.confirm_bank_match(&lines[at], &BankMatchTarget::Invoice(invoice.clone()))
                .await,
            expect,
        );
    }
    assert_eq!(acc.billing_payments(&invoice).await.unwrap().len(), 0);
    assert!(acc.fin_entries(None, None, 100).await.unwrap().is_empty());
}

#[tokio::test]
async fn a_chart_without_a_receivable_account_refuses_by_naming_the_role() {
    let store = common::test_store().await;
    // No chart at all: this tenant has never opened the Accounts screen.
    let tenant = store.create_tenant("reconcile-chartless").await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user("chartless@reconciling.test")
        .await
        .unwrap();
    let acc = store.for_account(tenant, user);

    let customer = customer(&acc, "chartless").await;
    let (invoice, number, issued_on) = issued_invoice(&acc, &customer).await;
    let lines = stage(&acc, &[(issued_on, GROSS_CENTS, number)]).await;

    let message = refused(
        acc.confirm_bank_match(&lines[0], &BankMatchTarget::Invoice(invoice.clone()))
            .await,
        "Accounts screen",
    );
    assert!(message.contains("'ar'"), "{message}");
    assert_eq!(
        acc.billing_payments(&invoice).await.unwrap().len(),
        0,
        "the money is not recorded without the books that explain it"
    );
    assert_eq!(
        acc.bank_line(&lines[0]).await.unwrap().unwrap().status,
        BankLineStatus::Unmatched
    );
}

#[tokio::test]
async fn two_tenants_holding_the_same_statement_and_the_same_number_never_meet() {
    let store = common::test_store().await;
    let (ours, _) = tenant_with_chart(&store, "ours").await;
    let (theirs, _) = tenant_with_chart(&store, "theirs").await;
    let our_customer = customer(&ours, "ours").await;
    let their_customer = customer(&theirs, "theirs").await;

    // Both tenants' first invoice: the sequence is per tenant, so both carry
    // the same number, for the same amount, on the same day.
    let (our_invoice, our_number, issued_on) = issued_invoice(&ours, &our_customer).await;
    let (their_invoice, their_number, _) = issued_invoice(&theirs, &their_customer).await;
    assert_eq!(our_number, their_number, "the same number, twice over");

    // And the byte-identical statement, imported by each of them.
    let rows = [(issued_on, GROSS_CENTS, our_number.clone())];
    let our_lines = stage(&ours, &rows).await;
    let their_lines = stage(&theirs, &rows).await;

    // Each sees exactly one suggestion, and it is their own document.
    let our_suggestions = ours.bank_match_suggestions(None).await.unwrap();
    assert_eq!(our_suggestions.lines.len(), 1);
    assert_eq!(our_suggestions.lines[0].exact.len(), 1);
    assert_eq!(our_suggestions.lines[0].exact[0].invoice_id, our_invoice);
    let their_suggestions = theirs.bank_match_suggestions(None).await.unwrap();
    assert_eq!(
        their_suggestions.lines[0].exact[0].invoice_id,
        their_invoice
    );

    // Their line is not ours to confirm, and our invoice is not theirs to
    // settle — both indistinguishable from an id that never existed.
    assert_not_found(
        ours.confirm_bank_match(
            &their_lines[0],
            &BankMatchTarget::Invoice(our_invoice.clone()),
        )
        .await,
    );
    assert_not_found(
        ours.confirm_bank_match(
            &our_lines[0],
            &BankMatchTarget::Invoice(their_invoice.clone()),
        )
        .await,
    );
    assert_eq!(ours.bank_line(&their_lines[0]).await.unwrap(), None);
    assert_eq!(ours.bank_match(&their_lines[0]).await.unwrap(), None);

    // We settle ours. Nothing of theirs moves.
    ours.confirm_bank_match(&our_lines[0], &BankMatchTarget::Invoice(our_invoice))
        .await
        .unwrap();
    assert_eq!(
        theirs
            .bank_line(&their_lines[0])
            .await
            .unwrap()
            .unwrap()
            .status,
        BankLineStatus::Unmatched
    );
    assert_eq!(
        theirs
            .billing_invoice(&their_invoice)
            .await
            .unwrap()
            .unwrap()
            .invoice
            .status,
        InvoiceStatus::Issued
    );
    assert!(
        theirs
            .fin_entries(None, None, 100)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(receivable(&theirs).await, (0, 0));
}
