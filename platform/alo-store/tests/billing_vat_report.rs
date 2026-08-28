//! The VAT summary of a period (alo Billing, wave B1.20): a seeded quarter
//! whose every figure is computed by hand outside the code under test, the
//! documents the report must leave out, and the tenancy proof (Law 1: isolation
//! is tested, not assumed).
//!
//! The hand computation is the point of the suite. A VAT return is copied off
//! this report by a human who is legally answerable for it, so the assertions
//! below state the arithmetic in full rather than comparing the report against
//! a second implementation of itself.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::billing_vat_report::{VatPeriod, VatPeriodCurrency, VatPeriodRate};
use alo_store::{
    AccountStore, BillingCustomerId, BillingInvoiceId, NewCustomer, NewInvoice, NewLine, Store,
    StoreError, TenantId,
};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use time::{Date, Month};

/// The quarter every document below is seeded into or around: Q3 2025, long
/// past, so nothing the database's own clock does can drift into it.
fn q3_start() -> Date {
    Date::from_calendar_date(2025, Month::July, 1).unwrap()
}

fn q3_end() -> Date {
    Date::from_calendar_date(2025, Month::September, 30).unwrap()
}

/// A day inside the quarter.
fn day(month: Month, day: u8) -> Date {
    Date::from_calendar_date(2025, month, day).unwrap()
}

/// A tenant with one user and one customer, returning the account door, the
/// tenant id and that customer.
async fn tenant_with_customer(
    store: &Store,
    tag: &str,
) -> (AccountStore, TenantId, BillingCustomerId) {
    let tenant = store.create_tenant(&format!("vat-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@vat-report.test"))
        .await
        .unwrap();
    let account = store.for_account(tenant.clone(), user);
    common::seed_default_chart(&account).await;
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

/// A raw pool alongside the store, for dating documents into the past — the one
/// thing a test cannot do by waiting, and which the store deliberately refuses
/// to let a caller do (an issue date is the database's own clock).
async fn raw_pool() -> PgPool {
    PgPoolOptions::new()
        .max_connections(2)
        .connect(&common::database_url())
        .await
        .unwrap()
}

/// The database's own current date — the day the store issues on, and therefore
/// the day a rate has to be published for.
async fn today_of(pool: &PgPool) -> Date {
    sqlx::query_scalar("SELECT CURRENT_DATE")
        .fetch_one(pool)
        .await
        .unwrap()
}

/// `units` whole units at `price_cents`, taxed at `rate_bp`.
fn item(units: i64, price_cents: i64, rate_bp: i32) -> NewLine {
    NewLine {
        description: "Item".to_owned(),
        unit: "piece".to_owned(),
        qty_milli: units * 1_000,
        unit_price_cents: price_cents,
        vat_rate_bp: rate_bp,
    }
}

/// Raises a draft with these lines and issues it, then moves its issue date to
/// `issued_on` — the seeding a period report needs.
async fn issued_on(
    account: &AccountStore,
    pool: &PgPool,
    tenant: &TenantId,
    customer: &BillingCustomerId,
    lines: &[NewLine],
    date: Date,
) -> BillingInvoiceId {
    let id = account
        .create_billing_invoice(&NewInvoice::for_customer(customer.clone()))
        .await
        .unwrap();
    account.set_billing_invoice_lines(&id, lines).await.unwrap();
    account.issue_billing_invoice(&id).await.unwrap();
    backdate(pool, tenant, &id, date).await;
    id
}

/// Moves a document's issue and due dates to `date`.
async fn backdate(pool: &PgPool, tenant: &TenantId, id: &BillingInvoiceId, date: Date) {
    let done = sqlx::query(
        "UPDATE billing_invoices SET issue_date = $3, due_date = $3 \
         WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_str())
    .bind(id.as_str())
    .bind(date)
    .execute(pool)
    .await
    .unwrap();
    assert_eq!(done.rows_affected(), 1);
}

/// The single currency group of a report, or a panic naming what came instead.
fn only(period: &VatPeriod) -> &VatPeriodCurrency {
    assert_eq!(
        period.currencies.len(),
        1,
        "expected one currency: {period:?}"
    );
    period.currencies.first().unwrap()
}

#[tokio::test]
async fn a_seeded_quarter_reproduces_the_hand_computed_totals() {
    let store = common::test_store().await;
    let pool = raw_pool().await;
    let (a, tenant, customer) = tenant_with_customer(&store, "quarter").await;

    // ---- what the quarter holds -------------------------------------------
    //
    // 1. 10 × €100.00 at 21 %      → net 100 000, VAT 21 000
    let first = issued_on(
        &a,
        &pool,
        &tenant,
        &customer,
        &[item(10, 10_000, 2100)],
        day(Month::July, 4),
    )
    .await;
    // 2. 1 × €500.00 at 21 % and 1 × €250.00 at 9 %
    //                             → net 50 000 + 25 000, VAT 10 500 + 2 250
    issued_on(
        &a,
        &pool,
        &tenant,
        &customer,
        &[item(1, 50_000, 2100), item(1, 25_000, 900)],
        day(Month::August, 15),
    )
    .await;
    // 3. Three documents of €9.99 at 21 %, each charging 2.10 (0.21 × 9.99 =
    //    2.0979 → 2.10). Their tax is 6.30 — a cent more than 21 % of the
    //    summed net (29.97 × 0.21 = 6.2937 → 6.29), which is exactly the
    //    difference between summing what was charged and re-applying the rate.
    for date in [
        day(Month::September, 1),
        day(Month::September, 2),
        day(Month::September, 3),
    ] {
        issued_on(&a, &pool, &tenant, &customer, &[item(3, 333, 2100)], date).await;
    }
    // 4. A credit note against the first document, edited down to half of it:
    //    5 × €100.00 at 21 % → net −50 000, VAT −10 500.
    let credit = a.create_billing_credit_note(&first).await.unwrap();
    a.set_billing_invoice_lines(
        &credit,
        &[NewLine {
            qty_milli: -5_000,
            ..item(5, 10_000, 2100)
        }],
    )
    .await
    .unwrap();
    a.issue_billing_invoice(&credit).await.unwrap();
    backdate(&pool, &tenant, &credit, day(Month::September, 20)).await;

    // ---- and what must stay out of it -------------------------------------
    //
    // A document issued the day before the period opens, and one the day after
    // it closes: the boundaries are inclusive, so these two are the proof that
    // they are not one day wider.
    issued_on(
        &a,
        &pool,
        &tenant,
        &customer,
        &[item(1, 1_000_000, 2100)],
        day(Month::June, 30),
    )
    .await;
    issued_on(
        &a,
        &pool,
        &tenant,
        &customer,
        &[item(1, 1_000_000, 2100)],
        day(Month::October, 1),
    )
    .await;
    // A void document: cancelled, so it charged nobody any tax.
    let voided = issued_on(
        &a,
        &pool,
        &tenant,
        &customer,
        &[item(1, 2_000_000, 2100)],
        day(Month::August, 1),
    )
    .await;
    a.void_billing_invoice(&voided).await.unwrap();
    // A draft: never raised, carries no number, and has no issue date at all.
    let draft = a
        .create_billing_invoice(&NewInvoice::for_customer(customer.clone()))
        .await
        .unwrap();
    a.set_billing_invoice_lines(&draft, &[item(1, 3_000_000, 2100)])
        .await
        .unwrap();

    // ---- the report --------------------------------------------------------
    let period = a.billing_vat_period(q3_start(), q3_end()).await.unwrap();
    assert_eq!(period.from, q3_start());
    assert_eq!(period.to, q3_end());
    let eur = only(&period);
    assert_eq!(eur.currency, "EUR");
    assert_eq!(eur.invoice_count, 5, "the five that stand in the quarter");
    assert_eq!(eur.credit_note_count, 1);

    // 9 %:  25 000 net, 2 250 VAT.
    // 21 %: 100 000 + 50 000 + 2 997 − 50 000 = 102 997 net,
    //        21 000 + 10 500 +   630 − 10 500 =  21 630 VAT.
    assert_eq!(
        eur.by_rate,
        vec![
            VatPeriodRate {
                rate_bp: 900,
                net_cents: 25_000,
                vat_cents: 2_250,
            },
            VatPeriodRate {
                rate_bp: 2100,
                net_cents: 102_997,
                vat_cents: 21_630,
            },
        ]
    );
    assert_eq!(eur.net_cents, 127_997);
    assert_eq!(eur.vat_cents, 23_880);
    assert_eq!(eur.gross_cents, 151_877);
    // The totals are exactly the rows: what makes the report checkable by hand.
    assert_eq!(
        eur.net_cents,
        eur.by_rate.iter().map(|r| r.net_cents).sum::<i64>()
    );
    assert_eq!(
        eur.vat_cents,
        eur.by_rate.iter().map(|r| r.vat_cents).sum::<i64>()
    );
    assert_eq!(eur.gross_cents, eur.net_cents + eur.vat_cents);

    // ---- the excluded documents are not merely invisible here --------------
    // A wider period picks up the two outside it, which proves they were left
    // out by the dates rather than by never having been written.
    let year = a
        .billing_vat_period(
            Date::from_calendar_date(2025, Month::January, 1).unwrap(),
            Date::from_calendar_date(2025, Month::December, 31).unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        only(&year).invoice_count,
        7,
        "the five plus the two outside"
    );
    // The void one and the draft are in neither: they charged nobody.
    assert_eq!(
        only(&year).net_cents,
        127_997 + 2_000_000,
        "the two €10 000 documents, and not the voided or drafted ones"
    );

    // ---- one day is a period too ------------------------------------------
    let one_day = a
        .billing_vat_period(day(Month::July, 4), day(Month::July, 4))
        .await
        .unwrap();
    let that_day = only(&one_day);
    assert_eq!(that_day.invoice_count, 1);
    assert_eq!(that_day.net_cents, 100_000);
    assert_eq!(that_day.vat_cents, 21_000);

    // ---- an empty period is empty, not a row of zeros ----------------------
    let quiet = a
        .billing_vat_period(
            Date::from_calendar_date(2024, Month::January, 1).unwrap(),
            Date::from_calendar_date(2024, Month::March, 31).unwrap(),
        )
        .await
        .unwrap();
    assert!(quiet.currencies.is_empty(), "{quiet:?}");
}

#[tokio::test]
async fn each_currency_is_summarised_on_its_own_and_then_once_in_the_accounting_currency() {
    let store = common::test_store().await;
    let pool = raw_pool().await;
    let (a, tenant, customer) = tenant_with_customer(&store, "currencies").await;

    issued_on(
        &a,
        &pool,
        &tenant,
        &customer,
        &[item(1, 10_000, 2100)],
        day(Month::July, 10),
    )
    .await;

    // The same customer billed in dollars. Issuing it needs a published rate:
    // without one the document could not state its VAT in the tenant's own
    // currency, so the store refuses (B1.21) rather than inventing a rate.
    let usd = a
        .create_billing_invoice(&NewInvoice {
            currency: Some("USD".to_owned()),
            ..NewInvoice::for_customer(customer.clone())
        })
        .await
        .unwrap();
    a.set_billing_invoice_lines(&usd, &[item(1, 20_000, 0)])
        .await
        .unwrap();
    match a.issue_billing_invoice(&usd).await {
        Err(StoreError::Validation(message)) => {
            assert!(message.contains("no exchange rate for USD"), "{message}");
            assert!(message.contains("import the reference rates"), "{message}");
        }
        other => panic!("expected a refusal without a rate, got: {other:?}"),
    }

    // 1 EUR = 1.1626 USD, published today (the day the store will issue on).
    let today = today_of(&pool).await;
    a.save_billing_fx_rate("USD", today, 1_162_600)
        .await
        .unwrap();
    let issued = a.issue_billing_invoice(&usd).await.unwrap();
    let fx = issued
        .invoice
        .fx
        .as_ref()
        .expect("a snapshot was frozen on it");
    assert_eq!(fx.base_currency, "EUR");
    assert_eq!(fx.rate_micro, 1_162_600);
    assert_eq!(fx.rate_date, today);
    // The document also knows what it is worth in the tenant's books:
    // 200.00 USD / 1.1626 = 172.0282… → 172.03.
    assert_eq!(
        issued.base_totals().map(|t| t.net_cents),
        Some(17_203),
        "the figure a foreign-currency invoice has to print to state its VAT"
    );
    backdate(&pool, &tenant, &usd, day(Month::July, 11)).await;

    let period = a.billing_vat_period(q3_start(), q3_end()).await.unwrap();
    assert_eq!(
        period
            .currencies
            .iter()
            .map(|c| c.currency.as_str())
            .collect::<Vec<_>>(),
        vec!["EUR", "USD"],
        "never added together in their own groups, and ascending by code"
    );
    assert_eq!(period.currencies[0].net_cents, 10_000);
    assert_eq!(period.currencies[0].vat_cents, 2_100);
    assert_eq!(period.currencies[1].net_cents, 20_000);
    assert_eq!(period.currencies[1].vat_cents, 0);
    // Each group also says what it contributes to the books, at the rate frozen
    // on its own documents.
    assert_eq!(
        period.currencies[0].base_net_cents, 10_000,
        "already in euro"
    );
    assert_eq!(period.currencies[1].base_net_cents, 17_203);
    assert_eq!(period.currencies[1].unconverted_count, 0);
    // And then, once, the figure a return is filed from.
    assert_eq!(period.base.currency, "EUR");
    assert_eq!(period.base.net_cents, 10_000 + 17_203);
    assert_eq!(period.base.vat_cents, 2_100);
    assert_eq!(period.base.gross_cents, 10_000 + 17_203 + 2_100);
    assert_eq!(period.base.unconverted_count, 0);
    assert_eq!(
        period.base.net_cents,
        period.base.by_rate.iter().map(|r| r.net_cents).sum::<i64>(),
        "the base rows add up to the base total"
    );
}

#[tokio::test]
async fn a_period_that_ends_before_it_starts_is_refused() {
    let store = common::test_store().await;
    let (a, _tenant, _customer) = tenant_with_customer(&store, "backwards").await;

    match a.billing_vat_period(q3_end(), q3_start()).await {
        Err(StoreError::Validation(message)) => {
            assert!(message.contains("before its start"), "{message}");
        }
        other => panic!("expected Validation, got: {other:?}"),
    }
    // The degenerate case is not the backwards one: a single day is a period.
    let same = a.billing_vat_period(q3_end(), q3_end()).await.unwrap();
    assert!(same.currencies.is_empty());
}

#[tokio::test]
async fn one_tenants_documents_never_reach_another_tenants_report() {
    let store = common::test_store().await;
    let pool = raw_pool().await;
    let (a, tenant_a, customer_a) = tenant_with_customer(&store, "iso-a").await;
    let (b, tenant_b, customer_b) = tenant_with_customer(&store, "iso-b").await;

    issued_on(
        &a,
        &pool,
        &tenant_a,
        &customer_a,
        &[item(10, 10_000, 2100)],
        day(Month::July, 4),
    )
    .await;
    issued_on(
        &b,
        &pool,
        &tenant_b,
        &customer_b,
        &[item(1, 700, 600)],
        day(Month::July, 4),
    )
    .await;

    // Each tenant sees exactly its own documents over the same days — B's
    // report is not A's minus something, it is B's.
    let for_a = a.billing_vat_period(q3_start(), q3_end()).await.unwrap();
    assert_eq!(only(&for_a).net_cents, 100_000);
    assert_eq!(only(&for_a).vat_cents, 21_000);
    assert_eq!(only(&for_a).invoice_count, 1);

    let for_b = b.billing_vat_period(q3_start(), q3_end()).await.unwrap();
    assert_eq!(only(&for_b).net_cents, 700);
    assert_eq!(only(&for_b).vat_cents, 42);
    assert_eq!(only(&for_b).invoice_count, 1);
    assert_eq!(
        only(&for_b).by_rate,
        vec![VatPeriodRate {
            rate_bp: 600,
            net_cents: 700,
            vat_cents: 42,
        }],
        "A's 21 % subtotal appears nowhere in B's report"
    );

    // A third tenant with nothing at all sees nothing at all — never the
    // others' figures, and never an error that would tell it they exist.
    let (c, _tenant_c, _customer_c) = tenant_with_customer(&store, "iso-c").await;
    let for_c = c.billing_vat_period(q3_start(), q3_end()).await.unwrap();
    assert!(for_c.currencies.is_empty(), "{for_c:?}");
}
