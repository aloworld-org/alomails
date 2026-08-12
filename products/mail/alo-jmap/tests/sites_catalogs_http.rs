//! The `/sites/{id}/catalogs*` edit surface (ADR 0036 / ADR 0041, S2.12c),
//! driven through the real router over a real Postgres.
//!
//! `alo-store`'s suites prove the storage; what this suite pins is the **edge**:
//! the auth guard, the price that is typed rather than posted as a number, the
//! handle the editor may leave to us, the three availability words, and —
//! mandatory — that another tenant's catalog is invisible and untouchable on
//! every verb.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use common::{Harness, harness, harness_on, send};

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

/// A subdomain unique to this harness run — the global namespace is shared.
fn sub(tag: &str, h: &Harness) -> String {
    let salt: String = h
        .tenant
        .as_str()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .take(20)
        .collect();
    format!("{tag}{salt}")
}

fn created_id(kind: &str, (status, body): (StatusCode, Value)) -> String {
    assert_eq!(status, StatusCode::OK, "create {kind} failed: {body}");
    body["id"].as_str().expect("created id").to_owned()
}

async fn site_of(h: &Harness, tag: &str) -> String {
    created_id(
        "site",
        post(
            &h.app,
            &h.token,
            "/sites",
            json!({ "name": "Bakery", "subdomain": sub(tag, h) }),
        )
        .await,
    )
}

#[tokio::test]
async fn catalogs_are_created_read_replaced_and_deleted_through_the_account_door() {
    let owner = harness("sites-catalogs-http").await;
    let site = site_of(&owner, "cat").await;

    let (status, empty) = get(&owner.app, &owner.token, &format!("/sites/{site}/catalogs")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(empty["catalogs"].as_array().unwrap().len(), 0);

    let (status, created) = post(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/catalogs"),
        json!({ "name": "Menu", "currency": "eur" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    // The currency is canonicalised by the store, and ordering is off until
    // somebody decides otherwise.
    assert_eq!(created["currency"], "EUR");
    assert_eq!(created["currencyExponent"], 2);
    assert_eq!(created["ordersEnabled"], false);
    let catalog = created["id"].as_str().unwrap().to_owned();

    let (status, listed) = get(&owner.app, &owner.token, &format!("/sites/{site}/catalogs")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed["catalogs"].as_array().unwrap().len(), 1);
    assert_eq!(listed["catalogs"][0]["name"], "Menu");

    let (status, replaced) = put(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/catalogs/{catalog}"),
        json!({ "name": "Saturday menu", "currency": "EUR", "ordersEnabled": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{replaced}");
    assert_eq!(replaced["name"], "Saturday menu");
    assert_eq!(replaced["ordersEnabled"], true);

    let (status, read) = get(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/catalogs/{catalog}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(read["catalog"]["name"], "Saturday menu");
    assert_eq!(read["categories"].as_array().unwrap().len(), 0);
    assert_eq!(read["items"].as_array().unwrap().len(), 0);

    let (status, _) = delete(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/catalogs/{catalog}"),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, gone) = get(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/catalogs/{catalog}"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{gone}");
}

#[tokio::test]
async fn an_item_is_priced_from_what_was_typed_and_handles_derive_from_names() {
    let owner = harness("sites-catalog-items").await;
    let site = site_of(&owner, "items").await;
    let catalog = created_id(
        "catalog",
        post(
            &owner.app,
            &owner.token,
            &format!("/sites/{site}/catalogs"),
            json!({ "name": "Menu", "currency": "EUR" }),
        )
        .await,
    );

    let category = created_id(
        "category",
        post(
            &owner.app,
            &owner.token,
            &format!("/sites/{site}/catalogs/{catalog}/categories"),
            json!({ "name": "Breads & rolls" }),
        )
        .await,
    );

    // A comma decimal, a currency sign and no handle at all: the store's own
    // parser reads the price, and the name derives the handle.
    let (status, bread) = post(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/catalogs/{catalog}/items"),
        json!({
            "name": "Sourdough loaf",
            "categoryId": category,
            "description": "Baked at six.",
            "price": "€4,50",
            "priceNote": "per loaf",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{bread}");
    assert_eq!(bread["slug"], "sourdough-loaf");
    assert_eq!(bread["priceCents"], 450);
    assert_eq!(bread["availability"], "available");
    assert_eq!(bread["categoryId"], category);
    assert_eq!(bread["position"], 0);
    let bread_id = bread["id"].as_str().unwrap().to_owned();

    // No price is not zero: an enquiry-only item stores nothing at all.
    let (status, cake) = post(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/catalogs/{catalog}/items"),
        json!({ "name": "Wedding cake", "slug": "wedding-cake", "price": "" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{cake}");
    assert!(cake["priceCents"].is_null());
    assert_eq!(cake["position"], 1, "a new item appends after the last");

    // A replace carries every field the editor shows; the stored position is
    // kept when the body is silent about it.
    let (status, sold_out) = put(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/catalogs/{catalog}/items/{bread_id}"),
        json!({
            "name": "Sourdough loaf",
            "slug": "sourdough",
            "price": "5",
            "availability": "sold_out",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{sold_out}");
    assert_eq!(sold_out["slug"], "sourdough");
    assert_eq!(sold_out["priceCents"], 500);
    assert_eq!(sold_out["availability"], "sold_out");
    assert_eq!(sold_out["position"], 0);
    assert!(
        sold_out["priceNote"].is_null(),
        "a cleared field is cleared"
    );
    assert!(sold_out["categoryId"].is_null());

    // Correcting a name leaves the public handle alone: a published page and
    // an order both name this item by its handle, and a rename is not a
    // decision to rename that.
    let (status, renamed) = put(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/catalogs/{catalog}/items/{bread_id}"),
        json!({ "name": "Sourdough loaf, large", "price": "5" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{renamed}");
    assert_eq!(renamed["name"], "Sourdough loaf, large");
    assert_eq!(renamed["slug"], "sourdough");
    let (status, regrouped) = put(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/catalogs/{catalog}/categories/{category}"),
        json!({ "name": "Breads" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{regrouped}");
    assert_eq!(regrouped["name"], "Breads");
    assert_eq!(regrouped["slug"], "breads-rolls");

    // The editor's read shows hidden items too — they are exactly the ones
    // their owner needs to find in order to put them back.
    let (status, hidden) = post(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/catalogs/{catalog}/items"),
        json!({ "name": "Test bun", "availability": "hidden" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{hidden}");
    let (status, read) = get(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/catalogs/{catalog}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(read["items"].as_array().unwrap().len(), 3);
    assert_eq!(read["categories"][0]["slug"], "breads-rolls");

    // Deleting the grouping keeps the things grouped by it.
    let (status, _) = delete(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/catalogs/{catalog}/categories/{category}"),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, after) = get(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/catalogs/{catalog}"),
    )
    .await;
    assert_eq!(after["categories"].as_array().unwrap().len(), 0);
    assert_eq!(after["items"].as_array().unwrap().len(), 3);

    let (status, _) = delete(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/catalogs/{catalog}/items/{bread_id}"),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, again) = delete(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/catalogs/{catalog}/items/{bread_id}"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{again}");
}

#[tokio::test]
async fn a_refusal_names_the_rule_and_a_body_that_is_not_the_shape_is_a_400() {
    let owner = harness("sites-catalog-refusals").await;
    let site = site_of(&owner, "refuse").await;

    let (status, bad_currency) = post(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/catalogs"),
        json!({ "name": "Menu", "currency": "euro" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        bad_currency["detail"]
            .as_str()
            .unwrap()
            .contains("ISO 4217"),
        "{bad_currency}"
    );

    let catalog = created_id(
        "catalog",
        post(
            &owner.app,
            &owner.token,
            &format!("/sites/{site}/catalogs"),
            json!({ "name": "Menu", "currency": "EUR" }),
        )
        .await,
    );

    // A price whose separator could mean two different prices is refused
    // rather than guessed — a wrong guess is a wrong price on a public page.
    let (status, ambiguous) = post(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/catalogs/{catalog}/items"),
        json!({ "name": "Cake", "price": "1,234" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        ambiguous["detail"]
            .as_str()
            .unwrap()
            .contains("two different prices"),
        "{ambiguous}"
    );

    let (status, availability) = post(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/catalogs/{catalog}/items"),
        json!({ "name": "Cake", "availability": "maybe" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        availability["detail"]
            .as_str()
            .unwrap()
            .contains("sold_out"),
        "{availability}"
    );

    let (status, blank_handle) = post(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/catalogs/{catalog}/items"),
        json!({ "name": "———" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{blank_handle}");

    let (status, unknown_field) = post(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/catalogs/{catalog}/items"),
        json!({ "name": "Cake", "priceCents": 400 }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{unknown_field}");

    let taken = post(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/catalogs/{catalog}/items"),
        json!({ "name": "Cake", "slug": "cake" }),
    )
    .await;
    assert_eq!(taken.0, StatusCode::OK);
    let (status, duplicate) = post(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/catalogs/{catalog}/items"),
        json!({ "name": "Other cake", "slug": "cake" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        duplicate["detail"].as_str().unwrap().contains("handle"),
        "{duplicate}"
    );

    // An image blob that is not the tenant's cannot be attached.
    let (status, foreign_image) = post(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/catalogs/{catalog}/items"),
        json!({ "name": "Cake", "slug": "photo-cake", "imageBlobId": "nope" }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{foreign_image}");
}

#[tokio::test]
async fn another_tenants_catalog_is_invisible_and_untouchable_on_every_verb() {
    let owner = harness("sites-catalogs-owner").await;
    let outsider = harness_on(Arc::clone(&owner.store), "sites-catalogs-outsider").await;
    let site = site_of(&owner, "own").await;
    let catalog = created_id(
        "catalog",
        post(
            &owner.app,
            &owner.token,
            &format!("/sites/{site}/catalogs"),
            json!({ "name": "Menu", "currency": "EUR" }),
        )
        .await,
    );
    let item = created_id(
        "item",
        post(
            &owner.app,
            &owner.token,
            &format!("/sites/{site}/catalogs/{catalog}/items"),
            json!({ "name": "Sourdough", "price": "4.50" }),
        )
        .await,
    );
    let category = created_id(
        "category",
        post(
            &owner.app,
            &owner.token,
            &format!("/sites/{site}/catalogs/{catalog}/categories"),
            json!({ "name": "Breads" }),
        )
        .await,
    );

    // Unauthenticated: every route is behind the account door.
    for (method, uri) in [
        ("GET", format!("/sites/{site}/catalogs")),
        ("POST", format!("/sites/{site}/catalogs")),
        ("GET", format!("/sites/{site}/catalogs/{catalog}")),
        ("PUT", format!("/sites/{site}/catalogs/{catalog}")),
        ("DELETE", format!("/sites/{site}/catalogs/{catalog}")),
        (
            "POST",
            format!("/sites/{site}/catalogs/{catalog}/categories"),
        ),
        (
            "PUT",
            format!("/sites/{site}/catalogs/{catalog}/categories/{category}"),
        ),
        (
            "DELETE",
            format!("/sites/{site}/catalogs/{catalog}/categories/{category}"),
        ),
        ("POST", format!("/sites/{site}/catalogs/{catalog}/items")),
        (
            "PUT",
            format!("/sites/{site}/catalogs/{catalog}/items/{item}"),
        ),
        (
            "DELETE",
            format!("/sites/{site}/catalogs/{catalog}/items/{item}"),
        ),
    ] {
        let (status, body) = send(&owner.app, with_json(method, &uri, None, json!({}))).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {uri}: {body}");
    }

    // The rival tenant: a 404 everywhere, indistinguishable from a catalog
    // that never existed.
    let rival = [
        get(
            &outsider.app,
            &outsider.token,
            &format!("/sites/{site}/catalogs"),
        )
        .await,
        post(
            &outsider.app,
            &outsider.token,
            &format!("/sites/{site}/catalogs"),
            json!({ "name": "Theirs", "currency": "EUR" }),
        )
        .await,
        get(
            &outsider.app,
            &outsider.token,
            &format!("/sites/{site}/catalogs/{catalog}"),
        )
        .await,
        put(
            &outsider.app,
            &outsider.token,
            &format!("/sites/{site}/catalogs/{catalog}"),
            json!({ "name": "Theirs", "currency": "USD" }),
        )
        .await,
        delete(
            &outsider.app,
            &outsider.token,
            &format!("/sites/{site}/catalogs/{catalog}"),
        )
        .await,
        post(
            &outsider.app,
            &outsider.token,
            &format!("/sites/{site}/catalogs/{catalog}/items"),
            json!({ "name": "Theirs" }),
        )
        .await,
        put(
            &outsider.app,
            &outsider.token,
            &format!("/sites/{site}/catalogs/{catalog}/items/{item}"),
            json!({ "name": "Theirs", "price": "0.01" }),
        )
        .await,
        delete(
            &outsider.app,
            &outsider.token,
            &format!("/sites/{site}/catalogs/{catalog}/items/{item}"),
        )
        .await,
        post(
            &outsider.app,
            &outsider.token,
            &format!("/sites/{site}/catalogs/{catalog}/categories"),
            json!({ "name": "Theirs" }),
        )
        .await,
        put(
            &outsider.app,
            &outsider.token,
            &format!("/sites/{site}/catalogs/{catalog}/categories/{category}"),
            json!({ "name": "Theirs" }),
        )
        .await,
        delete(
            &outsider.app,
            &outsider.token,
            &format!("/sites/{site}/catalogs/{catalog}/categories/{category}"),
        )
        .await,
    ];
    for (status, body) in rival {
        assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    }

    // And the owner's catalog is exactly as it was left.
    let (status, read) = get(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/catalogs/{catalog}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(read["catalog"]["name"], "Menu");
    assert_eq!(read["catalog"]["currency"], "EUR");
    assert_eq!(read["items"].as_array().unwrap().len(), 1);
    assert_eq!(read["items"][0]["priceCents"], 450);
    assert_eq!(read["categories"].as_array().unwrap().len(), 1);

    // The outsider's own site sees none of it either.
    let their_site = site_of(&outsider, "theirs").await;
    let (status, theirs) = get(
        &outsider.app,
        &outsider.token,
        &format!("/sites/{their_site}/catalogs"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(theirs["catalogs"].as_array().unwrap().len(), 0);
}
