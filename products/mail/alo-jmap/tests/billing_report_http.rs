//! The `/billing/reports/vat` surface (B1.20), driven through the real router
//! over a real Postgres.
//!
//! `alo-store`'s own suite proves the arithmetic of a period; what this one is
//! for is the **edge**: that the two representations of the same read — the
//! JSON a screen shows and the CSV an accountant opens — carry the same
//! figures; that a period is always stated and never guessed; that the file is
//! served as an attachment that no cache keeps; and that another tenant's
//! turnover appears in neither, on either route.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use time::format_description::well_known::Iso8601;
use time::{Duration, OffsetDateTime};
use tower::ServiceExt;

use common::{harness, send};

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

async fn post(app: &Router, token: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(body.to_string()))
        .unwrap();
    send(app, req).await
}

fn created_id(kind: &str, (status, body): (StatusCode, Value)) -> String {
    assert_eq!(status, StatusCode::OK, "create failed: {body}");
    body[kind]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("no {kind} id in {body}"))
        .to_owned()
}

fn day(offset: i64) -> String {
    (OffsetDateTime::now_utc().date() + Duration::days(offset))
        .format(&Iso8601::DATE)
        .unwrap()
}

fn today() -> String {
    day(0)
}

/// A customer in euro, on 30-day terms.
async fn a_customer(app: &Router, token: &str, name: &str) -> String {
    created_id(
        "customer",
        post(
            app,
            token,
            "/billing/customers",
            json!({ "name": name, "country": "NL", "currency": "EUR", "paymentTermsDays": 30 }),
        )
        .await,
    )
}

/// Raises a document with these lines, issues it, and returns its id.
async fn issued(app: &Router, token: &str, customer: &str, lines: Value) -> String {
    let id = created_id(
        "invoice",
        post(
            app,
            token,
            "/billing/invoices",
            json!({ "customerId": customer, "lines": lines }),
        )
        .await,
    );
    let (status, body) = post(
        app,
        token,
        &format!("/billing/invoices/{id}/issue"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "issue failed: {body}");
    id
}

/// 10 hours at €100.00, 21 % → net 100 000, VAT 21 000.
fn consulting() -> Value {
    json!([{ "description": "Consulting", "unit": "hour", "qtyMilli": 10_000,
             "unitPriceCents": 10_000, "vatRateBp": 2_100 }])
}

/// 1 × €250.00 at 9 % → net 25 000, VAT 2 250.
fn reduced_rate() -> Value {
    json!([{ "description": "Printed manual", "unit": "piece", "qtyMilli": 1_000,
             "unitPriceCents": 25_000, "vatRateBp": 900 }])
}

/// The rows of a CSV body, without the trailing blank.
fn rows(body: &str) -> Vec<&str> {
    body.split("\r\n").filter(|line| !line.is_empty()).collect()
}

// ---- the report on the wire (the item's done-when) --------------------------

#[tokio::test]
async fn a_periods_summary_answers_the_same_figures_as_json_and_as_a_file() {
    let h = harness("bill-vat-arc").await;
    common::seed_default_chart(&h.acc).await;
    let customer = a_customer(&h.app, &h.token, "Acme GmbH").await;

    // Today's documents: two invoices across two rates, then a credit note
    // taking half of the first one back.
    let first = issued(&h.app, &h.token, &customer, consulting()).await;
    issued(&h.app, &h.token, &customer, reduced_rate()).await;
    let credit = created_id(
        "invoice",
        post(
            &h.app,
            &h.token,
            &format!("/billing/invoices/{first}/credit-note"),
            json!({}),
        )
        .await,
    );
    // Edited down to half before issuing — a partial credit is a matter of
    // editing the mirrored lines.
    let (status, body) = send(
        &h.app,
        Request::builder()
            .method("PATCH")
            .uri(format!("/billing/invoices/{credit}"))
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {}", h.token))
            .body(Body::from(
                json!({ "lines": [{ "description": "Consulting", "unit": "hour",
                                    "qtyMilli": -5_000, "unitPriceCents": 10_000,
                                    "vatRateBp": 2_100 }] })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/billing/invoices/{credit}/issue"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // A draft, which was never raised, and a voided document, which was
    // cancelled: neither charged anybody any tax, so neither may appear.
    created_id(
        "invoice",
        post(
            &h.app,
            &h.token,
            "/billing/invoices",
            json!({ "customerId": customer, "lines": [{ "description": "Never sent",
                    "qtyMilli": 1_000, "unitPriceCents": 900_000, "vatRateBp": 2_100 }] }),
        )
        .await,
    );
    let cancelled = issued(
        &h.app,
        &h.token,
        &customer,
        json!([{ "description": "Wrong", "qtyMilli": 1_000,
                 "unitPriceCents": 800_000, "vatRateBp": 2_100 }]),
    )
    .await;
    let (status, _) = post(
        &h.app,
        &h.token,
        &format!("/billing/invoices/{cancelled}/void"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // ---- the JSON -----------------------------------------------------------
    //
    // 9 %:  25 000 net, 2 250 VAT.
    // 21 %: 100 000 − 50 000 = 50 000 net, 21 000 − 10 500 = 10 500 VAT.
    let (status, body) = get(
        &h.app,
        &h.token,
        &format!("/billing/reports/vat?from={}&to={}", today(), today()),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let report = &body["report"];
    assert_eq!(report["from"], today());
    assert_eq!(report["to"], today());
    let eur = &report["currencies"][0];
    assert_eq!(eur["currency"], "EUR");
    assert_eq!(eur["invoiceCount"], 2, "the draft and the void one are out");
    assert_eq!(eur["creditNoteCount"], 1);
    assert_eq!(
        eur["byRate"],
        json!([
            { "rateBp": 900, "netCents": 25_000, "vatCents": 2_250 },
            { "rateBp": 2_100, "netCents": 50_000, "vatCents": 10_500 },
        ])
    );
    assert_eq!(eur["netCents"], 75_000);
    assert_eq!(eur["vatCents"], 12_750);
    assert_eq!(eur["grossCents"], 87_750);
    assert_eq!(
        report["currencies"].as_array().map(Vec::len),
        Some(1),
        "one currency, so one group"
    );

    // ---- and the same figures as a file ------------------------------------
    let (status, headers, csv) = get_file(
        &h.app,
        &h.token,
        &format!("/billing/reports/vat.csv?from={}&to={}", today(), today()),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{csv}");
    let header = |name: &str| {
        headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
            .unwrap_or_default()
            .to_owned()
    };
    assert_eq!(header("content-type"), "text/csv; charset=utf-8");
    assert_eq!(
        header("content-disposition"),
        format!(
            "attachment; filename=\"vat-{}-to-{}.csv\"",
            today(),
            today()
        ),
        "saved as a file, never rendered in our own origin"
    );
    assert_eq!(header("cache-control"), "no-store", "a tenant's turnover");
    assert_eq!(header("x-content-type-options"), "nosniff");

    let lines = rows(&csv);
    assert_eq!(
        lines[0],
        "row,periodFrom,periodTo,currency,vatRatePercent,net,vat,gross,invoices,creditNotes,\
         unconverted",
        "the columns are a contract, in English, whatever the UI language"
    );
    assert_eq!(
        lines[1],
        format!(
            "rate,{},{},EUR,9.00,250.00,22.50,272.50,,,",
            today(),
            today()
        )
    );
    assert_eq!(
        lines[2],
        format!(
            "rate,{},{},EUR,21.00,500.00,105.00,605.00,,,",
            today(),
            today()
        )
    );
    assert_eq!(
        lines[3],
        format!(
            "total,{},{},EUR,,750.00,127.50,877.50,2,1,0",
            today(),
            today()
        ),
        "the same figures as the JSON, as decimals"
    );
    // Then the same period once more in the currency the books are kept in —
    // here the same figures, since the tenant bills in it (B1.21).
    assert_eq!(
        lines[4],
        format!(
            "baseRate,{},{},EUR,9.00,250.00,22.50,272.50,,,",
            today(),
            today()
        )
    );
    assert_eq!(
        lines[5],
        format!(
            "baseRate,{},{},EUR,21.00,500.00,105.00,605.00,,,",
            today(),
            today()
        )
    );
    assert_eq!(
        lines[6],
        format!(
            "baseTotal,{},{},EUR,,750.00,127.50,877.50,,,0",
            today(),
            today()
        ),
        "the figure a return is filed from, and nothing missing from it"
    );
    assert_eq!(lines.len(), 7, "{csv}");
    // Nothing that names anybody: a summary emailed to an accountant carries
    // no customer list.
    assert!(!csv.contains("Acme"), "{csv}");

    // ---- a period that holds nothing ---------------------------------------
    let (status, body) = get(
        &h.app,
        &h.token,
        &format!("/billing/reports/vat?from={}&to={}", day(-30), day(-2)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["report"]["currencies"],
        json!([]),
        "no currencies, not a row of zeros"
    );
    let (status, _, csv) = get_file(
        &h.app,
        &h.token,
        &format!("/billing/reports/vat.csv?from={}&to={}", day(-30), day(-2)),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    // The header, and the `baseTotal` row that says the nothing is nothing in
    // euro — a file that does not name its currency is a question (B1.21).
    let empty = rows(&csv);
    assert_eq!(empty.len(), 2, "{csv:?}");
    assert_eq!(
        empty[1],
        format!("baseTotal,{},{},EUR,,0.00,0.00,0.00,,,0", day(-30), day(-2))
    );
}

// ---- the refusals -----------------------------------------------------------

#[tokio::test]
async fn a_period_is_always_stated_and_never_guessed_at() {
    let h = harness("bill-vat-guards").await;
    common::seed_default_chart(&h.acc).await;

    for query in [
        String::new(),
        "?from=".to_owned() + &today(),
        format!("?to={}", today()),
        format!("?from=&to={}", today()),
        format!("?from=01/07/2025&to={}", today()),
        format!("?from={}&to=2025-13-01", today()),
        format!("?from={}&to={}T00:00:00Z", today(), today()),
    ] {
        for path in ["/billing/reports/vat", "/billing/reports/vat.csv"] {
            let (status, body) = get(&h.app, &h.token, &format!("{path}{query}")).await;
            assert_eq!(
                status,
                StatusCode::UNPROCESSABLE_ENTITY,
                "{path}{query} → {body}"
            );
            // The refusal names which end is wrong, so a caller with both
            // wrong learns which one it is looking at.
            let detail = body["detail"].as_str().unwrap_or_default();
            assert!(
                detail.starts_with("from") || detail.starts_with("to"),
                "{path}{query} → {body}"
            );
        }
    }

    // A period that ends before it starts is the store's refusal, in the
    // store's own words.
    let (status, body) = get(
        &h.app,
        &h.token,
        &format!("/billing/reports/vat?from={}&to={}", today(), day(-1)),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        body["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("before its start"),
        "{body}"
    );

    // No token: the auth guard runs before the period is even parsed, so an
    // unauthenticated caller cannot use a malformed query as an oracle either.
    for uri in [
        format!("/billing/reports/vat?from={}&to={}", today(), today()),
        "/billing/reports/vat".to_owned(),
        format!("/billing/reports/vat.csv?from={}&to={}", today(), today()),
        "/billing/reports/vat.csv".to_owned(),
    ] {
        let (status, body) = send(&h.app, request(&uri, None)).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{uri} → {body}");
    }
}

// ---- the wrong-tenant test (mandatory: CLAUDE.md law 1) ----------------------

#[tokio::test]
async fn another_tenants_turnover_never_appears_in_a_report() {
    let a = harness("bill-vat-tenant-a").await;
    common::seed_default_chart(&a.acc).await;
    let b = harness("bill-vat-tenant-b").await;
    common::seed_default_chart(&b.acc).await;

    // B bills something today, through B's own door.
    let b_customer = a_customer(&b.app, &b.token, "B GmbH").await;
    issued(&b.app, &b.token, &b_customer, consulting()).await;

    // A, who has billed nothing, sees nothing at all for the same days — not
    // B's figures, and not an error that would tell A they exist.
    let period = format!("from={}&to={}", today(), today());
    let (status, body) = get(&a.app, &a.token, &format!("/billing/reports/vat?{period}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["report"]["currencies"], json!([]));
    let (status, _, csv) = get_file(
        &a.app,
        &a.token,
        &format!("/billing/reports/vat.csv?{period}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let empty = rows(&csv);
    assert_eq!(empty.len(), 2, "the header and an empty total: {csv:?}");
    assert_eq!(
        empty[1],
        format!("baseTotal,{},{},EUR,,0.00,0.00,0.00,,,0", today(), today()),
        "zero, in A's own accounting currency — never a figure of B's"
    );

    // A now bills something of its own: A's report is A's, not B's minus
    // something.
    let a_customer = a_customer(&a.app, &a.token, "A BV").await;
    issued(&a.app, &a.token, &a_customer, reduced_rate()).await;
    let (status, body) = get(&a.app, &a.token, &format!("/billing/reports/vat?{period}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let eur = &body["report"]["currencies"][0];
    assert_eq!(eur["netCents"], 25_000);
    assert_eq!(eur["vatCents"], 2_250);
    assert_eq!(eur["invoiceCount"], 1);
    assert_eq!(
        eur["byRate"],
        json!([{ "rateBp": 900, "netCents": 25_000, "vatCents": 2_250 }]),
        "B's 21 % subtotal appears nowhere in A's report"
    );

    // And B still sees exactly B's.
    let (status, body) = get(&b.app, &b.token, &format!("/billing/reports/vat?{period}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let eur = &body["report"]["currencies"][0];
    assert_eq!(eur["netCents"], 100_000);
    assert_eq!(eur["vatCents"], 21_000);
}
