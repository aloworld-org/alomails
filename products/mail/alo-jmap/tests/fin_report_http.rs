//! The `/finance/reports/pl` surface (B4.11a), driven through the real router
//! over a real Postgres.
//!
//! `alo-store`'s own golden suite proves the arithmetic of a year; what this
//! one is for is the **edge**: that a P&L is admin-only and says so; that the
//! two representations of one read — the JSON a screen shows and the CSV an
//! accountant opens — carry the same figures; that a period is always stated
//! and never guessed; that the file is served as an attachment no cache keeps;
//! and that another tenant's result appears in neither, on either route.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::sync::Arc;

use alo_store::{
    AccountStore, CHART, ChartName, ChartSeed, EntryKind, FxSnapshot, NewEntry, NewPosting,
};
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::Value;
use time::{Date, Month};
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

fn on(year: i32, month: Month, day: u8) -> Date {
    Date::from_calendar_date(year, month, day).unwrap()
}

// ---- the books a tenant arrives with -----------------------------------------

/// The chart, named per tenant so a leak reads as the wrong tag rather than as
/// a plausible number.
async fn chart_for(acc: &AccountStore, tag: &str) {
    acc.fin_accounts_or_seed(
        &ChartSeed {
            names: CHART
                .iter()
                .map(|account| ChartName {
                    code: account.code.to_owned(),
                    name: format!("{tag} {}", account.code),
                })
                .collect(),
        },
        false,
    )
    .await
    .unwrap();
}

/// Posts one balanced entry by account code, in the accounting currency.
async fn post(acc: &AccountStore, date: Date, kind: EntryKind, memo: &str, lines: &[(&str, i64)]) {
    let chart = acc.fin_accounts(false).await.unwrap();
    let postings = lines
        .iter()
        .map(|&(code, cents)| {
            let id = chart
                .iter()
                .find(|account| account.code == code)
                .unwrap_or_else(|| panic!("the seeded chart holds {code}"))
                .id
                .clone();
            NewPosting::new(id, cents, cents)
        })
        .collect();
    acc.post_fin_entry(&NewEntry {
        entry_date: date,
        kind,
        source: None,
        memo: memo.to_owned(),
        reverses_entry_id: None,
        attachment_node_id: None,
        currency: "EUR".to_owned(),
        fx: FxSnapshot::identity("EUR", date),
        postings,
    })
    .await
    .unwrap_or_else(|e| panic!("{memo} should post: {e:?}"));
}

/// A year of books: €1 000.00 of revenue and €150.00 of hosting in 2026, half
/// of each in 2025 (the comparative), and a payment that moves the balance
/// sheet and must leave the result alone.
async fn a_year(acc: &AccountStore, tag: &str) {
    chart_for(acc, tag).await;
    post(
        acc,
        on(2025, Month::June, 30),
        EntryKind::Invoice,
        "INV-2025-00001",
        &[("1100", 50_000), ("4000", -50_000)],
    )
    .await;
    post(
        acc,
        on(2025, Month::July, 1),
        EntryKind::Bill,
        "hosting 2025",
        &[("6000", 7_500), ("2000", -7_500)],
    )
    .await;
    post(
        acc,
        on(2026, Month::March, 3),
        EntryKind::Invoice,
        "INV-2026-00001",
        &[("1100", 121_000), ("4000", -100_000), ("2100", -21_000)],
    )
    .await;
    post(
        acc,
        on(2026, Month::April, 4),
        EntryKind::Payment,
        "INV-2026-00001 settled",
        &[("1000", 121_000), ("1100", -121_000)],
    )
    .await;
    post(
        acc,
        on(2026, Month::November, 30),
        EntryKind::Bill,
        "hosting 2026",
        &[("6000", 15_000), ("2000", -15_000)],
    )
    .await;
}

/// An admin with a year of books behind them.
async fn books(tag: &str) -> Harness {
    let h = harness(tag).await;
    h.ts.set_admin(&h.user, true).await.unwrap();
    a_year(&h.acc, tag).await;
    h
}

const YEAR: &str = "/finance/reports/pl?from=2026-01-01&to=2026-12-31";

// ---- the tests ---------------------------------------------------------------

#[tokio::test]
async fn a_year_of_books_reads_the_same_on_both_representations() {
    let h = books("plyear").await;

    let (status, body) = get(&h.app, &h.token, YEAR).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let report = &body["report"];
    assert_eq!(report["from"], "2026-01-01");
    assert_eq!(report["to"], "2026-12-31");
    assert_eq!(report["previousFrom"], "2025-01-01");
    assert_eq!(report["previousTo"], "2025-12-31");
    assert_eq!(report["currency"], "EUR");
    assert_eq!(report["incomeCents"], 100_000);
    assert_eq!(report["expenseCents"], 15_000);
    assert_eq!(report["resultCents"], 85_000);
    assert_eq!(report["previousIncomeCents"], 50_000);
    assert_eq!(report["previousExpenseCents"], 7_500);
    assert_eq!(report["previousResultCents"], 42_500);
    assert_eq!(report["income"][0]["code"], "4000");
    assert_eq!(report["income"][0]["name"], "plyear 4000");
    assert_eq!(report["income"][0]["previousCents"], 50_000);
    assert_eq!(
        report["income"].as_array().map(Vec::len),
        Some(1),
        "the receivable and the VAT are not a result: {report}"
    );

    // The same read as a file: the same figures, and the headers that make it
    // a download.
    let (status, headers, csv) = get_file(
        &h.app,
        &h.token,
        "/finance/reports/pl.csv?from=2026-01-01&to=2026-12-31",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        header(&headers, "content-type"),
        Some("text/csv; charset=utf-8")
    );
    assert_eq!(
        header(&headers, "content-disposition"),
        Some("attachment; filename=\"profit-and-loss-2026-01-01-to-2026-12-31.csv\"")
    );
    assert_eq!(header(&headers, "cache-control"), Some("no-store"));
    assert_eq!(header(&headers, "x-content-type-options"), Some("nosniff"));

    let lines = rows(&csv);
    assert_eq!(
        lines[0],
        "row,periodFrom,periodTo,previousFrom,previousTo,currency,accountCode,accountName,\
         amount,previousAmount"
    );
    assert!(
        lines.contains(
            &"income,2026-01-01,2026-12-31,2025-01-01,2025-12-31,EUR,4000,plyear 4000,\
              1000.00,500.00"
        ),
        "{csv}"
    );
    assert!(
        lines
            .iter()
            .any(|line| line.starts_with("result,") && line.ends_with(",850.00,425.00")),
        "{csv}"
    );
}

#[tokio::test]
async fn a_profit_and_loss_is_not_read_by_a_member_who_is_not_an_admin() {
    let h = harness("plclerk").await;
    a_year(&h.acc, "plclerk").await;

    // The harness user is an ordinary member: both doors are shut.
    for uri in [
        YEAR,
        "/finance/reports/pl.csv?from=2026-01-01&to=2026-12-31",
    ] {
        let (status, _) = get(&h.app, &h.token, uri).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{uri}");
    }
    // …and the periods list, which any member reads, still answers — so the
    // refusal above is this route's rule and not a broken token.
    let (status, _) = get(&h.app, &h.token, "/finance/periods").await;
    assert_eq!(status, StatusCode::OK);

    // Made an admin, the same person reads the same books.
    h.ts.set_admin(&h.user, true).await.unwrap();
    let (status, body) = get(&h.app, &h.token, YEAR).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["report"]["resultCents"], 85_000);
}

#[tokio::test]
async fn no_token_reads_nothing_on_either_route() {
    let h = harness("plnoauth").await;
    for uri in [
        YEAR,
        "/finance/reports/pl.csv?from=2026-01-01&to=2026-12-31",
    ] {
        let (status, _) = send(&h.app, request(uri, None)).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{uri}");
    }
}

#[tokio::test]
async fn the_period_is_always_stated_and_never_guessed_at() {
    let h = books("plperiod").await;

    for (uri, expected) in [
        ("/finance/reports/pl", "from is required"),
        ("/finance/reports/pl?to=2026-12-31", "from is required"),
        ("/finance/reports/pl?from=2026-01-01", "to is required"),
        (
            "/finance/reports/pl?from=01/01/2026&to=2026-12-31",
            "from must be a date",
        ),
        (
            "/finance/reports/pl?from=2026-01-01&to=yesterday",
            "to must be a date",
        ),
    ] {
        let (status, body) = get(&h.app, &h.token, uri).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{uri}");
        let detail = body["detail"].as_str().unwrap_or_default();
        assert!(detail.starts_with(expected), "{uri} answered {detail}");
    }

    // A period that ends before it starts is the store's refusal, and it
    // reaches the caller as the same 422 rather than as an empty report.
    let (status, body) = get(
        &h.app,
        &h.token,
        "/finance/reports/pl?from=2026-12-31&to=2026-01-01",
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        body["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("before its start"),
        "{body}"
    );

    // The file route is refused the same way, so a script that saves a report
    // never saves a wrong one.
    let (status, ..) = get_file(&h.app, &h.token, "/finance/reports/pl.csv?from=2026-01-01").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn a_period_nothing_was_booked_in_answers_zeroes_rather_than_nothing() {
    let h = books("plquiet").await;
    let (status, body) = get(
        &h.app,
        &h.token,
        "/finance/reports/pl?from=2020-01-01&to=2020-12-31",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["report"]["income"], Value::Array(Vec::new()));
    assert_eq!(body["report"]["resultCents"], 0);
    assert_eq!(
        body["report"]["currency"], "EUR",
        "a file that does not say what the nothing is nothing in is a question"
    );
}

#[tokio::test]
async fn one_tenants_result_is_no_part_of_anothers_on_either_route() {
    let ours = books("plours").await;
    // A second tenant on the same store, with ten times the books, on the same
    // days and the same account codes.
    let theirs = harness_on(Arc::clone(&ours.store), "pltheirs").await;
    theirs.ts.set_admin(&theirs.user, true).await.unwrap();
    chart_for(&theirs.acc, "pltheirs").await;
    post(
        &theirs.acc,
        on(2026, Month::March, 3),
        EntryKind::Invoice,
        "THEIRS-1",
        &[
            ("1100", 1_210_000),
            ("4000", -1_000_000),
            ("2100", -210_000),
        ],
    )
    .await;

    let (status, body) = get(&ours.app, &ours.token, YEAR).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["report"]["incomeCents"], 100_000, "unmoved: {body}");
    assert_eq!(body["report"]["income"][0]["name"], "plours 4000");

    let (status, _, csv) = get_file(
        &ours.app,
        &ours.token,
        "/finance/reports/pl.csv?from=2026-01-01&to=2026-12-31",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(!csv.contains("pltheirs"), "{csv}");
    assert!(!csv.contains("10000.00"), "{csv}");

    // And theirs reads only theirs.
    let (status, body) = get(&theirs.app, &theirs.token, YEAR).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["report"]["incomeCents"], 1_000_000);
    assert_eq!(body["report"]["expenseCents"], 0);
}
