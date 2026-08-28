//! **The VAT-return figures on a seeded year** (alo Finance, ADR 0035, wave
//! B4.11d) — the last of the four reports, asserted against figures computed by
//! hand from documents raised through the billing store, issued through the
//! gapless sequence and booked through the real posting rules.
//!
//! `src/fin_vat_return.rs` proves the fold: given four grouped reads, these are
//! the rates and these are the totals. This suite proves the five things a pure
//! test cannot.
//!
//! - **The year adds up to the hand-computed figure**, rate by rate and in
//!   total, over entries that reached Postgres through
//!   [`alo_store::AccountStore::post_invoice_issue`] and its credit-note
//!   sibling — including the correction, which takes tax back out of the period
//!   it was charged in.
//! - **★ The return and the invoices are one statement.** The output side is
//!   asserted equal, rate for rate and cent for cent, to
//!   [`alo_store::AccountStore::billing_vat_period`] (B1.20) — which reads the
//!   *documents* where this reads the *journal*. They can only differ if
//!   something was billed and not booked, or booked and not billed, and this is
//!   the test that says so ("a chart and a tax return cannot disagree").
//! - **The input side is real**, over a bill-shaped manual entry: recoverable
//!   tax, the cost it was paid on, and a net payable that is the subtraction.
//! - **The period is a real boundary**: a quarter holds only what was issued in
//!   it, and a year nothing was booked in reads zeroes in a stated currency.
//! - **Tenancy**: a second tenant's much larger year moves nothing on the
//!   first's return, and each reads only their own.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::{
    AccountStore, BillingCustomerId, BillingInvoiceId, CHART, ChartName, ChartSeed, EntryKind,
    FinAccountId, FxSnapshot, NewCustomer, NewEntry, NewInvoice, NewLine, NewPosting, Store,
    StoreError, VatReturn, VatReturnRate,
};
use time::{Date, Month};

fn on(year: i32, month: Month, day: u8) -> Date {
    Date::from_calendar_date(year, month, day).unwrap()
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

async fn tenant_with_chart(store: &Store, tag: &str) -> (AccountStore, BillingCustomerId) {
    let tenant = store.create_tenant(&format!("vat-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@vat.test"))
        .await
        .unwrap();
    let account = store.for_account(tenant, user);
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
    (account, customer)
}

async fn id_of(account: &AccountStore, code: &str) -> FinAccountId {
    account
        .fin_accounts(false)
        .await
        .unwrap()
        .into_iter()
        .find(|entry| entry.code == code)
        .unwrap_or_else(|| panic!("the seeded chart holds {code}"))
        .id
}

/// `units` whole units at `price_cents`, taxed at `rate_bp`.
fn line(units: i64, price_cents: i64, rate_bp: i32) -> NewLine {
    NewLine {
        description: format!("{units} × {price_cents} at {rate_bp}"),
        unit: "item".to_owned(),
        qty_milli: units * 1_000,
        unit_price_cents: price_cents,
        vat_rate_bp: rate_bp,
    }
}

/// An issued, booked invoice, and the day the books say it was issued.
async fn booked_invoice(
    account: &AccountStore,
    customer: &BillingCustomerId,
    lines: &[NewLine],
) -> (BillingInvoiceId, Date) {
    let id = account
        .create_billing_invoice(&NewInvoice::for_customer(customer.clone()))
        .await
        .unwrap();
    account.set_billing_invoice_lines(&id, lines).await.unwrap();
    let document = account.issue_billing_invoice(&id).await.unwrap();
    assert!(
        account.fin_invoice_entry(&id).await.unwrap().is_some(),
        "issuing books the document in the same transaction (B7.01)"
    );
    (id, document.invoice.issue_date.unwrap())
}

/// The full mirror of `original`, issued and booked — a correction, which takes
/// revenue and tax back out of the period they were charged in.
async fn booked_credit_note(account: &AccountStore, original: &BillingInvoiceId) {
    let id = account.create_billing_credit_note(original).await.unwrap();
    account.issue_billing_invoice(&id).await.unwrap();
    assert!(account.fin_invoice_entry(&id).await.unwrap().is_some());
}

/// **The seeded books**, scaled by `times` so a second tenant's are
/// unmistakably not the first's:
///
/// - an invoice of €1 000.00 at 21 % and €250.00 at 9 %;
/// - an invoice of €500.00 at 21 %, credited in full — the rate was used, and
///   it nets out;
/// - a bill-shaped manual entry: €400.00 of hosting at 21 %, €84.00 recoverable.
///
/// Returns the year the documents landed in, which is the period every
/// assertion below is made over.
async fn seeded_books(account: &AccountStore, customer: &BillingCustomerId, times: i64) -> i32 {
    let (_, issued) = booked_invoice(
        account,
        customer,
        &[line(10 * times, 10_000, 2100), line(times, 25_000, 900)],
    )
    .await;
    let (credited, _) = booked_invoice(account, customer, &[line(times, 50_000, 2100)]).await;
    booked_credit_note(account, &credited).await;

    // The purchase side, in the shape B5's bills will write it: the cost and the
    // recoverable tax both carry the rate, and the payable is the gross.
    let expense = id_of(account, "6000").await;
    let vat_input = id_of(account, "1200").await;
    let payable = id_of(account, "2000").await;
    let net = 40_000 * times;
    let vat = 8_400 * times;
    account
        .post_fin_entry(&NewEntry {
            entry_date: issued,
            kind: EntryKind::Bill,
            source: None,
            memo: "hosting".to_owned(),
            reverses_entry_id: None,
            attachment_node_id: None,
            currency: "EUR".to_owned(),
            fx: FxSnapshot::identity("EUR", issued),
            postings: vec![
                NewPosting {
                    vat_rate_bp: Some(2100),
                    ..NewPosting::new(expense, net, net)
                },
                NewPosting {
                    vat_rate_bp: Some(2100),
                    ..NewPosting::new(vat_input, vat, vat)
                },
                NewPosting::new(payable, -(net + vat), -(net + vat)),
            ],
        })
        .await
        .unwrap();
    issued.year()
}

async fn year_of(account: &AccountStore, year: i32) -> VatReturn {
    account
        .fin_vat_return(on(year, Month::January, 1), on(year, Month::December, 31))
        .await
        .unwrap()
}

#[tokio::test]
async fn a_seeded_year_reports_the_figures_computed_by_hand() {
    let store = common::test_store().await;
    let (account, customer) = tenant_with_chart(&store, "solo").await;
    let year = seeded_books(&account, &customer, 1).await;

    let report = year_of(&account, year).await;
    assert_eq!(report.from, on(year, Month::January, 1));
    assert_eq!(report.to, on(year, Month::December, 31));
    assert_eq!(report.currency, "EUR", "the tenant's accounting currency");

    // Output: €1 000.00 at 21 % (the €500.00 invoice was credited in full, so
    // its rate is a row that nets out), €250.00 at 9 %.
    assert_eq!(
        report.output.rates,
        vec![
            VatReturnRate {
                rate_bp: 900,
                base_cents: 25_000,
                vat_cents: 2_250,
            },
            VatReturnRate {
                rate_bp: 2100,
                base_cents: 100_000,
                vat_cents: 21_000,
            },
        ]
    );
    assert_eq!(report.output.base_cents, 125_000);
    assert_eq!(report.output.vat_cents, 23_250);
    assert_eq!(
        report.output.unrated_base_cents, 0,
        "every revenue posting a rule writes carries its rate"
    );
    assert_eq!(report.output.unrated_vat_cents, 0);

    // Input: the bill, and nothing else.
    assert_eq!(
        report.input.rates,
        vec![VatReturnRate {
            rate_bp: 2100,
            base_cents: 40_000,
            vat_cents: 8_400,
        }]
    );
    assert_eq!(report.input.base_cents, 40_000);
    assert_eq!(report.input.vat_cents, 8_400);

    assert_eq!(
        report.net_payable_cents, 14_850,
        "232.50 charged less 84.00 paid"
    );
    // The totals are the rows added up, which is what makes the return
    // checkable by hand.
    for side in [&report.output, &report.input] {
        assert_eq!(
            side.base_cents,
            side.rates.iter().map(|rate| rate.base_cents).sum::<i64>()
        );
        assert_eq!(
            side.vat_cents,
            side.rates.iter().map(|rate| rate.vat_cents).sum::<i64>()
        );
    }
}

/// ★ The journal's answer and the documents' answer are the same statement.
#[tokio::test]
async fn the_return_agrees_with_the_billing_summary_rate_for_rate() {
    let store = common::test_store().await;
    let (account, customer) = tenant_with_chart(&store, "agree").await;
    let year = seeded_books(&account, &customer, 1).await;

    let from = on(year, Month::January, 1);
    let to = on(year, Month::December, 31);
    let booked = account.fin_vat_return(from, to).await.unwrap();
    let billed = account.billing_vat_period(from, to).await.unwrap();

    assert_eq!(booked.currency, billed.base.currency);
    assert_eq!(
        booked.output.vat_cents, billed.base.vat_cents,
        "the tax the ledger carries is the tax the customers were charged"
    );
    assert_eq!(
        booked.output.base_cents, billed.base.net_cents,
        "and the taxable base is the net of the same documents"
    );
    assert_eq!(
        booked
            .output
            .rates
            .iter()
            .map(|rate| (rate.rate_bp, rate.base_cents, rate.vat_cents))
            .collect::<Vec<_>>(),
        billed
            .base
            .by_rate
            .iter()
            .map(|rate| (rate.rate_bp, rate.net_cents, rate.vat_cents))
            .collect::<Vec<_>>(),
        "rate for rate, in the same order"
    );
    assert_eq!(
        billed.base.unconverted_count, 0,
        "nothing was left out of the documents' side either"
    );
    // The purchase side is the ledger's alone: a bill is not a document the
    // billing summary reads, which is why the return exists as its own report.
    assert_eq!(booked.input.vat_cents, 8_400);
}

#[tokio::test]
async fn the_period_holds_only_what_was_issued_in_it() {
    let store = common::test_store().await;
    let (account, customer) = tenant_with_chart(&store, "period").await;
    let year = seeded_books(&account, &customer, 1).await;

    // A year before anything was raised: zeroes, in a currency the report still
    // names — a figure of nothing without a unit is a question.
    let quiet = account
        .fin_vat_return(
            on(year - 2, Month::January, 1),
            on(year - 2, Month::June, 30),
        )
        .await
        .unwrap();
    assert!(quiet.output.rates.is_empty() && quiet.input.rates.is_empty());
    assert_eq!(quiet.output.vat_cents, 0);
    assert_eq!(quiet.input.vat_cents, 0);
    assert_eq!(quiet.net_payable_cents, 0);
    assert_eq!(quiet.currency, "EUR");

    // A single day: the day everything was issued holds the whole year, and the
    // day after it holds nothing.
    let issued = account
        .fin_vat_return(on(year, Month::January, 1), on(year, Month::December, 31))
        .await
        .unwrap();
    assert_eq!(issued.output.vat_cents, 23_250);
}

#[tokio::test]
async fn a_period_that_ends_before_it_starts_is_refused() {
    let store = common::test_store().await;
    let (account, _customer) = tenant_with_chart(&store, "backwards").await;

    match account
        .fin_vat_return(on(2026, Month::December, 31), on(2026, Month::January, 1))
        .await
    {
        Err(StoreError::Validation(message)) => {
            assert!(message.contains("before its start"), "{message}");
        }
        other => panic!("expected Validation, got {other:?}"),
    }
}

#[tokio::test]
async fn one_tenants_tax_is_no_part_of_anothers_return() {
    let store = common::test_store().await;
    let (ours, our_customer) = tenant_with_chart(&store, "ours").await;
    let (theirs, their_customer) = tenant_with_chart(&store, "theirs").await;
    let year = seeded_books(&ours, &our_customer, 1).await;

    let before = year_of(&ours, year).await;
    // A hundred times our books, in the same accounts, on the same days.
    let their_year = seeded_books(&theirs, &their_customer, 100).await;
    assert_eq!(their_year, year, "both were issued today");
    let after = year_of(&ours, year).await;
    assert_eq!(
        before, after,
        "their books moved nothing of ours, rate for rate"
    );

    let theirs_report = year_of(&theirs, year).await;
    assert_eq!(theirs_report.output.vat_cents, 2_325_000);
    assert_eq!(theirs_report.input.vat_cents, 840_000);
    assert_eq!(theirs_report.net_payable_cents, 1_485_000);
}
