//! The `/billing/quotes` HTTP surface (B1.12), driven through the real router
//! over a real Postgres.
//!
//! `alo-store`'s own suites prove the offer and its acceptance work; what this
//! suite is for is the **edge**: that the arc a salesperson actually walks —
//! draft an offer, send it, have it accepted — comes back over the wire with
//! the numbers and the status codes `docs/design/billing.md` publishes; that
//! acceptance hands over both documents, the invoice worth exactly what was
//! offered; that a sent offer refuses every write verb; and that another
//! tenant's offer is invisible and untouchable on all nine routes, answering
//! exactly as an id that never existed.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use time::format_description::well_known::Iso8601;
use time::{Duration, OffsetDateTime};

use crate::common::{database_url, harness, send};

// ---- request helpers ---------------------------------------------------------

fn with_json(method: &str, uri: &str, token: Option<&str>, body: Value) -> Request<Body> {
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    if let Some(token) = token {
        req = req.header("authorization", format!("Bearer {token}"));
    }
    req.body(Body::from(body.to_string())).unwrap()
}

async fn post(app: &Router, token: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    send(app, with_json("POST", uri, Some(token), body)).await
}

async fn patch(app: &Router, token: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    send(app, with_json("PATCH", uri, Some(token), body)).await
}

async fn get(app: &Router, token: &str, uri: &str) -> (StatusCode, Value) {
    send(app, with_json("GET", uri, Some(token), json!({}))).await
}

async fn delete(app: &Router, token: &str, uri: &str) -> (StatusCode, Value) {
    send(app, with_json("DELETE", uri, Some(token), json!({}))).await
}

fn created_id(kind: &str, (status, body): (StatusCode, Value)) -> String {
    assert_eq!(status, StatusCode::OK, "create failed: {body}");
    body[kind]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("no {kind} id in {body}"))
        .to_owned()
}

/// A customer on 30-day terms in euro — the party every offer here is made to.
async fn a_customer(app: &Router, token: &str) -> String {
    created_id(
        "customer",
        post(
            app,
            token,
            "/billing/customers",
            json!({
                "name": "Acme GmbH",
                "addressLine1": "Hauptstraße 1",
                "postalCode": "10115",
                "city": "Berlin",
                "country": "DE",
                "vatId": "DE811907980",
                "paymentTermsDays": 30,
                "currency": "EUR",
            }),
        )
        .await,
    )
}

/// Three lines across two VAT rates, one fractional quantity and one discount
/// — a set whose totals only come out right if the server rounds once per rate
/// and the copy onto the invoice is exact.
fn three_lines() -> Value {
    json!([
        { "description": "Consulting", "unit": "hour", "qtyMilli": 7_500,
          "unitPriceCents": 12_500, "vatRateBp": 2_100 },
        { "description": "Printed manual", "unit": "piece", "qtyMilli": 3_000,
          "unitPriceCents": 999, "vatRateBp": 900 },
        { "description": "Introductory discount", "unit": "hour", "qtyMilli": -1_000,
          "unitPriceCents": 12_500, "vatRateBp": 2_100 },
    ])
}

fn today() -> time::Date {
    OffsetDateTime::now_utc().date()
}

fn as_day(d: time::Date) -> String {
    d.format(&Iso8601::DATE).unwrap()
}

fn ids(body: &Value, key: &str) -> Vec<String> {
    body[key]
        .as_array()
        .unwrap_or_else(|| panic!("no {key} array in {body}"))
        .iter()
        .map(|i| i["id"].as_str().unwrap_or_default().to_owned())
        .collect()
}

/// A sent offer with the three lines above, returning its id.
async fn a_sent_quote(app: &Router, token: &str, customer: &str) -> String {
    let id = created_id(
        "quote",
        post(
            app,
            token,
            "/billing/quotes",
            json!({ "customerId": customer, "reference": "RFQ-2026-88",
                    "validDays": 14, "lines": three_lines() }),
        )
        .await,
    );
    let (status, body) = post(app, token, &format!("/billing/quotes/{id}/send"), json!({})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    id
}

// ---- the arc (the item's done-when) -----------------------------------------

#[tokio::test]
async fn the_draft_to_sent_to_accepted_arc_yields_the_invoice_on_the_wire() {
    let h = harness("bill-quo-arc").await;
    common::seed_default_chart(&h.acc).await;
    let customer = a_customer(&h.app, &h.token).await;

    // --- raise a draft offer, header and lines in one body -------------------
    let (status, body) = post(
        &h.app,
        &h.token,
        "/billing/quotes",
        json!({ "customerId": customer, "reference": "RFQ-2026-88", "validDays": 14,
                "note": "This offer stands for a fortnight.", "lines": three_lines() }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let quote = &body["quote"];
    let id = quote["id"].as_str().unwrap().to_owned();
    assert_eq!(quote["status"], "draft");
    assert_eq!(quote["reference"], "RFQ-2026-88");
    assert_eq!(quote["validDays"], 14);
    // The customer's currency was resolved and snapshotted on the document.
    assert_eq!(quote["currency"], "EUR");
    // A draft carries no number, no dates and no decision — that is what makes
    // it a draft, and it is how a client tells the phases apart.
    assert_eq!(quote["number"], Value::Null);
    assert_eq!(quote["sentDate"], Value::Null);
    assert_eq!(quote["validUntil"], Value::Null);
    assert_eq!(quote["decidedDate"], Value::Null);
    assert_eq!(quote["expired"], false);

    // The money is the server's, rounded once per rate: 7.5 h less 1 h at €125
    // = 81 250 at 21 % (VAT 17 062.5 → 17 063), plus 2 997 at 9 % (VAT 269.73
    // → 270).
    let totals = &quote["totals"];
    assert_eq!(totals["netCents"], 84_247);
    assert_eq!(totals["vatCents"], 17_333);
    assert_eq!(totals["grossCents"], 101_580);
    assert_eq!(
        totals["vatByRate"],
        json!([
            { "rateBp": 900, "netCents": 2_997, "vatCents": 270 },
            { "rateBp": 2_100, "netCents": 81_250, "vatCents": 17_063 },
        ]),
        "the breakdown must be per rate, in rate order"
    );
    let lines = quote["lines"].as_array().unwrap();
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[2]["netCents"], -12_500, "a discount line is negative");
    assert!(lines[0].get("vatCents").is_none(), "{}", lines[0]);

    // --- a draft is editable, and a lines-only PATCH leaves the header -------
    let (status, body) = patch(
        &h.app,
        &h.token,
        &format!("/billing/quotes/{id}"),
        json!({ "lines": three_lines() }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["quote"]["reference"], "RFQ-2026-88");
    assert_eq!(body["quote"]["note"], "This offer stands for a fortnight.");
    assert_eq!(body["quote"]["totals"]["grossCents"], 101_580);

    // --- send it: a number from the quote series, and the dates -------------
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/billing/quotes/{id}/send"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let sent = &body["quote"];
    assert_eq!(sent["status"], "sent");
    assert_eq!(sent["number"], format!("QUO-{}-00001", today().year()));
    assert_eq!(sent["sentDate"], as_day(today()));
    assert_eq!(sent["validUntil"], as_day(today() + Duration::days(14)));
    assert_eq!(sent["decidedDate"], Value::Null);
    assert_eq!(sent["expired"], false, "it stands for a fortnight");

    // --- and it is frozen: every write verb refuses, naming the state -------
    for (status, body) in [
        patch(
            &h.app,
            &h.token,
            &format!("/billing/quotes/{id}"),
            json!({ "reference": "RFQ-9" }),
        )
        .await,
        patch(
            &h.app,
            &h.token,
            &format!("/billing/quotes/{id}"),
            json!({ "lines": [] }),
        )
        .await,
        delete(&h.app, &h.token, &format!("/billing/quotes/{id}")).await,
        post(
            &h.app,
            &h.token,
            &format!("/billing/quotes/{id}/send"),
            json!({}),
        )
        .await,
    ] {
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert!(
            body["detail"].as_str().unwrap_or_default().contains("sent"),
            "the refusal names the state: {body}"
        );
    }

    // --- accept: two documents in one answer --------------------------------
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/billing/quotes/{id}/accept"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["quote"]["status"], "accepted");
    assert_eq!(body["quote"]["decidedDate"], as_day(today()));
    assert_eq!(
        body["quote"]["number"],
        format!("QUO-{}-00001", today().year()),
        "answering keeps the offer's own number"
    );

    let invoice = &body["invoice"];
    let invoice_id = invoice["id"].as_str().unwrap().to_owned();
    // The done-when: an editable draft worth exactly the same.
    assert_eq!(invoice["status"], "draft");
    assert_eq!(invoice["number"], Value::Null);
    assert_eq!(invoice["quoteId"], id);
    assert_eq!(invoice["customerId"], customer);
    assert_eq!(invoice["currency"], "EUR");
    assert_eq!(
        invoice["reference"], "RFQ-2026-88",
        "the customer's own reference follows them onto the invoice"
    );
    assert_eq!(
        invoice["note"], "",
        "a quote's note states the terms of an offer, which a bill is not"
    );
    assert_eq!(
        invoice["paymentTermsDays"], 30,
        "a quote carries no terms: the customer's own are snapshotted"
    );
    assert_eq!(invoice["totals"], body["quote"]["totals"]);
    assert_eq!(
        invoice["lines"].as_array().unwrap().len(),
        3,
        "every line of the offer, copied"
    );
    for (copy, offered) in invoice["lines"]
        .as_array()
        .unwrap()
        .iter()
        .zip(body["quote"]["lines"].as_array().unwrap())
    {
        for field in [
            "description",
            "unit",
            "qtyMilli",
            "unitPriceCents",
            "vatRateBp",
            "netCents",
        ] {
            assert_eq!(copy[field], offered[field], "{field} was not copied");
        }
        assert_ne!(copy["id"], offered["id"], "a copied line is its own line");
    }

    // --- the link, from both ends -------------------------------------------
    let (status, body) = get(&h.app, &h.token, &format!("/billing/quotes/{id}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["invoiceId"], invoice_id);
    assert_eq!(body["quote"]["status"], "accepted");
    let (status, body) = get(&h.app, &h.token, &format!("/billing/invoices/{invoice_id}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["invoice"]["quoteId"], id);

    // --- and the draft really is a draft: editable, and issued as any other -
    let (status, body) = patch(
        &h.app,
        &h.token,
        &format!("/billing/invoices/{invoice_id}"),
        json!({ "note": "Zahlbar innerhalb von 30 Tagen" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["invoice"]["totals"]["grossCents"], 101_580);

    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/billing/invoices/{invoice_id}/issue"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["invoice"]["number"],
        format!("INV-{}-00001", today().year()),
        "the invoice series is untouched by the quote's own numbering"
    );
    assert_eq!(
        body["invoice"]["dueDate"],
        as_day(today() + Duration::days(30))
    );
    assert_eq!(body["invoice"]["quoteId"], id, "issuing keeps the origin");

    // --- accepting again is refused, and raises no second document ----------
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/billing/quotes/{id}/accept"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    let (_, list) = get(&h.app, &h.token, "/billing/invoices").await;
    assert_eq!(ids(&list, "invoices").len(), 1);
}

// ---- refusals ----------------------------------------------------------------

#[tokio::test]
async fn a_refused_request_writes_nothing_and_says_what_is_wrong() {
    let h = harness("bill-quo-refuse").await;
    common::seed_default_chart(&h.acc).await;
    let customer = a_customer(&h.app, &h.token).await;

    // A body naming no customer is about the field, not a 404 about an id it
    // never sent.
    let (status, body) = post(&h.app, &h.token, "/billing/quotes", json!({})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        body["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("customerId"),
        "{body}"
    );
    let (status, _) = post(
        &h.app,
        &h.token,
        "/billing/quotes",
        json!({ "customerId": "cust-nope" }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // A bad line is a 422 naming the line's position, and leaves no draft.
    for bad in [
        json!([{ "description": "  ", "qtyMilli": 1_000, "unitPriceCents": 100 }]),
        json!([{ "description": "X", "unitPriceCents": -1 }]),
        json!([{ "description": "X", "vatRateBp": 10_001 }]),
    ] {
        let (status, body) = post(
            &h.app,
            &h.token,
            "/billing/quotes",
            json!({ "customerId": customer, "lines": bad }),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    }
    // 19.99 in a cents field is a malformed body, never a rounded number, and
    // the answer never quotes what was sent.
    let (status, body) = post(
        &h.app,
        &h.token,
        "/billing/quotes",
        json!({ "customerId": customer,
                "lines": [{ "description": "X", "unitPriceCents": 19.99 }] }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["detail"], "malformed request body");
    let (_, list) = get(&h.app, &h.token, "/billing/quotes").await;
    assert!(
        ids(&list, "quotes").is_empty(),
        "not one refused request left a draft behind: {list}"
    );

    // An offer that says nothing cannot be sent, and stays a draft.
    let empty = created_id(
        "quote",
        post(
            &h.app,
            &h.token,
            "/billing/quotes",
            json!({ "customerId": customer }),
        )
        .await,
    );
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/billing/quotes/{empty}/send"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        body["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("no lines"),
        "{body}"
    );

    // A draft was never an offer: it cannot be answered, in any of the three
    // ways, and the refusal says what it can do instead.
    for verb in ["accept", "decline", "expire"] {
        let (status, body) = post(
            &h.app,
            &h.token,
            &format!("/billing/quotes/{empty}/{verb}"),
            json!({}),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        let detail = body["detail"].as_str().unwrap_or_default().to_owned();
        assert!(
            detail.contains("draft") && detail.contains("sent"),
            "{body}"
        );
    }
    // Nothing was billed by any of that.
    let (_, list) = get(&h.app, &h.token, "/billing/invoices").await;
    assert!(ids(&list, "invoices").is_empty(), "{list}");
    let (_, one) = get(&h.app, &h.token, &format!("/billing/quotes/{empty}")).await;
    assert_eq!(one["invoiceId"], Value::Null);

    // A validity outside the accepted range is a 422 from the store.
    let (status, body) = patch(
        &h.app,
        &h.token,
        &format!("/billing/quotes/{empty}"),
        json!({ "validDays": 400 }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");

    // The status filter is strict: an invoice's states are not a quote's.
    for bad in ["issued", "open", "draft,sent"] {
        let (status, body) = get(&h.app, &h.token, &format!("/billing/quotes?status={bad}")).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}: {body}");
    }
    let (status, _) = get(&h.app, &h.token, "/billing/quotes?status=").await;
    assert_eq!(status, StatusCode::OK, "a blank filter is simply no filter");
}

// ---- guards ------------------------------------------------------------------

#[tokio::test]
async fn every_route_needs_a_token_and_an_id_that_exists() {
    let h = harness("bill-quo-guards").await;
    common::seed_default_chart(&h.acc).await;

    // No token: the guard runs before anything is looked up, so an
    // unauthenticated caller learns nothing about which ids exist.
    for (method, uri) in [
        ("GET", "/billing/quotes"),
        ("POST", "/billing/quotes"),
        ("GET", "/billing/quotes/x"),
        ("PATCH", "/billing/quotes/x"),
        ("DELETE", "/billing/quotes/x"),
        ("POST", "/billing/quotes/x/send"),
        ("POST", "/billing/quotes/x/accept"),
        ("POST", "/billing/quotes/x/decline"),
        ("POST", "/billing/quotes/x/expire"),
    ] {
        let (status, _) = send(&h.app, with_json(method, uri, None, json!({}))).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {uri}");
    }

    // With a token, an id that was never raised is a 404 on every route.
    for (method, uri) in [
        ("GET", "/billing/quotes/quo-nope"),
        ("PATCH", "/billing/quotes/quo-nope"),
        ("DELETE", "/billing/quotes/quo-nope"),
        ("POST", "/billing/quotes/quo-nope/send"),
        ("POST", "/billing/quotes/quo-nope/accept"),
        ("POST", "/billing/quotes/quo-nope/decline"),
        ("POST", "/billing/quotes/quo-nope/expire"),
    ] {
        let (status, _) = send(&h.app, with_json(method, uri, Some(&h.token), json!({}))).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method} {uri}");
    }
}

// ---- the lapse flag ----------------------------------------------------------

#[tokio::test]
async fn a_lapsed_offer_reads_as_lapsed_and_may_still_be_accepted() {
    let h = harness("bill-quo-lapsed").await;
    common::seed_default_chart(&h.acc).await;
    let customer = a_customer(&h.app, &h.token).await;
    let id = a_sent_quote(&h.app, &h.token, &customer).await;

    // The past is planted with SQL, since the store refuses to backdate a send
    // — so the flag is tested against the stored document. The two dates move
    // as a pair: the table's own CHECK refuses an offer that expires before it
    // was made.
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url())
        .await
        .unwrap();
    sqlx::query(
        "UPDATE billing_quotes SET sent_date = CURRENT_DATE - 17, \
             valid_until = CURRENT_DATE - 3 WHERE tenant_id = $1 AND id = $2",
    )
    .bind(h.tenant.as_str())
    .bind(&id)
    .execute(&pool)
    .await
    .unwrap();

    let (status, body) = get(&h.app, &h.token, &format!("/billing/quotes/{id}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["quote"]["expired"], true, "the reader is told");
    assert_eq!(
        body["quote"]["status"], "sent",
        "no background sweep closed it behind the tenant's back"
    );
    let (_, list) = get(&h.app, &h.token, "/billing/quotes?status=sent").await;
    assert_eq!(list["quotes"][0]["expired"], true, "and on the list too");

    // Honouring it late is the tenant's decision: the store refuses on state,
    // never on a date.
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/billing/quotes/{id}/accept"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["quote"]["expired"], false, "it has its answer now");
    assert_eq!(body["invoice"]["status"], "draft");
}

// ---- the three answers, and the list ----------------------------------------

#[tokio::test]
async fn an_offer_is_answered_once_and_only_an_accepted_one_is_billed() {
    let h = harness("bill-quo-answers").await;
    common::seed_default_chart(&h.acc).await;
    let customer = a_customer(&h.app, &h.token).await;

    let accepted = a_sent_quote(&h.app, &h.token, &customer).await;
    let declined = a_sent_quote(&h.app, &h.token, &customer).await;
    let expired = a_sent_quote(&h.app, &h.token, &customer).await;
    for (id, verb) in [(&declined, "decline"), (&expired, "expire")] {
        let (status, body) = post(
            &h.app,
            &h.token,
            &format!("/billing/quotes/{id}/{verb}"),
            json!({}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["quote"]["decidedDate"], as_day(today()));
        assert!(
            body.get("invoice").is_none(),
            "a turned-down offer produces no document: {body}"
        );
    }
    let (_, body) = post(
        &h.app,
        &h.token,
        &format!("/billing/quotes/{accepted}/accept"),
        json!({}),
    )
    .await;
    let invoice_id = body["invoice"]["id"].as_str().unwrap().to_owned();

    // Closed is closed: every further move on any of the three is a 409, and
    // none of them raises a document.
    for id in [&accepted, &declined, &expired] {
        for verb in ["send", "accept", "decline", "expire"] {
            let (status, body) = post(
                &h.app,
                &h.token,
                &format!("/billing/quotes/{id}/{verb}"),
                json!({}),
            )
            .await;
            assert_eq!(status, StatusCode::CONFLICT, "{id}/{verb}: {body}");
        }
        let (status, _) = delete(&h.app, &h.token, &format!("/billing/quotes/{id}")).await;
        assert_eq!(status, StatusCode::CONFLICT);
    }
    let (_, list) = get(&h.app, &h.token, "/billing/invoices").await;
    assert_eq!(ids(&list, "invoices"), vec![invoice_id.clone()]);

    // Only the accepted offer names an invoice.
    for (id, expected) in [
        (&accepted, Value::String(invoice_id)),
        (&declined, Value::Null),
        (&expired, Value::Null),
    ] {
        let (_, body) = get(&h.app, &h.token, &format!("/billing/quotes/{id}")).await;
        assert_eq!(body["invoiceId"], expected, "{id}");
    }

    // The status filter partitions the three, newest first, and a draft that
    // nobody sent is deleted like any other.
    let draft = created_id(
        "quote",
        post(
            &h.app,
            &h.token,
            "/billing/quotes",
            json!({ "customerId": customer }),
        )
        .await,
    );
    for (filter, expected) in [
        ("accepted", vec![accepted.clone()]),
        ("declined", vec![declined.clone()]),
        ("expired", vec![expired.clone()]),
        ("draft", vec![draft.clone()]),
        ("sent", vec![]),
    ] {
        let (status, body) = get(
            &h.app,
            &h.token,
            &format!("/billing/quotes?status={filter}"),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(ids(&body, "quotes"), expected, "?status={filter}");
    }
    let (_, all) = get(&h.app, &h.token, "/billing/quotes").await;
    assert_eq!(ids(&all, "quotes").len(), 4);
    let (status, _) = delete(&h.app, &h.token, &format!("/billing/quotes/{draft}")).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = get(&h.app, &h.token, &format!("/billing/quotes/{draft}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ---- tenancy (the mandatory wrong-tenant proof) ------------------------------

#[tokio::test]
async fn another_tenants_offer_is_invisible_and_untouchable_on_every_route() {
    let a = harness("bill-quo-tenant-a").await;
    common::seed_default_chart(&a.acc).await;
    let b = harness("bill-quo-tenant-b").await;
    common::seed_default_chart(&b.acc).await;
    let customer_b = a_customer(&b.app, &b.token).await;
    let quote_b = a_sent_quote(&b.app, &b.token, &customer_b).await;

    // A holds B's id — the strongest position an attacker reaches.
    for (method, uri) in [
        ("GET", format!("/billing/quotes/{quote_b}")),
        ("PATCH", format!("/billing/quotes/{quote_b}")),
        ("DELETE", format!("/billing/quotes/{quote_b}")),
        ("POST", format!("/billing/quotes/{quote_b}/send")),
        ("POST", format!("/billing/quotes/{quote_b}/accept")),
        ("POST", format!("/billing/quotes/{quote_b}/decline")),
        ("POST", format!("/billing/quotes/{quote_b}/expire")),
    ] {
        let (status, body) = send(
            &a.app,
            with_json(
                method,
                &uri,
                Some(&a.token),
                json!({ "reference": "stolen" }),
            ),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "{method} {uri} must be 404, never 409 — that would confirm the id \
             exists and leak its state: {body}"
        );
        assert!(
            !body.to_string().contains("RFQ-2026-88"),
            "no refusal echoes what it refused: {body}"
        );
    }

    // A's lists never mention it, on any filter.
    for filter in ["", "?status=sent", "?status=accepted"] {
        let (_, list) = get(&a.app, &a.token, &format!("/billing/quotes{filter}")).await;
        assert!(ids(&list, "quotes").is_empty(), "{filter}: {list}");
    }
    // A cannot make an offer to B's customer either.
    let (status, _) = post(
        &a.app,
        &a.token,
        "/billing/quotes",
        json!({ "customerId": customer_b, "lines": three_lines() }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // B's offer is untouched, and B accepts it themselves — the invoice lands
    // in B's tenant, and A can see neither it nor the link.
    let (_, body) = post(
        &b.app,
        &b.token,
        &format!("/billing/quotes/{quote_b}/accept"),
        json!({}),
    )
    .await;
    assert_eq!(body["quote"]["status"], "accepted", "{body}");
    let invoice_b = body["invoice"]["id"].as_str().unwrap().to_owned();
    let (status, _) = get(&a.app, &a.token, &format!("/billing/invoices/{invoice_b}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (_, list) = get(&a.app, &a.token, "/billing/invoices").await;
    assert!(ids(&list, "invoices").is_empty(), "{list}");

    // And B's series is untouched by A's attempts: their next offer is number
    // two, so nothing A did consumed one of B's numbers.
    let next = a_sent_quote(&b.app, &b.token, &customer_b).await;
    let (_, body) = get(&b.app, &b.token, &format!("/billing/quotes/{next}")).await;
    assert_eq!(
        body["quote"]["number"],
        format!("QUO-{}-00002", today().year())
    );
}
