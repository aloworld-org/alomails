//! **The Tasks agent past `create_task`, end to end** (A2.7) — the four
//! sentences the queue item leaves a to-do agent to prove, each asked the way a
//! person asks it:
//!
//! - `@tasks what have I got on?` — **answered in the room with no button in
//!   between**, out of the list rather than out of a search, with the tasks
//!   nobody dated among them and a colleague's work left out;
//! - `@tasks who is late?` — grouped by the person it is assigned to, over the
//!   boards the asker can already open and no others;
//! - `@tasks chase Ben about the pricing sheet` — a **proposal**, and nothing is
//!   written until the asker taps it; then a comment under their own name, on a
//!   task that really is late;
//! - `@tasks write down what we agreed in #launch` — a proposal that writes
//!   **proposed** tasks (ADR 0023), so each one is still accepted or rejected in
//!   the task list, and asking the room again says they have already been
//!   captured.
//!
//! And the isolation sentence the wave holds every agent to: a task of another
//! tenant's cannot be named, and neither can one on a colleague's private board
//! — the refusal is the one an unknown title gets, so nobody learns what is on
//! somebody else's list by asking about it.
//!
//! **No live model is ever called** (the loop's standing rail): the tenant's AI
//! backend is the scripted local socket in `common::model`, and the assertions
//! are about the bytes the model was *shown* and the rows the store holds
//! afterwards.
//!
//! Run the transcript with
//! `cargo nextest run -p alo-jmap --test agent_tasks_http --no-capture`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use std::time::{Duration as Wait, Instant};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use time::{Date, Duration, OffsetDateTime, Time};

use crate::common::model::{Seen, says, scripted_model, use_model, wants};
use crate::common::{Harness, harness, send};
use alo_store::{AccountStore, AgentProduct, NewTask, ProjectId, TaskId, UserId};

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

/// The id of the tenant's own Tasks agent, out of the set a first look at
/// `GET /chat/agents` seeds (A1.5).
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

/// A room, with that agent in it — both over HTTP, as a person makes them.
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

/// Says something in a room and waits for the agent's reply.
async fn ask_in_room(h: &Harness, channel: &str, question: &str) -> Value {
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
        tokio::time::sleep(Wait::from_millis(50)).await;
    }
}

/// Somebody says something in a room, so a conversation has something in it to
/// read actions out of.
async fn said(h: &Harness, channel: &str, body: &str) {
    let (status, out) = post(
        &h.app,
        &h.token,
        &format!("/chat/channels/{channel}/messages"),
        json!({ "body": body }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{out}");
}

/// The asker's own tap on a proposal — the only thing that makes a change happen.
async fn approve(h: &Harness, proposal: &str) -> Value {
    let (status, decided) = post(
        &h.app,
        &h.token,
        &format!("/chat/proposals/{proposal}"),
        json!({ "approve": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{decided}");
    decided
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

fn transcript(title: &str, lines: &[String]) {
    println!("\n===== A2.7 TRANSCRIPT: {title} =====");
    for line in lines {
        println!("{line}");
    }
    println!("===== end: {title} =====\n");
}

// ---- the boards under test ------------------------------------------------------

fn today() -> Date {
    OffsetDateTime::now_utc().date()
}

/// A day, as a due date is stored: midnight UTC, exactly as `create_task` and
/// `capture_actions` write one.
fn due(days_from_today: i64) -> OffsetDateTime {
    (today() + Duration::days(days_from_today))
        .with_time(Time::MIDNIGHT)
        .assume_utc()
}

/// A task on a board, with everything a plate reports about it.
async fn a_task(
    acc: &AccountStore,
    project: &ProjectId,
    title: &str,
    when: Option<OffsetDateTime>,
    assignee: Option<&UserId>,
) -> TaskId {
    acc.create_task(
        project,
        &NewTask {
            title: title.to_owned(),
            description: Some(format!("Notes for {title}")),
            status: None,
            assignee: assignee.map(|user| user.as_str().to_owned()),
            due_at: when,
            priority: Some("medium".to_owned()),
            state: None,
            source_kind: None,
            source_id: None,
        },
    )
    .await
    .unwrap()
}

/// A colleague in the same tenant, with a board of their own.
async fn a_colleague(h: &Harness, address: &str) -> (UserId, AccountStore) {
    let user = h.ts.create_user(address).await.unwrap();
    let acc = h.store.for_account(h.tenant.clone(), user.clone());
    (user, acc)
}

/// Every title in one bucket of a plate.
fn titles(bucket: &Value) -> Vec<String> {
    bucket
        .as_array()
        .unwrap()
        .iter()
        .map(|task| task["title"].as_str().unwrap().to_owned())
        .collect()
}

// ---- what is on my plate, on the wire -------------------------------------------

/// **The item's first sentence, end to end.** The list is read, the buckets are
/// the ones a day is read in, and there is no button in between — asking what
/// you have to do changes nothing.
#[tokio::test]
async fn the_tasks_agent_answers_what_is_on_my_plate_with_no_button_in_between() {
    let h = harness("agent-a27-plate").await;
    let mine = h.acc.ensure_personal_project().await.unwrap();
    a_task(&h.acc, &mine, "File the VAT return", Some(due(-3)), None).await;
    a_task(&h.acc, &mine, "Call the printer", Some(due(0)), None).await;
    a_task(&h.acc, &mine, "Draft the offsite plan", Some(due(4)), None).await;
    a_task(&h.acc, &mine, "Tidy the shared drive", None, None).await;
    a_task(&h.acc, &mine, "Book the venue", Some(due(60)), None).await;
    // Finished work is not on a plate.
    let done = a_task(&h.acc, &mine, "Renew the domain", Some(due(-9)), None).await;
    h.acc.move_task(&done, "done", 1.0).await.unwrap();
    // …and neither is a colleague's, even on a board we share.
    let (ben, _bens) = a_colleague(&h, "ben@a27-plate.test").await;
    let team = h.acc.create_task_project("Sales", None).await.unwrap();
    a_task(
        &h.acc,
        &team,
        "Bens pricing sheet",
        Some(due(-2)),
        Some(&ben),
    )
    .await;

    const ANSWER: &str = "The VAT return is three days late, the printer is due today, \
and nobody has dated tidying the shared drive [1].";
    let (base, seen) = scripted_model(vec![
        wants("my_plate", json!({}), "Let me look at your list."),
        says(ANSWER),
    ])
    .await;
    use_model(&h, &base).await;
    let agent = the_tasks_agent(&h).await;
    let channel = a_room_with(&h, "the day", &agent).await;

    const QUESTION: &str = "@tasks what have I got on?";
    let spoken = ask_in_room(&h, &channel, QUESTION).await;

    assert_eq!(spoken["body"], json!(ANSWER));
    assert_eq!(
        spoken["proposal"],
        Value::Null,
        "asking what you have to do must not put a button in the room"
    );

    // What the model was shown: the buckets, out of the list itself.
    let second = shown(&seen, 1);
    assert!(second.contains("myPlate"), "{second}");
    assert!(second.contains("File the VAT return"), "{second}");
    assert!(second.contains("\"daysLate\":3"), "{second}");
    assert!(second.contains("Tidy the shared drive"), "{second}");
    // …and the buckets themselves, asked directly so the shape is pinned.
    let (status, body) = run(&h, "my_plate", json!({})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let plate = &body["result"];
    assert_eq!(plate["kind"], json!("myPlate"));
    assert_eq!(plate["today"], json!(today().to_string()));
    assert_eq!(titles(&plate["overdue"]), ["File the VAT return"]);
    assert_eq!(titles(&plate["dueToday"]), ["Call the printer"]);
    assert_eq!(titles(&plate["comingUp"]), ["Draft the offsite plan"]);
    assert_eq!(titles(&plate["later"]), ["Book the venue"]);
    assert_eq!(titles(&plate["noDate"]), ["Tidy the shared drive"]);
    assert_eq!(plate["truncated"], json!(false));
    // The horizon moves the boundary between "coming up" and "later", and
    // nothing else about the answer.
    let (status, body) = run(&h, "my_plate", json!({ "days": 90 })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        titles(&body["result"]["comingUp"]),
        ["Draft the offsite plan", "Book the venue"]
    );
    assert!(titles(&body["result"]["later"]).is_empty());

    transcript(
        QUESTION,
        &[
            format!("POST /chat/channels/{channel}/messages"),
            format!("     {}", json!({ "body": QUESTION })),
            "--- what the model was shown on the second call ---".to_owned(),
            second,
            "--- the agent's message ---".to_owned(),
            spoken.to_string(),
        ],
    );
}

/// The plate's own trap: a task the agent itself created has **no assignee**
/// (`create_task` sets none), so a plate that filtered on assignee would hide
/// exactly the tasks this agent makes.
#[tokio::test]
async fn a_plate_holds_the_tasks_the_agent_made_and_the_ones_it_was_given() {
    let h = harness("agent-a27-mine").await;
    let (status, body) = run(
        &h,
        "create_task",
        json!({ "title": "Ring the accountant", "due": due(1).date().to_string() }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // …and one somebody else put on their own private board and assigned to us:
    // ours to do, and its board is not ours to name.
    let (_, theirs) = a_colleague(&h, "marta@a27-mine.test").await;
    let hers = theirs.ensure_personal_project().await.unwrap();
    a_task(
        &theirs,
        &hers,
        "Review the Q3 numbers",
        Some(due(2)),
        Some(&h.user),
    )
    .await;

    let (status, body) = run(&h, "my_plate", json!({})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let coming = body["result"]["comingUp"].as_array().unwrap();
    let names = titles(&body["result"]["comingUp"]);
    assert!(names.contains(&"Ring the accountant".to_owned()), "{body}");
    assert!(
        names.contains(&"Review the Q3 numbers".to_owned()),
        "{body}"
    );
    let borrowed = coming
        .iter()
        .find(|task| task["title"] == json!("Review the Q3 numbers"))
        .unwrap();
    assert_eq!(
        borrowed["board"],
        Value::Null,
        "a board the asker cannot open is not named: {body}"
    );
}

// ---- who is late ----------------------------------------------------------------

/// **The item's second sentence.** Late work is grouped by the person it is
/// assigned to, over the boards the asker can open — and a colleague's private
/// board is not one of them.
#[tokio::test]
async fn overdue_work_is_grouped_by_owner_over_the_boards_the_asker_can_open() {
    let h = harness("agent-a27-late").await;
    let (ben, _) = a_colleague(&h, "ben@a27-late.test").await;
    let (marta, martas) = a_colleague(&h, "marta@a27-late.test").await;
    let team = h.acc.create_task_project("Sales", None).await.unwrap();
    a_task(&h.acc, &team, "Pricing sheet", Some(due(-5)), Some(&ben)).await;
    a_task(&h.acc, &team, "Renewal quote", Some(due(-1)), Some(&ben)).await;
    a_task(&h.acc, &team, "Case study", Some(due(-2)), Some(&marta)).await;
    // On time, so it is not a chase.
    a_task(&h.acc, &team, "Q4 forecast", Some(due(3)), Some(&marta)).await;
    // Nobody's yet — reported, and never given a name it does not have.
    a_task(&h.acc, &team, "Website copy", Some(due(-4)), None).await;
    // Marta's own board is hers: late work on it is not the asker's to see.
    let hers = martas.ensure_personal_project().await.unwrap();
    a_task(
        &martas,
        &hers,
        "Her own therapy homework",
        Some(due(-8)),
        None,
    )
    .await;

    const ANSWER: &str = "Ben has two late items and Marta one; the website copy has no owner [1].";
    let (base, seen) = scripted_model(vec![
        wants("overdue_by_owner", json!({}), "Let me see who is late."),
        says(ANSWER),
    ])
    .await;
    use_model(&h, &base).await;
    let agent = the_tasks_agent(&h).await;
    let channel = a_room_with(&h, "the chase", &agent).await;

    const QUESTION: &str = "@tasks who is late?";
    let spoken = ask_in_room(&h, &channel, QUESTION).await;
    assert_eq!(spoken["body"], json!(ANSWER));
    assert_eq!(
        spoken["proposal"],
        Value::Null,
        "asking who is late changes nothing"
    );

    let second = shown(&seen, 1);
    assert!(second.contains("overdueByOwner"), "{second}");
    assert!(second.contains("ben@a27-late.test"), "{second}");
    assert!(
        !second.contains("Her own therapy homework"),
        "a private board is not readable by asking who is late: {second}"
    );

    let (status, body) = run(&h, "overdue_by_owner", json!({})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let people = body["result"]["people"].as_array().unwrap();
    assert_eq!(people.len(), 3, "{body}");
    let of = |address: Value| {
        people
            .iter()
            .find(|person| person["who"] == address)
            .unwrap_or_else(|| panic!("nobody is {address}: {body}"))
            .clone()
    };
    let bens = of(json!("ben@a27-late.test"));
    assert_eq!(titles(&bens["tasks"]), ["Pricing sheet", "Renewal quote"]);
    assert_eq!(bens["tasks"][0]["daysLate"], json!(5));
    assert_eq!(bens["tasks"][0]["board"], json!("Sales"));
    assert_eq!(titles(&of(Value::Null)["tasks"]), ["Website copy"]);
    // On time is not late, so Marta's forecast is not in her group.
    assert_eq!(
        titles(&of(json!("marta@a27-late.test"))["tasks"]),
        ["Case study"]
    );

    // Narrowed to one person, and to one board, by the names the asker used.
    let (status, body) = run(&h, "overdue_by_owner", json!({ "person": "ben" })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["people"].as_array().unwrap().len(), 1);
    let (status, body) = run(
        &h,
        "overdue_by_owner",
        json!({ "person": "ben", "project": "Sales" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(titles(&body["result"]["people"][0]["tasks"]).len(), 2);

    // A name that resolves to nobody is a refusal that says so — and says
    // nothing about whether that person exists.
    let (status, body) = run(&h, "overdue_by_owner", json!({ "person": "hovercraft" })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(why(&body).contains("no colleague with late work"), "{body}");
    let (status, body) = run(&h, "overdue_by_owner", json!({ "project": "Nowhere" })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(why(&body).contains("no board of yours"), "{body}");

    transcript(
        QUESTION,
        &[
            format!("POST /chat/channels/{channel}/messages"),
            format!("     {}", json!({ "body": QUESTION })),
            "--- what the model was shown on the second call ---".to_owned(),
            second,
            "--- the agent's message ---".to_owned(),
            spoken.to_string(),
        ],
    );
}

// ---- chasing --------------------------------------------------------------------

/// **The item's third sentence.** A chase is proposed, and only the asker's own
/// tap leaves the comment — under their name, on the task, and changing nothing
/// else about it.
#[tokio::test]
async fn a_chase_is_proposed_then_left_as_a_comment_in_the_askers_own_name() {
    let h = harness("agent-a27-chase").await;
    let (ben, _) = a_colleague(&h, "ben@a27-chase.test").await;
    let team = h.acc.create_task_project("Sales", None).await.unwrap();
    let sheet = a_task(&h.acc, &team, "Pricing sheet", Some(due(-5)), Some(&ben)).await;

    const SAY: &str = "I'll ask Ben where the pricing sheet has got to.";
    const CHASE: &str = "Hi Ben — the pricing sheet was due five days ago. Where has it got to?";
    let (base, _seen) = scripted_model(vec![wants(
        "chase_task",
        json!({ "task": "Pricing sheet", "message": CHASE }),
        SAY,
    )])
    .await;
    use_model(&h, &base).await;
    let agent = the_tasks_agent(&h).await;
    let channel = a_room_with(&h, "the chase", &agent).await;

    const QUESTION: &str = "@tasks chase Ben about the pricing sheet";
    let spoken = ask_in_room(&h, &channel, QUESTION).await;
    let proposal = spoken["proposal"]["id"]
        .as_str()
        .expect("a write is proposed, never run")
        .to_owned();
    assert_eq!(spoken["proposal"]["tool"], json!("chase_task"));
    assert!(
        h.acc.task_comments(&sheet).await.unwrap().is_empty(),
        "nothing has been said on the task yet"
    );

    let decided = approve(&h, &proposal).await;
    let result = &decided["result"]["result"];
    assert_eq!(result["kind"], json!("taskChased"));
    assert_eq!(result["title"], json!("Pricing sheet"));
    assert_eq!(result["owner"], json!("ben@a27-chase.test"));
    assert_eq!(result["daysLate"], json!(5));

    // The row itself: one comment, authored by the person who approved it.
    let comments = h.acc.task_comments(&sheet).await.unwrap();
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].body, CHASE);
    assert_eq!(comments[0].author, h.user.as_str());
    // …and nothing else about the task moved.
    let after = h.acc.task(&sheet).await.unwrap().unwrap();
    assert_eq!(after.assignee.as_deref(), Some(ben.as_str()));
    assert_eq!(after.due_at, Some(due(-5)));
    assert_eq!(after.status, "todo");

    transcript(
        QUESTION,
        &[
            format!("POST /chat/channels/{channel}/messages"),
            format!("     {}", json!({ "body": QUESTION })),
            "--- the agent's message, with its proposal ---".to_owned(),
            spoken.to_string(),
            format!(
                "POST /chat/proposals/{proposal} {}",
                json!({"approve": true})
            ),
            decided.to_string(),
        ],
    );
}

/// The refusals a chase earns. Chasing somebody about work that is not late is
/// the mistake an agent makes when it reads "soon" as "overdue", and it is the
/// one that costs a colleague's goodwill.
#[tokio::test]
async fn nothing_that_is_not_late_is_ever_chased() {
    let h = harness("agent-a27-nochase").await;
    let mine = h.acc.ensure_personal_project().await.unwrap();
    a_task(&h.acc, &mine, "Due tomorrow", Some(due(1)), None).await;
    a_task(&h.acc, &mine, "Due today", Some(due(0)), None).await;
    a_task(&h.acc, &mine, "Undated thing", None, None).await;
    let late = a_task(&h.acc, &mine, "Long overdue", Some(due(-2)), None).await;

    for (task, expected) in [
        ("Due tomorrow", "is not late — it is due on"),
        ("Due today", "is not late — it is due on"),
        (
            "Undated thing",
            "has no due date, so nobody is late with it",
        ),
    ] {
        let (status, body) = run(
            &h,
            "chase_task",
            json!({ "task": task, "message": "well?" }),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{task}: {body}");
        assert!(why(&body).contains(expected), "{task}: {body}");
    }
    // A missing message is a refusal too: a chase with no words is a
    // notification nobody can answer.
    let (status, body) = run(&h, "chase_task", json!({ "task": "Long overdue" })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(why(&body), "message is required");
    // …and the task really can be chased once it is late, with no owner named.
    let (status, body) = run(
        &h,
        "chase_task",
        json!({ "task": "Long overdue", "message": "where are we?" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["owner"], Value::Null);
    assert_eq!(body["result"]["daysLate"], json!(2));
    assert_eq!(h.acc.task_comments(&late).await.unwrap().len(), 1);
}

// ---- prioritising ---------------------------------------------------------------

/// One task's priority, and nothing else about it — the edit that must not
/// quietly rewrite a title or drop a due date on its way through.
#[tokio::test]
async fn setting_a_priority_changes_the_priority_and_nothing_else() {
    let h = harness("agent-a27-priority").await;
    let (ben, _) = a_colleague(&h, "ben@a27-priority.test").await;
    let team = h.acc.create_task_project("Sales", None).await.unwrap();
    let sheet = a_task(&h.acc, &team, "Pricing sheet", Some(due(2)), Some(&ben)).await;
    a_task(
        &h.acc,
        &team,
        "Pricing sheet review",
        Some(due(3)),
        Some(&ben),
    )
    .await;

    let (status, body) = run(
        &h,
        "set_task_priority",
        json!({ "task": "Pricing sheet", "priority": "HIGH" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["kind"], json!("taskPriority"));
    assert_eq!(body["result"]["was"], json!("medium"));
    assert_eq!(body["result"]["now"], json!("high"));

    let after = h.acc.task(&sheet).await.unwrap().unwrap();
    assert_eq!(after.priority, "high");
    assert_eq!(after.title, "Pricing sheet");
    assert_eq!(
        after.description.as_deref(),
        Some("Notes for Pricing sheet")
    );
    assert_eq!(after.assignee.as_deref(), Some(ben.as_str()));
    assert_eq!(after.due_at, Some(due(2)));

    // A word the board does not have is a refusal that lists the ones it does.
    let (status, body) = run(
        &h,
        "set_task_priority",
        json!({ "task": "Pricing sheet", "priority": "urgent" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(why(&body).contains("none, low, medium, high"), "{body}");

    // A title two unfinished tasks share is a question that names them, never a
    // guess — the rule a mis-aimed edit exists to prevent.
    let (status, body) = run(
        &h,
        "set_task_priority",
        json!({ "task": "Pricing", "priority": "low" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        why(&body).contains("more than one unfinished task"),
        "{body}"
    );
    assert!(
        why(&body).contains("Pricing sheet, Pricing sheet review"),
        "{body}"
    );
}

// ---- a conversation, written down -----------------------------------------------

/// **The item's fourth sentence.** A room is read, what it agreed is proposed,
/// approving it writes *proposals* rather than work, and asking the room again
/// says the actions have already been captured.
#[tokio::test]
async fn a_conversation_becomes_proposed_tasks_the_user_still_accepts() {
    let h = harness("agent-a27-capture").await;
    let agent = the_tasks_agent(&h).await;
    let launch = a_room_with(&h, "launch", &agent).await;
    said(
        &h,
        &launch,
        "We agreed Ben writes the press note by Friday.",
    )
    .await;
    said(&h, &launch, "And I will book the venue.").await;

    // First: the read. Nothing has been captured out of this room yet.
    let (status, body) = run(&h, "thread_actions", json!({ "room": "launch" })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["found"], json!(true));
    assert_eq!(body["result"]["messages"].as_array().unwrap().len(), 2);
    assert_eq!(
        body["result"]["messages"][0]["body"],
        json!("We agreed Ben writes the press note by Friday.")
    );
    assert!(
        body["result"]["alreadyCaptured"]
            .as_array()
            .unwrap()
            .is_empty(),
        "{body}"
    );
    // A room the asker cannot read is not an error and not an admission.
    let (status, body) = run(&h, "thread_actions", json!({ "room": "board-only" })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["found"], json!(false));

    const SAY: &str = "I'll write down the two things you agreed.";
    let actions = json!({
        "room": "launch",
        "tasks": [
            { "title": "Write the press note", "due": due(2).date().to_string(),
              "notes": "Ben, agreed in #launch" },
            { "title": "Book the venue" },
        ],
    });
    let (base, _seen) = scripted_model(vec![wants("capture_actions", actions.clone(), SAY)]).await;
    use_model(&h, &base).await;
    let room = a_room_with(&h, "the wrap up", &agent).await;

    const QUESTION: &str = "@tasks write down what we agreed in #launch";
    let spoken = ask_in_room(&h, &room, QUESTION).await;
    let proposal = spoken["proposal"]["id"]
        .as_str()
        .expect("a write is proposed, never run")
        .to_owned();
    assert_eq!(spoken["proposal"]["tool"], json!("capture_actions"));
    assert!(
        h.acc.task_proposals().await.unwrap().is_empty(),
        "nothing is written down until the asker taps it"
    );

    let decided = approve(&h, &proposal).await;
    let result = &decided["result"]["result"];
    assert_eq!(result["kind"], json!("actionsCaptured"));
    assert_eq!(result["captured"], json!(2));
    assert_eq!(result["state"], json!("proposed"));

    // The rows themselves: proposals, on the asker's own board, each carrying
    // the room it came out of. ADR 0023 — they are not work until accepted.
    let proposals = h.acc.task_proposals().await.unwrap();
    assert_eq!(proposals.len(), 2, "{proposals:?}");
    let note = proposals
        .iter()
        .find(|task| task.title == "Write the press note")
        .unwrap();
    assert_eq!(note.state, "proposed");
    assert_eq!(note.due_at, Some(due(2)));
    assert_eq!(note.source_kind.as_deref(), Some("chat"));
    assert_eq!(note.source_id.as_deref(), Some(launch.as_str()));
    // None of them is on the plate — a proposal is not work yet.
    let (status, body) = run(&h, "my_plate", json!({})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(titles(&body["result"]["comingUp"]).is_empty(), "{body}");

    // And the room now says so, so the same commitment is not written twice —
    // including the one still waiting to be accepted.
    let (status, body) = run(&h, "thread_actions", json!({ "room": "launch" })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let captured = &body["result"]["alreadyCaptured"];
    assert_eq!(captured.as_array().unwrap().len(), 2, "{body}");
    assert!(
        titles(captured).contains(&"Book the venue".to_owned()),
        "{body}"
    );
    assert_eq!(captured[0]["state"], json!("proposed"));

    // Accepting one keeps it captured — now as work rather than as a suggestion.
    h.acc.accept_task(&note.id, None).await.unwrap();
    let (status, body) = run(&h, "thread_actions", json!({ "room": "launch" })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let captured = &body["result"]["alreadyCaptured"];
    assert_eq!(captured.as_array().unwrap().len(), 2, "{body}");
    assert!(
        titles(captured).contains(&"Write the press note".to_owned()),
        "{body}"
    );

    transcript(
        QUESTION,
        &[
            format!("POST /chat/channels/{room}/messages"),
            format!("     {}", json!({ "body": QUESTION })),
            "--- the agent's message, with its proposal ---".to_owned(),
            spoken.to_string(),
            format!(
                "POST /chat/proposals/{proposal} {}",
                json!({"approve": true})
            ),
            decided.to_string(),
        ],
    );
}

/// What a capture refuses. Each of these would put half a conversation on
/// somebody's board, which is worse than none of it: the half that failed is
/// the half nobody notices is missing.
#[tokio::test]
async fn a_capture_refuses_a_room_that_is_not_yours_and_a_list_that_is_not_one() {
    let h = harness("agent-a27-capture-no").await;
    let agent = the_tasks_agent(&h).await;
    a_room_with(&h, "launch", &agent).await;

    let bad: [(Value, &str); 6] = [
        (
            json!({ "room": "nowhere", "tasks": [{ "title": "x" }] }),
            "no room of yours is called nowhere",
        ),
        (json!({ "tasks": [{ "title": "x" }] }), "room is required"),
        (json!({ "room": "launch" }), "tasks is a list of actions"),
        (
            json!({ "room": "launch", "tasks": [] }),
            "tasks is a list of actions",
        ),
        (
            json!({ "room": "launch", "tasks": [{ "notes": "no title" }] }),
            "action 1 has no title",
        ),
        (
            json!({ "room": "launch", "tasks": [{ "title": "x" }, { "title": "y", "due": "Friday" }] }),
            "action 2: due must be YYYY-MM-DD",
        ),
    ];
    for (args, expected) in bad {
        let (status, body) = run(&h, "capture_actions", args.clone()).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{args}: {body}");
        assert_eq!(why(&body), expected, "{args}");
    }
    // Eleven at a time is refused whole, not truncated to ten.
    let many: Vec<Value> = (0..11)
        .map(|n| json!({ "title": format!("a{n}") }))
        .collect();
    let (status, body) = run(
        &h,
        "capture_actions",
        json!({ "room": "launch", "tasks": many }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(why(&body).contains("at most 10 actions"), "{body}");
    // Nothing was written by any of them.
    assert!(h.acc.task_proposals().await.unwrap().is_empty());
}

// ---- isolation ------------------------------------------------------------------

/// **A list the asker could not open is not one the agent can reach** — across
/// a tenant boundary and across a colleague's private board alike, for every
/// tool in the set, reading and writing.
///
/// The refusal is the one an unknown title gets and never admits the task
/// exists: an agent that answered differently for a real one would be a way to
/// read somebody else's board by guessing at it.
#[tokio::test]
async fn a_task_of_another_tenant_or_another_person_cannot_be_named() {
    let h = harness("agent-a27-isolation").await;
    let other = common::harness_on(h.store.clone(), "agent-a27-stranger").await;
    let theirs = other.acc.ensure_personal_project().await.unwrap();
    let strangers = a_task(
        &other.acc,
        &theirs,
        "Their board meeting pack",
        Some(due(-3)),
        None,
    )
    .await;
    let (_, martas) = a_colleague(&h, "marta@a27-isolation.test").await;
    let hers = martas.ensure_personal_project().await.unwrap();
    let private = a_task(
        &martas,
        &hers,
        "Martas appraisal notes",
        Some(due(-3)),
        None,
    )
    .await;

    // Ours, so the refusals below are about reach and not about emptiness.
    let mine = h.acc.ensure_personal_project().await.unwrap();
    a_task(&h.acc, &mine, "Our own late thing", Some(due(-1)), None).await;

    for stranger in ["Their board meeting pack", "Martas appraisal notes"] {
        for (tool, args) in [
            (
                "set_task_priority",
                json!({ "task": stranger, "priority": "high" }),
            ),
            (
                "chase_task",
                json!({ "task": stranger, "message": "well?" }),
            ),
        ] {
            let (status, body) = run(&h, tool, args).await;
            assert_eq!(
                status,
                StatusCode::UNPROCESSABLE_ENTITY,
                "{tool} reached {stranger}: {body}"
            );
            assert_eq!(
                why(&body),
                format!("no unfinished task of yours is called {stranger}"),
                "{tool}/{stranger}"
            );
        }
    }
    // Nothing was written on either of them.
    assert_eq!(other.acc.task_comments(&strangers).await.unwrap().len(), 0);
    assert_eq!(martas.task_comments(&private).await.unwrap().len(), 0);
    assert_eq!(
        martas.task(&private).await.unwrap().unwrap().priority,
        "medium"
    );

    // And neither board can be looked into by asking the questions that read
    // one: the plate and the chase list hold our own late thing and nothing of
    // theirs.
    for tool in ["my_plate", "overdue_by_owner"] {
        let (status, body) = run(&h, tool, json!({})).await;
        assert_eq!(status, StatusCode::OK, "{tool}: {body}");
        let text = body.to_string();
        assert!(text.contains("Our own late thing"), "{tool}: {body}");
        assert!(!text.contains("Their board meeting pack"), "{tool}: {body}");
        assert!(!text.contains("Martas appraisal notes"), "{tool}: {body}");
    }
    // …and a room of another tenant's, named exactly, is simply not found.
    let (status, body) = run(&h, "thread_actions", json!({ "room": "launch" })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["found"], json!(false));
}
