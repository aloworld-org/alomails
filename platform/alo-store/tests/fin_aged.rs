//! **Aged receivables and payables on seeded books** (alo Finance, ADR 0035,
//! wave B4.11c) — the third of the four reports, asserted against figures
//! computed by hand from documents that reached Postgres through the same doors
//! a bookkeeper uses.
//!
//! `src/fin_aged.rs` proves the fold: given these open documents, these are the
//! bands. This suite proves the six things a pure test cannot.
//!
//! - **The bands add up to the hand-computed figures**, party by party and in
//!   total, over invoices raised, issued, part-paid and credited through the
//!   store's own API.
//! - **The day is a boundary in both directions**: a document issued after the
//!   date asked for is on no earlier report, and money that arrived after it has
//!   settled nothing on it.
//! - **The payable side reads approved bills and nothing else** — a bill nobody
//!   has decided about is an intention, and a rejected one is a refusal.
//! - **A bill that states no due date is payable on receipt**, which is what
//!   ages it from its issue date.
//! - **A foreign document is added at the rate frozen on it**, and one that
//!   cannot be restated is in no band and is counted — the fx columns read back
//!   the way they were written.
//! - **Tenancy** (Law 1): a second tenant's much larger debts move nothing on
//!   the first's report, in either direction.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::{
    AccountStore, AgedBucket, AgedReport, AgedSide, BillStatus, BillTotals, BillingCustomerId,
    BillingInvoiceId, EInvoiceSyntax, NewBill, NewCustomer, NewInvoice, NewLine, NewPayment, Store,
    TenantId,
};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use time::Date;

/// A raw pool alongside the store, for the two things only time can do
/// legitimately: moving a document's dates into the past, and freezing a rate on
/// one that was raised before the tenant had a foreign customer.
async fn raw_pool() -> PgPool {
    PgPoolOptions::new()
        .max_connections(2)
        .connect(&common::database_url())
        .await
        .unwrap()
}

/// The database's own today — the clock every date below is expressed against,
/// so the suite reads the same on any day of any year.
async fn today(pool: &PgPool) -> Date {
    sqlx::query_scalar("SELECT CURRENT_DATE")
        .fetch_one(pool)
        .await
        .unwrap()
}

fn days(count: i64) -> time::Duration {
    time::Duration::days(count)
}

/// A tenant with one user, tagged so a leak between two of them reads as the
/// wrong tag rather than as a plausible number.
async fn tenant(store: &Store, tag: &str) -> (AccountStore, TenantId) {
    let tenant = store.create_tenant(&format!("aged-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@aged.test"))
        .await
        .unwrap();
    (store.for_account(tenant.clone(), user), tenant)
}

async fn customer(account: &AccountStore, name: &str) -> BillingCustomerId {
    account
        .create_billing_customer(&NewCustomer {
            name: name.to_owned(),
            country: "NL".to_owned(),
            currency: "EUR".to_owned(),
            payment_terms_days: 14,
            ..Default::default()
        })
        .await
        .unwrap()
}

/// 10 hours of consulting at €100.00, 21 % VAT: net 100 000, VAT 21 000, gross
/// 121 000 cents. Hand-computed, so every figure below is checked against
/// arithmetic done outside the code under test.
fn consulting() -> NewLine {
    NewLine {
        description: "Consulting".to_owned(),
        unit: "hour".to_owned(),
        qty_milli: 10_000,
        unit_price_cents: 10_000,
        vat_rate_bp: 2100,
    }
}

/// A one-line invoice of `hours` hours, issued and then stamped with the dates
/// the test needs — the one thing a test cannot do by waiting.
async fn issued(
    account: &AccountStore,
    pool: &PgPool,
    tenant: &TenantId,
    customer: &BillingCustomerId,
    hours: i64,
    issue_date: Date,
    due_date: Date,
) -> BillingInvoiceId {
    let id = account
        .create_billing_invoice(&NewInvoice::for_customer(customer.clone()))
        .await
        .unwrap();
    account
        .set_billing_invoice_lines(
            &id,
            &[NewLine {
                qty_milli: hours * 1_000,
                ..consulting()
            }],
        )
        .await
        .unwrap();
    account.issue_billing_invoice(&id).await.unwrap();
    stamp_dates(pool, tenant, &id, issue_date, due_date).await;
    id
}

async fn stamp_dates(
    pool: &PgPool,
    tenant: &TenantId,
    id: &BillingInvoiceId,
    issue_date: Date,
    due_date: Date,
) {
    let done = sqlx::query(
        "UPDATE billing_invoices SET issue_date = $3, due_date = $4 \
         WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_str())
    .bind(id.as_str())
    .bind(issue_date)
    .bind(due_date)
    .execute(pool)
    .await
    .unwrap();
    assert_eq!(done.rows_affected(), 1);
}

/// Money arriving on a stated day.
fn transfer(amount_cents: i64, paid_on: Date) -> NewPayment {
    NewPayment {
        paid_on: Some(paid_on),
        amount_cents,
        method: "bank transfer".to_owned(),
        reference: "NL02RABO0123456789".to_owned(),
    }
}

/// A hand-entered bill from `supplier`, payable `payable_cents`.
fn bill(supplier: &str, number: &str, issue_date: Date, due_date: Option<Date>) -> NewBill {
    NewBill {
        // Stated, though nothing was imported: the column is `NOT NULL` even
        // though `NewBill` models a hand-entered bill as carrying no syntax
        // (flagged in `docs/autonomy/STATE.md` — a schema fix needs its own
        // migration, and this suite is not the place for one).
        source_syntax: Some(EInvoiceSyntax::Cii),
        source_sha256: "ab".repeat(32),
        // No VAT id: a small supplier may state none, and then the comparable
        // key is their name — the fallback this exercises on the way past.
        supplier: alo_store::Supplier {
            name: supplier.to_owned(),
            ..Default::default()
        },
        number: number.to_owned(),
        issue_date: Some(issue_date),
        due_date,
        currency: "EUR".to_owned(),
        totals: BillTotals {
            line_total_cents: 100_000,
            tax_exclusive_cents: 100_000,
            tax_total_cents: 21_000,
            tax_inclusive_cents: 121_000,
            payable_cents: 121_000,
            ..Default::default()
        },
        lines: vec![consulting()],
        ..Default::default()
    }
}

async fn aged(account: &AccountStore, on: Date, side: AgedSide) -> AgedReport {
    account.fin_aged(on, side).await.unwrap()
}

fn party<'a>(report: &'a AgedReport, name: &str) -> &'a alo_store::AgedParty {
    report
        .parties
        .iter()
        .find(|party| party.name == name)
        .unwrap_or_else(|| panic!("{name} should be on the report"))
}

#[tokio::test]
async fn a_seeded_ledger_of_debts_reports_the_bands_computed_by_hand() {
    let store = common::test_store().await;
    let pool = raw_pool().await;
    let (account, tenant_id) = tenant(&store, "ladder").await;
    let on = today(&pool).await;
    let anchor = customer(&account, "Anchor BV").await;
    let zephyr = customer(&account, "Zephyr NV").await;

    // Anchor: one not yet due, one a fortnight late, one nearly a year late.
    issued(
        &account,
        &pool,
        &tenant_id,
        &anchor,
        1,
        on - days(3),
        on + days(11),
    )
    .await;
    let late = issued(
        &account,
        &pool,
        &tenant_id,
        &anchor,
        2,
        on - days(30),
        on - days(16),
    )
    .await;
    issued(
        &account,
        &pool,
        &tenant_id,
        &anchor,
        3,
        on - days(300),
        on - days(286),
    )
    .await;
    // Zephyr: one part-paid, sixty-five days late.
    let part_paid = issued(
        &account,
        &pool,
        &tenant_id,
        &zephyr,
        10,
        on - days(79),
        on - days(65),
    )
    .await;
    account
        .record_billing_payment(&part_paid, &transfer(100_000, on - days(20)))
        .await
        .unwrap();
    // …and one settled in full, which is nobody's debt and therefore no row.
    let settled = issued(
        &account,
        &pool,
        &tenant_id,
        &zephyr,
        1,
        on - days(40),
        on - days(26),
    )
    .await;
    account
        .record_billing_payment(&settled, &transfer(12_100, on - days(25)))
        .await
        .unwrap();

    let report = aged(&account, on, AgedSide::Receivable).await;
    assert_eq!(report.on, on);
    assert_eq!(report.side, AgedSide::Receivable);
    assert_eq!(report.currency, "EUR");
    assert_eq!(report.unconverted_count, 0);
    assert_eq!(report.document_count, 4, "the settled one is not a row");

    // Anchor: 121.00 current, 242.00 sixteen days late, 363.00 far past ninety.
    let anchor_aged = party(&report, "Anchor BV");
    assert_eq!(anchor_aged.buckets.current_cents, 12_100);
    assert_eq!(anchor_aged.buckets.days_1_30_cents, 24_200);
    assert_eq!(anchor_aged.buckets.days_31_60_cents, 0);
    assert_eq!(anchor_aged.buckets.days_61_90_cents, 0);
    assert_eq!(anchor_aged.buckets.days_90_plus_cents, 36_300);
    assert_eq!(anchor_aged.buckets.total_cents, 72_600);
    assert_eq!(anchor_aged.documents.len(), 3);
    assert_eq!(anchor_aged.unconverted_count, 0);

    // Zephyr: 1 210.00 raised less 1 000.00 received, 65 days late.
    let zephyr_aged = party(&report, "Zephyr NV");
    assert_eq!(zephyr_aged.documents.len(), 1);
    assert_eq!(zephyr_aged.buckets.days_61_90_cents, 21_000);
    assert_eq!(zephyr_aged.buckets.total_cents, 21_000);
    let document = &zephyr_aged.documents[0];
    assert_eq!(document.open_cents, 21_000, "121 000 raised, 100 000 in");
    assert_eq!(document.base_open_cents, Some(21_000));
    assert_eq!(document.days_overdue, 65);
    assert_eq!(document.bucket, AgedBucket::Days61To90);
    assert_eq!(document.currency, "EUR");
    assert!(!document.is_credit_note);
    assert!(
        document.number.starts_with("INV-"),
        "a document on this list is a numbered one: {}",
        document.number
    );
    assert_eq!(document.due_date, on - days(65));

    // The whole report is its parties added up.
    assert_eq!(report.buckets.current_cents, 12_100);
    assert_eq!(report.buckets.days_1_30_cents, 24_200);
    assert_eq!(report.buckets.days_61_90_cents, 21_000);
    assert_eq!(report.buckets.days_90_plus_cents, 36_300);
    assert_eq!(report.buckets.total_cents, 93_600);
    let names: Vec<&str> = report.parties.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["Anchor BV", "Zephyr NV"]);

    // A credit note against the late invoice reduces what Anchor owes, in
    // Anchor's own group and in the band of the credit note's own due date.
    let credit = account.create_billing_credit_note(&late).await.unwrap();
    account.issue_billing_invoice(&credit).await.unwrap();
    stamp_dates(&pool, &tenant_id, &credit, on - days(2), on - days(2)).await;
    let after = aged(&account, on, AgedSide::Receivable).await;
    let anchor_aged = party(&after, "Anchor BV");
    assert_eq!(anchor_aged.buckets.days_1_30_cents, 0, "242.00 less 242.00");
    assert_eq!(anchor_aged.buckets.total_cents, 48_400);
    let credited = anchor_aged
        .documents
        .iter()
        .find(|d| d.is_credit_note)
        .unwrap_or_else(|| panic!("the credit note is a row of its own"));
    assert_eq!(credited.open_cents, -24_200);
    assert_eq!(credited.bucket, AgedBucket::Days1To30);
    assert_eq!(after.buckets.total_cents, 69_400);
}

#[tokio::test]
async fn the_day_is_a_boundary_for_the_document_and_for_the_money() {
    let store = common::test_store().await;
    let pool = raw_pool().await;
    let (account, tenant_id) = tenant(&store, "boundary").await;
    let on = today(&pool).await;
    let anchor = customer(&account, "Anchor BV").await;

    let old = issued(
        &account,
        &pool,
        &tenant_id,
        &anchor,
        1,
        on - days(60),
        on - days(46),
    )
    .await;
    issued(
        &account,
        &pool,
        &tenant_id,
        &anchor,
        2,
        on - days(10),
        on + days(4),
    )
    .await;
    account
        .record_billing_payment(&old, &transfer(5_000, on - days(5)))
        .await
        .unwrap();

    // Today: both documents, the older one net of the money that has arrived.
    let now = aged(&account, on, AgedSide::Receivable).await;
    assert_eq!(now.document_count, 2);
    assert_eq!(now.buckets.total_cents, 12_100 - 5_000 + 24_200);

    // Thirty days ago: the second document did not exist and the payment had
    // not arrived, so the first stands at its full gross, sixteen days late.
    let then = aged(&account, on - days(30), AgedSide::Receivable).await;
    assert_eq!(then.document_count, 1);
    assert_eq!(then.buckets.days_1_30_cents, 12_100);
    assert_eq!(then.buckets.total_cents, 12_100);
    assert_eq!(then.parties[0].documents[0].days_overdue, 16);

    // The day before anything was raised is a report of zeroes, not an absence.
    let before = aged(&account, on - days(90), AgedSide::Receivable).await;
    assert!(before.parties.is_empty());
    assert_eq!(before.buckets.total_cents, 0);
    assert_eq!(before.currency, "EUR");
    assert_eq!(before.document_count, 0);

    // A draft is on no report at all, however old it is.
    let draft = account
        .create_billing_invoice(&NewInvoice::for_customer(anchor.clone()))
        .await
        .unwrap();
    account
        .set_billing_invoice_lines(&draft, &[consulting()])
        .await
        .unwrap();
    assert_eq!(
        aged(&account, on, AgedSide::Receivable)
            .await
            .document_count,
        2,
        "a draft was never raised and owes nobody anything"
    );
}

#[tokio::test]
async fn the_payable_side_reads_approved_bills_and_nothing_else() {
    let store = common::test_store().await;
    let pool = raw_pool().await;
    let (account, _tenant_id) = tenant(&store, "payables").await;
    let on = today(&pool).await;

    // Approved, forty days past due.
    let approved = account
        .create_billing_bill(&bill(
            "Lieferant GmbH",
            "R-1",
            on - days(70),
            Some(on - days(40)),
        ))
        .await
        .unwrap();
    account
        .decide_billing_bill(&approved, BillStatus::Approved)
        .await
        .unwrap();
    // Approved, and stating no due date at all: payable on receipt.
    let on_receipt = account
        .create_billing_bill(&bill("Hosting SAS", "H-9", on - days(5), None))
        .await
        .unwrap();
    account
        .decide_billing_bill(&on_receipt, BillStatus::Approved)
        .await
        .unwrap();
    // Nobody has decided about this one, and this one was refused.
    account
        .create_billing_bill(&bill("Undecided BV", "U-3", on - days(9), Some(on)))
        .await
        .unwrap();
    let rejected = account
        .create_billing_bill(&bill("Rejected BV", "X-77", on - days(9), Some(on)))
        .await
        .unwrap();
    account
        .decide_billing_bill(&rejected, BillStatus::Rejected)
        .await
        .unwrap();

    let report = aged(&account, on, AgedSide::Payable).await;
    assert_eq!(report.side, AgedSide::Payable);
    assert_eq!(report.document_count, 2, "only what we have accepted");
    let names: Vec<&str> = report.parties.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["Hosting SAS", "Lieferant GmbH"]);

    let overdue = party(&report, "Lieferant GmbH");
    assert_eq!(overdue.buckets.days_31_60_cents, 121_000);
    assert_eq!(overdue.buckets.total_cents, 121_000);
    assert_eq!(overdue.documents[0].number, "R-1");
    assert_eq!(overdue.documents[0].days_overdue, 40);

    let receipt = party(&report, "Hosting SAS");
    assert_eq!(
        receipt.documents[0].due_date,
        on - days(5),
        "a bill stating no due date was payable when it arrived"
    );
    assert_eq!(receipt.documents[0].days_overdue, 5);
    assert_eq!(receipt.buckets.days_1_30_cents, 121_000);
    assert_eq!(report.buckets.total_cents, 242_000);
    assert_eq!(report.unconverted_count, 0, "a euro bill in euro books");

    // The two sides are different reports over different tables: nothing we owe
    // is owed to us.
    let receivable = aged(&account, on, AgedSide::Receivable).await;
    assert!(receivable.parties.is_empty());
}

#[tokio::test]
async fn a_foreign_document_is_added_at_its_own_rate_and_an_unconvertible_one_is_counted() {
    let store = common::test_store().await;
    let pool = raw_pool().await;
    let (account, tenant_id) = tenant(&store, "fx").await;
    let on = today(&pool).await;
    let abroad = customer(&account, "Overseas Inc").await;

    let crossed = issued(
        &account,
        &pool,
        &tenant_id,
        &abroad,
        10,
        on - days(20),
        on - days(6),
    )
    .await;
    let orphan = issued(
        &account,
        &pool,
        &tenant_id,
        &abroad,
        10,
        on - days(20),
        on - days(6),
    )
    .await;
    // 1 EUR = 1.10 USD on the day each was issued; the second carries no
    // snapshot, as a document raised before the tenant kept rates would.
    sqlx::query(
        "UPDATE billing_invoices SET currency = 'USD', fx_base_currency = 'EUR', \
             fx_rate_micro = 1100000, fx_rate_date = issue_date \
         WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant_id.as_str())
    .bind(crossed.as_str())
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE billing_invoices SET currency = 'USD', fx_base_currency = NULL, \
             fx_rate_micro = NULL, fx_rate_date = NULL \
         WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant_id.as_str())
    .bind(orphan.as_str())
    .execute(&pool)
    .await
    .unwrap();

    let report = aged(&account, on, AgedSide::Receivable).await;
    assert_eq!(report.document_count, 2);
    assert_eq!(report.unconverted_count, 1);
    assert_eq!(report.parties[0].unconverted_count, 1);
    // 1 210.00 dollars at 1.10 is 1 100.00 euro, and only the converted one is
    // in a band: the total is never part invention.
    assert_eq!(report.buckets.days_1_30_cents, 110_000);
    assert_eq!(report.buckets.total_cents, 110_000);
    for document in &report.parties[0].documents {
        assert_eq!(document.currency, "USD");
        assert_eq!(document.open_cents, 121_000, "its own currency, untouched");
    }
    let converted: Vec<Option<i64>> = report.parties[0]
        .documents
        .iter()
        .map(|d| d.base_open_cents)
        .collect();
    assert!(converted.contains(&Some(110_000)), "{converted:?}");
    assert!(converted.contains(&None), "{converted:?}");
}

#[tokio::test]
async fn one_tenants_debts_are_no_part_of_anothers_ageing() {
    let store = common::test_store().await;
    let pool = raw_pool().await;
    let (ours, our_tenant) = tenant(&store, "ours").await;
    let (theirs, their_tenant) = tenant(&store, "theirs").await;
    let on = today(&pool).await;

    let our_customer = customer(&ours, "Ours BV").await;
    issued(
        &ours,
        &pool,
        &our_tenant,
        &our_customer,
        1,
        on - days(40),
        on - days(26),
    )
    .await;
    let ours_before = aged(&ours, on, AgedSide::Receivable).await;

    // A hundred times our debts next door, on the same days, plus a payable.
    let their_customer = customer(&theirs, "Theirs BV").await;
    issued(
        &theirs,
        &pool,
        &their_tenant,
        &their_customer,
        100,
        on - days(40),
        on - days(26),
    )
    .await;
    let their_bill = theirs
        .create_billing_bill(&bill(
            "Their Supplier",
            "T-1",
            on - days(9),
            Some(on - days(1)),
        ))
        .await
        .unwrap();
    theirs
        .decide_billing_bill(&their_bill, BillStatus::Approved)
        .await
        .unwrap();

    let ours_after = aged(&ours, on, AgedSide::Receivable).await;
    assert_eq!(
        ours_before, ours_after,
        "their books moved nothing of ours, document for document"
    );
    assert_eq!(ours_after.buckets.total_cents, 12_100);
    assert_eq!(ours_after.parties.len(), 1);
    assert_eq!(ours_after.parties[0].name, "Ours BV");
    assert!(
        aged(&ours, on, AgedSide::Payable).await.parties.is_empty(),
        "their supplier is not ours"
    );

    let theirs_report = aged(&theirs, on, AgedSide::Receivable).await;
    assert_eq!(theirs_report.buckets.total_cents, 1_210_000);
    assert_eq!(theirs_report.parties.len(), 1);
    assert_eq!(theirs_report.parties[0].name, "Theirs BV");
    assert_eq!(
        aged(&theirs, on, AgedSide::Payable)
            .await
            .buckets
            .total_cents,
        121_000
    );
}
