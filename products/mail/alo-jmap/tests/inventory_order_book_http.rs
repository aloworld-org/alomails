//! The `/inventory/order-book` HTTP surface (alo Orders, item O1.d), driven
//! through the real router over a real Postgres.
//!
//! The store's own suite proves the arithmetic. What matters here is the
//! **edge**: that the five figures reach the wire, that the scope filter is
//! strict rather than silently widening, that a book with nothing in it answers
//! with a shape rather than a null a screen has to special-case, and above all
//! that **one tenant's orders never appear in another tenant's book** — the
//! failure that would be worst here, because this is the screen somebody reads
//! to decide what their business is owed.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use common::{Harness, harness, send};

async fn get(app: &Router, token: &str, uri: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("GET")
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    send(app, req).await
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

/// A customer, a stocked product with goods on the shelf, and one **confirmed**
/// order for `units` of it at `price` cents — the smallest thing that puts a row
/// in the book.
async fn an_order_worth(h: &Harness, tag: &str, units: i64, price: i64) -> String {
    let (status, customer) = post(
        &h.app,
        &h.token,
        "/billing/customers",
        json!({
            "name": format!("Koelhuis {tag}"),
            "addressLine1": "Keizersgracht 1",
            "postalCode": "1015",
            "city": "Amsterdam",
            "country": "NL",
            "currency": "EUR",
            "paymentTermsDays": 30,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{customer}");
    let customer = &customer["customer"];

    let (status, product) = post(
        &h.app,
        &h.token,
        "/billing/products",
        json!({
            "name": "AF-630 axial fan",
            "unit": "piece",
            "unitPriceCents": price,
            "vatRateBp": 2100,
            "stocked": true,
            "purchasePriceCents": 74_000,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{product}");
    let product = &product["product"];

    // Goods on the shelf, so the confirmation is not refused for
    // over-commitment (O1.a).
    h.acc
        .inv_locations_or_seed(
            &alo_store::inv_locations::LocationSeed {
                stock: "Hoofdmagazijn".to_owned(),
                supplier: "Leveranciers".to_owned(),
                customer: "Klanten".to_owned(),
                adjustment: "Correcties".to_owned(),
                production: "Productie".to_owned(),
            },
            false,
        )
        .await
        .unwrap();
    let seeded = h.acc.inv_locations(false).await.unwrap();
    let of = |kind: alo_store::inv_locations::LocationKind| {
        seeded
            .iter()
            .find(|l| l.kind == kind)
            .expect("seeded location")
            .id
            .clone()
    };
    h.acc
        .record_move(&alo_store::inv_moves::NewMove {
            product_id: alo_store::BillingProductId::new(product["id"].as_str().unwrap()),
            from_location_id: of(alo_store::inv_locations::LocationKind::Supplier),
            to_location_id: of(alo_store::inv_locations::LocationKind::Stock),
            qty_milli: units * 1_000,
            reason: alo_store::inv_moves::MoveReason::Purchase,
            reason_code: None,
            note: String::new(),
            reference: None,
            occurred_at: None,
        })
        .await
        .unwrap();

    let (status, order) = post(
        &h.app,
        &h.token,
        "/inventory/sales-orders",
        json!({
            "customerId": customer["id"],
            "lines": [{
                "productId": product["id"],
                "description": "AF-630 axial fan",
                "unit": "piece",
                "qtyMilli": units * 1_000,
                "unitPriceCents": price,
                "vatRateBp": 2100,
            }],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{order}");
    let id = order["salesOrder"]["id"].as_str().unwrap().to_owned();

    let (status, confirmed) = post(
        &h.app,
        &h.token,
        &format!("/inventory/sales-orders/{id}/confirm"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{confirmed}");
    id
}

#[tokio::test]
async fn the_book_carries_the_five_figures_and_its_totals() {
    let h = harness("book").await;
    let id = an_order_worth(&h, "book", 6, 129_500).await;

    let (status, body) = get(&h.app, &h.token, "/inventory/order-book").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["scope"], "open");

    let orders = body["orders"].as_array().expect("orders");
    assert_eq!(orders.len(), 1);
    let row = &orders[0];
    assert_eq!(row["id"], id.as_str());
    assert_eq!(row["status"], "confirmed");
    assert_eq!(row["currency"], "EUR");
    assert!(
        row["customerName"].as_str().is_some_and(|n| !n.is_empty()),
        "a book is read by a person: {row}"
    );

    // Six fans at EUR 1 295.00 net, nothing shipped, nothing billed.
    let f = &row["figures"];
    assert_eq!(f["orderedNetCents"], 777_000);
    assert_eq!(f["deliveredNetCents"], 0);
    assert_eq!(f["invoicedNetCents"], 0);
    assert_eq!(f["outstandingNetCents"], 777_000);
    // Confirmed, so the whole of it is held against the warehouse.
    assert_eq!(f["reservedNetCents"], 777_000);
    assert_eq!(f["reservedQtyMilli"], 6_000);
    assert_eq!(f["orderedQtyMilli"], 6_000);

    // The totals are the rows, and the currency is stated so a client knows the
    // total means something.
    assert_eq!(body["totals"]["orderedNetCents"], 777_000);
    assert_eq!(body["currencies"], json!(["EUR"]));
}

#[tokio::test]
async fn a_book_with_nothing_in_it_is_a_shape_and_not_a_null() {
    let h = harness("book-empty").await;
    let (status, body) = get(&h.app, &h.token, "/inventory/order-book").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["orders"], json!([]));
    assert_eq!(body["totals"]["orderedNetCents"], 0);
    assert_eq!(body["totals"]["reservedQtyMilli"], 0);
    assert_eq!(body["currencies"], json!([]));
}

#[tokio::test]
async fn the_scope_is_strict_and_a_draft_is_not_open_business() {
    let h = harness("book-scope").await;

    // A draft: raised, never confirmed. It is not open business and must not be
    // in the morning's book — but it is findable when somebody asks for
    // everything.
    let (status, customer) = post(
        &h.app,
        &h.token,
        "/billing/customers",
        json!({
            "name": "Koelhuis draft",
            "addressLine1": "Keizersgracht 1",
            "postalCode": "1015",
            "city": "Amsterdam",
            "country": "NL",
            "currency": "EUR",
            "paymentTermsDays": 30,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{customer}");
    let customer = &customer["customer"];
    let (status, draft) = post(
        &h.app,
        &h.token,
        "/inventory/sales-orders",
        json!({
            "customerId": customer["id"],
            "lines": [{
                "description": "Commissioning, two days",
                "unit": "day",
                "qtyMilli": 2_000,
                "unitPriceCents": 95_000,
                "vatRateBp": 2100,
            }],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{draft}");

    let (_, open) = get(&h.app, &h.token, "/inventory/order-book").await;
    assert_eq!(
        open["orders"],
        json!([]),
        "a draft promises nobody anything"
    );

    let (status, all) = get(&h.app, &h.token, "/inventory/order-book?scope=all").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(all["scope"], "all");
    assert_eq!(all["orders"].as_array().map(Vec::len), Some(1));
    // A charge in words is value and never goods, so it is money in the book
    // and nothing in the quantity columns.
    let f = &all["orders"][0]["figures"];
    assert_eq!(f["orderedNetCents"], 190_000);
    assert_eq!(f["orderedQtyMilli"], 0);
    // And a draft holds nothing against the warehouse, whatever it is worth.
    assert_eq!(f["reservedNetCents"], 0);

    // A scope this build cannot name is refused rather than widened.
    let (status, refused) = get(&h.app, &h.token, "/inventory/order-book?scope=closed").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
    // Case is not the strictness that matters — the sales-order list is
    // case-insensitive too.
    let (status, _) = get(&h.app, &h.token, "/inventory/order-book?scope=ALL").await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn one_tenants_book_never_contains_another_tenants_orders() {
    // The mandatory wrong-tenant test, on the surface where it would be worst:
    // this is the screen somebody reads to decide what their business is owed.
    let ours = harness("book-ours").await;
    let theirs = harness("book-theirs").await;

    an_order_worth(&ours, "ours", 2, 100_000).await;
    let theirs_id = an_order_worth(&theirs, "theirs", 9, 500_000).await;

    let (_, our_book) = get(&ours.app, &ours.token, "/inventory/order-book?scope=all").await;
    let ids: Vec<&str> = our_book["orders"]
        .as_array()
        .expect("orders")
        .iter()
        .map(|o| o["id"].as_str().unwrap())
        .collect();
    assert!(
        !ids.contains(&theirs_id.as_str()),
        "a neighbour's order must not appear in our book: {ids:?}"
    );
    assert_eq!(ids.len(), 1);
    assert_eq!(our_book["totals"]["orderedNetCents"], 200_000);

    // Asserted from both sides, so a leak would have to show up as a named row
    // rather than as a number nobody checked.
    let (_, their_book) = get(
        &theirs.app,
        &theirs.token,
        "/inventory/order-book?scope=all",
    )
    .await;
    assert_eq!(their_book["orders"].as_array().map(Vec::len), Some(1));
    assert_eq!(their_book["orders"][0]["id"], theirs_id.as_str());
    assert_eq!(their_book["totals"]["orderedNetCents"], 4_500_000);

    // And the door itself is shut to a caller with no token at all.
    let req = Request::builder()
        .method("GET")
        .uri("/inventory/order-book")
        .body(Body::empty())
        .unwrap();
    let (status, _) = send(&ours.app, req).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
