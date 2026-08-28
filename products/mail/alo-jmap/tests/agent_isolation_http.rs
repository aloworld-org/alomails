//! **An agent reaches nothing the person who asked could not** — one test per
//! surface, on the wire (A1.6).
//!
//! The roster half of this is already settled elsewhere: a colleague and
//! another tenant cannot see an agent (`chat_agent_seed.rs`) or its one-to-one
//! (`chat_agent_dm.rs`). What is left is the half that actually reads records —
//! the **grounding** a turn is handed and the **reading tools** it runs inside
//! itself — across the three places a turn can happen:
//!
//! | surface | how a turn starts | what is proved here |
//! |---|---|---|
//! | channel | naming the agent in a room | a private room the asker is not in |
//! | agent DM | any message in a one-to-one | a colleague's diary |
//! | in-module | `POST /ai/agent` (Ask alo) | the one agent that looks everywhere still only looks where the asker can |
//!
//! Each test runs the **same question through the same agent twice**, changing
//! only who asked, and asserts on the two things a person would call the agent
//! reading something: what the model was *shown*, and what a tool executed
//! *inside the turn* handed back. A single-person test cannot tell an isolation
//! rule from a query that returns nothing at all, which is why every negative
//! here is paired with the positive of somebody who may see it.
//!
//! No live model is called: the tenant's AI backend is the scripted local
//! socket in `common::model`, which also records every request, and that
//! recording is what makes "shown" assertable.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use time::{Date, Month, Time};

use crate::common::model::{says, scripted_model, use_model, wants};
use crate::common::{Harness, harness, harness_on, send};
use alo_store::{
    AccountStore, AgentProduct, CalendarEvent, ChannelVisibility, ChatAgentId, EventId, MessageId,
};

// ---- the words every test files in two places --------------------------------

/// The word every record below carries, so what separates two people's answers
/// is never that they asked different questions.
const QUESTION: &str = "what did we say about the kestrel?";

/// Filed where only Ben can read it — a private room, and his own diary.
const BENS_ROOM_LINE: &str = "the kestrel deal closes on friday";
const BENS_DIARY: &str = "kestrel review with the board";

/// Filed where only Anna can read it.
const ANNAS_ROOM_LINE: &str = "the kestrel invoice is late";
const ANNAS_DIARY: &str = "kestrel planning";

/// The day both diaries are asked about, and the day both entries are on.
const DAY: &str = "2027-03-11";

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

/// Everything the model was shown in one request — the system prompt and the
/// numbered sources — as plain text.
///
/// Reading the message contents rather than the raw envelope matters: a source
/// is a JSON string inside a JSON body, so `to_string()` would have an
/// assertion matching escaped quotes and passing for the wrong reason.
fn shown(request: &Value) -> String {
    request["messages"]
        .as_array()
        .map(|messages| {
            messages
                .iter()
                .filter_map(|message| message["content"].as_str())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

/// A second person of the tenant: their own login, and their own door into the
/// store for the records only they can read.
struct Person {
    token: String,
    acc: AccountStore,
}

async fn colleague(h: &Harness, tag: &str) -> Person {
    let email = format!("{tag}-{}@agentiso.test", h.tenant);
    let user = h.ts.create_user(&email).await.unwrap();
    h.identity
        .set_password(&h.tenant, &user, &email, "s3cret-pw")
        .await
        .unwrap();
    let token = h
        .identity
        .password_login(&email, "s3cret-pw", None)
        .await
        .unwrap()
        .expect("token issued")
        .0
        .reveal()
        .to_owned();
    Person {
        token,
        acc: h.store.for_account(h.tenant.clone(), user),
    }
}

/// Says something and waits for the agent's answer to land in the room.
///
/// The turn is spawned off the request — the asker's words must not wait on
/// inference — so the reply has to be waited for. Waiting for a message *after*
/// this one rather than for any agent message at all, so a second question in a
/// room that already has an answer in it cannot pass on the first one.
async fn ask(app: &Router, token: &str, channel: &str, question: &str) -> Value {
    let (status, mine) = post(
        app,
        token,
        &format!("/chat/channels/{channel}/messages"),
        json!({ "body": question }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{mine}");
    let asked_at = mine["seq"].as_i64().unwrap_or(0);

    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let (status, body) = get(app, token, &format!("/chat/channels/{channel}/messages")).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let spoken = body["messages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["authorKind"] == "agent" && m["seq"].as_i64().unwrap_or(0) > asked_at)
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

/// Opens this person's one-to-one with `agent` and returns the room id.
async fn one_to_one(app: &Router, token: &str, agent: &ChatAgentId) -> String {
    let (status, room) = post(
        app,
        token,
        &format!("/chat/agents/{}/dm", agent.as_str()),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{room}");
    room["id"].as_str().unwrap().to_owned()
}

/// One diary entry on this person's own calendar, on [`DAY`].
async fn diary(acc: &AccountStore, summary: &str) {
    let calendar = acc.ensure_personal_calendar().await.unwrap();
    let start = Date::from_calendar_date(2027, Month::March, 11)
        .unwrap()
        .with_time(Time::from_hms(9, 0, 0).unwrap())
        .assume_utc();
    acc.create_event(&CalendarEvent {
        id: EventId::generate(),
        calendar_id: calendar,
        summary: summary.to_owned(),
        description: None,
        location: None,
        starts_at: start,
        ends_at: start + time::Duration::hours(1),
        all_day: false,
        recurrence: None,
        attendees: Vec::new(),
        exdates: Vec::new(),
        timezone: None,
        rdates: Vec::new(),
        recurrence_id: None,
        reminder_minutes: None,
        attendee_status: Vec::new(),
    })
    .await
    .unwrap();
}

/// One email in this person's own inbox, and the id of it.
async fn email(acc: &AccountStore, subject: &str, message_id: &str) -> MessageId {
    let inbox = acc.inbox().await.unwrap();
    let raw = format!(
        "From: sender@example.test\r\nSubject: {subject}\r\n\
         Message-ID: <{message_id}>\r\n\r\nthey wrote about the kestrel\r\n"
    );
    acc.ingest(&inbox, raw.as_bytes()).await.unwrap()
}

// ---- the channel ------------------------------------------------------------

/// **Surface one: a room.** The same agent, asked the same question, looks up
/// the same room by the same name for two people — and reads it for the one
/// who is in it.
///
/// Both halves of a turn are covered by that one pairing: the grounding it is
/// handed before it decides anything, and `catch_up_room` executed *inside* the
/// turn afterwards. The lookup is the sharper of the two, because the model
/// asked for a room by name and the name resolves for exactly one of them — a
/// refusal that comes back as "no such room" rather than as a 403, so an agent
/// cannot be used to discover that a private room exists.
#[tokio::test]
async fn a_room_turn_reads_only_what_the_person_who_asked_can_read() {
    let h = harness("agentisoroom").await;
    let ben = colleague(&h, "ben").await;
    let agent = h
        .acc
        .create_agent("chat", "Chat", Some("reads rooms"), AgentProduct::Chat)
        .await
        .unwrap();

    // Ben's private room. Anna is not in it, and nothing may tell her it is
    // there.
    let board = ben
        .acc
        .create_channel("boardroom", None, ChannelVisibility::Private)
        .await
        .unwrap();
    ben.acc
        .post_message(&board, BENS_ROOM_LINE, None)
        .await
        .unwrap();
    ben.acc.add_agent_to_channel(&board, &agent).await.unwrap();

    // Anna's own room, with the same agent in it.
    let desk = h
        .acc
        .create_channel("annas desk", None, ChannelVisibility::Public)
        .await
        .unwrap();
    h.acc
        .post_message(&desk, ANNAS_ROOM_LINE, None)
        .await
        .unwrap();
    h.acc.add_agent_to_channel(&desk, &agent).await.unwrap();

    // Two turns of two calls each: a lookup of the SAME room by the same name,
    // then a sentence. The only thing that differs between them is who asked.
    let (base, seen) = scripted_model(vec![
        wants(
            "catch_up_room",
            json!({ "room": "boardroom" }),
            "Let me catch up on that room.",
        ),
        says("I can't see anything about that."),
        wants(
            "catch_up_room",
            json!({ "room": "boardroom" }),
            "Let me catch up on that room.",
        ),
        says("It closes on friday."),
    ])
    .await;
    use_model(&h, &base).await;

    let annas = ask(
        &h.app,
        &h.token,
        desk.as_str(),
        &format!("@chat {QUESTION}"),
    )
    .await;
    assert_eq!(annas["body"], json!("I can't see anything about that."));
    let bens = ask(
        &h.app,
        &ben.token,
        board.as_str(),
        &format!("@chat {QUESTION}"),
    )
    .await;
    assert_eq!(bens["body"], json!("It closes on friday."));

    let asked = seen.lock().unwrap().clone();
    assert_eq!(asked.len(), 4, "two turns of two calls each: {asked:?}");

    // What each turn was grounded in before it decided anything.
    let annas_ground = shown(&asked[0]);
    assert!(
        annas_ground.contains(ANNAS_ROOM_LINE),
        "her own room grounds her turn: {annas_ground}"
    );
    assert!(
        !annas_ground.contains("closes on friday"),
        "a room she is not in must not ground her turn: {annas_ground}"
    );
    assert!(
        shown(&asked[2]).contains("closes on friday"),
        "Ben is in it, so it grounds his"
    );

    // And what the lookup — same tool, same arguments — handed back.
    let annas_lookup = shown(&asked[1]);
    assert!(
        annas_lookup.contains("\"found\":false"),
        "the room is not hers to read, and says so as an absence: {annas_lookup}"
    );
    assert!(
        !annas_lookup.contains("closes on friday"),
        "and hands back none of its words: {annas_lookup}"
    );
    let bens_lookup = shown(&asked[3]);
    assert!(
        bens_lookup.contains("\"found\":true") && bens_lookup.contains("closes on friday"),
        "the same lookup reads the room for the person who is in it: {bens_lookup}"
    );

    // The turn ran as Anna throughout: the refusal is hers, not the agent's.
    let runs = h.acc.agent_tool_runs(50).await.unwrap();
    assert_eq!(runs.len(), 1, "her own account holds her own run: {runs:?}");
    assert_eq!(runs[0].tool, "catch_up_room");
    assert_eq!(runs[0].effect, "read");
}

// ---- the one-to-one ---------------------------------------------------------

/// **Surface two: a one-to-one with an agent.** Every message in it is the
/// trigger, so there is no handle to get wrong — and the diary it reads is
/// still only the diary of whoever typed.
///
/// The Agenda agent grounds in events (A1.3) and reads them with `whats_on`, so
/// one question exercises both halves against a colleague's diary — the second
/// thing the queue item names.
#[tokio::test]
async fn a_one_to_one_carries_the_askers_own_diary_and_no_colleagues() {
    let h = harness("agentisodm").await;
    let ben = colleague(&h, "ben").await;
    let agent = h
        .acc
        .create_agent(
            "agenda",
            "Agenda",
            Some("knows the diary"),
            AgentProduct::Agenda,
        )
        .await
        .unwrap();

    diary(&h.acc, ANNAS_DIARY).await;
    diary(&ben.acc, BENS_DIARY).await;

    let (base, seen) = scripted_model(vec![
        wants(
            "whats_on",
            json!({ "from": DAY, "to": DAY }),
            "Let me look at the diary.",
        ),
        says("Planning, at nine."),
        wants(
            "whats_on",
            json!({ "from": DAY, "to": DAY }),
            "Let me look at the diary.",
        ),
        says("The review, at nine."),
    ])
    .await;
    use_model(&h, &base).await;

    let annas_room = one_to_one(&h.app, &h.token, &agent).await;
    let annas = ask(&h.app, &h.token, &annas_room, "anything about the kestrel?").await;
    assert_eq!(annas["body"], json!("Planning, at nine."));

    let bens_room = one_to_one(&h.app, &ben.token, &agent).await;
    assert_ne!(bens_room, annas_room, "a one-to-one is one person's");
    let bens = ask(
        &h.app,
        &ben.token,
        &bens_room,
        "anything about the kestrel?",
    )
    .await;
    assert_eq!(bens["body"], json!("The review, at nine."));

    let asked = seen.lock().unwrap().clone();
    assert_eq!(asked.len(), 4, "two turns of two calls each: {asked:?}");

    for (index, mine, theirs) in [
        (0, ANNAS_DIARY, BENS_DIARY),
        (1, ANNAS_DIARY, BENS_DIARY),
        (2, BENS_DIARY, ANNAS_DIARY),
        (3, BENS_DIARY, ANNAS_DIARY),
    ] {
        let text = shown(&asked[index]);
        assert!(
            text.contains(mine),
            "call {index} must carry the asker's own diary: {text}"
        );
        assert!(
            !text.contains(theirs),
            "call {index} must carry none of their colleague's: {text}"
        );
    }
}

// ---- in a module ------------------------------------------------------------

/// **Surface three: the command palette, in whatever module it was opened
/// from.** It *is* "Ask alo", the one agent ADR 0034 lets look across every
/// product — and looking everywhere still means everywhere **this person** can
/// look.
///
/// The tool half here is the execution boundary rather than a lookup: the
/// palette's approval route is handed a colleague's email by id, which is what
/// an injected turn would try, and it is refused because the door it runs
/// through is the caller's own.
#[tokio::test]
async fn the_palette_looks_everywhere_and_still_only_where_the_asker_can() {
    let h = harness("agentisoask").await;
    let ben = colleague(&h, "ben").await;

    let hers = email(&h.acc, "the kestrel quote", "iso-anna@alo.test").await;
    let his = email(&ben.acc, "the kestrel contract", "iso-ben@alo.test").await;

    let (base, seen) = scripted_model(vec![says("Your quote is the only one [1].")]).await;
    use_model(&h, &base).await;

    let (status, body) = post(&h.app, &h.token, "/ai/agent", json!({ "q": QUESTION })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let titles: Vec<String> = body["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|source| source["title"].as_str().unwrap_or_default().to_owned())
        .collect();
    assert!(
        titles.iter().any(|t| t == "the kestrel quote"),
        "Ask alo finds her own correspondence: {titles:?}"
    );
    assert!(
        !titles.iter().any(|t| t == "the kestrel contract"),
        "and never a colleague's, however wide its remit: {titles:?}"
    );

    let asked = seen.lock().unwrap().clone();
    assert_eq!(asked.len(), 1, "one call, no tool run: {asked:?}");
    let text = shown(&asked[0]);
    assert!(text.contains("the kestrel quote"), "{text}");
    assert!(
        !text.contains("the kestrel contract"),
        "the model is shown her reach and nothing wider: {text}"
    );

    // The execution boundary, on the same surface: a colleague's email named by
    // id is not hers to touch, and the refusal says nothing about it existing.
    let (status, refused) = post(
        &h.app,
        &h.token,
        "/ai/agent/execute",
        json!({ "tool": "mark_read", "args": { "message_id": his.as_str() } }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "a colleague's email is not hers to mark: {refused}"
    );
    // Paired with the positive, so the refusal above cannot be the route
    // failing for some other reason.
    let (status, done) = post(
        &h.app,
        &h.token,
        "/ai/agent/execute",
        json!({ "tool": "mark_read", "args": { "message_id": hers.as_str() } }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "her own email is hers: {done}");
    assert_eq!(done["result"]["id"], json!(hers.as_str()));
}

// ---- the tenant -------------------------------------------------------------

/// **Law 1, across the surfaces: no turn is ever another tenant's.**
///
/// The other tenant is deliberately a mirror — the same room name, the same
/// words, the same question — so nothing here can pass because the second
/// workspace happened to hold something different. `boardroom` resolves for
/// Carla and does not exist for Anna, and that is the whole assertion.
#[tokio::test]
async fn a_turn_is_never_another_tenants_on_any_surface() {
    let h = harness("agentisoa").await;
    let other = harness_on(Arc::clone(&h.store), "agentisob").await;

    // Carla's workspace: a public room called boardroom, and an email.
    let theirs = other
        .acc
        .create_channel("boardroom", None, ChannelVisibility::Public)
        .await
        .unwrap();
    other
        .acc
        .post_message(&theirs, "the kestrel launch is on tuesday", None)
        .await
        .unwrap();
    let their_email = email(&other.acc, "the kestrel launch", "iso-carla@alo.test").await;
    let their_agent = other
        .acc
        .create_agent("chat", "Chat", None, AgentProduct::Chat)
        .await
        .unwrap();
    other
        .acc
        .add_agent_to_channel(&theirs, &their_agent)
        .await
        .unwrap();

    // Anna's workspace: her own room, her own email, her own agent.
    let her_email = email(&h.acc, "the kestrel quote", "iso-anna2@alo.test").await;
    let hers = h
        .acc
        .create_channel("annas desk", None, ChannelVisibility::Public)
        .await
        .unwrap();
    let her_agent = h
        .acc
        .create_agent("chat", "Chat", None, AgentProduct::Chat)
        .await
        .unwrap();
    h.acc.add_agent_to_channel(&hers, &her_agent).await.unwrap();

    // A scripted model per tenant: a provider is a tenant's own setting, and
    // sharing one socket would interleave two workspaces' calls.
    let (her_base, her_seen) = scripted_model(vec![
        wants(
            "catch_up_room",
            json!({ "room": "boardroom" }),
            "Let me catch up on that room.",
        ),
        says("There's no such room here."),
    ])
    .await;
    use_model(&h, &her_base).await;
    let (their_base, their_seen) = scripted_model(vec![
        wants(
            "catch_up_room",
            json!({ "room": "boardroom" }),
            "Let me catch up on that room.",
        ),
        says("The launch is on tuesday."),
    ])
    .await;
    use_model(&other, &their_base).await;

    // The channel surface, both sides.
    ask(
        &h.app,
        &h.token,
        hers.as_str(),
        &format!("@chat {QUESTION}"),
    )
    .await;
    ask(
        &other.app,
        &other.token,
        theirs.as_str(),
        &format!("@chat {QUESTION}"),
    )
    .await;

    let her_calls = her_seen.lock().unwrap().clone();
    let their_calls = their_seen.lock().unwrap().clone();
    assert_eq!(her_calls.len(), 2, "{her_calls:?}");
    assert_eq!(their_calls.len(), 2, "{their_calls:?}");
    let her_lookup = shown(&her_calls[1]);
    assert!(
        her_lookup.contains("\"found\":false") && !her_lookup.contains("launch is on tuesday"),
        "another tenant's room does not exist for her: {her_lookup}"
    );
    assert!(
        shown(&their_calls[1]).contains("launch is on tuesday"),
        "and does for the tenant whose room it is"
    );
    assert!(
        !shown(&her_calls[0]).contains("launch is on tuesday"),
        "nor does it ground her turn"
    );

    // The in-module surface: the widest agent there is, still inside one
    // tenant.
    let (status, body) = post(&h.app, &h.token, "/ai/agent", json!({ "q": QUESTION })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let titles: Vec<String> = body["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|source| source["title"].as_str().unwrap_or_default().to_owned())
        .collect();
    assert!(
        titles.iter().any(|t| t == "the kestrel quote"),
        "{titles:?}"
    );
    assert!(
        !titles.iter().any(|t| t == "the kestrel launch"),
        "another tenant's mail is never a source: {titles:?}"
    );
    let (status, refused) = post(
        &h.app,
        &h.token,
        "/ai/agent/execute",
        json!({ "tool": "mark_read", "args": { "message_id": their_email.as_str() } }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "another tenant's email is not hers to touch: {refused}"
    );
    // And the id that does work proves the refusal was about the tenant.
    let (status, done) = post(
        &h.app,
        &h.token,
        "/ai/agent/execute",
        json!({ "tool": "mark_read", "args": { "message_id": her_email.as_str() } }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{done}");

    // The one-to-one surface: its room and its feed are `chat_agent_dm.rs`'s
    // wrong-tenant test; what belongs here is that the agent id itself is not a
    // way in, so a turn cannot start at all.
    let (status, body) = post(
        &other.app,
        &other.token,
        &format!("/chat/agents/{}/dm", her_agent.as_str()),
        json!({}),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "another tenant's agent is not one you can open a room with: {body}"
    );
}

// ---- the approval -----------------------------------------------------------

/// The other end of a turn: a change an agent proposed runs with the **asker's**
/// reach, so only the asker may set it going.
///
/// Everyone in the room can see the proposal — it is a sentence in a
/// conversation — and a colleague tapping it would run Anna's question through
/// Ben's access, which is the same widening from the other direction. The task
/// landing in her own list and in nobody else's is what proves whose reach ran.
#[tokio::test]
async fn only_the_person_who_asked_can_approve_what_an_agent_proposed() {
    let h = harness("agentisotap").await;
    let ben = colleague(&h, "ben").await;
    let agent = h
        .acc
        .create_agent(
            "tasks",
            "Tasks",
            Some("keeps the list"),
            AgentProduct::Tasks,
        )
        .await
        .unwrap();
    let room = h
        .acc
        .create_channel("ordering", None, ChannelVisibility::Public)
        .await
        .unwrap();
    h.acc.add_agent_to_channel(&room, &agent).await.unwrap();
    let (status, joined) = post(
        &h.app,
        &ben.token,
        &format!("/chat/channels/{}/join", room.as_str()),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{joined}");

    let (base, _seen) = scripted_model(vec![wants(
        "create_task",
        json!({ "title": "Chase the kestrel invoice" }),
        "I'll add a task to chase it.",
    )])
    .await;
    use_model(&h, &base).await;

    let spoken = ask(&h.app, &h.token, room.as_str(), "@tasks chase the kestrel").await;
    assert_eq!(spoken["proposal"]["state"], json!("pending"));
    assert_eq!(spoken["proposal"]["askedBy"], json!(h.user.as_str()));
    let proposal = spoken["proposal"]["id"].as_str().unwrap().to_owned();

    // Ben can read it — it is a message in a room he is in — and cannot decide
    // it. Said as a 403 rather than a 404 on purpose: hiding it would be a lie
    // about a sentence he is looking at.
    let (status, refused) = post(
        &h.app,
        &ben.token,
        &format!("/chat/proposals/{proposal}"),
        json!({ "approve": true }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{refused}");

    let hers = h.acc.ensure_personal_project().await.unwrap();
    let his = ben.acc.ensure_personal_project().await.unwrap();
    assert!(
        h.acc.tasks_in_project(&hers).await.unwrap().is_empty(),
        "a refused tap creates nothing for the asker"
    );
    assert!(
        ben.acc.tasks_in_project(&his).await.unwrap().is_empty(),
        "and nothing for the person who tapped"
    );
    assert!(
        h.acc.agent_tool_runs(50).await.unwrap().is_empty(),
        "nothing ran, so nothing is logged as having run"
    );

    // Her own tap runs it, once, through her own door.
    let (status, decided) = post(
        &h.app,
        &h.token,
        &format!("/chat/proposals/{proposal}"),
        json!({ "approve": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{decided}");
    assert_eq!(decided["state"], json!("approved"));
    let tasks = h.acc.tasks_in_project(&hers).await.unwrap();
    assert_eq!(tasks.len(), 1, "{tasks:?}");
    assert_eq!(tasks[0].title, "Chase the kestrel invoice");
    assert!(
        ben.acc.tasks_in_project(&his).await.unwrap().is_empty(),
        "the change landed in the asker's list, not the room's"
    );
}
