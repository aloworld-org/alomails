//! The Insights agent over its intents (AC.3, ADR 0058), on the wire: in a
//! real room, against the real router and store, with a scripted model.
//!
//! The figures half (catalog, answer, change, report) is proven end to end in
//! `agent_insights_http`; this suite holds the half AC.3 adds — the *boards*.
//! What is already pinned is answered from the stored boards inside the turn,
//! with no button in between; pinning one more chart is only ever a previewed
//! proposal that is validated and answered before the tile lands, once the
//! asker approves. And another tenant's boards do not exist for this agent: a
//! board of theirs asked for by name earns the words an invented name earns,
//! and not one word of their captions reaches this tenant's model.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::{Duration as Wait, Instant};

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

async fn insights_agent(h: &Harness) -> String {
    let handle = alo_store::default_handle(AgentProduct::Insights);
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
    let deadline = Instant::now() + Wait::from_secs(20);
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
        tokio::time::sleep(Wait::from_millis(50)).await;
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

/// A specification any tenant can answer, empty books included: how much was
/// billed over the last three months, as one figure.
fn billed_lately() -> Value {
    json!({
        "schema_version": 1,
        "dataset": "billing.documents",
        "measure": { "id": "net", "agg": "sum" },
        "period": { "kind": "last_n", "n": 3, "grain": "month" },
        "viz": "number",
    })
}

/// A board made over the product's own route, so the agent reads exactly what
/// the screen made.
async fn a_board(h: &Harness, name: &str) -> String {
    let (status, body) = post(
        &h.app,
        &h.token,
        "/insights/dashboards",
        json!({ "name": name }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["dashboard"]["id"].as_str().unwrap().to_owned()
}

/// The titles pinned to a board, over the route the screen reads.
async fn tile_titles(h: &Harness, board: &str) -> Vec<String> {
    let (status, body) = get(&h.app, &h.token, &format!("/insights/dashboards/{board}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["tiles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tile| tile["title"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn whats_on_the_board_is_answered_from_the_stored_tiles() {
    let h = harness("insights-intents-tiles").await;
    let board = a_board(&h, "Sales").await;
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/insights/dashboards/{board}/tiles"),
        json!({ "title": "Billed lately", "spec": billed_lately() }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let agent = insights_agent(&h).await;
    let room = a_room_with(&h, "ask", &agent).await;
    let (model, seen) = scripted_model(vec![
        wants(
            "dashboard_tiles",
            json!({ "board": "Sales" }),
            "Let me look at the board.",
        ),
        says("The Sales board has one chart: Billed lately [1]."),
    ])
    .await;
    use_model(&h, &model).await;

    let answer = ask_in_room(&h, &room, "@insights what is on the sales board?").await;
    assert_eq!(
        answer["body"],
        "The Sales board has one chart: Billed lately [1]."
    );
    assert!(
        answer["proposal"].is_null(),
        "a read is answered, never proposed"
    );

    // The agent was offered its verbs — the four kept figure verbs and the
    // new board pair, reads and writes alike, from the intent registry.
    let prompt = offered(&seen, 0);
    for verb in [
        "insight_catalog",
        "insight_answer",
        "insight_change",
        "dashboard_tiles",
        "insight_report",
        "pin_chart",
    ] {
        assert!(
            prompt.contains(&format!("- {verb}:")),
            "the prompt does not offer {verb}"
        );
    }
    // The stored board came back as its own record: the caption, and the
    // question the tile asks in the catalog's words — never a figure, which
    // the tile does not carry.
    let sources = shown(&seen, 1);
    assert!(sources.contains("Billed lately"), "{sources}");
    assert!(sources.contains("billing.documents"), "{sources}");
    assert!(sources.contains("\"tileCount\":1"), "{sources}");
}

#[tokio::test]
async fn pinning_a_chart_waits_for_the_askers_tap() {
    let h = harness("insights-intents-pin").await;
    let board = a_board(&h, "Overview").await;

    let agent = insights_agent(&h).await;
    let room = a_room_with(&h, "ask", &agent).await;
    let (model, _seen) = scripted_model(vec![wants(
        "pin_chart",
        json!({ "board": "Overview", "title": "Billed lately", "spec": billed_lately() }),
        "I'll pin that to the Overview board.",
    )])
    .await;
    use_model(&h, &model).await;

    let answer = ask_in_room(&h, &room, "@insights pin billed lately to the overview").await;
    assert!(
        !answer["proposal"].is_null(),
        "a write is a proposal: {answer}"
    );
    assert_eq!(answer["proposal"]["tool"], "pin_chart");
    // Nothing ran without a tap: the board is still empty.
    assert!(
        tile_titles(&h, &board).await.is_empty(),
        "pinned before approval"
    );

    // The asker approves — and the tile is on the board, over the same route
    // the screen reads, with the caption the preview named.
    let proposal = answer["proposal"]["id"].as_str().unwrap().to_owned();
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/chat/proposals/{proposal}"),
        json!({ "approve": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(tile_titles(&h, &board).await, ["Billed lately"]);
}

#[tokio::test]
async fn another_tenants_boards_do_not_exist_here() {
    let h = harness("insights-intents-iso").await;
    // Another tenant on the same store, with a board whose caption is theirs.
    let other = harness_on(Arc::clone(&h.store), "insights-intents-iso2").await;
    let theirs = a_board(&other, "warroom figures").await;
    let (status, body) = post(
        &other.app,
        &other.token,
        &format!("/insights/dashboards/{theirs}/tiles"),
        json!({ "title": "the secret plan", "spec": billed_lately() }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let agent = insights_agent(&h).await;
    let room = a_room_with(&h, "ask", &agent).await;
    let (model, seen) = scripted_model(vec![
        wants(
            "dashboard_tiles",
            json!({ "board": "warroom figures" }),
            "Let me look at the board.",
        ),
        says("You have no board called warroom figures."),
    ])
    .await;
    use_model(&h, &model).await;

    let answer = ask_in_room(&h, &room, "@insights what is on the warroom figures board?").await;
    assert_eq!(answer["body"], "You have no board called warroom figures.");
    // The other tenant's board earns the words an invented name earns —
    // indistinguishable on purpose — and not one of their captions reaches
    // this tenant's model.
    let sources = shown(&seen, 1);
    assert!(
        sources.contains("no board of yours is called warroom figures"),
        "{sources}"
    );
    assert!(
        !sources.contains("the secret plan"),
        "another tenant's captions leaked into the sources: {sources}"
    );
}
