//! **Booking an issued invoice** (alo Finance, ADR 0035, wave B4.04a) — the
//! first posting rule, on the real wire: a document raised through the billing
//! store, issued through the gapless sequence, and booked through
//! [`alo_store::AccountStore::post_invoice_issue`] into the journal it then
//! has to be read back out of.
//!
//! `src/fin_rules.rs` holds the hand-written golden entry, asserted against the
//! pure function. This suite asserts the four things a pure test cannot:
//!
//! - the entry that reaches the **database** is that golden entry, with the
//!   accounts the tenant's own chart gives the roles `ar`, `revenue` and
//!   `vat_output`, and with the customer and rate dimensions a report groups by;
//! - **P3 on the wire** — the receivable the ledger carries is
//!   `billing_totals`' gross for the same document, and the trial balance the
//!   reports will fold over says the same thing;
//! - **idempotency** — booking twice is a typed `Conflict` and writes nothing
//!   the second time (P7, now through a rule rather than a generator);
//! - **tenancy** — another tenant's invoice id is a `NotFound` through this
//!   handle, and their books are untouched by the attempt.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::{
    Account, AccountRole, AccountStore, BillingCustomerId, BillingInvoiceId, CHART, ChartName,
    ChartSeed, EntryKind, FinAccountId, InvoiceDocument, LineFigures, NewCustomer, NewInvoice,
    NewLine, SourceEvent, SourceKind, Store, StoreError, TenantId, billing_fx::restated_into,
    billing_totals::totals,
};
use time::Date;

fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::NotFound) => {}
        other => panic!("expected NotFound, got {other:?}"),
    }
}

fn assert_conflict<T: std::fmt::Debug>(result: Result<T, StoreError>) -> String {
    match result {
        Err(StoreError::Conflict(message)) => message,
        other => panic!("expected Conflict, got {other:?}"),
    }
}

fn assert_validation<T: std::fmt::Debug>(result: Result<T, StoreError>) -> String {
    match result {
        Err(StoreError::Validation(message)) => message,
        other => panic!("expected Validation, got {other:?}"),
    }
}

/// The chart, named per tenant so a leak between two of them would show up as a
/// name from the wrong tenant rather than as a number that happens to match.
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

async fn tenant_with_chart(
    store: &Store,
    tag: &str,
) -> (AccountStore, TenantId, BillingCustomerId) {
    let tenant = store.create_tenant(&format!("book-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@booking.test"))
        .await
        .unwrap();
    let account = store.for_account(tenant.clone(), user);
    account
        .fin_accounts_or_seed(&seed(tag), false)
        .await
        .unwrap();
    let customer = account
        .create_billing_customer(&NewCustomer {
            name: format!("Customer {tag}"),
            country: "NL".to_owned(),
            currency: "EUR".to_owned(),
            payment_terms_days: 30,
            ..Default::default()
        })
        .await
        .unwrap();
    (account, tenant, customer)
}

async fn role_account(account: &AccountStore, role: AccountRole) -> Account {
    account
        .fin_account_for_role(role)
        .await
        .unwrap()
        .unwrap_or_else(|| panic!("the seeded chart holds {}", role.as_str()))
}

/// The two-rate document `src/fin_rules.rs`'s golden is written for: €900 of
/// consulting at 21 % (ten hours less an hour of goodwill) and €200 of books at
/// 9 %.
const LINES: &[(i64, i64, i32)] = &[
    (10_000, 10_000, 2100),
    (4_000, 5_000, 900),
    (-1_000, 10_000, 2100),
];

fn lines() -> Vec<NewLine> {
    LINES
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

/// What the lines are worth, computed here from `billing_totals` — the
/// independent side of P3, so the assertions below do not read the figure they
/// are checking out of the thing under test.
fn expected_totals() -> alo_store::Totals {
    let figures: Vec<LineFigures> = LINES
        .iter()
        .map(|&(qty_milli, unit_price_cents, vat_rate_bp)| LineFigures {
            qty_milli,
            unit_price_cents,
            vat_rate_bp,
        })
        .collect();
    totals(&figures)
}

async fn issued_invoice(
    account: &AccountStore,
    customer: &BillingCustomerId,
) -> (BillingInvoiceId, InvoiceDocument) {
    let id = account
        .create_billing_invoice(&NewInvoice::for_customer(customer.clone()))
        .await
        .unwrap();
    account
        .set_billing_invoice_lines(&id, &lines())
        .await
        .unwrap();
    let document = account.issue_billing_invoice(&id).await.unwrap();
    (id, document)
}

/// A posting as this suite compares them: the account, the two columns, and the
/// dimensions a report groups by.
type Row = (String, i64, i64, Option<i32>, Option<String>);

async fn rows(account: &AccountStore, entry: &alo_store::FinEntryId) -> Vec<Row> {
    account
        .fin_entry_postings(entry)
        .await
        .unwrap()
        .into_iter()
        .map(|posting| {
            (
                posting.account_id.as_str().to_owned(),
                posting.amount_cents,
                posting.base_cents,
                posting.vat_rate_bp,
                posting.customer_id,
            )
        })
        .collect()
}

/// The golden entry, in the database, with the tenant's own accounts.
#[tokio::test]
async fn booking_an_issued_invoice_writes_the_golden_entry() {
    let store = common::test_store().await;
    let (account, _tenant, customer) = tenant_with_chart(&store, "golden").await;
    let (id, document) = issued_invoice(&account, &customer).await;

    let entry_id = account
        .fin_invoice_entry(&id)
        .await
        .unwrap()
        .expect("issuing books the document in the same transaction (B7.01)");

    let booked = account
        .fin_journal_entry(&entry_id)
        .await
        .unwrap()
        .unwrap_or_else(|| panic!("the entry is readable"));
    assert_eq!(booked.entry.kind, EntryKind::Invoice);
    assert_eq!(
        booked.entry.entry_date,
        document.invoice.issue_date.unwrap(),
        "the document's date, never today"
    );
    assert_eq!(booked.entry.memo, document.invoice.number.clone().unwrap());
    assert_eq!(booked.entry.currency, "EUR");
    let source = booked.entry.source.unwrap_or_else(|| panic!("a source"));
    assert_eq!(source.kind, SourceKind::Invoice);
    assert_eq!(source.id, id.as_str());
    assert_eq!(source.event, SourceEvent::Issue);

    let ar = role_account(&account, AccountRole::Ar).await;
    let revenue = role_account(&account, AccountRole::Revenue).await;
    let vat = role_account(&account, AccountRole::VatOutput).await;
    let customer_id = customer.as_str().to_owned();
    assert_eq!(
        rows(&account, &entry_id).await,
        vec![
            (
                ar.id.as_str().to_owned(),
                130_700,
                130_700,
                None,
                Some(customer_id)
            ),
            (
                revenue.id.as_str().to_owned(),
                -20_000,
                -20_000,
                Some(900),
                None
            ),
            (vat.id.as_str().to_owned(), -1_800, -1_800, Some(900), None),
            (
                revenue.id.as_str().to_owned(),
                -90_000,
                -90_000,
                Some(2100),
                None
            ),
            (
                vat.id.as_str().to_owned(),
                -18_900,
                -18_900,
                Some(2100),
                None
            ),
        ]
    );
    assert!(
        account.fin_unbalanced_entries().await.unwrap().is_empty(),
        "P1, re-derived from the database rather than from the rule"
    );
}

/// **P3 on the wire.** Every figure the ledger holds for the document is
/// `billing_totals`' figure for the same lines — the receivable, each rate's
/// net and each rate's tax — and the trial balance the reports fold over says
/// the same thing account by account.
#[tokio::test]
async fn the_ledger_holds_exactly_what_billing_computed() {
    let store = common::test_store().await;
    let (account, _tenant, customer) = tenant_with_chart(&store, "p3").await;
    let (id, document) = issued_invoice(&account, &customer).await;
    let entry_id = account.fin_invoice_entry(&id).await.unwrap().unwrap();

    let expected = expected_totals();
    assert_eq!(
        document.totals, expected,
        "the store's own totals are the ones this test computes independently"
    );

    let ar = role_account(&account, AccountRole::Ar).await;
    let revenue = role_account(&account, AccountRole::Revenue).await;
    let vat = role_account(&account, AccountRole::VatOutput).await;
    let postings = account.fin_entry_postings(&entry_id).await.unwrap();
    let at = |account_id: &FinAccountId, rate: Option<i32>| -> i64 {
        postings
            .iter()
            .filter(|posting| {
                posting.account_id.as_str() == account_id.as_str()
                    && (rate.is_none() || posting.vat_rate_bp == rate)
            })
            .map(|posting| posting.amount_cents)
            .sum()
    };
    assert_eq!(at(&ar.id, None), expected.gross_cents);
    for subtotal in &expected.vat_by_rate {
        assert_eq!(-at(&revenue.id, Some(subtotal.rate_bp)), subtotal.net_cents);
        assert_eq!(-at(&vat.id, Some(subtotal.rate_bp)), subtotal.vat_cents);
    }

    // The same statement, one layer up: what the reports will read.
    let trial = account.fin_trial_balance(None, None).await.unwrap();
    assert!(trial.balances(), "the period's debits equal its credits");
    let balance = |account_id: &FinAccountId| -> i64 {
        trial
            .accounts
            .iter()
            .find(|row| row.account_id.as_str() == account_id.as_str())
            .map(|row| row.balance_cents)
            .unwrap_or_else(|| panic!("the account appears in the trial balance"))
    };
    assert_eq!(balance(&ar.id), expected.gross_cents);
    assert_eq!(-balance(&revenue.id), expected.net_cents);
    assert_eq!(-balance(&vat.id), expected.vat_cents);
    // And in the accounting currency it is the figure the document itself
    // prints: a EUR document restates into EUR as itself.
    assert_eq!(
        restated_into("EUR", document.invoice.fx.as_ref(), &document.totals)
            .unwrap_or_else(|| panic!("a snapshot restates"))
            .gross_cents,
        expected.gross_cents
    );
}

/// **P7 through a rule.** A retry, a double-click and a re-run of a backfill
/// all hit the idempotency key: one entry, a typed conflict, and not one extra
/// posting.
#[tokio::test]
async fn booking_the_same_invoice_twice_changes_nothing() {
    let store = common::test_store().await;
    let (account, _tenant, customer) = tenant_with_chart(&store, "twice").await;
    let (id, _) = issued_invoice(&account, &customer).await;

    let entry_id = account.fin_invoice_entry(&id).await.unwrap().unwrap();
    let before = rows(&account, &entry_id).await;
    // The explicit door is now the retry/backfill path, and it answers a
    // typed conflict for a document the issue already booked.
    let message = assert_conflict(account.post_invoice_issue(&id).await);
    assert!(message.contains("already posted"), "{message}");

    assert_eq!(
        account.fin_entries(None, None, 100).await.unwrap().len(),
        1,
        "the second call wrote no second entry"
    );
    assert_eq!(rows(&account, &entry_id).await, before);
}

/// Only a document that is an event books at issue: a draft is an intention, a
/// void one is corrected by its reversal, and a credit note has its own rule.
/// Each refusal leaves the journal empty.
#[tokio::test]
async fn a_draft_a_void_and_a_credit_note_are_refused() {
    let store = common::test_store().await;
    let (account, _tenant, customer) = tenant_with_chart(&store, "refused").await;

    let draft = account
        .create_billing_invoice(&NewInvoice::for_customer(customer.clone()))
        .await
        .unwrap();
    account
        .set_billing_invoice_lines(&draft, &lines())
        .await
        .unwrap();
    assert!(
        assert_conflict(account.post_invoice_issue(&draft).await).contains("draft"),
        "a draft is an intention"
    );
    assert!(
        account
            .fin_entries(None, None, 100)
            .await
            .unwrap()
            .is_empty(),
        "a refused draft books nothing"
    );

    let (voided, _) = issued_invoice(&account, &customer).await;
    account.void_billing_invoice(&voided).await.unwrap();
    assert!(assert_conflict(account.post_invoice_issue(&voided).await).contains("void"));

    let (original, _) = issued_invoice(&account, &customer).await;
    let credit = account.create_billing_credit_note(&original).await.unwrap();
    assert!(
        assert_conflict(account.post_invoice_issue(&credit).await).contains("credit note"),
        "a credit note is the credit-note rule's document"
    );

    assert_eq!(
        account.fin_entries(None, None, 100).await.unwrap().len(),
        3,
        "the journal holds the two issues and the void's reversal, and the \
         refusals added nothing beside them"
    );
}

/// A chart without the role a rule needs **refuses the document**, naming the
/// role — never a silent posting to suspense, which is discovered at the year
/// end by somebody who cannot remember the invoice.
#[tokio::test]
async fn a_chart_missing_a_role_refuses_the_issue_and_burns_no_number() {
    let store = common::test_store().await;
    let (account, _tenant, customer) = tenant_with_chart(&store, "norole").await;
    let ar = role_account(&account, AccountRole::Ar).await;
    account.set_fin_account_active(&ar.id, false).await.unwrap();

    let draft = account
        .create_billing_invoice(&NewInvoice::for_customer(customer.clone()))
        .await
        .unwrap();
    account
        .set_billing_invoice_lines(&draft, &lines())
        .await
        .unwrap();
    let message = assert_validation(account.issue_billing_invoice(&draft).await);
    assert!(
        message.contains("'ar'"),
        "the refusal names the role: {message}"
    );
    assert!(
        account
            .fin_entries(None, None, 100)
            .await
            .unwrap()
            .is_empty(),
        "a refused issue writes no half-entry"
    );
    let document = account.billing_invoice(&draft).await.unwrap().unwrap();
    assert_eq!(
        document.invoice.status,
        alo_store::InvoiceStatus::Draft,
        "and the document is still a draft"
    );
    assert_eq!(document.invoice.number, None);

    // Reactivated, the same document issues — with the FIRST number of the
    // series: the refused issue gave its drawn number back.
    account.set_fin_account_active(&ar.id, true).await.unwrap();
    let document = account.issue_billing_invoice(&draft).await.unwrap();
    let number = document.invoice.number.unwrap();
    assert!(
        number.ends_with("-00001"),
        "the refusal burned no number: {number}"
    );
    assert!(account.fin_invoice_entry(&draft).await.unwrap().is_some());
}

/// **Law 1.** Another tenant's invoice id is a `NotFound` through this handle —
/// not a `500`, not a booking — and the attempt leaves both sets of books
/// exactly as they were.
#[tokio::test]
async fn another_tenants_invoice_can_never_be_booked() {
    let store = common::test_store().await;
    let (theirs, _t1, their_customer) = tenant_with_chart(&store, "a").await;
    let (ours, _t2, our_customer) = tenant_with_chart(&store, "b").await;

    let (their_invoice, _) = issued_invoice(&theirs, &their_customer).await;
    let their_entry = theirs
        .fin_invoice_entry(&their_invoice)
        .await
        .unwrap()
        .unwrap();
    let their_rows = rows(&theirs, &their_entry).await;

    assert_not_found(ours.post_invoice_issue(&their_invoice).await);
    assert_eq!(
        ours.fin_invoice_entry(&their_invoice).await.unwrap(),
        None,
        "their document is not in our books, and asking does not say otherwise"
    );
    assert!(
        ours.fin_journal_entry(&their_entry)
            .await
            .unwrap()
            .is_none(),
        "their entry is not readable through our handle"
    );
    assert!(
        ours.fin_entry_postings(&their_entry)
            .await
            .unwrap()
            .is_empty(),
        "nor are its postings"
    );
    assert!(
        ours.fin_entries(None, None, 100).await.unwrap().is_empty(),
        "our journal is still empty"
    );
    assert!(
        ours.fin_trial_balance(None, None)
            .await
            .unwrap()
            .accounts
            .iter()
            .all(|row| row.postings == 0),
        "and no aggregate of ours picked up their postings"
    );

    // Ours books normally afterwards, and theirs is untouched by all of it.
    let (our_invoice, _) = issued_invoice(&ours, &our_customer).await;
    assert!(
        ours.fin_invoice_entry(&our_invoice)
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(rows(&theirs, &their_entry).await, their_rows);
}

/// A foreign-currency document books at the rate frozen on it, and the
/// receivable the books carry is **exactly** the figure the document prints in
/// the accounting currency — the crossed parts added, never the gross crossed.
#[tokio::test]
async fn a_foreign_currency_invoice_books_at_its_frozen_rate() {
    let store = common::test_store().await;
    let (account, _tenant, _customer) = tenant_with_chart(&store, "usd").await;
    // A published rate for today and the two days around it, so the snapshot
    // resolves whichever day the database's clock is on.
    let today: Date = time::OffsetDateTime::now_utc().date();
    for day in [
        today + time::Duration::days(1),
        today,
        today - time::Duration::days(1),
    ] {
        account
            .save_billing_fx_rate("USD", day, 1_088_000)
            .await
            .unwrap();
    }
    let customer = account
        .create_billing_customer(&NewCustomer {
            name: "Customer USD".to_owned(),
            country: "US".to_owned(),
            currency: "USD".to_owned(),
            payment_terms_days: 30,
            ..Default::default()
        })
        .await
        .unwrap();

    let (id, document) = issued_invoice(&account, &customer).await;
    assert_eq!(document.invoice.currency, "USD");
    let entry_id = account.fin_invoice_entry(&id).await.unwrap().unwrap();
    let postings = account.fin_entry_postings(&entry_id).await.unwrap();

    let printed = restated_into("EUR", document.invoice.fx.as_ref(), &document.totals)
        .unwrap_or_else(|| panic!("an issued document carries a usable snapshot"));
    assert_eq!(
        postings[0].amount_cents, document.totals.gross_cents,
        "the document column is the dollars owed"
    );
    assert_eq!(
        postings[0].base_cents, printed.gross_cents,
        "and the base column is what the document itself prints"
    );
    assert_eq!(
        postings.iter().map(|p| p.base_cents).sum::<i64>(),
        0,
        "both columns balance with no rounding posting"
    );
    assert!(account.fin_unbalanced_entries().await.unwrap().is_empty());
}
