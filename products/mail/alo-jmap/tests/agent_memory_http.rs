//! Channel memory, on the wire (ADR 0057 §6, A6.1): what an agent remembers,
//! where consent for that lives, and who controls the switches.
//!
//! The properties, none of which a unit test can see:
//!
//! * an explicit **"remember that …" is an instruction, not a question**: the
//!   fact is stored verbatim and confirmed with **no model call at all** — it
//!   works with no provider configured and with the room's learning switched
//!   off, because a person asking by name is the consent the switch
//!   approximates;
//! * a **one-to-one with an agent feeds only that person's memory**, and a
//!   room feeds the room's — the store refuses the combination that would
//!   cross them;
//! * an **answered turn learns** — one extraction call after the answer, the
//!   facts stored against the asker's message — and **only where the room's
//!   switch says so**: a room switched off spends nothing and stores nothing,
//!   a room that never chose follows the workspace default, and a room's own
//!   choice beats that default in both directions;
//! * the switch is the **owner's** in a named room and the default is the
//!   **admin's**, and neither is anybody else's;
//! * memories are **the room's and the tenant's alone**: a non-member cannot
//!   read a private room's, and another tenant cannot read or write anything
//!   here at all.
//!
//! No live model is called: the tenant's AI backend is the scripted local
//! socket in `common::model`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use crate::common::model::{says, scripted_model, use_model};
use crate::common::{Harness, harness, harness_on, send};
use alo_store::{
    AgentMemory, AgentProduct, ChatAgentId, ChatChannelId, MemoryLearnedFrom, StoreError,
};

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

/// A room with one product agent in it, created by the harness user (its
/// owner).
async fn a_room_with(h: &Harness, handle: &str, product: AgentProduct) -> (String, ChatAgentId) {
    let (status, body) = post(
        &h.app,
        &h.token,
        "/chat/channels",
        json!({ "name": format!("{handle}-room"), "visibility": "public" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let channel = body["id"].as_str().unwrap().to_owned();
    let agent = h
        .acc
        .create_agent(handle, handle, Some("its product's agent"), product)
        .await
        .unwrap();
    h.acc
        .add_agent_to_channel(&ChatChannelId::new(channel.clone()), &agent)
        .await
        .unwrap();
    (channel, agent)
}

/// Say something in the room, returning the said message's id, and wait for
/// the agent to say anything back.
async fn ask_and_await_reply(h: &Harness, channel: &str, question: &str) -> (String, String) {
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/chat/channels/{channel}/messages"),
        json!({ "body": question }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let asked = body["id"].as_str().unwrap().to_owned();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let (status, body) = get(
            &h.app,
            &h.token,
            &format!("/chat/channels/{channel}/messages"),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let reply = body["messages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["authorKind"] == "agent")
            .and_then(|m| m["body"].as_str())
            .map(str::to_owned);
        if let Some(reply) = reply {
            return (asked, reply);
        }
        assert!(Instant::now() < deadline, "the agent never spoke: {body}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Wait until the room's memory for `agent` is non-empty, or fail.
async fn await_memories(h: &Harness, agent: &ChatAgentId, channel: &str) -> Vec<AgentMemory> {
    let channel = ChatChannelId::new(channel.to_owned());
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let rows = h.acc.channel_memories(agent, &channel).await.unwrap();
        if !rows.is_empty() {
            return rows;
        }
        assert!(Instant::now() < deadline, "nothing was learned in time");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// **"Remember that …" stores the fact and confirms, with no model and no
/// switch.** No AI provider is configured in this tenant at all, and the
/// room's learning is switched off first — the two things that gate learning,
/// neither of which may gate an explicit instruction.
#[tokio::test]
async fn an_explicit_remember_needs_no_model_and_ignores_the_switch() {
    let h = harness("memexplicit").await;
    let (channel, agent) = a_room_with(&h, "billing", AgentProduct::Billing).await;

    let (status, off) = post(
        &h.app,
        &h.token,
        &format!("/chat/channels/{channel}/memory"),
        json!({ "enabled": false }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{off}");
    assert_eq!(off["enabled"], json!(false));

    let (asked, reply) = ask_and_await_reply(
        &h,
        &channel,
        "@billing remember that Northstar Foods invoices are net 30",
    )
    .await;
    assert!(reply.contains("remember"), "not a confirmation: {reply}");

    let rows = h
        .acc
        .channel_memories(&agent, &ChatChannelId::new(channel.clone()))
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    let memory = &rows[0];
    assert_eq!(memory.fact, "Northstar Foods invoices are net 30");
    assert_eq!(memory.learned_from, MemoryLearnedFrom::Explicit);
    assert_eq!(memory.agent.as_str(), agent.as_str());
    assert_eq!(
        memory.channel.as_ref().map(|c| c.as_str().to_owned()),
        Some(channel.clone()),
        "a room's memory names its room"
    );
    assert!(
        memory.user.is_none(),
        "a room's memory belongs to no person"
    );
    assert_eq!(
        memory.source_msg.as_ref().map(|m| m.as_str().to_owned()),
        Some(asked),
        "the fact carries the message it came from"
    );
}

/// **A one-to-one with an agent feeds only that person's memory** — the row is
/// person-scoped, the room's memory stays empty, and the store refuses a
/// channel-scoped write into an agent DM outright.
#[tokio::test]
async fn a_one_to_one_remember_feeds_only_that_persons_memory() {
    let h = harness("memdm").await;
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
    let dm = room["id"].as_str().unwrap().to_owned();

    // No handle needed in a one-to-one: the room is the address.
    let (_, reply) = ask_and_await_reply(&h, &dm, "Remember that I prefer morning meetings").await;
    assert!(reply.contains("remember"), "not a confirmation: {reply}");

    let mine = h.acc.my_memories(&agent).await.unwrap();
    assert_eq!(mine.len(), 1);
    assert_eq!(mine[0].fact, "I prefer morning meetings");
    assert_eq!(
        mine[0].user.as_ref().map(|u| u.as_str().to_owned()),
        Some(h.user.as_str().to_owned())
    );
    assert!(mine[0].channel.is_none());
    assert!(
        h.acc
            .channel_memories(&agent, &ChatChannelId::new(dm.clone()))
            .await
            .unwrap()
            .is_empty(),
        "nothing is remembered against the room itself"
    );
    // The scope cannot be crossed even by a caller with every right: the
    // store's own shape rule, not a route's judgement.
    let refused = h
        .acc
        .remember_in_channel(
            &agent,
            &ChatChannelId::new(dm),
            "smuggled into the room",
            None,
            MemoryLearnedFrom::Explicit,
        )
        .await;
    assert!(
        matches!(refused, Err(StoreError::Validation(_))),
        "{refused:?}"
    );
}

/// **An answered turn learns, behind the room's switch**: the answer is
/// followed by exactly one extraction call, and what it returns lands in the
/// room's memory against the asker's message.
#[tokio::test]
async fn an_answered_turn_learns_what_the_extractor_returns() {
    let h = harness("memlearn").await;
    let (channel, agent) = a_room_with(&h, "billing", AgentProduct::Billing).await;
    let (base, seen) = scripted_model(vec![
        says("Two quotes are open, both Northstar [1]."),
        json!(["Northstar Foods buys quarterly"]).to_string(),
        says("Mornings are free this week."),
        json!(["prefers the first slot of the day"]).to_string(),
    ])
    .await;
    use_model(&h, &base).await;
    // The scripted harness opts the tenant out of learning so every other
    // suite's call counts stay exact; this room opts back in — which is also
    // the "a room's own switch beats the default" rule doing its job.
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/chat/channels/{channel}/memory"),
        json!({ "enabled": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (asked, reply) = ask_and_await_reply(&h, &channel, "@billing which quotes are open?").await;
    assert!(reply.contains("Two quotes"), "{reply}");

    let rows = await_memories(&h, &agent, &channel).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].fact, "Northstar Foods buys quarterly");
    assert_eq!(rows[0].learned_from, MemoryLearnedFrom::Turn);
    assert_eq!(
        rows[0].source_msg.as_ref().map(|m| m.as_str().to_owned()),
        Some(asked)
    );
    // The extraction was shown the exchange, not the world: question, answer,
    // and what the turn read — this turn read only its grounding.
    {
        let calls = seen.lock().unwrap();
        assert_eq!(calls.len(), 2, "one turn, one extraction");
        let extraction = calls[1]["messages"][1]["content"].as_str().unwrap();
        assert!(extraction.contains("which quotes are open?"));
        assert!(extraction.contains("Two quotes are open"));
    }

    // The same end-of-turn path in a one-to-one feeds the person, not a room:
    // the agent DM's switch is the member's own to flip.
    let agenda = h
        .acc
        .create_agent("agenda", "Agenda", None, AgentProduct::Agenda)
        .await
        .unwrap();
    let (status, room) = post(
        &h.app,
        &h.token,
        &format!("/chat/agents/{}/dm", agenda.as_str()),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{room}");
    let dm = room["id"].as_str().unwrap().to_owned();
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/chat/channels/{dm}/memory"),
        json!({ "enabled": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (asked, reply) = ask_and_await_reply(&h, &dm, "when am I free this week?").await;
    assert!(reply.contains("Mornings"), "{reply}");
    let deadline = Instant::now() + Duration::from_secs(10);
    let mine = loop {
        let mine = h.acc.my_memories(&agenda).await.unwrap();
        if !mine.is_empty() {
            break mine;
        }
        assert!(Instant::now() < deadline, "the one-to-one never learned");
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    assert_eq!(mine.len(), 1);
    assert_eq!(mine[0].fact, "prefers the first slot of the day");
    assert_eq!(mine[0].learned_from, MemoryLearnedFrom::Turn);
    assert!(mine[0].channel.is_none(), "the person's, not the room's");
    assert_eq!(
        mine[0].source_msg.as_ref().map(|m| m.as_str().to_owned()),
        Some(asked)
    );
    assert!(
        h.acc
            .channel_memories(&agenda, &ChatChannelId::new(dm))
            .await
            .unwrap()
            .is_empty()
    );
}

/// **The switches govern learning**: a room switched off spends no extraction
/// call and stores nothing; a room that never chose follows the workspace
/// default; a room's own ON beats a workspace OFF.
#[tokio::test]
async fn the_room_switch_and_the_workspace_default_govern_learning() {
    let h = harness("memswitch").await;
    h.ts.set_admin(&h.user, true).await.unwrap();
    let (room_off, agent) = a_room_with(&h, "billing", AgentProduct::Billing).await;
    let (base, seen) = scripted_model(vec![
        says("Answer one [1]."),
        says("Answer two [1]."),
        says("Answer three [1]."),
        json!(["the fact the override room keeps"]).to_string(),
    ])
    .await;
    use_model(&h, &base).await;

    // Room one: switched off by its owner. The turn answers, nothing follows.
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/chat/channels/{room_off}/memory"),
        json!({ "enabled": false }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (_, reply) = ask_and_await_reply(&h, &room_off, "@billing anything open?").await;
    assert!(reply.contains("Answer one"), "{reply}");
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(
        seen.lock().unwrap().len(),
        1,
        "no extraction call was spent"
    );
    assert!(
        h.acc
            .channel_memories(&agent, &ChatChannelId::new(room_off.clone()))
            .await
            .unwrap()
            .is_empty()
    );

    // The workspace default goes off (admin console)…
    let (status, body) = post(
        &h.app,
        &h.token,
        "/admin/agent-memory",
        json!({ "enabled": false }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    // …so a second room that never chose for itself does not learn…
    let (status, body) = post(
        &h.app,
        &h.token,
        "/chat/channels",
        json!({ "name": "follows-default", "visibility": "public" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let room_default = body["id"].as_str().unwrap().to_owned();
    h.acc
        .add_agent_to_channel(&ChatChannelId::new(room_default.clone()), &agent)
        .await
        .unwrap();
    let (status, body) = get(
        &h.app,
        &h.token,
        &format!("/chat/channels/{room_default}/memory"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body,
        json!({ "enabled": false, "override": Value::Null, "workspaceDefault": false })
    );
    let (_, reply) = ask_and_await_reply(&h, &room_default, "@billing anything open?").await;
    assert!(reply.contains("Answer two"), "{reply}");
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(seen.lock().unwrap().len(), 2, "the default held");

    // …while a third room's own ON beats that default.
    let (status, body) = post(
        &h.app,
        &h.token,
        "/chat/channels",
        json!({ "name": "chose-for-itself", "visibility": "public" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let room_on = body["id"].as_str().unwrap().to_owned();
    h.acc
        .add_agent_to_channel(&ChatChannelId::new(room_on.clone()), &agent)
        .await
        .unwrap();
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/chat/channels/{room_on}/memory"),
        json!({ "enabled": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body,
        json!({ "enabled": true, "override": true, "workspaceDefault": false })
    );
    let (_, reply) = ask_and_await_reply(&h, &room_on, "@billing anything open?").await;
    assert!(reply.contains("Answer three"), "{reply}");
    let rows = await_memories(&h, &agent, &room_on).await;
    assert_eq!(rows[0].fact, "the fact the override room keeps");
    assert_eq!(seen.lock().unwrap().len(), 4, "answer plus extraction");
}

/// **The switch is the owner's; the default is the admin's.** A member reads
/// the room's switch and cannot set it; a non-admin cannot touch the
/// workspace default; and `null` returns a room to following the default.
#[tokio::test]
async fn the_switch_is_the_owners_and_the_default_is_the_admins() {
    let h = harness("memperm").await;
    let (channel, _) = a_room_with(&h, "billing", AgentProduct::Billing).await;

    // A second person of the tenant, a member of the room but not its owner.
    let email = format!("member-{}@memperm.test", h.tenant);
    let member = h.ts.create_user(&email).await.unwrap();
    h.identity
        .set_password(&h.tenant, &member, &email, "s3cret-pw")
        .await
        .unwrap();
    let member_token = h
        .identity
        .password_login(&email, "s3cret-pw", None)
        .await
        .unwrap()
        .expect("token issued")
        .0
        .reveal()
        .to_owned();
    let (status, body) = post(
        &h.app,
        &member_token,
        &format!("/chat/channels/{channel}/join"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (status, body) = get(
        &h.app,
        &member_token,
        &format!("/chat/channels/{channel}/memory"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body,
        json!({ "enabled": true, "override": Value::Null, "workspaceDefault": true }),
        "on by default, and any member may read the switch"
    );
    let (status, body) = post(
        &h.app,
        &member_token,
        &format!("/chat/channels/{channel}/memory"),
        json!({ "enabled": false }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");

    // The owner sets it, and null hands the room back to the default.
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/chat/channels/{channel}/memory"),
        json!({ "enabled": false }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["override"], json!(false));
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/chat/channels/{channel}/memory"),
        json!({ "enabled": Value::Null }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body,
        json!({ "enabled": true, "override": Value::Null, "workspaceDefault": true })
    );
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/chat/channels/{channel}/memory"),
        json!({ "enabled": "sometimes" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");

    // The workspace default is the admin's alone.
    let (status, body) = post(
        &h.app,
        &h.token,
        "/admin/agent-memory",
        json!({ "enabled": false }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    h.ts.set_admin(&h.user, true).await.unwrap();
    let (status, body) = post(
        &h.app,
        &h.token,
        "/admin/agent-memory",
        json!({ "enabled": false }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = get(&h.app, &h.token, "/admin/agent-memory").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body, json!({ "enabled": false }));
}

/// **Memories are the room's and the tenant's alone.** A non-member cannot
/// read a private room's memories; another tenant on the same store can
/// neither read nor write them; and nothing about either refusal is an
/// oracle — both get the same not-found a room that never existed gets.
#[tokio::test]
async fn memories_are_the_tenants_and_the_rooms_alone() {
    let h = harness("memiso").await;
    let (status, body) = post(
        &h.app,
        &h.token,
        "/chat/channels",
        json!({ "name": "war-room", "visibility": "private" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let channel = body["id"].as_str().unwrap().to_owned();
    let agent = h
        .acc
        .create_agent("billing", "billing", None, AgentProduct::Billing)
        .await
        .unwrap();
    let room = ChatChannelId::new(channel.clone());
    h.acc.add_agent_to_channel(&room, &agent).await.unwrap();
    h.acc
        .remember_in_channel(
            &agent,
            &room,
            "the merger closes in October",
            None,
            MemoryLearnedFrom::Explicit,
        )
        .await
        .unwrap();

    // A colleague outside the private room: the room does not exist for them.
    let colleague =
        h.ts.create_user(&format!("outside-{}@memiso.test", h.tenant))
            .await
            .unwrap();
    let their_door = h.store.for_account(h.tenant.clone(), colleague);
    assert!(matches!(
        their_door.channel_memories(&agent, &room).await,
        Err(StoreError::NotFound)
    ));

    // Another tenant on the same store: nothing here is theirs to see or say.
    let other = harness_on(std::sync::Arc::clone(&h.store), "memisob").await;
    assert!(matches!(
        other.acc.channel_memories(&agent, &room).await,
        Err(StoreError::NotFound)
    ));
    assert!(matches!(
        other
            .acc
            .remember_in_channel(
                &agent,
                &room,
                "smuggled across the fence",
                None,
                MemoryLearnedFrom::Explicit,
            )
            .await,
        Err(StoreError::NotFound)
    ));
    // And the room's memory is exactly what it was.
    let rows = h.acc.channel_memories(&agent, &room).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].fact, "the merger closes in October");
}
