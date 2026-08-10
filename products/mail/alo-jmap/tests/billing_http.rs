//! The `/billing/customers` and `/billing/products` HTTP surface (B1.05),
//! driven through the real router over a real Postgres.
//!
//! What this suite is for: the routes are the door the web module and the
//! billing agent both come through, so what matters is not that the store
//! works — `alo-store`'s own suites prove that — but that the **edge** behaves:
//! the auth guard, the status codes `docs/design/billing.md` publishes, the
//! merge semantics of a partial `PATCH`, and above all that a customer or a
//! price belonging to another tenant is invisible and untouchable on every
//! verb, answering exactly as an id that never existed.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use alo_store::model::Contact;
use alo_store::{ContactId, StoreError};

use common::{harness, send};

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
    let req = Request::builder()
        .method("GET")
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

fn names(body: &Value, kind: &str) -> Vec<String> {
    body[kind]
        .as_array()
        .unwrap_or_else(|| panic!("no {kind} array in {body}"))
        .iter()
        .map(|c| c["name"].as_str().unwrap_or_default().to_owned())
        .collect()
}

fn acme() -> Value {
    json!({
        "name": "  Acme GmbH  ",
        "addressLine1": "Hauptstraße 1",
        "postalCode": "10115",
        "city": " Berlin ",
        "country": "de",
        "vatId": " de 811.907-980 ",
        "email": " billing@acme.test ",
        "paymentTermsDays": 14,
        "currency": "eur",
    })
}

// ---- the customer arc --------------------------------------------------------

#[tokio::test]
async fn customer_arc_creates_lists_reads_updates_and_archives() {
    let h = harness("bill-cust-arc").await;

    // Create — the response is the STORED record, so it shows the canonical
    // form rather than the text that was sent.
    let (status, body) = post(&h.app, &h.token, "/billing/customers", acme()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let c = &body["customer"];
    assert_eq!(c["name"], "Acme GmbH");
    assert_eq!(c["city"], "Berlin");
    assert_eq!(c["country"], "DE");
    assert_eq!(c["currency"], "EUR");
    assert_eq!(c["vatId"], "DE811907980");
    assert_eq!(c["email"], "billing@acme.test");
    assert_eq!(c["paymentTermsDays"], 14);
    assert_eq!(c["archived"], false);
    assert_eq!(c["archivedAt"], Value::Null);
    assert!(c["createdAt"].as_str().is_some_and(|s| s.contains('T')));
    let id = c["id"].as_str().unwrap().to_owned();

    // List.
    let (status, body) = get(&h.app, &h.token, "/billing/customers").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(names(&body, "customers"), vec!["Acme GmbH".to_owned()]);

    // Read one.
    let (status, body) = get(&h.app, &h.token, &format!("/billing/customers/{id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["customer"]["id"], id.as_str());

    // PATCH: stated fields change, unstated ones survive untouched.
    let (status, body) = patch(
        &h.app,
        &h.token,
        &format!("/billing/customers/{id}"),
        json!({ "city": "Hamburg", "paymentTermsDays": 30 }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let c = &body["customer"];
    assert_eq!(c["city"], "Hamburg");
    assert_eq!(c["paymentTermsDays"], 30);
    assert_eq!(c["name"], "Acme GmbH");
    assert_eq!(c["vatId"], "DE811907980");
    assert_eq!(c["postalCode"], "10115");

    // Archive: gone from the default list, still readable by id, back with
    // the flag — and restoring puts it in the list again.
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/billing/customers/{id}/archive"),
        json!({ "archived": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["customer"]["archived"], true);
    assert!(body["customer"]["archivedAt"].is_string());

    let (_, body) = get(&h.app, &h.token, "/billing/customers").await;
    assert!(names(&body, "customers").is_empty(), "{body}");
    let (_, body) = get(&h.app, &h.token, "/billing/customers?includeArchived=1").await;
    assert_eq!(names(&body, "customers"), vec!["Acme GmbH".to_owned()]);
    let (status, body) = get(&h.app, &h.token, &format!("/billing/customers/{id}")).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "an archived customer stays nameable"
    );
    assert_eq!(body["customer"]["archived"], true);

    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/billing/customers/{id}/archive"),
        json!({ "archived": false }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["customer"]["archived"], false);
    assert_eq!(body["customer"]["archivedAt"], Value::Null);
    let (_, body) = get(&h.app, &h.token, "/billing/customers").await;
    assert_eq!(names(&body, "customers"), vec!["Acme GmbH".to_owned()]);
}

#[tokio::test]
async fn an_empty_archive_body_archives_and_re_archiving_is_idempotent() {
    let h = harness("bill-cust-arch").await;
    let id = created_id(
        "customer",
        post(&h.app, &h.token, "/billing/customers", acme()).await,
    );
    let path = format!("/billing/customers/{id}/archive");

    let req = Request::builder()
        .method("POST")
        .uri(&path)
        .header("authorization", format!("Bearer {}", h.token))
        .body(Body::empty())
        .unwrap();
    let (status, body) = send(&h.app, req).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["customer"]["archived"], true);
    let first = body["customer"]["archivedAt"].clone();

    let (status, body) = post(&h.app, &h.token, &path, json!({ "archived": true })).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["customer"]["archivedAt"], first,
        "re-archiving must keep the original time"
    );
}

#[tokio::test]
async fn a_patch_can_clear_a_vat_id_that_was_entered_by_mistake() {
    let h = harness("bill-cust-clear").await;
    let id = created_id(
        "customer",
        post(&h.app, &h.token, "/billing/customers", acme()).await,
    );
    let path = format!("/billing/customers/{id}");

    // Explicit null clears; the neighbouring nullable is untouched.
    let (status, body) = patch(&h.app, &h.token, &path, json!({ "vatId": null })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["customer"]["vatId"], Value::Null);
    assert_eq!(body["customer"]["email"], "billing@acme.test");

    // A blank string is the same intent — that is what a cleared form field
    // sends.
    let (status, body) = patch(&h.app, &h.token, &path, json!({ "email": "" })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["customer"]["email"], Value::Null);

    // And an untouched PATCH really does touch nothing.
    let (status, body) = patch(&h.app, &h.token, &path, json!({})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["customer"]["name"], "Acme GmbH");
    assert_eq!(body["customer"]["city"], "Berlin");
}

#[tokio::test]
async fn a_foreign_vat_registration_is_accepted_as_written() {
    // A Dutch-addressed company really can invoice under a German
    // registration, so an id that names its own country and is valid for it is
    // stored as written (B1.03). This is a rule the routes must not quietly
    // tighten: refusing it would block a real, common cross-border customer.
    let h = harness("bill-cust-foreign").await;
    let mut body = acme();
    body["country"] = json!("NL");
    let (status, answer) = post(&h.app, &h.token, "/billing/customers", body).await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(answer["customer"]["country"], "NL");
    assert_eq!(answer["customer"]["vatId"], "DE811907980");
}

// ---- the product arc ---------------------------------------------------------

#[tokio::test]
async fn product_arc_creates_lists_updates_and_archives() {
    let h = harness("bill-prod-arc").await;

    let (status, body) = post(
        &h.app,
        &h.token,
        "/billing/products",
        json!({ "name": " Consulting ", "unit": "hour", "unitPriceCents": 12_500, "vatRateBp": 2100 }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let p = &body["product"];
    assert_eq!(p["name"], "Consulting");
    assert_eq!(p["unit"], "hour");
    assert_eq!(p["unitPriceCents"], 12_500);
    assert_eq!(p["vatRateBp"], 2100);
    assert_eq!(p["archived"], false);
    let id = p["id"].as_str().unwrap().to_owned();

    // Cents survive the round trip exactly — no float ever touches the wire.
    let (_, body) = get(&h.app, &h.token, "/billing/products").await;
    assert_eq!(body["products"][0]["unitPriceCents"], 12_500);
    assert!(
        body["products"][0]["unitPriceCents"].is_i64(),
        "a price must stay an integer on the wire"
    );

    let (status, body) = patch(
        &h.app,
        &h.token,
        &format!("/billing/products/{id}"),
        json!({ "unitPriceCents": 13_000 }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["product"]["unitPriceCents"], 13_000);
    assert_eq!(
        body["product"]["name"], "Consulting",
        "unstated fields hold"
    );
    assert_eq!(body["product"]["vatRateBp"], 2100);

    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/billing/products/{id}/archive"),
        json!({ "archived": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["product"]["archived"], true);
    let (_, body) = get(&h.app, &h.token, "/billing/products").await;
    assert!(names(&body, "products").is_empty());
    let (_, body) = get(&h.app, &h.token, "/billing/products?includeArchived=true").await;
    assert_eq!(names(&body, "products"), vec!["Consulting".to_owned()]);
}

#[tokio::test]
async fn a_zero_price_is_a_stated_value_on_the_wire() {
    let h = harness("bill-prod-zero").await;
    let id = created_id(
        "product",
        post(
            &h.app,
            &h.token,
            "/billing/products",
            json!({ "name": "Consulting", "unitPriceCents": 12_500, "vatRateBp": 2100 }),
        )
        .await,
    );
    // A free item and an exempt rate are real; neither may fall back to the
    // stored value just because JSON `0` is falsy in the client's language.
    let (status, body) = patch(
        &h.app,
        &h.token,
        &format!("/billing/products/{id}"),
        json!({ "unitPriceCents": 0, "vatRateBp": 0 }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["product"]["unitPriceCents"], 0);
    assert_eq!(body["product"]["vatRateBp"], 0);
}

// ---- the guards --------------------------------------------------------------

#[tokio::test]
async fn every_billing_route_refuses_an_unauthenticated_caller() {
    let h = harness("bill-401").await;
    // A real record exists, so a 401 can only be the auth guard and never a
    // missing row standing in for one.
    let id = created_id(
        "customer",
        post(&h.app, &h.token, "/billing/customers", acme()).await,
    );

    let unauthenticated: Vec<Request<Body>> = vec![
        Request::builder()
            .uri("/billing/customers")
            .body(Body::empty())
            .unwrap(),
        Request::builder()
            .uri(format!("/billing/customers/{id}"))
            .body(Body::empty())
            .unwrap(),
        Request::builder()
            .uri("/billing/products")
            .body(Body::empty())
            .unwrap(),
        with_json("POST", "/billing/customers", None, acme()),
        with_json(
            "PATCH",
            &format!("/billing/customers/{id}"),
            None,
            json!({ "city": "Hamburg" }),
        ),
        with_json(
            "POST",
            &format!("/billing/customers/{id}/archive"),
            None,
            json!({ "archived": true }),
        ),
        with_json("POST", "/billing/products", None, json!({ "name": "x" })),
    ];
    for req in unauthenticated {
        let uri = req.uri().to_string();
        let method = req.method().to_string();
        let (status, _) = send(&h.app, req).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {uri}");
    }

    // The record it could not see is still exactly as it was.
    let (_, body) = get(&h.app, &h.token, "/billing/customers").await;
    assert_eq!(names(&body, "customers"), vec!["Acme GmbH".to_owned()]);
}

#[tokio::test]
async fn a_field_the_caller_can_fix_is_a_422_naming_the_rule() {
    let h = harness("bill-422").await;

    let cases: Vec<(Value, &str)> = vec![
        (json!({ "name": "  ", "country": "DE" }), "name"),
        (json!({ "name": "Acme", "country": "Germany" }), "country"),
        (
            json!({ "name": "Acme", "country": "DE", "currency": "EURO" }),
            "currency",
        ),
        (
            json!({ "name": "Acme", "country": "DE", "email": "not-an-address" }),
            "email",
        ),
        (
            json!({ "name": "Acme", "country": "DE", "paymentTermsDays": 4000 }),
            "payment terms",
        ),
        // An unprefixed id is judged against the customer's own member state,
        // so a German body on a Dutch customer is reported against the Dutch
        // rule; a broken check digit is caught in its own country.
        (
            json!({ "name": "Acme", "country": "DE", "vatId": "DE811907981" }),
            "check digit",
        ),
        (
            json!({ "name": "Acme", "country": "NL", "vatId": "811907980" }),
            "NL",
        ),
    ];
    for (body, expected) in cases {
        let (status, answer) = post(&h.app, &h.token, "/billing/customers", body.clone()).await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "expected 422 for {body}, got {answer}"
        );
        let detail = answer["detail"].as_str().unwrap_or_default();
        assert!(
            detail.contains(expected),
            "detail {detail:?} does not name {expected:?}"
        );
    }

    for body in [
        json!({ "name": "  " }),
        json!({ "name": "Consulting", "unitPriceCents": -1 }),
        json!({ "name": "Consulting", "vatRateBp": 10_001 }),
    ] {
        let (status, answer) = post(&h.app, &h.token, "/billing/products", body.clone()).await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "expected 422 for {body}, got {answer}"
        );
    }

    // Nothing was written along the way.
    let (_, body) = get(&h.app, &h.token, "/billing/customers?includeArchived=1").await;
    assert!(names(&body, "customers").is_empty(), "{body}");
    let (_, body) = get(&h.app, &h.token, "/billing/products?includeArchived=1").await;
    assert!(names(&body, "products").is_empty(), "{body}");
}

#[tokio::test]
async fn a_malformed_body_is_a_400_that_never_quotes_the_body() {
    let h = harness("bill-400").await;

    // A price with a decimal point is refused rather than rounded — money is
    // integer cents at every layer including the wire.
    let (status, body) = post(
        &h.app,
        &h.token,
        "/billing/products",
        json!({ "name": "Consulting", "unitPriceCents": 19.99 }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["detail"], "malformed request body");

    // Unparseable bytes, and a body whose customer data must not come back in
    // the error.
    let req = Request::builder()
        .method("POST")
        .uri("/billing/customers")
        .header("authorization", format!("Bearer {}", h.token))
        .header("content-type", "application/json")
        .body(Body::from(r#"{"name": 7, "city": "Geheimstadt""#))
        .unwrap();
    let (status, body) = send(&h.app, req).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        !body.to_string().contains("Geheimstadt"),
        "the error echoed the request: {body}"
    );

    // An `archived` field of the wrong type is refused rather than guessed at.
    let id = created_id(
        "customer",
        post(&h.app, &h.token, "/billing/customers", acme()).await,
    );
    let (status, _) = post(
        &h.app,
        &h.token,
        &format!("/billing/customers/{id}/archive"),
        json!({ "archived": "yes" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn an_unknown_id_is_a_404_on_every_verb() {
    let h = harness("bill-404").await;
    for (method, uri, body) in [
        ("GET", "/billing/customers/no-such-id", json!({})),
        (
            "PATCH",
            "/billing/customers/no-such-id",
            json!({ "city": "X" }),
        ),
        (
            "POST",
            "/billing/customers/no-such-id/archive",
            json!({ "archived": true }),
        ),
        ("GET", "/billing/products/no-such-id", json!({})),
        (
            "PATCH",
            "/billing/products/no-such-id",
            json!({ "unitPriceCents": 1 }),
        ),
        (
            "POST",
            "/billing/products/no-such-id/archive",
            json!({ "archived": true }),
        ),
    ] {
        let (status, answer) = send(&h.app, with_json(method, uri, Some(&h.token), body)).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method} {uri} → {answer}");
    }
}

#[tokio::test]
async fn a_contact_link_only_ever_reaches_the_callers_own_tenant() {
    let a = harness("bill-contact-a").await;
    let b = harness("bill-contact-b").await;
    let contact = Contact {
        id: ContactId::new(String::new()),
        display_name: "Bea Buyer".to_owned(),
        first_name: None,
        last_name: None,
        emails: Vec::new(),
        phones: Vec::new(),
        organization: None,
        job_title: None,
        notes: None,
    };
    let b_contact = b.acc.create_contact(&contact).await.unwrap();

    // Tenant A naming tenant B's contact gets the same 404 as a contact that
    // never existed — no cross-tenant link, and no oracle for its existence.
    let mut body = acme();
    body["contactId"] = json!(b_contact.as_str());
    let (status, answer) = post(&a.app, &a.token, "/billing/customers", body).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{answer}");

    let mut body = acme();
    body["contactId"] = json!("no-such-contact");
    let (status, _) = post(&a.app, &a.token, "/billing/customers", body).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Nothing was written on either failed attempt.
    let (_, listed) = get(&a.app, &a.token, "/billing/customers?includeArchived=1").await;
    assert!(names(&listed, "customers").is_empty(), "{listed}");

    // Its own contact links fine, which is what makes the two 404s above mean
    // something.
    let a_contact = a.acc.create_contact(&contact).await.unwrap();
    let mut body = acme();
    body["contactId"] = json!(a_contact.as_str());
    let (status, answer) = post(&a.app, &a.token, "/billing/customers", body).await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(answer["customer"]["contactId"], a_contact.as_str());
}

// ---- the wrong-tenant test (mandatory: CLAUDE.md law 1) ----------------------

#[tokio::test]
async fn another_tenants_customer_and_price_are_invisible_on_every_route() {
    let a = harness("bill-tenant-a").await;
    let b = harness("bill-tenant-b").await;

    // Tenant B's records, created through B's own door.
    let b_customer = created_id(
        "customer",
        post(&b.app, &b.token, "/billing/customers", acme()).await,
    );
    let b_product = created_id(
        "product",
        post(
            &b.app,
            &b.token,
            "/billing/products",
            json!({ "name": "B Consulting", "unitPriceCents": 9_900, "vatRateBp": 2100 }),
        )
        .await,
    );

    // A's lists never mention them, archived included.
    for uri in [
        "/billing/customers",
        "/billing/customers?includeArchived=1",
        "/billing/products",
        "/billing/products?includeArchived=1",
    ] {
        let (status, body) = get(&a.app, &a.token, uri).await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            !body.to_string().contains("Acme GmbH") && !body.to_string().contains("B Consulting"),
            "{uri} leaked another tenant's record: {body}"
        );
    }

    // Every verb on B's ids answers A with the same 404 an invented id gets —
    // the reads, the writes, and the archive.
    let attempts: Vec<(&str, String, Value)> = vec![
        ("GET", format!("/billing/customers/{b_customer}"), json!({})),
        (
            "PATCH",
            format!("/billing/customers/{b_customer}"),
            json!({ "name": "Taken Over" }),
        ),
        (
            "POST",
            format!("/billing/customers/{b_customer}/archive"),
            json!({ "archived": true }),
        ),
        ("GET", format!("/billing/products/{b_product}"), json!({})),
        (
            "PATCH",
            format!("/billing/products/{b_product}"),
            json!({ "unitPriceCents": 1 }),
        ),
        (
            "POST",
            format!("/billing/products/{b_product}/archive"),
            json!({ "archived": true }),
        ),
    ];
    for (method, uri, body) in attempts {
        let (status, answer) = send(&a.app, with_json(method, &uri, Some(&a.token), body)).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method} {uri} → {answer}");
        assert!(
            !answer.to_string().contains("Acme") && !answer.to_string().contains("Consulting"),
            "{method} {uri} leaked the record it refused: {answer}"
        );
    }

    // And B's records are untouched by everything A just tried.
    let (_, body) = get(&b.app, &b.token, "/billing/customers").await;
    assert_eq!(body["customers"][0]["name"], "Acme GmbH");
    assert_eq!(body["customers"][0]["archived"], false);
    let (_, body) = get(&b.app, &b.token, "/billing/products").await;
    assert_eq!(body["products"][0]["name"], "B Consulting");
    assert_eq!(body["products"][0]["unitPriceCents"], 9_900);
    assert_eq!(body["products"][0]["archived"], false);

    // Read back through the store handle too, past the routes entirely, so the
    // proof does not rest on the same tenant predicate it is testing.
    let mistaken = a
        .acc
        .billing_customer(&alo_store::BillingCustomerId::new(b_customer))
        .await
        .unwrap();
    assert!(mistaken.is_none());
    match a
        .acc
        .update_billing_product(
            &alo_store::BillingProductId::new(b_product),
            &alo_store::NewProduct {
                name: "Taken Over".to_owned(),
                ..Default::default()
            },
        )
        .await
    {
        Err(StoreError::NotFound) => {}
        other => panic!("expected NotFound, got {other:?}"),
    }
}

// ---- the catalog on the wire (B5.02) -----------------------------------------

/// The five catalog fields alo Inventory adds to a product, and the two
/// refusals that come with them: a barcode that fails its own check digit is a
/// `422` naming the field and never the code, and a code another product of the
/// **same** tenant already carries is a `409`.
#[tokio::test]
async fn a_product_carries_its_catalog_facts_and_refuses_a_bad_code() {
    let h = harness("bill-prod-catalog").await;

    let (status, body) = post(
        &h.app,
        &h.token,
        "/billing/products",
        json!({
            "name": "Blue chair",
            "unit": "piece",
            "unitPriceCents": 4_900,
            "vatRateBp": 2100,
            "sku": "  CH-BLUE-01 ",
            "barcode": " 400-638 133 393 1 ",
            "stocked": true,
            "purchasePriceCents": 2_150,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let p = &body["product"];
    assert_eq!(p["sku"], "CH-BLUE-01", "stored trimmed");
    assert_eq!(p["barcode"], "4006381333931", "separators are presentation");
    assert_eq!(p["stocked"], true);
    assert_eq!(p["purchasePriceCents"], 2_150);
    assert_eq!(
        p["unitPriceCents"], 4_900,
        "what we charge is not what we pay"
    );
    assert!(
        p["purchasePriceCents"].is_i64(),
        "what we pay is integer cents too"
    );
    assert!(p["photoNodeId"].is_null());
    let id = p["id"].as_str().unwrap().to_owned();

    // A product created without any of them is a service with no codes — the
    // shape every billing tenant already has, unchanged by this release.
    let (_, body) = post(
        &h.app,
        &h.token,
        "/billing/products",
        json!({ "name": "Consulting" }),
    )
    .await;
    assert_eq!(body["product"]["stocked"], false);
    assert_eq!(body["product"]["sku"], "");
    assert_eq!(body["product"]["barcode"], "");
    assert_eq!(body["product"]["purchasePriceCents"], 0);

    // A typo in the code is caught here rather than when the wrong item ships,
    // and the refusal never echoes the code back into a log.
    for bad in ["4006381333930", "12345", "40063813339A1"] {
        let (status, body) = post(
            &h.app,
            &h.token,
            "/billing/products",
            json!({ "name": "Mistyped", "barcode": bad }),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
        let detail = body["detail"].as_str().unwrap_or_default();
        assert!(detail.contains("barcode"), "unhelpful: {detail}");
        assert!(
            !detail.contains(bad),
            "the detail carried the code: {detail}"
        );
    }

    // The same code twice inside one tenant is a conflict naming the field.
    for (field, value, word) in [
        ("sku", "CH-BLUE-01", "SKU"),
        ("barcode", "4006381333931", "barcode"),
    ] {
        let mut request = json!({ "name": "Second chair" });
        request[field] = json!(value);
        let (status, body) = post(&h.app, &h.token, "/billing/products", request).await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert!(
            body["detail"].as_str().unwrap_or_default().contains(word),
            "unhelpful: {body}"
        );
    }

    // A photo nobody can see is the same 404 an id that never existed gets.
    let (status, body) = patch(
        &h.app,
        &h.token,
        &format!("/billing/products/{id}"),
        json!({ "photoNodeId": "no-such-node" }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");

    // Un-stocking and clearing a code are stated values, not absences.
    let (status, body) = patch(
        &h.app,
        &h.token,
        &format!("/billing/products/{id}"),
        json!({ "stocked": false, "sku": "", "barcode": "" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["product"]["stocked"], false);
    assert_eq!(body["product"]["sku"], "");
    assert_eq!(body["product"]["barcode"], "");
    assert_eq!(
        body["product"]["name"], "Blue chair",
        "unstated fields hold"
    );
}

/// Uniqueness is the tenant's own business: two companies selling the same
/// book never collide, and neither insert fails because of the other. A global
/// index would leak the existence of another tenant's product through a
/// constraint violation — the kind of side channel that is easy to miss and
/// impossible to explain afterwards.
#[tokio::test]
async fn the_same_barcode_in_two_tenants_is_two_products() {
    let a = harness("bill-code-a").await;
    let b = harness("bill-code-b").await;

    let chair = json!({
        "name": "Blue chair",
        "sku": "CH-BLUE-01",
        "barcode": "4006381333931",
        "stocked": true,
        "purchasePriceCents": 2_150,
    });
    let (status, body) = post(&a.app, &a.token, "/billing/products", chair.clone()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = post(&b.app, &b.token, "/billing/products", chair).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "another tenant's identical code must not block this one: {body}"
    );
    assert_eq!(body["product"]["barcode"], "4006381333931");

    // ... and each tenant sees exactly one chair.
    for h in [&a, &b] {
        let (_, body) = get(&h.app, &h.token, "/billing/products").await;
        assert_eq!(names(&body, "products"), vec!["Blue chair".to_owned()]);
    }
}
