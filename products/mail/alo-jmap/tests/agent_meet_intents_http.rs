//! The Meet agent over its intents (AC.2, ADR 0058), on the wire: in a real
//! room, against the real router and store, with a scripted model.
//!
//! The after-the-fact half (record reads, minutes) is proven end to end in
//! `agent_meet_http`; this suite holds the half AC.2 adds — the *before*. A
//! question about the meetings ahead is answered from the asker's own diary
//! inside the turn, with no button in between; one meeting is looked up with
//! the notes on its invitation; scheduling one is only ever a previewed
//! proposal that runs the Agenda module's own calendar write, as the asker,
//! once they approve. And another tenant's diary does not exist for this
//! agent: a lookup into it earns the words an invented title earns, and not
//! one word of the other tenant's notes reaches this tenant's model.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::{Duration as Wait, Instant};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use time::format_description::well_known::Rfc3339;
use time::{Duration, OffsetDateTime};

use alo_store::{AccountStore, AgentProduct, CalendarEvent, EventId};

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

async fn meet_agent(h: &Harness) -> String {
    let handle = alo_store::default_handle(AgentProduct::Meet);
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

/// A meeting ahead in somebody's diary, on their personal calendar, with the
/// notes and the place an invitation carries.
async fn in_the_diary(
    acc: &AccountStore,
    title: &str,
    starts_at: OffsetDateTime,
    notes: &str,
) -> EventId {
    let calendar = acc.ensure_personal_calendar().await.unwrap();
    let event = CalendarEvent {
        id: EventId::generate(),
        calendar_id: calendar,
        summary: title.to_owned(),
        description: Some(notes.to_owned()),
        location: Some("Room 2".to_owned()),
        starts_at,
        ends_at: starts_at + Duration::hours(1),
        all_day: false,
        recurrence: None,
        attendees: Vec::new(),
        exdates: Vec::new(),
        timezone: None,
        rdates: Vec::new(),
        recurrence_id: None,
        reminder_minutes: None,
        attendee_status: Vec::new(),
    };
    acc.create_event(&event).await.unwrap()
}

/// The asker's diary over the next week, by title.
async fn diary_titles(h: &Harness) -> Vec<String> {
    let now = OffsetDateTime::now_utc();
    h.acc
        .events_in_range(now, now + Duration::days(7))
        .await
        .unwrap()
        .into_iter()
        .map(|event| event.summary)
        .collect()
}

#[tokio::test]
async fn what_is_coming_up_is_answered_from_the_diary() {
    let h = harness("meet-intents-upcoming").await;
    let two_days = OffsetDateTime::now_utc() + Duration::days(2);
    in_the_diary(&h.acc, "Q3 budget review", two_days, "Bring the figures").await;
    in_the_diary(
        &h.acc,
        "Design kickoff",
        two_days + Duration::hours(3),
        "Sketches ready",
    )
    .await;

    let agent = meet_agent(&h).await;
    let room = a_room_with(&h, "ask", &agent).await;
    let (model, seen) = scripted_model(vec![
        wants("upcoming_meetings", json!({}), "Let me look at your diary."),
        says("You have the Q3 budget review and the Design kickoff coming up [1]."),
    ])
    .await;
    use_model(&h, &model).await;

    let answer = ask_in_room(&h, &room, "@meet what meetings do I have coming up?").await;
    assert_eq!(
        answer["body"],
        "You have the Q3 budget review and the Design kickoff coming up [1]."
    );
    assert!(
        answer["proposal"].is_null(),
        "a read is answered, never proposed"
    );

    // The agent was offered its verbs — the kept record reads and the new
    // diary pair, reads and writes alike, from the intent registry.
    let prompt = offered(&seen, 0);
    for verb in [
        "meetings_recent",
        "meeting_record",
        "upcoming_meetings",
        "meeting_lookup",
        "meeting_minutes",
        "schedule_meeting",
    ] {
        assert!(
            prompt.contains(&format!("- {verb}:")),
            "the prompt does not offer {verb}"
        );
    }
    let sources = shown(&seen, 1);
    assert!(sources.contains("Q3 budget review"), "{sources}");
    assert!(sources.contains("Design kickoff"), "{sources}");
}

#[tokio::test]
async fn one_meeting_ahead_is_looked_up_with_its_notes() {
    let h = harness("meet-intents-lookup").await;
    let three_days = OffsetDateTime::now_utc() + Duration::days(3);
    in_the_diary(&h.acc, "Design kickoff", three_days, "Bring the Q3 figures").await;

    let agent = meet_agent(&h).await;
    let room = a_room_with(&h, "ask", &agent).await;
    let (model, seen) = scripted_model(vec![
        wants(
            "meeting_lookup",
            json!({ "meeting": "design kickoff" }),
            "Let me look it up.",
        ),
        says("The Design kickoff is in three days, in Room 2 [1]."),
    ])
    .await;
    use_model(&h, &model).await;

    let answer = ask_in_room(&h, &room, "@meet when is the design kickoff?").await;
    assert_eq!(
        answer["body"],
        "The Design kickoff is in three days, in Room 2 [1]."
    );
    assert!(answer["proposal"].is_null());
    // The invitation's own record came back: the notes, the place, and the
    // plain fact that no sitting of it has run yet.
    let sources = shown(&seen, 1);
    assert!(sources.contains("Bring the Q3 figures"), "{sources}");
    assert!(sources.contains("Room 2"), "{sources}");
    assert!(sources.contains("\"record\":null"), "{sources}");
}

#[tokio::test]
async fn scheduling_a_meeting_is_proposed_and_lands_only_on_approval() {
    let h = harness("meet-intents-schedule").await;
    let agent = meet_agent(&h).await;
    let room = a_room_with(&h, "ask", &agent).await;

    let start = (OffsetDateTime::now_utc() + Duration::days(1))
        .replace_nanosecond(0)
        .unwrap();
    let start_s = start.format(&Rfc3339).unwrap();
    let (model, _seen) = scripted_model(vec![wants(
        "schedule_meeting",
        json!({ "title": "Budget review", "start": start_s }),
        "I'll put a budget review in your diary.",
    )])
    .await;
    use_model(&h, &model).await;

    let answer = ask_in_room(&h, &room, "@meet schedule a budget review for tomorrow").await;
    assert!(
        !answer["proposal"].is_null(),
        "a write is a proposal: {answer}"
    );
    assert_eq!(answer["proposal"]["tool"], "schedule_meeting");
    // Nothing ran without a tap: the diary has no such meeting.
    assert!(
        !diary_titles(&h).await.contains(&"Budget review".to_owned()),
        "scheduled early"
    );

    // The asker approves — and the entry is in THEIR own diary, made by the
    // Agenda module's shared calendar write.
    let proposal = answer["proposal"]["id"].as_str().unwrap().to_owned();
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/chat/proposals/{proposal}"),
        json!({ "approve": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let now = OffsetDateTime::now_utc();
    let booked = h
        .acc
        .events_in_range(now, now + Duration::days(7))
        .await
        .unwrap()
        .into_iter()
        .find(|event| event.summary == "Budget review")
        .expect("the approved meeting is in the diary");
    assert_eq!(booked.starts_at, start);
}

#[tokio::test]
async fn another_tenants_diary_meeting_cannot_be_looked_up() {
    let h = harness("meet-intents-iso").await;
    // Another tenant on the same store, with a meeting ahead and a secret in
    // its notes.
    let other = harness_on(Arc::clone(&h.store), "meet-intents-iso2").await;
    let ahead = OffsetDateTime::now_utc() + Duration::days(2);
    in_the_diary(&other.acc, "warroom sync", ahead, "the secret plan").await;

    let agent = meet_agent(&h).await;
    let room = a_room_with(&h, "ask", &agent).await;
    let (model, seen) = scripted_model(vec![
        wants(
            "meeting_lookup",
            json!({ "meeting": "warroom sync" }),
            "Let me look it up.",
        ),
        says("You have no meeting called warroom sync in your diary."),
    ])
    .await;
    use_model(&h, &model).await;

    let answer = ask_in_room(&h, &room, "@meet when is the warroom sync?").await;
    assert_eq!(
        answer["body"],
        "You have no meeting called warroom sync in your diary."
    );
    // The other tenant's meeting earns the words an invented title earns —
    // indistinguishable on purpose — and not a word of their notes reaches
    // this tenant's model.
    let sources = shown(&seen, 1);
    assert!(
        sources.contains("no meeting of yours in the diary is called warroom sync"),
        "{sources}"
    );
    assert!(
        !sources.contains("the secret plan"),
        "another tenant's notes leaked into the sources: {sources}"
    );
}
