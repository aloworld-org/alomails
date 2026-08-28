//! **The Meet agent, end to end** (A3.2) — a meeting after the fact.
//!
//! The queue item leaves a Meet agent four sentences to prove, each asked the
//! way a person asks it:
//!
//! - `@meet what happened in the Q3 budget review?` — **answered in the room
//!   with no button in between**, out of the meeting's own record (who was
//!   there, what was said, what was typed during it) rather than out of a
//!   search;
//! - `@meet write up the Q3 budget review` — a **proposal**, and nothing is
//!   posted until the asker taps it; then the minutes appear in the meeting's
//!   own conversation, under the asker's own name, with the decisions and the
//!   actions as the meeting agreed them;
//! - and then the sentence the item exists for: `@tasks write down what we
//!   agreed in #q3-budget` — the actions become **proposed tasks through the
//!   Tasks agent's own tool**, accepted one at a time in the task list
//!   (ADR 0023). There is no second mechanism inside Meet, and the boundary
//!   proves it: a Meet agent that asks for `create_task` is refused at the
//!   execution boundary, not merely left uninvited in the prompt.
//! - the refusals: a meeting still running has no minutes yet, a title that ran
//!   twice is a question that names the days, and a meeting nobody started from
//!   a conversation has nowhere for its minutes to go.
//!
//! And the isolation sentence the wave holds every agent to: another tenant's
//! meeting cannot be named, and neither can one in a room the asker is not in —
//! the refusal is the one an unknown title gets, so nobody learns what somebody
//! else was in a meeting about by asking about it.
//!
//! **No live model is ever called** (the loop's standing rail): the tenant's AI
//! backend is the scripted local socket in `common::model`, and the assertions
//! are about the bytes the model was *shown* and the rows the store holds
//! afterwards.
//!
//! Run the transcript with
//! `cargo nextest run -p alo-jmap --test agent_meet_http --no-capture`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::{Duration as Wait, Instant};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use crate::common::model::{Seen, says, scripted_model, use_model, wants};
use crate::common::{Harness, harness, send};
use alo_store::{
    AccountStore, AgentProduct, ChannelVisibility, ChatChannelId, MeetingId, NewMeeting, UserId,
};

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

/// The id of one of the tenant's own agents, out of the set a first look at
/// `GET /chat/agents` seeds (A1.5). Meet is among them and always was — until
/// this item it simply had nothing to do.
async fn the_agent(h: &Harness, product: AgentProduct) -> String {
    let handle = alo_store::default_handle(product);
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

/// A room, with an agent in it — both over HTTP, as a person makes them.
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
    println!("\n===== A3.2 TRANSCRIPT: {title} =====");
    for line in lines {
        println!("{line}");
    }
    println!("===== end: {title} =====\n");
}

// ---- the meetings under test ----------------------------------------------------

/// A meeting that has happened: started from a room, attended, spoken in, typed
/// in, and over.
///
/// Written through the store rather than over `/meet/{id}/join`, for one
/// reason: joining mints a LiveKit token, and a test harness deliberately has
/// no media engine configured. Attendance itself is ours (`alo_store::meet`),
/// which is exactly why it can be recorded without one.
async fn a_meeting(acc: &AccountStore, title: &str, room: Option<&ChatChannelId>) -> MeetingId {
    let meeting = acc
        .create_meeting(&NewMeeting {
            title: title.to_owned(),
            channel_id: room.cloned(),
            event_id: None,
        })
        .await
        .unwrap();
    acc.join_meeting(&meeting.id).await.unwrap();
    meeting.id
}

/// Somebody says something out loud in a meeting — one transcript segment,
/// spoken by whoever's door it is written through.
async fn spoke(acc: &AccountStore, meeting: &MeetingId, id: &str, text: &str) {
    acc.put_meeting_transcript_segment(meeting, id, text, true)
        .await
        .unwrap();
}

/// …and somebody types one, in the meeting's own chat.
async fn typed(acc: &AccountStore, meeting: &MeetingId, text: &str) {
    acc.post_meeting_message(meeting, text, None).await.unwrap();
}

/// A colleague in the same tenant, with a door of their own.
async fn a_colleague(h: &Harness, address: &str) -> (UserId, AccountStore) {
    let user = h.ts.create_user(address).await.unwrap();
    let acc = h.store.for_account(h.tenant.clone(), user.clone());
    (user, acc)
}

/// The whole of one sitting: a room, a meeting in it, two people, what they
/// said and typed, and the end of it.
async fn the_q3_budget_review(h: &Harness, room: &str) -> (String, MeetingId, UserId) {
    let (ben, bens) = a_colleague(h, "ben@a32.test").await;
    let channel = ChatChannelId::new(room.to_owned());
    bens.join_channel(&channel).await.unwrap();
    let meeting = a_meeting(&h.acc, "Q3 budget review", Some(&channel)).await;
    bens.join_meeting(&meeting).await.unwrap();
    spoke(
        &h.acc,
        &meeting,
        "s1",
        "We are eleven thousand over on the marketing line.",
    )
    .await;
    spoke(
        &bens,
        &meeting,
        "s2",
        "Then we hold marketing flat and move the rest to hosting.",
    )
    .await;
    spoke(
        &h.acc,
        &meeting,
        "s3",
        "Agreed. Ben sends the revised sheet before Thursday.",
    )
    .await;
    typed(&bens, &meeting, "I will send the revised sheet.").await;
    h.acc.end_meeting(&meeting).await.unwrap();
    (room.to_owned(), meeting, ben)
}

// ---- what happened in the meeting, on the wire ----------------------------------

/// **The item's first sentence, end to end.** The record is read, the answer
/// lands in the room, and there is no button in between — asking what happened
/// in a meeting changes nothing.
#[tokio::test]
async fn the_meet_agent_answers_from_the_record_with_no_button_in_between() {
    let h = harness("agent-a32-record").await;
    let agent = the_agent(&h, AgentProduct::Meet).await;
    let room = a_room_with(&h, "q3-budget", &agent).await;
    let (_, _meeting, ben) = the_q3_budget_review(&h, &room).await;

    const ANSWER: &str = "You were eleven thousand over on marketing; you agreed to hold \
marketing flat and move the rest to hosting, and Ben is sending the revised sheet before \
Thursday [1].";
    let (base, seen) = scripted_model(vec![
        wants(
            "meeting_record",
            json!({ "meeting": "Q3 budget review" }),
            "Let me look at what was said.",
        ),
        says(ANSWER),
    ])
    .await;
    use_model(&h, &base).await;

    const QUESTION: &str = "@meet what happened in the Q3 budget review?";
    let spoken = ask_in_room(&h, &room, QUESTION).await;

    assert_eq!(spoken["body"], json!(ANSWER));
    assert_eq!(
        spoken["proposal"],
        Value::Null,
        "asking what happened in a meeting must not put a button in the room"
    );

    // What the model was shown: the meeting's own record, not a search snippet.
    let second = shown(&seen, 1);
    assert!(second.contains("meetingRecord"), "{second}");
    assert!(second.contains("eleven thousand over"), "{second}");
    assert!(second.contains("hold marketing flat"), "{second}");
    // The typed message counts as part of what happened…
    assert!(
        second.contains("I will send the revised sheet."),
        "{second}"
    );
    // …and so does who was actually in the room, by the name a colleague knows.
    assert!(
        second.contains(ben.as_str()) || second.contains("ben@a32.test"),
        "{second}"
    );

    // …and the first call offered Meet's tools and nobody else's: a Meet agent
    // that could see `create_task` would eventually use it.
    let first = seen.lock().unwrap()[0]["messages"][0]["content"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(first.starts_with("You are the alo Meet agent"), "{first}");
    assert!(first.contains("- meeting_minutes:"), "{first}");
    assert!(!first.contains("- create_task:"), "{first}");
    assert!(!first.contains("- create_event:"), "{first}");

    transcript(
        QUESTION,
        &[
            format!("POST /chat/channels/{room}/messages"),
            format!("     {}", json!({ "body": QUESTION })),
            "--- what came back in the room ---".to_owned(),
            spoken.to_string(),
        ],
    );
}

// ---- the minutes, and what they become ------------------------------------------

/// **The item's second and third sentences.** The minutes are proposed, the
/// asker's own tap posts them into the meeting's conversation under their own
/// name, and the actions in them become work only through the Tasks agent's own
/// proposals — which is what "the ordinary agent path" means.
#[tokio::test]
async fn minutes_are_proposed_then_posted_and_their_actions_become_proposed_tasks() {
    let h = harness("agent-a32-minutes").await;
    let meet = the_agent(&h, AgentProduct::Meet).await;
    let room = a_room_with(&h, "q3-budget", &meet).await;
    let (_, _meeting, _ben) = the_q3_budget_review(&h, &room).await;

    const SAY: &str = "I'll post the minutes of the Q3 budget review.";
    let minutes = json!({
        "meeting": "Q3 budget review",
        "summary": "We are eleven thousand over on the marketing line, so marketing holds \
    flat and the difference moves to hosting.",
        "decisions": [
            "Hold the marketing budget flat",
            "Move the difference to hosting",
        ],
        "actions": [
            { "what": "Send the revised sheet", "owner": "Ben", "due": "2026-08-20" },
        ],
    });
    let (base, _seen) = scripted_model(vec![wants("meeting_minutes", minutes.clone(), SAY)]).await;
    use_model(&h, &base).await;

    const QUESTION: &str = "@meet write up the Q3 budget review";
    let spoken = ask_in_room(&h, &room, QUESTION).await;
    let proposal = spoken["proposal"]["id"]
        .as_str()
        .expect("a write is proposed, never run")
        .to_owned();
    assert_eq!(spoken["proposal"]["tool"], json!("meeting_minutes"));

    // Nothing is in the room yet but the question and the agent's sentence.
    let (status, before) = get(&h.app, &h.token, &format!("/chat/channels/{room}/messages")).await;
    assert_eq!(status, StatusCode::OK, "{before}");
    assert!(
        !before.to_string().contains("Minutes — Q3 budget review"),
        "nothing is posted until the asker taps it: {before}"
    );

    let decided = approve(&h, &proposal).await;
    let result = &decided["result"]["result"];
    assert_eq!(result["kind"], json!("meetingMinutes"));
    assert_eq!(result["title"], json!("Q3 budget review"));
    assert_eq!(result["room"], json!("q3-budget"));
    assert_eq!(result["decisions"], json!(2));
    assert_eq!(result["actions"], json!(1));

    // The message itself: in the meeting's own conversation, under the name of
    // the person who approved it — not a robot's.
    let posted = h
        .acc
        .messages(&ChatChannelId::new(room.clone()), None, 20)
        .await
        .unwrap()
        .into_iter()
        .find(|m| m.message.body.starts_with("Minutes — Q3 budget review"))
        .expect("the minutes are in the room");
    assert!(
        !posted.message.author_is_agent,
        "the minutes are the asker's"
    );
    assert_eq!(posted.message.author.as_str(), h.user.as_str());
    let body = posted.message.body.clone();
    assert!(body.contains("eleven thousand over"), "{body}");
    assert!(
        body.contains("\nDecisions\n- Hold the marketing budget flat"),
        "{body}"
    );
    assert!(
        body.contains("\nActions\n- Send the revised sheet — Ben, by 2026-08-20"),
        "{body}"
    );

    // …and nothing else happened: posting minutes creates no task and no event.
    assert!(
        h.acc.task_proposals().await.unwrap().is_empty(),
        "minutes are a message, not a second way to put work on a board"
    );

    // Now the ordinary agent path: the Tasks agent writes the actions down out
    // of the same room, as proposals the user still accepts one at a time.
    let tasks = the_agent(&h, AgentProduct::Tasks).await;
    let (status, added) = post(
        &h.app,
        &h.token,
        &format!("/chat/channels/{room}/agents"),
        json!({ "agent": tasks }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{added}");
    let capture = json!({
        "room": "q3-budget",
        "tasks": [
            { "title": "Send the revised sheet", "due": "2026-08-20",
              "notes": "Ben, agreed in the Q3 budget review" },
        ],
    });
    let (base, _seen) = scripted_model(vec![wants(
        "capture_actions",
        capture,
        "I'll write down what the minutes say you agreed.",
    )])
    .await;
    use_model(&h, &base).await;
    let elsewhere = a_room_with(&h, "the wrap up", &tasks).await;
    const HANDOVER: &str = "@tasks write down what we agreed in #q3-budget";
    let handed = ask_in_room(&h, &elsewhere, HANDOVER).await;
    let second = handed["proposal"]["id"].as_str().unwrap().to_owned();
    assert_eq!(handed["proposal"]["tool"], json!("capture_actions"));
    let ran = approve(&h, &second).await;
    assert_eq!(ran["result"]["result"]["state"], json!("proposed"));

    let proposals = h.acc.task_proposals().await.unwrap();
    assert_eq!(proposals.len(), 1, "{proposals:?}");
    assert_eq!(proposals[0].title, "Send the revised sheet");
    assert_eq!(proposals[0].state, "proposed");
    assert_eq!(proposals[0].source_kind.as_deref(), Some("chat"));

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
            "--- what the room now holds ---".to_owned(),
            body,
            format!("--- then, through the ordinary agent path ---\n{HANDOVER}"),
            handed.to_string(),
            ran.to_string(),
        ],
    );
}

/// The whole of "no second mechanism", at the layer that decides rather than
/// the one that asks nicely: a Meet agent that names `create_task` is refused
/// at the **execution boundary**, and nothing lands on anybody's board.
#[tokio::test]
async fn a_meet_agent_that_asks_for_a_task_is_refused_at_the_boundary() {
    let h = harness("agent-a32-boundary").await;
    let meet = the_agent(&h, AgentProduct::Meet).await;
    let room = a_room_with(&h, "q3-budget", &meet).await;
    the_q3_budget_review(&h, &room).await;

    let (base, _seen) = scripted_model(vec![wants(
        "create_task",
        json!({ "title": "Send the revised sheet" }),
        "I'll add a task for that.",
    )])
    .await;
    use_model(&h, &base).await;

    let spoken = ask_in_room(&h, &room, "@meet turn that into a task").await;
    let proposal = spoken["proposal"]["id"].as_str().unwrap().to_owned();
    let (status, refused) = post(
        &h.app,
        &h.token,
        &format!("/chat/proposals/{proposal}"),
        json!({ "approve": true }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{refused}");
    assert!(why(&refused).contains("meet"), "{refused}");

    let mine = h.acc.ensure_personal_project().await.unwrap();
    assert!(
        h.acc.tasks_in_project(&mine).await.unwrap().is_empty(),
        "a Meet agent cannot put work on a board, however the tap was made"
    );
}

// ---- the refusals ---------------------------------------------------------------

/// A meeting that is still running has no minutes yet, and saying "no such
/// meeting" about one somebody is sitting in would be a lie. A title that ran
/// twice is a question naming the days. A meeting nobody started from a room
/// has nowhere for its minutes to go.
#[tokio::test]
async fn the_three_refusals_a_meeting_earns() {
    let h = harness("agent-a32-refusals").await;
    let meet = the_agent(&h, AgentProduct::Meet).await;
    let room = a_room_with(&h, "q3-budget", &meet).await;
    let channel = ChatChannelId::new(room.clone());

    // One still running.
    a_meeting(&h.acc, "Standup", Some(&channel)).await;
    let (status, refused) = run(&h, "meeting_record", json!({ "meeting": "Standup" })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
    assert!(why(&refused).contains("has not ended yet"), "{refused}");

    // One title, twice — the days are what tell them apart.
    for _ in 0..2 {
        let twice = a_meeting(&h.acc, "Weekly review", Some(&channel)).await;
        h.acc.end_meeting(&twice).await.unwrap();
    }
    let (status, refused) = run(&h, "meeting_record", json!({ "meeting": "Weekly review" })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
    assert!(why(&refused).contains("more than once"), "{refused}");
    assert!(why(&refused).contains("say which day"), "{refused}");
    // …and the day really does narrow: a day neither of them ran on leaves
    // nothing, in the words an unknown title earns. (Both of these ran today,
    // seconds apart, so today cannot tell them apart — which is the honest
    // limit of a fixture that cannot backdate a meeting's ending.)
    let yesterday = (time::OffsetDateTime::now_utc().date() - time::Duration::days(1)).to_string();
    let (status, none) = run(
        &h,
        "meeting_record",
        json!({ "meeting": "Weekly review", "day": yesterday }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{none}");
    assert!(why(&none).contains("no meeting of yours"), "{none}");
    let (status, bad) = run(
        &h,
        "meeting_record",
        json!({ "meeting": "Weekly review", "day": "last Tuesday" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}");
    assert!(why(&bad).contains("YYYY-MM-DD"), "{bad}");

    // One started outside any conversation: its record reads, its minutes have
    // nowhere to go, and the refusal says which of the two it is.
    let loose = a_meeting(&h.acc, "Kickoff", None).await;
    h.acc.end_meeting(&loose).await.unwrap();
    let (status, read) = run(&h, "meeting_record", json!({ "meeting": "Kickoff" })).await;
    assert_eq!(status, StatusCode::OK, "{read}");
    assert_eq!(read["result"]["room"], Value::Null);
    let (status, refused) = run(
        &h,
        "meeting_minutes",
        json!({ "meeting": "Kickoff", "summary": "We began." }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
    assert!(why(&refused).contains("no thread"), "{refused}");

    // A name nothing answers to is the ordinary refusal, and it repeats what
    // was asked for rather than describing what exists.
    let (status, refused) = run(&h, "meeting_record", json!({ "meeting": "Hovercraft" })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
    assert!(why(&refused).contains("no meeting of yours"), "{refused}");
}

// ---- isolation ------------------------------------------------------------------

/// The wave's isolation sentence, on this surface. Another tenant's meeting and
/// a colleague's private room both come back as "no meeting of yours" — the
/// same words an invented title gets, so asking is not a way to find out that
/// somebody else's meeting exists.
#[tokio::test]
async fn a_meeting_that_is_not_the_askers_cannot_be_named() {
    let h = harness("agent-a32-iso-a").await;
    let other = harness("agent-a32-iso-b").await;

    // Another tenant's, in their own room.
    let theirs = other
        .acc
        .create_channel("their-room", None, ChannelVisibility::Public)
        .await
        .unwrap();
    let hidden = a_meeting(&other.acc, "Acme renewal", Some(&theirs)).await;
    spoke(&other.acc, &hidden, "s1", "They are asking for a discount.").await;
    other.acc.end_meeting(&hidden).await.unwrap();

    // …and a colleague's, in a private room of this tenant that the asker is
    // not in.
    let (_ben, bens) = a_colleague(&h, "ben@a32-iso.test").await;
    let private = bens
        .create_channel("bens-room", None, ChannelVisibility::Private)
        .await
        .unwrap();
    let bens_meeting = a_meeting(&bens, "Pay review", Some(&private)).await;
    bens.end_meeting(&bens_meeting).await.unwrap();

    // Neither is in the listing…
    let (status, listed) = run(&h, "meetings_recent", json!({})).await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    let all = listed.to_string();
    assert!(!all.contains("Acme renewal"), "{all}");
    assert!(!all.contains("Pay review"), "{all}");

    // …and neither can be named, in the words an invented title earns.
    for title in ["Acme renewal", "Pay review"] {
        let (status, refused) = run(&h, "meeting_record", json!({ "meeting": title })).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
        assert_eq!(
            why(&refused),
            format!("no meeting of yours that has ended is called {title}"),
            "the refusal must not say whether it exists"
        );
        let (status, refused) = run(
            &h,
            "meeting_minutes",
            json!({ "meeting": title, "summary": "..." }),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
    }

    // And the tenant that owns them still reads its own.
    let (status, theirs_listed) = run(&other, "meetings_recent", json!({})).await;
    assert_eq!(status, StatusCode::OK, "{theirs_listed}");
    assert!(
        theirs_listed.to_string().contains("Acme renewal"),
        "{theirs_listed}"
    );
}
