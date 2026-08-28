//! The alo Insights query engine against the real Postgres (BI1.03): golden
//! series a human can check by hand, and the proof that **a spec is not a
//! capability** (Law 1: isolation is tested, not assumed).
//!
//! Two tenants are seeded with different figures and then handed the *same*
//! ChartSpec. Every evaluation answers its own tenant's numbers — the tenant id
//! comes from the account door and a spec has no field that could name one — and
//! a filter naming the other tenant's customer is a typed refusal rather than a
//! silently empty chart, which is how a business comes to believe it billed
//! nothing last quarter.
//!
//! The rest is the arithmetic: the money a chart shows is the money the
//! documents say, folded by the same functions the printed invoice and the VAT
//! return use, restated at each document's own frozen rate, and never derived in
//! SQL. Where a bucket is computed twice — in Rust for the folded datasets and
//! by Postgres for the grouped ones — the two spellings are checked against each
//! other, so one dataset's January can never land beside another's.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::insight_series::{ALL_GROUP, OTHER_BUCKET, TOTAL_BUCKET};
use alo_store::{
    AccountStore, BillingCustomerId, BillingInvoiceId, ChartSpec, CrmPipelineId, CrmStageId, Label,
    NewCustomer, NewDeal, NewInvoice, NewLine, NewPayment, NewPipeline, NewStage, Series,
    StageMove, Store, StoreError, TenantId, Unit, UserId,
};
use serde_json::{Value, json};
use sqlx::postgres::PgPool;
use time::{Date, Month};

/// A tenant with one user, its account door, and the ids both are known by.
struct Workspace {
    store: AccountStore,
    tenant: TenantId,
    user: UserId,
}

async fn workspace(store: &Store, tag: &str) -> Workspace {
    let tenant = store.create_tenant(&format!("bi3-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@insights.test"))
        .await
        .unwrap();
    let account = store.for_account(tenant.clone(), user.clone());
    common::seed_default_chart(&account).await;
    Workspace {
        store: account,
        tenant,
        user,
    }
}

fn day(year: i32, month: Month, day: u8) -> Date {
    Date::from_calendar_date(year, month, day).unwrap()
}

/// The day every dated assertion below is made against — stated, never `now`,
/// so a golden series does not change at midnight.
fn today() -> Date {
    day(2026, Month::August, 7)
}

fn spec(value: Value) -> ChartSpec {
    ChartSpec::from_value(value).unwrap_or_else(|e| panic!("spec rejected: {e}"))
}

/// The points of the one group a single-series answer has.
fn points(series: &Series, group: &str) -> Vec<(String, i64)> {
    series
        .groups
        .iter()
        .find(|g| g.key == group)
        .map(|g| {
            g.points
                .iter()
                .map(|p| (p.bucket.clone(), p.value))
                .collect()
        })
        .unwrap_or_default()
}

/// The label of one bucket of one group.
fn label_of(series: &Series, group: &str, bucket: &str) -> Option<Label> {
    series
        .groups
        .iter()
        .find(|g| g.key == group)?
        .points
        .iter()
        .find(|p| p.bucket == bucket)?
        .label
        .clone()
}

async fn customer(ws: &Workspace, name: &str) -> BillingCustomerId {
    ws.store
        .create_billing_customer(&NewCustomer {
            name: name.to_owned(),
            country: "NL".to_owned(),
            payment_terms_days: 30,
            ..NewCustomer::default()
        })
        .await
        .unwrap()
}

/// `units` whole units at `price_cents`, taxed at `rate_bp`.
fn line(units: i64, price_cents: i64, rate_bp: i32) -> NewLine {
    NewLine {
        description: "Consulting".to_owned(),
        unit: "hour".to_owned(),
        qty_milli: units * 1_000,
        unit_price_cents: price_cents,
        vat_rate_bp: rate_bp,
    }
}

/// An issued invoice for `customer` with the given lines.
async fn issued(
    ws: &Workspace,
    customer: &BillingCustomerId,
    lines: &[NewLine],
) -> BillingInvoiceId {
    let id = ws
        .store
        .create_billing_invoice(&NewInvoice {
            customer_id: customer.clone(),
            currency: None,
            payment_terms_days: None,
            reference: String::new(),
            note: String::new(),
        })
        .await
        .unwrap();
    ws.store
        .set_billing_invoice_lines(&id, lines)
        .await
        .unwrap();
    ws.store.issue_billing_invoice(&id).await.unwrap();
    id
}

/// Moves an issued document's dates to a stated day.
///
/// Issuing stamps the **database's** date, which is exactly right in production
/// and useless for a golden series: a fixture has to say which month it is in.
/// The columns are written directly rather than through the store because there
/// is deliberately no API that back-dates a numbered document.
async fn backdate(
    pool: &PgPool,
    tenant: &TenantId,
    id: &BillingInvoiceId,
    issued: Date,
    due: Date,
) {
    sqlx::query(
        "UPDATE billing_invoices SET issue_date = $3, due_date = $4 \
         WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_str())
    .bind(id.as_str())
    .bind(issued)
    .bind(due)
    .execute(pool)
    .await
    .unwrap();
}

/// A board with an open, a won and a lost column.
async fn board(ws: &Workspace) -> (CrmPipelineId, CrmStageId, CrmStageId, CrmStageId) {
    let pipeline = ws
        .store
        .create_crm_pipeline(&NewPipeline {
            name: "Sales".to_owned(),
            description: String::new(),
        })
        .await
        .unwrap();
    let stage = |name: &str, won: bool, lost: bool| {
        let input = NewStage {
            name: name.to_owned(),
            is_won: won,
            is_lost: lost,
        };
        let pipeline = pipeline.clone();
        async move { ws.store.create_crm_stage(&pipeline, &input).await.unwrap() }
    };
    let new = stage("New", false, false).await;
    let won = stage("Won", true, false).await;
    let lost = stage("Lost", false, true).await;
    (pipeline, new, won, lost)
}

// ---- golden series ----------------------------------------------------------

#[tokio::test]
async fn revenue_by_month_is_the_money_the_documents_say() {
    let store = common::test_store().await;
    let pool = PgPool::connect(&common::database_url()).await.unwrap();
    let ws = workspace(&store, "revenue").await;
    let acme = customer(&ws, "Acme BV").await;

    // Hand-computed, outside the code under test:
    //   June: 10 × €100.00 at 21 %      → net 100 000, VAT 21 000, gross 121 000
    //   August: 1 × €500.00 at 21 % and 1 × €250.00 at 9 %
    //           → net 50 000 + 25 000 = 75 000
    //           → VAT 10 500 + 2 250   = 12 750
    let june = issued(&ws, &acme, &[line(10, 10_000, 2100)]).await;
    backdate(
        &pool,
        &ws.tenant,
        &june,
        day(2026, Month::June, 10),
        day(2026, Month::July, 10),
    )
    .await;
    let august = issued(&ws, &acme, &[line(1, 50_000, 2100), line(1, 25_000, 900)]).await;
    backdate(
        &pool,
        &ws.tenant,
        &august,
        day(2026, Month::August, 3),
        day(2026, Month::September, 2),
    )
    .await;

    let net_by_month = spec(json!({
        "schema_version": 1,
        "dataset": "billing.documents",
        "measure": { "id": "net", "agg": "sum" },
        "dimension": { "id": "issue_date", "grain": "month" },
        "period": { "kind": "range", "from": "2026-06-01", "to": "2026-08-31" },
        "viz": "bar"
    }));
    let series = ws
        .store
        .insight_evaluate_on(&net_by_month, today())
        .await
        .unwrap();
    assert_eq!(series.unit.kind, Unit::Money);
    assert_eq!(series.unit.currency.as_deref(), Some("EUR"));
    assert_eq!(
        points(&series, "EUR"),
        vec![
            ("2026-06".to_owned(), 100_000),
            // A quiet month is nothing earned, not a month that did not happen.
            ("2026-07".to_owned(), 0),
            ("2026-08".to_owned(), 75_000),
        ]
    );
    assert!(series.notes.is_empty(), "nothing needed converting");
    assert!(!series.truncated);

    // The same period measured three ways adds up the way the documents do.
    for (measure, expected) in [("vat", 21_000 + 12_750), ("gross", 121_000 + 87_750)] {
        let mut value = net_by_month.to_value().unwrap();
        value["measure"] = json!({ "id": measure, "agg": "sum" });
        value["dimension"] = json!(null);
        value.as_object_mut().unwrap().remove("dimension");
        value["viz"] = json!("number");
        let one = ws
            .store
            .insight_evaluate_on(&spec(value), today())
            .await
            .unwrap();
        assert_eq!(
            points(&one, "EUR"),
            vec![(TOTAL_BUCKET.to_owned(), expected)],
            "{measure} over the period"
        );
    }

    // Counting documents is not money: no currency, no restatement.
    let mut counted = net_by_month.to_value().unwrap();
    counted["measure"] = json!({ "id": "count", "agg": "count" });
    let counted = ws
        .store
        .insight_evaluate_on(&spec(counted), today())
        .await
        .unwrap();
    assert_eq!(counted.unit.kind, Unit::Count);
    assert!(counted.unit.currency.is_none());
    assert_eq!(
        points(&counted, ALL_GROUP),
        vec![
            ("2026-06".to_owned(), 1),
            ("2026-07".to_owned(), 0),
            ("2026-08".to_owned(), 1),
        ]
    );

    pool.close().await;
}

#[tokio::test]
async fn a_two_rate_document_splits_its_money_per_rate_and_still_counts_once() {
    let store = common::test_store().await;
    let ws = workspace(&store, "rates").await;
    let acme = customer(&ws, "Acme BV").await;
    // 1 × €500.00 at 21 % and 1 × €250.00 at 9 %, on ONE document.
    issued(&ws, &acme, &[line(1, 50_000, 2100), line(1, 25_000, 900)]).await;

    let by_rate = spec(json!({
        "schema_version": 1,
        "dataset": "billing.documents",
        "measure": { "id": "net", "agg": "sum" },
        "dimension": { "id": "vat_rate" },
        "period": { "kind": "all" },
        "sort": { "by": "dimension", "dir": "asc" },
        "viz": "bar"
    }));
    let series = ws
        .store
        .insight_evaluate_on(&by_rate, today())
        .await
        .unwrap();
    assert_eq!(
        points(&series, "EUR"),
        vec![("00900".to_owned(), 25_000), ("02100".to_owned(), 50_000)],
        "each rate carries its own subtotal, and the two add up to the document"
    );
    assert_eq!(
        label_of(&series, "EUR", "00900"),
        Some(Label::RateBp { bp: 900 }),
        "a rate is a number the client formats, never English from us — and its \
         key is padded so 9 % sorts before 21 % rather than after it"
    );

    // Asking for one rate asks for that PART of the document: its 21 % money,
    // and the document counted once because it used the rate.
    let mut only_21 = by_rate.to_value().unwrap();
    only_21["filters"] = json!([{ "id": "vat_rate", "op": "in", "values": ["2100"] }]);
    let restricted = ws
        .store
        .insight_evaluate_on(&spec(only_21.clone()), today())
        .await
        .unwrap();
    assert_eq!(
        points(&restricted, "EUR"),
        vec![("02100".to_owned(), 50_000)]
    );

    only_21["measure"] = json!({ "id": "count", "agg": "count" });
    only_21["dimension"] = json!({ "id": "currency" });
    let counted = ws
        .store
        .insight_evaluate_on(&spec(only_21), today())
        .await
        .unwrap();
    assert_eq!(
        points(&counted, ALL_GROUP),
        vec![("EUR".to_owned(), 1)],
        "one document, whatever its rates — never one per rate"
    );

    // A rate nobody charged is an empty chart, not a wrong one.
    let mut only_6 = by_rate.to_value().unwrap();
    only_6["filters"] = json!([{ "id": "vat_rate", "op": "in", "values": ["600"] }]);
    let none = ws
        .store
        .insight_evaluate_on(&spec(only_6), today())
        .await
        .unwrap();
    assert!(points(&none, "EUR").is_empty());
}

#[tokio::test]
async fn a_receivable_is_what_is_still_owed_and_how_late_it_is() {
    let store = common::test_store().await;
    let pool = PgPool::connect(&common::database_url()).await.unwrap();
    let ws = workspace(&store, "owed").await;
    let acme = customer(&ws, "Acme BV").await;

    // Gross 121 000. Forty days late, with 21 000 received.
    let late = issued(&ws, &acme, &[line(10, 10_000, 2100)]).await;
    backdate(
        &pool,
        &ws.tenant,
        &late,
        day(2026, Month::May, 29),
        day(2026, Month::June, 28),
    )
    .await;
    ws.store
        .record_billing_payment(
            &late,
            &NewPayment {
                paid_on: Some(day(2026, Month::July, 1)),
                amount_cents: 21_000,
                method: "SEPA".to_owned(),
                reference: String::new(),
            },
        )
        .await
        .unwrap();
    // Gross 12 100, not yet due.
    let soon = issued(&ws, &acme, &[line(1, 10_000, 2100)]).await;
    backdate(
        &pool,
        &ws.tenant,
        &soon,
        day(2026, Month::August, 1),
        day(2026, Month::August, 31),
    )
    .await;

    let outstanding = spec(json!({
        "schema_version": 1,
        "dataset": "billing.receivables",
        "measure": { "id": "outstanding", "agg": "sum" },
        "period": { "kind": "all" },
        "viz": "number"
    }));
    let series = ws
        .store
        .insight_evaluate_on(&outstanding, today())
        .await
        .unwrap();
    assert_eq!(
        points(&series, "EUR"),
        vec![(TOTAL_BUCKET.to_owned(), 100_000 + 12_100)],
        "121 000 less the 21 000 received, plus the one not yet due"
    );

    let mut aged = outstanding.to_value().unwrap();
    aged["dimension"] = json!({ "id": "age_bucket" });
    aged["viz"] = json!("bar");
    aged["sort"] = json!({ "by": "dimension", "dir": "asc" });
    let aged = ws
        .store
        .insight_evaluate_on(&spec(aged), today())
        .await
        .unwrap();
    assert_eq!(
        points(&aged, "EUR"),
        vec![
            ("age.31_60".to_owned(), 100_000),
            // Money that is not yet due is not money that is late: an aged
            // report that mixed them would overstate the problem.
            ("age.not_due".to_owned(), 12_100),
        ]
    );
    assert_eq!(
        label_of(&aged, "EUR", "age.31_60"),
        Some(Label::Catalog { id: "age.31_60" }),
        "our own vocabulary crosses as an id the client translates"
    );

    // Settling it in full takes it out of the receivables entirely.
    ws.store
        .record_billing_payment(
            &late,
            &NewPayment {
                paid_on: Some(today()),
                amount_cents: 100_000,
                method: "SEPA".to_owned(),
                reference: String::new(),
            },
        )
        .await
        .unwrap();
    let after = ws
        .store
        .insight_evaluate_on(&outstanding, today())
        .await
        .unwrap();
    assert_eq!(
        points(&after, "EUR"),
        vec![(TOTAL_BUCKET.to_owned(), 12_100)]
    );

    pool.close().await;
}

#[tokio::test]
async fn payments_bucket_by_the_day_the_money_arrived_and_stay_in_their_currency() {
    let store = common::test_store().await;
    let ws = workspace(&store, "paid").await;
    let acme = customer(&ws, "Acme BV").await;
    let invoice = issued(&ws, &acme, &[line(100, 10_000, 0)]).await;
    for (paid_on, cents) in [
        (day(2026, Month::June, 3), 25_000i64),
        (day(2026, Month::June, 29), 75_000),
        (day(2026, Month::August, 1), 40_000),
    ] {
        ws.store
            .record_billing_payment(
                &invoice,
                &NewPayment {
                    paid_on: Some(paid_on),
                    amount_cents: cents,
                    method: "SEPA".to_owned(),
                    reference: String::new(),
                },
            )
            .await
            .unwrap();
    }

    let by_month = spec(json!({
        "schema_version": 1,
        "dataset": "billing.payments",
        "measure": { "id": "amount", "agg": "sum" },
        "dimension": { "id": "paid_on", "grain": "month" },
        "period": { "kind": "range", "from": "2026-06-01", "to": "2026-08-31" },
        "viz": "line"
    }));
    let series = ws
        .store
        .insight_evaluate_on(&by_month, today())
        .await
        .unwrap();
    assert!(
        series.unit.currency.is_none(),
        "a payment's value date is not the document's, so nothing is converted"
    );
    assert_eq!(
        points(&series, "EUR"),
        vec![
            ("2026-06".to_owned(), 100_000),
            ("2026-07".to_owned(), 0),
            ("2026-08".to_owned(), 40_000),
        ],
        "Postgres groups the buckets and Rust fills the quiet month"
    );

    // The bucket key Postgres computes is the bucket key Rust computes — the
    // two spellings of "January" have to be one spelling.
    for (grain, expected) in [
        ("day", vec!["2026-06-03", "2026-06-29", "2026-08-01"]),
        ("week", vec!["2026-W23", "2026-W27", "2026-W31"]),
        ("quarter", vec!["2026-Q2", "2026-Q3"]),
        ("year", vec!["2026"]),
    ] {
        let mut value = by_month.to_value().unwrap();
        value["dimension"] = json!({ "id": "paid_on", "grain": grain });
        value["period"] = json!({ "kind": "all" });
        value["sort"] = json!({ "by": "dimension", "dir": "asc" });
        let series = ws
            .store
            .insight_evaluate_on(&spec(value), today())
            .await
            .unwrap();
        assert_eq!(
            points(&series, "EUR")
                .into_iter()
                .map(|(bucket, _)| bucket)
                .collect::<Vec<_>>(),
            expected,
            "{grain} buckets"
        );
    }

    // A method is the tenant's own word, and one it never wrote is a refusal to
    // guess rather than an English default.
    let mut by_method = by_month.to_value().unwrap();
    by_method["dimension"] = json!({ "id": "method" });
    by_method["period"] = json!({ "kind": "all" });
    by_method["viz"] = json!("pie");
    let series = ws
        .store
        .insight_evaluate_on(&spec(by_method), today())
        .await
        .unwrap();
    assert_eq!(points(&series, "EUR"), vec![("SEPA".to_owned(), 140_000)]);
    assert_eq!(
        label_of(&series, "EUR", "SEPA"),
        Some(Label::Raw {
            text: "SEPA".to_owned()
        })
    );
}

#[tokio::test]
async fn deals_answer_per_currency_and_a_win_rate_is_basis_points() {
    let store = common::test_store().await;
    let ws = workspace(&store, "deals").await;
    let (pipeline, new, won, lost) = board(&ws).await;

    let deal = |title: &str, cents: i64, currency: &str, close: Date| NewDeal {
        title: title.to_owned(),
        value_cents: cents,
        currency: currency.to_owned(),
        expected_close: Some(close),
        source: "referral".to_owned(),
        ..NewDeal::default()
    };
    let a = ws
        .store
        .create_crm_deal(
            &pipeline,
            &new,
            &deal("Acme", 500_000, "EUR", day(2026, Month::September, 30)),
        )
        .await
        .unwrap();
    let b = ws
        .store
        .create_crm_deal(
            &pipeline,
            &new,
            &deal("Beta", 300_000, "EUR", day(2026, Month::October, 31)),
        )
        .await
        .unwrap();
    let c = ws
        .store
        .create_crm_deal(
            &pipeline,
            &new,
            &deal("Cosmo", 900_000, "USD", day(2026, Month::September, 15)),
        )
        .await
        .unwrap();
    ws.store
        .move_crm_deal(
            &a,
            &StageMove {
                stage_id: won.clone(),
                position: None,
                lost_reason: None,
            },
        )
        .await
        .unwrap();
    ws.store
        .move_crm_deal(
            &b,
            &StageMove {
                stage_id: lost.clone(),
                position: None,
                lost_reason: Some("price".to_owned()),
            },
        )
        .await
        .unwrap();

    let by_currency = spec(json!({
        "schema_version": 1,
        "dataset": "crm.deals",
        "measure": { "id": "value", "agg": "sum" },
        "dimension": { "id": "expected_close", "grain": "month" },
        "period": { "kind": "range", "from": "2026-09-01", "to": "2026-10-31" },
        "sort": { "by": "dimension", "dir": "asc" },
        "viz": "line"
    }));
    let series = ws
        .store
        .insight_evaluate_on(&by_currency, today())
        .await
        .unwrap();
    assert_eq!(
        series
            .groups
            .iter()
            .map(|g| g.key.as_str())
            .collect::<Vec<_>>(),
        ["EUR", "USD"],
        "a forecast has no tax point, so euros are never added to dollars"
    );
    assert_eq!(
        points(&series, "EUR"),
        vec![
            ("2026-09".to_owned(), 500_000),
            ("2026-10".to_owned(), 300_000)
        ]
    );
    assert_eq!(
        points(&series, "USD"),
        vec![("2026-09".to_owned(), 900_000), ("2026-10".to_owned(), 0)]
    );

    // The win rate: one won, one lost, one still open — a third of nothing to
    // do with the open one.
    let win_rate = spec(json!({
        "schema_version": 1,
        "dataset": "crm.deals",
        "measure": { "id": "win_rate", "agg": "ratio" },
        "dimension": { "id": "source" },
        "period": { "kind": "all" },
        "viz": "bar"
    }));
    let series = ws
        .store
        .insight_evaluate_on(&win_rate, today())
        .await
        .unwrap();
    assert_eq!(series.unit.kind, Unit::PercentBp);
    assert!(series.unit.currency.is_none());
    assert_eq!(
        points(&series, ALL_GROUP),
        vec![("referral".to_owned(), 5_000)],
        "one won of two closed — the open deal is in neither half"
    );

    // A stage breakdown is named in the tenant's own words.
    let mut by_stage = win_rate.to_value().unwrap();
    by_stage["measure"] = json!({ "id": "count", "agg": "count" });
    by_stage["dimension"] = json!({ "id": "stage" });
    let series = ws
        .store
        .insight_evaluate_on(&spec(by_stage), today())
        .await
        .unwrap();
    let named: Vec<Label> = series.groups[0]
        .points
        .iter()
        .filter_map(|p| p.label.clone())
        .collect();
    assert!(
        named.contains(&Label::Raw {
            text: "Won".to_owned()
        }),
        "expected the board's own column headers, got {named:?}"
    );
    let _ = c;
}

#[tokio::test]
async fn a_category_tail_is_folded_into_other_and_the_total_survives() {
    let store = common::test_store().await;
    let ws = workspace(&store, "tail").await;
    let (pipeline, new, _, _) = board(&ws).await;
    for index in 0..5 {
        ws.store
            .create_crm_deal(
                &pipeline,
                &new,
                &NewDeal {
                    title: format!("Deal {index}"),
                    value_cents: (index + 1) * 100_000,
                    currency: "EUR".to_owned(),
                    source: format!("source-{index}"),
                    ..NewDeal::default()
                },
            )
            .await
            .unwrap();
    }

    let top_two = spec(json!({
        "schema_version": 1,
        "dataset": "crm.deals",
        "measure": { "id": "value", "agg": "sum" },
        "dimension": { "id": "source" },
        "period": { "kind": "all" },
        "limit": 2,
        "viz": "pie"
    }));
    let series = ws
        .store
        .insight_evaluate_on(&top_two, today())
        .await
        .unwrap();
    assert!(series.truncated, "the reader is told the tail was folded");
    assert_eq!(
        points(&series, "EUR"),
        vec![
            ("source-4".to_owned(), 500_000),
            ("source-3".to_owned(), 400_000),
            (OTHER_BUCKET.to_owned(), 100_000 + 200_000 + 300_000),
        ],
        "the two largest, then everything else — no row is ever dropped"
    );
    assert_eq!(
        points(&series, "EUR")
            .iter()
            .map(|(_, value)| value)
            .sum::<i64>(),
        1_500_000,
        "and the whole is still the whole"
    );
}

// ---- tenancy ----------------------------------------------------------------

#[tokio::test]
async fn a_spec_is_not_a_capability() {
    let store = common::test_store().await;
    let a = workspace(&store, "tenant-a").await;
    let b = workspace(&store, "tenant-b").await;

    let acme = customer(&a, "Acme BV").await;
    issued(&a, &acme, &[line(10, 10_000, 2100)]).await; // net 100 000
    let beta = customer(&b, "Beta NV").await;
    issued(&b, &beta, &[line(1, 4_200, 0)]).await; // net 4 200

    let revenue = spec(json!({
        "schema_version": 1,
        "dataset": "billing.documents",
        "measure": { "id": "net", "agg": "sum" },
        "period": { "kind": "all" },
        "viz": "number"
    }));
    // The SAME spec, through two doors. Each answers its own tenant, because
    // the tenant comes from the handle and a spec has no field that could name
    // one.
    let mine = a
        .store
        .insight_evaluate_on(&revenue, today())
        .await
        .unwrap();
    let theirs = b
        .store
        .insight_evaluate_on(&revenue, today())
        .await
        .unwrap();
    assert_eq!(
        points(&mine, "EUR"),
        vec![(TOTAL_BUCKET.to_owned(), 100_000)]
    );
    assert_eq!(
        points(&theirs, "EUR"),
        vec![(TOTAL_BUCKET.to_owned(), 4_200)]
    );

    // A filter naming the other tenant's customer is a refusal, never an empty
    // chart: a silently empty tile is how a business believes it billed nothing.
    let mut borrowed = revenue.to_value().unwrap();
    borrowed["filters"] = json!([{ "id": "customer", "op": "in", "values": [acme.as_str()] }]);
    let borrowed = spec(borrowed);
    match b.store.insight_evaluate_on(&borrowed, today()).await {
        Err(StoreError::Validation(message)) => {
            assert!(
                message.contains("not one of this workspace's records"),
                "unexpected message: {message}"
            );
        }
        other => panic!("expected a refusal, got: {other:?}"),
    }
    // …and the tenant it does belong to is answered normally.
    let filtered = a
        .store
        .insight_evaluate_on(&borrowed, today())
        .await
        .unwrap();
    assert_eq!(
        points(&filtered, "EUR"),
        vec![(TOTAL_BUCKET.to_owned(), 100_000)]
    );

    // An id that never existed anywhere answers exactly like the other
    // tenant's: existence is never leaked by the shape of the refusal.
    let mut invented = revenue.to_value().unwrap();
    invented["filters"] = json!([{ "id": "customer", "op": "in", "values": ["cus-nowhere"] }]);
    match b.store.insight_evaluate_on(&spec(invented), today()).await {
        Err(StoreError::Validation(message)) => {
            assert!(message.contains("not one of this workspace's records"));
        }
        other => panic!("expected a refusal, got: {other:?}"),
    }

    // A breakdown by customer names only this tenant's customers, whichever
    // door asked.
    let mut by_customer = revenue.to_value().unwrap();
    by_customer["dimension"] = json!({ "id": "customer" });
    by_customer["viz"] = json!("bar");
    let by_customer = spec(by_customer);
    let theirs = b
        .store
        .insight_evaluate_on(&by_customer, today())
        .await
        .unwrap();
    assert_eq!(
        points(&theirs, "EUR"),
        vec![(beta.as_str().to_owned(), 4_200)]
    );
    assert_eq!(
        label_of(&theirs, "EUR", beta.as_str()),
        Some(Label::Raw {
            text: "Beta NV".to_owned()
        })
    );
    assert!(
        !points(&theirs, "EUR")
            .iter()
            .any(|(bucket, _)| bucket == acme.as_str()),
        "no bucket of another tenant's, not even an empty one"
    );

    // The deal side of the same proof: another tenant's pipeline and another
    // tenant's user are both refusals.
    let (pipeline, new, _, _) = board(&a).await;
    a.store
        .create_crm_deal(
            &pipeline,
            &new,
            &NewDeal {
                title: "Acme expansion".to_owned(),
                value_cents: 750_000,
                currency: "EUR".to_owned(),
                ..NewDeal::default()
            },
        )
        .await
        .unwrap();
    let deals = json!({
        "schema_version": 1,
        "dataset": "crm.deals",
        "measure": { "id": "value", "agg": "sum" },
        "period": { "kind": "all" },
        "viz": "number"
    });
    let mine = a
        .store
        .insight_evaluate_on(&spec(deals.clone()), today())
        .await
        .unwrap();
    assert_eq!(
        points(&mine, "EUR"),
        vec![(TOTAL_BUCKET.to_owned(), 750_000)]
    );
    let theirs = b
        .store
        .insight_evaluate_on(&spec(deals.clone()), today())
        .await
        .unwrap();
    assert!(
        theirs.groups.iter().all(|g| g.points.is_empty()),
        "tenant B has no deals, and A's are not in the answer"
    );

    for (field, value) in [
        ("pipeline", pipeline.as_str().to_owned()),
        ("owner", a.user.as_str().to_owned()),
    ] {
        let mut borrowed = deals.clone();
        borrowed["filters"] = json!([{ "id": field, "op": "in", "values": [value] }]);
        match b.store.insight_evaluate_on(&spec(borrowed), today()).await {
            Err(StoreError::Validation(message)) => {
                assert!(
                    message.contains("not one of this workspace's records"),
                    "{field}"
                );
            }
            other => panic!("expected a refusal for {field}, got: {other:?}"),
        }
    }
}

/// Every chart the catalog can express, compiled and run — against a tenant
/// that owns nothing.
///
/// This is the structural half of the tenancy story, and it **grows with the
/// catalog by construction**: a dataset, measure or dimension added without its
/// tenant predicate makes some combination here read the seeded tenant's rows,
/// and every figure below is asserted to be nothing. It is also a compile-and-run
/// check of the whole matrix, so a fragment that does not parse cannot ship
/// either.
#[tokio::test]
async fn the_whole_catalog_compiles_and_no_combination_of_it_escapes_its_tenant() {
    let store = common::test_store().await;
    let rich = workspace(&store, "catalog-rich").await;
    let poor = workspace(&store, "catalog-poor").await;

    // A tenant with something of every kind, so an escaping query would have
    // something to find.
    let acme = customer(&rich, "Acme BV").await;
    let invoice = issued(&rich, &acme, &[line(10, 10_000, 2100)]).await;
    rich.store
        .record_billing_payment(
            &invoice,
            &NewPayment {
                paid_on: Some(day(2026, Month::July, 1)),
                amount_cents: 21_000,
                method: "SEPA".to_owned(),
                reference: String::new(),
            },
        )
        .await
        .unwrap();
    let (pipeline, new, won, _) = board(&rich).await;
    let deal = rich
        .store
        .create_crm_deal(
            &pipeline,
            &new,
            &NewDeal {
                title: "Acme".to_owned(),
                value_cents: 750_000,
                currency: "EUR".to_owned(),
                expected_close: Some(day(2026, Month::September, 1)),
                source: "referral".to_owned(),
                ..NewDeal::default()
            },
        )
        .await
        .unwrap();
    rich.store
        .move_crm_deal(
            &deal,
            &StageMove {
                stage_id: won,
                position: None,
                lost_reason: None,
            },
        )
        .await
        .unwrap();

    let mut charts = 0;
    for dataset in alo_store::insight_catalog::DATASETS {
        let entry = alo_store::insight_catalog::dataset(*dataset);
        for measure in entry.measures {
            for aggregate in measure.aggregates {
                // Every breakdown the measure allows, plus no breakdown at all.
                let breakdowns: Vec<Option<Value>> = std::iter::once(None)
                    .chain(measure.dimensions.iter().map(|dimension| {
                        let grain = entry.dimension(*dimension).and_then(|d| match d.kind {
                            alo_store::DimensionKind::Time(grains) => grains.first().copied(),
                            alo_store::DimensionKind::Category => None,
                        });
                        Some(match grain {
                            Some(grain) => json!({ "id": dimension, "grain": grain }),
                            None => json!({ "id": dimension }),
                        })
                    }))
                    .collect();
                for breakdown in breakdowns {
                    let viz = match &breakdown {
                        None => "number",
                        Some(d) if d.get("grain").is_some() => "line",
                        Some(_) => "bar",
                    };
                    let mut value = json!({
                        "schema_version": 1,
                        "dataset": dataset,
                        "measure": { "id": measure.measure, "agg": aggregate },
                        "period": { "kind": "all" },
                        "viz": viz,
                    });
                    if let Some(breakdown) = breakdown {
                        value["dimension"] = breakdown;
                    }
                    let chart = spec(value.clone());
                    charts += 1;

                    // It answers for the tenant that has the rows…
                    rich.store
                        .insight_evaluate_on(&chart, today())
                        .await
                        .unwrap_or_else(|e| panic!("{value} failed for its own tenant: {e:?}"));
                    // …and nothing at all for the tenant that has none.
                    let empty = poor
                        .store
                        .insight_evaluate_on(&chart, today())
                        .await
                        .unwrap_or_else(|e| panic!("{value} failed: {e:?}"));
                    for group in &empty.groups {
                        for point in &group.points {
                            assert_eq!(
                                point.value, 0,
                                "{value} leaked {} into another tenant's chart",
                                point.value
                            );
                        }
                    }
                    assert!(
                        empty.notes.is_empty(),
                        "{value} carried another tenant's documents into a note"
                    );
                }
            }
        }
    }
    assert!(
        charts >= 40,
        "the catalog should express more than {charts} charts"
    );
}

#[tokio::test]
async fn a_document_that_cannot_be_restated_is_counted_apart_never_guessed_at() {
    let store = common::test_store().await;
    let pool = PgPool::connect(&common::database_url()).await.unwrap();
    let ws = workspace(&store, "unconverted").await;
    let acme = customer(&ws, "Acme BV").await;
    let plain = issued(&ws, &acme, &[line(1, 10_000, 2100)]).await;
    let stale = issued(&ws, &acme, &[line(1, 50_000, 2100)]).await;
    // A document raised before B1.21 carries no snapshot at all. There is no
    // API that strips one — that is the point — so the fixture writes the
    // absence directly.
    sqlx::query(
        "UPDATE billing_invoices SET fx_base_currency = NULL, fx_rate_micro = NULL, \
         fx_rate_date = NULL WHERE tenant_id = $1 AND id = $2",
    )
    .bind(ws.tenant.as_str())
    .bind(stale.as_str())
    .execute(&pool)
    .await
    .unwrap();

    let revenue = spec(json!({
        "schema_version": 1,
        "dataset": "billing.documents",
        "measure": { "id": "net", "agg": "sum" },
        "period": { "kind": "all" },
        "viz": "number"
    }));
    let series = ws
        .store
        .insight_evaluate_on(&revenue, today())
        .await
        .unwrap();
    assert_eq!(
        points(&series, "EUR"),
        vec![(TOTAL_BUCKET.to_owned(), 10_000)],
        "the figure holds only what could be restated"
    );
    assert_eq!(
        series.notes,
        vec![alo_store::Note {
            code: "unconverted_documents",
            count: 1
        }],
        "and the tile says how much of the period is missing from it"
    );

    // Counting them needs no rate, so both documents are counted.
    let mut counted = revenue.to_value().unwrap();
    counted["measure"] = json!({ "id": "count", "agg": "count" });
    let counted = ws
        .store
        .insight_evaluate_on(&spec(counted), today())
        .await
        .unwrap();
    assert_eq!(
        points(&counted, ALL_GROUP),
        vec![(TOTAL_BUCKET.to_owned(), 2)]
    );
    assert!(counted.notes.is_empty());
    let _ = plain;

    pool.close().await;
}

#[tokio::test]
async fn a_deleted_tenants_rows_are_in_nobodys_chart() {
    let store = common::test_store().await;
    let a = workspace(&store, "gone").await;
    let acme = customer(&a, "Acme BV").await;
    issued(&a, &acme, &[line(1, 10_000, 2100)]).await;
    store.delete_tenant(&a.tenant).await.unwrap();

    let b = workspace(&store, "left").await;
    let revenue = spec(json!({
        "schema_version": 1,
        "dataset": "billing.documents",
        "measure": { "id": "net", "agg": "sum" },
        "period": { "kind": "all" },
        "viz": "number"
    }));
    let series = b
        .store
        .insight_evaluate_on(&revenue, today())
        .await
        .unwrap();
    assert_eq!(
        points(&series, "EUR"),
        vec![(TOTAL_BUCKET.to_owned(), 0)],
        "nothing, in euro — which is an answer, not a question"
    );
}
