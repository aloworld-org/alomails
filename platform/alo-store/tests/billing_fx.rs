//! Multi-currency invoicing (alo Billing, wave B1.21): the reference rates a
//! tenant imports, the rate a document is frozen at when it is issued, and the
//! tenancy proof (Law 1: isolation is tested, not assumed).
//!
//! The figures are hand-computed outside the code under test, like the VAT
//! summary's, because the money on a foreign-currency invoice is what an auditor
//! recomputes: `amount / rate`, rounded once, half away from zero.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::billing_fx::{IDENTITY_RATE_MICRO, parse_rate};
use alo_store::billing_fx_rates::{FxRateSource, MAX_RATE_AGE_DAYS};
use alo_store::{
    AccountStore, BillingCustomerId, BillingInvoiceId, NewBillingSettings, NewCustomer, NewInvoice,
    NewLine, Store, StoreError,
};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use time::{Date, Duration};

/// The daily reference-rate file, in the shape the ECB publishes it. `{day}` is
/// filled in with the day the test issues on, so the rates are the ones a
/// document raised now would actually reach.
const DAILY: &str = "Date, USD, JPY, PLN, CHF\n{day}, 1.1626, 171.42, 4.2755, 0.9385, \n";

async fn raw_pool() -> PgPool {
    PgPoolOptions::new()
        .max_connections(2)
        .connect(&common::database_url())
        .await
        .unwrap()
}

/// The database's own current date: the day the store stamps on an issue, and
/// therefore the day a rate has to be published for.
async fn today_of(pool: &PgPool) -> Date {
    sqlx::query_scalar("SELECT CURRENT_DATE")
        .fetch_one(pool)
        .await
        .unwrap()
}

/// A tenant with one user and one customer.
async fn tenant_with_customer(store: &Store, tag: &str) -> (AccountStore, BillingCustomerId) {
    let tenant = store.create_tenant(&format!("fx-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@fx.test"))
        .await
        .unwrap();
    let account = store.for_account(tenant, user);
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
    (account, customer)
}

/// One line: `units` whole units at `price_cents`, taxed at `rate_bp`.
fn item(units: i64, price_cents: i64, rate_bp: i32) -> NewLine {
    NewLine {
        description: "Item".to_owned(),
        unit: "piece".to_owned(),
        qty_milli: units * 1_000,
        unit_price_cents: price_cents,
        vat_rate_bp: rate_bp,
    }
}

/// A draft in `currency` with these lines.
async fn draft(
    account: &AccountStore,
    customer: &BillingCustomerId,
    currency: &str,
    lines: &[NewLine],
) -> BillingInvoiceId {
    let id = account
        .create_billing_invoice(&NewInvoice {
            currency: Some(currency.to_owned()),
            ..NewInvoice::for_customer(customer.clone())
        })
        .await
        .unwrap();
    account.set_billing_invoice_lines(&id, lines).await.unwrap();
    id
}

#[tokio::test]
async fn a_published_file_imports_and_reads_back_as_the_rates_it_states() {
    let store = common::test_store().await;
    let pool = raw_pool().await;
    let (a, _customer) = tenant_with_customer(&store, "import").await;
    let today = today_of(&pool).await;

    let summary = a
        .import_billing_fx_rates(&DAILY.replace("{day}", &today.to_string()))
        .await
        .unwrap();
    assert_eq!(summary.rates, 4, "one per quoted currency");
    assert_eq!(summary.days, 1);
    assert_eq!(summary.currencies, 4);
    assert_eq!(summary.from, Some(today));
    assert_eq!(summary.to, Some(today));

    let stored = a.billing_fx_rate_list(None, None, None).await.unwrap();
    assert_eq!(stored.len(), 4);
    let usd = stored.iter().find(|r| r.currency == "USD").unwrap();
    assert_eq!(usd.rate_micro, 1_162_600);
    assert_eq!(usd.rate_date, today);
    assert_eq!(usd.source, FxRateSource::Ecb, "where the figure came from");

    // Narrowing: one currency, and a period that excludes the day.
    assert_eq!(
        a.billing_fx_rate_list(Some("usd"), None, None)
            .await
            .unwrap()
            .len(),
        1,
        "the code is matched case-insensitively, like everywhere else"
    );
    let before = today - Duration::days(2);
    assert!(
        a.billing_fx_rate_list(None, Some(before), Some(before))
            .await
            .unwrap()
            .is_empty()
    );
    match a
        .billing_fx_rate_list(None, Some(today), Some(before))
        .await
    {
        Err(StoreError::Validation(message)) => {
            assert!(message.contains("before its start"), "{message}");
        }
        other => panic!("expected a refusal of a backwards period, got: {other:?}"),
    }

    // A correction re-imports the same day and overwrites it — one row per
    // currency per day, always.
    let corrected = a
        .import_billing_fx_rates(&format!("Date,USD\n{today},1.1700\n"))
        .await
        .unwrap();
    assert_eq!(corrected.rates, 1);
    let stored = a
        .billing_fx_rate_list(Some("USD"), None, None)
        .await
        .unwrap();
    assert_eq!(stored.len(), 1, "corrected, not duplicated");
    assert_eq!(stored[0].rate_micro, 1_170_000);

    // A malformed file leaves the table exactly as it was: half an import would
    // convert the next document from rates nobody checked.
    let before_bad = a.billing_fx_rate_list(None, None, None).await.unwrap();
    assert!(
        a.import_billing_fx_rates(&format!("Date,USD,JPY\n{today},1.16,not-a-rate\n"))
            .await
            .is_err()
    );
    assert_eq!(
        a.billing_fx_rate_list(None, None, None).await.unwrap(),
        before_bad
    );
}

#[tokio::test]
async fn a_rate_is_read_from_the_last_publication_at_or_before_the_day() {
    let store = common::test_store().await;
    let pool = raw_pool().await;
    let (a, _customer) = tenant_with_customer(&store, "lookback").await;
    let today = today_of(&pool).await;
    let friday = today - Duration::days(3);

    a.save_billing_fx_rate("USD", friday, parse_rate("1.1626").unwrap())
        .await
        .unwrap();

    // A weekend or a holiday is not a gap in the law: art. 91(2) says the last
    // preceding publication, which is exactly what comes back.
    let found = a.billing_fx_rate_on("USD", today).await.unwrap().unwrap();
    assert_eq!(found.rate_date, friday);
    assert_eq!(found.rate_micro, 1_162_600);
    assert_eq!(found.source, FxRateSource::Manual);

    // A day before the rate was published reaches nothing: a rate is never
    // applied backwards in time.
    assert!(
        a.billing_fx_rate_on("USD", friday - Duration::days(1))
            .await
            .unwrap()
            .is_none()
    );
    // And a stale rate is not reached across either.
    assert!(
        a.billing_fx_rate_on("USD", friday + Duration::days(MAX_RATE_AGE_DAYS + 1))
            .await
            .unwrap()
            .is_none()
    );
    // The euro is what the table quotes against, so it is never a stored row —
    // and never a refusal to a caller that asks.
    assert!(a.billing_fx_rate_on("EUR", today).await.unwrap().is_none());
    match a
        .save_billing_fx_rate("EUR", today, IDENTITY_RATE_MICRO)
        .await
    {
        Err(StoreError::Validation(message)) => {
            assert!(message.contains("quoted against"), "{message}");
        }
        other => panic!("expected a refusal to store a euro rate, got: {other:?}"),
    }
    // A rate outside the usable range is refused rather than stored.
    for bad in [0, -1] {
        assert!(a.save_billing_fx_rate("USD", today, bad).await.is_err());
    }
}

#[tokio::test]
async fn issuing_freezes_the_rate_and_the_document_states_its_own_conversion() {
    let store = common::test_store().await;
    let pool = raw_pool().await;
    let (a, customer) = tenant_with_customer(&store, "issue").await;
    let today = today_of(&pool).await;
    a.import_billing_fx_rates(&DAILY.replace("{day}", &today.to_string()))
        .await
        .unwrap();

    // 10 × $50.00 at 21 %: net $500.00, VAT $105.00.
    // At 1 EUR = 1.1626 USD: net 50 000 / 1.1626 = 43 007.05… → 430.07,
    //                        VAT 10 500 / 1.1626 =  9 031.48… →  90.31.
    let usd = draft(&a, &customer, "USD", &[item(10, 5_000, 2100)]).await;
    let document = a.issue_billing_invoice(&usd).await.unwrap();
    let fx = document.invoice.fx.as_ref().unwrap();
    assert_eq!(fx.base_currency, "EUR");
    assert_eq!(fx.rate_micro, 1_162_600);
    assert_eq!(fx.rate_date, today);
    let base = document.base_totals().unwrap();
    assert_eq!(base.net_cents, 43_007);
    assert_eq!(base.vat_cents, 9_031);
    assert_eq!(base.gross_cents, 52_038);
    assert_eq!(
        base.net_cents,
        base.vat_by_rate.iter().map(|r| r.net_cents).sum::<i64>(),
        "the restated rows add up to the restated total"
    );

    // A euro document in a euro-based tenant is stamped with the identity and
    // restates nothing: one figure, not the same figure twice.
    let eur = draft(&a, &customer, "EUR", &[item(1, 10_000, 2100)]).await;
    let domestic = a.issue_billing_invoice(&eur).await.unwrap();
    let fx = domestic.invoice.fx.as_ref().unwrap();
    assert_eq!(fx.rate_micro, IDENTITY_RATE_MICRO);
    assert_eq!(fx.rate_date, today);
    assert_eq!(domestic.base_totals(), None);

    // A draft carries no snapshot at all: the rate belongs to the moment the
    // document became a document.
    let pending = draft(&a, &customer, "USD", &[item(1, 1_000, 0)]).await;
    let read = a.billing_invoice(&pending).await.unwrap().unwrap();
    assert!(read.invoice.fx.is_none());
    assert_eq!(read.base_totals(), None);

    // A currency nobody has published a rate for cannot be issued — an invoice
    // that cannot state its VAT in the member state's currency is incomplete.
    let sek = draft(&a, &customer, "SEK", &[item(1, 1_000, 0)]).await;
    match a.issue_billing_invoice(&sek).await {
        Err(StoreError::Validation(message)) => {
            assert!(message.contains("no exchange rate for SEK"), "{message}");
        }
        other => panic!("expected a refusal, got: {other:?}"),
    }
    // …and it is still a draft afterwards, with no number spent on it.
    let still = a.billing_invoice(&sek).await.unwrap().unwrap();
    assert!(still.invoice.number.is_none());
    assert!(still.invoice.status.is_draft());
}

#[tokio::test]
async fn a_credit_note_inherits_its_originals_rate_so_the_pair_nets_to_zero() {
    let store = common::test_store().await;
    let pool = raw_pool().await;
    let (a, customer) = tenant_with_customer(&store, "credit").await;
    let today = today_of(&pool).await;
    a.import_billing_fx_rates(&DAILY.replace("{day}", &today.to_string()))
        .await
        .unwrap();

    let usd = draft(&a, &customer, "USD", &[item(10, 5_000, 2100)]).await;
    let original = a.issue_billing_invoice(&usd).await.unwrap();

    // The rate moves after the invoice was issued — as it does every day.
    a.save_billing_fx_rate("USD", today, parse_rate("1.3000").unwrap())
        .await
        .unwrap();

    let credit_id = a.create_billing_credit_note(&usd).await.unwrap();
    let credit = a.issue_billing_invoice(&credit_id).await.unwrap();
    let fx = credit.invoice.fx.as_ref().unwrap();
    assert_eq!(
        fx.rate_micro, 1_162_600,
        "the correction converts at the rate of the supply it corrects, not today's"
    );
    assert_eq!(
        fx.rate_date,
        original.invoice.fx.as_ref().unwrap().rate_date
    );

    // Which is the whole point: the pair sums to zero in the books, to the cent.
    let credited = credit.base_totals().unwrap();
    let invoiced = original.base_totals().unwrap();
    assert_eq!(invoiced.net_cents + credited.net_cents, 0);
    assert_eq!(invoiced.vat_cents + credited.vat_cents, 0);
    assert_eq!(invoiced.gross_cents + credited.gross_cents, 0);

    // A document issued afterwards does take today's rate — the snapshot is per
    // document, not per tenant.
    let later = draft(&a, &customer, "USD", &[item(1, 1_000, 0)]).await;
    let fresh = a.issue_billing_invoice(&later).await.unwrap();
    assert_eq!(fresh.invoice.fx.as_ref().unwrap().rate_micro, 1_300_000);
}

#[tokio::test]
async fn a_non_euro_issuer_crosses_two_rates_of_one_publication_day() {
    let store = common::test_store().await;
    let pool = raw_pool().await;
    let (a, customer) = tenant_with_customer(&store, "cross").await;
    let today = today_of(&pool).await;
    a.import_billing_fx_rates(&DAILY.replace("{day}", &today.to_string()))
        .await
        .unwrap();
    a.save_billing_settings(&NewBillingSettings {
        legal_name: "Alo Polska sp. z o.o.".to_owned(),
        country: "PL".to_owned(),
        base_currency: "PLN".to_owned(),
        ..Default::default()
    })
    .await
    .unwrap();
    assert_eq!(a.billing_base_currency().await.unwrap(), "PLN");

    // 1 EUR = 1.1626 USD and 1 EUR = 4.2755 PLN, so
    // 1 PLN = 1.1626 / 4.2755 = 0.2719213… USD → 271 921 micro-units.
    let usd = draft(&a, &customer, "USD", &[item(1, 10_000, 0)]).await;
    let document = a.issue_billing_invoice(&usd).await.unwrap();
    let fx = document.invoice.fx.as_ref().unwrap();
    assert_eq!(fx.base_currency, "PLN");
    assert_eq!(fx.rate_micro, 271_921);
    // $100.00 / 0.271921 = zł 367.754… → zł 367.75, in cents.
    assert_eq!(document.base_totals().unwrap().net_cents, 36_775);

    // A euro document is now the foreign one, crossed the other way:
    // 1 PLN = 1 / 4.2755 EUR = 0.2338907… → 233 891 micro-units.
    let eur = draft(&a, &customer, "EUR", &[item(1, 10_000, 0)]).await;
    let euro_document = a.issue_billing_invoice(&eur).await.unwrap();
    assert_eq!(
        euro_document.invoice.fx.as_ref().unwrap().rate_micro,
        233_891
    );
    // And a złoty document needs no rate at all: it is already the books.
    let pln = draft(&a, &customer, "PLN", &[item(1, 10_000, 0)]).await;
    let domestic = a.issue_billing_invoice(&pln).await.unwrap();
    assert_eq!(
        domestic.invoice.fx.as_ref().unwrap().rate_micro,
        IDENTITY_RATE_MICRO
    );
    assert_eq!(domestic.base_totals(), None);

    // A currency quoted on no day that also quotes the złoty cannot be crossed,
    // and the refusal says so rather than combining two days' rates.
    a.save_billing_fx_rate(
        "SEK",
        today - Duration::days(2),
        parse_rate("11.15").unwrap(),
    )
    .await
    .unwrap();
    let sek = draft(&a, &customer, "SEK", &[item(1, 1_000, 0)]).await;
    match a.issue_billing_invoice(&sek).await {
        Err(StoreError::Validation(message)) => {
            assert!(message.contains("quotes both SEK and PLN"), "{message}");
        }
        other => panic!("expected a refusal, got: {other:?}"),
    }
}

#[tokio::test]
async fn one_tenants_rates_never_reach_another_tenants_documents() {
    let store = common::test_store().await;
    let pool = raw_pool().await;
    let (a, customer_a) = tenant_with_customer(&store, "iso-a").await;
    let (b, customer_b) = tenant_with_customer(&store, "iso-b").await;
    let today = today_of(&pool).await;

    // Only A imports rates. B quotes the same currency on the same day.
    a.import_billing_fx_rates(&DAILY.replace("{day}", &today.to_string()))
        .await
        .unwrap();

    assert_eq!(
        a.billing_fx_rate_list(None, None, None)
            .await
            .unwrap()
            .len(),
        4
    );
    assert!(
        b.billing_fx_rate_list(None, None, None)
            .await
            .unwrap()
            .is_empty(),
        "B's table is B's, not A's minus something"
    );
    assert!(b.billing_fx_rate_on("USD", today).await.unwrap().is_none());

    // So B cannot issue a dollar invoice at all, while A can.
    let for_b = draft(&b, &customer_b, "USD", &[item(1, 10_000, 0)]).await;
    match b.issue_billing_invoice(&for_b).await {
        Err(StoreError::Validation(message)) => {
            assert!(message.contains("no exchange rate for USD"), "{message}");
        }
        other => panic!("expected B to be refused, got: {other:?}"),
    }
    let for_a = draft(&a, &customer_a, "USD", &[item(1, 10_000, 0)]).await;
    assert_eq!(
        a.issue_billing_invoice(&for_a)
            .await
            .unwrap()
            .invoice
            .fx
            .as_ref()
            .unwrap()
            .rate_micro,
        1_162_600
    );

    // B's own rate, once imported, is B's: a different figure for the same day,
    // and A's documents are unaffected by it.
    b.save_billing_fx_rate("USD", today, parse_rate("2.0").unwrap())
        .await
        .unwrap();
    assert_eq!(
        b.billing_fx_rate_on("USD", today)
            .await
            .unwrap()
            .unwrap()
            .rate_micro,
        2_000_000
    );
    assert_eq!(
        a.billing_fx_rate_on("USD", today)
            .await
            .unwrap()
            .unwrap()
            .rate_micro,
        1_162_600
    );
    // And each tenant's accounting currency is its own.
    b.save_billing_settings(&NewBillingSettings {
        legal_name: "B GmbH".to_owned(),
        country: "DE".to_owned(),
        base_currency: "CHF".to_owned(),
        ..Default::default()
    })
    .await
    .unwrap();
    assert_eq!(b.billing_base_currency().await.unwrap(), "CHF");
    assert_eq!(a.billing_base_currency().await.unwrap(), "EUR");
}
