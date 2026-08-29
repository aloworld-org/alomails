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

use sqlx::postgres::PgPoolOptions;

use crate::common::model::{says, scripted_model, use_model};
use crate::common::{Harness, database_url, harness, harness_on, send};
use alo_store::{
    AgentMemory, AgentProduct, ChannelVisibility, ChatAgentId, ChatChannelId, MemoryLearnedFrom,
    StoreError,
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

async fn del(app: &Router, token: &str, uri: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("DELETE")
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

/// The agent-authored messages of a room, id and body, through `token`'s eyes.
async fn agent_replies(h: &Harness, token: &str, channel: &str) -> Vec<(String, String)> {
    let (status, body) = get(&h.app, token, &format!("/chat/channels/{channel}/messages")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["authorKind"] == "agent")
        .map(|m| {
            (
                m["id"].as_str().unwrap().to_owned(),
                m["body"].as_str().unwrap_or_default().to_owned(),
            )
        })
        .collect()
}

/// Say something as `token` and wait for a **new** agent reply containing
/// `marker` — new, because a room that has already spoken holds old agent
/// messages a first-match search would return instantly.
async fn ask_for(h: &Harness, token: &str, channel: &str, question: &str, marker: &str) -> String {
    let before: std::collections::HashSet<String> = agent_replies(h, token, channel)
        .await
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    let (status, body) = post(
        &h.app,
        token,
        &format!("/chat/channels/{channel}/messages"),
        json!({ "body": question }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let asked = body["id"].as_str().unwrap().to_owned();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let now = agent_replies(h, token, channel).await;
        if now
            .iter()
            .any(|(id, said)| !before.contains(id) && said.contains(marker))
        {
            return asked;
        }
        assert!(
            Instant::now() < deadline,
            "no new reply containing {marker:?}: {now:?}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Wait until the scripted model has been asked `n` times — the fence that
/// keeps a script's entries landing on the calls they were written for.
async fn await_calls(seen: &crate::common::model::Seen, n: usize) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while seen.lock().unwrap().len() < n {
        assert!(
            Instant::now() < deadline,
            "the model was never asked {n} times"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// The user content of the model's `i`th call — the prompt carrying the
/// numbered sources the turn was grounded in.
fn grounded_with(seen: &crate::common::model::Seen, i: usize) -> String {
    let calls = seen.lock().unwrap();
    calls[i]["messages"][1]["content"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// Flip a room's memory switch through the wire, as `token`.
async fn switch_memory(h: &Harness, token: &str, channel: &str, enabled: bool) {
    let (status, body) = post(
        &h.app,
        token,
        &format!("/chat/channels/{channel}/memory"),
        json!({ "enabled": enabled }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

/// **A turn reads its own room's memories and no other room's** — the
/// wrong-channel test (A6.2). One agent, two rooms, one fact remembered in
/// the first: the first room's turn is grounded in it as a numbered source,
/// and the second room's turn — same agent, same asker, same tenant — never
/// sees it. And a room switched off hides what it remembers without deleting
/// it.
#[tokio::test]
async fn a_turn_reads_only_its_own_rooms_memories() {
    let h = harness("memread").await;
    let (deals, agent) = a_room_with(&h, "billing", AgentProduct::Billing).await;
    let (status, body) = post(
        &h.app,
        &h.token,
        "/chat/channels",
        json!({ "name": "ops", "visibility": "public" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let ops = body["id"].as_str().unwrap().to_owned();
    h.acc
        .add_agent_to_channel(&ChatChannelId::new(ops.clone()), &agent)
        .await
        .unwrap();

    ask_for(
        &h,
        &h.token,
        &deals,
        "@billing remember that Northstar invoices are net 30",
        "remember",
    )
    .await;

    let (base, seen) = scripted_model(vec![
        says("Net 30, as agreed [1]."),
        json!(["Northstar sends orders quarterly"]).to_string(),
        says("Nothing here says what Northstar's terms are."),
        json!([]).to_string(),
        says("I can't see anything remembered just now."),
    ])
    .await;
    use_model(&h, &base).await;
    switch_memory(&h, &h.token, &deals, true).await;
    switch_memory(&h, &h.token, &ops, true).await;

    // The room the fact lives in: the turn is grounded in it.
    ask_for(
        &h,
        &h.token,
        &deals,
        "@billing what terms do Northstar have?",
        "Net 30",
    )
    .await;
    let prompt = grounded_with(&seen, 0);
    assert!(
        prompt.contains("remembered \"Northstar invoices are net 30\""),
        "{prompt}"
    );

    // The extraction lands its fact — which is also the fence that keeps the
    // script in order before the next room speaks.
    let deadline = Instant::now() + Duration::from_secs(10);
    while h
        .acc
        .channel_memories(&agent, &ChatChannelId::new(deals.clone()))
        .await
        .unwrap()
        .len()
        < 2
    {
        assert!(Instant::now() < deadline, "the turn never learned");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // The wrong channel: the same agent asked the same thing by the same
    // person, one room over — and no memory of it anywhere in the prompt.
    ask_for(
        &h,
        &h.token,
        &ops,
        "@billing what terms do Northstar have?",
        "Nothing here",
    )
    .await;
    let prompt = grounded_with(&seen, 2);
    assert!(
        !prompt.contains("Northstar invoices are net 30"),
        "{prompt}"
    );
    assert!(!prompt.contains("remembered \""), "{prompt}");
    await_calls(&seen, 4).await;

    // Off hides: the room's own memories stop grounding it the moment its
    // switch goes off — and stay in the store, ready for the switch back on.
    switch_memory(&h, &h.token, &deals, false).await;
    ask_for(
        &h,
        &h.token,
        &deals,
        "@billing what terms do Northstar have?",
        "can't see",
    )
    .await;
    let prompt = grounded_with(&seen, 4);
    assert!(!prompt.contains("remembered \""), "{prompt}");
    assert_eq!(
        h.acc
            .channel_memories(&agent, &ChatChannelId::new(deals))
            .await
            .unwrap()
            .len(),
        2,
        "hidden is not deleted"
    );
}

/// **A one-to-one turn reads what the agent remembers about the asker — and
/// only the asker.** A colleague's turn with the same agent is grounded in
/// their own facts, never the first person's; and a room turn never reads
/// anybody's one-to-one memory, whatever its switch says.
#[tokio::test]
async fn a_dm_turn_reads_the_askers_own_memories_and_nobody_elses() {
    let h = harness("memdmread").await;
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
    let mine = room["id"].as_str().unwrap().to_owned();
    ask_for(
        &h,
        &h.token,
        &mine,
        "Remember that I prefer morning meetings",
        "remember",
    )
    .await;

    // A colleague of the same tenant, with their own one-to-one.
    let email = format!("colleague-{}@memdmread.test", h.tenant);
    let colleague = h.ts.create_user(&email).await.unwrap();
    h.identity
        .set_password(&h.tenant, &colleague, &email, "s3cret-pw")
        .await
        .unwrap();
    let their_token = h
        .identity
        .password_login(&email, "s3cret-pw", None)
        .await
        .unwrap()
        .expect("token issued")
        .0
        .reveal()
        .to_owned();
    let (status, room) = post(
        &h.app,
        &their_token,
        &format!("/chat/agents/{}/dm", agenda.as_str()),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{room}");
    let theirs = room["id"].as_str().unwrap().to_owned();
    ask_for(
        &h,
        &their_token,
        &theirs,
        "Remember that I prefer late afternoons",
        "remember",
    )
    .await;

    let (base, seen) = scripted_model(vec![
        says("Morning it is [1]."),
        json!([]).to_string(),
        says("Late afternoon, then [1]."),
        json!([]).to_string(),
        says("No preference is noted in this room."),
        json!([]).to_string(),
    ])
    .await;
    use_model(&h, &base).await;
    switch_memory(&h, &h.token, &mine, true).await;
    switch_memory(&h, &their_token, &theirs, true).await;

    ask_for(&h, &h.token, &mine, "when should we meet?", "Morning").await;
    let prompt = grounded_with(&seen, 0);
    assert!(
        prompt.contains("remembered \"I prefer morning meetings\""),
        "{prompt}"
    );
    assert!(
        !prompt.contains("late afternoons"),
        "a colleague's memory must never ground another person's turn: {prompt}"
    );
    await_calls(&seen, 2).await;

    ask_for(
        &h,
        &their_token,
        &theirs,
        "when should we meet?",
        "afternoon",
    )
    .await;
    let prompt = grounded_with(&seen, 2);
    assert!(
        prompt.contains("remembered \"I prefer late afternoons\""),
        "{prompt}"
    );
    assert!(!prompt.contains("morning meetings"), "{prompt}");
    await_calls(&seen, 4).await;

    // A room turn reads the room's memory, never a person's — even with the
    // room's switch on and both one-to-ones full of preferences.
    let (status, body) = post(
        &h.app,
        &h.token,
        "/chat/channels",
        json!({ "name": "planning", "visibility": "public" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let planning = body["id"].as_str().unwrap().to_owned();
    h.acc
        .add_agent_to_channel(&ChatChannelId::new(planning.clone()), &agenda)
        .await
        .unwrap();
    switch_memory(&h, &h.token, &planning, true).await;
    ask_for(
        &h,
        &h.token,
        &planning,
        "@agenda when should we meet?",
        "No preference",
    )
    .await;
    let prompt = grounded_with(&seen, 4);
    assert!(
        !prompt.contains("morning meetings") && !prompt.contains("late afternoons"),
        "{prompt}"
    );
}

/// **Deletion follows the withdrawn message** (A6.3): the facts an agent
/// learned from it go with it — in a room and in a one-to-one alike — and
/// only that message's facts, everywhere else untouched.
#[tokio::test]
async fn a_withdrawn_message_takes_the_facts_learned_from_it() {
    let h = harness("memsrc").await;
    let (channel, agent) = a_room_with(&h, "billing", AgentProduct::Billing).await;
    let withdrawn = ask_for(
        &h,
        &h.token,
        &channel,
        "@billing remember that Northstar invoices are net 30",
        "remember",
    )
    .await;
    ask_for(
        &h,
        &h.token,
        &channel,
        "@billing remember that the X100 ships from Ghent",
        "remember",
    )
    .await;

    let (status, body) = del(&h.app, &h.token, &format!("/chat/messages/{withdrawn}")).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
    let rows = h
        .acc
        .channel_memories(&agent, &ChatChannelId::new(channel))
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "only the withdrawn message's fact went");
    assert_eq!(rows[0].fact, "the X100 ships from Ghent");

    // The same rule in a one-to-one: withdrawing the instruction forgets it.
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
    let told = ask_for(
        &h,
        &h.token,
        &dm,
        "Remember that I prefer morning meetings",
        "remember",
    )
    .await;
    assert_eq!(h.acc.my_memories(&agenda).await.unwrap().len(), 1);
    let (status, body) = del(&h.app, &h.token, &format!("/chat/messages/{told}")).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
    assert!(
        h.acc.my_memories(&agenda).await.unwrap().is_empty(),
        "the person's memory follows their withdrawn words too"
    );
}

/// **An archived room, and an agent shown the door, both forget** (A6.3).
/// Removing one agent takes only its memories of that room; archiving the
/// room takes every agent's — and neither reaches another room or what an
/// agent remembers about a person.
#[tokio::test]
async fn an_archived_room_and_a_removed_agent_forget_what_was_learned_there() {
    let h = harness("memgone").await;
    let (deals, billing) = a_room_with(&h, "billing", AgentProduct::Billing).await;
    let tasks = h
        .acc
        .create_agent("tasks", "Tasks", None, AgentProduct::Tasks)
        .await
        .unwrap();
    let deals_id = ChatChannelId::new(deals.clone());
    h.acc.add_agent_to_channel(&deals_id, &tasks).await.unwrap();
    // A second room with the same billing agent, whose memory must survive.
    let (status, body) = post(
        &h.app,
        &h.token,
        "/chat/channels",
        json!({ "name": "ops", "visibility": "public" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let ops = body["id"].as_str().unwrap().to_owned();
    let ops_id = ChatChannelId::new(ops.clone());
    h.acc.add_agent_to_channel(&ops_id, &billing).await.unwrap();

    ask_for(
        &h,
        &h.token,
        &deals,
        "@billing remember that Northstar invoices are net 30",
        "remember",
    )
    .await;
    ask_for(
        &h,
        &h.token,
        &deals,
        "@tasks remember that Fridays are for reviews",
        "remember",
    )
    .await;
    ask_for(
        &h,
        &h.token,
        &ops,
        "@billing remember that ops invoices go to Rotterdam",
        "remember",
    )
    .await;
    // And what the agent knows about a person, which no room's fate touches.
    let (status, room) = post(
        &h.app,
        &h.token,
        &format!("/chat/agents/{}/dm", billing.as_str()),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{room}");
    let dm = room["id"].as_str().unwrap().to_owned();
    ask_for(
        &h,
        &h.token,
        &dm,
        "Remember that I prefer morning meetings",
        "remember",
    )
    .await;

    // Removing billing from deals forgets ITS memories of deals — and
    // nothing else's, and nowhere else's.
    let (status, body) = del(
        &h.app,
        &h.token,
        &format!("/chat/channels/{deals}/agents/{}", billing.as_str()),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
    assert!(
        h.acc
            .channel_memories(&billing, &deals_id)
            .await
            .unwrap()
            .is_empty(),
        "the removed agent's memories of the room went"
    );
    assert_eq!(
        h.acc
            .channel_memories(&tasks, &deals_id)
            .await
            .unwrap()
            .len(),
        1,
        "another agent's memory of the same room stays"
    );
    assert_eq!(
        h.acc
            .channel_memories(&billing, &ops_id)
            .await
            .unwrap()
            .len(),
        1,
        "the same agent's memory of another room stays"
    );

    // Archiving the room forgets every agent's memories of it.
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/chat/channels/{deals}/archive"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        h.acc
            .channel_memories(&tasks, &deals_id)
            .await
            .unwrap()
            .is_empty(),
        "an archived room's memories went with its place in the lists"
    );
    assert_eq!(
        h.acc
            .channel_memories(&billing, &ops_id)
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        h.acc.my_memories(&billing).await.unwrap().len(),
        1,
        "what the agent knows about the person is not the room's to take"
    );
}

/// **A switch left off for thirty days deletes what it hides** (A6.3). The
/// sweep follows the room's own OFF, a workspace default OFF for rooms that
/// never chose, and a one-to-one's OFF alike; a room switched ON keeps
/// everything; a fact stored while the room was already off gets its own
/// thirty days; and one tenant's OFF default reaches no other tenant's rows.
#[tokio::test]
async fn a_switch_left_off_for_thirty_days_deletes_what_it_hides() {
    let h = harness("memsweep").await;
    h.ts.set_admin(&h.user, true).await.unwrap();
    let (off_room, agent) = a_room_with(&h, "billing", AgentProduct::Billing).await;
    let (status, body) = post(
        &h.app,
        &h.token,
        "/chat/channels",
        json!({ "name": "chose-on", "visibility": "public" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let on_room = body["id"].as_str().unwrap().to_owned();
    h.acc
        .add_agent_to_channel(&ChatChannelId::new(on_room.clone()), &agent)
        .await
        .unwrap();
    let (status, body) = post(
        &h.app,
        &h.token,
        "/chat/channels",
        json!({ "name": "follows-default", "visibility": "public" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let default_room = body["id"].as_str().unwrap().to_owned();
    h.acc
        .add_agent_to_channel(&ChatChannelId::new(default_room.clone()), &agent)
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

    switch_memory(&h, &h.token, &off_room, false).await;
    switch_memory(&h, &h.token, &on_room, true).await;
    switch_memory(&h, &h.token, &dm, false).await;
    let (status, body) = post(
        &h.app,
        &h.token,
        "/admin/agent-memory",
        json!({ "enabled": false }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // Explicit remembering ignores every switch — which is how the rows the
    // sweep will judge get seeded at all.
    ask_for(
        &h,
        &h.token,
        &off_room,
        "@billing remember that the old fact was hidden here",
        "remember",
    )
    .await;
    ask_for(
        &h,
        &h.token,
        &on_room,
        "@billing remember that this room chose to keep learning",
        "remember",
    )
    .await;
    ask_for(
        &h,
        &h.token,
        &default_room,
        "@billing remember that this room never chose",
        "remember",
    )
    .await;
    ask_for(
        &h,
        &h.token,
        &dm,
        "Remember that I prefer morning meetings",
        "remember",
    )
    .await;

    // Another tenant on the same store, its default untouched: its rows may
    // never follow this tenant's OFF, however old they grow.
    let other = harness_on(std::sync::Arc::clone(&h.store), "memsweepb").await;
    let their_room = other
        .acc
        .create_channel("their-deals", None, ChannelVisibility::Public)
        .await
        .unwrap();
    let their_agent = other
        .acc
        .create_agent("billing", "billing", None, AgentProduct::Billing)
        .await
        .unwrap();
    other
        .acc
        .add_agent_to_channel(&their_room, &their_agent)
        .await
        .unwrap();
    other
        .acc
        .remember_in_channel(
            &their_agent,
            &their_room,
            "their fact, their switch",
            None,
            MemoryLearnedFrom::Explicit,
        )
        .await
        .unwrap();

    // Not before its time: everything is younger than thirty days.
    h.store.sweep_agent_memories().await.unwrap();
    let off_id = ChatChannelId::new(off_room.clone());
    assert_eq!(
        h.acc.channel_memories(&agent, &off_id).await.unwrap().len(),
        1,
        "hidden is not deleted until the thirty days pass"
    );

    // Thirty-one days pass — for the switches, for the workspace default,
    // and for every row so far (the other tenant's age too).
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url())
        .await
        .unwrap();
    sqlx::query(
        "UPDATE chat_channels SET agent_memory_set_at = now() - interval '31 days' \
         WHERE tenant_id = $1 AND id IN ($2, $3)",
    )
    .bind(h.tenant.as_str())
    .bind(&off_room)
    .bind(&dm)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE agent_memory_defaults SET updated_at = now() - interval '31 days' \
         WHERE tenant_id = $1",
    )
    .bind(h.tenant.as_str())
    .execute(&pool)
    .await
    .unwrap();
    for tenant in [h.tenant.as_str(), other.tenant.as_str()] {
        sqlx::query(
            "UPDATE agent_memories SET created_at = now() - interval '31 days' \
             WHERE tenant_id = $1",
        )
        .bind(tenant)
        .execute(&pool)
        .await
        .unwrap();
    }
    // A fact stored while the room was already off gets its own thirty days.
    ask_for(
        &h,
        &h.token,
        &off_room,
        "@billing remember that the fresh fact is younger",
        "remember",
    )
    .await;

    h.store.sweep_agent_memories().await.unwrap();

    let rows = h.acc.channel_memories(&agent, &off_id).await.unwrap();
    assert_eq!(rows.len(), 1, "the room's aged memories went: {rows:?}");
    assert_eq!(rows[0].fact, "the fresh fact is younger");
    assert!(
        h.acc
            .channel_memories(&agent, &ChatChannelId::new(default_room))
            .await
            .unwrap()
            .is_empty(),
        "a room that follows an OFF default is swept on the default's clock"
    );
    assert_eq!(
        h.acc
            .channel_memories(&agent, &ChatChannelId::new(on_room))
            .await
            .unwrap()
            .len(),
        1,
        "a room switched ON keeps its memories whatever the default says"
    );
    assert!(
        h.acc.my_memories(&agent).await.unwrap().is_empty(),
        "a one-to-one switched off is swept the same way"
    );
    assert_eq!(
        other
            .acc
            .channel_memories(&their_agent, &their_room)
            .await
            .unwrap()
            .len(),
        1,
        "one tenant's OFF default reaches no other tenant's rows"
    );
}

/// The `{fact: (id, canForget)}` map of one What-I-remember listing, and the
/// order its facts came in.
async fn memory_panel(
    h: &Harness,
    token: &str,
    channel: &str,
    agent: &ChatAgentId,
) -> (
    std::collections::HashMap<String, (String, bool)>,
    Vec<String>,
) {
    let (status, body) = get(
        &h.app,
        token,
        &format!(
            "/chat/channels/{channel}/agents/{}/memories",
            agent.as_str()
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let rows = body["memories"].as_array().unwrap();
    let order: Vec<String> = rows
        .iter()
        .map(|m| m["fact"].as_str().unwrap().to_owned())
        .collect();
    let map = rows
        .iter()
        .map(|m| {
            (
                m["fact"].as_str().unwrap().to_owned(),
                (
                    m["id"].as_str().unwrap().to_owned(),
                    m["canForget"].as_bool().unwrap(),
                ),
            )
        })
        .collect();
    (map, order)
}

/// **What an agent remembers here is read by every member, and one fact is
/// forgotten by the room's owner or by the author of its source** (A6.4). The
/// listing says who may forget what (`canForget`), and the DELETE holds the
/// same line: a member who is neither owner nor the fact's source author gets
/// a plain 403 and the fact stays.
#[tokio::test]
async fn the_room_reads_what_is_remembered_and_owner_or_author_forget() {
    let h = harness("mempanel").await;
    let (channel, agent) = a_room_with(&h, "billing", AgentProduct::Billing).await;

    // A second person of the tenant, a member of the room but not its owner.
    let email = format!("member-{}@mempanel.test", h.tenant);
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

    // The owner teaches one fact, the member another.
    ask_for(
        &h,
        &h.token,
        &channel,
        "@billing remember that Northstar invoices are net 30",
        "remember",
    )
    .await;
    ask_for(
        &h,
        &member_token,
        &channel,
        "@billing remember that the X100 ships from Ghent",
        "remember",
    )
    .await;

    // The member reads the whole list, newest first — and may forget only the
    // fact their own words taught.
    let (theirs, order) = memory_panel(&h, &member_token, &channel, &agent).await;
    assert_eq!(
        order,
        vec![
            "the X100 ships from Ghent".to_owned(),
            "Northstar invoices are net 30".to_owned(),
        ]
    );
    assert!(theirs["the X100 ships from Ghent"].1, "their own words");
    assert!(
        !theirs["Northstar invoices are net 30"].1,
        "not theirs to forget — the owner taught it"
    );
    // The owner may forget anything.
    let (owners, _) = memory_panel(&h, &h.token, &channel, &agent).await;
    assert!(owners["Northstar invoices are net 30"].1);
    assert!(owners["the X100 ships from Ghent"].1);

    // The DELETE enforces what the listing said: 403 for the member on the
    // owner's fact, and the fact stays…
    let owners_fact = &theirs["Northstar invoices are net 30"].0;
    let (status, body) = del(
        &h.app,
        &member_token,
        &format!("/chat/memories/{owners_fact}"),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert!(
        body["detail"].as_str().unwrap().contains("owner"),
        "the refusal names the rule: {body}"
    );
    let (still, _) = memory_panel(&h, &member_token, &channel, &agent).await;
    assert_eq!(still.len(), 2, "a refused forget deletes nothing");

    // …204 for the member on their own fact, and 204 for the owner on theirs.
    let (status, body) = del(
        &h.app,
        &member_token,
        &format!("/chat/memories/{}", theirs["the X100 ships from Ghent"].0),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
    let (status, body) = del(&h.app, &h.token, &format!("/chat/memories/{owners_fact}")).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
    let (empty, _) = memory_panel(&h, &h.token, &channel, &agent).await;
    assert!(empty.is_empty(), "both facts were forgotten");
}

/// **A one-to-one's memories are listed to their person alone** (A6.4): the
/// panel in an agent's own one-to-one shows what it remembers about the
/// caller (each fact theirs to forget), another agent has nothing there, a
/// colleague cannot reach the room, and another tenant can neither read nor
/// forget anything — the row outlives every refusal.
#[tokio::test]
async fn a_one_to_ones_memories_are_the_persons_alone() {
    let h = harness("mempaneldm").await;
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
    ask_for(
        &h,
        &h.token,
        &dm,
        "Remember that I prefer morning meetings",
        "remember",
    )
    .await;

    let (mine, _) = memory_panel(&h, &h.token, &dm, &agenda).await;
    assert_eq!(mine.len(), 1);
    let (fact_id, can_forget) = &mine["I prefer morning meetings"];
    assert!(*can_forget, "one's own memory is one's own to forget");

    // Another agent asked about in the same one-to-one: nothing is its.
    let billing = h
        .acc
        .create_agent("billing", "Billing", None, AgentProduct::Billing)
        .await
        .unwrap();
    let (others, _) = memory_panel(&h, &h.token, &dm, &billing).await;
    assert!(others.is_empty(), "another agent holds nothing here");

    // A colleague of the same tenant: the room is not theirs to see.
    let email = format!("colleague-{}@mempaneldm.test", h.tenant);
    let colleague = h.ts.create_user(&email).await.unwrap();
    h.identity
        .set_password(&h.tenant, &colleague, &email, "s3cret-pw")
        .await
        .unwrap();
    let their_token = h
        .identity
        .password_login(&email, "s3cret-pw", None)
        .await
        .unwrap()
        .expect("token issued")
        .0
        .reveal()
        .to_owned();
    let (status, body) = get(
        &h.app,
        &their_token,
        &format!("/chat/channels/{dm}/agents/{}/memories", agenda.as_str()),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    let (status, body) = del(&h.app, &their_token, &format!("/chat/memories/{fact_id}")).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "a person-scoped memory does not exist for anyone else: {body}"
    );

    // Another tenant on the same store: the same nothing.
    let other = harness_on(std::sync::Arc::clone(&h.store), "mempaneldmb").await;
    let (status, body) = get(
        &other.app,
        &other.token,
        &format!("/chat/channels/{dm}/agents/{}/memories", agenda.as_str()),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    let (status, body) = del(
        &other.app,
        &other.token,
        &format!("/chat/memories/{fact_id}"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");

    // Every refusal above deleted nothing.
    assert_eq!(h.acc.my_memories(&agenda).await.unwrap().len(), 1);

    // The person themselves: 204, and the memory is gone.
    let (status, body) = del(&h.app, &h.token, &format!("/chat/memories/{fact_id}")).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
    assert!(h.acc.my_memories(&agenda).await.unwrap().is_empty());
}

/// The delegate envelope, as the model returns it.
fn delegates(to: &str, ask: &str) -> String {
    json!({ "kind": "delegate", "delegate": { "to": to, "ask": ask } }).to_string()
}

/// **A delegate grounds in its own memories, never the asking agent's** —
/// memories are per agent even inside one room. Billing hands a sub-question
/// to Tasks in a room where both have remembered something: each turn's
/// prompt carries its own agent's fact and not the other's.
#[tokio::test]
async fn a_delegate_grounds_in_its_own_memories_not_the_asking_agents() {
    let h = harness("memdeleg").await;
    let (room, _billing) = a_room_with(&h, "billing", AgentProduct::Billing).await;
    let tasks = h
        .acc
        .create_agent("tasks", "Tasks", None, AgentProduct::Tasks)
        .await
        .unwrap();
    h.acc
        .add_agent_to_channel(&ChatChannelId::new(room.clone()), &tasks)
        .await
        .unwrap();

    ask_for(
        &h,
        &h.token,
        &room,
        "@billing remember that Northstar invoices are net 30",
        "remember",
    )
    .await;
    ask_for(
        &h,
        &h.token,
        &room,
        "@tasks remember that Fridays are for reviews",
        "remember",
    )
    .await;

    let (base, seen) = scripted_model(vec![
        delegates("tasks", "what is due this week?"),
        says("The review is due Friday [1]."),
        says("All set — @tasks says the review is due Friday [2]."),
        json!([]).to_string(),
    ])
    .await;
    use_model(&h, &base).await;
    switch_memory(&h, &h.token, &room, true).await;

    ask_for(
        &h,
        &h.token,
        &room,
        "@billing are we ready for Northstar?",
        "All set",
    )
    .await;
    let asking = grounded_with(&seen, 0);
    assert!(
        asking.contains("remembered \"Northstar invoices are net 30\""),
        "{asking}"
    );
    assert!(
        !asking.contains("Fridays are for reviews"),
        "another agent's memory must never ground this one's turn: {asking}"
    );
    let delegated = grounded_with(&seen, 1);
    assert!(
        delegated.contains("remembered \"Fridays are for reviews\""),
        "{delegated}"
    );
    assert!(
        !delegated.contains("Northstar invoices are net 30"),
        "{delegated}"
    );
}
