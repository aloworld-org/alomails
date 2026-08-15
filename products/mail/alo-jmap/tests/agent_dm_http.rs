//! A one-to-one with an agent, on the wire (ADR 0048, A1.4).
//!
//! The property this suite exists for cannot be seen from the store: **no
//! handle is typed**. In a channel an agent answers because it was named; here
//! the room itself is the address, so the test says an ordinary sentence with no
//! `@` in it and the agent answers anyway — and the same sentence in a channel
//! is answered by nobody, which is what proves the trigger came from the room.
//!
//! No live model is called: the tenant's AI backend is the scripted local socket
//! in `common::model`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use alo_store::AgentProduct;
use common::model::{says, scripted_model, use_model};
use common::{Harness, harness, send};

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

/// Says something in a room and waits for an agent to reply, or gives up.
///
/// The turn is spawned off the request on purpose, so a reply has to be waited
/// for; `None` means nobody spoke within the window, which is the assertion a
/// negative test needs rather than a flake.
async fn agent_reply(
    h: &Harness,
    channel: &str,
    question: &str,
    within: Duration,
) -> Option<Value> {
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/chat/channels/{channel}/messages"),
        json!({ "body": question }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let deadline = Instant::now() + within;
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
        if spoken.is_some() {
            return spoken;
        }
        if Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Opening one is idempotent, it comes back as its own kind naming its agent,
/// and it is listed beside the rest of the caller's rooms.
#[tokio::test]
async fn opening_a_one_to_one_with_an_agent_twice_is_one_room() {
    let h = harness("agentdmopen").await;
    let agent = h
        .acc
        .create_agent("mail", "Mail", None, AgentProduct::Mail)
        .await
        .unwrap();

    let (status, room) = post(
        &h.app,
        &h.token,
        &format!("/chat/agents/{}/dm", agent.as_str()),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{room}");
    assert_eq!(room["kind"], json!("agent_dm"));
    assert_eq!(room["agent"], json!(agent.as_str()));
    assert_eq!(room["name"], Value::Null, "a one-to-one has no name");
    assert_eq!(room["visibility"], json!("private"));

    let (status, again) = post(
        &h.app,
        &h.token,
        &format!("/chat/agents/{}/dm", agent.as_str()),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{again}");
    assert_eq!(
        again["id"], room["id"],
        "the same conversation, not a second"
    );

    // Beside the human DMs in the caller's own list, labelled by who it is
    // with. Discovery never shows it: browsing is channels only.
    let (status, listed) = get(&h.app, &h.token, "/chat/channels").await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    let mine = listed["channels"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == room["id"])
        .expect("an agent DM is in the room list");
    assert_eq!(mine["counterpart"], json!("@mail"));
    let (status, browsable) = get(&h.app, &h.token, "/chat/channels/joinable").await;
    assert_eq!(status, StatusCode::OK, "{browsable}");
    assert!(
        browsable["channels"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["id"] != room["id"]),
        "a one-to-one is not a room to be browsed: {browsable}"
    );

    // A tenant that has no such agent has no room to open with it.
    let (status, body) = post(&h.app, &h.token, "/chat/agents/nope/dm", json!({})).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
}

/// The sentence with no handle in it. In the one-to-one the agent answers; in a
/// channel the identical words reach nobody — the room is the address.
#[tokio::test]
async fn in_a_one_to_one_every_message_is_the_trigger_and_no_handle_is_typed() {
    let h = harness("agentdmask").await;
    let (base, seen) = scripted_model(vec![says("Two are still unanswered.")]).await;
    use_model(&h, &base).await;
    let agent = h
        .acc
        .create_agent("mail", "Mail", None, AgentProduct::Mail)
        .await
        .unwrap();

    let (status, room) = post(
        &h.app,
        &h.token,
        &format!("/chat/agents/{}/dm", agent.as_str()),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{room}");
    let room_id = room["id"].as_str().unwrap().to_owned();

    let spoken = agent_reply(
        &h,
        &room_id,
        "how many mails am I still owing a reply?",
        Duration::from_secs(20),
    )
    .await
    .expect("the agent answers its own one-to-one without being named");
    assert_eq!(spoken["body"], json!("Two are still unanswered."));
    assert_eq!(
        spoken["author"],
        json!(agent.as_str()),
        "it answers under its own name"
    );
    // The model really was asked, once, with the person's own words.
    let asked = seen.lock().unwrap().clone();
    assert_eq!(asked.len(), 1, "one question, one call: {asked:?}");
    assert!(
        asked[0].to_string().contains("still owing a reply"),
        "the question is what was asked: {}",
        asked[0]
    );

    // The same words in a named room the agent is even a member of: nobody is
    // named, so nobody answers. Without this the test above would pass on an
    // agent that simply answers everything.
    let (status, channel) = post(
        &h.app,
        &h.token,
        "/chat/channels",
        json!({ "name": "planning", "visibility": "public" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{channel}");
    let channel_id = channel["id"].as_str().unwrap().to_owned();
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/chat/channels/{channel_id}/agents"),
        json!({ "agent": agent.as_str() }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        agent_reply(
            &h,
            &channel_id,
            "how many mails am I still owing a reply?",
            Duration::from_secs(3),
        )
        .await,
        None,
        "in a channel a handle is still the trigger"
    );
    assert_eq!(
        seen.lock().unwrap().len(),
        1,
        "an unaddressed message must not cost a model call"
    );
}

/// A colleague reaches nothing through somebody else's one-to-one, and asking
/// the same agent gives them a conversation of their own.
#[tokio::test]
async fn a_colleagues_one_to_one_is_not_visible_and_not_shared() {
    let h = harness("agentdmiso").await;
    let agent = h
        .acc
        .create_agent("hr", "HR", None, AgentProduct::Hr)
        .await
        .unwrap();
    let (status, room) = post(
        &h.app,
        &h.token,
        &format!("/chat/agents/{}/dm", agent.as_str()),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{room}");
    let room_id = room["id"].as_str().unwrap().to_owned();
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/chat/channels/{room_id}/messages"),
        json!({ "body": "what is my leave balance?" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // A second person in the same tenant, logged in the same way.
    let ben = colleague(&h).await;

    // The room, its members, its feed: none of it exists for them.
    for uri in [
        format!("/chat/channels/{room_id}"),
        format!("/chat/channels/{room_id}/messages"),
        format!("/chat/channels/{room_id}/agents"),
    ] {
        let (status, body) = get(&h.app, &ben, &uri).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri} leaked: {body}");
    }
    let (status, listed) = get(&h.app, &ben, "/chat/channels").await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert!(
        listed["channels"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["id"] != json!(room_id)),
        "a colleague's one-to-one is not in anybody else's list: {listed}"
    );

    // Asking the same agent gives them their own conversation, not this one.
    let (status, theirs) = post(
        &h.app,
        &ben,
        &format!("/chat/agents/{}/dm", agent.as_str()),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{theirs}");
    assert_ne!(
        theirs["id"],
        json!(room_id),
        "one room per person per agent: {theirs}"
    );
    let (status, feed) = get(
        &h.app,
        &ben,
        &format!("/chat/channels/{}/messages", theirs["id"].as_str().unwrap()),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{feed}");
    assert!(
        feed["messages"].as_array().unwrap().is_empty(),
        "their own room starts empty — no history comes with the agent: {feed}"
    );
}

/// A second person of the same tenant, with their own token.
async fn colleague(h: &Harness) -> String {
    let email = format!("ben-{}@agentdm.test", h.tenant);
    let user = h.ts.create_user(&email).await.unwrap();
    h.identity
        .set_password(&h.tenant, &user, &email, "s3cret-pw")
        .await
        .unwrap();
    h.identity
        .password_login(&email, "s3cret-pw", None)
        .await
        .unwrap()
        .expect("token issued")
        .0
        .reveal()
        .to_owned()
}
