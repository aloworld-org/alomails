//! **Booking a recorded payment** (alo Finance, ADR 0035, wave B4.04b) — the
//! second posting rule, on the real wire: money recorded against an issued
//! invoice through the billing store and booked through
//! [`alo_store::AccountStore::post_payment_settle`] into the journal it then
//! has to be read back out of.
//!
//! `src/fin_rules.rs` holds the hand-written goldens, asserted against the pure
//! function. This suite asserts the five things a pure test cannot:
//!
//! - the entry that reaches the **database** is that golden entry, with the
//!   accounts the tenant's own chart gives the roles the method resolved to;
//! - **P5 on the wire** — after a partial payment the receivable the ledger
//!   carries is the outstanding `billing_payments::Settlement` reports, and
//!   after the last one it is **zero**, in both currency columns;
//! - the **exchange difference** a foreign-currency settlement produces is a
//!   real posting on a real account, and the entry still balances;
//! - **idempotency** — booking one payment twice is a typed `Conflict`, and a
//!   payment will not book before the invoice it settles does;
//! - **tenancy** — another tenant's payment id is a `NotFound` through this
//!   handle, and their books are untouched by the attempt.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::{
    Account, AccountRole, AccountStore, BillingCustomerId, BillingInvoiceId, BillingPaymentId,
    CHART, ChartName, ChartSeed, EntryKind, FinAccountId, FinEntryId, InvoiceStatus, NewCustomer,
    NewInvoice, NewLine, NewPayment, PaymentState, Settlement, SourceEvent, SourceKind, Store,
    StoreError, TenantId,
};
use time::{Date, Duration};

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

async fn tenant_with_chart(store: &Store, tag: &str) -> (AccountStore, TenantId) {
    let tenant = store.create_tenant(&format!("settle-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@settling.test"))
        .await
        .unwrap();
    let account = store.for_account(tenant.clone(), user);
    account
        .fin_accounts_or_seed(&seed(tag), false)
        .await
        .unwrap();
    (account, tenant)
}

async fn customer(account: &AccountStore, tag: &str, currency: &str) -> BillingCustomerId {
    account
        .create_billing_customer(&NewCustomer {
            name: format!("Customer {tag}"),
            country: if currency == "EUR" { "NL" } else { "US" }.to_owned(),
            currency: currency.to_owned(),
            payment_terms_days: 30,
            ..Default::default()
        })
        .await
        .unwrap()
}

async fn role_account(account: &AccountStore, role: AccountRole) -> Account {
    account
        .fin_account_for_role(role)
        .await
        .unwrap()
        .unwrap_or_else(|| panic!("the seeded chart holds {}", role.as_str()))
}

/// The same two-rate document the invoice rule's goldens are written for: €900
/// of consulting at 21 % and €200 of books at 9 %, gross 1 307.00.
const GROSS_CENTS: i64 = 130_700;

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

/// An issued, booked invoice for `customer`, and the day the books say it was
/// issued (the **database's** date, which is what the payment dates below are
/// measured from).
async fn booked_invoice(
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
    assert!(
        account.fin_invoice_entry(&id).await.unwrap().is_some(),
        "issuing books the document in the same transaction (B7.01)"
    );
    (id, document.invoice.issue_date.unwrap())
}

async fn pay(
    account: &AccountStore,
    invoice: &BillingInvoiceId,
    amount_cents: i64,
    method: &str,
    paid_on: Date,
) -> BillingPaymentId {
    account
        .record_billing_payment(
            invoice,
            &NewPayment {
                paid_on: Some(paid_on),
                amount_cents,
                method: method.to_owned(),
                reference: "E2E-9911".to_owned(),
            },
        )
        .await
        .unwrap()
}

/// A posting as this suite compares them: the account, the two columns, and the
/// customer dimension a receivables report groups by.
type Row = (String, i64, i64, Option<String>);

async fn rows(account: &AccountStore, entry: &FinEntryId) -> Vec<Row> {
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
                posting.customer_id,
            )
        })
        .collect()
}

/// What the trial balance says one account is worth **in the accounting
/// currency** — the figure the reports (B4.11) will fold over. An account with
/// no posting is absent from the report, which is a zero balance.
async fn balance(account: &AccountStore, id: &FinAccountId) -> i64 {
    account
        .fin_trial_balance(None, None)
        .await
        .unwrap()
        .accounts
        .iter()
        .find(|row| row.account_id.as_str() == id.as_str())
        .map(|row| row.balance_cents)
        .unwrap_or(0)
}

/// The same account's balance in the **document** currency, which the trial
/// balance deliberately does not carry (it sums one comparable column). Summed
/// from the account's own ledger lines.
async fn document_balance(account: &AccountStore, id: &FinAccountId) -> i64 {
    account
        .fin_account_ledger(id, None, None, 200)
        .await
        .unwrap()
        .lines
        .iter()
        .map(|line| line.amount_cents)
        .sum()
}

/// The golden, in the database, with the tenant's own accounts: the money where
/// it landed against the receivable it cleared.
#[tokio::test]
async fn booking_a_payment_writes_the_money_where_it_landed() {
    let store = common::test_store().await;
    let (account, _tenant) = tenant_with_chart(&store, "golden").await;
    let customer = customer(&account, "golden", "EUR").await;
    let (invoice, issued_on) = booked_invoice(&account, &customer).await;
    let payment = pay(&account, &invoice, GROSS_CENTS, "bank transfer", issued_on).await;

    let entry_id = account
        .fin_payment_entry(&payment)
        .await
        .unwrap()
        .expect("recording books the settlement in the same transaction (B7.01)");

    let booked = account
        .fin_journal_entry(&entry_id)
        .await
        .unwrap()
        .unwrap_or_else(|| panic!("the entry is readable"));
    assert_eq!(booked.entry.kind, EntryKind::Payment);
    assert_eq!(
        booked.entry.entry_date, issued_on,
        "the day the money arrived"
    );
    assert_eq!(booked.entry.currency, "EUR");
    let source = booked.entry.source.unwrap_or_else(|| panic!("a source"));
    assert_eq!(source.kind, SourceKind::Payment);
    assert_eq!(source.id, payment.as_str());
    assert_eq!(source.event, SourceEvent::Settle);

    let bank = role_account(&account, AccountRole::Bank).await;
    let ar = role_account(&account, AccountRole::Ar).await;
    assert_eq!(
        rows(&account, &entry_id).await,
        vec![
            (bank.id.as_str().to_owned(), GROSS_CENTS, GROSS_CENTS, None),
            (
                ar.id.as_str().to_owned(),
                -GROSS_CENTS,
                -GROSS_CENTS,
                Some(customer.as_str().to_owned())
            ),
        ]
    );
    // P5, one layer up: the issue put the receivable there and the settlement
    // took exactly it away.
    assert_eq!(balance(&account, &ar.id).await, 0);
    assert_eq!(balance(&account, &bank.id).await, GROSS_CENTS);
    assert!(
        account.fin_unbalanced_entries().await.unwrap().is_empty(),
        "P1, re-derived from the database rather than from the rule"
    );
    // And billing agrees the document is settled.
    let document = account.billing_invoice(&invoice).await.unwrap().unwrap();
    assert_eq!(document.invoice.status, InvoiceStatus::Paid);
    assert_eq!(document.settlement().state, PaymentState::Paid);
}

/// **P5 on the wire.** After each partial payment the receivable the books
/// carry is the outstanding `billing_payments` reports for the same document,
/// and after the last one it is zero.
#[tokio::test]
async fn partial_payments_leave_exactly_what_is_still_owed() {
    let store = common::test_store().await;
    let (account, _tenant) = tenant_with_chart(&store, "partial").await;
    let customer = customer(&account, "partial", "EUR").await;
    let (invoice, issued_on) = booked_invoice(&account, &customer).await;
    let ar = role_account(&account, AccountRole::Ar).await;
    let cash = role_account(&account, AccountRole::Cash).await;

    let _first = pay(&account, &invoice, 30_000, "cash", issued_on).await;
    assert_eq!(
        balance(&account, &ar.id).await,
        100_700,
        "the books owe what billing says is outstanding"
    );
    assert_eq!(
        Settlement::of(GROSS_CENTS, 30_000).outstanding_cents,
        100_700
    );
    assert_eq!(
        balance(&account, &cash.id).await,
        30_000,
        "and a cash payment landed in cash, not in the bank"
    );
    assert_eq!(
        balance(
            &account,
            &role_account(&account, AccountRole::Bank).await.id
        )
        .await,
        0,
        "the bank saw nothing of it"
    );
    let document = account.billing_invoice(&invoice).await.unwrap().unwrap();
    assert_eq!(document.settlement().state, PaymentState::PartiallyPaid);

    let _second = pay(&account, &invoice, 100_700, "bank transfer", issued_on).await;
    assert_eq!(
        balance(&account, &ar.id).await,
        0,
        "and the settled document leaves no receivable behind"
    );
    assert!(account.fin_unbalanced_entries().await.unwrap().is_empty());
}

/// A foreign-currency settlement: each leg crossed at its own rate, the
/// difference posted to `fx_diff`, and the receivable **exactly** zero in the
/// accounting currency once the document is settled — the cent the crossed
/// gross would have left behind carried by the last payment.
///
/// Hand-computed at the rates seeded below (`src/fin_rules.rs` writes the same
/// golden out in full):
///
/// ```text
/// invoice  $1 307.00 @ 1.0880 → receivable €1 201.28 (the crossed parts, summed)
/// payment  $500.00   @ 1.1000 → bank €454.55, ar €459.56, fx_diff €5.01 debit
/// payment  $807.00   @ 1.0500 → bank €768.57, ar €741.72, fx_diff €26.85 credit
/// ```
#[tokio::test]
async fn a_foreign_currency_settlement_posts_the_exchange_difference() {
    let store = common::test_store().await;
    let (account, _tenant) = tenant_with_chart(&store, "usd").await;
    // A rate for the issue day and the days around it, so the document freezes
    // 1.0880 whichever day the database's clock is on.
    let today = time::OffsetDateTime::now_utc().date();
    for offset in -3..=1 {
        account
            .save_billing_fx_rate("USD", today + Duration::days(offset), 1_088_000)
            .await
            .unwrap();
    }
    let customer = customer(&account, "usd", "USD").await;
    let (invoice, issued_on) = booked_invoice(&account, &customer).await;
    // The two settlement days, priced after the document was frozen: the rate
    // that reaches a payment is the one published for the day the money
    // arrived, never the one on the invoice.
    let (early, late) = (issued_on - Duration::days(2), issued_on - Duration::days(1));
    account
        .save_billing_fx_rate("USD", early, 1_100_000)
        .await
        .unwrap();
    account
        .save_billing_fx_rate("USD", late, 1_050_000)
        .await
        .unwrap();

    let bank = role_account(&account, AccountRole::Bank).await;
    let ar = role_account(&account, AccountRole::Ar).await;
    let fx = role_account(&account, AccountRole::FxDiff).await;
    let customer_id = Some(customer.as_str().to_owned());
    assert_eq!(
        (
            document_balance(&account, &ar.id).await,
            balance(&account, &ar.id).await
        ),
        (GROSS_CENTS, 120_128),
        "what the issue entry put on the receivable, in both columns"
    );

    let first = pay(&account, &invoice, 50_000, "bank transfer", early).await;
    let first_entry = account.fin_payment_entry(&first).await.unwrap().unwrap();
    assert_eq!(
        rows(&account, &first_entry).await,
        vec![
            (bank.id.as_str().to_owned(), 50_000, 45_455, None),
            (
                ar.id.as_str().to_owned(),
                -50_000,
                -45_956,
                customer_id.clone()
            ),
            (fx.id.as_str().to_owned(), 0, 501, None),
        ]
    );

    let second = pay(&account, &invoice, 80_700, "bank transfer", late).await;
    let second_entry = account.fin_payment_entry(&second).await.unwrap().unwrap();
    assert_eq!(
        rows(&account, &second_entry).await,
        vec![
            (bank.id.as_str().to_owned(), 80_700, 76_857, None),
            (ar.id.as_str().to_owned(), -80_700, -74_172, customer_id),
            (fx.id.as_str().to_owned(), 0, -2_685, None),
        ]
    );

    assert_eq!(
        (
            document_balance(&account, &ar.id).await,
            balance(&account, &ar.id).await
        ),
        (0, 0),
        "the settled document leaves no receivable in EITHER column"
    );
    assert_eq!(
        (
            document_balance(&account, &bank.id).await,
            balance(&account, &bank.id).await
        ),
        (130_700, 122_312),
        "the dollars received, and the euro they were worth on the days they arrived"
    );
    assert_eq!(
        (
            document_balance(&account, &fx.id).await,
            balance(&account, &fx.id).await
        ),
        (0, -2_184),
        "a net exchange gain of €21.84, and not one dollar of movement to explain it"
    );
    assert!(
        account
            .fin_trial_balance(None, None)
            .await
            .unwrap()
            .balances(),
        "and the tenant's books still balance whole"
    );
    assert!(account.fin_unbalanced_entries().await.unwrap().is_empty());
}

/// A retry, a double-click and a re-run of a backfill all hit the idempotency
/// key: recording booked the payment once, and the explicit door answers a
/// typed conflict afterwards.
#[tokio::test]
async fn a_payment_books_once_and_never_before_its_invoice() {
    let store = common::test_store().await;
    let (account, _tenant) = tenant_with_chart(&store, "once").await;
    let customer = customer(&account, "once", "EUR").await;

    let (invoice, issued_on) = booked_invoice(&account, &customer).await;
    let payment = pay(&account, &invoice, 40_000, "bank transfer", issued_on).await;
    let entry_id = account.fin_payment_entry(&payment).await.unwrap().unwrap();
    let written = rows(&account, &entry_id).await;

    assert!(
        assert_conflict(account.post_payment_settle(&invoice, &payment).await)
            .contains("already posted")
    );
    assert_eq!(
        account.fin_payment_entry(&payment).await.unwrap().as_ref(),
        Some(&entry_id),
        "still the one entry"
    );
    assert_eq!(
        rows(&account, &entry_id).await,
        written,
        "and not one extra posting"
    );

    // An id that is not this document's payment is a NotFound, not a posting
    // somewhere unexpected.
    assert_not_found(
        account
            .post_payment_settle(&invoice, &BillingPaymentId::new("pay-nowhere"))
            .await,
    );
}

/// **The wrong-tenant test.** Tenant B books tenant A's payment: a `NotFound`,
/// no entry anywhere, and A's postings byte-identical afterwards.
#[tokio::test]
async fn another_tenants_payment_cannot_be_booked() {
    let store = common::test_store().await;
    let (ours, _our_tenant) = tenant_with_chart(&store, "ours").await;
    let (theirs, _their_tenant) = tenant_with_chart(&store, "theirs").await;
    let their_customer = customer(&theirs, "theirs", "EUR").await;
    let (their_invoice, their_day) = booked_invoice(&theirs, &their_customer).await;
    let their_payment = pay(
        &theirs,
        &their_invoice,
        GROSS_CENTS,
        "bank transfer",
        their_day,
    )
    .await;
    let their_entry = theirs
        .fin_payment_entry(&their_payment)
        .await
        .unwrap()
        .unwrap();
    let their_rows = rows(&theirs, &their_entry).await;

    assert_not_found(
        ours.post_payment_settle(&their_invoice, &their_payment)
            .await,
    );
    assert_eq!(
        ours.fin_payment_entry(&their_payment).await.unwrap(),
        None,
        "and we cannot even see that it is booked"
    );
    assert!(
        ours.fin_entries(None, None, 50).await.unwrap().is_empty(),
        "our journal is empty"
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
    let our_customer = customer(&ours, "ours", "EUR").await;
    let (our_invoice, our_day) = booked_invoice(&ours, &our_customer).await;
    let our_payment = pay(&ours, &our_invoice, 10_000, "bank transfer", our_day).await;
    assert!(
        ours.fin_payment_entry(&our_payment)
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(rows(&theirs, &their_entry).await, their_rows);
}

/// **The backfill.** A tenant who invoiced before the B7.01 wiring holds
/// issued documents with no entry. The moment money is recorded against one,
/// the issue entry is written first — at the document's own issue date — and
/// the settlement second, in the same transaction: the exact behaviour a
/// confirmed bank match has always had. (The pre-wiring state is manufactured
/// with test-only surgery on the journal, because the product itself can no
/// longer produce it.)
#[tokio::test]
async fn a_pre_wiring_invoice_is_backfilled_when_money_arrives() {
    let store = common::test_store().await;
    let (account, tenant) = tenant_with_chart(&store, "backfill").await;
    let customer = customer(&account, "backfill", "EUR").await;
    let (invoice, issued_on) = booked_invoice(&account, &customer).await;

    // Surgery: make the tenant look pre-wiring by removing the issue entry.
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&common::database_url())
        .await
        .unwrap();
    for table in ["fin_postings", "fin_entries"] {
        sqlx::query(&format!("DELETE FROM {table} WHERE tenant_id = $1"))
            .bind(tenant.as_str())
            .execute(&pool)
            .await
            .unwrap();
    }
    assert_eq!(account.fin_invoice_entry(&invoice).await.unwrap(), None);

    let payment = pay(&account, &invoice, 30_000, "bank transfer", issued_on).await;
    let issue_entry = account
        .fin_invoice_entry(&invoice)
        .await
        .unwrap()
        .expect("the recording backfilled the invoice's own issue entry");
    let settle_entry = account
        .fin_payment_entry(&payment)
        .await
        .unwrap()
        .expect("and booked the settlement beside it");
    assert_ne!(issue_entry, settle_entry);
    assert_eq!(
        account
            .fin_journal_entry(&issue_entry)
            .await
            .unwrap()
            .unwrap()
            .entry
            .entry_date,
        issued_on,
        "the backfilled issue books at the document's own date"
    );
    let ar = role_account(&account, AccountRole::Ar).await;
    assert_eq!(
        balance(&account, &ar.id).await,
        GROSS_CENTS - 30_000,
        "the books owe exactly what billing says is outstanding"
    );
    assert!(account.fin_unbalanced_entries().await.unwrap().is_empty());
}
