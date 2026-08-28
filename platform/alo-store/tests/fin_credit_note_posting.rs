//! **Booking an issued credit note** (alo Finance, ADR 0035, wave B4.04c) — the
//! third posting rule, on the real wire: a credit note raised against a booked
//! invoice through the billing store and booked through
//! [`alo_store::AccountStore::post_credit_note_issue`] into the journal it then
//! has to be read back out of.
//!
//! `src/fin_rules.rs` holds the hand-written golden, asserted against the pure
//! function. This suite asserts the five things a pure test cannot:
//!
//! - the entry that reaches the **database** is that golden entry, on the
//!   accounts the tenant's own chart gives the roles, naming the original's
//!   entry as the one it corrects;
//! - **P4 on the wire** — a document and its full credit note sum to zero *per
//!   account and per dimension*, in both money columns, including at a rate
//!   where the crossed whole and the crossed parts disagree by a cent;
//! - a **partial** credit note leaves exactly the uncredited part standing, and
//!   the receivable the books carry is what the customer still owes;
//! - **idempotency and ordering** — a credit note books once, and never before
//!   the invoice it corrects;
//! - **tenancy** — another tenant's credit note is a `NotFound` through this
//!   handle, and their books are untouched by the attempt.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::{
    Account, AccountRole, AccountStore, BillingCustomerId, BillingInvoiceId, CHART, ChartName,
    ChartSeed, EntryKind, FinAccountId, FinEntryId, LedgerDimension, LedgerScope, NewCustomer,
    NewInvoice, NewLine, SourceEvent, SourceKind, Store, StoreError, TenantId,
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
    let tenant = store.create_tenant(&format!("credit-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@crediting.test"))
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
/// issued.
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

/// A credit note against `original`, issued: the full mirror unless `lines` says
/// otherwise, which is how a **partial** credit is raised (edit the draft's
/// lines, then issue).
async fn issued_credit_note(
    account: &AccountStore,
    original: &BillingInvoiceId,
    lines: Option<Vec<NewLine>>,
) -> BillingInvoiceId {
    let id = account.create_billing_credit_note(original).await.unwrap();
    if let Some(lines) = lines {
        account
            .set_billing_invoice_lines(&id, &lines)
            .await
            .unwrap();
    }
    account.issue_billing_invoice(&id).await.unwrap();
    id
}

/// A posting as this suite compares them: the account, the two columns, and the
/// dimensions a receivables report and a VAT return group by.
type Row = (String, i64, i64, Option<String>, Option<i32>);

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
                posting.vat_rate_bp,
            )
        })
        .collect()
}

/// What the trial balance says one account is worth **in the accounting
/// currency**. An account with no posting is absent from the report, which is a
/// zero balance.
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

/// The same account's balance in the **document** currency, summed from its own
/// ledger lines (the trial balance carries one comparable column).
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

/// Every group of one dimension on one account, as `(value, balance)` — how P4
/// is asserted "per dimension" rather than only per account.
///
/// Sorted by the dimension's value rather than left in the report's own order
/// (largest debit first), which `fin_ledger`'s own suite owns: what this suite
/// is asserting is which groups exist and what each is worth.
async fn groups(
    account: &AccountStore,
    id: &FinAccountId,
    dimension: LedgerDimension,
) -> Vec<(Option<String>, i64)> {
    let balances = account
        .fin_dimension_balances(&LedgerScope::Account(id.clone()), dimension, None, None)
        .await
        .unwrap();
    assert!(!balances.truncated, "the whole grouping, not a page of it");
    let mut rows: Vec<(Option<String>, i64)> = balances
        .rows
        .into_iter()
        .map(|row| (row.value, row.balance_cents))
        .collect();
    rows.sort_by(|left, right| left.0.cmp(&right.0));
    rows
}

/// The golden, in the database: the mirror of the invoice entry, on the tenant's
/// own accounts, naming the entry it corrects.
#[tokio::test]
async fn booking_a_credit_note_writes_the_mirror_of_the_original() {
    let store = common::test_store().await;
    let (account, _tenant) = tenant_with_chart(&store, "golden").await;
    let customer = customer(&account, "golden", "EUR").await;
    let (invoice, issued_on) = booked_invoice(&account, &customer).await;
    let original_entry = account.fin_invoice_entry(&invoice).await.unwrap().unwrap();

    let credit = issued_credit_note(&account, &invoice, None).await;
    let entry_id = account
        .fin_invoice_entry(&credit)
        .await
        .unwrap()
        .expect("issuing a credit note books the mirror in the same transaction (B7.01)");

    let booked = account
        .fin_journal_entry(&entry_id)
        .await
        .unwrap()
        .unwrap_or_else(|| panic!("the entry is readable"));
    assert_eq!(booked.entry.kind, EntryKind::CreditNote);
    assert_eq!(booked.entry.entry_date, issued_on, "its own issue date");
    assert_eq!(booked.entry.currency, "EUR");
    assert_eq!(
        booked.entry.reverses_entry_id.as_ref(),
        Some(&original_entry),
        "a journal reader walks from the correction to what it corrected"
    );
    let source = booked.entry.source.unwrap_or_else(|| panic!("a source"));
    assert_eq!(source.kind, SourceKind::Invoice);
    assert_eq!(
        source.id,
        credit.as_str(),
        "keyed on the credit note itself"
    );
    assert_eq!(source.event, SourceEvent::Issue);

    let ar = role_account(&account, AccountRole::Ar).await;
    let revenue = role_account(&account, AccountRole::Revenue).await;
    let vat = role_account(&account, AccountRole::VatOutput).await;
    let owed_by = Some(customer.as_str().to_owned());
    assert_eq!(
        rows(&account, &entry_id).await,
        vec![
            (ar.id.as_str().to_owned(), -130_700, -130_700, owed_by, None),
            (
                revenue.id.as_str().to_owned(),
                20_000,
                20_000,
                None,
                Some(900)
            ),
            (vat.id.as_str().to_owned(), 1_800, 1_800, None, Some(900)),
            (
                revenue.id.as_str().to_owned(),
                90_000,
                90_000,
                None,
                Some(2100)
            ),
            (vat.id.as_str().to_owned(), 18_900, 18_900, None, Some(2100)),
        ]
    );

    // **P4, per account.** The pair moved nothing.
    for id in [&ar.id, &revenue.id, &vat.id] {
        assert_eq!(balance(&account, id).await, 0);
        assert_eq!(document_balance(&account, id).await, 0);
    }
    // **P4, per dimension.** The customer owes nothing, and each VAT rate nets
    // to nothing — which is what makes the return and the books one statement.
    assert_eq!(
        groups(&account, &ar.id, LedgerDimension::Customer).await,
        vec![(Some(customer.as_str().to_owned()), 0)]
    );
    for id in [&revenue.id, &vat.id] {
        assert_eq!(
            groups(&account, id, LedgerDimension::VatRate).await,
            vec![(Some("2100".to_owned()), 0), (Some("900".to_owned()), 0)]
        );
    }
    assert!(
        account.fin_unbalanced_entries().await.unwrap().is_empty(),
        "P1, re-derived from the database rather than from the rule"
    );
}

/// A **partial** credit note books only what it gives back, and the receivable
/// the books still carry is what the customer still owes — the 21 % part with
/// its tax, €1 089.00.
#[tokio::test]
async fn a_partial_credit_note_leaves_the_uncredited_part_standing() {
    let store = common::test_store().await;
    let (account, _tenant) = tenant_with_chart(&store, "partial").await;
    let customer = customer(&account, "partial", "EUR").await;
    let (invoice, _issued_on) = booked_invoice(&account, &customer).await;

    // Only the 9 % line given back, quantity negated as a credit note's lines
    // always are.
    let credit = issued_credit_note(
        &account,
        &invoice,
        Some(vec![NewLine {
            description: "Line 1".to_owned(),
            unit: "hour".to_owned(),
            qty_milli: -4_000,
            unit_price_cents: 5_000,
            vat_rate_bp: 900,
        }]),
    )
    .await;
    let entry_id = account.fin_invoice_entry(&credit).await.unwrap().unwrap();

    let ar = role_account(&account, AccountRole::Ar).await;
    let revenue = role_account(&account, AccountRole::Revenue).await;
    let vat = role_account(&account, AccountRole::VatOutput).await;
    let owed_by = Some(customer.as_str().to_owned());
    assert_eq!(
        rows(&account, &entry_id).await,
        vec![
            (
                ar.id.as_str().to_owned(),
                -21_800,
                -21_800,
                owed_by.clone(),
                None
            ),
            (
                revenue.id.as_str().to_owned(),
                20_000,
                20_000,
                None,
                Some(900)
            ),
            (vat.id.as_str().to_owned(), 1_800, 1_800, None, Some(900)),
        ]
    );

    assert_eq!(
        balance(&account, &ar.id).await,
        108_900,
        "the 21 % part with its tax is still owed"
    );
    assert_eq!(
        groups(&account, &ar.id, LedgerDimension::Customer).await,
        vec![(owed_by, 108_900)],
        "and it is owed by that customer"
    );
    // The credited rate is flat; the uncredited one is untouched.
    assert_eq!(
        groups(&account, &revenue.id, LedgerDimension::VatRate).await,
        vec![
            (Some("2100".to_owned()), -90_000),
            (Some("900".to_owned()), 0)
        ]
    );
    assert_eq!(balance(&account, &vat.id).await, -18_900);
    assert!(
        account
            .fin_trial_balance(None, None)
            .await
            .unwrap()
            .balances(),
        "and the tenant's books still balance whole"
    );
}

/// **P4 in a foreign currency.** The credit note inherits the original's frozen
/// rate, so the pair sums to zero in the accounting currency too — at a rate
/// where the crossed gross (€1 201.29) and the crossed parts (€1 201.28)
/// deliberately disagree.
#[tokio::test]
async fn a_foreign_currency_credit_note_mirrors_in_both_columns() {
    let store = common::test_store().await;
    let (account, _tenant) = tenant_with_chart(&store, "usd").await;
    // A rate for the issue day and the days around it, so both documents freeze
    // 1.0880 whichever day the database's clock is on.
    let today = time::OffsetDateTime::now_utc().date();
    for offset in -3..=1 {
        account
            .save_billing_fx_rate("USD", today + Duration::days(offset), 1_088_000)
            .await
            .unwrap();
    }
    let customer = customer(&account, "usd", "USD").await;
    let (invoice, _issued_on) = booked_invoice(&account, &customer).await;
    let ar = role_account(&account, AccountRole::Ar).await;
    assert_eq!(
        (
            document_balance(&account, &ar.id).await,
            balance(&account, &ar.id).await
        ),
        (GROSS_CENTS, 120_128),
        "what the issue entry put on the receivable, in both columns"
    );

    let credit = issued_credit_note(&account, &invoice, None).await;
    let document = account.billing_invoice(&credit).await.unwrap().unwrap();
    assert_eq!(
        document.invoice.fx.map(|fx| fx.rate_micro),
        Some(1_088_000),
        "a credit note inherits its original's rate rather than taking today's"
    );
    let entry_id = account.fin_invoice_entry(&credit).await.unwrap().unwrap();
    assert_eq!(
        rows(&account, &entry_id).await[0].2,
        -120_128,
        "the receivable is taken back at exactly the figure it was put there at"
    );

    for role in [
        AccountRole::Ar,
        AccountRole::Revenue,
        AccountRole::VatOutput,
    ] {
        let id = role_account(&account, role).await.id;
        assert_eq!(
            (
                document_balance(&account, &id).await,
                balance(&account, &id).await
            ),
            (0, 0),
            "{} nets to nothing in BOTH columns",
            role.as_str()
        );
    }
    assert!(
        account
            .fin_trial_balance(None, None)
            .await
            .unwrap()
            .balances(),
        "the books balance whole, with no residual cent to explain"
    );
    assert!(account.fin_unbalanced_entries().await.unwrap().is_empty());
}

/// A credit note books once, never through the rule that does not own it —
/// and a pre-wiring original with no entry is **backfilled** the moment its
/// credit note issues, so the mirror never leaves a customer owing a negative.
#[tokio::test]
async fn a_credit_note_books_once_and_backfills_a_pre_wiring_original() {
    let store = common::test_store().await;
    let (account, tenant) = tenant_with_chart(&store, "once").await;
    let customer = customer(&account, "once", "EUR").await;

    // A pre-wiring original: issued and booked, then its journal removed with
    // test-only surgery (the product itself can no longer produce this state).
    let (older, older_day) = booked_invoice(&account, &customer).await;
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
    assert_eq!(account.fin_invoice_entry(&older).await.unwrap(), None);

    let early = issued_credit_note(&account, &older, None).await;
    let older_entry = account
        .fin_invoice_entry(&older)
        .await
        .unwrap()
        .expect("issuing the credit note backfilled its original first");
    assert_eq!(
        account
            .fin_journal_entry(&older_entry)
            .await
            .unwrap()
            .unwrap()
            .entry
            .entry_date,
        older_day,
        "the backfilled issue books at the original's own date"
    );
    assert!(account.fin_invoice_entry(&early).await.unwrap().is_some());
    assert_eq!(
        balance(&account, &role_account(&account, AccountRole::Ar).await.id).await,
        0,
        "and the pair still sums to nothing"
    );

    let (invoice, _issued_on) = booked_invoice(&account, &customer).await;
    let credit = issued_credit_note(&account, &invoice, None).await;
    let entry_id = account.fin_invoice_entry(&credit).await.unwrap().unwrap();
    let written = rows(&account, &entry_id).await;

    assert!(
        assert_conflict(account.post_credit_note_issue(&credit).await).contains("already posted")
    );
    assert_eq!(
        account.fin_invoice_entry(&credit).await.unwrap().as_ref(),
        Some(&entry_id),
        "still the one entry"
    );
    assert_eq!(
        rows(&account, &entry_id).await,
        written,
        "and not one extra posting"
    );

    // Each rule refuses the other's document, naming the one that owns it.
    assert!(
        assert_conflict(account.post_invoice_issue(&credit).await).contains("credit-note rule")
    );
    assert!(
        assert_conflict(account.post_credit_note_issue(&invoice).await).contains("invoice rule")
    );
    assert_not_found(
        account
            .post_credit_note_issue(&BillingInvoiceId::new("inv-nowhere"))
            .await,
    );
}

/// **The wrong-tenant test.** Tenant B books tenant A's credit note: a
/// `NotFound`, no entry anywhere, and A's postings byte-identical afterwards.
#[tokio::test]
async fn another_tenants_credit_note_cannot_be_booked() {
    let store = common::test_store().await;
    let (ours, _our_tenant) = tenant_with_chart(&store, "ours").await;
    let (theirs, _their_tenant) = tenant_with_chart(&store, "theirs").await;
    let their_customer = customer(&theirs, "theirs", "EUR").await;
    let (their_invoice, _their_day) = booked_invoice(&theirs, &their_customer).await;
    let their_credit = issued_credit_note(&theirs, &their_invoice, None).await;
    let their_entry = theirs
        .fin_invoice_entry(&their_credit)
        .await
        .unwrap()
        .unwrap();
    let their_rows = rows(&theirs, &their_entry).await;

    assert_not_found(ours.post_credit_note_issue(&their_credit).await);
    assert_eq!(
        ours.fin_invoice_entry(&their_credit).await.unwrap(),
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
    let (our_invoice, _our_day) = booked_invoice(&ours, &our_customer).await;
    let our_credit = issued_credit_note(&ours, &our_invoice, None).await;
    assert!(ours.fin_invoice_entry(&our_credit).await.unwrap().is_some());
    assert_eq!(rows(&theirs, &their_entry).await, their_rows);
}
