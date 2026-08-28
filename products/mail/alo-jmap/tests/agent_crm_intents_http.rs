//! The CRM agent over its intents (AA.1, ADR 0058), on the wire: in a real
//! room, against the real router and store, with a scripted model.
//!
//! Before the move to intents the CRM agent had only writes: "@crm which deals
//! are open?" had no verb to run and the agent answered from nothing. This
//! suite holds the opposite: the read runs inside the turn, the board's own
//! record views come back to the model as a source, the answer lands in the
//! room — a write is proposed, previewed and not run — and another tenant's
//! deal is unreachable by its exact title.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use alo_store::AgentProduct;

use common::model::{Seen, says, scripted_model, use_model, wants};
use common::{Harness, harness, harness_on, send};

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

async fn crm_agent(h: &Harness) -> String {
    let handle = alo_store::default_handle(AgentProduct::Crm);
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

/// The tenant's seeded board and its columns, left to right.
async fn the_board(h: &Harness) -> (String, Vec<(String, String)>) {
    let (status, body) = get(&h.app, &h.token, "/crm/pipelines").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let pipeline = body["pipelines"][0]["id"].as_str().unwrap().to_owned();
    let (status, body) = get(
        &h.app,
        &h.token,
        &format!("/crm/pipelines/{pipeline}/stages"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let stages = body["stages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["id"].as_str().unwrap().to_owned(),
                s["name"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    (pipeline, stages)
}

async fn a_deal(h: &Harness, pipeline: &str, stage: &str, body: Value) -> String {
    let mut deal = body;
    deal["pipelineId"] = json!(pipeline);
    deal["stageId"] = json!(stage);
    let (status, body) = post(&h.app, &h.token, "/crm/deals", deal).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["deal"]["id"].as_str().unwrap().to_owned()
}

/// Two open deals on the seeded board: the kestrel in the first column, the
/// falcon moved to the second.
async fn a_board_with_two_deals(h: &Harness) -> (String, Vec<(String, String)>, String, String) {
    let (pipeline, stages) = the_board(h).await;
    let kestrel = a_deal(
        h,
        &pipeline,
        &stages[0].0,
        json!({ "title": "Kestrel Windfarm", "companyName": "Northstar Foods BV",
                "contactName": "Ada", "contactEmail": "ada@northstar.test",
                "valueCents": 500_000, "currency": "EUR" }),
    )
    .await;
    let falcon = a_deal(
        h,
        &pipeline,
        &stages[0].0,
        json!({ "title": "Falcon Rollout", "companyName": "Acme GmbH",
                "valueCents": 250_000, "currency": "EUR" }),
    )
    .await;
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/crm/deals/{falcon}/stage"),
        json!({ "stageId": stages[1].0 }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    (pipeline, stages, kestrel, falcon)
}

#[tokio::test]
async fn which_deals_are_open_is_answered_from_the_record() {
    let h = harness("crm-intents-open").await;
    let (_, stages, _, _) = a_board_with_two_deals(&h).await;
    let agent = crm_agent(&h).await;
    let room = a_room_with(&h, "sales", &agent).await;

    let (model, seen) = scripted_model(vec![
        wants("open_deals", json!({}), "Let me look at the board."),
        says("Two deals are open: [1]."),
    ])
    .await;
    use_model(&h, &model).await;

    let answer = ask_in_room(&h, &room, "@crm which deals are open, and at what stage?").await;
    assert_eq!(answer["body"], "Two deals are open: [1].");
    assert!(
        answer["proposal"].is_null(),
        "a read is answered, never proposed"
    );

    // The agent was offered its verbs — reads and writes alike, from the
    // intent registry — and the read's record view came back as a source.
    let prompt = offered(&seen, 0);
    for verb in [
        "open_deals",
        "deal_lookup",
        "pipeline_summary",
        "company_history",
        "create_deal",
        "move_deal_stage",
        "draft_followup",
    ] {
        assert!(
            prompt.contains(&format!("- {verb}:")),
            "the prompt does not offer {verb}"
        );
    }
    let sources = shown(&seen, 1);
    assert!(sources.contains("Kestrel Windfarm"), "{sources}");
    assert!(sources.contains("Falcon Rollout"), "{sources}");
    assert!(sources.contains("\"dealCount\":2"), "{sources}");
    // Each card names its column, and each column tallies its cards.
    assert!(
        sources.contains(&format!("\"stageName\":\"{}\"", stages[0].1)),
        "{sources}"
    );
    assert!(
        sources.contains(&format!("\"stageName\":\"{}\"", stages[1].1)),
        "{sources}"
    );
    assert!(sources.contains("\"byStage\""), "{sources}");
    assert!(sources.contains("\"byOwner\""), "{sources}");
    // The money is the integer of the record, with its reading beside it.
    assert!(sources.contains("\"valueCents\":500000"), "{sources}");
    assert!(
        sources.contains("\"valueDisplay\":\"5000.00 EUR\""),
        "{sources}"
    );
}

#[tokio::test]
async fn where_are_we_with_a_deal_reads_its_history_and_notes() {
    let h = harness("crm-intents-lookup").await;
    let (_, stages, kestrel, _) = a_board_with_two_deals(&h).await;
    // A note on the deal, and a move — the trail the lookup must carry.
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/crm/deals/{kestrel}/activities"),
        json!({ "kind": "call", "body": "Called Ada — she wants a revised offer" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/crm/deals/{kestrel}/stage"),
        json!({ "stageId": stages[1].0 }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let agent = crm_agent(&h).await;
    let room = a_room_with(&h, "sales", &agent).await;

    let (model, seen) = scripted_model(vec![
        wants(
            "deal_lookup",
            json!({ "company": "Northstar" }),
            "Looking up the Northstar deal.",
        ),
        says("It stands in the second column; Ada wants a revised offer [1]."),
    ])
    .await;
    use_model(&h, &model).await;

    let answer = ask_in_room(&h, &room, "@crm where are we with Northstar Foods?").await;
    assert_eq!(
        answer["body"],
        "It stands in the second column; Ada wants a revised offer [1]."
    );
    let sources = shown(&seen, 1);
    assert!(sources.contains("Kestrel Windfarm"), "{sources}");
    assert!(
        sources.contains(&format!("\"stageName\":\"{}\"", stages[1].1)),
        "the deal stands where it was moved: {sources}"
    );
    assert!(
        sources.contains("Called Ada — she wants a revised offer"),
        "the note is in the sources: {sources}"
    );
    assert!(
        sources.contains(&format!("\"toStageName\":\"{}\"", stages[1].1)),
        "the move is in the history: {sources}"
    );
}

#[tokio::test]
async fn moving_a_deal_is_proposed_and_not_run() {
    let h = harness("crm-intents-move").await;
    let (_, stages, kestrel, _) = a_board_with_two_deals(&h).await;
    let agent = crm_agent(&h).await;
    let room = a_room_with(&h, "sales", &agent).await;

    let (model, _seen) = scripted_model(vec![wants(
        "move_deal_stage",
        json!({ "deal": "Kestrel Windfarm", "stage": stages[1].1 }),
        "I'll move the kestrel deal along.",
    )])
    .await;
    use_model(&h, &model).await;

    let answer = ask_in_room(&h, &room, "@crm move Kestrel Windfarm along").await;
    assert!(
        !answer["proposal"].is_null(),
        "a write is a proposal: {answer}"
    );
    assert_eq!(answer["proposal"]["tool"], "move_deal_stage");
    // The card has not moved: nothing ran without a tap.
    let (status, body) = get(&h.app, &h.token, &format!("/crm/deals/{kestrel}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["deal"]["stageId"], stages[0].0.as_str(), "{body}");
}

#[tokio::test]
async fn another_tenants_deal_is_unreachable_by_its_exact_title() {
    let h = harness("crm-intents-iso-a").await;
    let other = harness_on(h.store.clone(), "crm-intents-iso-b").await;
    // Tenant B holds the deal, under exactly the title tenant A will ask for.
    a_board_with_two_deals(&other).await;
    let agent = crm_agent(&h).await;
    let room = a_room_with(&h, "sales", &agent).await;

    let (model, seen) = scripted_model(vec![
        wants(
            "deal_lookup",
            json!({ "deal": "Kestrel Windfarm" }),
            "Looking up the kestrel deal.",
        ),
        says("There is no deal by that name."),
    ])
    .await;
    use_model(&h, &model).await;

    let answer = ask_in_room(&h, &room, "@crm where are we with Kestrel Windfarm?").await;
    assert_eq!(answer["body"], "There is no deal by that name.");
    // What the model was shown carries the refusal and none of tenant B's
    // record — not the company, not the contact, not the value.
    let sources = shown(&seen, 1);
    assert!(sources.contains("no deal is titled"), "{sources}");
    assert!(!sources.contains("Northstar Foods BV"), "{sources}");
    assert!(!sources.contains("ada@northstar.test"), "{sources}");
    assert!(!sources.contains("500000"), "{sources}");
}
