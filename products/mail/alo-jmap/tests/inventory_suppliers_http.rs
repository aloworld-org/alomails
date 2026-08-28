//! The `/inventory/suppliers` HTTP surface (B5.03), driven through the real
//! router over a real Postgres.
//!
//! What this suite is for: the routes are the door the inventory module and the
//! purchasing flows both come through, so what matters is not that the store
//! works — `alo-store`'s own suites prove that — but that the **edge** behaves:
//! the auth guard, the status codes `docs/design/inventory.md` publishes, the
//! merge semantics of a partial `PATCH` against the full statement a `PUT`
//! makes, and above all that a supplier or an offer belonging to another tenant
//! is invisible and untouchable on every verb, answering exactly as an id that
//! never existed.

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

async fn put(app: &Router, token: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    send(app, with_json("PUT", uri, Some(token), body)).await
}

async fn get(app: &Router, token: &str, uri: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("GET")
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    send(app, req).await
}

async fn delete(app: &Router, token: &str, uri: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("DELETE")
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    send(app, req).await
}

/// The id of a created resource, or a panic naming the status that came back
/// instead — a failed create otherwise shows up as a confusing later failure.
fn created_id(kind: &str, (status, body): (StatusCode, Value)) -> String {
    assert_eq!(status, StatusCode::OK, "create failed: {body}");
    body[kind]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("no {kind} id in {body}"))
        .to_owned()
}

fn hoffmann() -> Value {
    json!({
        "name": "  Hoffmann Möbel GmbH  ",
        "addressLine1": "Industriestraße 4",
        "postalCode": "50733",
        "city": " Köln ",
        "country": "de",
        "vatId": " de 811.907-980 ",
        "registrationNo": "HRB 12345",
        "email": "  orders@hoffmann.test ",
        "phone": "+49 221 123456",
        "iban": "nl91 abna 0417 1643 00",
        "paymentTermsDays": 14,
        "leadTimeDays": 9,
        "note": "Ask for Frau Berger"
    })
}

#[tokio::test]
async fn a_supplier_carries_its_terms_and_refuses_what_the_caller_can_fix() {
    let h = harness("inv-supp").await;

    // ---- the auth guard, before anything else ----------------------------
    for req in [
        with_json("POST", "/inventory/suppliers", None, hoffmann()),
        Request::builder()
            .method("GET")
            .uri("/inventory/suppliers")
            .body(Body::empty())
            .unwrap(),
    ] {
        let (status, _) = send(&h.app, req).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "no token, no answer");
    }

    // ---- create: the store's normalisation, echoed back ------------------
    let (status, body) = post(&h.app, &h.token, "/inventory/suppliers", hoffmann()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let s = &body["supplier"];
    assert_eq!(s["name"], "Hoffmann Möbel GmbH");
    assert_eq!(s["city"], "Köln");
    assert_eq!(s["country"], "DE");
    assert_eq!(s["vatId"], "DE811907980");
    assert_eq!(s["iban"], "NL91ABNA0417164300");
    assert_eq!(s["email"], "orders@hoffmann.test");
    assert_eq!(s["currency"], "EUR");
    assert_eq!(s["paymentTermsDays"], 14);
    assert_eq!(s["leadTimeDays"], 9);
    assert_eq!(s["archived"], false);
    let id = s["id"].as_str().unwrap().to_owned();

    // ---- the refusals, each naming its rule and never the value ----------
    for (bad, rule) in [
        (json!({"name": "   ", "country": "DE"}), "name"),
        (json!({"name": "X", "country": "Germany"}), "country"),
        (
            json!({"name": "X", "country": "DE", "vatId": "DE811907981"}),
            "check digit",
        ),
        (
            json!({"name": "X", "country": "DE", "iban": "NL92ABNA0417164300"}),
            "IBAN",
        ),
        (
            json!({"name": "X", "country": "DE", "leadTimeDays": 400}),
            "lead time",
        ),
    ] {
        let (status, body) = post(&h.app, &h.token, "/inventory/suppliers", bad.clone()).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{bad} → {body}");
        let detail = body["detail"].as_str().unwrap_or_default();
        assert!(detail.contains(rule), "{detail} should name {rule}");
        assert!(
            !detail.contains("811907981") && !detail.contains("ABNA0417164300"),
            "the refusal echoed the value: {detail}"
        );
    }

    // ---- PATCH merges; the nullable fields can be cleared ----------------
    let (status, body) = patch(
        &h.app,
        &h.token,
        &format!("/inventory/suppliers/{id}"),
        json!({ "leadTimeDays": 21 }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["supplier"]["leadTimeDays"], 21);
    assert_eq!(
        body["supplier"]["name"], "Hoffmann Möbel GmbH",
        "a partial PATCH left the rest alone"
    );
    let (_, body) = patch(
        &h.app,
        &h.token,
        &format!("/inventory/suppliers/{id}"),
        json!({ "vatId": null, "iban": "" }),
    )
    .await;
    assert_eq!(body["supplier"]["vatId"], Value::Null);
    assert_eq!(body["supplier"]["iban"], Value::Null);

    // ---- archive is its own POST, and it is idempotent -------------------
    // An empty body archives: the route's name is already the intent (the
    // shape `/billing/products/{id}/archive` established).
    let (status, body) = send(
        &h.app,
        Request::builder()
            .method("POST")
            .uri(format!("/inventory/suppliers/{id}/archive"))
            .header("authorization", format!("Bearer {}", h.token))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["supplier"]["archived"], true);
    let stamp = body["supplier"]["archivedAt"].clone();
    let (_, body) = post(
        &h.app,
        &h.token,
        &format!("/inventory/suppliers/{id}/archive"),
        json!({ "archived": true }),
    )
    .await;
    assert_eq!(body["supplier"]["archivedAt"], stamp, "no restamping");
    let (_, body) = get(&h.app, &h.token, "/inventory/suppliers").await;
    assert_eq!(
        body["suppliers"].as_array().unwrap().len(),
        0,
        "archived suppliers leave the picker"
    );
    let (_, body) = get(&h.app, &h.token, "/inventory/suppliers?includeArchived=1").await;
    assert_eq!(body["suppliers"].as_array().unwrap().len(), 1);

    // ---- an id that never existed ----------------------------------------
    let (status, _) = get(&h.app, &h.token, "/inventory/suppliers/nope").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = patch(
        &h.app,
        &h.token,
        "/inventory/suppliers/nope",
        json!({"name": "X"}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn an_offer_is_an_idempotent_put_and_never_crosses_a_tenant() {
    let a = harness("inv-off-a").await;
    let b = harness("inv-off-b").await;

    let supplier = created_id(
        "supplier",
        post(&a.app, &a.token, "/inventory/suppliers", hoffmann()).await,
    );
    let chair = created_id(
        "product",
        post(
            &a.app,
            &a.token,
            "/billing/products",
            json!({"name": "Blue chair", "unit": "piece", "stocked": true}),
        )
        .await,
    );

    // ---- PUT states the whole offer --------------------------------------
    let uri = format!("/inventory/suppliers/{supplier}/products/{chair}");
    let (status, body) = put(
        &a.app,
        &a.token,
        &uri,
        json!({
            "supplierCode": " HM-4471 ",
            "purchasePriceCents": 315,
            "currency": "eur",
            "minOrderQtyMilli": 10_000,
            "leadTimeDays": 9
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let offer = &body["offer"];
    assert_eq!(offer["productName"], "Blue chair");
    assert_eq!(offer["supplierCode"], "HM-4471");
    assert_eq!(offer["purchasePriceCents"], 315);
    assert_eq!(offer["currency"], "EUR");
    assert_eq!(offer["leadTimeDays"], 9);
    assert_eq!(offer["effectiveLeadTimeDays"], 9);

    // The same call twice leaves one row saying the same thing.
    let (_, body) = put(&a.app, &a.token, &uri, json!({ "purchasePriceCents": 299 })).await;
    assert_eq!(body["offer"]["purchasePriceCents"], 299);
    assert_eq!(
        body["offer"]["supplierCode"], "",
        "a PUT states the whole offer; it never merges"
    );
    assert_eq!(
        body["offer"]["effectiveLeadTimeDays"], 9,
        "and falls back to the supplier's own lead time"
    );
    let (_, body) = get(
        &a.app,
        &a.token,
        &format!("/inventory/suppliers/{supplier}/products"),
    )
    .await;
    assert_eq!(
        body["offers"].as_array().unwrap().len(),
        1,
        "a re-quote replaces, never accumulates"
    );

    // ---- money is integers on the wire -----------------------------------
    let (status, _) = put(&a.app, &a.token, &uri, json!({"purchasePriceCents": 3.15})).await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "a decimal price is refused, never rounded"
    );
    let (status, body) = put(&a.app, &a.token, &uri, json!({"currency": "EURO"})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");

    // ---- the product's default supplier goes through the same gate -------
    let (status, body) = patch(
        &a.app,
        &a.token,
        &format!("/billing/products/{chair}"),
        json!({ "defaultSupplierId": supplier }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["product"]["defaultSupplierId"], supplier);

    // ---- tenant B: everything of A's is a 404 on every verb --------------
    let b_supplier = created_id(
        "supplier",
        post(&b.app, &b.token, "/inventory/suppliers", hoffmann()).await,
    );
    for (method, uri) in [
        ("GET", format!("/inventory/suppliers/{supplier}")),
        ("GET", format!("/inventory/suppliers/{supplier}/products")),
    ] {
        let (status, _) = if method == "GET" {
            get(&b.app, &b.token, &uri).await
        } else {
            unreachable!()
        };
        assert_eq!(status, StatusCode::NOT_FOUND, "{method} {uri} leaked");
    }
    let (status, _) = put(&b.app, &b.token, &uri, json!({})).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "B wrote into A's price list");
    let (status, _) = delete(&b.app, &b.token, &uri).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = patch(
        &b.app,
        &b.token,
        &format!("/inventory/suppliers/{supplier}"),
        json!({"name": "Stolen"}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    // A pointing at B's supplier is the same refusal, from the other side.
    let (status, _) = patch(
        &a.app,
        &a.token,
        &format!("/billing/products/{chair}"),
        json!({ "defaultSupplierId": b_supplier }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    // …and A's own offer is exactly as it was.
    let (_, body) = get(
        &a.app,
        &a.token,
        &format!("/inventory/suppliers/{supplier}/products"),
    )
    .await;
    assert_eq!(body["offers"].as_array().unwrap().len(), 1);
    assert_eq!(body["offers"][0]["purchasePriceCents"], 299);

    // ---- removing an offer, twice ----------------------------------------
    let (status, body) = delete(&a.app, &a.token, &uri).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["removed"], true);
    let (status, _) = delete(&a.app, &a.token, &uri).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "there is nothing left to remove"
    );
}
