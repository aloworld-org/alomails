//! The Tasks agent over its intents (ADR 0058, queue item AB.4), on the wire:
//! in a real room, against the real router and store, with a scripted model.
//!
//! What AB.4 adds is one board's open work (`board_tasks`), one task in full
//! (`task_lookup`), and the two writes a to-do agent must never run on a
//! hunch — `complete_task` and `reassign_task`, both previewed and waiting
//! for a tap. This suite holds the wave's three sentences: a read runs inside
//! the turn and the board's own record view reaches the model as a source; a
//! task of another tenant's, or on a colleague's private board, is not among
//! the things that can be named; a write is proposed and not run. The deep
//! behaviour of the six kept tools — the plate's buckets, the chase's bounds,
//! the double proposal of a capture — stays proven by `agent_tasks_http.rs`,
//! which runs the same executors.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use alo_store::{AccountStore, AgentProduct, NewTask, ProjectId, TaskId};

use crate::common;
use crate::common::model::{Seen, says, scripted_model, use_model, wants};
use crate::common::{Harness, harness, send};

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

async fn the_tasks_agent(h: &Harness) -> String {
    let handle = alo_store::default_handle(AgentProduct::Tasks);
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

/// Says something in the room and waits for the agent's reply.
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
        assert!(Instant::now() < deadline, "the agent never spoke");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// One approved tool run over the ordinary approval route — the same path the
/// command palette's button takes. The tests that use it are about **arguments
/// and refusals**, which a chat turn cannot vary as finely as they need.
async fn run(h: &Harness, tool: &str, args: Value) -> (StatusCode, Value) {
    post(
        &h.app,
        &h.token,
        "/ai/agent/execute",
        json!({ "tool": tool, "args": args }),
    )
    .await
}

/// The `detail` of a refusal, which is the sentence the client shows.
fn why(body: &Value) -> String {
    body["detail"].as_str().unwrap_or_default().to_owned()
}

/// What the model was shown on call `n` — the user turn, where the grounding
/// and the tool results live.
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

// ---- seeding, the way the board's own routes store it ------------------------

/// A task on a board, titled and nothing more — the shape the board renders.
async fn a_task(acc: &AccountStore, board: &ProjectId, title: &str) -> TaskId {
    acc.create_task(
        board,
        &NewTask {
            title: title.to_owned(),
            description: None,
            status: None,
            assignee: None,
            due_at: None,
            priority: None,
            state: None,
            source_kind: None,
            source_id: None,
        },
    )
    .await
    .unwrap()
}

/// A team board with two open tasks and one already done — enough for "open"
/// to mean something.
async fn the_launch_board(acc: &AccountStore) -> ProjectId {
    let board = acc.create_task_project("Launch", None).await.unwrap();
    a_task(acc, &board, "Book the venue").await;
    a_task(acc, &board, "Send the invitations").await;
    let done = a_task(acc, &board, "Pick the date").await;
    acc.move_task(&done, "done", 1.0).await.unwrap();
    board
}

// ---- the read: answered from the record, inside the tenant -------------------

/// **AB.4's headline sentence**: "@tasks what is open on the Launch board?"
/// is answered from the record — the board's open tasks reach the model as a
/// source, the finished one does not, and neither does another tenant's board
/// or a colleague's private list.
#[tokio::test]
async fn a_boards_open_tasks_are_answered_from_the_record() {
    let h = harness("tasks-intents-board").await;
    the_launch_board(&h.acc).await;
    // A stranger's board and a colleague's private list, so the assertions
    // below are about reach and not about emptiness.
    let other = common::harness_on(h.store.clone(), "tasks-intents-stranger").await;
    let theirs = other
        .acc
        .create_task_project("Their launch", None)
        .await
        .unwrap();
    a_task(&other.acc, &theirs, "Their secret rollout").await;
    let colleague =
        h.ts.create_user("ben@tasks-intents-board.test")
            .await
            .unwrap();
    let bens = h.store.for_account(h.tenant.clone(), colleague);
    let private = bens.ensure_personal_project().await.unwrap();
    a_task(&bens, &private, "Bens private errand").await;

    let agent = the_tasks_agent(&h).await;
    let room = a_room_with(&h, "launch", &agent).await;
    let (model, seen) = scripted_model(vec![
        wants(
            "board_tasks",
            json!({ "board": "Launch" }),
            "Let me look at the board.",
        ),
        says("Two tasks are open: the venue and the invitations [1]."),
    ])
    .await;
    use_model(&h, &model).await;

    let answer = ask_in_room(&h, &room, "@tasks what is open on the Launch board?").await;
    assert_eq!(
        answer["body"],
        "Two tasks are open: the venue and the invitations [1]."
    );
    assert!(
        answer["proposal"].is_null(),
        "a read is answered, never proposed"
    );

    // The agent was offered its verbs — reads and writes alike, rendered from
    // the intent registry — and no other product's.
    let prompt = offered(&seen, 0);
    for verb in [
        "my_plate",
        "overdue_by_owner",
        "thread_actions",
        "board_tasks",
        "task_lookup",
        "create_task",
        "set_task_priority",
        "chase_task",
        "capture_actions",
        "complete_task",
        "reassign_task",
    ] {
        assert!(
            prompt.contains(&format!("- {verb}:")),
            "the prompt does not offer {verb}"
        );
    }
    assert!(
        !prompt.contains("- file_read:") && !prompt.contains("- open_quotes:"),
        "another product's tools reached the Tasks agent"
    );
    // The record view came back as a source: the board's open tasks, nobody
    // else's and not the finished one.
    let sources = shown(&seen, 1);
    assert!(sources.contains("boardTasks"), "{sources}");
    assert!(sources.contains("Book the venue"), "{sources}");
    assert!(sources.contains("Send the invitations"), "{sources}");
    assert!(
        !sources.contains("Pick the date"),
        "a finished task is not open: {sources}"
    );
    assert!(
        !sources.contains("Their secret rollout"),
        "another tenant's board reached the model: {sources}"
    );
    assert!(
        !sources.contains("Bens private errand"),
        "a colleague's private list reached the model: {sources}"
    );
}

// ---- the write: proposed, previewed, not run ---------------------------------

#[tokio::test]
async fn completing_a_task_is_proposed_and_not_run() {
    let h = harness("tasks-intents-write").await;
    let board = h.acc.create_task_project("Launch", None).await.unwrap();
    let task = a_task(&h.acc, &board, "Book the venue").await;
    let agent = the_tasks_agent(&h).await;
    let room = a_room_with(&h, "launch", &agent).await;
    let (model, _seen) = scripted_model(vec![wants(
        "complete_task",
        json!({ "task": "Book the venue" }),
        "I'll mark the venue booking done.",
    )])
    .await;
    use_model(&h, &model).await;

    let answer = ask_in_room(&h, &room, "@tasks the venue is booked — mark it done").await;
    assert!(
        !answer["proposal"].is_null(),
        "a write is a proposal: {answer}"
    );
    assert_eq!(answer["proposal"]["tool"], "complete_task");
    // Nothing ran without a tap: the task still sits in its column, unfinished.
    let stored = h.acc.task(&task).await.unwrap().unwrap();
    assert_ne!(
        stored.status, "done",
        "the task was completed before the tap"
    );
    assert!(stored.completed_at.is_none());
}

// ---- arguments and refusals, over the approval route -------------------------

/// The four new verbs against the real store: the board read is exact and
/// refuses an unknown board with the boards that exist; the lookup opens one
/// task in full and treats two matches as a question; completing moves the
/// task through the board's own move; a handover changes the owner and
/// nothing else, and a name that matches nobody on the caller's boards is
/// refused without consulting a directory.
#[tokio::test]
async fn the_new_verbs_run_against_the_real_board() {
    let h = harness("tasks-intents-verbs").await;
    let board = the_launch_board(&h.acc).await;
    let colleague =
        h.ts.create_user("ben@tasks-intents-verbs.test")
            .await
            .unwrap();

    // The board read: open tasks only, by the board's name.
    let (status, body) = run(&h, "board_tasks", json!({ "board": "Launch" })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["kind"], "boardTasks");
    assert_eq!(body["result"]["open"], 2, "{body}");
    let titles: Vec<String> = body["result"]["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|task| task["title"].as_str().unwrap().to_owned())
        .collect();
    assert!(titles.contains(&"Book the venue".to_owned()), "{titles:?}");
    assert!(
        !titles.contains(&"Pick the date".to_owned()),
        "a finished task is not open: {titles:?}"
    );
    // A board nobody has is refused by name — the same sentence whether it
    // does not exist or belongs to somebody else, so asking leaks nothing.
    let (status, body) = run(&h, "board_tasks", json!({ "board": "Their launch" })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(why(&body).contains("no board of yours"), "{body}");

    // The lookup: one task in full — the record the task's own route serves —
    // finished work included, and two matches are a question.
    let (status, body) = run(&h, "task_lookup", json!({ "task": "Pick the date" })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["kind"], "taskLookup");
    assert_eq!(body["result"]["board"], "Launch");
    assert_eq!(body["result"]["record"]["task"]["title"], "Pick the date");
    assert_eq!(body["result"]["record"]["task"]["status"], "done");
    assert!(body["result"]["record"]["comments"].is_array(), "{body}");
    let (status, body) = run(&h, "task_lookup", json!({ "task": "the" })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(why(&body).contains("more than one task"), "{body}");

    // Completing: the board's own move, completed_at set by the store.
    let (status, body) = run(
        &h,
        "complete_task",
        json!({ "task": "Send the invitations" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["kind"], "taskCompleted");
    assert_eq!(body["result"]["board"], "Launch");
    let done: Vec<_> = h
        .acc
        .tasks_in_project(&board)
        .await
        .unwrap()
        .into_iter()
        .filter(|task| task.title == "Send the invitations")
        .collect();
    assert_eq!(done[0].status, "done");
    assert!(done[0].completed_at.is_some(), "completed_at was not set");
    // …and a task already finished is not among the unfinished to complete.
    let (status, body) = run(&h, "complete_task", json!({ "task": "Pick the date" })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(why(&body).contains("no unfinished task"), "{body}");

    // The handover: an exact address settles it; everything else about the
    // task is carried across unchanged.
    let (status, body) = run(
        &h,
        "reassign_task",
        json!({ "task": "Book the venue", "to": "ben@tasks-intents-verbs.test" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["kind"], "taskReassigned");
    assert_eq!(body["result"]["now"], "ben@tasks-intents-verbs.test");
    let venue: Vec<_> = h
        .acc
        .tasks_in_project(&board)
        .await
        .unwrap()
        .into_iter()
        .filter(|task| task.title == "Book the venue")
        .collect();
    assert_eq!(venue[0].assignee.as_deref(), Some(colleague.as_str()));
    assert_eq!(venue[0].priority, "none", "a handover rewrote the priority");
    // A first name now resolves against the people on the caller's boards —
    // Ben is one since the handover — and a stranger's name is refused the
    // same way whether they exist in the tenant or not.
    let (status, body) = run(
        &h,
        "reassign_task",
        json!({ "task": "Book the venue", "to": "ben" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["now"], "ben@tasks-intents-verbs.test");
    let (status, body) = run(
        &h,
        "reassign_task",
        json!({ "task": "Book the venue", "to": "Zelda" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(why(&body).contains("no colleague on your boards"), "{body}");
}
