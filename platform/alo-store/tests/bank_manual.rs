//! **Reconciliation by hand** (alo Finance, ADR 0035, wave B4.09c) — the pick a
//! person makes, the undo of it, and the line that is nobody's document, on the
//! real wire.
//!
//! `src/bank_manual.rs` states the pick's rule as arithmetic and proves it
//! without a database. This suite asserts the six things a pure test cannot:
//!
//! - the **pick**: a line the payer wrote nothing useful on becomes a payment,
//!   two entries and a settled document, because a person said which document
//!   it was;
//! - the **part payment**: less than is owed leaves the document owed, the
//!   receivable standing, and the difference visible in both money columns;
//! - the **undo**: unmatching deletes the payment and **reverses** the entry —
//!   the settlement is still readable in the journal, and so is its mirror —
//!   and puts the line back in the pile;
//! - **the newest first**: a payment with a later one on the same document
//!   refuses to be taken back, naming what to do, so the cumulative relief the
//!   settlement rule computes is never left standing on a prefix that is gone;
//! - the **rule**: a learned rule that proposed the match is recorded on it and
//!   counted once, by the confirmation and never by a read;
//! - **ignoring**: a line nobody has to book leaves the pile with its reason,
//!   and comes back without it;
//! - **tenancy**: two tenants holding the byte-identical statement can neither
//!   see nor touch each other's lines, documents or rules (Law 1).
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::bank_read::BankImportRequest;
use alo_store::{
    AccountRole, AccountStore, BankCsvDates, BankCsvDecimal, BankCsvMapping, BankLineId,
    BankLineStatus, BankMatchTarget, BankSource, BillingCustomerId, BillingInvoiceId, CHART,
    ChartName, ChartSeed, EntryKind, InvoiceStatus, MatchOn, NewCustomer, NewInvoice, NewLine,
    PaymentState, Store, StoreError, TenantId,
};
use time::Date;

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
    let tenant = store.create_tenant(&format!("manual-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@by-hand.test"))
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

/// An issued invoice worth [`GROSS_CENTS`], and the day it was issued.
async fn issued_invoice(
    account: &AccountStore,
    customer: &BillingCustomerId,
) -> (BillingInvoiceId, Date) {
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
    (id, document.invoice.issue_date.expect("an issue date"))
}

/// A CSV statement of `rows` — `(booked on, signed cents, what the payer wrote,
/// who the bank named)` — in the shape a bank portal exports.
fn statement(rows: &[(Date, i64, String, String)]) -> Vec<u8> {
    let mut csv = String::from("date,amount,description,counterparty,reference\n");
    for (index, (booked_on, cents, remittance, counterparty)) in rows.iter().enumerate() {
        let sign = if *cents < 0 { "-" } else { "" };
        csv.push_str(&format!(
            "{booked_on},{sign}{}.{:02},{remittance},{counterparty},REF{index}\n",
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
            counterparty_name: Some("counterparty".to_owned()),
            bank_ref: Some("reference".to_owned()),
            ..Default::default()
        },
    }
}

/// Imports `rows` as a statement and answers the staged lines, in file order.
async fn stage(account: &AccountStore, rows: &[(Date, i64, String, String)]) -> Vec<BankLineId> {
    let report = account
        .import_bank_file(&csv_request(), &statement(rows))
        .await
        .unwrap();
    let imported = report.imported.expect("the file imports");
    assert_eq!(imported.staged, rows.len(), "every row stages");
    let mut lines = account
        .bank_lines(Some(&imported.statement.id), None)
        .await
        .unwrap();
    lines.sort_by_key(|line| line.line_no);
    lines.into_iter().map(|line| line.id).collect()
}

/// One transfer with nothing in the remittance either guessing stage can use.
fn anonymous(day: Date, cents: i64) -> (Date, i64, String, String) {
    (
        day,
        cents,
        "Ueberweisung".to_owned(),
        "Kaffeehaus Bergmann GmbH".to_owned(),
    )
}

/// What the tenant's ledger says the receivable is, in **both** money columns.
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
async fn a_person_picks_the_document_the_payer_never_named() {
    let store = common::test_store().await;
    let (acc, _) = tenant_with_chart(&store, "pick").await;
    let customer = customer(&acc, "pick").await;
    let (invoice, issued_on) = issued_invoice(&acc, &customer).await;
    let lines = stage(&acc, &[anonymous(issued_on, GROSS_CENTS)]).await;

    // Neither guessing stage can name the document by the remittance; the
    // amount alone is what the heuristic has, and a person has the rest.
    let suggestions = acc.bank_match_suggestions(None).await.unwrap();
    assert_eq!(suggestions.lines.len(), 1);
    assert!(
        suggestions.lines[0].exact.is_empty(),
        "nothing quoted, nothing exact"
    );

    let confirmed = acc
        .match_bank_line(&lines[0], &invoice, GROSS_CENTS, None)
        .await
        .unwrap();
    assert!(
        !confirmed.invoice_booked_now,
        "the issue itself booked the document (B7.01); the pick found it there"
    );
    assert_eq!(confirmed.matched.amount_cents, GROSS_CENTS);
    assert_eq!(
        confirmed.matched.target,
        BankMatchTarget::Invoice(invoice.clone())
    );
    assert_eq!(confirmed.matched.rule_id, None, "no rule proposed this");

    // The payment, dated the day the bank booked it, quoting the bank's own
    // reference — the same facts the exact stage records, by a different route.
    let payments = acc.billing_payments(&invoice).await.unwrap();
    assert_eq!(payments.len(), 1);
    assert_eq!(payments[0].amount_cents, GROSS_CENTS);
    assert_eq!(payments[0].paid_on, issued_on);
    assert_eq!(payments[0].reference, "REF0");

    let document = acc.billing_invoice(&invoice).await.unwrap().unwrap();
    assert_eq!(document.invoice.status, InvoiceStatus::Paid);
    assert_eq!(document.settlement().state, PaymentState::Paid);

    let line = acc.bank_line(&lines[0]).await.unwrap().unwrap();
    assert_eq!(line.status, BankLineStatus::Matched);
    assert_eq!(line.ignored_reason, "");

    // Issued and settled: the receivable is exactly zero in both columns.
    assert_eq!(receivable(&acc).await, (0, 0));

    // And the line cannot be spent twice.
    refused(
        acc.match_bank_line(&lines[0], &invoice, GROSS_CENTS, None)
            .await,
        "already matched",
    );
}

#[tokio::test]
async fn a_part_payment_leaves_the_document_owed_and_the_rest_visible() {
    let store = common::test_store().await;
    let (acc, _) = tenant_with_chart(&store, "part").await;
    let customer = customer(&acc, "part").await;
    let (invoice, issued_on) = issued_invoice(&acc, &customer).await;
    let lines = stage(&acc, &[anonymous(issued_on, 50_000)]).await;

    acc.match_bank_line(&lines[0], &invoice, 50_000, None)
        .await
        .unwrap();

    let document = acc.billing_invoice(&invoice).await.unwrap().unwrap();
    assert_eq!(document.invoice.status, InvoiceStatus::Issued);
    let settlement = document.settlement();
    assert_eq!(settlement.state, PaymentState::PartiallyPaid);
    assert_eq!(settlement.outstanding_cents, GROSS_CENTS - 50_000);
    // The books say the same thing the document does, in both columns.
    assert_eq!(
        receivable(&acc).await,
        (GROSS_CENTS - 50_000, GROSS_CENTS - 50_000)
    );
}

#[tokio::test]
async fn more_than_is_owed_is_refused_and_so_is_an_amount_the_bank_did_not_state() {
    let store = common::test_store().await;
    let (acc, _) = tenant_with_chart(&store, "precision").await;
    let customer = customer(&acc, "precision").await;
    let (invoice, issued_on) = issued_invoice(&acc, &customer).await;
    let lines = stage(
        &acc,
        &[
            anonymous(issued_on, GROSS_CENTS + 1),
            (
                issued_on,
                -8_990,
                "Stadtwerke".to_owned(),
                "Stadtwerke Muenchen".to_owned(),
            ),
        ],
    )
    .await;

    // A cent more than the debt.
    refused(
        acc.match_bank_line(&lines[0], &invoice, GROSS_CENTS + 1, None)
            .await,
        "more than",
    );
    // The person's figure has to be the bank's figure.
    refused(
        acc.match_bank_line(&lines[0], &invoice, GROSS_CENTS, None)
            .await,
        "splitting a transfer",
    );
    // Money leaving the account never settles a receivable.
    refused(
        acc.match_bank_line(&lines[1], &invoice, -8_990, None).await,
        "money leaving",
    );

    // Nothing was written by any of them: no payment, and the receivable
    // stands exactly as the issue booked it (B7.01).
    assert!(acc.billing_payments(&invoice).await.unwrap().is_empty());
    assert_eq!(receivable(&acc).await, (GROSS_CENTS, GROSS_CENTS));
    for line in &lines {
        assert_eq!(
            acc.bank_line(line).await.unwrap().unwrap().status,
            BankLineStatus::Unmatched
        );
    }
}

#[tokio::test]
async fn taking_a_match_back_reverses_the_entry_and_keeps_both_readable() {
    let store = common::test_store().await;
    let (acc, _) = tenant_with_chart(&store, "undo").await;
    let customer = customer(&acc, "undo").await;
    let (invoice, issued_on) = issued_invoice(&acc, &customer).await;
    let lines = stage(&acc, &[anonymous(issued_on, GROSS_CENTS)]).await;

    let confirmed = acc
        .match_bank_line(&lines[0], &invoice, GROSS_CENTS, None)
        .await
        .unwrap();
    let settlement_entry = confirmed.matched.entry_id.clone().expect("an entry");
    assert_eq!(receivable(&acc).await, (0, 0));

    let unmatched = acc.unmatch_bank_line(&lines[0]).await.unwrap();
    assert_eq!(unmatched.amount_cents, GROSS_CENTS);
    assert_eq!(unmatched.target, BankMatchTarget::Invoice(invoice.clone()));

    // The payment is gone, because no money arrived.
    assert!(acc.billing_payments(&invoice).await.unwrap().is_empty());
    let document = acc.billing_invoice(&invoice).await.unwrap().unwrap();
    assert_eq!(document.invoice.status, InvoiceStatus::Issued);
    assert_eq!(document.settlement().state, PaymentState::Unpaid);

    // The entry is NOT gone: it and its mirror are both in the journal, and
    // together they leave the receivable exactly where the issue put it.
    let original = acc
        .fin_journal_entry(&settlement_entry)
        .await
        .unwrap()
        .expect("the settlement is still readable");
    let reversal = acc
        .fin_journal_entry(&unmatched.reversal_entry_id)
        .await
        .unwrap()
        .expect("the reversal");
    assert_eq!(reversal.entry.kind, EntryKind::Reversal);
    assert_eq!(
        reversal.entry.reverses_entry_id.as_ref(),
        Some(&settlement_entry)
    );
    assert_eq!(
        reversal.entry.entry_date, original.entry.entry_date,
        "a correction belongs in the period the thing it corrects moved money in"
    );
    assert_eq!(reversal.postings.len(), original.postings.len());
    for (mirror, posting) in reversal.postings.iter().zip(&original.postings) {
        assert_eq!(mirror.account_id, posting.account_id);
        assert_eq!(mirror.amount_cents, -posting.amount_cents);
        assert_eq!(mirror.base_cents, -posting.base_cents);
        assert_eq!(
            mirror.customer_id, posting.customer_id,
            "a relief taken back is taken back against the same customer"
        );
    }
    assert_eq!(
        receivable(&acc).await,
        (GROSS_CENTS, GROSS_CENTS),
        "the document is owed again"
    );

    // The line is back in the pile, with no match on it, and can be matched
    // again — the undo left nothing behind that stops a person trying again.
    let line = acc.bank_line(&lines[0]).await.unwrap().unwrap();
    assert_eq!(line.status, BankLineStatus::Unmatched);
    assert_eq!(acc.bank_match(&lines[0]).await.unwrap(), None);
    // A line carrying no match has nothing to take back, and says so the way
    // every absent record in alo does.
    assert_not_found(acc.unmatch_bank_line(&lines[0]).await);
    let again = acc
        .match_bank_line(&lines[0], &invoice, GROSS_CENTS, None)
        .await
        .unwrap();
    assert!(
        !again.invoice_booked_now,
        "the issue was booked the first time and is booked once"
    );
    assert_eq!(receivable(&acc).await, (0, 0));
}

#[tokio::test]
async fn only_the_newest_payment_on_a_document_can_be_taken_back() {
    let store = common::test_store().await;
    let (acc, _) = tenant_with_chart(&store, "lifo").await;
    let customer = customer(&acc, "lifo").await;
    let (invoice, issued_on) = issued_invoice(&acc, &customer).await;
    let lines = stage(
        &acc,
        &[anonymous(issued_on, 50_000), anonymous(issued_on, 80_700)],
    )
    .await;

    acc.match_bank_line(&lines[0], &invoice, 50_000, None)
        .await
        .unwrap();
    acc.match_bank_line(&lines[1], &invoice, 80_700, None)
        .await
        .unwrap();
    assert_eq!(receivable(&acc).await, (0, 0));

    // The first one cannot go while the second stands on it.
    refused(
        acc.unmatch_bank_line(&lines[0]).await,
        "take that one back first",
    );
    assert_eq!(acc.billing_payments(&invoice).await.unwrap().len(), 2);

    // In the other order, both come back — and the receivable with them.
    acc.unmatch_bank_line(&lines[1]).await.unwrap();
    acc.unmatch_bank_line(&lines[0]).await.unwrap();
    assert!(acc.billing_payments(&invoice).await.unwrap().is_empty());
    assert_eq!(receivable(&acc).await, (GROSS_CENTS, GROSS_CENTS));
}

#[tokio::test]
async fn a_rule_that_proposed_the_match_is_counted_by_the_confirmation() {
    let store = common::test_store().await;
    let (acc, _) = tenant_with_chart(&store, "rule").await;
    let customer = customer(&acc, "rule").await;
    let (invoice, issued_on) = issued_invoice(&acc, &customer).await;
    let lines = stage(&acc, &[anonymous(issued_on, GROSS_CENTS)]).await;

    let rule = acc
        .learn_fin_match_rule(&lines[0], MatchOn::Counterparty, &customer)
        .await
        .unwrap();
    assert_eq!(rule.hits, 0, "a rule is not counted by being written");

    // The screen offers it, with the rule named.
    let suggestions = acc.bank_match_suggestions(None).await.unwrap();
    let likely = &suggestions.lines[0].likely;
    assert_eq!(likely.len(), 1);
    assert_eq!(likely[0].rule_id.as_ref(), Some(&rule.id));
    assert_eq!(
        acc.fin_match_rules().await.unwrap()[0].hits,
        0,
        "reading the screen is not a hit"
    );

    let confirmed = acc
        .match_bank_line(&lines[0], &invoice, GROSS_CENTS, Some(&rule.id))
        .await
        .unwrap();
    assert_eq!(
        confirmed.matched.rule_id.as_deref(),
        Some(rule.id.as_str()),
        "the match records which rule earned it"
    );
    assert_eq!(acc.fin_match_rules().await.unwrap()[0].hits, 1);

    // The match reads back with the rule on it, from the database.
    let stored = acc.bank_match(&lines[0]).await.unwrap().expect("a match");
    assert_eq!(stored.rule_id.as_deref(), Some(rule.id.as_str()));

    // Forgetting the rule leaves the match alone: it is money somebody
    // confirmed, and `rule_id` on it is history rather than a link.
    acc.delete_fin_match_rule(&rule.id).await.unwrap();
    let stored = acc.bank_match(&lines[0]).await.unwrap().expect("a match");
    assert_eq!(stored.rule_id.as_deref(), Some(rule.id.as_str()));
}

#[tokio::test]
async fn a_line_nobody_has_to_book_leaves_the_pile_with_its_reason() {
    let store = common::test_store().await;
    let (acc, _) = tenant_with_chart(&store, "ignore").await;
    let customer = customer(&acc, "ignore").await;
    let (invoice, issued_on) = issued_invoice(&acc, &customer).await;
    let lines = stage(
        &acc,
        &[
            anonymous(issued_on, GROSS_CENTS),
            (issued_on, -450, "Kontofuehrung".to_owned(), String::new()),
        ],
    )
    .await;

    // A reason is required: "ignored" alone is a state nobody can audit.
    refused(acc.ignore_bank_line(&lines[1], "   ").await, "say why");

    let ignored = acc
        .ignore_bank_line(&lines[1], " the bank's own account fee ")
        .await
        .unwrap();
    assert_eq!(ignored.status, BankLineStatus::Ignored);
    assert_eq!(ignored.ignored_reason, "the bank's own account fee");

    // It is off the reconciliation screen, and off the unmatched list.
    let suggestions = acc.bank_match_suggestions(None).await.unwrap();
    assert_eq!(suggestions.lines.len(), 1);
    assert_eq!(suggestions.lines[0].line.id, lines[0]);
    let open = acc
        .bank_lines(None, Some(BankLineStatus::Unmatched))
        .await
        .unwrap();
    assert_eq!(open.len(), 1);

    // Nothing was booked by dismissing it: the receivable is still exactly
    // what the issue booked (B7.01).
    assert_eq!(receivable(&acc).await, (GROSS_CENTS, GROSS_CENTS));
    refused(
        acc.match_bank_line(&lines[1], &invoice, -450, None).await,
        "not ours to book",
    );

    // Saying it again with a better sentence is a person fixing their words.
    let ignored = acc
        .ignore_bank_line(&lines[1], "account fee, booked by the bank")
        .await
        .unwrap();
    assert_eq!(ignored.ignored_reason, "account fee, booked by the bank");

    // Taking it back clears the sentence with the status.
    let back = acc.unignore_bank_line(&lines[1]).await.unwrap();
    assert_eq!(back.status, BankLineStatus::Unmatched);
    assert_eq!(back.ignored_reason, "");
    refused(acc.unignore_bank_line(&lines[1]).await, "not marked");

    // A matched line is money in the books and is not dismissed out of them.
    acc.match_bank_line(&lines[0], &invoice, GROSS_CENTS, None)
        .await
        .unwrap();
    refused(
        acc.ignore_bank_line(&lines[0], "changed my mind").await,
        "take that back",
    );
}

#[tokio::test]
async fn two_tenants_holding_the_same_statement_never_reach_each_others_lines() {
    let store = common::test_store().await;
    let (ours, _) = tenant_with_chart(&store, "ours").await;
    let (theirs, _) = tenant_with_chart(&store, "theirs").await;
    let our_customer = customer(&ours, "ours").await;
    let their_customer = customer(&theirs, "theirs").await;
    let (our_invoice, issued_on) = issued_invoice(&ours, &our_customer).await;
    let (their_invoice, _) = issued_invoice(&theirs, &their_customer).await;

    // The byte-identical statement, imported by both.
    let rows = [anonymous(issued_on, GROSS_CENTS)];
    let our_lines = stage(&ours, &rows).await;
    let their_lines = stage(&theirs, &rows).await;
    assert_ne!(our_lines[0], their_lines[0]);

    // Their handle cannot pick our line, nor our document with their line.
    assert_not_found(
        theirs
            .match_bank_line(&our_lines[0], &their_invoice, GROSS_CENTS, None)
            .await,
    );
    assert_not_found(
        theirs
            .match_bank_line(&their_lines[0], &our_invoice, GROSS_CENTS, None)
            .await,
    );
    // Nor dismiss it, nor take it back, nor read its match.
    assert_not_found(theirs.ignore_bank_line(&our_lines[0], "not ours").await);
    assert_not_found(theirs.unmatch_bank_line(&our_lines[0]).await);
    assert_eq!(theirs.bank_match(&our_lines[0]).await.unwrap(), None);

    // A rule of ours is not a rule they can spend.
    let our_rule = ours
        .learn_fin_match_rule(&our_lines[0], MatchOn::Counterparty, &our_customer)
        .await
        .unwrap();
    assert_not_found(
        theirs
            .match_bank_line(
                &their_lines[0],
                &their_invoice,
                GROSS_CENTS,
                Some(&our_rule.id),
            )
            .await,
    );
    assert_eq!(
        our_rule.hits, 0,
        "a refused settlement counts no hit against our rule"
    );
    assert_eq!(ours.fin_match_rules().await.unwrap()[0].hits, 0);

    // Each settles their own, and neither ledger knows about the other.
    ours.match_bank_line(&our_lines[0], &our_invoice, GROSS_CENTS, None)
        .await
        .unwrap();
    assert_eq!(receivable(&ours).await, (0, 0));
    assert_eq!(
        receivable(&theirs).await,
        (GROSS_CENTS, GROSS_CENTS),
        "their receivable stands exactly as their own issue booked it"
    );
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
    // And ours cannot be taken back through their door.
    assert_not_found(theirs.unmatch_bank_line(&our_lines[0]).await);
    assert_eq!(
        ours.bank_line(&our_lines[0]).await.unwrap().unwrap().status,
        BankLineStatus::Matched
    );
}
