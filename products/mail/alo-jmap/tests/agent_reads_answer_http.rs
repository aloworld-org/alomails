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

use alo_store::{AgentProduct, ChatAgentId, ChatChannelId};
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

/// A public room with one agent in it, both made the way the product makes
/// them. `product` is what the agent is the agent **of** (ADR 0034, A1.2) — the
/// value that decides which tools it is offered and which the boundary refuses.
async fn a_room_with_an_agent(
    h: &Harness,
    name: &str,
    handle: &str,
    product: AgentProduct,
) -> (String, ChatAgentId) {
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
        .create_agent(handle, handle, Some("knows its own product"), product)
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
    let (channel, agent) = a_room_with_an_agent(&h, "stock", "chat", AgentProduct::Chat).await;

    let spoken = ask_in_room(&h, &channel, "@chat what did I just ask about?").await;

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
    let (channel, agent) = a_room_with_an_agent(&h, "ordering", "tasks", AgentProduct::Tasks).await;

    let spoken = ask_in_room(&h, &channel, "@tasks we need more X100").await;

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

// ---- the product boundary (A1.2) --------------------------------------------

/// **A lookup belonging to another product is refused, and the agent says so.**
///
/// The Inventory agent's model asks for `whats_on`, which is the Agenda agent's.
/// It takes the reading path — a lookup must never wear a button (ADR 0047) —
/// and the execution boundary refuses it there, handing the model back the
/// reason. The turn's second call therefore carries the refusal among its
/// sources, and what lands in the room is a sentence naming the agent that owns
/// the question, not a diary.
///
/// The refusal is audited as an attempt that did not succeed: an audit that
/// records only what worked hides exactly the rows worth reading.
#[tokio::test]
async fn a_lookup_from_another_product_is_refused_and_the_agent_says_who_owns_it() {
    let h = harness("agentscopeR").await;
    let (base, seen) = scripted_model(vec![
        wants(
            "whats_on",
            json!({ "from": "2026-08-15" }),
            "Let me check the diary.",
        ),
        says("That's the Agenda agent's — ask @agenda what's on."),
    ])
    .await;
    use_model(&h, &base).await;
    let (channel, agent) =
        a_room_with_an_agent(&h, "stock", "inventory", AgentProduct::Inventory).await;

    let spoken = ask_in_room(&h, &channel, "@inventory what have I got on today?").await;

    // What the room got: a sentence, not a button and not a diary.
    assert_eq!(
        spoken["body"],
        json!("That's the Agenda agent's — ask @agenda what's on.")
    );
    assert_eq!(spoken["proposal"], Value::Null);

    // The model was told, in its own second call, exactly why the lookup did
    // not run — which is what let it answer usefully instead of dying.
    let asked = seen.lock().unwrap().clone();
    assert_eq!(asked.len(), 2, "the refusal costs one further call");
    let second = asked[1].to_string();
    assert!(
        second.contains("this lookup did not run"),
        "the refusal must reach the model: {second}"
    );
    assert!(
        second.contains("not a tool the inventory agent has"),
        "and it must name the product: {second}"
    );
    // The Inventory agent's own prompt never offered it the diary in the first
    // place — the prompt and the boundary read one registry.
    let first = asked[0].to_string();
    assert!(!first.contains("- whats_on:"), "{first}");
    assert!(first.contains("- stock_answer:"), "{first}");

    // Audited as an attempt that failed, against this agent and this room.
    let runs = h.acc.agent_tool_runs(50).await.unwrap();
    assert_eq!(runs.len(), 1, "{runs:?}");
    assert_eq!(runs[0].tool, "whats_on");
    assert!(!runs[0].ok, "a refused run is recorded as refused");
    assert_eq!(
        runs[0].agent.as_ref().map(ChatAgentId::as_str),
        Some(agent.as_str())
    );
    assert_eq!(
        runs[0].channel.as_ref().map(ChatChannelId::as_str),
        Some(channel.as_str())
    );
    // Nothing was read, so nothing counts as read.
    let records = h.acc.agent_records().await.unwrap();
    assert_eq!(records.get(agent.as_str()).unwrap().reads, 0);
}

/// **A change belonging to another product is refused even when the asker
/// approves it** — the property A1.2 is actually judged on.
///
/// The Inventory agent's model asks for `create_task`, which is the Tasks
/// agent's. It is a write, so it arrives in the room as a proposal exactly as
/// any write does; the asker taps approve; and the boundary refuses it because
/// of *whose* tool it is, not because of who tapped. Approval widens who may
/// run a tool, never which product's tools an agent has.
#[tokio::test]
async fn approving_another_products_change_still_runs_nothing() {
    let h = harness("agentscopeW").await;
    let (base, _seen) = scripted_model(vec![wants(
        "create_task",
        json!({ "title": "Order more X100" }),
        "I'll add a task to order more.",
    )])
    .await;
    use_model(&h, &base).await;
    let (channel, agent) =
        a_room_with_an_agent(&h, "ordering", "inventory", AgentProduct::Inventory).await;

    let spoken = ask_in_room(&h, &channel, "@inventory we need more X100").await;
    assert_eq!(spoken["proposal"]["tool"], json!("create_task"));
    assert_eq!(spoken["proposal"]["state"], json!("pending"));
    let proposal = spoken["proposal"]["id"].as_str().unwrap().to_owned();

    // The asker's own tap. It is refused, and the refusal says why.
    let (status, problem) = post(
        &h.app,
        &h.token,
        &format!("/chat/proposals/{proposal}"),
        json!({ "approve": true }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{problem}");
    let detail = problem["detail"].as_str().unwrap_or_default();
    assert!(detail.contains("create_task"), "{problem}");
    assert!(detail.contains("inventory"), "{problem}");

    // Nothing was created. This is the sentence the whole item is for.
    let project = h.acc.ensure_personal_project().await.unwrap();
    assert!(
        h.acc.tasks_in_project(&project).await.unwrap().is_empty(),
        "an agent must not run another product's tool, approved or not"
    );

    // And the attempt is on the record as an attempt.
    let runs = h.acc.agent_tool_runs(50).await.unwrap();
    assert_eq!(runs.len(), 1, "{runs:?}");
    assert_eq!(runs[0].tool, "create_task");
    assert_eq!(runs[0].effect, "write");
    assert!(!runs[0].ok);
    assert_eq!(
        runs[0].agent.as_ref().map(ChatAgentId::as_str),
        Some(agent.as_str())
    );
}

// ---- product-scoped grounding (A1.3) ----------------------------------------

/// Files a Drive document and an email that both match the same word, so a
/// product's grounding can only be narrower than the workspace's — never a
/// different question.
async fn a_file_and_an_email_about_the_same_thing(h: &Harness) {
    use alo_store::{DriveLocation, NewDriveFile};
    h.acc
        .drive_create_file(
            &DriveLocation::Personal,
            None,
            &NewDriveFile {
                name: "pangolin report.docx".to_owned(),
                blob_id: "x".to_owned(),
                size: 1,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let inbox = h.acc.inbox().await.unwrap();
    h.acc
        .ingest(
            &inbox,
            b"From: sender@example.test\r\nSubject: the pangolin account\r\n\
              Message-ID: <ground-wire@alo.test>\r\n\r\nthey replied about the pangolin\r\n",
        )
        .await
        .unwrap();
}

/// The sentence A1.3 asks to be proved, on the wire: a Mail agent's grounding
/// carries the asker's own correspondence and **no Drive row**, out of a
/// workspace where a file matches the question just as well.
#[tokio::test]
async fn a_mail_agents_grounding_is_its_own_records_and_holds_no_drive_rows() {
    let h = harness("agentground").await;
    a_file_and_an_email_about_the_same_thing(&h).await;
    let (base, seen) = scripted_model(vec![says("They wrote about it last week [1].")]).await;
    use_model(&h, &base).await;
    let (channel, _) = a_room_with_an_agent(&h, "desk", "mail", AgentProduct::Mail).await;

    let spoken = ask_in_room(&h, &channel, "@mail what about the pangolin?").await;
    assert_eq!(spoken["body"], json!("They wrote about it last week [1]."));

    let asked = seen.lock().unwrap().clone();
    assert_eq!(asked.len(), 1, "one call, no tool run");
    let first = asked[0].to_string();
    assert!(
        first.contains("the pangolin account"),
        "the Mail agent grounds in its own correspondence: {first}"
    );
    assert!(
        !first.contains("pangolin report.docx"),
        "a Mail agent's grounding must contain no Drive rows: {first}"
    );
}

/// The other half of the same rule: an agent whose product reaches its records
/// through a reading tool is grounded in **nothing**, in the same workspace
/// where two records match — so it cannot answer a stock question from
/// somebody's email, and its prompt says where its records actually are.
#[tokio::test]
async fn an_inventory_agent_is_grounded_in_nothing_and_told_to_look_it_up() {
    let h = harness("agentnoground").await;
    a_file_and_an_email_about_the_same_thing(&h).await;
    let (base, seen) = scripted_model(vec![says("I'd have to look that up in stock.")]).await;
    use_model(&h, &base).await;
    let (channel, _) =
        a_room_with_an_agent(&h, "stockroom", "inventory", AgentProduct::Inventory).await;

    ask_in_room(&h, &channel, "@inventory what about the pangolin?").await;

    let asked = seen.lock().unwrap().clone();
    assert_eq!(asked.len(), 1);
    let first = asked[0].to_string();
    assert!(
        !first.contains("the pangolin account"),
        "the Inventory agent must not be handed the asker's email: {first}"
    );
    assert!(
        !first.contains("pangolin report.docx"),
        "nor their files: {first}"
    );
    // Its system prompt says so rather than leaving an empty list to be read as
    // "there is nothing".
    assert!(
        first.contains("Nothing in your product is searched for you"),
        "it must be told to use its reading tool: {first}"
    );

    // Ask alo, in the same workspace, still sees both — it is the one agent
    // that looks everywhere (ADR 0034).
    let (status, body) = post(
        &h.app,
        &h.token,
        "/ai/agent",
        json!({ "q": "what about the pangolin?" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let titles: Vec<String> = body["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|source| source["title"].as_str().unwrap_or_default().to_owned())
        .collect();
    assert!(
        titles.iter().any(|t| t == "pangolin report.docx"),
        "{titles:?}"
    );
    assert!(
        titles.iter().any(|t| t == "the pangolin account"),
        "{titles:?}"
    );
}
