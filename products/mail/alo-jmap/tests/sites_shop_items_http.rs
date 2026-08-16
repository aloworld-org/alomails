//! The owner's shop-shelf routes through the real router and Postgres
//! (S3.05c, ADR 0041): which stocked products a site lists, resolved by the
//! owning seams at every read.
//!
//! Pinned here: every number in an answer being the catalog and stock-sale
//! seams' answer *now* (never a stored copy), the store's refusal sentences
//! travelling verbatim, a product that left the price list saying so instead
//! of showing a stale price, and a foreign tenant's knock on every verb
//! reading exactly like a site that never existed.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use alo_store::inv_locations::{LocationKind, LocationSeed};
use alo_store::inv_moves::{MoveReason, NewMove};
use alo_store::{BillingProductId, NewProduct};

use common::{Harness, get, harness, harness_on, send};

/// A subdomain unique to this harness run — the namespace is global.
fn sub(tag: &str, h: &Harness) -> String {
    let salt: String = h
        .tenant
        .as_str()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .take(16)
        .collect();
    format!("{tag}{salt}")
}

async fn post(app: &Router, token: Option<&str>, uri: &str, body: Value) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json");
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    send(app, request.body(Body::from(body.to_string())).unwrap()).await
}

async fn delete(app: &Router, token: &str, uri: &str) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("DELETE")
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    send(app, request).await
}

/// A stocked product with `shelf` whole units received into the tenant's
/// stock location — the shape wave two sells.
async fn stocked_product(h: &Harness, name: &str, cents: i64, shelf: i64) -> BillingProductId {
    let seeded = h
        .acc
        .inv_locations_or_seed(
            &LocationSeed {
                stock: "Main".to_owned(),
                supplier: "Suppliers".to_owned(),
                customer: "Customers".to_owned(),
                adjustment: "Adjustments".to_owned(),
                production: "Production".to_owned(),
            },
            false,
        )
        .await
        .unwrap();
    let of = |kind: LocationKind| {
        seeded
            .iter()
            .find(|location| location.kind == kind)
            .expect("the seed writes every kind")
            .id
            .clone()
    };
    let product = h
        .acc
        .create_billing_product(&NewProduct {
            name: name.to_owned(),
            unit: "piece".to_owned(),
            unit_price_cents: cents,
            vat_rate_bp: 2100,
            stocked: true,
            ..Default::default()
        })
        .await
        .unwrap();
    if shelf > 0 {
        h.acc
            .record_move(&NewMove {
                product_id: product.clone(),
                from_location_id: of(LocationKind::Supplier),
                to_location_id: of(LocationKind::Stock),
                qty_milli: shelf * 1_000,
                reason: MoveReason::Purchase,
                reason_code: None,
                note: String::new(),
                reference: None,
                occurred_at: None,
            })
            .await
            .unwrap();
    }
    product
}

#[tokio::test]
async fn the_shelf_is_the_owning_seams_answer_now() {
    let h = harness("shopitems-shelf").await;
    let site = h
        .acc
        .create_site("Shop", &sub("shopshelf", &h))
        .await
        .unwrap();
    let product = stocked_product(&h, "Field guide", 2_400, 7).await;
    // A priced but unstocked service must never reach the shop's pickers.
    h.acc
        .create_billing_product(&NewProduct {
            name: "Consulting hour".to_owned(),
            unit: "hour".to_owned(),
            unit_price_cents: 9_500,
            vat_rate_bp: 2100,
            stocked: false,
            ..Default::default()
        })
        .await
        .unwrap();
    let items_path = format!("/sites/{site}/shop-items");

    // The empty shelf is an honest empty list, currency included.
    let (status, answer) = get(&h.app, &h.token, &items_path).await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(answer["items"], json!([]));
    assert_eq!(answer["currency"], "EUR");

    // The picker offers exactly the stocked product, with its shelf count.
    let (status, offered) = get(&h.app, &h.token, &format!("/sites/{site}/shop-products")).await;
    assert_eq!(status, StatusCode::OK, "{offered}");
    let products = offered["products"].as_array().unwrap();
    assert_eq!(products.len(), 1, "{offered}");
    assert_eq!(products[0]["name"], "Field guide");
    assert_eq!(products[0]["unitPriceCents"], 2_400);
    assert_eq!(products[0]["availableUnits"], 7);

    // Listing it answers the resolved row, and the list then shows it.
    let (status, added) = post(
        &h.app,
        Some(&h.token),
        &items_path,
        json!({ "productId": product.as_str() }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{added}");
    assert_eq!(added["item"]["productName"], "Field guide");
    assert_eq!(added["item"]["unitPriceCents"], 2_400);
    assert_eq!(added["item"]["availableUnits"], 7);
    let (_, listed) = get(&h.app, &h.token, &items_path).await;
    let rows = listed["items"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{listed}");
    assert_eq!(rows[0]["id"], added["item"]["id"]);
    assert_eq!(rows[0]["productId"], product.as_str());

    // Delisting is a 204, and the shelf is empty again; a second knock on the
    // gone listing is a 404.
    let item_path = format!("{items_path}/{}", added["item"]["id"].as_str().unwrap());
    let (status, _) = delete(&h.app, &h.token, &item_path).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, listed) = get(&h.app, &h.token, &items_path).await;
    assert_eq!(listed["items"], json!([]));
    let (status, _) = delete(&h.app, &h.token, &item_path).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_store_rules_on_what_may_be_listed_in_its_own_words() {
    let h = harness("shopitems-rules").await;
    let site = h
        .acc
        .create_site("Shop", &sub("shoprules", &h))
        .await
        .unwrap();
    let items_path = format!("/sites/{site}/shop-items");
    let service = h
        .acc
        .create_billing_product(&NewProduct {
            name: "Consulting hour".to_owned(),
            unit: "hour".to_owned(),
            unit_price_cents: 9_500,
            vat_rate_bp: 2100,
            stocked: false,
            ..Default::default()
        })
        .await
        .unwrap();

    // A service has no shelf: the store's sentence, verbatim.
    let (status, problem) = post(
        &h.app,
        Some(&h.token),
        &items_path,
        json!({ "productId": service.as_str() }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{problem}");
    assert!(
        problem["detail"]
            .as_str()
            .unwrap()
            .contains("not a stocked product"),
        "{problem}"
    );

    // An invented product id is not on the price list.
    let (status, problem) = post(
        &h.app,
        Some(&h.token),
        &items_path,
        json!({ "productId": "no-such-product" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{problem}");
    assert!(
        problem["detail"]
            .as_str()
            .unwrap()
            .contains("not on the price list"),
        "{problem}"
    );

    // Listing twice is a refusal, not a second row — the `/sites/{id}`
    // contract speaks conflicts as 422 with the store's own sentence.
    let product = stocked_product(&h, "Field guide", 2_400, 3).await;
    let body = json!({ "productId": product.as_str() });
    let (status, _) = post(&h.app, Some(&h.token), &items_path, body.clone()).await;
    assert_eq!(status, StatusCode::OK);
    let (status, problem) = post(&h.app, Some(&h.token), &items_path, body).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{problem}");
    assert!(
        problem["detail"].as_str().unwrap().contains("already"),
        "{problem}"
    );

    // A body without the one field has nothing to say.
    let (status, _) = post(&h.app, Some(&h.token), &items_path, json!({})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (_, listed) = get(&h.app, &h.token, &items_path).await;
    assert_eq!(listed["items"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn a_gone_product_says_so_instead_of_showing_a_stale_price() {
    let h = harness("shopitems-gone").await;
    let site = h
        .acc
        .create_site("Shop", &sub("shopgone", &h))
        .await
        .unwrap();
    let product = stocked_product(&h, "Field guide", 2_400, 5).await;
    let items_path = format!("/sites/{site}/shop-items");
    let (status, _) = post(
        &h.app,
        Some(&h.token),
        &items_path,
        json!({ "productId": product.as_str() }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    h.acc
        .set_billing_product_archived(&product, true)
        .await
        .unwrap();

    // The listing stays — a reference, not a copy — but every resolved fact
    // is the honest null, never yesterday's price.
    let (status, listed) = get(&h.app, &h.token, &items_path).await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    let rows = listed["items"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{listed}");
    assert_eq!(rows[0]["productName"], Value::Null);
    assert_eq!(rows[0]["unitPriceCents"], Value::Null);
    assert_eq!(rows[0]["availableUnits"], Value::Null);

    // And the picker no longer offers it.
    let (_, offered) = get(&h.app, &h.token, &format!("/sites/{site}/shop-products")).await;
    assert_eq!(offered["products"], json!([]));
}

#[tokio::test]
async fn the_tenant_walls_hold_on_every_verb() {
    let a = harness("shopitems-owner").await;
    let b = harness_on(Arc::clone(&a.store), "shopitems-stranger").await;
    let site = a
        .acc
        .create_site("Shop", &sub("shopwall", &a))
        .await
        .unwrap();
    let product = stocked_product(&a, "Field guide", 2_400, 2).await;
    let items_path = format!("/sites/{site}/shop-items");
    let (status, added) = post(
        &a.app,
        Some(&a.token),
        &items_path,
        json!({ "productId": product.as_str() }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{added}");
    let item_path = format!("{items_path}/{}", added["item"]["id"].as_str().unwrap());

    // No token at all: 401 before anything resolves.
    for (method, uri) in [
        ("GET", items_path.as_str()),
        ("GET", &format!("/sites/{site}/shop-products")),
        ("DELETE", item_path.as_str()),
    ] {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .body(Body::empty())
            .unwrap();
        let (status, _) = send(&a.app, request).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {uri}");
    }
    let (status, _) = post(&a.app, None, &items_path, json!({ "productId": "x" })).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Tenant B, real token, A's site: the same 404 an invented site gets on
    // every verb — never the shelf, never a hint the site exists.
    let (status, problem) = get(&b.app, &b.token, &items_path).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{problem}");
    let (missing_status, missing) = get(&b.app, &b.token, "/sites/no-such-site/shop-items").await;
    assert_eq!(missing_status, StatusCode::NOT_FOUND);
    assert_eq!(problem, missing);
    let (status, _) = get(&b.app, &b.token, &format!("/sites/{site}/shop-products")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = post(
        &b.app,
        Some(&b.token),
        &items_path,
        json!({ "productId": product.as_str() }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = delete(&b.app, &b.token, &item_path).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // A's shelf is exactly where it was.
    let (_, listed) = get(&a.app, &a.token, &items_path).await;
    assert_eq!(listed["items"].as_array().unwrap().len(), 1);
}
