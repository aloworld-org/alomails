//! The Chat agent over its intents (AC.1, ADR 0058), on the wire: in a real
//! room, against the real router and store, with a scripted model.
//!
//! Three properties are held here. A question about the asker's rooms is
//! answered from the record — the same summaries the sidebar draws — inside
//! the turn, with no button in between. A message is only ever posted as a
//! previewed proposal the asker approves, and lands in the room **in their own
//! name**. And another tenant's room does not exist for this agent: not as a
//! denial, but as a room that was never there.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
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

async fn chat_agent(h: &Harness) -> String {
    let handle = alo_store::default_handle(AgentProduct::Chat);
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

/// A public room by this name, with the given agent listening in it.
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

#[tokio::test]
async fn what_rooms_am_i_in_is_answered_from_the_record() {
    let h = harness("chat-intents-rooms").await;
    let agent = chat_agent(&h).await;
    // Two rooms besides the one the agent is asked in, one with a last word.
    let release = a_room_with(&h, "release", &agent).await;
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/chat/channels/{release}/messages"),
        json!({ "body": "the launch is Friday" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = post(
        &h.app,
        &h.token,
        "/chat/channels",
        json!({ "kind": "channel", "name": "ops", "visibility": "public" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let room = a_room_with(&h, "ask", &agent).await;

    let (model, seen) = scripted_model(vec![
        wants("my_rooms", json!({}), "Let me look at your conversations."),
        says("You are in #release, #ops and #ask [1]."),
    ])
    .await;
    use_model(&h, &model).await;

    let answer = ask_in_room(&h, &room, "@chat what conversations am I in?").await;
    assert_eq!(answer["body"], "You are in #release, #ops and #ask [1].");
    assert!(
        answer["proposal"].is_null(),
        "a read is answered, never proposed"
    );

    // The agent was offered its verbs — reads and writes alike, from the
    // intent registry — and the read's record view came back as a source.
    let prompt = offered(&seen, 0);
    for verb in [
        "my_rooms",
        "unread_rooms",
        "room_members",
        "catch_up_room",
        "find_in_chat",
        "post_message",
        "create_room",
    ] {
        assert!(
            prompt.contains(&format!("- {verb}:")),
            "the prompt does not offer {verb}"
        );
    }
    let sources = shown(&seen, 1);
    assert!(sources.contains("release"), "{sources}");
    assert!(sources.contains("ops"), "{sources}");
    assert!(
        sources.contains("the launch is Friday"),
        "the room's last word is in the sources: {sources}"
    );
}

#[tokio::test]
async fn who_is_in_a_room_reads_the_membership() {
    let h = harness("chat-intents-members").await;
    let agent = chat_agent(&h).await;
    a_room_with(&h, "launch", &agent).await;
    let room = a_room_with(&h, "ask", &agent).await;

    let (model, seen) = scripted_model(vec![
        wants(
            "room_members",
            json!({ "room": "launch" }),
            "Let me look at who is in #launch.",
        ),
        says("Only you are in #launch [1]."),
    ])
    .await;
    use_model(&h, &model).await;

    let answer = ask_in_room(&h, &room, "@chat who is in #launch?").await;
    assert_eq!(answer["body"], "Only you are in #launch [1].");
    assert!(answer["proposal"].is_null());
    let sources = shown(&seen, 1);
    assert!(sources.contains("\"found\":true"), "{sources}");
    assert!(
        sources.contains(&h.email),
        "the member's address is in the sources: {sources}"
    );
    assert!(sources.contains("owner"), "{sources}");
}

#[tokio::test]
async fn posting_a_message_is_proposed_and_lands_only_on_approval() {
    let h = harness("chat-intents-post").await;
    let agent = chat_agent(&h).await;
    let general = a_room_with(&h, "general", &agent).await;
    let room = a_room_with(&h, "ask", &agent).await;

    let (model, _seen) = scripted_model(vec![wants(
        "post_message",
        json!({ "room": "general", "message": "The deploy is done." }),
        "I'll post that to #general for you.",
    )])
    .await;
    use_model(&h, &model).await;

    let answer = ask_in_room(&h, &room, "@chat tell #general the deploy is done").await;
    assert!(
        !answer["proposal"].is_null(),
        "a write is a proposal: {answer}"
    );
    assert_eq!(answer["proposal"]["tool"], "post_message");
    // Nothing ran without a tap: the room has no such message.
    let said = |all: &[Value]| all.iter().any(|m| m["body"] == "The deploy is done.");
    assert!(!said(&messages(&h, &general).await), "posted early");

    // The asker approves — and the words land in the room in THEIR name.
    let proposal = answer["proposal"]["id"].as_str().unwrap().to_owned();
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/chat/proposals/{proposal}"),
        json!({ "approve": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let all = messages(&h, &general).await;
    let posted = all
        .iter()
        .find(|m| m["body"] == "The deploy is done.")
        .expect("the approved message is in the room");
    assert_eq!(
        posted["authorKind"], "user",
        "the asker speaks, not the agent"
    );
    assert_eq!(posted["author"], h.account_id);
}

#[tokio::test]
async fn creating_a_room_is_proposed_and_created_only_on_approval() {
    let h = harness("chat-intents-create").await;
    let agent = chat_agent(&h).await;
    let room = a_room_with(&h, "ask", &agent).await;

    let (model, _seen) = scripted_model(vec![wants(
        "create_room",
        json!({ "name": "audit", "visibility": "private" }),
        "I'll set up a private #audit room for you.",
    )])
    .await;
    use_model(&h, &model).await;

    let answer = ask_in_room(&h, &room, "@chat make a private room called audit").await;
    assert!(!answer["proposal"].is_null(), "{answer}");
    assert_eq!(answer["proposal"]["tool"], "create_room");
    let named_audit = |body: &Value| {
        body["channels"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["name"] == "audit")
    };
    let (status, body) = get(&h.app, &h.token, "/chat/channels").await;
    assert_eq!(status, StatusCode::OK);
    assert!(!named_audit(&body), "created early: {body}");

    let proposal = answer["proposal"]["id"].as_str().unwrap().to_owned();
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/chat/proposals/{proposal}"),
        json!({ "approve": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = get(&h.app, &h.token, "/chat/channels").await;
    assert_eq!(status, StatusCode::OK);
    let audit = body["channels"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "audit")
        .expect("the approved room exists")
        .clone();
    assert_eq!(audit["visibility"], "private");
}

#[tokio::test]
async fn another_tenants_room_does_not_exist_for_this_agent() {
    let h = harness("chat-intents-iso").await;
    // Another tenant on the same store, with a room and a secret in it.
    let other = harness_on(Arc::clone(&h.store), "chat-intents-iso2").await;
    let (status, body) = post(
        &other.app,
        &other.token,
        "/chat/channels",
        json!({ "kind": "channel", "name": "warroom", "visibility": "public" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let warroom = body["id"].as_str().unwrap().to_owned();
    let (status, body) = post(
        &other.app,
        &other.token,
        &format!("/chat/channels/{warroom}/messages"),
        json!({ "body": "the secret plan" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let agent = chat_agent(&h).await;
    let room = a_room_with(&h, "ask", &agent).await;
    let (model, seen) = scripted_model(vec![
        wants(
            "catch_up_room",
            json!({ "room": "warroom" }),
            "Let me look at #warroom.",
        ),
        says("You are not in a room called warroom."),
    ])
    .await;
    use_model(&h, &model).await;

    let answer = ask_in_room(&h, &room, "@chat what did I miss in #warroom?").await;
    assert_eq!(answer["body"], "You are not in a room called warroom.");
    // The other tenant's room reads as absent — not forbidden, absent — and
    // not a word of it reaches this tenant's model.
    let sources = shown(&seen, 1);
    assert!(sources.contains("\"found\":false"), "{sources}");
    assert!(
        !sources.contains("the secret plan"),
        "another tenant's words leaked into the sources: {sources}"
    );
}
