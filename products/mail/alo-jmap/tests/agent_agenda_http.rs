//! **The Agenda agent past the caller's own diary, end to end** (A2.6) — the
//! three sentences the queue item leaves a calendar agent to prove, each asked
//! the way a person asks it:
//!
//! - `@agenda when can Ben and I meet for an hour this week?` — **answered in
//!   the room with no button in between**, out of both diaries, and a
//!   colleague whose calendar is not shared comes back named rather than
//!   counted as free;
//! - `@agenda what do I need for the Delaunay review?` — the meeting, the mail
//!   that goes with it and the text of what was attached to that mail, so a
//!   briefing is written from the correspondence rather than from the title;
//! - `@agenda move the Delaunay review to Thursday at 2` — a **proposal**, and
//!   nothing has moved until the asker taps it; the meeting keeps its length,
//!   one sitting of a repeating meeting moves on its own, and the series stays.
//!
//! And the isolation sentence the wave holds every agent to: a meeting of
//! another tenant's cannot be named, and neither can a colleague's private one
//! — the refusal is the same one an unknown title gets, so nobody learns what
//! is in somebody else's diary by asking about it.
//!
//! **No live model is ever called** (the loop's standing rail): the tenant's AI
//! backend is the scripted local socket in `common::model`, and the assertions
//! are about the bytes the model was *shown* and the rows the store holds
//! afterwards.
//!
//! Run the transcript with
//! `cargo nextest run -p alo-jmap --test agent_agenda_http --no-capture`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use std::time::{Duration as Wait, Instant};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use time::{Date, Duration, OffsetDateTime};

use crate::common::model::{Seen, says, scripted_model, use_model, wants};
use crate::common::{Harness, harness, send};
use alo_store::{AccountStore, AgentProduct, CalendarEvent, CalendarId, EventId, UserId};

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

/// The id of the tenant's own Agenda agent, out of the set a first look at
/// `GET /chat/agents` seeds (A1.5).
async fn the_agenda_agent(h: &Harness) -> String {
    let handle = alo_store::default_handle(AgentProduct::Agenda);
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
    println!("\n===== A2.6 TRANSCRIPT: {title} =====");
    for line in lines {
        println!("{line}");
    }
    println!("===== end: {title} =====\n");
}

// ---- the diaries under test ----------------------------------------------------

/// The one day every test in this file is about: far enough ahead to be inside
/// the window `meeting_prep` looks in, and stated rather than "today" so a run
/// at 23:55 does not test a different day from the one it set up.
fn the_day() -> Date {
    OffsetDateTime::now_utc().date() + Duration::days(3)
}

fn at(day: Date, hour: u8, minute: u8) -> OffsetDateTime {
    day.with_hms(hour, minute, 0).unwrap().assume_utc()
}

/// A meeting in somebody's diary, on their personal calendar.
async fn a_meeting(
    acc: &AccountStore,
    title: &str,
    start: OffsetDateTime,
    minutes: i64,
) -> EventId {
    let event = an_event(acc, title, start, minutes, None, false).await;
    acc.create_event(&event).await.unwrap()
}

/// …and the general form: recurring, or all-day, or neither.
async fn an_event(
    acc: &AccountStore,
    title: &str,
    start: OffsetDateTime,
    minutes: i64,
    rrule: Option<&str>,
    all_day: bool,
) -> CalendarEvent {
    let calendar = acc.ensure_personal_calendar().await.unwrap();
    CalendarEvent {
        id: EventId::generate(),
        calendar_id: calendar,
        summary: title.to_owned(),
        description: Some(format!("Notes for {title}")),
        location: Some("Room 2".to_owned()),
        starts_at: start,
        ends_at: start + Duration::minutes(minutes),
        all_day,
        recurrence: rrule.map(str::to_owned),
        attendees: vec!["paula@delaunay.example".to_owned()],
        exdates: Vec::new(),
        timezone: None,
        rdates: Vec::new(),
        recurrence_id: None,
        reminder_minutes: Some(10),
        attendee_status: Vec::new(),
    }
}

/// A colleague in the same tenant, with a diary of their own.
async fn a_colleague(h: &Harness, address: &str) -> (UserId, AccountStore) {
    let user = h.ts.create_user(address).await.unwrap();
    let acc = h.store.for_account(h.tenant.clone(), user.clone());
    (user, acc)
}

/// Their diary, shared with the asker at a role.
async fn shares_with(theirs: &AccountStore, me: &UserId, role: &str) -> CalendarId {
    let calendar = theirs.ensure_personal_calendar().await.unwrap();
    theirs
        .grant_calendar(&calendar, "user", me.as_str(), role)
        .await
        .unwrap();
    calendar
}

/// When an event now starts and ends, read back through its owner's store.
async fn now_at(acc: &AccountStore, id: &EventId) -> (OffsetDateTime, OffsetDateTime) {
    let event = acc.event(id).await.unwrap().unwrap();
    (event.starts_at, event.ends_at)
}

// ---- finding a time, on the wire ------------------------------------------------

/// **The item's first sentence, end to end.** Both diaries are read, the slots
/// avoid what is in either of them, and there is no button in between — asking
/// when people are free changes nothing.
#[tokio::test]
async fn the_agenda_agent_finds_a_time_across_two_diaries_with_no_button_in_between() {
    let h = harness("agent-a26-find").await;
    let day = the_day();
    let (_ben, bens) = a_colleague(&h, "ben@a26-find.test").await;
    shares_with(&bens, &h.user, "viewer").await;
    a_meeting(&h.acc, "My morning call", at(day, 10, 0), 60).await;
    a_meeting(&bens, "Bens afternoon", at(day, 13, 0), 60).await;
    // A meeting of Ben's the asker is NOT told about, because nobody named the
    // owner of this one: a shared diary is read only for the people asked about.
    let (_, marta) = a_colleague(&h, "marta@a26-find.test").await;
    shares_with(&marta, &h.user, "viewer").await;
    a_meeting(&marta, "Martas all-afternoon workshop", at(day, 11, 0), 300).await;

    const ANSWER: &str =
        "You and Ben are both free 09:00–10:00 and 11:00–13:00, and after 14:00 [1].";
    let (base, seen) = scripted_model(vec![
        wants(
            "find_a_time",
            json!({ "people": ["ben"], "from": day.to_string(), "minutes": 60 }),
            "Let me look at both diaries.",
        ),
        says(ANSWER),
    ])
    .await;
    use_model(&h, &base).await;
    let agent = the_agenda_agent(&h).await;
    let channel = a_room_with(&h, "the meeting", &agent).await;

    const QUESTION: &str = "@agenda when can Ben and I meet for an hour?";
    let spoken = ask_in_room(&h, &channel, QUESTION).await;

    assert_eq!(spoken["body"], json!(ANSWER));
    assert_eq!(
        spoken["proposal"],
        Value::Null,
        "asking when people are free must not put a button in the room"
    );

    // What the model was shown: the slots, computed out of both diaries.
    let second = shown(&seen, 1);
    assert!(second.contains("agendaSlots"), "{second}");
    assert!(second.contains("ben@a26-find.test"), "{second}");
    assert!(
        second.contains(&format!("\"start\":\"{day}T09:00:00Z\"")),
        "{second}"
    );
    // Ben's afternoon is honoured: nothing is offered starting inside it.
    assert!(
        !second.contains(&format!("\"start\":\"{day}T13")),
        "a slot was offered inside Ben's meeting: {second}"
    );
    // …and Marta, whom nobody asked about, blocks nothing even though her
    // diary is readable.
    assert!(
        second.contains(&format!("\"start\":\"{day}T11:00:00Z\"")),
        "an unnamed colleague's meeting must not block a slot: {second}"
    );

    transcript(
        QUESTION,
        &[
            format!("POST /chat/channels/{channel}/messages"),
            format!("     {}", json!({ "body": QUESTION })),
            "--- what the model was shown (call 2 of 2, user turn) ---".to_owned(),
            second,
            "--- the agent's message ---".to_owned(),
            spoken.to_string(),
        ],
    );
}

/// **The rule the whole tool exists for.** A colleague whose diary is not
/// shared is reported by name with the reason, the answer says it is not
/// complete, and their meetings are not in the arithmetic — an unreadable diary
/// is never a free one.
#[tokio::test]
async fn a_diary_that_is_not_shared_is_reported_and_never_counted_free() {
    let h = harness("agent-a26-unshared").await;
    let day = the_day();
    let (_, paula) = a_colleague(&h, "paula@a26-unshared.test").await;
    // Paula's whole afternoon is booked, and she has shared nothing.
    a_meeting(&paula, "Paulas private day", at(day, 9, 0), 480).await;

    let (status, body) = run(
        &h,
        "find_a_time",
        json!({ "people": ["paula"], "from": day.to_string(), "minutes": 60 }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let result = &body["result"];
    assert_eq!(result["kind"], json!("agendaSlots"));
    assert_eq!(
        result["complete"],
        json!(false),
        "an answer that skipped a diary is not an answer about everybody"
    );
    assert_eq!(result["couldNotCheck"][0]["person"], json!("paula"));
    assert_eq!(
        result["couldNotCheck"][0]["reason"],
        json!("no diary of paula's is shared with you")
    );
    // Only the asker is in the arithmetic, and their day is empty…
    assert_eq!(result["people"].as_array().unwrap().len(), 1);
    assert_eq!(result["people"][0]["person"], json!("you"));
    assert_eq!(result["slots"].as_array().unwrap().len(), 1);
    assert_eq!(
        result["slots"][0]["start"],
        json!(format!("{}T09:00:00Z", day)),
        "the whole working day is free for the one person who was read"
    );
    // …and the same sentence comes back for somebody who is in no tenant at
    // all, which is what makes asking useless as a way to find out who exists.
    let (status, body) = run(
        &h,
        "find_a_time",
        json!({ "people": ["nobody-here"], "from": day.to_string() }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["result"]["couldNotCheck"][0]["reason"],
        json!("no diary of nobody-here's is shared with you")
    );
}

/// The window, the length and the days are the caller's to state, and a range
/// that is not a range is refused rather than answered.
#[tokio::test]
async fn the_working_window_and_the_range_are_stated_and_checked() {
    let h = harness("agent-a26-window").await;
    let day = the_day();
    a_meeting(&h.acc, "Morning", at(day, 9, 0), 120).await;

    // A working day the caller widened: the slot before the meeting appears.
    let (status, body) = run(
        &h,
        "find_a_time",
        json!({ "from": day.to_string(), "earliest": "08:00", "latest": "18:00", "minutes": 30 }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let slots = body["result"]["slots"].as_array().unwrap().clone();
    assert_eq!(slots.len(), 2, "{body}");
    assert_eq!(slots[0]["start"], json!(format!("{day}T08:00:00Z")));
    assert_eq!(slots[0]["end"], json!(format!("{day}T08:30:00Z")));
    assert_eq!(
        slots[0]["freeUntil"],
        json!(format!("{day}T09:00:00Z")),
        "how far the gap runs, so a later start can be offered without asking again"
    );
    assert_eq!(slots[1]["start"], json!(format!("{day}T11:00:00Z")));
    assert_eq!(body["result"]["earliest"], json!("08:00"));
    assert_eq!(body["result"]["latest"], json!("18:00"));

    // A meeting nobody has time for is an empty answer, not an offered slot.
    let (status, body) = run(
        &h,
        "find_a_time",
        json!({ "from": day.to_string(), "earliest": "09:00", "latest": "11:00", "minutes": 480 }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["result"]["slots"].as_array().unwrap().is_empty());

    for (args, expected) in [
        (json!({}), "from is required"),
        (json!({ "from": "next Tuesday" }), "from must be YYYY-MM-DD"),
        (
            json!({ "from": day.to_string(), "to": (day - Duration::days(1)).to_string() }),
            "to is before from",
        ),
        (
            json!({ "from": day.to_string(), "to": (day + Duration::days(40)).to_string() }),
            "a range covers at most 31 days",
        ),
        (
            json!({ "from": day.to_string(), "earliest": "17:00", "latest": "09:00" }),
            "latest is not after earliest",
        ),
        (
            json!({ "from": day.to_string(), "earliest": "half nine" }),
            "earliest must be a time like 09:00",
        ),
        (
            json!({ "from": day.to_string(), "people": "ben" }),
            "people must be a list of names",
        ),
    ] {
        let (status, body) = run(&h, "find_a_time", args.clone()).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{args}: {body}");
        assert_eq!(why(&body), expected, "{args}");
    }
}

/// An all-day entry is reported beside the slots, never against them: "Leave"
/// and "Company offsite" look identical in a database and only one of them
/// means busy.
#[tokio::test]
async fn an_all_day_entry_is_reported_and_does_not_block_the_day() {
    let h = harness("agent-a26-allday").await;
    let day = the_day();
    let offsite = an_event(&h.acc, "Company offsite", at(day, 0, 0), 1440, None, true).await;
    h.acc.create_event(&offsite).await.unwrap();

    let (status, body) = run(&h, "find_a_time", json!({ "from": day.to_string() })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["result"]["slots"].as_array().unwrap().len(),
        1,
        "an all-day entry is not a full diary: {body}"
    );
    assert_eq!(
        body["result"]["allDay"][0]["title"],
        json!("Company offsite")
    );
    assert_eq!(body["result"]["allDay"][0]["whose"], json!("you"));
}

// ---- preparing a meeting --------------------------------------------------------

/// **The item's second sentence.** The prep gathers the meeting itself, the
/// mail whose subject matches it, and the text of what was attached to that
/// mail — the Agenda agent is not offered Drive's tools, so it reads the
/// attachment through its own reach or not at all.
#[tokio::test]
async fn a_meeting_is_prepared_from_its_mail_and_what_was_attached_to_it() {
    let h = harness("agent-a26-prep").await;
    let day = the_day();
    a_meeting(&h.acc, "Delaunay review", at(day, 10, 0), 90).await;
    let raw = concat!(
        "From: paula@delaunay.example\r\n",
        "To: me@example.test\r\n",
        "Subject: Delaunay review agenda\r\n",
        "MIME-Version: 1.0\r\n",
        "Content-Type: multipart/mixed; boundary=\"sep\"\r\n",
        "\r\n",
        "--sep\r\n",
        "Content-Type: text/plain; charset=utf-8\r\n",
        "\r\n",
        "Three things to settle: the renewal, the discount, the handover.\r\n",
        "--sep\r\n",
        "Content-Type: text/csv; charset=utf-8\r\n",
        "Content-Disposition: attachment; filename=\"positions.csv\"\r\n",
        "\r\n",
        "item,value\r\nrenewal,142000\r\n",
        "--sep\r\n",
        "Content-Type: application/pdf\r\n",
        "Content-Disposition: attachment; filename=\"board-pack.pdf\"\r\n",
        "Content-Transfer-Encoding: base64\r\n",
        "\r\n",
        "JVBERi0xLjQKJeLjz9MK\r\n",
        "--sep--\r\n",
    );
    h.acc.deliver(raw.as_bytes()).await.unwrap();

    let (status, body) = run(&h, "meeting_prep", json!({ "meeting": "Delaunay review" })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let result = &body["result"];
    assert_eq!(result["kind"], json!("meetingPrep"));
    assert_eq!(result["meeting"]["title"], json!("Delaunay review"));
    assert_eq!(
        result["meeting"]["startsAt"],
        json!(format!("{day}T10:00:00Z"))
    );
    assert_eq!(
        result["meeting"]["endsAt"],
        json!(format!("{day}T11:30:00Z"))
    );
    assert_eq!(result["meeting"]["location"], json!("Room 2"));
    assert_eq!(
        result["meeting"]["notes"],
        json!("Notes for Delaunay review")
    );
    assert_eq!(
        result["meeting"]["guests"],
        json!(["paula@delaunay.example"])
    );
    assert_eq!(result["meeting"]["recurring"], json!(false));

    let mail = result["thread"].as_array().unwrap();
    assert_eq!(mail.len(), 1, "{body}");
    assert_eq!(mail[0]["subject"], json!("Delaunay review agenda"));
    assert_eq!(mail[0]["from"], json!("paula@delaunay.example"));
    assert_eq!(mail[0]["opened"], json!(true));
    assert!(
        mail[0]["preview"]
            .as_str()
            .unwrap()
            .contains("the renewal, the discount, the handover"),
        "{body}"
    );
    // What is attached is named with what it is, and only the text one is read
    // out — a PDF summarised from its filename is the failure this refuses.
    let attached = mail[0]["attachments"].as_array().unwrap();
    assert_eq!(attached.len(), 2);
    assert_eq!(attached[0]["name"], json!("positions.csv"));
    assert_eq!(attached[0]["readable"], json!(true));
    assert_eq!(attached[1]["name"], json!("board-pack.pdf"));
    assert_eq!(attached[1]["readable"], json!(false));
    let texts = mail[0]["attachmentText"].as_array().unwrap();
    assert_eq!(texts.len(), 1, "the PDF is not read: {body}");
    assert_eq!(texts[0]["name"], json!("positions.csv"));
    assert!(
        texts[0]["text"]
            .as_str()
            .unwrap()
            .contains("renewal,142000"),
        "{body}"
    );
}

/// A meeting nobody named, a title nothing matches, and a title the diary holds
/// twice — the last a question that lists the days rather than a guess that
/// prepares the wrong Tuesday.
#[tokio::test]
async fn a_title_that_matches_nothing_or_several_sittings_is_a_refusal_that_says_which() {
    let h = harness("agent-a26-resolve").await;
    let day = the_day();
    a_meeting(&h.acc, "Standup", at(day, 9, 0), 15).await;
    a_meeting(&h.acc, "Standup", at(day + Duration::days(1), 9, 0), 15).await;

    let (status, body) = run(&h, "meeting_prep", json!({})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(why(&body), "say which meeting, by its title");

    let (status, body) = run(&h, "meeting_prep", json!({ "meeting": "Hovercraft" })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(
        why(&body),
        "no meeting of yours in the diary is called Hovercraft"
    );

    let (status, body) = run(&h, "meeting_prep", json!({ "meeting": "Standup" })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    let detail = why(&body);
    assert!(
        detail.contains("is in the diary more than once"),
        "{detail}"
    );
    assert!(detail.contains(&format!("{day} 09:00")), "{detail}");
    assert!(detail.ends_with("say which day"), "{detail}");

    // …and the day settles it.
    let (status, body) = run(
        &h,
        "meeting_prep",
        json!({ "meeting": "Standup", "on": day.to_string() }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["result"]["meeting"]["startsAt"],
        json!(format!("{day}T09:00:00Z"))
    );
}

// ---- moving a meeting -----------------------------------------------------------

/// **The item's third sentence, end to end.** The move is proposed and nothing
/// has happened until the asker taps it; afterwards the meeting is where they
/// said, exactly as long as it was, with everything else about it untouched.
#[tokio::test]
async fn moving_a_meeting_waits_for_a_tap_and_keeps_its_length() {
    let h = harness("agent-a26-move").await;
    let day = the_day();
    let review = a_meeting(&h.acc, "Delaunay review", at(day, 10, 0), 90).await;

    const SAY: &str = "I'll move the Delaunay review to 14:00.";
    let (base, _seen) = scripted_model(vec![wants(
        "reschedule_event",
        json!({ "meeting": "Delaunay review", "start": format!("{day}T14:00:00Z") }),
        SAY,
    )])
    .await;
    use_model(&h, &base).await;
    let agent = the_agenda_agent(&h).await;
    let channel = a_room_with(&h, "the review", &agent).await;

    const QUESTION: &str = "@agenda move the Delaunay review to 2pm";
    let spoken = ask_in_room(&h, &channel, QUESTION).await;
    let proposal = spoken["proposal"]["id"]
        .as_str()
        .expect("a write is proposed, never run")
        .to_owned();
    assert_eq!(spoken["proposal"]["tool"], json!("reschedule_event"));
    assert_eq!(
        now_at(&h.acc, &review).await.0,
        at(day, 10, 0),
        "nothing has moved yet"
    );

    let decided = approve(&h, &proposal).await;
    let result = &decided["result"]["result"];
    assert_eq!(result["kind"], json!("eventMoved"));
    assert_eq!(result["title"], json!("Delaunay review"));
    assert_eq!(result["wasStartsAt"], json!(format!("{day}T10:00:00Z")));
    assert_eq!(result["startsAt"], json!(format!("{day}T14:00:00Z")));
    assert_eq!(
        result["endsAt"],
        json!(format!("{day}T15:30:00Z")),
        "a move with no end keeps the meeting as long as it already was"
    );
    assert_eq!(result["occurrenceOfSeries"], json!(false));

    // The row itself: moved, and nothing else about it changed.
    let after = h.acc.event(&review).await.unwrap().unwrap();
    assert_eq!(after.starts_at, at(day, 14, 0));
    assert_eq!(after.ends_at, at(day, 15, 30));
    assert_eq!(after.summary, "Delaunay review");
    assert_eq!(after.location.as_deref(), Some("Room 2"));
    assert_eq!(
        after.description.as_deref(),
        Some("Notes for Delaunay review")
    );
    assert_eq!(after.attendees, vec!["paula@delaunay.example".to_owned()]);
    assert_eq!(after.reminder_minutes, Some(10));

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

/// One sitting of a repeating meeting moves on its own, and every other sitting
/// stays exactly where it was — the difference between moving a meeting and
/// moving a series.
#[tokio::test]
async fn one_sitting_of_a_repeating_meeting_moves_and_the_series_stays() {
    let h = harness("agent-a26-series").await;
    let day = the_day();
    let series = an_event(
        &h.acc,
        "Standup",
        at(day, 9, 0),
        15,
        Some("FREQ=DAILY;COUNT=4"),
        false,
    )
    .await;
    h.acc.create_event(&series).await.unwrap();

    let (status, body) = run(
        &h,
        "reschedule_event",
        json!({
            "meeting": "Standup",
            "on": (day + Duration::days(1)).to_string(),
            "start": format!("{}T11:00:00Z", day + Duration::days(1)),
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["occurrenceOfSeries"], json!(true));
    assert_eq!(
        body["result"]["endsAt"],
        json!(format!("{}T11:15:00Z", day + Duration::days(1))),
        "an occurrence keeps the series' length"
    );

    // The week, read back: one sitting moved, the rest untouched.
    let week = h
        .acc
        .events_in_range(at(day, 0, 0), at(day + Duration::days(4), 0, 0))
        .await
        .unwrap();
    let starts: Vec<OffsetDateTime> = week.iter().map(|event| event.starts_at).collect();
    assert!(starts.contains(&at(day, 9, 0)), "{starts:?}");
    assert!(
        starts.contains(&at(day + Duration::days(1), 11, 0)),
        "the moved sitting: {starts:?}"
    );
    assert!(
        !starts.contains(&at(day + Duration::days(1), 9, 0)),
        "the sitting was moved, not copied: {starts:?}"
    );
    assert!(
        starts.contains(&at(day + Duration::days(2), 9, 0))
            && starts.contains(&at(day + Duration::days(3), 9, 0)),
        "the rest of the series must not follow it: {starts:?}"
    );
}

/// The refusals a move earns: a start that is not a time, an end that is not
/// after it, an all-day entry that has no time to move, and a diary the caller
/// may read but not change. Each names the meeting rather than failing blankly.
#[tokio::test]
async fn a_move_is_refused_by_name_when_it_cannot_honestly_be_made() {
    let h = harness("agent-a26-refuse").await;
    let day = the_day();
    a_meeting(&h.acc, "Delaunay review", at(day, 10, 0), 90).await;
    let leave = an_event(&h.acc, "Leave", at(day, 0, 0), 1440, None, true).await;
    h.acc.create_event(&leave).await.unwrap();

    for (args, expected) in [
        (
            json!({ "meeting": "Delaunay review" }),
            "start is required".to_owned(),
        ),
        (
            json!({ "meeting": "Delaunay review", "start": "2pm" }),
            "start must be an RFC 3339 datetime".to_owned(),
        ),
        (
            json!({
                "meeting": "Delaunay review",
                "start": format!("{day}T14:00:00Z"),
                "end": format!("{day}T13:00:00Z"),
            }),
            "end is not after start".to_owned(),
        ),
        (
            json!({ "meeting": "Leave", "start": format!("{day}T14:00:00Z") }),
            "Leave is an all-day entry, so it has no time to move — change it in the calendar"
                .to_owned(),
        ),
    ] {
        let (status, body) = run(&h, "reschedule_event", args.clone()).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{args}: {body}");
        assert_eq!(why(&body), expected, "{args}");
    }

    // A colleague's diary the asker can see but not edit: the meeting can be
    // named, and moving it is still refused — and it does not move.
    let (_, bens) = a_colleague(&h, "ben@a26-refuse.test").await;
    shares_with(&bens, &h.user, "viewer").await;
    let theirs = a_meeting(&bens, "Bens one to one", at(day, 15, 0), 30).await;
    let (status, body) = run(
        &h,
        "reschedule_event",
        json!({ "meeting": "Bens one to one", "start": format!("{day}T16:00:00Z") }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(
        why(&body),
        "Bens one to one is in a diary you can read but not change"
    );
    assert_eq!(now_at(&bens, &theirs).await.0, at(day, 15, 0));

    // …and with an editor's grant, the same call moves it.
    shares_with(&bens, &h.user, "editor").await;
    let (status, body) = run(
        &h,
        "reschedule_event",
        json!({ "meeting": "Bens one to one", "start": format!("{day}T16:00:00Z") }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(now_at(&bens, &theirs).await.0, at(day, 16, 0));
}

// ---- isolation ------------------------------------------------------------------

/// **A diary the asker could not open is not one the agent can reach** — across
/// a tenant boundary and across a colleague's private calendar alike, for every
/// tool in the set, reading and writing.
///
/// The refusal is the one an unknown title gets and never admits the meeting
/// exists: an agent that answered differently for a real one would be a way to
/// read somebody else's diary by guessing at it.
#[tokio::test]
async fn a_meeting_of_another_tenant_or_another_person_cannot_be_named() {
    let h = harness("agent-a26-isolation").await;
    let day = the_day();
    let other = common::harness_on(h.store.clone(), "agent-a26-stranger").await;
    a_meeting(&other.acc, "Their board meeting", at(day, 10, 0), 60).await;
    let (_, bens) = a_colleague(&h, "ben@a26-isolation.test").await;
    let bens_own = a_meeting(&bens, "Bens appraisal", at(day, 11, 0), 60).await;

    // Ours, so the refusals below are about reach and not about emptiness.
    a_meeting(&h.acc, "Our own review", at(day, 9, 0), 60).await;

    for stranger in ["Their board meeting", "Bens appraisal"] {
        for (tool, args) in [
            ("meeting_prep", json!({ "meeting": stranger })),
            (
                "reschedule_event",
                json!({ "meeting": stranger, "start": format!("{day}T16:00:00Z") }),
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
                format!("no meeting of yours in the diary is called {stranger}"),
                "{tool}/{stranger}"
            );
        }
    }
    // Nothing moved out from under its owner.
    assert_eq!(now_at(&bens, &bens_own).await.0, at(day, 11, 0));

    // And neither diary can be looked into by naming its owner: the asker of
    // another tenant is not even a name here.
    let (status, body) = run(
        &h,
        "find_a_time",
        json!({ "people": ["ben", &other.email], "from": day.to_string() }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let could_not = body["result"]["couldNotCheck"].as_array().unwrap();
    assert_eq!(could_not.len(), 2, "{body}");
    assert_eq!(body["result"]["complete"], json!(false));
    assert_eq!(
        body["result"]["people"].as_array().unwrap().len(),
        1,
        "only the asker's own diary was read: {body}"
    );
    // The asker's own day is what it is — the strangers' meetings are not in it.
    let slots = body["result"]["slots"].as_array().unwrap();
    assert_eq!(slots[0]["start"], json!(format!("{day}T10:00:00Z")));
}
