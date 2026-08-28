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
        !confirmed.invoice_booked_now,
        "the issue itself booked the document (B7.01); the confirmation found it there"
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
    let issue_entry = acc.fin_invoice_entry(&invoice).await.unwrap().unwrap();

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

    // €300.00 arrived by hand first — recording it books it (B7.01).
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
    assert!(acc.fin_payment_entry(&deposit).await.unwrap().is_some());

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
        acc.fin_entries(None, None, 10).await.unwrap().len(),
        2,
        "the issue's entry and the keyed payment's settlement (B7.01) — the          refused confirmation added nothing beside them"
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
    assert_eq!(
        acc.fin_entries(None, None, 100).await.unwrap().len(),
        1,
        "only the issue's own entry — no refusal booked anything"
    );
}

#[tokio::test]
async fn a_chart_missing_a_role_refuses_by_naming_the_role() {
    let store = common::test_store().await;
    // No chart at all: this tenant has never opened the Accounts screen.
    // Since B7.01 the refusal reaches them at the ISSUE, not at the match —
    // an invoice that cannot be booked is not issued in the first place.
    let tenant = store.create_tenant("reconcile-chartless").await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user("chartless@reconciling.test")
        .await
        .unwrap();
    let acc = store.for_account(tenant, user);
    let customer = customer(&acc, "chartless").await;
    let draft = acc
        .create_billing_invoice(&NewInvoice::for_customer(customer.clone()))
        .await
        .unwrap();
    acc.set_billing_invoice_lines(&draft, &lines())
        .await
        .unwrap();
    let refusal = match acc.issue_billing_invoice(&draft).await {
        Err(StoreError::Validation(message)) => message,
        other => panic!("expected a validation refusal, got {other:?}"),
    };
    assert!(refusal.contains("'ar'"), "{refusal}");
    assert!(refusal.contains("Accounts screen"), "{refusal}");

    // A chart that opened and then lost a role refuses the CONFIRMATION the
    // same way: the settlement needs somewhere for the money to land.
    common::seed_default_chart(&acc).await;
    let (invoice, number, issued_on) = issued_invoice(&acc, &customer).await;
    let bank = acc
        .fin_account_for_role(AccountRole::Bank)
        .await
        .unwrap()
        .unwrap();
    acc.set_fin_account_active(&bank.id, false).await.unwrap();
    let lines = stage(&acc, &[(issued_on, GROSS_CENTS, number)]).await;

    let message = refused(
        acc.confirm_bank_match(&lines[0], &BankMatchTarget::Invoice(invoice.clone()))
            .await,
        "Accounts screen",
    );
    assert!(message.contains("'bank'"), "{message}");
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
    assert_eq!(
        theirs.fin_entries(None, None, 100).await.unwrap().len(),
        1,
        "their journal holds their own issue entry and nothing of ours"
    );
    assert_eq!(
        receivable(&theirs).await,
        (GROSS_CENTS, GROSS_CENTS),
        "their receivable stands exactly as their issue booked it"
    );
}

/// A payment a bank line settles cannot be deleted through the billing door
/// (B7.02): the line would go on claiming to be settled by money that is gone.
/// The refusal names the act that does it right, and that act — unmatching —
/// still works and does all three things at once.
#[tokio::test]
async fn a_matched_payment_refuses_deletion_and_unmatching_is_the_door() {
    let store = common::test_store().await;
    let (acc, _) = tenant_with_chart(&store, "guard").await;
    let customer = customer(&acc, "guard").await;
    let (invoice, number, issued_on) = issued_invoice(&acc, &customer).await;
    let lines = stage(&acc, &[(issued_on, GROSS_CENTS, number)]).await;
    acc.confirm_bank_match(&lines[0], &BankMatchTarget::Invoice(invoice.clone()))
        .await
        .unwrap();
    let payments = acc.billing_payments(&invoice).await.unwrap();
    assert_eq!(payments.len(), 1);

    // The billing door refuses, whole: the payment stays, the line stays
    // matched, the document stays settled, and no reversal was written.
    let entries_before = acc.fin_entries(None, None, 100).await.unwrap().len();
    refused(
        acc.delete_billing_payment(&invoice, &payments[0].id).await,
        "take the match back",
    );
    assert_eq!(acc.billing_payments(&invoice).await.unwrap().len(), 1);
    assert_eq!(
        acc.bank_line(&lines[0]).await.unwrap().unwrap().status,
        BankLineStatus::Matched
    );
    assert_eq!(
        acc.billing_invoice(&invoice)
            .await
            .unwrap()
            .unwrap()
            .invoice
            .status,
        InvoiceStatus::Paid
    );
    assert_eq!(
        acc.fin_entries(None, None, 100).await.unwrap().len(),
        entries_before,
        "a refused deletion writes nothing to the books"
    );

    // The named door: one act removes the payment, reverses the settlement and
    // returns the line to the pile.
    acc.unmatch_bank_line(&lines[0]).await.unwrap();
    assert!(acc.billing_payments(&invoice).await.unwrap().is_empty());
    assert_eq!(
        acc.bank_line(&lines[0]).await.unwrap().unwrap().status,
        BankLineStatus::Unmatched
    );
    assert_eq!(receivable(&acc).await, (GROSS_CENTS, GROSS_CENTS));
}

/// Law 1's last verb (B7.02): deleting a tenant who has imported, matched and
/// booked leaves **nothing** behind — erasure is a GDPR obligation, and it must
/// not depend on which cascade Postgres runs first. Migration 0143's RESTRICT
/// keys made exactly that dependence; 0174 softens them to NO ACTION and
/// `delete_tenant` clears the matches itself before the cascade starts, so no
/// ordering can leave a check staring at a match the cascade has not reached.
/// Two tenants, so the deletion also proves it takes nobody else's rows with
/// it.
#[tokio::test]
async fn deleting_a_tenant_who_reconciled_erases_everything_and_only_theirs() {
    let store = common::test_store().await;
    let (going, going_tenant) = tenant_with_chart(&store, "going").await;
    let (staying, staying_tenant) = tenant_with_chart(&store, "staying").await;

    // Both tenants live the same full life: an issued invoice, a hand-keyed
    // deposit, a statement of two lines, the rest confirmed against the
    // document — payments both matched and manual, entries of every kind, and
    // one line left unmatched in the pile.
    for (acc, tag) in [(&going, "going"), (&staying, "staying")] {
        let customer = customer(acc, tag).await;
        let (invoice, number, issued_on) = issued_invoice(acc, &customer).await;
        acc.record_billing_payment(
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
        let lines = stage(
            acc,
            &[
                (issued_on, GROSS_CENTS - 30_000, number),
                (issued_on, -8_990, "Stadtwerke Muenchen".to_owned()),
            ],
        )
        .await;
        acc.confirm_bank_match(&lines[0], &BankMatchTarget::Invoice(invoice.clone()))
            .await
            .unwrap();
        assert_eq!(receivable(acc).await, (0, 0));
    }

    // The act under test: before 0174 this failed on
    // `bank_matches_tenant_id_payment_id_fkey` the moment the cascade reached
    // a payment the match still named.
    store.delete_tenant(&going_tenant).await.unwrap();

    // The sweep: every bank, billing and finance table that carries a
    // tenant_id holds not one row of the deleted tenant. Read raw — a scoped
    // query would hide an orphan by construction.
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&common::database_url())
        .await
        .unwrap();
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT c.table_name FROM information_schema.columns c \
         JOIN information_schema.tables t \
           ON t.table_schema = c.table_schema AND t.table_name = c.table_name \
         WHERE c.table_schema = 'public' AND c.column_name = 'tenant_id' \
           AND t.table_type = 'BASE TABLE' \
           AND (c.table_name LIKE 'bank\\_%' \
             OR c.table_name LIKE 'billing\\_%' \
             OR c.table_name LIKE 'fin\\_%') \
         ORDER BY c.table_name",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    for expected in [
        "bank_lines",
        "bank_matches",
        "bank_statements",
        "billing_invoices",
        "billing_payments",
        "fin_entries",
        "fin_postings",
    ] {
        assert!(
            tables.iter().any(|table| table == expected),
            "the sweep must cover {expected}; it found {tables:?}"
        );
    }
    for table in &tables {
        let remaining: i64 = sqlx::query_scalar(&format!(
            "SELECT count(*) FROM {table} WHERE tenant_id = $1"
        ))
        .bind(going_tenant.as_str())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(remaining, 0, "{table} still holds rows of a deleted tenant");
    }

    // The other tenant's world is exactly as they left it.
    let mut kept: i64 = 0;
    for table in &tables {
        let count: i64 = sqlx::query_scalar(&format!(
            "SELECT count(*) FROM {table} WHERE tenant_id = $1"
        ))
        .bind(staying_tenant.as_str())
        .fetch_one(&pool)
        .await
        .unwrap();
        kept += count;
    }
    assert!(
        kept > 0,
        "the surviving tenant's reconciliation is still there"
    );
    assert_eq!(receivable(&staying).await, (0, 0));
    assert!(staying.fin_unbalanced_entries().await.unwrap().is_empty());
}
