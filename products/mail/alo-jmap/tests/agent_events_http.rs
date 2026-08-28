//! The event stream on the wire (ADR 0058 §5, A4.6): an agent's approved
//! write lands on the record's own audit tab, a read joins the stream without
//! joining the record's story, and another tenant reading our record id gets
//! the answer an id that never existed gets.
//!
//! Driven end to end — a real room, the real router and store, a scripted
//! model — because the item's promise is about the seam: `execute_tool` emits,
//! the store keeps, `GET /audit` merges, and no step of that is a unit.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use alo_store::AgentProduct;

use crate::common::model::{says, scripted_model, use_model, wants};
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

/// Northstar Foods with one draft offer, returning (customer id, quote id).
async fn northstar_with_a_draft(h: &Harness) -> (String, String) {
    let (status, body) = post(
        &h.app,
        &h.token,
        "/billing/customers",
        json!({ "name": "Northstar Foods BV", "addressLine1": "Demo Street 1",
                "postalCode": "1011 AB", "city": "Amsterdam", "country": "NL",
                "paymentTermsDays": 30, "currency": "EUR" }),
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
    let quote = body["quote"]["id"].as_str().unwrap().to_owned();
    (customer, quote)
}

/// One record's merged history as `(action, agent)` pairs, newest first.
async fn history(h: &Harness, token: &str, entity: &str) -> Vec<(String, Option<String>)> {
    let (status, body) = get(&h.app, token, &format!("/audit?entity={entity}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            (
                entry["action"].as_str().unwrap().to_owned(),
                entry["agent"].as_str().map(str::to_owned),
            )
        })
        .collect()
}

/// The agent's approved write emits onto the stream, and the record's audit
/// tab shows it beside the entries a person's clicks wrote — merged, in
/// order, and never twice.
#[tokio::test]
async fn an_approved_write_lands_on_the_records_own_history() {
    let h = harness("events-write").await;
    let (_, quote) = northstar_with_a_draft(&h).await;
    let agent = billing_agent(&h).await;
    let room = a_room_with(&h, "sales", &agent).await;

    let (model, _seen) = scripted_model(vec![wants(
        "send_quote",
        json!({ "customer": "Northstar Foods BV" }),
        "I'll send Northstar their draft offer.",
    )])
    .await;
    use_model(&h, &model).await;

    let spoken = ask_in_room(&h, &room, "@billing send Northstar Foods their offer").await;
    let proposal = spoken["proposal"]["id"].as_str().unwrap().to_owned();
    let (status, decided) = post(
        &h.app,
        &h.token,
        &format!("/chat/proposals/{proposal}"),
        json!({ "approve": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{decided}");

    let entity = format!("billing.quote:{quote}");
    let entries = history(&h, &h.token, &entity).await;
    let sends: Vec<_> = entries
        .iter()
        .filter(|(action, _)| action == "send_quote")
        .collect();
    assert_eq!(sends.len(), 1, "once and only once: {entries:?}");
    assert_eq!(
        sends[0].1.as_deref(),
        Some("billing"),
        "the item names the agent that ran it"
    );
    // The person's own click that raised the draft is still there, from the
    // audit middleware — the merge holds both sources.
    assert!(
        entries
            .iter()
            .any(|(action, _)| action == "billing.quote.create"),
        "{entries:?}"
    );
    // …and the stream kept the execution with the record it touched.
    let mine = h.acc.my_events(50).await.unwrap();
    let sent = mine
        .iter()
        .find(|event| event.kind == "send_quote")
        .expect("the write is on the stream");
    assert_eq!(sent.effect, "write");
    assert_eq!(sent.record_type.as_deref(), Some("quote"));
    assert_eq!(sent.record_id.as_deref(), Some(quote.as_str()));
    assert_eq!(sent.agent.as_deref(), Some("billing"));
    assert_eq!(sent.actor.as_deref(), Some(h.email.as_str()));
}

/// A read inside the turn joins the stream — it is an intent execution — but
/// never joins any record's story.
#[tokio::test]
async fn a_read_joins_the_stream_and_no_records_story() {
    let h = harness("events-read").await;
    let (_, quote) = northstar_with_a_draft(&h).await;
    let agent = billing_agent(&h).await;
    let room = a_room_with(&h, "sales", &agent).await;

    let (model, _seen) = scripted_model(vec![
        wants("open_quotes", json!({}), "Let me look at the open offers."),
        says("Nothing is open; one draft."),
    ])
    .await;
    use_model(&h, &model).await;
    ask_in_room(&h, &room, "@billing which quotes are open?").await;

    let mine = h.acc.my_events(50).await.unwrap();
    let looked = mine
        .iter()
        .find(|event| event.kind == "open_quotes")
        .expect("the read is on the stream");
    assert_eq!(looked.effect, "read");
    assert_eq!(looked.record_id, None, "a list read is about no one record");
    // The quote's own history knows nothing of who looked around.
    let entries = history(&h, &h.token, &format!("billing.quote:{quote}")).await;
    assert!(
        entries.iter().all(|(action, _)| action != "open_quotes"),
        "{entries:?}"
    );
}

/// Another tenant asking with our exact record id gets an empty history —
/// exactly like an id that was never issued, on both of the merge's sources.
#[tokio::test]
async fn another_tenant_reads_nothing_of_our_stream() {
    let h = harness("events-tenant-a").await;
    let (_, quote) = northstar_with_a_draft(&h).await;
    let agent = billing_agent(&h).await;
    let room = a_room_with(&h, "sales", &agent).await;

    let (model, _seen) = scripted_model(vec![wants(
        "send_quote",
        json!({ "customer": "Northstar Foods BV" }),
        "Sending the offer.",
    )])
    .await;
    use_model(&h, &model).await;
    let spoken = ask_in_room(&h, &room, "@billing send Northstar Foods their offer").await;
    let proposal = spoken["proposal"]["id"].as_str().unwrap().to_owned();
    let (status, decided) = post(
        &h.app,
        &h.token,
        &format!("/chat/proposals/{proposal}"),
        json!({ "approve": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{decided}");
    assert!(
        !history(&h, &h.token, &format!("billing.quote:{quote}"))
            .await
            .is_empty()
    );

    let other = harness_on(std::sync::Arc::clone(&h.store), "events-tenant-b").await;
    let stolen = history(&h, &other.token, &format!("billing.quote:{quote}")).await;
    assert!(
        stolen.is_empty(),
        "another tenant read our history: {stolen:?}"
    );
}
