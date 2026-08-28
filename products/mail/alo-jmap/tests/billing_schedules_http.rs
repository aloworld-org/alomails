//! The `/billing/schedules` HTTP surface (B2.11), driven through the real
//! router over a real Postgres.
//!
//! `alo-store`'s own suite proves the arrangement works; what this suite is for
//! is the **edge**: that the arc a bookkeeper walks — set one up from an
//! invoice, run what is due, see the drafts, pause it, delete the one that
//! never billed — comes back over the wire with the codes
//! `docs/design/billing.md` publishes; that a run raises **drafts** and never a
//! numbered document; that the fields a client must not be able to set are
//! ignored; and that another tenant's arrangement is invisible and untouchable
//! on every route, answering exactly as an id that never existed.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use crate::common::{harness, send};

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

/// A customer on 14-day terms in euro.
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
                "paymentTermsDays": 14,
                "currency": "EUR",
            }),
        )
        .await,
    )
}

/// Two lines across two VAT rates, one with a fractional quantity — a template
/// whose totals only come out right if the copy is exact.
fn lines() -> Value {
    json!([
        { "description": "Hosting", "unit": "month", "qtyMilli": 1_000,
          "unitPriceCents": 9_900, "vatRateBp": 2100 },
        { "description": "Support", "unit": "hour", "qtyMilli": 2_500,
          "unitPriceCents": 8_000, "vatRateBp": 900 },
    ])
}

/// Today as the routes spell dates, so a schedule set up here is due at once.
fn today() -> String {
    let now = time::OffsetDateTime::now_utc().date();
    format!(
        "{:04}-{:02}-{:02}",
        now.year(),
        u8::from(now.month()),
        now.day()
    )
}

/// A monthly arrangement for `customer`, due today.
async fn a_schedule(app: &Router, token: &str, customer: &str, name: &str) -> String {
    created_id(
        "schedule",
        post(
            app,
            token,
            "/billing/schedules",
            json!({
                "customerId": customer,
                "name": name,
                "cadence": "monthly",
                "startDate": today(),
                "reference": "PO-2026",
                "note": "Thank you for your business",
                "lines": lines(),
            }),
        )
        .await,
    )
}

#[tokio::test]
async fn the_arc_a_bookkeeper_walks_comes_back_over_the_wire() {
    let h = harness("sched-arc").await;
    let (app, token) = (&h.app, h.token.as_str());
    let customer = a_customer(app, token).await;

    // Set one up: the response is the stored arrangement, with the derived
    // flags a screen reads and the totals of ONE occurrence.
    let (status, body) = post(
        app,
        token,
        "/billing/schedules",
        json!({
            "customerId": customer,
            "name": "Hosting — monthly",
            "cadence": "monthly",
            "startDate": today(),
            "reference": "PO-2026",
            "lines": lines(),
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let schedule = body["schedule"].clone();
    let id = schedule["id"].as_str().unwrap().to_owned();
    assert_eq!(schedule["cadence"], "monthly");
    assert_eq!(schedule["active"], true);
    assert_eq!(schedule["due"], true, "starting today is due today");
    assert_eq!(schedule["ended"], false);
    assert_eq!(schedule["raisedCount"], 0);
    assert_eq!(schedule["nextRunDate"], today());
    // The currency and terms fall back to the customer's own, then stay.
    assert_eq!(schedule["currency"], "EUR");
    assert_eq!(schedule["paymentTermsDays"], 14);
    // 99.00 @ 21 % + 200.00 @ 9 % = 299.00 net, 38.79 VAT — the server's
    // arithmetic, per rate, and nothing the client sent.
    assert_eq!(schedule["totals"]["netCents"], 29_900);
    assert_eq!(schedule["totals"]["grossCents"], 33_779);
    assert_eq!(schedule["lines"].as_array().unwrap().len(), 2);

    // Run what is due: DRAFTS, never a numbered document.
    let (status, body) = post(app, token, "/billing/schedules/run", json!({})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let raised = body["invoices"].as_array().unwrap().clone();
    assert_eq!(raised.len(), 1, "{body}");
    let invoice = &raised[0];
    assert_eq!(invoice["status"], "draft");
    assert_eq!(invoice["number"], Value::Null);
    assert_eq!(invoice["issueDate"], Value::Null);
    assert_eq!(invoice["scheduleId"], id.as_str());
    assert_eq!(invoice["scheduleDueDate"], today());
    // The copy is exact, and the header is the arrangement's snapshot.
    assert_eq!(invoice["totals"]["grossCents"], 33_779);
    assert_eq!(invoice["reference"], "PO-2026");
    assert_eq!(invoice["paymentTermsDays"], 14);
    assert_eq!(invoice["lines"].as_array().unwrap().len(), 2);

    // Running again the same day raises nothing — an occurrence bills once.
    let (status, body) = post(app, token, "/billing/schedules/run", json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body["invoices"].as_array().unwrap().is_empty(),
        "the same day billed twice: {body}"
    );

    // The arrangement now knows what it did, and the drafts hang off it.
    let (status, body) = get(app, token, &format!("/billing/schedules/{id}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["schedule"]["raisedCount"], 1);
    assert_eq!(body["schedule"]["lastRunDate"], today());
    assert_eq!(body["schedule"]["due"], false);
    assert_ne!(body["schedule"]["nextRunDate"], today());
    assert_eq!(body["invoices"].as_array().unwrap().len(), 1);

    // And the invoice list carries the provenance the badge is drawn from.
    let (status, body) = get(app, token, "/billing/invoices").await;
    assert_eq!(status, StatusCode::OK);
    let listed = body["invoices"].as_array().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0]["scheduleId"], id.as_str());

    // Pausing keeps every date and stops every run.
    let (status, body) = post(
        app,
        token,
        &format!("/billing/schedules/{id}/pause"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["schedule"]["active"], false);
    assert_eq!(body["schedule"]["due"], false, "a paused one is never due");
    let (_, resumed) = post(
        app,
        token,
        &format!("/billing/schedules/{id}/resume"),
        json!({}),
    )
    .await;
    assert_eq!(resumed["schedule"]["active"], true);

    // One that has raised documents is paused, not deleted.
    let (status, body) = delete(app, token, &format!("/billing/schedules/{id}")).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        body["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("pause"),
        "{body}"
    );

    // One that never billed is deleted cleanly.
    let unused = a_schedule(app, token, &customer, "Never run").await;
    let (status, _) = post(
        app,
        token,
        &format!("/billing/schedules/{unused}/pause"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = delete(app, token, &format!("/billing/schedules/{unused}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, _) = get(app, token, &format!("/billing/schedules/{unused}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_edge_refuses_what_it_cannot_bill_and_ignores_what_it_must_not_set() {
    let h = harness("sched-refuse").await;
    let (app, token) = (&h.app, h.token.as_str());
    let customer = a_customer(app, token).await;

    // The two facts a request has to state, each named rather than falling
    // through into a confusing later failure.
    for (body, expected) in [
        (
            json!({ "cadence": "monthly", "startDate": today(), "lines": lines() }),
            "customerId",
        ),
        (
            json!({ "customerId": &customer, "cadence": "monthly", "lines": lines() }),
            "startDate",
        ),
    ] {
        let (status, answer) = post(app, token, "/billing/schedules", body).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{answer}");
        assert!(
            answer["detail"]
                .as_str()
                .unwrap_or_default()
                .contains(expected),
            "{answer}"
        );
    }

    // A cadence is never defaulted: absent and unknown are both 422.
    for cadence in [Value::Null, json!("daily"), json!("")] {
        let (status, answer) = post(
            app,
            token,
            "/billing/schedules",
            json!({ "customerId": &customer, "name": "X", "cadence": cadence,
                    "startDate": today(), "lines": lines() }),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{answer}");
    }

    // A template is required — the store's own sentence, over the wire.
    let (status, answer) = post(
        app,
        token,
        "/billing/schedules",
        json!({ "customerId": &customer, "name": "Empty", "cadence": "monthly",
                "startDate": today(), "lines": [] }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{answer}");
    assert!(
        answer["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("at least one line"),
        "{answer}"
    );

    // A date that is not a date is refused, never silently ignored.
    let (status, _) = post(
        app,
        token,
        "/billing/schedules",
        json!({ "customerId": &customer, "name": "Bad date", "cadence": "monthly",
                "startDate": "31/01/2027", "lines": lines() }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    let id = a_schedule(app, token, &customer, "Ignoring test").await;
    let (_, before) = get(app, token, &format!("/billing/schedules/{id}")).await;
    let next = before["schedule"]["nextRunDate"].clone();

    // The fields a client must not be able to set are ignored like any unknown
    // one: a settable next date could bill a period twice, and pausing is its
    // own route.
    let (status, body) = patch(
        app,
        token,
        &format!("/billing/schedules/{id}"),
        json!({
            "nextRunDate": "2020-01-01", "active": false, "anchorDay": 9,
            "customerId": "somebody-else", "currency": "USD", "paymentTermsDays": 90,
            "startDate": "2020-01-01", "raisedCount": 99,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["schedule"]["nextRunDate"], next);
    assert_eq!(body["schedule"]["active"], true);
    assert_eq!(body["schedule"]["customerId"], customer.as_str());
    assert_eq!(body["schedule"]["currency"], "EUR");
    assert_eq!(body["schedule"]["paymentTermsDays"], 14);

    // What IS editable: the name, the rhythm, the end date, and the template.
    // Changing the cadence deliberately does not move the date already
    // scheduled — the new rhythm applies from the one after it.
    let (status, body) = patch(
        app,
        token,
        &format!("/billing/schedules/{id}"),
        json!({ "name": "Renamed", "cadence": "quarterly", "endDate": "2030-12-31" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["schedule"]["name"], "Renamed");
    assert_eq!(body["schedule"]["cadence"], "quarterly");
    assert_eq!(body["schedule"]["endDate"], "2030-12-31");
    assert_eq!(body["schedule"]["nextRunDate"], next);

    // `null` clears the end date; an absent field would have kept it.
    let (_, body) = patch(
        app,
        token,
        &format!("/billing/schedules/{id}"),
        json!({ "endDate": null }),
    )
    .await;
    assert_eq!(body["schedule"]["endDate"], Value::Null);

    // An end date before the start is the store's refusal, mapped to 422.
    let (status, answer) = patch(
        app,
        token,
        &format!("/billing/schedules/{id}"),
        json!({ "endDate": "2020-01-01" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{answer}");
}

#[tokio::test]
async fn the_door_is_shut_without_a_token_and_to_another_tenant() {
    let alpha = harness("sched-alpha").await;
    let beta = harness("sched-beta").await;
    let customer = a_customer(&alpha.app, &alpha.token).await;
    let id = a_schedule(&alpha.app, &alpha.token, &customer, "Alpha hosting").await;

    // No token at all: every route, including the run.
    for (method, uri) in [
        ("GET", "/billing/schedules".to_owned()),
        ("POST", "/billing/schedules".to_owned()),
        ("POST", "/billing/schedules/run".to_owned()),
        ("GET", format!("/billing/schedules/{id}")),
        ("PATCH", format!("/billing/schedules/{id}")),
        ("DELETE", format!("/billing/schedules/{id}")),
        ("POST", format!("/billing/schedules/{id}/pause")),
        ("POST", format!("/billing/schedules/{id}/resume")),
    ] {
        let (status, _) = send(&alpha.app, with_json(method, &uri, None, json!({}))).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {uri}");
    }

    // Beta's token, alpha's arrangement: `404` on every route, exactly as an id
    // that never existed — never a `403`, which would confirm it exists.
    let (beta_app, beta_token) = (&beta.app, beta.token.as_str());
    assert_eq!(
        get(beta_app, beta_token, &format!("/billing/schedules/{id}"))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        patch(
            beta_app,
            beta_token,
            &format!("/billing/schedules/{id}"),
            json!({ "name": "Taken over" }),
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        delete(beta_app, beta_token, &format!("/billing/schedules/{id}"))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    for verb in ["pause", "resume"] {
        assert_eq!(
            post(
                beta_app,
                beta_token,
                &format!("/billing/schedules/{id}/{verb}"),
                json!({}),
            )
            .await
            .0,
            StatusCode::NOT_FOUND,
            "{verb}"
        );
    }
    // A ghost id answers identically, so the two are indistinguishable.
    assert_eq!(
        get(beta_app, beta_token, "/billing/schedules/never-existed")
            .await
            .0,
        StatusCode::NOT_FOUND
    );

    // Beta's list is empty, and beta's run raises nothing of alpha's.
    let (status, body) = get(beta_app, beta_token, "/billing/schedules").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["schedules"].as_array().unwrap().is_empty(), "{body}");
    let (status, body) = post(beta_app, beta_token, "/billing/schedules/run", json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["invoices"].as_array().unwrap().is_empty(), "{body}");

    // Alpha's arrangement is untouched, and still raises for alpha alone.
    let (_, body) = get(
        &alpha.app,
        &alpha.token,
        &format!("/billing/schedules/{id}"),
    )
    .await;
    assert_eq!(body["schedule"]["name"], "Alpha hosting");
    assert_eq!(body["schedule"]["active"], true);
    assert_eq!(body["schedule"]["raisedCount"], 0);

    let (_, body) = post(
        &alpha.app,
        &alpha.token,
        "/billing/schedules/run",
        json!({}),
    )
    .await;
    assert_eq!(body["invoices"].as_array().unwrap().len(), 1);
    let (_, beta_invoices) = get(beta_app, beta_token, "/billing/invoices").await;
    assert!(
        beta_invoices["invoices"].as_array().unwrap().is_empty(),
        "alpha's run raised a document for beta: {beta_invoices}"
    );

    // A schedule cannot be pointed at another tenant's customer either.
    let (status, _) = post(
        beta_app,
        beta_token,
        "/billing/schedules",
        json!({ "customerId": &customer, "name": "Somebody else's customer",
                "cadence": "monthly", "startDate": today(), "lines": lines() }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
