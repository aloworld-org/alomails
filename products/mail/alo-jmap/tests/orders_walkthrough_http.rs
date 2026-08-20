//! **Wave O1's exit gate, walked end to end over the real router** (alo Orders).
//!
//! One offer for six fans becomes an order, is confirmed, ships four on one note
//! and two on another, bills each consignment, and the order book shows the
//! correct remainder at every step. Every call is the HTTP surface a client
//! actually uses; nothing reaches around it into the store.
//!
//! This is a walkthrough rather than a unit test, and it earns its place by
//! joining things each of whose own suites passes in isolation: the routing that
//! raises the order (O1.c), the refusal that will not over-promise (O1.a), the
//! link back to the offer (O1.b), delivery, invoicing, and the book (O1.d). A
//! wave whose parts each pass and whose whole was never walked is a wave nobody
//! has actually seen work.
//!
//! The figures are asserted at every step, so a regression anywhere in the arc
//! shows up as a number rather than as a shrug.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use common::{Harness, harness, send};

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

async fn get(app: &Router, token: &str, uri: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("GET")
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    send(app, req).await
}

/// Asserts a call succeeded, showing the body when it did not — a walkthrough
/// that fails must say which step and with what.
fn ok(step: &str, (status, body): (StatusCode, Value)) -> Value {
    assert_eq!(status, StatusCode::OK, "{step} failed: {body}");
    body
}

/// The five figures of the order book for one order, as the screen reads them.
async fn book_row(h: &Harness, order_id: &str) -> Value {
    let body = ok(
        "read the order book",
        get(&h.app, &h.token, "/inventory/order-book?scope=all").await,
    );
    body["orders"]
        .as_array()
        .expect("orders")
        .iter()
        .find(|o| o["id"] == order_id)
        .expect("our order is in the book")
        .clone()
}

#[tokio::test]
async fn the_fan_order_walks_from_offer_to_two_invoices_and_the_book_agrees_at_every_step() {
    let h = harness("walkthrough").await;

    // ---- the counterparty and the goods ------------------------------------
    let customer = ok(
        "create the customer",
        post(
            &h.app,
            &h.token,
            "/billing/customers",
            json!({
                "name": "Koelhuis Ventilatie BV",
                "addressLine1": "Keizersgracht 1",
                "postalCode": "1015",
                "city": "Amsterdam",
                "country": "NL",
                "currency": "EUR",
                "paymentTermsDays": 30,
            }),
        )
        .await,
    )["customer"]
        .clone();
    let fan = ok(
        "create the product",
        post(
            &h.app,
            &h.token,
            "/billing/products",
            json!({
                "name": "AF-630 axial fan",
                "unit": "piece",
                "unitPriceCents": 129_500,
                "vatRateBp": 2100,
                "stocked": true,
                "purchasePriceCents": 74_000,
            }),
        )
        .await,
    )["product"]
        .clone();

    // Six fans on the shelf, put there the only way this ledger allows.
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
    let locations = h.acc.inv_locations(false).await.unwrap();
    let of = |kind: alo_store::inv_locations::LocationKind| {
        locations
            .iter()
            .find(|l| l.kind == kind)
            .expect("seeded location")
            .id
            .clone()
    };
    let warehouse = of(alo_store::inv_locations::LocationKind::Stock);
    h.acc
        .record_move(&alo_store::inv_moves::NewMove {
            product_id: alo_store::BillingProductId::new(fan["id"].as_str().unwrap()),
            from_location_id: of(alo_store::inv_locations::LocationKind::Supplier),
            to_location_id: warehouse.clone(),
            qty_milli: 6_000,
            reason: alo_store::inv_moves::MoveReason::Purchase,
            reason_code: None,
            note: String::new(),
            reference: None,
            occurred_at: None,
        })
        .await
        .unwrap();

    // ---- 1. the offer -------------------------------------------------------
    let quote = ok(
        "raise the quote",
        post(
            &h.app,
            &h.token,
            "/billing/quotes",
            json!({
                "customerId": customer["id"],
                "reference": "RFQ-2026-88",
                "lines": [{
                    "productId": fan["id"],
                    "description": "AF-630 axial fan",
                    "unit": "piece",
                    "qtyMilli": 6_000,
                    "unitPriceCents": 129_500,
                    "vatRateBp": 2100,
                }],
            }),
        )
        .await,
    )["quote"]
        .clone();
    let quote_id = quote["id"].as_str().unwrap().to_owned();
    assert_eq!(
        quote["lines"][0]["productId"], fan["id"],
        "the offer says which item it sells — the fact the whole route turns on"
    );

    ok(
        "send the quote",
        post(
            &h.app,
            &h.token,
            &format!("/billing/quotes/{quote_id}/send"),
            json!({}),
        )
        .await,
    );

    // ---- 2. accepting it raises an ORDER, not an invoice --------------------
    let accepted = ok(
        "accept the quote",
        post(
            &h.app,
            &h.token,
            &format!("/billing/quotes/{quote_id}/accept"),
            json!({}),
        )
        .await,
    );
    assert!(
        accepted["invoice"].is_null(),
        "goods are billed once they have shipped: {accepted}"
    );
    let order = accepted["salesOrder"].clone();
    let order_id = order["id"].as_str().unwrap().to_owned();
    assert_eq!(order["status"], "draft", "acceptance never commits stock");
    assert_eq!(
        order["quoteId"].as_str(),
        Some(quote_id.as_str()),
        "and the order remembers the offer it came from"
    );
    assert_eq!(order["totals"]["netCents"], 777_000);
    let line_id = order["lines"][0]["id"].as_str().unwrap().to_owned();
    assert_eq!(order["lines"][0]["productId"], fan["id"]);

    // ---- 3. confirming promises the goods -----------------------------------
    let confirmed = ok(
        "confirm the order",
        post(
            &h.app,
            &h.token,
            &format!("/inventory/sales-orders/{order_id}/confirm"),
            json!({}),
        )
        .await,
    )["salesOrder"]
        .clone();
    assert_eq!(confirmed["status"], "confirmed");
    assert!(
        confirmed["number"]
            .as_str()
            .is_some_and(|n| n.starts_with("SO-")),
        "confirming draws the order's own number: {confirmed}"
    );

    // The book: everything promised, nothing gone.
    let row = book_row(&h, &order_id).await;
    assert_eq!(row["figures"]["orderedQtyMilli"], 6_000);
    assert_eq!(row["figures"]["reservedQtyMilli"], 6_000);
    assert_eq!(row["figures"]["deliveredQtyMilli"], 0);
    assert_eq!(row["figures"]["outstandingNetCents"], 777_000);
    assert_eq!(row["figures"]["invoicedNetCents"], 0);

    // ---- 4. four on the first note ------------------------------------------
    ok(
        "deliver four",
        post(
            &h.app,
            &h.token,
            &format!("/inventory/sales-orders/{order_id}/deliveries"),
            json!({
                "locationId": warehouse.as_str(),
                "lines": [{ "lineId": line_id, "qtyMilli": 4_000 }],
                "note": "Four on the first pallet.",
            }),
        )
        .await,
    );
    let row = book_row(&h, &order_id).await;
    assert_eq!(row["status"], "partially_delivered");
    assert_eq!(row["figures"]["deliveredQtyMilli"], 4_000);
    assert_eq!(row["figures"]["outstandingQtyMilli"], 2_000);
    assert_eq!(
        row["figures"]["reservedQtyMilli"], 2_000,
        "what has gone out stops being promised — no hook released it"
    );
    assert_eq!(row["figures"]["deliveredNetCents"], 518_000);
    assert_eq!(row["figures"]["outstandingNetCents"], 259_000);
    assert_eq!(
        row["figures"]["invoicedNetCents"], 0,
        "delivering is not billing"
    );

    // ---- 5. bill what shipped ------------------------------------------------
    let first_invoice = ok(
        "invoice the first consignment",
        post(
            &h.app,
            &h.token,
            &format!("/inventory/sales-orders/{order_id}/invoice"),
            json!({}),
        )
        .await,
    );
    // The route answers with the link between the order and the billing
    // document: which invoice, and which quantities of which ordered lines went
    // onto it.
    assert_eq!(
        first_invoice["invoice"]["lines"][0]["qtyMilli"], 4_000,
        "it bills what left the building, never what was ordered: {first_invoice}"
    );
    let first_id = first_invoice["invoice"]["invoiceId"].as_str().unwrap();
    let billed = ok(
        "read the first invoice",
        get(&h.app, &h.token, &format!("/billing/invoices/{first_id}")).await,
    );
    assert_eq!(billed["invoice"]["totals"]["netCents"], 518_000);
    assert_eq!(
        billed["invoice"]["status"], "draft",
        "billing what shipped raises a draft; issuing it is the tenant's own act"
    );
    let row = book_row(&h, &order_id).await;
    assert_eq!(row["figures"]["invoicedNetCents"], 518_000);
    assert_eq!(row["figures"]["outstandingNetCents"], 259_000);

    // ---- 6. the remaining two, and the order closes --------------------------
    ok(
        "deliver the rest",
        post(
            &h.app,
            &h.token,
            &format!("/inventory/sales-orders/{order_id}/deliveries"),
            json!({
                "locationId": warehouse.as_str(),
                "note": "The remaining two.",
            }),
        )
        .await,
    );
    let row = book_row(&h, &order_id).await;
    assert_eq!(row["status"], "delivered");
    assert_eq!(row["figures"]["deliveredQtyMilli"], 6_000);
    assert_eq!(row["figures"]["outstandingQtyMilli"], 0);
    assert_eq!(
        row["figures"]["reservedQtyMilli"], 0,
        "a finished order holds nothing against the warehouse"
    );

    // ---- 7. bill the second consignment --------------------------------------
    let second_invoice = ok(
        "invoice the second consignment",
        post(
            &h.app,
            &h.token,
            &format!("/inventory/sales-orders/{order_id}/invoice"),
            json!({}),
        )
        .await,
    );
    assert_eq!(
        second_invoice["invoice"]["lines"][0]["qtyMilli"], 2_000,
        "the NEW quantity only — the first four are not billed twice"
    );
    let second_id = second_invoice["invoice"]["invoiceId"].as_str().unwrap();
    assert_ne!(
        second_id, first_id,
        "a second consignment, a second document"
    );
    let billed_again = ok(
        "read the second invoice",
        get(&h.app, &h.token, &format!("/billing/invoices/{second_id}")).await,
    );
    assert_eq!(billed_again["invoice"]["totals"]["netCents"], 259_000);

    // ---- the books balance ---------------------------------------------------
    let row = book_row(&h, &order_id).await;
    assert_eq!(row["figures"]["orderedNetCents"], 777_000);
    assert_eq!(row["figures"]["deliveredNetCents"], 777_000);
    assert_eq!(row["figures"]["invoicedNetCents"], 777_000);
    assert_eq!(row["figures"]["outstandingNetCents"], 0);
    assert_eq!(row["figures"]["reservedNetCents"], 0);

    // The shelf is empty, which is the physical half of the same story.
    let on_hand = h
        .acc
        .inv_on_hand(
            &alo_store::BillingProductId::new(fan["id"].as_str().unwrap()),
            &warehouse,
        )
        .await
        .unwrap();
    assert_eq!(on_hand, 0, "six went out and six left the shelf");

    // And the morning's book is empty again: nothing is open.
    let open = ok(
        "read the open book",
        get(&h.app, &h.token, "/inventory/order-book").await,
    );
    assert_eq!(
        open["orders"],
        json!([]),
        "a finished order is not open business"
    );
}
