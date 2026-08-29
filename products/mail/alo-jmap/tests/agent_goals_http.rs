//! Goals on the wire (ADR 0058 §7, A8.3): the Northstar demo.
//!
//! "Close the Northstar deal by Friday" becomes ONE goal record across four
//! products — Sales looks the deal up, Billing raises the invoice, Mail drafts
//! the customer's update, Agenda books the review — and the properties the
//! record exists for are what is asserted:
//!
//! * the goal keeps the plan and its progress, readable from the room;
//! * it waits behind **one approval surface** at a time, naming the proposal;
//! * an approval **resumes** it — "the rest of this waits until you approve
//!   that" is finally a fact, not a courtesy;
//! * a refusal ends it, exactly as the room was promised;
//! * Stop reaches a resumed segment the same way it reaches a fresh run.
//!
//! No live model is called: the tenant's AI backend is the scripted local
//! socket in `common::model`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use crate::common::model::{says, scripted_model, scripted_model_paced, use_model, wants};
use crate::common::{Harness, harness, send};
use alo_store::{AgentProduct, ChatAgentId, ChatChannelId};

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

/// A plan envelope, as the planner returns it.
fn plans(steps: &[(&str, &str)]) -> String {
    json!({
        "kind": "plan",
        "steps": steps
            .iter()
            .map(|(agent, ask)| json!({ "agent": agent, "ask": ask }))
            .collect::<Vec<_>>()
    })
    .to_string()
}

/// A room with Ask alo in it, and the product agents defined in the tenant but
/// not in the room — putting them there is the run's own job.
async fn a_room_with_ask_alo(h: &Harness) -> (String, ChatAgentId) {
    let (status, body) = post(
        &h.app,
        &h.token,
        "/chat/channels",
        json!({ "name": "deals", "visibility": "public" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let channel = body["id"].as_str().unwrap().to_owned();
    let alo = h
        .acc
        .create_agent(
            "alo",
            "alo",
            Some("asks the others"),
            AgentProduct::Workspace,
        )
        .await
        .unwrap();
    h.acc
        .add_agent_to_channel(&ChatChannelId::new(channel.clone()), &alo)
        .await
        .unwrap();
    (channel, alo)
}

async fn an_agent(h: &Harness, handle: &str, product: AgentProduct) -> ChatAgentId {
    h.acc
        .create_agent(handle, handle, Some("knows its own product"), product)
        .await
        .unwrap()
}

/// The billing customer the demo invoices.
async fn northstar(h: &Harness) {
    let (status, body) = post(
        &h.app,
        &h.token,
        "/billing/customers",
        json!({ "name": "Northstar Foods BV", "addressLine1": "Demo Street 1",
                "postalCode": "1011 AB", "city": "Amsterdam", "country": "NL",
                "paymentTermsDays": 30, "currency": "EUR" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
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

/// Wait until `done` is true of the room's messages.
async fn wait_for(h: &Harness, channel: &str, done: impl Fn(&[Value]) -> bool) -> Vec<Value> {
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

/// The room's one goal, as `GET /chat/channels/{id}/goals` reads it.
async fn the_goal(h: &Harness, channel: &str) -> Value {
    let (status, body) = get(&h.app, &h.token, &format!("/chat/channels/{channel}/goals")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let goals = body["goals"].as_array().unwrap();
    assert_eq!(goals.len(), 1, "one ask, one goal: {body}");
    goals[0].clone()
}

/// Wait until the room's goal satisfies `done`.
async fn wait_goal(h: &Harness, channel: &str, done: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let goal = the_goal(h, channel).await;
        if done(&goal) {
            return goal;
        }
        assert!(
            Instant::now() < deadline,
            "the goal never got there: {goal}"
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

/// The room's pending proposals — the approval surface, which must never hold
/// more than one card.
fn pending(all: &[Value]) -> Vec<Value> {
    all.iter()
        .filter(|m| m["proposal"]["state"] == json!("pending"))
        .map(|m| m["proposal"].clone())
        .collect()
}

/// The per-step states off the goal card.
fn step_states(goal: &Value) -> Vec<&str> {
    goal["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["state"].as_str().unwrap())
        .collect()
}

/// Approve one card and hand back what the execution answered.
async fn approve(h: &Harness, card: &str) -> Value {
    let (status, done) = post(
        &h.app,
        &h.token,
        &format!("/chat/proposals/{card}"),
        json!({ "approve": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{done}");
    assert_eq!(done["state"], json!("approved"));
    done
}

/// **The Northstar demo.** One goal across Sales, Billing, Mail and Agenda:
/// the deal looked up, the invoice raised, the customer written to, the review
/// booked — three approvals, each of which resumes the goal, until it is done.
#[tokio::test]
async fn the_northstar_demo_one_goal_across_four_products() {
    let h = harness("goalnorth").await;
    let (channel, alo) = a_room_with_ask_alo(&h).await;
    let crm = an_agent(&h, "crm", AgentProduct::Crm).await;
    let _billing = an_agent(&h, "billing", AgentProduct::Billing).await;
    let mail = an_agent(&h, "mail", AgentProduct::Mail).await;
    let agenda = an_agent(&h, "agenda", AgentProduct::Agenda).await;
    northstar(&h).await;

    let (base, seen) = scripted_model(vec![
        plans(&[
            ("crm", "which stage is the Northstar Foods deal at?"),
            (
                "billing",
                "raise a draft invoice for Northstar Foods for the consulting",
            ),
            (
                "mail",
                "draft an email to Northstar Foods about the invoice",
            ),
            ("agenda", "book the Friday review with Northstar Foods"),
        ]),
        says("Northstar Foods is at the proposal stage, waiting on our number."),
        wants(
            "create_invoice_draft",
            json!({ "customer": "Northstar Foods BV",
                    "lines": [{ "description": "Consulting", "quantity": 2,
                                "unitPriceCents": 10_000, "vatRateBp": 2_100 }] }),
            "I'll raise a draft invoice for Northstar Foods.",
        ),
        wants(
            "draft_email",
            json!({ "to": "finance@northstar.example",
                    "subject": "Your consulting invoice",
                    "body": "The draft invoice for the consulting is on its way." }),
            "I'll draft the update to Northstar Foods.",
        ),
        wants(
            "create_event",
            json!({ "title": "Northstar review", "start": "2026-09-04T10:00:00Z" }),
            "I'll book the Friday review.",
        ),
    ])
    .await;
    use_model(&h, &base).await;

    // The ask. The run stops at Billing's write, and the goal is waiting
    // behind exactly that card, one step already done.
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/chat/channels/{channel}/messages"),
        json!({ "body": "@alo close the Northstar deal by Friday" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let all = wait_for(&h, &channel, |all| {
        all.iter().any(|m| {
            m["body"]
                .as_str()
                .unwrap_or_default()
                .contains("waits until you approve")
        })
    })
    .await;

    let goal = the_goal(&h, &channel).await;
    assert_eq!(goal["status"], json!("waiting"));
    assert_eq!(goal["cursor"], json!(1));
    assert_eq!(
        step_states(&goal),
        vec!["done", "waiting", "pending", "pending"]
    );
    assert!(
        goal["request"]
            .as_str()
            .unwrap()
            .contains("close the Northstar deal"),
    );
    assert_eq!(goal["askedBy"], json!(h.user.as_str()));
    // The CRM step answered — a read wears no button — and the plan named all
    // four products before any of them spoke.
    assert_eq!(said_by(&all, &crm).len(), 1);
    let plan_line = said_by(&all, &alo)[0]["body"].as_str().unwrap();
    assert!(plan_line.contains("4. @agenda"), "{plan_line}");
    // One approval surface, and the goal names it.
    let cards = pending(&all);
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0]["tool"], json!("create_invoice_draft"));
    assert_eq!(goal["proposal"], cards[0]["id"]);

    // Approval one: the invoice is really raised, and the goal carries on —
    // the promise "the rest of this waits until you approve that" kept.
    let done = approve(&h, cards[0]["id"].as_str().unwrap()).await;
    assert!(
        done["result"]["result"]["invoice"]["id"].is_string(),
        "{done}"
    );
    let all = wait_for(&h, &channel, |all| {
        all.iter()
            .any(|m| m["body"] == json!("Carrying on — step 3 of 4."))
            && !pending(all).is_empty()
    })
    .await;
    let goal = the_goal(&h, &channel).await;
    assert_eq!(goal["status"], json!("waiting"));
    assert_eq!(goal["cursor"], json!(2));
    assert_eq!(
        step_states(&goal),
        vec!["done", "done", "waiting", "pending"]
    );
    let cards = pending(&all);
    assert_eq!(cards.len(), 1, "one surface at a time");
    assert_eq!(cards[0]["tool"], json!("draft_email"));
    assert_eq!(said_by(&all, &mail).len(), 1);

    // Approval two: the mail draft lands, Agenda proposes the review.
    approve(&h, cards[0]["id"].as_str().unwrap()).await;
    let all = wait_for(&h, &channel, |all| {
        all.iter()
            .any(|m| m["body"] == json!("Carrying on — step 4 of 4."))
            && !pending(all).is_empty()
    })
    .await;
    let goal = the_goal(&h, &channel).await;
    assert_eq!(goal["cursor"], json!(3));
    let cards = pending(&all);
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0]["tool"], json!("create_event"));
    assert_eq!(said_by(&all, &agenda).len(), 1);

    // Approval three settles the last step: nothing to carry on with, the
    // goal is done, and nothing anywhere is left pending.
    let done = approve(&h, cards[0]["id"].as_str().unwrap()).await;
    assert!(!done["result"].is_null());
    let goal = wait_goal(&h, &channel, |g| g["status"] == json!("done")).await;
    assert_eq!(goal["cursor"], json!(4));
    assert_eq!(step_states(&goal), vec!["done", "done", "done", "done"]);
    assert!(goal["proposal"].is_null());
    assert!(pending(&messages(&h, &channel).await).is_empty());
    // Five model calls: the plan and one per step — resuming replays nothing.
    assert_eq!(seen.lock().unwrap().len(), 5);
}

/// Turning the waited-on proposal down ends the goal — "turn it down and I'll
/// leave it there", now a property of the record: the remaining step never
/// runs and the card says why it stopped.
#[tokio::test]
async fn turning_the_proposal_down_leaves_the_goal_there() {
    let h = harness("goaldecline").await;
    let (channel, _alo) = a_room_with_ask_alo(&h).await;
    let _billing = an_agent(&h, "billing", AgentProduct::Billing).await;
    let mail = an_agent(&h, "mail", AgentProduct::Mail).await;
    northstar(&h).await;

    let (base, seen) = scripted_model(vec![
        plans(&[
            ("billing", "raise a draft invoice for Northstar Foods"),
            ("mail", "tell Northstar Foods it is coming"),
        ]),
        wants(
            "create_invoice_draft",
            json!({ "customer": "Northstar Foods BV",
                    "lines": [{ "description": "Consulting", "quantity": 1,
                                "unitPriceCents": 10_000, "vatRateBp": 2_100 }] }),
            "I'll raise a draft invoice for Northstar Foods.",
        ),
    ])
    .await;
    use_model(&h, &base).await;

    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/chat/channels/{channel}/messages"),
        json!({ "body": "@alo invoice Northstar and tell them" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let all = wait_for(&h, &channel, |all| !pending(all).is_empty()).await;
    let card = pending(&all)[0]["id"].as_str().unwrap().to_owned();

    let (status, done) = post(
        &h.app,
        &h.token,
        &format!("/chat/proposals/{card}"),
        json!({ "approve": false }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{done}");
    assert_eq!(done["state"], json!("discarded"));

    let goal = wait_goal(&h, &channel, |g| g["status"] == json!("stopped")).await;
    assert_eq!(goal["note"], json!("the proposal was turned down"));
    assert_eq!(goal["cursor"], json!(0));
    assert!(goal["proposal"].is_null());
    // The rest of the plan really was left there: no resumed segment, no mail
    // step, no further model call.
    tokio::time::sleep(Duration::from_millis(600)).await;
    let settled = messages(&h, &channel).await;
    assert!(said_by(&settled, &mail).is_empty());
    assert!(!settled.iter().any(|m| {
        m["body"]
            .as_str()
            .unwrap_or_default()
            .starts_with("Carrying on")
    }),);
    assert_eq!(seen.lock().unwrap().len(), 2, "the plan and the one step");
}

/// **Stop reaches a resumed segment.** The continuation registers a turn of
/// its own, so the same button that stops a fresh run stops a goal that an
/// approval just woke up — and the goal record says stopped, not working.
#[tokio::test]
async fn stop_ends_a_resumed_segment_and_the_goal_says_so() {
    let h = harness("goalstop").await;
    let (channel, alo) = a_room_with_ask_alo(&h).await;
    let _billing = an_agent(&h, "billing", AgentProduct::Billing).await;
    let _mail = an_agent(&h, "mail", AgentProduct::Mail).await;
    let tasks = an_agent(&h, "tasks", AgentProduct::Tasks).await;
    northstar(&h).await;

    // Paced, so the resumed segment is a run to interrupt, not a race to win.
    let (base, seen) = scripted_model_paced(
        vec![
            plans(&[
                ("billing", "raise a draft invoice for Northstar Foods"),
                ("mail", "who wrote to us at Northstar Foods?"),
                ("tasks", "what is on my plate?"),
            ]),
            wants(
                "create_invoice_draft",
                json!({ "customer": "Northstar Foods BV",
                        "lines": [{ "description": "Consulting", "quantity": 1,
                                    "unitPriceCents": 10_000, "vatRateBp": 2_100 }] }),
                "I'll raise a draft invoice for Northstar Foods.",
            ),
            says("Nobody at Northstar has written this month."),
            says("Three things, none overdue."),
        ],
        Duration::from_millis(500),
    )
    .await;
    use_model(&h, &base).await;

    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/chat/channels/{channel}/messages"),
        json!({ "body": "@alo chase Northstar" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let all = wait_for(&h, &channel, |all| !pending(all).is_empty()).await;
    let card = pending(&all)[0]["id"].as_str().unwrap().to_owned();

    // Approve: the goal resumes, says so, and is a running turn again.
    approve(&h, &card).await;
    wait_for(&h, &channel, |all| {
        all.iter()
            .any(|m| m["body"] == json!("Carrying on — step 2 of 3."))
    })
    .await;
    let (status, running) = get(&h.app, &h.token, &format!("/chat/channels/{channel}/turns")).await;
    assert_eq!(status, StatusCode::OK, "{running}");
    let turn = running["turns"][0]["id"]
        .as_str()
        .expect("the resumed segment is a registered turn");
    assert_eq!(running["turns"][0]["agent"], json!(alo.as_str()));
    assert_eq!(running["turns"][0]["mine"], json!(true));
    let (status, _) = post(
        &h.app,
        &h.token,
        &format!("/chat/channels/{channel}/turns/{turn}/stop"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let all = wait_for(&h, &channel, |all| {
        all.iter().any(|m| {
            m["body"]
                .as_str()
                .unwrap_or_default()
                .starts_with("Stopped —")
        })
    })
    .await;
    let goal = wait_goal(&h, &channel, |g| g["status"] == json!("stopped")).await;
    assert!(goal["cursor"].as_u64().unwrap() < 3, "{goal}");
    // The last step never ran, and nothing more arrives: stopped, not paused.
    assert!(said_by(&all, &tasks).is_empty());
    assert!(
        seen.lock().unwrap().len() < 4,
        "the last turn was never taken"
    );
    tokio::time::sleep(Duration::from_millis(800)).await;
    assert_eq!(messages(&h, &channel).await.len(), all.len());
}
