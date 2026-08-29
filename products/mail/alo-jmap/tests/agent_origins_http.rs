//! Provenance on the wire (ADR 0058 §4, A4.5): a record carries where it
//! came from — the thread a record was raised in, the quote an invoice was
//! raised from — set where the record is created, returned by the intents as
//! an `origin` field on the record view, and cited by the agent instead of
//! asserted bare.
//!
//! Driven end to end — a real room, the real router and store, a scripted
//! model — because the promise is about the seam: creation stamps, the store
//! keeps one answer, the read's grounding hands it to the model, and the
//! model is told to cite it.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use alo_store::AgentProduct;

use crate::common::model::{Seen, says, scripted_model, use_model, wants};
use crate::common::{Harness, harness, send};

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

/// Northstar Foods, so the draft-invoice verb has a customer to resolve.
async fn northstar(h: &Harness) -> String {
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
    body["customer"]["id"].as_str().unwrap().to_owned()
}

/// A record created out of a room carries the thread as its origin, the
/// intent's read answers with it, and the agent cites it — the full A4.5
/// promise on one record.
#[tokio::test]
async fn a_record_raised_in_a_room_carries_the_thread_and_the_agent_cites_it() {
    let h = harness("origins-thread").await;
    northstar(&h).await;
    let agent = billing_agent(&h).await;
    let room = a_room_with(&h, "finance", &agent).await;

    let (model, _seen) = scripted_model(vec![wants(
        "create_invoice_draft",
        json!({ "customer": "Northstar Foods BV",
                "lines": [{ "description": "Consulting", "quantity": 2,
                            "unitPriceCents": 10_000, "vatRateBp": 2_100 }] }),
        "I'll raise a draft invoice for Northstar.",
    )])
    .await;
    use_model(&h, &model).await;

    let spoken = ask_in_room(&h, &room, "@billing invoice Northstar for the consulting").await;
    let proposal = spoken["proposal"]["id"].as_str().unwrap().to_owned();
    let (status, decided) = post(
        &h.app,
        &h.token,
        &format!("/chat/proposals/{proposal}"),
        json!({ "approve": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{decided}");
    let invoice = decided["result"]["result"]["invoice"]["id"]
        .as_str()
        .expect("the approval answered the raised draft")
        .to_owned();

    // The execution's own reply already carries where the record came from…
    let origin = &decided["result"]["result"]["origin"];
    assert_eq!(origin["kind"], "thread", "{decided}");
    assert_eq!(origin["id"], room.as_str());
    assert_eq!(
        origin["label"], "finance",
        "the room's own name is the label"
    );

    // …and the store keeps it on the record, set once.
    let kept = h
        .acc
        .record_origin("invoice", &invoice)
        .await
        .unwrap()
        .expect("the created record was stamped");
    assert_eq!(kept.kind, "thread");
    assert_eq!(kept.id, room);
    assert_eq!(kept.label.as_deref(), Some("finance"));

    // The read returns it in the grounding, and the agent cites it: this is
    // "intents return it; agents cite it" as one exchange in the room.
    let (model, seen) = scripted_model(vec![
        wants(
            "invoice_lookup",
            json!({ "customer": "Northstar Foods BV" }),
            "Let me look at that invoice.",
        ),
        says("Northstar's draft is at 242.00 EUR — raised from the #finance thread."),
    ])
    .await;
    use_model(&h, &model).await;
    let answer = ask_in_room(&h, &room, "@billing where did Northstar's draft come from?").await;
    assert_eq!(
        answer["body"],
        "Northstar's draft is at 242.00 EUR — raised from the #finance thread."
    );
    let grounding = shown(&seen, 1);
    assert!(
        grounding.contains("\"origin\"") && grounding.contains("finance"),
        "the read's record view did not hand the model the origin: {grounding}"
    );
    // The rule the model answered under is the standing one, not a fixture.
    let prompt = alo_ai::system_prompt_for(AgentProduct::Billing);
    assert!(
        prompt.contains("origin") && prompt.contains("cite it"),
        "the agent is not told to cite provenance"
    );
}

/// An invoice raised by accepting a quote names the quote — set in the same
/// core the route and the verb share, so a person's click on
/// `/billing/quotes/{id}/accept` leaves the same provenance an agent's would,
/// and the intent's read answers with it. The specific source also beats the
/// generic thread stamp: nothing here happened in any room.
#[tokio::test]
async fn an_invoice_raised_from_a_quote_names_the_quote() {
    let h = harness("origins-quote").await;
    let customer = northstar(&h).await;
    let lines = json!([{ "description": "Managed hosting", "unit": "month", "qtyMilli": 1_000,
                         "unitPriceCents": 24_900, "vatRateBp": 2_100 }]);
    let (status, body) = post(
        &h.app,
        &h.token,
        "/billing/quotes",
        json!({ "customerId": customer, "lines": lines }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let quote = body["quote"]["id"].as_str().unwrap().to_owned();
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/billing/quotes/{quote}/send"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let number = body["quote"]["number"].as_str().unwrap().to_owned();
    let (status, accepted) = post(
        &h.app,
        &h.token,
        &format!("/billing/quotes/{quote}/accept"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{accepted}");
    let invoice = accepted["invoice"]["id"]
        .as_str()
        .expect("accepting a services quote raises a draft invoice")
        .to_owned();

    let kept = h
        .acc
        .record_origin("invoice", &invoice)
        .await
        .unwrap()
        .expect("the raised invoice was stamped with its quote");
    assert_eq!(kept.kind, "quote");
    assert_eq!(kept.id, quote);
    assert_eq!(kept.label.as_deref(), Some(number.as_str()));

    // The palette's read — a person's own tap, no room anywhere — returns the
    // record with its origin on it.
    let (status, read) = post(
        &h.app,
        &h.token,
        "/ai/agent/execute",
        json!({ "tool": "invoice_lookup", "args": { "customer": "Northstar Foods BV" } }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{read}");
    assert_eq!(read["result"]["id"], invoice);
    assert_eq!(read["result"]["origin"]["kind"], "quote");
    assert_eq!(read["result"]["origin"]["id"], quote.as_str());
    assert_eq!(read["result"]["origin"]["label"], number.as_str());
}

/// A record raised with no source stays bare: the palette is no room, so
/// nothing invents a thread, and the read answers without an `origin` —
/// which is what "never invent an origin" grounds on.
#[tokio::test]
async fn a_record_with_no_source_carries_no_origin() {
    let h = harness("origins-none").await;
    northstar(&h).await;
    let (status, made) = post(
        &h.app,
        &h.token,
        "/ai/agent/execute",
        json!({ "tool": "create_invoice_draft",
                "args": { "customer": "Northstar Foods BV",
                          "lines": [{ "description": "Consulting", "quantity": 1,
                                      "unitPriceCents": 10_000, "vatRateBp": 2_100 }] } }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{made}");
    let invoice = made["result"]["invoice"]["id"].as_str().unwrap().to_owned();
    assert!(
        made["result"].get("origin").is_none(),
        "a palette tap has no thread to stamp: {made}"
    );
    assert!(
        h.acc
            .record_origin("invoice", &invoice)
            .await
            .unwrap()
            .is_none()
    );
}
