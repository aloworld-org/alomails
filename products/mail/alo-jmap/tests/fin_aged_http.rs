//! The `/finance/reports/aged` surface (B4.11c), driven through the real router
//! over a real Postgres.
//!
//! `alo-store`'s own golden suite proves the ageing of a set of documents; what
//! this one is for is the **edge**: that an aged listing is admin-only and says
//! so; that the two representations of one read — the JSON a screen shows and
//! the CSV a bookkeeper opens — carry the same figures; that the day *and* the
//! side are always stated and never guessed; that the file is served as an
//! attachment no cache keeps; and that another tenant's debts appear in neither,
//! on either route and on either side.
//!
//! The documents are raised through the store's own doors and their **terms**
//! are what put them in different bands: four invoices on 14, 60, 95 and 120
//! days, read a hundred days out, stand one in each band. No date is edited
//! behind the API's back.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::sync::Arc;

use alo_store::{
    AccountStore, BillStatus, BillTotals, BillingCustomerId, EInvoiceSyntax, NewBill, NewCustomer,
    NewInvoice, NewLine, NewPayment, Supplier,
};
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::Value;
use sqlx::postgres::PgPoolOptions;
use time::{Date, Duration};
use tower::ServiceExt;

use common::{Harness, harness, harness_on, send};

// ---- request helpers ---------------------------------------------------------

fn request(uri: &str, token: Option<&str>) -> Request<Body> {
    let mut req = Request::builder().method("GET").uri(uri);
    if let Some(token) = token {
        req = req.header("authorization", format!("Bearer {token}"));
    }
    req.body(Body::empty()).unwrap()
}

async fn get(app: &Router, token: &str, uri: &str) -> (StatusCode, Value) {
    send(app, request(uri, Some(token))).await
}

/// A `GET` whose body is a file: the status, the headers that matter, and the
/// text.
async fn get_file(
    app: &Router,
    token: &str,
    uri: &str,
) -> (StatusCode, Vec<(String, String)>, String) {
    let resp = app
        .clone()
        .oneshot(request(uri, Some(token)))
        .await
        .unwrap();
    let status = resp.status();
    let headers = resp
        .headers()
        .iter()
        .map(|(name, value)| {
            (
                name.as_str().to_owned(),
                value.to_str().unwrap_or_default().to_owned(),
            )
        })
        .collect();
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    (status, headers, String::from_utf8(bytes.to_vec()).unwrap())
}

fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}

/// The rows of a CSV body, without the trailing blank.
fn rows(body: &str) -> Vec<&str> {
    body.split("\r\n").filter(|line| !line.is_empty()).collect()
}

/// The database's own today — the clock the documents below are stamped from,
/// so the suite reads the same on any day of any year.
async fn today() -> Date {
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&common::database_url())
        .await
        .unwrap();
    sqlx::query_scalar("SELECT CURRENT_DATE")
        .fetch_one(&pool)
        .await
        .unwrap()
}

fn iso(date: Date) -> String {
    format!(
        "{:04}-{:02}-{:02}",
        date.year(),
        u8::from(date.month()),
        date.day()
    )
}

// ---- the documents a tenant arrives with -------------------------------------

async fn customer(acc: &AccountStore, name: &str) -> BillingCustomerId {
    acc.create_billing_customer(&NewCustomer {
        name: name.to_owned(),
        country: "NL".to_owned(),
        currency: "EUR".to_owned(),
        payment_terms_days: 14,
        ..Default::default()
    })
    .await
    .unwrap()
}

/// `hours` hours of consulting at €100.00 + 21 % VAT: one hour is 12 100 cents.
fn consulting(hours: i64) -> NewLine {
    NewLine {
        description: "Consulting".to_owned(),
        unit: "hour".to_owned(),
        qty_milli: hours * 1_000,
        unit_price_cents: 10_000,
        vat_rate_bp: 2100,
    }
}

/// An invoice issued today on `terms` days — which is what decides the band it
/// stands in when the report is asked for a hundred days out.
async fn issued(
    acc: &AccountStore,
    customer: &BillingCustomerId,
    hours: i64,
    terms: i32,
) -> alo_store::BillingInvoiceId {
    let id = acc
        .create_billing_invoice(&NewInvoice {
            payment_terms_days: Some(terms),
            ..NewInvoice::for_customer(customer.clone())
        })
        .await
        .unwrap();
    acc.set_billing_invoice_lines(&id, &[consulting(hours)])
        .await
        .unwrap();
    acc.issue_billing_invoice(&id).await.unwrap();
    id
}

/// Four invoices for two customers, one of them part-paid: read a hundred days
/// out they stand one in each band, which is the point of the terms.
async fn debts(acc: &AccountStore) {
    let anchor = customer(acc, "Anchor BV").await;
    let zephyr = customer(acc, "Zephyr NV").await;
    // 86 days late in a hundred days' time.
    issued(acc, &anchor, 1, 14).await;
    // Still not due then.
    issued(acc, &anchor, 2, 120).await;
    // 40 days late, and a thousand euro of it has arrived.
    let part_paid = issued(acc, &zephyr, 10, 60).await;
    acc.record_billing_payment(
        &part_paid,
        &NewPayment {
            paid_on: None,
            amount_cents: 100_000,
            method: "bank transfer".to_owned(),
            reference: "NL02RABO0123456789".to_owned(),
        },
    )
    .await
    .unwrap();
    // Five days late.
    issued(acc, &zephyr, 1, 95).await;
}

/// An approved bill from `supplier`, payable €1 210.00.
async fn approved_bill(acc: &AccountStore, supplier: &str, number: &str, issue: Date, due: Date) {
    let id = acc
        .create_billing_bill(&NewBill {
            // Stated because the column is `NOT NULL`; nothing was imported.
            source_syntax: Some(EInvoiceSyntax::Cii),
            source_sha256: "cd".repeat(32),
            supplier: Supplier {
                name: supplier.to_owned(),
                ..Default::default()
            },
            number: number.to_owned(),
            issue_date: Some(issue),
            due_date: Some(due),
            currency: "EUR".to_owned(),
            totals: BillTotals {
                line_total_cents: 100_000,
                tax_exclusive_cents: 100_000,
                tax_total_cents: 21_000,
                tax_inclusive_cents: 121_000,
                payable_cents: 121_000,
                ..Default::default()
            },
            lines: vec![consulting(10)],
            ..Default::default()
        })
        .await
        .unwrap();
    acc.decide_billing_bill(&id, BillStatus::Approved)
        .await
        .unwrap();
}

/// An admin with four open invoices behind them, and the day the report is
/// asked for.
async fn owing(tag: &str) -> (Harness, Date) {
    let h = harness(tag).await;
    h.ts.set_admin(&h.user, true).await.unwrap();
    debts(&h.acc).await;
    (h, today().await + Duration::days(100))
}

fn receivable(on: Date) -> String {
    format!("/finance/reports/aged?on={}&side=receivable", iso(on))
}

fn receivable_csv(on: Date) -> String {
    format!("/finance/reports/aged.csv?on={}&side=receivable", iso(on))
}

// ---- the tests ---------------------------------------------------------------

#[tokio::test]
async fn an_ageing_reads_the_same_on_both_representations() {
    let (h, on) = owing("agedsame").await;

    let (status, body) = get(&h.app, &h.token, &receivable(on)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let report = &body["report"];
    assert_eq!(report["on"], iso(on));
    assert_eq!(report["side"], "receivable");
    assert_eq!(report["currency"], "EUR");
    assert_eq!(report["documentCount"], 4);
    assert_eq!(report["unconvertedCount"], 0);
    assert_eq!(
        report["buckets"],
        serde_json::json!({
            "currentCents": 24_200,
            "d1_30Cents": 12_100,
            "d31_60Cents": 21_000,
            "d61_90Cents": 12_100,
            "d90_plusCents": 0,
            "totalCents": 69_400,
        })
    );
    assert_eq!(report["parties"][0]["name"], "Anchor BV");
    assert_eq!(report["parties"][1]["name"], "Zephyr NV");
    assert_eq!(report["parties"][0]["buckets"]["totalCents"], 36_300);
    assert_eq!(report["parties"][1]["buckets"]["totalCents"], 33_100);

    // The part-paid document carries what is left, not what it was worth.
    let part_paid = &report["parties"][1]["documents"][0];
    assert_eq!(part_paid["openCents"], 21_000);
    assert_eq!(part_paid["baseOpenCents"], 21_000);
    assert_eq!(part_paid["daysOverdue"], 40);
    assert_eq!(part_paid["bucket"], "d31_60");
    assert_eq!(part_paid["currency"], "EUR");
    assert_eq!(part_paid["creditNote"], false);
    assert!(
        part_paid["number"]
            .as_str()
            .unwrap_or_default()
            .starts_with("INV-"),
        "{part_paid}"
    );

    // The same read as a file: the same figures, and the headers that make it a
    // download.
    let (status, headers, csv) = get_file(&h.app, &h.token, &receivable_csv(on)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        header(&headers, "content-type"),
        Some("text/csv; charset=utf-8")
    );
    assert_eq!(
        header(&headers, "content-disposition"),
        Some(format!("attachment; filename=\"aged-receivable-{}.csv\"", iso(on)).as_str())
    );
    assert_eq!(header(&headers, "cache-control"), Some("no-store"));
    assert_eq!(header(&headers, "x-content-type-options"), Some("nosniff"));

    let lines = rows(&csv);
    assert_eq!(
        lines[0],
        "row,on,side,currency,party,documentNumber,documentCurrency,documentAmount,\
         dueDate,daysOverdue,current,d1_30,d31_60,d61_90,d90_plus,total,unconverted"
    );
    let day = iso(on);
    assert!(
        lines.contains(
            &format!(
                "party,{day},receivable,EUR,Anchor BV,,,,,,242.00,0.00,0.00,121.00,0.00,363.00,0"
            )
            .as_str()
        ),
        "{csv}"
    );
    assert!(
        lines.contains(
            &format!(
                "party,{day},receivable,EUR,Zephyr NV,,,,,,0.00,121.00,210.00,0.00,0.00,331.00,0"
            )
            .as_str()
        ),
        "{csv}"
    );
    assert!(
        lines.contains(
            &format!("total,{day},receivable,EUR,,,,,,,242.00,121.00,210.00,121.00,0.00,694.00,0")
                .as_str()
        ),
        "the file states the same totals as the screen: {csv}"
    );
    assert_eq!(
        lines.len(),
        8,
        "four documents, two parties, one total: {csv}"
    );
}

#[tokio::test]
async fn an_ageing_is_not_read_by_a_member_who_is_not_an_admin() {
    let h = harness("agedclerk").await;
    debts(&h.acc).await;
    let on = today().await + Duration::days(100);

    for uri in [receivable(on), receivable_csv(on)] {
        let (status, _) = get(&h.app, &h.token, &uri).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{uri}");
    }
    // …and the periods list, which any member reads, still answers — so the
    // refusal above is this route's rule and not a broken token.
    let (status, _) = get(&h.app, &h.token, "/finance/periods").await;
    assert_eq!(status, StatusCode::OK);

    h.ts.set_admin(&h.user, true).await.unwrap();
    let (status, body) = get(&h.app, &h.token, &receivable(on)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["report"]["buckets"]["totalCents"], 69_400);
}

#[tokio::test]
async fn no_token_reads_nothing_on_either_route() {
    let h = harness("agednoauth").await;
    let on = today().await;
    for uri in [receivable(on), receivable_csv(on)] {
        let (status, _) = send(&h.app, request(&uri, None)).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{uri}");
    }
}

#[tokio::test]
async fn the_day_and_the_side_are_always_stated_and_never_guessed_at() {
    let (h, _) = owing("ageddate").await;

    for (uri, expected) in [
        ("/finance/reports/aged", "on is required"),
        ("/finance/reports/aged?side=receivable", "on is required"),
        ("/finance/reports/aged?on=&side=payable", "on is required"),
        (
            "/finance/reports/aged?on=today&side=payable",
            "on must be a date",
        ),
        (
            "/finance/reports/aged?on=31/12/2026&side=payable",
            "on must be a date",
        ),
        // The day is read first: a caller with both wrong is told about the day.
        (
            "/finance/reports/aged?on=nope&side=nope",
            "on must be a date",
        ),
        ("/finance/reports/aged?on=2026-12-31", "side is required"),
        (
            "/finance/reports/aged?on=2026-12-31&side=",
            "side is required",
        ),
        (
            "/finance/reports/aged?on=2026-12-31&side=debtors",
            "side must be 'receivable' or 'payable'",
        ),
        (
            // The words are the store's, and they are lower case.
            "/finance/reports/aged?on=2026-12-31&side=Receivable",
            "side must be 'receivable' or 'payable'",
        ),
    ] {
        let (status, body) = get(&h.app, &h.token, uri).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{uri}");
        let detail = body["detail"].as_str().unwrap_or_default();
        assert!(detail.starts_with(expected), "{uri} answered {detail}");
    }

    // The file route is refused the same way, so a script that saves a listing
    // never saves a wrong one.
    let (status, ..) = get_file(&h.app, &h.token, "/finance/reports/aged.csv").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn the_payable_side_is_its_own_report_over_its_own_documents() {
    let h = harness("agedpay").await;
    h.ts.set_admin(&h.user, true).await.unwrap();
    let on = today().await;
    debts(&h.acc).await;
    approved_bill(
        &h.acc,
        "Lieferant GmbH",
        "R-1",
        on - Duration::days(70),
        on - Duration::days(40),
    )
    .await;

    let (status, body) = get(
        &h.app,
        &h.token,
        &format!("/finance/reports/aged?on={}&side=payable", iso(on)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let report = &body["report"];
    assert_eq!(report["side"], "payable");
    assert_eq!(report["documentCount"], 1, "the invoices are not payables");
    assert_eq!(report["parties"][0]["name"], "Lieferant GmbH");
    assert_eq!(report["buckets"]["d31_60Cents"], 121_000);
    assert_eq!(report["buckets"]["totalCents"], 121_000);
    assert_eq!(report["parties"][0]["documents"][0]["number"], "R-1");
    assert_eq!(report["parties"][0]["documents"][0]["daysOverdue"], 40);

    // Today the invoices are not due yet, so the other side is all current —
    // two different reports over two different tables.
    let (status, body) = get(&h.app, &h.token, &receivable(on)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["report"]["buckets"]["currentCents"], 69_400);
    assert_eq!(body["report"]["buckets"]["d31_60Cents"], 0);

    // And the file says which side it is, in its name and in every row.
    let (status, headers, csv) = get_file(
        &h.app,
        &h.token,
        &format!("/finance/reports/aged.csv?on={}&side=payable", iso(on)),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        header(&headers, "content-disposition"),
        Some(format!("attachment; filename=\"aged-payable-{}.csv\"", iso(on)).as_str())
    );
    assert!(csv.contains("Lieferant GmbH"), "{csv}");
    assert!(!csv.contains("Anchor BV"), "{csv}");
    for line in rows(&csv).iter().skip(1) {
        assert!(line.contains(",payable,"), "{line}");
    }
}

#[tokio::test]
async fn one_tenants_debts_are_no_part_of_anothers_on_either_route() {
    let (ours, on) = owing("agedours").await;
    // A second tenant on the same store, with ten times the debts.
    let theirs = harness_on(Arc::clone(&ours.store), "agedtheirs").await;
    theirs.ts.set_admin(&theirs.user, true).await.unwrap();
    let big = customer(&theirs.acc, "Theirs BV").await;
    issued(&theirs.acc, &big, 100, 14).await;
    approved_bill(
        &theirs.acc,
        "Their Supplier",
        "T-1",
        on - Duration::days(120),
        on - Duration::days(100),
    )
    .await;

    let (status, body) = get(&ours.app, &ours.token, &receivable(on)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["report"]["buckets"]["totalCents"], 69_400, "unmoved");
    assert_eq!(body["report"]["parties"][0]["name"], "Anchor BV");

    let (status, _, csv) = get_file(&ours.app, &ours.token, &receivable_csv(on)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!csv.contains("Theirs BV"), "{csv}");
    assert!(!csv.contains("12100.00"), "{csv}");

    // Ours owes nobody anything, however much they do.
    let (status, body) = get(
        &ours.app,
        &ours.token,
        &format!("/finance/reports/aged?on={}&side=payable", iso(on)),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["report"]["documentCount"], 0);
    assert_eq!(body["report"]["buckets"]["totalCents"], 0);

    // And theirs reads only theirs, on both sides.
    let (status, body) = get(&theirs.app, &theirs.token, &receivable(on)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["report"]["buckets"]["totalCents"], 1_210_000);
    assert_eq!(body["report"]["parties"][0]["name"], "Theirs BV");
    let (status, body) = get(
        &theirs.app,
        &theirs.token,
        &format!("/finance/reports/aged?on={}&side=payable", iso(on)),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["report"]["buckets"]["totalCents"], 121_000);
}
