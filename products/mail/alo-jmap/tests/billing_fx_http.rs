//! The `/billing/fx/*` surface and the multi-currency arc it exists for
//! (B1.21), driven through the real router over a real Postgres.
//!
//! `alo-store`'s own suite proves the arithmetic and the tenancy; what this one
//! is for is the **edge**: that a rate is written and read back as the decimal it
//! was published as and never as a float; that a published file imports whole or
//! not at all; that a foreign-currency invoice cannot be issued until a rate
//! exists and then carries its own frozen snapshot; that the VAT summary states
//! the period in the tenant's accounting currency; and that none of it is
//! visible without a token or across tenants.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use time::format_description::well_known::Iso8601;
use time::{Duration, OffsetDateTime};

use crate::common::{harness, send};

// ---- request helpers ---------------------------------------------------------

async fn get(app: &Router, token: Option<&str>, uri: &str) -> (StatusCode, Value) {
    let mut req = Request::builder().method("GET").uri(uri);
    if let Some(token) = token {
        req = req.header("authorization", format!("Bearer {token}"));
    }
    send(app, req.body(Body::empty()).unwrap()).await
}

async fn body_request(
    app: &Router,
    method: &str,
    token: Option<&str>,
    uri: &str,
    content_type: &str,
    body: String,
) -> (StatusCode, Value) {
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", content_type);
    if let Some(token) = token {
        req = req.header("authorization", format!("Bearer {token}"));
    }
    send(app, req.body(Body::from(body)).unwrap()).await
}

async fn put(app: &Router, token: Option<&str>, uri: &str, body: Value) -> (StatusCode, Value) {
    body_request(app, "PUT", token, uri, "application/json", body.to_string()).await
}

async fn post_json(app: &Router, token: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    body_request(
        app,
        "POST",
        Some(token),
        uri,
        "application/json",
        body.to_string(),
    )
    .await
}

async fn post_csv(
    app: &Router,
    token: Option<&str>,
    uri: &str,
    csv: String,
) -> (StatusCode, Value) {
    body_request(app, "POST", token, uri, "text/csv", csv).await
}

fn day(offset: i64) -> String {
    (OffsetDateTime::now_utc().date() + Duration::days(offset))
        .format(&Iso8601::DATE)
        .unwrap()
}

fn today() -> String {
    day(0)
}

/// The published daily file, for the day the store will issue on.
fn daily_file() -> String {
    format!(
        "Date, USD, JPY, PLN\n{}, 1.1626, 171.42, 4.2755, \n",
        today()
    )
}

/// A customer billed in `currency`.
async fn a_customer(app: &Router, token: &str, name: &str, currency: &str) -> String {
    let (status, body) = post_json(
        app,
        token,
        "/billing/customers",
        json!({ "name": name, "country": "NL", "currency": currency, "paymentTermsDays": 30 }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["customer"]["id"].as_str().unwrap().to_owned()
}

/// A draft for `customer` in `currency`, with one line of `price_cents` at
/// `rate_bp`.
async fn a_draft(
    app: &Router,
    token: &str,
    customer: &str,
    currency: &str,
    price_cents: i64,
    rate_bp: i32,
) -> String {
    let (status, body) = post_json(
        app,
        token,
        "/billing/invoices",
        json!({
            "customerId": customer,
            "currency": currency,
            "lines": [{ "description": "Consulting", "unit": "hour", "qtyMilli": 1_000,
                        "unitPriceCents": price_cents, "vatRateBp": rate_bp }],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["invoice"]["id"].as_str().unwrap().to_owned()
}

// ---- the rate table on the wire ----------------------------------------------

#[tokio::test]
async fn rates_are_written_and_read_back_as_the_decimals_they_were_published_as() {
    let h = harness("bill-fx-rates").await;
    common::seed_default_chart(&h.acc).await;

    // Nothing at all without a token, on every route.
    for (status, _) in [
        get(&h.app, None, "/billing/fx/rates").await,
        put(&h.app, None, "/billing/fx/rates", json!({})).await,
        post_csv(&h.app, None, "/billing/fx/rates/import", daily_file()).await,
    ] {
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    // One rate, by hand.
    let (status, body) = put(
        &h.app,
        Some(&h.token),
        "/billing/fx/rates",
        json!({ "currency": "usd", "date": today(), "rate": "1.1626" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let rate = &body["rate"];
    assert_eq!(rate["currency"], "USD", "canonical, not as it was typed");
    assert_eq!(rate["date"], today());
    assert_eq!(rate["rateMicro"], json!(1_162_600));
    assert_eq!(rate["rate"], "1.1626", "and back as it was published");
    assert_eq!(rate["source"], "manual");

    // Writing the same day again is a correction, not a second rate.
    let (status, body) = put(
        &h.app,
        Some(&h.token),
        "/billing/fx/rates",
        json!({ "currency": "USD", "date": today(), "rate": "1.17" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["rate"]["rateMicro"], json!(1_170_000));
    let (status, body) = get(&h.app, Some(&h.token), "/billing/fx/rates?currency=USD").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["rates"].as_array().unwrap().len(), 1);

    // The refusals: a rate that is not a plain decimal, a euro rate, a day that
    // is not a plain day — each in the store's own words.
    for (payload, expect) in [
        (
            json!({ "currency": "USD", "date": today(), "rate": "1,1626" }),
            "decimal",
        ),
        (
            json!({ "currency": "USD", "date": today(), "rate": "0" }),
            "decimal",
        ),
        (
            json!({ "currency": "EUR", "date": today(), "rate": "1.0" }),
            "quoted against",
        ),
        (
            json!({ "currency": "US", "date": today(), "rate": "1.1" }),
            "ISO 4217",
        ),
        (
            json!({ "currency": "USD", "date": "07/08/2026", "rate": "1.1" }),
            "YYYY-MM-DD",
        ),
    ] {
        let (status, body) =
            put(&h.app, Some(&h.token), "/billing/fx/rates", payload.clone()).await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{payload} should have been refused: {body}"
        );
        let detail = body["detail"].as_str().unwrap_or_default();
        assert!(detail.contains(expect), "{payload} → {detail}");
    }
    // A rate sent as a JSON number is a malformed body: a rate never arrives as
    // a float.
    let (status, _) = put(
        &h.app,
        Some(&h.token),
        "/billing/fx/rates",
        json!({ "currency": "USD", "date": today(), "rate": 1.1626 }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_published_file_imports_whole_or_not_at_all() {
    let h = harness("bill-fx-import").await;
    common::seed_default_chart(&h.acc).await;

    let (status, body) = post_csv(
        &h.app,
        Some(&h.token),
        "/billing/fx/rates/import",
        daily_file(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["import"]["rates"], json!(3));
    assert_eq!(body["import"]["days"], json!(1));
    assert_eq!(body["import"]["currencies"], json!(3));
    assert_eq!(body["import"]["from"], today());
    assert_eq!(body["import"]["to"], today());

    let (status, body) = get(&h.app, Some(&h.token), "/billing/fx/rates").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let rates = body["rates"].as_array().unwrap();
    assert_eq!(rates.len(), 3);
    assert!(rates.iter().all(|r| r["source"] == "ecb"));
    assert!(
        rates
            .iter()
            .any(|r| r["currency"] == "JPY" && r["rate"] == "171.42")
    );

    // A file with one bad cell changes nothing, and says which row and column.
    let (status, body) = post_csv(
        &h.app,
        Some(&h.token),
        "/billing/fx/rates/import",
        format!("Date,USD,JPY\n{},1.16,17O.98\n", today()),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    let detail = body["detail"].as_str().unwrap_or_default();
    assert!(detail.contains("row 2, column JPY"), "{detail}");
    let (_, body) = get(&h.app, Some(&h.token), "/billing/fx/rates?currency=USD").await;
    assert_eq!(
        body["rates"][0]["rate"], "1.1626",
        "the rate it already had, not the half-imported one"
    );

    // A file that is not a reference-rate file at all.
    for (file, expect) in [
        (String::from("Day,USD\n2026-08-07,1.16\n"), "headed Date"),
        (String::new(), "empty"),
        (
            format!("Date,USD\n{},1,1626\n", today()),
            "more values than the header",
        ),
    ] {
        let (status, body) =
            post_csv(&h.app, Some(&h.token), "/billing/fx/rates/import", file).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
        let detail = body["detail"].as_str().unwrap_or_default();
        assert!(detail.contains(expect), "{detail}");
    }

    // A stated period narrows the list; a malformed one is refused.
    let (status, body) = get(
        &h.app,
        Some(&h.token),
        &format!("/billing/fx/rates?from={}&to={}", day(-9), day(-2)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["rates"], json!([]));
    let (status, body) = get(&h.app, Some(&h.token), "/billing/fx/rates?from=07/08/2026").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        body["detail"].as_str().unwrap_or_default(),
        "from must be a date of the form YYYY-MM-DD"
    );
}

// ---- the arc the item exists for (its done-when) ----------------------------

#[tokio::test]
async fn a_dollar_invoice_stores_its_rate_and_the_vat_report_converts_it() {
    let h = harness("bill-fx-arc").await;
    common::seed_default_chart(&h.acc).await;
    let customer = a_customer(&h.app, &h.token, "Acme Inc", "USD").await;

    // 1 × $500.00 at 21 %: net 50 000, VAT 10 500, gross 60 500.
    let usd = a_draft(&h.app, &h.token, &customer, "USD", 50_000, 2_100).await;
    let (status, body) = get(&h.app, Some(&h.token), &format!("/billing/invoices/{usd}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["invoice"]["currency"], "USD");
    assert_eq!(
        body["invoice"]["fx"],
        json!(null),
        "a draft carries no rate"
    );
    assert!(body["invoice"].get("baseTotals").is_none());

    // It cannot be issued before a rate exists: an invoice that cannot state
    // its VAT in the tenant's own currency is legally incomplete.
    let (status, body) = post_json(
        &h.app,
        &h.token,
        &format!("/billing/invoices/{usd}/issue"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        body["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("no exchange rate for USD"),
        "{body}"
    );

    // Import the published file, and issue.
    let (status, _) = post_csv(
        &h.app,
        Some(&h.token),
        "/billing/fx/rates/import",
        daily_file(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = post_json(
        &h.app,
        &h.token,
        &format!("/billing/invoices/{usd}/issue"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let invoice = &body["invoice"];
    assert_eq!(invoice["fx"]["baseCurrency"], "EUR");
    assert_eq!(invoice["fx"]["rateMicro"], json!(1_162_600));
    assert_eq!(invoice["fx"]["rate"], "1.1626");
    assert_eq!(invoice["fx"]["rateDate"], today());
    // Hand-computed: net 50 000 / 1.1626 = 43 007.05… → 430.07;
    //                VAT 10 500 / 1.1626 =  9 031.48… →  90.31.
    assert_eq!(invoice["totals"]["netCents"], json!(50_000));
    assert_eq!(invoice["baseTotals"]["netCents"], json!(43_007));
    assert_eq!(invoice["baseTotals"]["vatCents"], json!(9_031));
    assert_eq!(invoice["baseTotals"]["grossCents"], json!(52_038));

    // The list entry says the same thing — the screen and the document cannot
    // disagree about what the tenant owes on it.
    let (status, body) = get(&h.app, Some(&h.token), "/billing/invoices").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let listed = &body["invoices"][0];
    assert_eq!(listed["baseTotals"]["vatCents"], json!(9_031));

    // The printed document states the VAT in euro as well, with the rate that
    // produced it — art. 230 requires exactly that on a foreign-currency
    // invoice.
    let (status, _, page) =
        fetch_text(&h.app, &h.token, &format!("/billing/invoices/{usd}/print")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("VAT in EUR"), "{page}");
    assert!(page.contains("EUR 90.31"), "{page}");
    assert!(
        page.contains("1 EUR = 1.1626 USD") && page.contains(&today()),
        "the rate and the day it was published: {page}"
    );

    // And the VAT summary states the period in the accounting currency.
    let (status, body) = get(
        &h.app,
        Some(&h.token),
        &format!("/billing/reports/vat?from={}&to={}", today(), today()),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let report = &body["report"];
    let usd_group = &report["currencies"][0];
    assert_eq!(usd_group["currency"], "USD");
    assert_eq!(usd_group["netCents"], json!(50_000), "as billed");
    assert_eq!(usd_group["baseNetCents"], json!(43_007), "as booked");
    assert_eq!(usd_group["unconvertedCount"], json!(0));
    assert_eq!(report["base"]["currency"], "EUR");
    assert_eq!(report["base"]["netCents"], json!(43_007));
    assert_eq!(report["base"]["vatCents"], json!(9_031));
    assert_eq!(report["base"]["unconvertedCount"], json!(0));
}

#[tokio::test]
async fn the_accounting_currency_is_a_setting_and_it_is_the_tenants_own() {
    let a = harness("bill-fx-base-a").await;
    common::seed_default_chart(&a.acc).await;
    let b = harness("bill-fx-base-b").await;
    common::seed_default_chart(&b.acc).await;

    // Unstated, a tenant keeps books in euro — never a blank.
    let (status, body) = get(&a.app, Some(&a.token), "/billing/settings").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["settings"]["stated"], json!(false));
    assert_eq!(body["settings"]["baseCurrency"], "EUR");

    let (status, body) = body_request(
        &a.app,
        "PATCH",
        Some(&a.token),
        "/billing/settings",
        "application/json",
        json!({ "legalName": "Alo Polska sp. z o.o.", "country": "PL",
                "baseCurrency": "pln" })
        .to_string(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["settings"]["baseCurrency"], "PLN", "canonical");

    // A stated-but-impossible currency is refused rather than stored.
    let (status, body) = body_request(
        &a.app,
        "PATCH",
        Some(&a.token),
        "/billing/settings",
        "application/json",
        json!({ "baseCurrency": "ZLOTY" }).to_string(),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        body["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("ISO 4217"),
        "{body}"
    );

    // And B is untouched by A's choice, and by A's rates.
    let (_, body) = get(&b.app, Some(&b.token), "/billing/settings").await;
    assert_eq!(body["settings"]["baseCurrency"], "EUR");
    let (status, _) = post_csv(
        &a.app,
        Some(&a.token),
        "/billing/fx/rates/import",
        daily_file(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = get(&b.app, Some(&b.token), "/billing/fx/rates").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["rates"], json!([]), "B's table is B's");

    // So B cannot issue a dollar invoice at all.
    let customer = a_customer(&b.app, &b.token, "Acme Inc", "USD").await;
    let usd = a_draft(&b.app, &b.token, &customer, "USD", 10_000, 0).await;
    let (status, body) = post_json(
        &b.app,
        &b.token,
        &format!("/billing/invoices/{usd}/issue"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
}

/// A `GET` whose body is not JSON (the printed page).
async fn fetch_text(
    app: &Router,
    token: &str,
    uri: &str,
) -> (StatusCode, Vec<(String, String)>, String) {
    use tower::ServiceExt;
    let req = Request::builder()
        .method("GET")
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
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
