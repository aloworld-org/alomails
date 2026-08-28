//! One agent handing a sub-question to another inside its run, on the wire
//! (ADR 0057 §3, A5.1).
//!
//! The properties, none of which can be seen from a unit test:
//!
//! * the **room sees the handoff line** before the delegate's turn runs, and
//!   the delegate itself posts nothing — its answer is **folded in** as a
//!   numbered source the asking agent cites;
//! * the delegate's turn is an ordinary turn of its own: **its prompt, its
//!   grounding, its scope**, through the asker's account door;
//! * a handle the asker cannot see — another tenant's agent, an agent of a
//!   module switched off for them — is **dropped**, with no room line and no
//!   turn taken;
//! * the run is **bounded**: at most four handoffs, refusals included, and a
//!   chain no deeper than two;
//! * a **write a delegate wants is not proposed** from a handoff (that lands
//!   with A5.2's one approval surface): the asking agent is told, the person
//!   is pointed at the agent itself, and no proposal row exists.
//!
//! No live model is called: the tenant's AI backend is the scripted local
//! socket in `common::model`, which records what each turn was shown — and
//! what the model was shown is where the folding and the scoping are visible.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use alo_store::{AgentProduct, AppModule, ChatAgentId, ChatChannelId};
use common::model::{Seen, says, scripted_model, use_model, wants};
use common::{Harness, harness, harness_on, send};

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

/// The delegate envelope, as the model returns it.
fn delegates(to: &str, ask: &str) -> String {
    json!({ "kind": "delegate", "delegate": { "to": to, "ask": ask } }).to_string()
}

/// A room with one product agent in it — the agent that will be addressed.
/// Every other agent a test defines stays out of the room: a handoff needs no
/// membership of its target, only visibility.
async fn a_room_with(h: &Harness, handle: &str, product: AgentProduct) -> (String, ChatAgentId) {
    let (status, body) = post(
        &h.app,
        &h.token,
        "/chat/channels",
        json!({ "name": "ops", "visibility": "public" }),
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

/// One product agent, defined but in no room.
async fn an_agent(h: &Harness, handle: &str, product: AgentProduct) -> ChatAgentId {
    h.acc
        .create_agent(handle, handle, Some("knows its own product"), product)
        .await
        .unwrap()
}

async fn messages(h: &Harness, channel: &str) -> Vec<Value> {
    let (status, body) = get(
        &h.app,
        &h.token,
        &format!("/chat/channels/{channel}/messages"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let mut all = body["messages"].as_array().unwrap().clone();
    all.sort_by_key(|m| m["seq"].as_i64().unwrap_or_default());
    all
}

/// Say something in the room and wait until `done` is true of what is in it.
async fn ask_and_wait(
    h: &Harness,
    channel: &str,
    question: &str,
    done: impl Fn(&[Value]) -> bool,
) -> Vec<Value> {
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/chat/channels/{channel}/messages"),
        json!({ "body": question }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let all = messages(h, channel).await;
        if done(&all) {
            return all;
        }
        assert!(
            Instant::now() < deadline,
            "the run never got there: {}",
            json!(all)
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// The messages an agent said, in order.
fn said_by<'a>(all: &'a [Value], agent: &ChatAgentId) -> Vec<&'a Value> {
    all.iter()
        .filter(|m| m["authorKind"] == "agent" && m["author"] == json!(agent.as_str()))
        .collect()
}

/// The handoff lines in the room, in order.
fn handoff_lines(all: &[Value]) -> Vec<String> {
    all.iter()
        .filter_map(|m| m["body"].as_str())
        .filter(|body| body.starts_with("I'm asking @"))
        .map(str::to_owned)
        .collect()
}

/// The system prompt the model was shown on its `n`th call.
fn system_of(seen: &Seen, n: usize) -> String {
    seen.lock().unwrap()[n]["messages"][0]["content"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// What the model was asked on its `n`th call — the user turn, which carries
/// the sources, the folded answers, and the handoff offer.
fn user_of(seen: &Seen, n: usize) -> String {
    seen.lock().unwrap()[n]["messages"][1]["content"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn calls(seen: &Seen) -> usize {
    seen.lock().unwrap().len()
}

/// **The whole shape of a handoff**: the room sees who asked whom for what,
/// the delegate takes an ordinary turn of its own — its prompt, its reading
/// tool run for real — and its answer comes back to the asking agent as a
/// numbered source it cites. The delegate itself posts nothing.
#[tokio::test]
async fn a_handoff_runs_the_delegates_turn_and_folds_the_answer_in() {
    let h = harness("delegfold").await;
    let (channel, billing) = a_room_with(&h, "billing", AgentProduct::Billing).await;
    let tasks = an_agent(&h, "tasks", AgentProduct::Tasks).await;

    let (base, seen) = scripted_model(vec![
        delegates("tasks", "what is on my plate this week?"),
        wants("my_plate", json!({}), "Looking at the list."),
        says("Nothing is due this week."),
        says("Nothing stands in the way — @tasks says nothing is due [1]."),
    ])
    .await;
    use_model(&h, &base).await;

    let all = ask_and_wait(
        &h,
        &channel,
        "@billing is anything blocking the Northstar quote?",
        |all| {
            all.iter().any(|m| {
                m["body"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("Nothing stands in the way")
            })
        },
    )
    .await;

    // The room saw the handoff line, said by the asking agent, before the
    // answer — and the delegate itself said nothing at all.
    let lines = handoff_lines(&all);
    assert_eq!(
        lines,
        vec!["I'm asking @tasks: what is on my plate this week?"]
    );
    let spoke = said_by(&all, &billing);
    assert_eq!(spoke.len(), 2, "the handoff line and the answer");
    assert!(
        spoke[0]["body"]
            .as_str()
            .unwrap()
            .starts_with("I'm asking @tasks:")
    );
    assert!(said_by(&all, &tasks).is_empty(), "a delegate posts nothing");

    // Four model calls: the handoff decision, the delegate's read turn, the
    // delegate's answer, the asking agent's answer over the folded source.
    assert_eq!(calls(&seen), 4);

    // The offer named the roster; the delegate's turn was its own — its
    // prompt, its request — and its read actually ran before it answered.
    assert!(user_of(&seen, 0).contains("@tasks (the tasks agent)"));
    let delegate_prompt = system_of(&seen, 1);
    assert!(
        delegate_prompt.starts_with("You are the alo Tasks agent"),
        "{delegate_prompt}"
    );
    assert!(user_of(&seen, 1).contains("what is on my plate this week?"));
    let after_read = user_of(&seen, 2);
    assert!(after_read.contains("my_plate"));
    assert!(after_read.contains("result of a tool you just ran"));

    // The answer came back to the asking agent as a citable numbered source.
    let folded = user_of(&seen, 3);
    assert!(
        folded.contains("delegated answer \"@tasks\""),
        "the fold names who was asked: {folded}"
    );
    assert!(folded.contains("@tasks answered: Nothing is due this week."));

    // Reads answer, writes propose, handoffs fold: nothing here waits on a tap.
    assert!(all.iter().all(|m| m["proposal"].is_null()));
}

/// **Only agents the asker can see.** A handle from another tenant and the
/// handle of a module switched off for the asker meet the same fate: dropped
/// with a line the model can answer around — no handoff line in the room, no
/// delegate turn taken. This is the wrong-tenant test of the handoff surface.
#[tokio::test]
async fn a_handle_the_asker_cannot_see_is_dropped_without_a_room_line() {
    let h = harness("delegdrop").await;
    let (channel, billing) = a_room_with(&h, "billing", AgentProduct::Billing).await;
    // Tenant B's agent, on the same store — visible to nobody in tenant A.
    let other = harness_on(std::sync::Arc::clone(&h.store), "delegdropb").await;
    an_agent(&other, "ghost", AgentProduct::Inventory).await;
    // …and an agent of A's own whose module an admin switched off for the
    // asker, through the store the console writes through.
    an_agent(&h, "inventory", AgentProduct::Inventory).await;
    let admin = h.ts.create_user("console@delegdrop.test").await.unwrap();
    h.ts.set_admin(&admin, true).await.unwrap();
    h.ts.set_module_access(&h.user, AppModule::Inventory, false, &admin)
        .await
        .unwrap();

    let (base, seen) = scripted_model(vec![
        delegates("ghost", "is the X100 in stock?"),
        delegates("inventory", "is the X100 in stock?"),
        says("I couldn't reach anyone who can check the stock."),
    ])
    .await;
    use_model(&h, &base).await;

    let all = ask_and_wait(&h, &channel, "@billing can we fulfil the quote?", |all| {
        all.iter().any(|m| {
            m["body"]
                .as_str()
                .unwrap_or_default()
                .contains("couldn't reach anyone")
        })
    })
    .await;

    // Neither handle produced a handoff line or a delegate turn — the model
    // was told there is nobody by that name, and answered around it.
    assert!(handoff_lines(&all).is_empty(), "{}", json!(all));
    assert_eq!(said_by(&all, &billing).len(), 1);
    assert_eq!(calls(&seen), 3, "no delegate turn was ever taken");
    assert!(user_of(&seen, 1).contains("there is no @ghost"));
    assert!(user_of(&seen, 2).contains("there is no @inventory"));
    // The offer itself never named either: one is another tenant's, the other
    // is behind the module gate.
    assert!(!user_of(&seen, 0).contains("@ghost"));
    assert!(!user_of(&seen, 0).contains("@inventory"));
}

/// **At most four handoffs per run, refusals included.** The fifth is refused
/// without a model call for it, and the run ends saying so — bounded in code,
/// not in the prompt.
#[tokio::test]
async fn the_fifth_handoff_is_refused_and_the_run_ends() {
    let h = harness("delegcap").await;
    let (channel, billing) = a_room_with(&h, "billing", AgentProduct::Billing).await;
    an_agent(&h, "tasks", AgentProduct::Tasks).await;

    let script = vec![
        delegates("tasks", "part one?"),
        says("one"),
        delegates("tasks", "part two?"),
        says("two"),
        delegates("tasks", "part three?"),
        says("three"),
        delegates("tasks", "part four?"),
        says("four"),
        // The fifth handoff: refused by the budget, so this is the last call.
        delegates("tasks", "part five?"),
    ];
    let (base, seen) = scripted_model(script).await;
    use_model(&h, &base).await;

    let all = ask_and_wait(&h, &channel, "@billing reconcile everything", |all| {
        all.iter().any(|m| {
            m["body"]
                .as_str()
                .unwrap_or_default()
                .contains("ask the remaining part")
        })
    })
    .await;

    assert_eq!(handoff_lines(&all).len(), 4, "{}", json!(all));
    let last = said_by(&all, &billing).last().unwrap()["body"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(last.contains("as much as I'm allowed to"), "{last}");
    // Nine calls: four (decision + delegate answer) pairs and the refused
    // fifth decision. The fifth delegate turn was never taken.
    assert_eq!(calls(&seen), 9);
    assert!(all.iter().all(|m| m["proposal"].is_null()));
}

/// **A chain ends at depth two.** The turn two handoffs down is offered
/// nobody, and a stray envelope from it is dropped like an unknown handle —
/// so the third hop never happens and every answer still folds back up.
#[tokio::test]
async fn a_handoff_chain_ends_at_depth_two() {
    let h = harness("delegdeep").await;
    let (channel, billing) = a_room_with(&h, "billing", AgentProduct::Billing).await;
    an_agent(&h, "inventory", AgentProduct::Inventory).await;
    an_agent(&h, "tasks", AgentProduct::Tasks).await;

    let (base, seen) = scripted_model(vec![
        delegates("inventory", "can we ship the X100 order?"),
        delegates("tasks", "is there an open recount task?"),
        // The turn at depth two tries a third hop anyway.
        delegates("billing", "what did we quote?"),
        says("No open recount task."),
        says("Stock is fine and no recount is open."),
        says("All clear to ship."),
    ])
    .await;
    use_model(&h, &base).await;

    let all = ask_and_wait(&h, &channel, "@billing can we ship?", |all| {
        all.iter().any(|m| {
            m["body"]
                .as_str()
                .unwrap_or_default()
                .contains("All clear to ship")
        })
    })
    .await;

    // Two handoff lines — the third hop was never made.
    let lines = handoff_lines(&all);
    assert_eq!(lines.len(), 2, "{}", json!(all));
    assert!(lines[0].starts_with("I'm asking @inventory:"));
    assert!(lines[1].starts_with("I'm asking @tasks:"));

    // The offer is made at depths zero and one, and not at two; the stray
    // envelope from depth two met the same line an unknown handle does.
    assert!(user_of(&seen, 0).contains("You can hand off to:"));
    assert!(user_of(&seen, 1).contains("You can hand off to:"));
    assert!(!user_of(&seen, 2).contains("You can hand off to:"));
    assert!(user_of(&seen, 3).contains("there is no @billing"));

    // Each answer folded into the turn above it.
    assert!(user_of(&seen, 4).contains("@tasks answered: No open recount task."));
    assert!(user_of(&seen, 5).contains("@inventory answered: Stock is fine"));
    assert_eq!(calls(&seen), 6);
    assert_eq!(said_by(&all, &billing).len(), 2, "its line and its answer");
}

/// **A delegate's write is not proposed from a handoff.** The asking agent is
/// told what the delegate wanted, no proposal row exists anywhere, and the
/// person is pointed at the agent that can do it — until A5.2 lands the one
/// approval surface for delegated writes.
#[tokio::test]
async fn a_delegates_write_is_folded_as_words_and_proposed_nowhere() {
    let h = harness("delegwrite").await;
    let (channel, billing) = a_room_with(&h, "billing", AgentProduct::Billing).await;
    let tasks = an_agent(&h, "tasks", AgentProduct::Tasks).await;

    let (base, seen) = scripted_model(vec![
        delegates("tasks", "add a follow-up for the Northstar quote"),
        wants(
            "create_task",
            json!({ "title": "Follow up on the Northstar quote" }),
            "I'll add a follow-up task.",
        ),
        says("@tasks can add that follow-up if you ask it directly."),
    ])
    .await;
    use_model(&h, &base).await;

    let all = ask_and_wait(&h, &channel, "@billing chase the Northstar quote", |all| {
        all.iter().any(|m| {
            m["body"]
                .as_str()
                .unwrap_or_default()
                .contains("ask it directly")
        })
    })
    .await;

    // The delegate's wish came back as words for the asking agent…
    let folded = user_of(&seen, 2);
    assert!(folded.contains("wanted to make a change first"));
    assert!(folded.contains("I'll add a follow-up task."));
    assert!(folded.contains("not proposed from here"));
    // …and nowhere as a button: no proposal row, and the delegate said
    // nothing in the room.
    assert!(
        all.iter().all(|m| m["proposal"].is_null()),
        "{}",
        json!(all)
    );
    assert!(said_by(&all, &tasks).is_empty());
    assert_eq!(calls(&seen), 3);
    assert_eq!(said_by(&all, &billing).len(), 2);
}
