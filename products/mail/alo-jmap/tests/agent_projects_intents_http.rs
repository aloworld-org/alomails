//! The Projects agent over its intents (AA.3, ADR 0058), on the wire: in a
//! real room, against the real router and store, with a scripted model.
//!
//! Before the move to intents the Projects agent could summarise one project
//! and suggest hours, but "@projects which projects are active?" had no verb
//! to run. This suite holds the opposite: the read runs inside the turn and
//! answers from the portfolio — the boards as `/projects` serves them, a
//! finished one filtered out — a write (suggesting a timesheet entry) is
//! proposed, previewed and not run, and another tenant's boards are
//! unreachable from the overview.

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

async fn patch(app: &Router, token: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("PATCH")
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

async fn projects_agent(h: &Harness) -> String {
    let handle = alo_store::default_handle(AgentProduct::Projects);
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

/// A team project, by name, returning its id.
async fn a_project(h: &Harness, name: &str) -> String {
    let (status, body) = post(&h.app, &h.token, "/projects", json!({ "name": name })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["id"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn which_projects_are_active_is_answered_from_the_portfolio() {
    let h = harness("proj-intents-active").await;
    a_project(&h, "Website relaunch").await;
    let done = a_project(&h, "Archive migration").await;
    // The finished engagement stays on the books with its status, and the
    // default read leaves it out — "active" means still in play.
    let (status, body) = patch(
        &h.app,
        &h.token,
        &format!("/projects/{done}"),
        json!({ "name": "Archive migration", "status": "completed" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let agent = projects_agent(&h).await;
    let room = a_room_with(&h, "delivery", &agent).await;
    let (model, seen) = scripted_model(vec![
        wants("active_projects", json!({}), "Let me read the portfolio."),
        says("Two boards, one active: Website relaunch [1]."),
    ])
    .await;
    use_model(&h, &model).await;

    let answer = ask_in_room(&h, &room, "@projects which projects are active?").await;
    assert_eq!(
        answer["body"],
        "Two boards, one active: Website relaunch [1]."
    );
    assert!(
        answer["proposal"].is_null(),
        "a read is answered, never proposed"
    );

    // The agent was offered its verbs — reads and writes alike, from the
    // intent registry.
    let prompt = offered(&seen, 0);
    for verb in [
        "active_projects",
        "project_status_summary",
        "who_is_on_what",
        "time_this_week",
        "log_time",
        "draft_timesheet_from_calendar",
    ] {
        assert!(
            prompt.contains(&format!("- {verb}:")),
            "the prompt does not offer {verb}"
        );
    }
    // The read's rows are the portfolio's own: the running board with its
    // status and open work, the finished one left out of the default scope.
    let sources = shown(&seen, 1);
    assert!(sources.contains("Website relaunch"), "{sources}");
    assert!(sources.contains("\"status\":\"active\""), "{sources}");
    assert!(sources.contains("\"openTasks\""), "{sources}");
    assert!(!sources.contains("Archive migration"), "{sources}");
}

#[tokio::test]
async fn suggesting_an_hour_is_proposed_and_not_run() {
    let h = harness("proj-intents-log").await;
    a_project(&h, "Website relaunch").await;
    let agent = projects_agent(&h).await;
    let room = a_room_with(&h, "delivery", &agent).await;

    let (model, _seen) = scripted_model(vec![wants(
        "log_time",
        json!({ "project": "Website relaunch", "date": "2026-08-27", "minutes": 90 }),
        "I'll suggest the hour and a half.",
    )])
    .await;
    use_model(&h, &model).await;

    let answer = ask_in_room(
        &h,
        &room,
        "@projects log 90 minutes on Website relaunch for the 27th",
    )
    .await;
    assert!(
        !answer["proposal"].is_null(),
        "a write is a proposal: {answer}"
    );
    assert_eq!(answer["proposal"]["tool"], "log_time");
    // Nothing ran without a tap: no suggestion is waiting in the timesheet,
    // and the week holds no entry.
    let (status, body) = get(&h.app, &h.token, "/projects/time/proposals").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["entries"].as_array().unwrap().len(), 0, "{body}");
    let (status, body) = get(
        &h.app,
        &h.token,
        "/projects/time?from=2026-08-24&to=2026-08-30",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["entries"].as_array().unwrap().len(), 0, "{body}");
}

#[tokio::test]
async fn another_tenants_boards_are_unreachable() {
    let h = harness("proj-intents-iso-a").await;
    let other = harness_on(h.store.clone(), "proj-intents-iso-b").await;
    // Tenant B's engagement names a client codename, which is exactly the
    // kind of word that must never cross a tenant wall.
    a_project(&other, "Project Nightingale").await;
    let agent = projects_agent(&h).await;
    let room = a_room_with(&h, "delivery", &agent).await;

    let (model, seen) = scripted_model(vec![
        wants("active_projects", json!({}), "Let me read the portfolio."),
        says("Only your personal board is on the books."),
    ])
    .await;
    use_model(&h, &model).await;

    let answer = ask_in_room(&h, &room, "@projects which projects are active?").await;
    assert_eq!(answer["body"], "Only your personal board is on the books.");
    // What the model was shown is tenant A's own portfolio and none of tenant
    // B's record — not the board, not its name.
    let sources = shown(&seen, 1);
    assert!(!sources.contains("Project Nightingale"), "{sources}");
    assert!(!sources.contains("proj-intents-iso-b"), "{sources}");
}
