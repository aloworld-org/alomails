//! The Agenda agent over its intents (ADR 0058, queue item AB.5), on the
//! wire: in a real room, against the real router and store, with a scripted
//! model.
//!
//! What AB.5 adds is one meeting in full (`event_lookup`), one shared diary's
//! span of time (`colleague_free`), and the two writes a calendar agent must
//! never run on a hunch — `cancel_event` and `respond_to_invitation`, both
//! previewed and waiting for a tap. This suite holds the wave's three
//! sentences: a read runs inside the turn and the calendar's own record view
//! reaches the model as a source; a meeting of another tenant's, or in a
//! colleague's unshared diary, is not among the things that can be named; a
//! write is proposed and not run. The deep behaviour of the six kept tools —
//! the slots, the briefing, the length-preserving move — stays proven by
//! `agent_agenda_http.rs`, which runs the same executors.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::{Duration as Wait, Instant};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use time::{Date, Duration, OffsetDateTime};

use alo_store::{AccountStore, AgentProduct, CalendarEvent, EventId, UserId};

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

// ---- the diaries under test --------------------------------------------------

/// The one day the tests are about: stated rather than "today", and inside the
/// window `resolve_meeting` looks in.
fn the_day() -> Date {
    OffsetDateTime::now_utc().date() + Duration::days(3)
}

fn at(day: Date, hour: u8, minute: u8) -> OffsetDateTime {
    day.with_hms(hour, minute, 0).unwrap().assume_utc()
}

/// A meeting in somebody's diary, on their personal calendar, with one guest.
async fn a_meeting(
    acc: &AccountStore,
    title: &str,
    start: OffsetDateTime,
    minutes: i64,
    rrule: Option<&str>,
) -> EventId {
    let calendar = acc.ensure_personal_calendar().await.unwrap();
    let event = CalendarEvent {
        id: EventId::generate(),
        calendar_id: calendar,
        summary: title.to_owned(),
        description: Some(format!("Notes for {title}")),
        location: Some("Room 2".to_owned()),
        starts_at: start,
        ends_at: start + Duration::minutes(minutes),
        all_day: false,
        recurrence: rrule.map(str::to_owned),
        attendees: vec!["paula@delaunay.example".to_owned()],
        exdates: Vec::new(),
        timezone: None,
        rdates: Vec::new(),
        recurrence_id: None,
        reminder_minutes: Some(10),
        attendee_status: Vec::new(),
    };
    acc.create_event(&event).await.unwrap()
}

/// Paula's RSVP recorded on the organizer's copy, through the same store path
/// an inbound iMIP `REPLY` takes — `create_event` deliberately starts the
/// status map empty, replies are the only thing that fills it.
async fn paula_accepted(acc: &AccountStore, id: &EventId) {
    assert!(
        acc.set_attendee_status(id, "paula@delaunay.example", "ACCEPTED")
            .await
            .unwrap()
    );
}

/// A colleague in the same tenant, with a diary of their own.
async fn a_colleague(h: &Harness, address: &str) -> (UserId, AccountStore) {
    let user = h.ts.create_user(address).await.unwrap();
    let acc = h.store.for_account(h.tenant.clone(), user.clone());
    (user, acc)
}

/// Their diary, shared with the asker as a viewer.
async fn shares_with(theirs: &AccountStore, me: &UserId) {
    let calendar = theirs.ensure_personal_calendar().await.unwrap();
    theirs
        .grant_calendar(&calendar, "user", me.as_str(), "viewer")
        .await
        .unwrap();
}

/// An invitation email — a `text/calendar; method=REQUEST` part — delivered
/// into the caller's own mailbox, the way one arrives from an organizer.
async fn an_invitation_arrives(acc: &AccountStore, title: &str, uid: &str, day: Date) {
    let dt = |hour: u8| {
        format!(
            "{:04}{:02}{:02}T{:02}0000Z",
            day.year(),
            u8::from(day.month()),
            day.day(),
            hour
        )
    };
    let raw = format!(
        "From: paula@delaunay.example\r\n\
         To: me@example.test\r\n\
         Subject: Invitation: {title}\r\n\
         MIME-Version: 1.0\r\n\
         Content-Type: multipart/alternative; boundary=\"sep\"\r\n\
         \r\n\
         --sep\r\n\
         Content-Type: text/plain; charset=utf-8\r\n\
         \r\n\
         You're invited to {title}.\r\n\
         --sep\r\n\
         Content-Type: text/calendar; charset=utf-8; method=REQUEST\r\n\
         \r\n\
         BEGIN:VCALENDAR\r\n\
         VERSION:2.0\r\n\
         METHOD:REQUEST\r\n\
         BEGIN:VEVENT\r\n\
         UID:{uid}\r\n\
         DTSTART:{}\r\n\
         DTEND:{}\r\n\
         SUMMARY:{title}\r\n\
         ORGANIZER:mailto:paula@delaunay.example\r\n\
         ATTENDEE:mailto:me@example.test\r\n\
         END:VEVENT\r\n\
         END:VCALENDAR\r\n\
         --sep--\r\n",
        dt(14),
        dt(15),
    );
    acc.deliver(raw.as_bytes()).await.unwrap();
}

// ---- the read: answered from the record, inside the tenant -------------------

/// **AB.5's headline sentence**: "@agenda when is the Board review?" is
/// answered from the record — the calendar's own record view reaches the model
/// as a source, and neither another tenant's diary nor a colleague's unshared
/// one is among the things that can be named.
#[tokio::test]
async fn a_meetings_details_are_answered_from_the_record() {
    let h = harness("agenda-intents-lookup").await;
    let day = the_day();
    let review = a_meeting(&h.acc, "Board review", at(day, 10, 0), 90, None).await;
    paula_accepted(&h.acc, &review).await;
    // A stranger's diary and a colleague's unshared one, so the assertions
    // below are about reach and not about emptiness.
    let other = common::harness_on(h.store.clone(), "agenda-intents-stranger").await;
    a_meeting(
        &other.acc,
        "Their secret ceremony",
        at(day, 10, 0),
        60,
        None,
    )
    .await;
    let (_ben, bens) = a_colleague(&h, "ben@agenda-intents-lookup.test").await;
    a_meeting(&bens, "Bens private appraisal", at(day, 10, 0), 60, None).await;

    let agent = the_agenda_agent(&h).await;
    let room = a_room_with(&h, "diary", &agent).await;
    let (model, seen) = scripted_model(vec![
        wants(
            "event_lookup",
            json!({ "meeting": "Board review" }),
            "Let me open the diary.",
        ),
        says("The Board review is in Room 2, and Paula has accepted [1]."),
    ])
    .await;
    use_model(&h, &model).await;

    let answer = ask_in_room(&h, &room, "@agenda when is the Board review?").await;
    assert_eq!(
        answer["body"],
        "The Board review is in Room 2, and Paula has accepted [1]."
    );
    assert!(
        answer["proposal"].is_null(),
        "a read is answered, never proposed"
    );

    // The agent was offered its verbs — reads and writes alike, rendered from
    // the intent registry — and no other product's.
    let prompt = offered(&seen, 0);
    for verb in [
        "whats_on",
        "am_i_free",
        "find_a_time",
        "meeting_prep",
        "event_lookup",
        "colleague_free",
        "create_event",
        "reschedule_event",
        "cancel_event",
        "respond_to_invitation",
    ] {
        assert!(
            prompt.contains(&format!("- {verb}:")),
            "the prompt does not offer {verb}"
        );
    }
    assert!(
        !prompt.contains("- file_read:") && !prompt.contains("- schedule_meeting:"),
        "another product's tools reached the Agenda agent"
    );
    // The record view came back as a source: the meeting in the calendar's own
    // vocabulary, nobody else's diary beside it.
    let sources = shown(&seen, 1);
    assert!(sources.contains("eventLookup"), "{sources}");
    assert!(sources.contains("Board review"), "{sources}");
    assert!(sources.contains("Room 2"), "{sources}");
    assert!(sources.contains("ACCEPTED"), "{sources}");
    assert!(
        !sources.contains("Their secret ceremony"),
        "another tenant's diary reached the model: {sources}"
    );
    assert!(
        !sources.contains("Bens private appraisal"),
        "an unshared diary reached the model: {sources}"
    );
}

// ---- the write: proposed, previewed, not run ---------------------------------

#[tokio::test]
async fn cancelling_a_meeting_is_proposed_and_not_run() {
    let h = harness("agenda-intents-write").await;
    let day = the_day();
    let id = a_meeting(&h.acc, "Vendor demo", at(day, 10, 0), 60, None).await;
    let agent = the_agenda_agent(&h).await;
    let room = a_room_with(&h, "diary", &agent).await;
    let (model, _seen) = scripted_model(vec![wants(
        "cancel_event",
        json!({ "meeting": "Vendor demo" }),
        "I'll cancel the demo.",
    )])
    .await;
    use_model(&h, &model).await;

    let answer = ask_in_room(&h, &room, "@agenda cancel the vendor demo").await;
    assert!(
        !answer["proposal"].is_null(),
        "a write is a proposal: {answer}"
    );
    assert_eq!(answer["proposal"]["tool"], "cancel_event");
    // Nothing ran without a tap: the meeting is still in the diary.
    assert!(
        h.acc.event(&id).await.unwrap().is_some(),
        "the meeting was cancelled before the tap"
    );
}

// ---- arguments and refusals, over the approval route -------------------------

/// The two new reads against the real diaries: the lookup serves the route's
/// own record and treats two sittings as a question; the colleague check reads
/// a shared diary and refuses an unshared one with a sentence that says
/// nothing about who exists.
#[tokio::test]
async fn the_new_reads_run_against_the_real_diaries() {
    let h = harness("agenda-intents-reads").await;
    let day = the_day();
    let review = a_meeting(&h.acc, "Board review", at(day, 10, 0), 90, None).await;
    paula_accepted(&h.acc, &review).await;
    a_meeting(&h.acc, "Standup", at(day, 9, 0), 15, None).await;
    a_meeting(
        &h.acc,
        "Standup",
        at(day + Duration::days(1), 9, 0),
        15,
        None,
    )
    .await;

    // The lookup: the record `GET /calendar/events/{id}` serves, in full.
    let (status, body) = run(&h, "event_lookup", json!({ "meeting": "Board review" })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["kind"], "eventLookup");
    let record = &body["result"]["record"];
    assert_eq!(record["summary"], "Board review");
    assert_eq!(record["location"], "Room 2");
    assert_eq!(record["attendees"], json!(["paula@delaunay.example"]));
    assert_eq!(record["reminderMinutes"], 10);
    assert_eq!(
        record["attendeeStatus"],
        json!([{ "email": "paula@delaunay.example", "status": "ACCEPTED" }])
    );
    // Two sittings are a question that lists the days, never a guess.
    let (status, body) = run(&h, "event_lookup", json!({ "meeting": "Standup" })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(why(&body).contains("more than once"), "{body}");
    // …and the day settles it.
    let (status, body) = run(
        &h,
        "event_lookup",
        json!({ "meeting": "Standup", "on": day.to_string() }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // The colleague check: Ben's diary is shared, so his afternoon answers —
    // busy names the clash, free says free.
    let (ben, bens) = a_colleague(&h, "ben@agenda-intents-reads.test").await;
    let _ = ben;
    shares_with(&bens, &h.user).await;
    a_meeting(&bens, "Bens afternoon call", at(day, 13, 0), 60, None).await;
    let (status, body) = run(
        &h,
        "colleague_free",
        json!({ "person": "ben", "start": format!("{day}T13:00:00Z") }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["kind"], "colleagueFree");
    assert_eq!(body["result"]["free"], false);
    assert_eq!(
        body["result"]["clashes"][0]["title"], "Bens afternoon call",
        "{body}"
    );
    let (status, body) = run(
        &h,
        "colleague_free",
        json!({ "person": "ben", "start": format!("{day}T09:00:00Z") }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["free"], true, "{body}");
    // Marta exists and has a diary — but it was never shared, so the refusal
    // is the same sentence somebody who does not exist gets.
    let (_marta, martas) = a_colleague(&h, "marta@agenda-intents-reads.test").await;
    a_meeting(&martas, "Martas workshop", at(day, 9, 0), 300, None).await;
    let (status, body) = run(
        &h,
        "colleague_free",
        json!({ "person": "marta", "start": format!("{day}T09:00:00Z") }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(why(&body), "no diary of marta's is shared with you");
    let (status, body) = run(
        &h,
        "colleague_free",
        json!({ "person": "nobody-at-all", "start": format!("{day}T09:00:00Z") }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(why(&body), "no diary of nobody-at-all's is shared with you");
}

/// The two new writes against the real diary, over the approval route: a
/// cancel removes a one-off, skips one sitting of a series on its own, and
/// refuses a diary the caller can only read; an invitation is answered
/// through the calendar's own RSVP — accepted lands the meeting, declined
/// leaves the diary alone, and an answer nobody gave is refused.
#[tokio::test]
async fn cancelling_and_answering_run_the_calendars_own_paths() {
    let h = harness("agenda-intents-writes").await;
    let day = the_day();

    // A one-off: cancelled whole, and gone from the diary.
    let demo = a_meeting(&h.acc, "Vendor demo", at(day, 10, 0), 60, None).await;
    let (status, body) = run(&h, "cancel_event", json!({ "meeting": "Vendor demo" })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["kind"], "eventCancelled");
    assert_eq!(body["result"]["scope"], "series");
    assert_eq!(
        body["result"]["guestsTold"],
        json!(["paula@delaunay.example"])
    );
    assert!(h.acc.event(&demo).await.unwrap().is_none(), "still there");

    // One sitting of a weekly series: that day is skipped, the series stays.
    a_meeting(&h.acc, "Standup", at(day, 9, 0), 15, Some("FREQ=WEEKLY")).await;
    let (status, body) = run(
        &h,
        "cancel_event",
        json!({ "meeting": "Standup", "on": day.to_string() }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["scope"], "occurrence", "{body}");
    let that_day = h
        .acc
        .events_in_range(at(day, 0, 0), at(day, 23, 59))
        .await
        .unwrap();
    assert!(
        that_day.iter().all(|event| event.summary != "Standup"),
        "the cancelled sitting is still on its day"
    );
    let next_week = h
        .acc
        .events_in_range(
            at(day + Duration::days(7), 0, 0),
            at(day + Duration::days(7), 23, 59),
        )
        .await
        .unwrap();
    assert!(
        next_week.iter().any(|event| event.summary == "Standup"),
        "cancelling one sitting took the series with it"
    );

    // A diary the caller can read but not change is a refusal by name.
    let (_ben, bens) = a_colleague(&h, "ben@agenda-intents-writes.test").await;
    shares_with(&bens, &h.user).await;
    a_meeting(&bens, "Bens own call", at(day, 13, 0), 60, None).await;
    let (status, body) = run(&h, "cancel_event", json!({ "meeting": "Bens own call" })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(why(&body).contains("read but not change"), "{body}");

    // The invitation: accepted lands the meeting in the diary through the
    // calendar's own RSVP (upsert by the organizer's UID).
    an_invitation_arrives(&h.acc, "Sales kickoff", "kickoff-1@delaunay.example", day).await;
    let (status, body) = run(
        &h,
        "respond_to_invitation",
        json!({ "meeting": "Sales kickoff", "response": "accepted" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["kind"], "invitationAnswered");
    assert_eq!(body["result"]["response"], "accepted");
    assert_eq!(body["result"]["added"], true);
    let booked = h
        .acc
        .event(&EventId::new("kickoff-1@delaunay.example"))
        .await
        .unwrap();
    assert!(booked.is_some(), "accepting did not land the meeting");
    assert_eq!(booked.unwrap().summary, "Sales kickoff");

    // Declined sends the answer and leaves the diary alone.
    an_invitation_arrives(
        &h.acc,
        "Budget teardown",
        "teardown-9@delaunay.example",
        day,
    )
    .await;
    let (status, body) = run(
        &h,
        "respond_to_invitation",
        json!({ "meeting": "Budget teardown", "response": "declined" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["added"], false);
    assert!(
        h.acc
            .event(&EventId::new("teardown-9@delaunay.example"))
            .await
            .unwrap()
            .is_none(),
        "declining put the meeting in the diary anyway"
    );

    // An answer nobody gave, and an invitation nobody received, are refusals.
    let (status, body) = run(
        &h,
        "respond_to_invitation",
        json!({ "meeting": "Sales kickoff", "response": "maybe later" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        why(&body).contains("accepted, declined or tentative"),
        "{body}"
    );
    let (status, body) = run(
        &h,
        "respond_to_invitation",
        json!({ "meeting": "Hovercraft summit", "response": "accepted" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        why(&body).contains("no invitation to Hovercraft summit"),
        "{body}"
    );
}
