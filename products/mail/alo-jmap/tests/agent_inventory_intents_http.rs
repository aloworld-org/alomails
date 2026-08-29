//! The Inventory agent over its intents (AA.4, ADR 0058), on the wire: in a
//! real room, against the real router and store, with a scripted model.
//!
//! Before the move to intents the Inventory agent could answer for one product
//! and draft reorders, but "@inventory what is on order?" had no verb to run.
//! This suite holds the opposite: the read runs inside the turn and answers
//! from the order book — the orders as `GET /inventory/purchase-orders` serves
//! them — a write (booking a delivery) is proposed, previewed and not run, and
//! another tenant's suppliers and orders are unreachable from the overview.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use alo_store::AgentProduct;

use crate::common::model::{Seen, says, scripted_model, use_model, wants};
use crate::common::{Harness, harness, harness_on, send};

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
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    send(app, req).await
}

async fn inventory_agent(h: &Harness) -> String {
    let handle = alo_store::default_handle(AgentProduct::Inventory);
    let (status, body) = get(&h.app, &h.token, "/chat/agents").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["agents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|agent| agent["handle"] == handle)
        .unwrap_or_else(|| panic!("no @{handle} among this tenant's agents: {body}"))["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn a_room_with(h: &Harness, name: &str, agent: &str) -> String {
    let (status, body) = post(
        &h.app,
        &h.token,
        "/chat/channels",
        json!({ "kind": "channel", "name": name, "visibility": "public" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let channel = body["id"].as_str().unwrap().to_owned();
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/chat/channels/{channel}/agents"),
        json!({ "agent": agent }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    channel
}

async fn messages(h: &Harness, channel: &str) -> Vec<Value> {
    let (status, body) = get(
        &h.app,
        &h.token,
        &format!("/chat/channels/{channel}/messages"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["messages"].as_array().unwrap().clone()
}

/// Says something in the room and waits for the agent's reply.
async fn ask_in_room(h: &Harness, channel: &str, question: &str) -> Value {
    let before = messages(h, channel).await.len();
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/chat/channels/{channel}/messages"),
        json!({ "body": question }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let all = messages(h, channel).await;
        if let Some(message) = all
            .iter()
            .filter(|m| m["authorKind"] == "agent")
            .find(|_| all.len() > before + 1)
        {
            return message.clone();
        }
        assert!(Instant::now() < deadline, "the agent never spoke");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// The last message of the model's `n`th call — the numbered sources as the
/// model saw them, tool results included.
fn shown(seen: &Seen, n: usize) -> String {
    let asked = seen.lock().unwrap().clone();
    let messages = asked
        .get(n)
        .unwrap_or_else(|| panic!("the model was not called {} times", n + 1))["messages"]
        .as_array()
        .unwrap()
        .clone();
    messages.last().unwrap()["content"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// The system prompt of the model's `n`th call.
fn offered(seen: &Seen, n: usize) -> String {
    let asked = seen.lock().unwrap().clone();
    asked[n]["messages"][0]["content"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// A supplier, by name, returning its id.
async fn a_supplier(h: &Harness, name: &str) -> String {
    let (status, body) = post(
        &h.app,
        &h.token,
        "/inventory/suppliers",
        json!({ "name": name, "country": "SE" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["supplier"]["id"].as_str().unwrap().to_owned()
}

/// A draft purchase order with one line, returning its id.
async fn a_draft_order(h: &Harness, supplier: &str) -> String {
    let (status, body) = post(
        &h.app,
        &h.token,
        "/inventory/purchase-orders",
        json!({
            "supplierId": supplier,
            "lines": [{ "description": "Oak planks", "unit": "piece",
                        "qtyMilli": 5_000, "unitPriceCents": 1_200 }],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["purchaseOrder"]["id"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn what_is_on_order_is_answered_from_the_order_book() {
    let h = harness("inv-intents-orders").await;
    let supplier = a_supplier(&h, "Nordic Timber").await;
    a_draft_order(&h, &supplier).await;

    let agent = inventory_agent(&h).await;
    let room = a_room_with(&h, "warehouse", &agent).await;
    let (model, seen) = scripted_model(vec![
        wants(
            "open_purchase_orders",
            json!({}),
            "Let me read the order book.",
        ),
        says("One draft with Nordic Timber, nothing sent yet [1]."),
    ])
    .await;
    use_model(&h, &model).await;

    let answer = ask_in_room(&h, &room, "@inventory what is on order?").await;
    assert_eq!(
        answer["body"],
        "One draft with Nordic Timber, nothing sent yet [1]."
    );
    assert!(
        answer["proposal"].is_null(),
        "a read is answered, never proposed"
    );

    // The agent was offered its verbs — reads and writes alike, from the
    // intent registry.
    let prompt = offered(&seen, 0);
    for verb in [
        "stock_answer",
        "stock_below_minimum",
        "open_purchase_orders",
        "supplier_prices",
        "recent_moves",
        "reorder_proposals",
        "receive_delivery",
    ] {
        assert!(
            prompt.contains(&format!("- {verb}:")),
            "the prompt does not offer {verb}"
        );
    }
    // The read's rows are the order list's own: the draft with its supplier's
    // name, its status and its server-computed totals.
    let sources = shown(&seen, 1);
    assert!(sources.contains("Nordic Timber"), "{sources}");
    assert!(sources.contains("\"status\":\"draft\""), "{sources}");
    assert!(sources.contains("\"totals\""), "{sources}");
}

#[tokio::test]
async fn booking_a_delivery_is_proposed_and_not_run() {
    let h = harness("inv-intents-receive").await;
    let supplier = a_supplier(&h, "Nordic Timber").await;
    let order = a_draft_order(&h, &supplier).await;
    let agent = inventory_agent(&h).await;
    let room = a_room_with(&h, "warehouse", &agent).await;

    let (model, _seen) = scripted_model(vec![wants(
        "receive_delivery",
        json!({ "order": "Nordic Timber", "location": "Main warehouse" }),
        "I'll book the delivery in.",
    )])
    .await;
    use_model(&h, &model).await;

    let answer = ask_in_room(
        &h,
        &room,
        "@inventory the Nordic Timber delivery arrived, book it into the main warehouse",
    )
    .await;
    assert!(
        !answer["proposal"].is_null(),
        "a write is a proposal: {answer}"
    );
    assert_eq!(answer["proposal"]["tool"], "receive_delivery");
    // Nothing ran without a tap: no goods moved against the order, and it is
    // still an unnumbered draft.
    let (status, body) = get(
        &h.app,
        &h.token,
        &format!("/inventory/purchase-orders/{order}/receipts"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["receipts"].as_array().unwrap().len(), 0, "{body}");
    let (status, body) = get(
        &h.app,
        &h.token,
        &format!("/inventory/purchase-orders/{order}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["purchaseOrder"]["status"], "draft", "{body}");
    assert!(body["purchaseOrder"]["number"].is_null(), "{body}");
}

#[tokio::test]
async fn another_tenants_orders_are_unreachable() {
    let h = harness("inv-intents-iso-a").await;
    let other = harness_on(h.store.clone(), "inv-intents-iso-b").await;
    // Tenant B's supplier names a counterparty, which is exactly the kind of
    // word that must never cross a tenant wall.
    let their_supplier = a_supplier(&other, "Nightingale Timber").await;
    a_draft_order(&other, &their_supplier).await;
    let agent = inventory_agent(&h).await;
    let room = a_room_with(&h, "warehouse", &agent).await;

    let (model, seen) = scripted_model(vec![
        wants(
            "open_purchase_orders",
            json!({}),
            "Let me read the order book.",
        ),
        says("Nothing is on order."),
    ])
    .await;
    use_model(&h, &model).await;

    let answer = ask_in_room(&h, &room, "@inventory what is on order?").await;
    assert_eq!(answer["body"], "Nothing is on order.");
    // What the model was shown is tenant A's own order book and none of
    // tenant B's record — not the order, not the supplier's name.
    let sources = shown(&seen, 1);
    assert!(sources.contains("\"orderCount\":0"), "{sources}");
    assert!(!sources.contains("Nightingale Timber"), "{sources}");
    assert!(!sources.contains("inv-intents-iso-b"), "{sources}");
}
