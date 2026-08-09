//! The `/finance/reports/balance` surface (B4.11b), driven through the real
//! router over a real Postgres.
//!
//! `alo-store`'s own golden suite proves the arithmetic of a set of books; what
//! this one is for is the **edge**: that a balance sheet is admin-only and says
//! so; that the two representations of one read — the JSON a screen shows and
//! the CSV an accountant opens — carry the same figures and both say the sheet
//! balances; that the date is always stated and never guessed; that the file is
//! served as an attachment no cache keeps; and that another tenant's position
//! appears in neither, on either route.

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

/// Two years of books, the same as the P&L suite's: €1 000.00 of revenue and
/// €150.00 of hosting in 2026, half of each in 2025, and a payment that moves
/// the bank and the receivable without touching the result.
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

/// An admin with two years of books behind them.
async fn books(tag: &str) -> Harness {
    let h = harness(tag).await;
    h.ts.set_admin(&h.user, true).await.unwrap();
    a_year(&h.acc, tag).await;
    h
}

const YEAR_END: &str = "/finance/reports/balance?on=2026-12-31";
const YEAR_END_CSV: &str = "/finance/reports/balance.csv?on=2026-12-31";

// ---- the tests ---------------------------------------------------------------

#[tokio::test]
async fn a_year_end_reads_the_same_on_both_representations_and_balances() {
    let h = books("bsyear").await;

    let (status, body) = get(&h.app, &h.token, YEAR_END).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let report = &body["report"];
    assert_eq!(report["on"], "2026-12-31");
    assert_eq!(report["currency"], "EUR");
    // 1 210.00 in the bank, 500.00 still owed to us.
    assert_eq!(report["assetCents"], 171_000);
    // 225.00 to suppliers, 210.00 of VAT collected.
    assert_eq!(report["liabilityCents"], 43_500);
    assert_eq!(report["equityCents"], 0);
    assert_eq!(report["resultCents"], 127_500);
    assert_eq!(report["liabilityEquityCents"], 171_000);
    assert_eq!(report["differenceCents"], 0);
    assert_eq!(report["balances"], true);
    assert_eq!(report["assets"][0]["code"], "1000");
    assert_eq!(report["assets"][0]["name"], "bsyear 1000");
    assert_eq!(report["assets"][0]["role"], "bank");
    assert_eq!(report["assets"][0]["amountCents"], 121_000);
    assert_eq!(
        report["assets"].as_array().map(Vec::len),
        Some(2),
        "the bank and the receivable, and nothing that is not held: {report}"
    );
    assert_eq!(
        report["equity"],
        Value::Array(Vec::new()),
        "nobody has posted equity, and nothing invents any"
    );

    // The same read as a file: the same figures, and the headers that make it
    // a download.
    let (status, headers, csv) = get_file(&h.app, &h.token, YEAR_END_CSV).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        header(&headers, "content-type"),
        Some("text/csv; charset=utf-8")
    );
    assert_eq!(
        header(&headers, "content-disposition"),
        Some("attachment; filename=\"balance-sheet-2026-12-31.csv\"")
    );
    assert_eq!(header(&headers, "cache-control"), Some("no-store"));
    assert_eq!(header(&headers, "x-content-type-options"), Some("nosniff"));

    let lines = rows(&csv);
    assert_eq!(lines[0], "row,on,currency,accountCode,accountName,amount");
    assert!(
        lines.contains(&"asset,2026-12-31,EUR,1000,bsyear 1000,1210.00"),
        "{csv}"
    );
    assert!(
        lines.contains(&"assetTotal,2026-12-31,EUR,,,1710.00"),
        "{csv}"
    );
    assert!(
        lines.contains(&"liabilityTotal,2026-12-31,EUR,,,435.00"),
        "{csv}"
    );
    assert!(lines.contains(&"result,2026-12-31,EUR,,,1275.00"), "{csv}");
    assert!(
        lines.contains(&"liabilityEquityTotal,2026-12-31,EUR,,,1710.00"),
        "the file states the balancing figure, not only the screen: {csv}"
    );
    assert!(lines.contains(&"difference,2026-12-31,EUR,,,0.00"), "{csv}");
}

#[tokio::test]
async fn a_balance_sheet_is_not_read_by_a_member_who_is_not_an_admin() {
    let h = harness("bsclerk").await;
    a_year(&h.acc, "bsclerk").await;

    // The harness user is an ordinary member: both doors are shut.
    for uri in [YEAR_END, YEAR_END_CSV] {
        let (status, _) = get(&h.app, &h.token, uri).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{uri}");
    }
    // …and the periods list, which any member reads, still answers — so the
    // refusal above is this route's rule and not a broken token.
    let (status, _) = get(&h.app, &h.token, "/finance/periods").await;
    assert_eq!(status, StatusCode::OK);

    // Made an admin, the same person reads the same books.
    h.ts.set_admin(&h.user, true).await.unwrap();
    let (status, body) = get(&h.app, &h.token, YEAR_END).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["report"]["assetCents"], 171_000);
}

#[tokio::test]
async fn no_token_reads_nothing_on_either_route() {
    let h = harness("bsnoauth").await;
    for uri in [YEAR_END, YEAR_END_CSV] {
        let (status, _) = send(&h.app, request(uri, None)).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{uri}");
    }
}

#[tokio::test]
async fn the_date_is_always_stated_and_never_guessed_at() {
    let h = books("bsdate").await;

    for (uri, expected) in [
        ("/finance/reports/balance", "on is required"),
        ("/finance/reports/balance?on=", "on is required"),
        ("/finance/reports/balance?on=today", "on must be a date"),
        (
            "/finance/reports/balance?on=31/12/2026",
            "on must be a date",
        ),
        // A period is not what a balance sheet takes: `from`/`to` are ignored
        // and the missing `on` is what the refusal names.
        (
            "/finance/reports/balance?from=2026-01-01&to=2026-12-31",
            "on is required",
        ),
    ] {
        let (status, body) = get(&h.app, &h.token, uri).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{uri}");
        let detail = body["detail"].as_str().unwrap_or_default();
        assert!(detail.starts_with(expected), "{uri} answered {detail}");
    }

    // The file route is refused the same way, so a script that saves a sheet
    // never saves a wrong one.
    let (status, ..) = get_file(&h.app, &h.token, "/finance/reports/balance.csv").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn a_date_before_the_books_opened_answers_zeroes_rather_than_nothing() {
    let h = books("bsquiet").await;
    let (status, body) = get(&h.app, &h.token, "/finance/reports/balance?on=2020-01-01").await;
    assert_eq!(status, StatusCode::OK);
    let report = &body["report"];
    assert_eq!(report["assets"], Value::Array(Vec::new()));
    assert_eq!(report["assetCents"], 0);
    assert_eq!(report["resultCents"], 0);
    assert_eq!(report["balances"], true);
    assert_eq!(
        report["currency"], "EUR",
        "a file that does not say what the nothing is nothing in is a question"
    );

    // And the file is still a file, with its totals.
    let (status, _, csv) = get_file(
        &h.app,
        &h.token,
        "/finance/reports/balance.csv?on=2020-01-01",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(rows(&csv).len(), 7, "the header and the six figures: {csv}");
}

#[tokio::test]
async fn the_sheet_moves_with_the_date_asked_for() {
    let h = books("bswhen").await;

    // The end of 2025: one invoice outstanding, one bill unpaid.
    let (status, body) = get(&h.app, &h.token, "/finance/reports/balance?on=2025-12-31").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["report"]["assetCents"], 50_000);
    assert_eq!(body["report"]["liabilityCents"], 7_500);
    assert_eq!(body["report"]["resultCents"], 42_500);
    assert_eq!(body["report"]["balances"], true);

    // The day before the customer paid: the receivable is still ours and the
    // bank is empty.
    let (status, body) = get(&h.app, &h.token, "/finance/reports/balance?on=2026-04-03").await;
    assert_eq!(status, StatusCode::OK);
    let assets = body["report"]["assets"].as_array().cloned().unwrap();
    assert_eq!(assets.len(), 1, "{assets:?}");
    assert_eq!(assets[0]["code"], "1100");
    assert_eq!(assets[0]["amountCents"], 171_000);
    assert_eq!(body["report"]["balances"], true);
}

#[tokio::test]
async fn one_tenants_position_is_no_part_of_anothers_on_either_route() {
    let ours = books("bsours").await;
    // A second tenant on the same store, with ten times the books, on the same
    // days and the same account codes.
    let theirs = harness_on(Arc::clone(&ours.store), "bstheirs").await;
    theirs.ts.set_admin(&theirs.user, true).await.unwrap();
    chart_for(&theirs.acc, "bstheirs").await;
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

    let (status, body) = get(&ours.app, &ours.token, YEAR_END).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["report"]["assetCents"], 171_000, "unmoved: {body}");
    assert_eq!(body["report"]["assets"][0]["name"], "bsours 1000");

    let (status, _, csv) = get_file(&ours.app, &ours.token, YEAR_END_CSV).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!csv.contains("bstheirs"), "{csv}");
    assert!(!csv.contains("12100.00"), "{csv}");

    // And theirs reads only theirs, and balances on its own.
    let (status, body) = get(&theirs.app, &theirs.token, YEAR_END).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["report"]["assetCents"], 1_210_000);
    assert_eq!(body["report"]["liabilityCents"], 210_000);
    assert_eq!(body["report"]["resultCents"], 1_000_000);
    assert_eq!(body["report"]["balances"], true);
}
