//! Reads answer, writes propose — in a real room, on the wire (ADR 0047, A1.1).
//!
//! **No live model is ever called here.** The tenant's AI backend is a scripted
//! local socket handing back fixture completions in order, the shape
//! `tests/insights_ask_http.rs` already uses. That is what makes the two-turn
//! read loop testable at all: a first reply naming a *reading* tool, and a
//! second reply that must have the tool's own result among its sources.
//!
//! The unit tests in `agent_turn.rs` prove the decision — every read runs and
//! every write waits, over the whole registry. They cannot prove what happens to
//! the database, and that is the half ADR 0047 is actually judged on:
//!
//! - a read runs **inside** the turn, its result grounds the answer, and it
//!   leaves **no `chat_proposals` row** — nobody is asked to tap a lookup;
//! - a write **executes nothing** until the asker approves it, and then runs
//!   exactly once through their own door;
//! - both leave an audit row with the effect the registry declared, so a third
//!   of an agent's work is not invisible.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use alo_store::{ChatAgentId, ChatChannelId};
use common::{Harness, harness, send};

// ---- a scripted, local, offline "model" -------------------------------------

/// The request bodies the fake backend has been sent, in order.
type Seen = Arc<Mutex<Vec<Value>>>;

/// A minimal OpenAI-compatible chat-completions endpoint on localhost that
/// answers `script` in order (the last entry repeats), recording what it was
/// asked. It speaks just enough HTTP/1.1 for `reqwest`.
async fn scripted_model(script: Vec<String>) -> (String, Seen) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let seen: Seen = Arc::new(Mutex::new(Vec::new()));
    let record = Arc::clone(&seen);
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                return;
            };
            let record = Arc::clone(&record);
            let script = script.clone();
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let mut chunk = [0u8; 8192];
                let body = loop {
                    let Ok(n) = sock.read(&mut chunk).await else {
                        return;
                    };
                    if n == 0 {
                        return;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") else {
                        continue;
                    };
                    let head = String::from_utf8_lossy(&buf[..end]).into_owned();
                    let length: usize = head
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse().unwrap_or(0))
                        })
                        .unwrap_or(0);
                    if buf.len() >= end + 4 + length {
                        break buf[end + 4..end + 4 + length].to_vec();
                    }
                };
                let turn = {
                    let mut seen = record.lock().unwrap();
                    seen.push(serde_json::from_slice(&body).unwrap_or(Value::Null));
                    seen.len() - 1
                };
                let content = script
                    .get(turn)
                    .or_else(|| script.last())
                    .cloned()
                    .unwrap_or_default();
                let answer =
                    json!({ "choices": [{ "message": { "role": "assistant", "content": content } }] })
                        .to_string();
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    answer.len(),
                    answer
                );
                let _ = sock.write_all(response.as_bytes()).await;
                let _ = sock.flush().await;
            });
        }
    });
    (format!("http://{addr}"), seen)
}

/// The decision envelope for a tool the model wants used.
fn wants(tool: &str, args: Value, say: &str) -> String {
    json!({ "kind": "action", "say": say, "action": { "tool": tool, "args": args } }).to_string()
}

/// The decision envelope for a sentence.
fn says(answer: &str) -> String {
    json!({ "kind": "answer", "answer": answer }).to_string()
}

/// Points the tenant's default AI provider at `base_url`.
///
/// The provider id carries the tenant, because these suites share one Postgres
/// and a provider id is unique across it.
async fn use_model(h: &Harness, base_url: &str) {
    let id = format!("ai-{}", h.tenant.as_str());
    h.acc
        .upsert_ai_provider(
            &id,
            "openai",
            "scripted",
            base_url,
            "test-model",
            None,
            true,
        )
        .await
        .unwrap();
    h.acc.set_default_ai_provider(&id).await.unwrap();
}

// ---- request helpers ---------------------------------------------------------

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

/// A public room with one agent in it, both made the way the product makes them.
async fn a_room_with_an_agent(h: &Harness, name: &str, handle: &str) -> (String, ChatAgentId) {
    let (status, body) = post(
        &h.app,
        &h.token,
        "/chat/channels",
        json!({ "name": name, "visibility": "public" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let channel = body["id"].as_str().unwrap().to_owned();
    let agent = h
        .acc
        .create_agent(handle, "Inventory", Some("knows the stock"))
        .await
        .unwrap();
    h.acc
        .add_agent_to_channel(&ChatChannelId::new(channel.clone()), &agent)
        .await
        .unwrap();
    (channel, agent)
}

/// Says something in a room and waits for the agent's reply to arrive.
///
/// The turn is spawned off the request on purpose — the asker's words are stored
/// and delivered without waiting on inference — so the reply has to be waited
/// for. It returns the moment the agent has spoken; the deadline is only a
/// ceiling, and a blown one is a real failure rather than a slow machine.
async fn ask_in_room(h: &Harness, channel: &str, question: &str) -> Value {
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
        let (status, body) = get(
            &h.app,
            &h.token,
            &format!("/chat/channels/{channel}/messages"),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let spoken = body["messages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["authorKind"] == "agent")
            .cloned();
        if let Some(message) = spoken {
            return message;
        }
        assert!(
            Instant::now() < deadline,
            "the agent never spoke: {}",
            body["messages"]
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Every message in the room, so a test can assert about *all* of them.
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

// ---- the two properties ------------------------------------------------------

/// A lookup answers in the room. Nobody is offered a button, and the answer is
/// grounded in what the tool actually returned — the second call to the model
/// carries the room's own messages back to it as a numbered source.
#[tokio::test]
async fn a_read_answers_in_the_room_and_leaves_no_proposal() {
    let h = harness("agentread").await;
    let (base, seen) = scripted_model(vec![
        wants("catch_up_room", json!({ "room": "stock" }), "Let me look."),
        says("You asked about the X100 a moment ago [2]."),
    ])
    .await;
    use_model(&h, &base).await;
    let (channel, agent) = a_room_with_an_agent(&h, "stock", "inventory").await;

    let spoken = ask_in_room(&h, &channel, "@inventory what did I just ask about?").await;

    // The answer, in the room, with nothing to approve on it.
    assert_eq!(
        spoken["body"],
        json!("You asked about the X100 a moment ago [2].")
    );
    assert_eq!(spoken["proposal"], Value::Null);
    // …and not on any message in the room: a read files no proposal at all.
    for message in messages(&h, &channel).await {
        assert_eq!(
            message["proposal"],
            Value::Null,
            "a read must never create a proposal: {message}"
        );
    }

    // The model was asked twice, and the second time it was holding the tool's
    // own result. Without this the answer could be a guess off the search hits.
    let asked = seen.lock().unwrap().clone();
    assert_eq!(asked.len(), 2, "a read costs exactly one further call");
    let second = asked[1].to_string();
    assert!(
        second.contains("chatCatchUp"),
        "the tool result must ground the answer: {second}"
    );
    assert!(
        second.contains("what did I just ask about?"),
        "the room's own messages are what it read: {second}"
    );

    // Audited as a read, against the agent and the room, through the asker.
    let runs = h.acc.agent_tool_runs(50).await.unwrap();
    assert_eq!(runs.len(), 1, "{runs:?}");
    assert_eq!(runs[0].tool, "catch_up_room");
    assert_eq!(runs[0].effect, "read");
    assert!(runs[0].ok);
    assert_eq!(
        runs[0].agent.as_ref().map(ChatAgentId::as_str),
        Some(agent.as_str())
    );
    assert_eq!(
        runs[0].channel.as_ref().map(ChatChannelId::as_str),
        Some(channel.as_str())
    );
    // The read shows in the agent's record beside its answers.
    let records = h.acc.agent_records().await.unwrap();
    assert_eq!(records.get(agent.as_str()).unwrap().reads, 1);
}

/// A change waits for a tap. Nothing is created while it waits, nothing is
/// audited while it waits, and the tap runs it exactly once.
#[tokio::test]
async fn a_write_executes_nothing_until_the_asker_approves_it() {
    let h = harness("agentwrite").await;
    let (base, _seen) = scripted_model(vec![wants(
        "create_task",
        json!({ "title": "Order more X100" }),
        "I'll add a task to order more.",
    )])
    .await;
    use_model(&h, &base).await;
    let (channel, agent) = a_room_with_an_agent(&h, "ordering", "inventory").await;

    let spoken = ask_in_room(&h, &channel, "@inventory we need more X100").await;

    // What the room sees: the sentence, with the change hanging off it, pending.
    assert_eq!(spoken["body"], json!("I'll add a task to order more."));
    assert_eq!(spoken["proposal"]["tool"], json!("create_task"));
    assert_eq!(spoken["proposal"]["state"], json!("pending"));
    assert_eq!(spoken["proposal"]["askedBy"], json!(h.user.as_str()));
    let proposal = spoken["proposal"]["id"].as_str().unwrap().to_owned();

    // Nothing happened. Not the task…
    let project = h.acc.ensure_personal_project().await.unwrap();
    assert!(
        h.acc.tasks_in_project(&project).await.unwrap().is_empty(),
        "a write must not run before it is approved"
    );
    // …and not an audit row either: a refused-because-unapproved write never
    // reached the boundary, because the turn never offered it one.
    assert!(
        h.acc.agent_tool_runs(50).await.unwrap().is_empty(),
        "nothing ran, so nothing is logged as having run"
    );

    // The tap. It is the asker's own, and it is what makes the change happen.
    let (status, decided) = post(
        &h.app,
        &h.token,
        &format!("/chat/proposals/{proposal}"),
        json!({ "approve": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{decided}");
    assert_eq!(decided["state"], json!("approved"));

    let tasks = h.acc.tasks_in_project(&project).await.unwrap();
    assert_eq!(tasks.len(), 1, "approved exactly once: {tasks:?}");
    assert_eq!(tasks[0].title, "Order more X100");

    // Audited as a write, against the same agent and room as a read would be.
    let runs = h.acc.agent_tool_runs(50).await.unwrap();
    assert_eq!(runs.len(), 1, "{runs:?}");
    assert_eq!(runs[0].tool, "create_task");
    assert_eq!(runs[0].effect, "write");
    assert!(runs[0].ok);
    assert_eq!(
        runs[0].agent.as_ref().map(ChatAgentId::as_str),
        Some(agent.as_str())
    );
    assert_eq!(
        h.acc.agent_records().await.unwrap()[agent.as_str()].reads,
        0
    );
}
