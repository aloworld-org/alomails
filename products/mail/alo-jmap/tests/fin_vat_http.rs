//! The `/finance/reports/vat` surface (B4.11d), driven through the real router
//! over a real Postgres.
//!
//! `alo-store`'s own golden suite proves the arithmetic of a period and its
//! agreement with the billing summary; what this one is for is the **edge**:
//! that a VAT return is admin-only and says so; that the two representations of
//! one read — the JSON a screen shows and the CSV an accountant opens — carry
//! the same figures; that the period is always stated and never guessed; that
//! the file is served as an attachment no cache keeps; and that another
//! tenant's tax appears in neither.
//!
//! The books are written through the journal's own door, in the shape the
//! posting rules write them: revenue and its tax both carrying the rate, and a
//! bill-shaped purchase carrying the recoverable side.

#![allow(clippy::unwrap_used, clippy::expect_used)]

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

use crate::common::{Harness, harness, harness_on, send};

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

/// One posting by account code, with the VAT rate a return groups by.
async fn posting(acc: &AccountStore, code: &str, cents: i64, rate_bp: Option<i32>) -> NewPosting {
    let id = acc
        .fin_accounts(false)
        .await
        .unwrap()
        .into_iter()
        .find(|account| account.code == code)
        .unwrap_or_else(|| panic!("the seeded chart holds {code}"))
        .id;
    NewPosting {
        vat_rate_bp: rate_bp,
        ..NewPosting::new(id, cents, cents)
    }
}

/// Posts one balanced entry in the accounting currency.
async fn post(
    acc: &AccountStore,
    date: Date,
    kind: EntryKind,
    memo: &str,
    lines: &[(&str, i64, Option<i32>)],
) {
    let mut postings = Vec::new();
    for &(code, cents, rate_bp) in lines {
        postings.push(posting(acc, code, cents, rate_bp).await);
    }
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

/// A quarter's books, scaled by `times`: €1 000.00 billed at 21 % and €250.00
/// at 9 %, and €400.00 bought at 21 %. A payment is in as well, because money
/// arriving moves the balance sheet and must move no figure on a return.
async fn a_quarter(acc: &AccountStore, tag: &str, times: i64) {
    chart_for(acc, tag).await;
    post(
        acc,
        on(2026, Month::July, 15),
        EntryKind::Invoice,
        "INV-2026-00001",
        &[
            ("1100", 148_250 * times, None),
            ("4000", -100_000 * times, Some(2100)),
            ("2100", -21_000 * times, Some(2100)),
            ("4000", -25_000 * times, Some(900)),
            ("2100", -2_250 * times, Some(900)),
        ],
    )
    .await;
    post(
        acc,
        on(2026, Month::August, 1),
        EntryKind::Bill,
        "hosting",
        &[
            ("6000", 40_000 * times, Some(2100)),
            ("1200", 8_400 * times, Some(2100)),
            ("2000", -48_400 * times, None),
        ],
    )
    .await;
    post(
        acc,
        on(2026, Month::September, 3),
        EntryKind::Payment,
        "INV-2026-00001 settled",
        &[
            ("1000", 148_250 * times, None),
            ("1100", -148_250 * times, None),
        ],
    )
    .await;
}

/// An admin with a quarter of books behind them.
async fn books(tag: &str) -> Harness {
    let h = harness(tag).await;
    h.ts.set_admin(&h.user, true).await.unwrap();
    a_quarter(&h.acc, tag, 1).await;
    h
}

const QUARTER: &str = "/finance/reports/vat?from=2026-07-01&to=2026-09-30";
const QUARTER_CSV: &str = "/finance/reports/vat.csv?from=2026-07-01&to=2026-09-30";

// ---- the tests ---------------------------------------------------------------

#[tokio::test]
async fn a_quarter_of_books_reads_the_same_on_both_representations() {
    let h = books("vatquarter").await;

    let (status, body) = get(&h.app, &h.token, QUARTER).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let report = &body["report"];
    assert_eq!(report["from"], "2026-07-01");
    assert_eq!(report["to"], "2026-09-30");
    assert_eq!(report["currency"], "EUR");
    assert_eq!(
        report["output"]["rates"][0],
        serde_json::json!({ "rateBp": 900, "baseCents": 25_000, "vatCents": 2_250 })
    );
    assert_eq!(report["output"]["baseCents"], 125_000);
    assert_eq!(report["output"]["vatCents"], 23_250);
    assert_eq!(report["output"]["unratedBaseCents"], 0);
    assert_eq!(report["input"]["baseCents"], 40_000);
    assert_eq!(report["input"]["vatCents"], 8_400);
    assert_eq!(
        report["netPayableCents"], 14_850,
        "232.50 charged less 84.00 paid: {report}"
    );

    // The same read as a file: the same figures, and the headers that make it a
    // download.
    let (status, headers, csv) = get_file(&h.app, &h.token, QUARTER_CSV).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        header(&headers, "content-type"),
        Some("text/csv; charset=utf-8")
    );
    assert_eq!(
        header(&headers, "content-disposition"),
        Some("attachment; filename=\"vat-return-2026-07-01-to-2026-09-30.csv\"")
    );
    assert_eq!(header(&headers, "cache-control"), Some("no-store"));
    assert_eq!(header(&headers, "x-content-type-options"), Some("nosniff"));

    let lines = rows(&csv);
    assert_eq!(
        lines[0],
        "row,periodFrom,periodTo,currency,vatRatePercent,base,vat"
    );
    assert!(
        lines.contains(&"outputRate,2026-07-01,2026-09-30,EUR,21.00,1000.00,210.00"),
        "{csv}"
    );
    assert!(
        lines.contains(&"outputTotal,2026-07-01,2026-09-30,EUR,,1250.00,232.50"),
        "{csv}"
    );
    assert!(
        lines.contains(&"inputTotal,2026-07-01,2026-09-30,EUR,,400.00,84.00"),
        "{csv}"
    );
    assert!(
        lines.contains(&"netPayable,2026-07-01,2026-09-30,EUR,,,148.50"),
        "{csv}"
    );
}

#[tokio::test]
async fn a_vat_return_is_not_read_by_a_member_who_is_not_an_admin() {
    let h = harness("vatclerk").await;
    a_quarter(&h.acc, "vatclerk", 1).await;

    // The harness user is an ordinary member: both doors are shut.
    for uri in [QUARTER, QUARTER_CSV] {
        let (status, _) = get(&h.app, &h.token, uri).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{uri}");
    }
    // …and the periods list, which any member reads, still answers — so the
    // refusal above is this route's rule and not a broken token.
    let (status, _) = get(&h.app, &h.token, "/finance/periods").await;
    assert_eq!(status, StatusCode::OK);

    // Made an admin, the same person reads the same books.
    h.ts.set_admin(&h.user, true).await.unwrap();
    let (status, body) = get(&h.app, &h.token, QUARTER).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["report"]["netPayableCents"], 14_850);
}

#[tokio::test]
async fn no_token_reads_nothing_on_either_route() {
    let h = harness("vatnoauth").await;
    for uri in [QUARTER, QUARTER_CSV] {
        let (status, _) = send(&h.app, request(uri, None)).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{uri}");
    }
}

#[tokio::test]
async fn the_period_is_always_stated_and_never_guessed_at() {
    let h = books("vatperiod").await;

    for (uri, expected) in [
        ("/finance/reports/vat", "from is required"),
        ("/finance/reports/vat?to=2026-09-30", "from is required"),
        ("/finance/reports/vat?from=2026-07-01", "to is required"),
        (
            "/finance/reports/vat?from=01/07/2026&to=2026-09-30",
            "from must be a date",
        ),
        (
            "/finance/reports/vat?from=2026-07-01&to=whenever",
            "to must be a date",
        ),
    ] {
        let (status, body) = get(&h.app, &h.token, uri).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{uri}");
        let detail = body["detail"].as_str().unwrap_or_default();
        assert!(detail.starts_with(expected), "{uri} answered {detail}");
    }

    // A period that ends before it starts is the store's refusal, and it
    // reaches the caller as the same 422 rather than as an empty return.
    let (status, body) = get(
        &h.app,
        &h.token,
        "/finance/reports/vat?from=2026-09-30&to=2026-07-01",
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

    // The file route is refused the same way, so a script that saves a return
    // never saves a wrong one.
    let (status, ..) = get_file(&h.app, &h.token, "/finance/reports/vat.csv?from=2026-07-01").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn a_period_nothing_was_charged_in_answers_zeroes_rather_than_nothing() {
    let h = books("vatquiet").await;
    let (status, body) = get(
        &h.app,
        &h.token,
        "/finance/reports/vat?from=2020-01-01&to=2020-12-31",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["report"]["output"]["rates"], Value::Array(Vec::new()));
    assert_eq!(body["report"]["netPayableCents"], 0);
    assert_eq!(
        body["report"]["currency"], "EUR",
        "a return that does not say what the nothing is nothing in is a question"
    );
}

#[tokio::test]
async fn one_tenants_tax_is_no_part_of_anothers_on_either_route() {
    let ours = books("vatours").await;
    // A second tenant on the same store, with ten times the books, on the same
    // days and the same account codes.
    let theirs = harness_on(Arc::clone(&ours.store), "vattheirs").await;
    theirs.ts.set_admin(&theirs.user, true).await.unwrap();
    a_quarter(&theirs.acc, "vattheirs", 10).await;

    let (status, body) = get(&ours.app, &ours.token, QUARTER).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["report"]["output"]["vatCents"], 23_250,
        "unmoved: {body}"
    );
    assert_eq!(body["report"]["netPayableCents"], 14_850);

    let (status, _, csv) = get_file(&ours.app, &ours.token, QUARTER_CSV).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!csv.contains("2325.00"), "{csv}");

    // And theirs reads only theirs.
    let (status, body) = get(&theirs.app, &theirs.token, QUARTER).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["report"]["output"]["vatCents"], 232_500);
    assert_eq!(body["report"]["input"]["vatCents"], 84_000);
    assert_eq!(body["report"]["netPayableCents"], 148_500);
}
