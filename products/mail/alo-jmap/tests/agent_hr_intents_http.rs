//! The People agent over its intents (AA.5, ADR 0058), on the wire: in a
//! real room, against the real router and store, with a scripted model.
//!
//! Before the move to intents the People agent could say who was away and
//! fill in a letter, but "@hr who works here?" had no verb to run. This
//! suite holds the opposite: the read runs inside the turn and answers from
//! the directory — the same public projection `GET /hr/org` folds its chart
//! from — a write (approving a colleague's leave) is proposed, previewed and
//! not run, and another tenant's people are unreachable from the directory.

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

async fn get(app: &Router, token: &str, uri: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    send(app, req).await
}

async fn people_agent(h: &Harness) -> String {
    let handle = alo_store::default_handle(AgentProduct::Hr);
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

/// One employee on a team, returning their id.
async fn an_employee(h: &Harness, given: &str, family: &str, team: &str) -> String {
    let (status, body) = post(
        &h.app,
        &h.token,
        "/hr/employees",
        json!({
            "givenName": given,
            "familyName": family,
            "employment": {
                "jobTitle": "Joiner",
                "team": team,
                "startedOn": "2026-01-05",
            },
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["employee"]["id"].as_str().unwrap().to_owned()
}

/// A leave policy that needs approving, returning its id.
async fn a_policy(h: &Harness) -> String {
    let (status, body) = post(
        &h.app,
        &h.token,
        "/hr/leave-policies",
        json!({
            "name": "Holiday",
            "kind": "annual",
            "entitlementMinutes": 60_000,
            "requiresApproval": true,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["policy"]["id"].as_str().unwrap().to_owned()
}

/// A leave request waiting for a decision, filed by HR for the employee,
/// returning its id.
async fn a_waiting_request(h: &Harness, employee: &str, policy: &str, note: &str) -> String {
    let (status, body) = post(
        &h.app,
        &h.token,
        "/hr/leave-requests",
        json!({
            "employeeId": employee,
            "policyId": policy,
            "fromDay": "2026-10-05",
            "toDay": "2026-10-09",
            "note": note,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["request"]["status"], "requested", "{body}");
    body["request"]["id"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn who_works_here_is_answered_from_the_directory() {
    let h = harness("hr-intents-dir").await;
    // The seeding goes through the HR door, which the harness user holds as
    // the tenant's admin.
    h.ts.set_admin(&h.user, true).await.unwrap();
    an_employee(&h, "Amara", "van den Berg", "Workshop").await;
    an_employee(&h, "Mikkel", "Sørensen", "Workshop").await;

    let agent = people_agent(&h).await;
    let room = a_room_with(&h, "front-office", &agent).await;
    let (model, seen) = scripted_model(vec![
        wants("who_works_here", json!({}), "Let me read the directory."),
        says("Two colleagues: Amara and Mikkel, both in the Workshop [1]."),
    ])
    .await;
    use_model(&h, &model).await;

    let answer = ask_in_room(&h, &room, "@hr who works here?").await;
    assert_eq!(
        answer["body"],
        "Two colleagues: Amara and Mikkel, both in the Workshop [1]."
    );
    assert!(
        answer["proposal"].is_null(),
        "a read is answered, never proposed"
    );

    // The agent was offered its verbs — reads and writes alike, from the
    // intent registry.
    let prompt = offered(&seen, 0);
    for verb in [
        "who_is_off",
        "who_works_here",
        "my_leave_balance",
        "open_leave_requests",
        "open_checklists",
        "approve_leave_request",
        "draft_letter_from_template",
    ] {
        assert!(
            prompt.contains(&format!("- {verb}:")),
            "the prompt does not offer {verb}"
        );
    }
    // The read's rows are the directory's own public projection: name, job
    // title and team — and nothing the record keeps private.
    let sources = shown(&seen, 1);
    assert!(sources.contains("Amara van den Berg"), "{sources}");
    assert!(sources.contains("Mikkel Sørensen"), "{sources}");
    assert!(sources.contains("\"team\":\"Workshop\""), "{sources}");
    assert!(sources.contains("\"jobTitle\":\"Joiner\""), "{sources}");
    assert!(sources.contains("\"peopleCount\":2"), "{sources}");
}

#[tokio::test]
async fn approving_leave_is_proposed_and_not_run_and_the_note_stays_in_the_app() {
    let h = harness("hr-intents-approve").await;
    h.ts.set_admin(&h.user, true).await.unwrap();
    let employee = an_employee(&h, "Amara", "van den Berg", "Workshop").await;
    let policy = a_policy(&h).await;
    let request = a_waiting_request(
        &h,
        &employee,
        &policy,
        "hospital appointment on the Tuesday",
    )
    .await;
    let agent = people_agent(&h).await;
    let room = a_room_with(&h, "front-office", &agent).await;

    // The agent reads what is waiting, then proposes the approval: two model
    // calls, and the second's sources are what the note must never reach.
    let (model, seen) = scripted_model(vec![
        wants(
            "open_leave_requests",
            json!({}),
            "Let me see what is waiting.",
        ),
        wants(
            "approve_leave_request",
            json!({ "employee": "Amara" }),
            "I'll approve Amara's leave.",
        ),
    ])
    .await;
    use_model(&h, &model).await;

    let answer = ask_in_room(&h, &room, "@hr approve Amara's leave request").await;
    assert!(
        !answer["proposal"].is_null(),
        "a write is a proposal: {answer}"
    );
    assert_eq!(answer["proposal"]["tool"], "approve_leave_request");
    // What the model was shown of the queue is who, which days, at what cost
    // — and never the sentence Amara wrote under it.
    let sources = shown(&seen, 1);
    assert!(sources.contains("Amara van den Berg"), "{sources}");
    assert!(sources.contains("\"requestCount\":1"), "{sources}");
    assert!(!sources.contains("hospital"), "{sources}");
    // Nothing ran without a tap: the request is still waiting, undecided.
    let (status, body) = get(&h.app, &h.token, &format!("/hr/leave-requests/{request}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["request"]["status"], "requested", "{body}");
    assert!(body["request"]["decidedBy"].is_null(), "{body}");
}

#[tokio::test]
async fn another_tenants_people_are_unreachable() {
    let h = harness("hr-intents-iso-a").await;
    let other = harness_on(h.store.clone(), "hr-intents-iso-b").await;
    other.ts.set_admin(&other.user, true).await.unwrap();
    // Tenant B's employee is a person's name on a staff record — exactly the
    // kind of word that must never cross a tenant wall.
    an_employee(&other, "Greta", "Nachtigall", "Kontor").await;
    let agent = people_agent(&h).await;
    let room = a_room_with(&h, "front-office", &agent).await;

    let (model, seen) = scripted_model(vec![
        wants("who_works_here", json!({}), "Let me read the directory."),
        says("The directory lists nobody yet."),
    ])
    .await;
    use_model(&h, &model).await;

    let answer = ask_in_room(&h, &room, "@hr who works here?").await;
    assert_eq!(answer["body"], "The directory lists nobody yet.");
    // What the model was shown is tenant A's own (empty) directory and none
    // of tenant B's people — not the name, not the team.
    let sources = shown(&seen, 1);
    assert!(sources.contains("\"peopleCount\":0"), "{sources}");
    assert!(!sources.contains("Nachtigall"), "{sources}");
    assert!(!sources.contains("Kontor"), "{sources}");
}
