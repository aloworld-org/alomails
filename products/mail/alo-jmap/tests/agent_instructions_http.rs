//! Standing instructions, on the wire (ADR 0057 §7, queue item A7.1): a
//! person asks once, in advance, and each firing is an ordinary agent turn
//! with the author as asker.
//!
//! The properties, none of which a unit test can see:
//!
//! * an instruction is stood up **in a room, by a member, for an agent that
//!   is in the room** — and refused outside those bounds, twenty per channel;
//! * a due **schedule fires one turn as the author**: the agent's answer is
//!   posted into the room, the clock moves one whole repeat, and the same
//!   sweep run again fires nothing;
//! * a **write firing proposes to the author**: the proposal's `askedBy` is
//!   the author, nobody else can decide it, and nothing runs without a tap;
//! * an **event trigger fires from the tenant's stream** — nothing before the
//!   first matching event, one coalesced firing after any number of them, and
//!   at most one firing per hour;
//! * **Cancel is the author's and the room owner's**; a member who is neither
//!   is refused with the rule named;
//! * the author leaving **pauses** the instruction (the card says so), and an
//!   archived room or a removed agent deletes it;
//! * another tenant's rooms and instructions **do not exist here** — reads,
//!   writes and the sweep alike.
//!
//! No live model is called: the tenant's AI backend is the scripted local
//! socket in `common::model`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use sqlx::postgres::PgPoolOptions;

use crate::common::model::{says, scripted_model, use_model, wants};
use crate::common::{Harness, database_url, harness, harness_on, send};
use alo_store::{AgentProduct, ChatAgentId, ChatChannelId, NewDomainEvent};

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

/// A second person of the same tenant, joined into the (public) room.
async fn a_member_in(h: &Harness, channel: &str, tag: &str) -> (String, alo_store::UserId) {
    let email = format!("{tag}-{}@instr.test", h.tenant);
    let member = h.ts.create_user(&email).await.unwrap();
    h.identity
        .set_password(&h.tenant, &member, &email, "s3cret-pw")
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
    let (status, body) = post(
        &h.app,
        &token,
        &format!("/chat/channels/{channel}/join"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    (token, member)
}

/// The AppState the sweeper runs on — the same store the harness router uses,
/// exactly as `main.rs` holds one state for both.
fn sweeper_state(h: &Harness) -> alo_jmap::state::AppState {
    alo_jmap::app_state(Arc::clone(&h.store), h.identity.clone(), "https://test")
}

/// An instant RFC 3339 minutes away from now (negative = the past).
fn minutes_from_now(minutes: i64) -> String {
    (OffsetDateTime::now_utc() + time::Duration::minutes(minutes))
        .format(&Rfc3339)
        .unwrap()
}

/// The agent-authored messages currently in the room, oldest first.
async fn agent_messages(h: &Harness, token: &str, channel: &str) -> Vec<Value> {
    let (status, body) = get(&h.app, token, &format!("/chat/channels/{channel}/messages")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["authorKind"] == "agent")
        .cloned()
        .collect()
}

/// Wait until the room holds at least `count` agent messages, or fail — a
/// firing can be taken by any concurrent sweep over the shared store, so the
/// message is awaited rather than assumed to land inside one call.
async fn await_agent_messages(h: &Harness, channel: &str, count: usize) -> Vec<Value> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let spoken = agent_messages(h, &h.token, channel).await;
        if spoken.len() >= count {
            return spoken;
        }
        assert!(
            Instant::now() < deadline,
            "the agent never spoke: {spoken:?}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// **Standing one up is bounded**: member author, agent in the room, a sane
/// trigger, twenty per channel — and each refusal names its rule.
#[tokio::test]
async fn an_instruction_is_created_listed_and_bounded() {
    let h = harness("instr-create").await;
    let (channel, agent) = a_room_with(&h, "billing", AgentProduct::Billing).await;
    let (member_token, _) = a_member_in(&h, &channel, "author").await;

    // The member stands one up; the card carries what the author asked.
    let (status, made) = post(
        &h.app,
        &member_token,
        &format!("/chat/channels/{channel}/instructions"),
        json!({
            "agentId": agent.as_str(),
            "text": "every morning, list the invoices that fell overdue",
            "trigger": { "kind": "schedule", "everyMinutes": 1440 },
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{made}");
    assert_eq!(made["agentHandle"], "billing");
    assert_eq!(made["trigger"]["kind"], "schedule");
    assert_eq!(made["trigger"]["everyMinutes"], 1440);
    assert_eq!(made["paused"], false);
    assert!(made["canCancel"].as_bool().unwrap(), "{made}");
    // Asked in advance: with no firstAt the first firing is one repeat away,
    // never this second.
    let next = made["nextRun"].as_str().unwrap();
    let next = OffsetDateTime::parse(next, &Rfc3339).unwrap();
    assert!(next > OffsetDateTime::now_utc() + time::Duration::minutes(1000));

    // Every member reads the card; only the author and the owner may cancel.
    let (status, listed) = get(
        &h.app,
        &h.token,
        &format!("/chat/channels/{channel}/instructions"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    let cards = listed["instructions"].as_array().unwrap();
    assert_eq!(cards.len(), 1, "{listed}");
    assert!(
        cards[0]["canCancel"].as_bool().unwrap(),
        "the owner cancels any: {listed}"
    );
    let (_, other_view) = get(
        &h.app,
        &{
            let (token, _) = a_member_in(&h, &channel, "reader").await;
            token
        },
        &format!("/chat/channels/{channel}/instructions"),
    )
    .await;
    assert_eq!(
        other_view["instructions"][0]["canCancel"], false,
        "a mere member cancels nothing of others': {other_view}"
    );

    // The refusals, each with its rule.
    let path = format!("/chat/channels/{channel}/instructions");
    let cases = [
        (
            json!({ "agentId": agent.as_str(), "text": "x", "trigger": { "kind": "sometimes" } }),
            "an unknown trigger",
        ),
        (
            json!({ "agentId": agent.as_str(), "text": "x",
                    "trigger": { "kind": "schedule", "everyMinutes": 30 } }),
            "under an hour",
        ),
        (
            json!({ "agentId": agent.as_str(), "text": "x",
                    "trigger": { "kind": "event", "event": "summon_dragons" } }),
            "a verb the registry does not name",
        ),
        (
            json!({ "agentId": agent.as_str(), "text": "",
                    "trigger": { "kind": "schedule", "everyMinutes": 60 } }),
            "no words",
        ),
    ];
    for (body, what) in cases {
        let (status, err) = post(&h.app, &member_token, &path, body).await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{what} must be refused: {err}"
        );
    }
    // An agent that is not in the room takes no instruction for it.
    let elsewhere = h
        .acc
        .create_agent("tasks", "tasks", None, AgentProduct::Tasks)
        .await
        .unwrap();
    let (status, err) = post(
        &h.app,
        &member_token,
        &path,
        json!({ "agentId": elsewhere.as_str(), "text": "chase things",
                "trigger": { "kind": "schedule", "everyMinutes": 60 } }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{err}");
    assert!(
        err["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("not in this room"),
        "{err}"
    );

    // Twenty per channel, the design's bound, counted where the row is made.
    for n in 1..20 {
        let (status, err) = post(
            &h.app,
            &member_token,
            &path,
            json!({ "agentId": agent.as_str(), "text": format!("standing ask {n}"),
                    "trigger": { "kind": "schedule", "everyMinutes": 1440 } }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{err}");
    }
    let (status, err) = post(
        &h.app,
        &member_token,
        &path,
        json!({ "agentId": agent.as_str(), "text": "one too many",
                "trigger": { "kind": "schedule", "everyMinutes": 1440 } }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{err}");
    assert!(
        err["detail"].as_str().unwrap_or_default().contains("20"),
        "{err}"
    );
}

/// **Cancel is the author's and the room owner's** — a member who is neither
/// gets the rule named, an outsider gets nothing at all.
#[tokio::test]
async fn cancel_is_the_authors_and_the_owners() {
    let h = harness("instr-cancel").await;
    let (channel, agent) = a_room_with(&h, "billing", AgentProduct::Billing).await;
    let (author_token, _) = a_member_in(&h, &channel, "author").await;
    let (bystander_token, _) = a_member_in(&h, &channel, "bystander").await;

    let path = format!("/chat/channels/{channel}/instructions");
    let stand = |text: &str| {
        json!({ "agentId": agent.as_str(), "text": text,
                "trigger": { "kind": "schedule", "everyMinutes": 1440 } })
    };

    // The author cancels their own.
    let (_, mine) = post(&h.app, &author_token, &path, stand("mine to take back")).await;
    let (status, _) = del(
        &h.app,
        &author_token,
        &format!("/chat/instructions/{}", mine["id"].as_str().unwrap()),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // A bystander is refused with the rule; the owner then cancels it.
    let (_, theirs) = post(&h.app, &author_token, &path, stand("the owner's to end")).await;
    let held = format!("/chat/instructions/{}", theirs["id"].as_str().unwrap());
    let (status, err) = del(&h.app, &bystander_token, &held).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{err}");
    assert!(
        err["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("author"),
        "{err}"
    );
    let (status, _) = del(&h.app, &h.token, &held).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, listed) = get(&h.app, &h.token, &path).await;
    assert_eq!(
        listed["instructions"].as_array().unwrap().len(),
        0,
        "{listed}"
    );
}

/// **A due schedule fires one turn as its author**: the answer posts into the
/// room, the clock moves a whole repeat, and the same sweep fires nothing
/// twice.
#[tokio::test]
async fn a_scheduled_instruction_fires_a_turn_as_its_author() {
    let h = harness("instr-fire").await;
    let (channel, agent) = a_room_with(&h, "billing", AgentProduct::Billing).await;
    let (author_token, _) = a_member_in(&h, &channel, "author").await;
    let (model, seen) = scripted_model(vec![says(
        "Two invoices fell overdue this week: INV-7 and INV-9.",
    )])
    .await;
    use_model(&h, &model).await;

    let (status, made) = post(
        &h.app,
        &author_token,
        &format!("/chat/channels/{channel}/instructions"),
        json!({
            "agentId": agent.as_str(),
            "text": "list the invoices that fell overdue",
            "trigger": { "kind": "schedule", "everyMinutes": 60,
                          "firstAt": minutes_from_now(-5) },
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{made}");

    let state = sweeper_state(&h);
    alo_jmap::agent_instructions::run_due(&state).await;
    let spoken = await_agent_messages(&h, &channel, 1).await;
    assert!(
        spoken[0]["body"].as_str().unwrap().contains("INV-7"),
        "the reading turn's answer posts into the room: {spoken:?}"
    );
    // The turn ran the author's words verbatim as its question. The guard is
    // scoped so no lock is held across the awaits below.
    let prompt = {
        let shown = seen.lock().unwrap();
        shown
            .first()
            .map(|req| req["messages"].to_string())
            .unwrap_or_default()
    };
    assert!(
        prompt.contains("list the invoices that fell overdue"),
        "the model was asked the instruction's own words: {prompt}"
    );

    // Claimed means claimed: the clock has moved a whole repeat, and a second
    // sweep finds nothing due.
    let (_, listed) = get(
        &h.app,
        &h.token,
        &format!("/chat/channels/{channel}/instructions"),
    )
    .await;
    let card = &listed["instructions"][0];
    let next = OffsetDateTime::parse(card["nextRun"].as_str().unwrap(), &Rfc3339).unwrap();
    assert!(next > OffsetDateTime::now_utc(), "{listed}");
    assert!(card["lastFiredAt"].as_str().is_some(), "{listed}");
    alo_jmap::agent_instructions::run_due(&state).await;
    assert_eq!(
        agent_messages(&h, &h.token, &channel).await.len(),
        1,
        "one firing per due moment, not one per sweep"
    );
}

/// **A write firing proposes to the author**: the proposal lands on the
/// author's approval surface and nobody else's, and nothing runs untapped.
#[tokio::test]
async fn a_write_firing_proposes_to_the_author() {
    let h = harness("instr-propose").await;
    let (channel, agent) = a_room_with(&h, "billing", AgentProduct::Billing).await;
    let (author_token, author) = a_member_in(&h, &channel, "author").await;
    let (model, _seen) = scripted_model(vec![wants(
        "send_quote",
        json!({ "customer": "Northstar Foods BV" }),
        "I'll send Northstar their weekly offer.",
    )])
    .await;
    use_model(&h, &model).await;

    let (status, made) = post(
        &h.app,
        &author_token,
        &format!("/chat/channels/{channel}/instructions"),
        json!({
            "agentId": agent.as_str(),
            "text": "send Northstar Foods their weekly offer",
            "trigger": { "kind": "schedule", "everyMinutes": 60,
                          "firstAt": minutes_from_now(-5) },
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{made}");

    alo_jmap::agent_instructions::run_due(&sweeper_state(&h)).await;
    let spoken = await_agent_messages(&h, &channel, 1).await;
    let proposal = &spoken[0]["proposal"];
    assert!(!proposal.is_null(), "a write is a proposal: {spoken:?}");
    assert_eq!(proposal["tool"], "send_quote");
    assert_eq!(
        proposal["askedBy"].as_str().unwrap(),
        author.as_str(),
        "the proposal is the author's: {proposal}"
    );

    // The room's owner is not the asker, and cannot decide it; the author
    // can — declining here, so nothing billing-shaped needs to exist.
    let decide = format!("/chat/proposals/{}", proposal["id"].as_str().unwrap());
    let (status, err) = post(&h.app, &h.token, &decide, json!({ "approve": false })).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{err}");
    let (status, body) = post(&h.app, &author_token, &decide, json!({ "approve": false })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

/// **An event trigger fires from the tenant's stream** — nothing before the
/// first matching event, one coalesced firing after, at most one an hour.
#[tokio::test]
async fn an_event_instruction_fires_on_the_stream_and_coalesces() {
    let h = harness("instr-event").await;
    let (channel, agent) = a_room_with(&h, "billing", AgentProduct::Billing).await;
    let (author_token, _) = a_member_in(&h, &channel, "author").await;
    let (model, _seen) =
        scripted_model(vec![says("An invoice was just issued; the books moved.")]).await;
    use_model(&h, &model).await;

    let (status, made) = post(
        &h.app,
        &author_token,
        &format!("/chat/channels/{channel}/instructions"),
        json!({
            "agentId": agent.as_str(),
            "text": "when an invoice is issued, note it here",
            "trigger": { "kind": "event", "event": "issue_invoice" },
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{made}");
    let card = made["id"].as_str().unwrap().to_owned();

    // No matching event yet: the sweep leaves it alone.
    let state = sweeper_state(&h);
    alo_jmap::agent_instructions::run_due(&state).await;
    assert_eq!(agent_messages(&h, &h.token, &channel).await.len(), 0);

    // Two executions land on the stream; one sweep, ONE coalesced firing.
    for _ in 0..2 {
        h.acc
            .emit_event(&NewDomainEvent {
                kind: "issue_invoice",
                effect: "write",
                record_type: None,
                record_id: None,
                agent: None,
            })
            .await
            .unwrap();
    }
    alo_jmap::agent_instructions::run_due(&state).await;
    let spoken = await_agent_messages(&h, &channel, 1).await;
    assert_eq!(spoken.len(), 1, "coalesced: {spoken:?}");

    // A third event inside the hour: the cooldown holds.
    h.acc
        .emit_event(&NewDomainEvent {
            kind: "issue_invoice",
            effect: "write",
            record_type: None,
            record_id: None,
            agent: None,
        })
        .await
        .unwrap();
    alo_jmap::agent_instructions::run_due(&state).await;
    assert_eq!(
        agent_messages(&h, &h.token, &channel).await.len(),
        1,
        "one firing per instruction per hour"
    );

    // The hour passes (backdated): the held-back event now fires it again.
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url())
        .await
        .unwrap();
    sqlx::query(
        "UPDATE agent_instructions SET last_fired_at = now() - interval '2 hours' \
         WHERE tenant_id = $1 AND id = $2",
    )
    .bind(h.tenant.as_str())
    .bind(&card)
    .execute(&pool)
    .await
    .unwrap();
    alo_jmap::agent_instructions::run_due(&state).await;
    let spoken = await_agent_messages(&h, &channel, 2).await;
    assert_eq!(spoken.len(), 2, "{spoken:?}");
}

/// **Paused when the author leaves; deleted with the room or the agent.**
#[tokio::test]
async fn the_author_leaving_pauses_and_the_room_or_agent_going_deletes() {
    let h = harness("instr-pause").await;
    let (channel, agent) = a_room_with(&h, "billing", AgentProduct::Billing).await;
    let (author_token, author) = a_member_in(&h, &channel, "author").await;
    let path = format!("/chat/channels/{channel}/instructions");
    let (status, made) = post(
        &h.app,
        &author_token,
        &path,
        json!({ "agentId": agent.as_str(), "text": "keep watch",
                "trigger": { "kind": "schedule", "everyMinutes": 1440 } }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{made}");

    // The owner removes the author: the card says paused.
    let (status, body) = del(
        &h.app,
        &h.token,
        &format!("/chat/channels/{channel}/members/{}", author.as_str()),
    )
    .await;
    assert!(status.is_success(), "{body}");
    let (_, listed) = get(&h.app, &h.token, &path).await;
    assert_eq!(listed["instructions"][0]["paused"], true, "{listed}");

    // Due but paused: the sweep must not touch it. The clock is forced into
    // the past directly, so this can never race another sweep — a paused row
    // is unclaimable however due it is.
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url())
        .await
        .unwrap();
    sqlx::query(
        "UPDATE agent_instructions SET next_run = now() - interval '1 hour' \
         WHERE tenant_id = $1 AND id = $2",
    )
    .bind(h.tenant.as_str())
    .bind(made["id"].as_str().unwrap())
    .execute(&pool)
    .await
    .unwrap();
    alo_jmap::agent_instructions::run_due(&sweeper_state(&h)).await;
    assert_eq!(
        agent_messages(&h, &h.token, &channel).await.len(),
        0,
        "a paused instruction never fires"
    );

    // The agent leaving the room takes its instructions with it.
    let (status, body) = del(
        &h.app,
        &h.token,
        &format!("/chat/channels/{channel}/agents/{}", agent.as_str()),
    )
    .await;
    assert!(status.is_success(), "{body}");
    let (_, listed) = get(&h.app, &h.token, &path).await;
    assert_eq!(
        listed["instructions"].as_array().unwrap().len(),
        0,
        "{listed}"
    );

    // And an archived room takes every remaining card with it.
    let (second, second_agent) = a_room_with(&h, "tasks", AgentProduct::Tasks).await;
    let (status, made) = post(
        &h.app,
        &h.token,
        &format!("/chat/channels/{second}/instructions"),
        json!({ "agentId": second_agent.as_str(), "text": "keep watch here too",
                "trigger": { "kind": "schedule", "everyMinutes": 1440 } }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{made}");
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/chat/channels/{second}/archive"),
        json!({}),
    )
    .await;
    assert!(status.is_success(), "{body}");
    let count: (i64,) = sqlx::query_as(
        "SELECT count(*) FROM agent_instructions WHERE tenant_id = $1 AND channel_id = $2",
    )
    .bind(h.tenant.as_str())
    .bind(&second)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count.0, 0, "an archived room holds no instructions");
}

/// **Another tenant's rooms and instructions do not exist here** — the
/// mandatory wrong-tenant proof, one answer for every verb: not found.
#[tokio::test]
async fn another_tenants_instructions_do_not_exist_here() {
    let h = harness("instr-iso-a").await;
    let other = harness_on(Arc::clone(&h.store), "instr-iso-b").await;
    let (channel, agent) = a_room_with(&h, "billing", AgentProduct::Billing).await;
    let path = format!("/chat/channels/{channel}/instructions");
    let (status, made) = post(
        &h.app,
        &h.token,
        &path,
        json!({ "agentId": agent.as_str(), "text": "watch the books",
                "trigger": { "kind": "schedule", "everyMinutes": 1440 } }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{made}");

    // The room, its cards and the card's id answer the other tenant with the
    // same nothing an id that was never issued gets.
    let (status, body) = get(&other.app, &other.token, &path).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    let (status, body) = post(
        &other.app,
        &other.token,
        &path,
        json!({ "agentId": agent.as_str(), "text": "reach across",
                "trigger": { "kind": "schedule", "everyMinutes": 1440 } }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    let (status, body) = del(
        &other.app,
        &other.token,
        &format!("/chat/instructions/{}", made["id"].as_str().unwrap()),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    // And the row is still standing where it belongs.
    let (_, listed) = get(&h.app, &h.token, &path).await;
    assert_eq!(
        listed["instructions"].as_array().unwrap().len(),
        1,
        "{listed}"
    );
}
