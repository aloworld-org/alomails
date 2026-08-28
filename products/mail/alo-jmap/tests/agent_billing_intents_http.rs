//! The Billing agent over its intents (ADR 0058), on the wire: in a real room,
//! against the real router and store, with a scripted model.
//!
//! What the 2026-08-28 evaluation showed was an agent that answered "I could
//! not find it" to "which quotes are open?" with nineteen quotes in the
//! record, because no read existed. This suite holds the opposite: the read
//! runs inside the turn, the record view comes back to the model as a source,
//! the answer lands in the room — and a write is proposed, previewed and not
//! run.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use alo_store::AgentProduct;

use common::model::{Seen, says, scripted_model, use_model, wants};
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
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    send(app, req).await
}

async fn billing_agent(h: &Harness) -> String {
    let handle = alo_store::default_handle(AgentProduct::Billing);
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

/// Says something in the room and waits for the agent's reply — the newest
/// agent message after `after` seqs already there.
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

/// Northstar Foods with one sent offer for managed hosting, and a draft.
async fn northstar_with_an_open_offer(h: &Harness) -> (String, String) {
    let (status, body) = post(
        &h.app,
        &h.token,
        "/billing/customers",
        json!({ "name": "Northstar Foods BV", "addressLine1": "Demo Street 1", "postalCode": "1011 AB",
                "city": "Amsterdam", "country": "NL", "paymentTermsDays": 30, "currency": "EUR" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let customer = body["customer"]["id"].as_str().unwrap().to_owned();
    let lines = json!([{ "description": "Managed hosting", "unit": "month", "qtyMilli": 1_000,
                         "unitPriceCents": 24_900, "vatRateBp": 2_100 }]);
    let (status, body) = post(
        &h.app,
        &h.token,
        "/billing/quotes",
        json!({ "customerId": customer, "reference": "RFQ-77", "lines": lines }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let sent = body["quote"]["id"].as_str().unwrap().to_owned();
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/billing/quotes/{sent}/send"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let number = body["quote"]["number"].as_str().unwrap().to_owned();
    // …and a draft that must NOT be reported as open.
    let (status, body) = post(
        &h.app,
        &h.token,
        "/billing/quotes",
        json!({ "customerId": customer, "lines": lines }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    (customer, number)
}

#[tokio::test]
async fn which_quotes_are_open_is_answered_from_the_record() {
    let h = harness("bill-intents-open").await;
    let (_, number) = northstar_with_an_open_offer(&h).await;
    let agent = billing_agent(&h).await;
    let room = a_room_with(&h, "sales", &agent).await;

    let (model, seen) = scripted_model(vec![
        wants("open_quotes", json!({}), "Let me look at the open offers."),
        says("One offer is open: [1]."),
    ])
    .await;
    use_model(&h, &model).await;

    let answer = ask_in_room(
        &h,
        &room,
        "@billing which quotes are open right now, and what are they worth?",
    )
    .await;
    assert_eq!(answer["body"], "One offer is open: [1].");
    assert!(
        answer["proposal"].is_null(),
        "a read is answered, never proposed"
    );

    // The agent was offered its verbs — reads and writes alike, from the
    // intent registry — and the read's record view came back as a source.
    let prompt = offered(&seen, 0);
    for verb in [
        "open_quotes",
        "quote_lookup",
        "unpaid_invoices",
        "billing_totals",
        "send_quote",
        "record_payment",
    ] {
        assert!(
            prompt.contains(&format!("- {verb}:")),
            "the prompt does not offer {verb}"
        );
    }
    let sources = shown(&seen, 1);
    assert!(
        sources.contains(&number),
        "the open quote's number is in the sources: {sources}"
    );
    assert!(
        sources.contains("Northstar Foods BV"),
        "the customer's name is in the sources"
    );
    assert!(
        sources.contains("\"gross_cents\":30129") || sources.contains("30129"),
        "the gross total is in the sources: {sources}"
    );
    assert!(
        sources.contains("\"openCount\":1"),
        "one open offer: {sources}"
    );
    assert!(
        sources.contains("\"draftCount\":1"),
        "the draft is counted, not listed: {sources}"
    );
}

#[tokio::test]
async fn what_did_we_quote_a_customer_reads_the_whole_offer() {
    let h = harness("bill-intents-lookup").await;
    let (_, number) = northstar_with_an_open_offer(&h).await;
    let agent = billing_agent(&h).await;
    let room = a_room_with(&h, "sales", &agent).await;

    let (model, seen) = scripted_model(vec![
        wants(
            "quote_lookup",
            json!({ "customer": "Northstar" }),
            "Looking up Northstar's offer.",
        ),
        says("Managed hosting, 249.00 a month; sent [1]."),
    ])
    .await;
    use_model(&h, &model).await;

    let answer = ask_in_room(
        &h,
        &room,
        "@billing what did we quote Northstar Foods, and has it been sent?",
    )
    .await;
    assert_eq!(answer["body"], "Managed hosting, 249.00 a month; sent [1].");
    let sources = shown(&seen, 1);
    // The sent offer is the one "what did we quote X" means — its number, its
    // lines — and the newer draft is listed beside it, not instead of it.
    assert!(sources.contains("Managed hosting"), "{sources}");
    assert!(sources.contains(&number), "{sources}");
    assert!(
        sources.contains("\"status\":\"sent\""),
        "the sent offer is the answer: {sources}"
    );
    assert!(
        sources.contains("\"otherQuotes\":[{"),
        "the draft is listed beside it: {sources}"
    );
    assert!(sources.contains("\"status\":\"draft\""), "{sources}");
}

#[tokio::test]
async fn sending_an_offer_is_proposed_and_not_run() {
    let h = harness("bill-intents-send").await;
    let (customer, _) = northstar_with_an_open_offer(&h).await;
    let agent = billing_agent(&h).await;
    let room = a_room_with(&h, "sales", &agent).await;

    let (model, _seen) = scripted_model(vec![wants(
        "send_quote",
        json!({ "customer": "Northstar Foods BV" }),
        "I'll send Northstar their draft offer.",
    )])
    .await;
    use_model(&h, &model).await;

    let answer = ask_in_room(&h, &room, "@billing send Northstar Foods their offer").await;
    assert!(
        !answer["proposal"].is_null(),
        "a write is a proposal: {answer}"
    );
    assert_eq!(answer["proposal"]["tool"], "send_quote");
    // The draft is still a draft: nothing ran without a tap.
    let (status, body) = get(&h.app, &h.token, "/billing/quotes?status=draft").await;
    assert_eq!(status, StatusCode::OK);
    let drafts = body["quotes"].as_array().unwrap();
    assert_eq!(
        drafts
            .iter()
            .filter(|q| q["customerId"] == customer)
            .count(),
        1,
        "{body}"
    );
}
