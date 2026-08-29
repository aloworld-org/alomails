//! The action record on the wire (ADR 0058 §6, A8.1): every intent
//! execution — an agent's approved proposal, a person's own tap in the
//! palette, a read inside a turn — leaves ONE row carrying what it would do
//! (the preview), what it touched (the record pointer), how to take it back
//! (the inverse verb with its arguments) and which card it settled. A
//! person's click and an agent's proposal are the same object: the two paths
//! are asserted field for field against each other, not against a copy of
//! the expectation.
//!
//! Driven end to end — a real room, the real router and store, a scripted
//! model — because the promise is about the seam: the boundary records, the
//! store keeps, the directory answers.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use alo_store::{AgentProduct, AgentToolRun};

use crate::common::model::{says, scripted_model, use_model, wants};
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

async fn billing_agent(h: &Harness) -> String {
    let handle = alo_store::default_handle(AgentProduct::Billing);
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

/// Northstar Foods, so the draft-invoice verb has a customer to resolve.
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

/// The arguments both paths run `create_invoice_draft` with.
fn draft_args() -> Value {
    json!({
        "customer": "Northstar Foods BV",
        "lines": [{ "description": "Consulting", "quantity": 2,
                    "unitPriceCents": 10_000, "vatRateBp": 2_100 }],
    })
}

/// What the registry's template says this write would do, with the customer
/// filled in — asserted verbatim, because the preview is the sentence a
/// person decided on.
const DRAFT_PREVIEW: &str = "A draft invoice for Northstar Foods BV will be raised — unnumbered, unsent, for the user to issue.";

/// The newest `create_invoice_draft` action in the caller's own record.
async fn newest_draft_action(h: &Harness) -> AgentToolRun {
    h.acc
        .agent_tool_runs(20)
        .await
        .unwrap()
        .into_iter()
        .find(|run| run.tool == "create_invoice_draft")
        .expect("the execution left an action row")
}

/// The action-record fields that must not depend on who triggered the run —
/// what it would do, what it touched, and how to take it back.
fn action_shape(run: &AgentToolRun) -> (Option<String>, Option<String>, Option<String>, Value) {
    (
        run.preview.clone(),
        run.record_type.clone(),
        run.undo_tool.clone(),
        run.undo_args.clone().unwrap_or(Value::Null),
    )
}

/// An agent's approved proposal leaves the full action row — preview, record,
/// undo, and the join back to the card the room saw — and the directory
/// answers with the same fields.
#[tokio::test]
async fn an_approved_proposal_is_the_action_record() {
    let h = harness("actions-proposal").await;
    northstar(&h).await;
    let agent = billing_agent(&h).await;
    let room = a_room_with(&h, "finance", &agent).await;

    let (model, _seen) = scripted_model(vec![wants(
        "create_invoice_draft",
        draft_args(),
        "I'll raise a draft invoice for Northstar.",
    )])
    .await;
    use_model(&h, &model).await;

    let spoken = ask_in_room(&h, &room, "@billing invoice Northstar for the consulting").await;
    let proposal = spoken["proposal"]["id"].as_str().unwrap().to_owned();
    let (status, decided) = post(
        &h.app,
        &h.token,
        &format!("/chat/proposals/{proposal}"),
        json!({ "approve": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{decided}");
    let invoice = decided["result"]["result"]["invoice"]["id"]
        .as_str()
        .expect("the approval answered the raised draft")
        .to_owned();

    let action = newest_draft_action(&h).await;
    assert!(action.ok);
    assert_eq!(action.effect, "write");
    assert_eq!(action.preview.as_deref(), Some(DRAFT_PREVIEW));
    assert_eq!(action.record_type.as_deref(), Some("invoice"));
    assert_eq!(action.record_id.as_deref(), Some(invoice.as_str()));
    assert_eq!(action.undo_tool.as_deref(), Some("discard_invoice_draft"));
    assert_eq!(
        action.undo_args.as_ref().unwrap()["invoice"],
        invoice,
        "the undo names the record this run touched"
    );
    assert_eq!(
        action
            .proposal
            .as_ref()
            .map(alo_store::ChatProposalId::as_str),
        Some(proposal.as_str()),
        "the action joins back to the card the room saw"
    );
    assert_eq!(
        action.agent.as_ref().map(alo_store::ChatAgentId::as_str),
        Some(agent.as_str())
    );

    // The directory reports the same action, not a thinner summary of it.
    let (status, body) = get(&h.app, &h.token, &format!("/chat/agents/{agent}/directory")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let recent = body["recent"]
        .as_array()
        .unwrap()
        .iter()
        .find(|run| run["tool"] == "create_invoice_draft")
        .expect("the directory lists the action");
    assert_eq!(recent["preview"], DRAFT_PREVIEW);
    assert_eq!(recent["record"]["kind"], "invoice");
    assert_eq!(recent["record"]["id"], invoice);
    assert_eq!(recent["undoable"], true);
    assert_eq!(recent["proposal"], proposal);
}

/// A person's own tap in the palette leaves the same object — the same
/// preview, the same record pointer, the same undo — with no agent and no
/// proposal, because nobody proposed anything: the person acted.
#[tokio::test]
async fn a_persons_tap_leaves_the_same_object() {
    let h = harness("actions-tap").await;
    northstar(&h).await;
    let agent = billing_agent(&h).await;
    let room = a_room_with(&h, "finance", &agent).await;

    // First the agent's path, so the person's row has something real to be
    // compared against.
    let (model, _seen) = scripted_model(vec![wants(
        "create_invoice_draft",
        draft_args(),
        "Raising the draft.",
    )])
    .await;
    use_model(&h, &model).await;
    let spoken = ask_in_room(&h, &room, "@billing invoice Northstar for the consulting").await;
    let proposal = spoken["proposal"]["id"].as_str().unwrap().to_owned();
    let (status, decided) = post(
        &h.app,
        &h.token,
        &format!("/chat/proposals/{proposal}"),
        json!({ "approve": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{decided}");
    let agents_action = newest_draft_action(&h).await;

    // Now the person's own tap: the palette's execute route, no model at all.
    let (status, body) = post(
        &h.app,
        &h.token,
        "/ai/agent/execute",
        json!({ "tool": "create_invoice_draft", "args": draft_args() }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let tapped = newest_draft_action(&h).await;

    assert!(tapped.ok);
    assert!(tapped.agent.is_none(), "the person acted for themselves");
    assert!(tapped.proposal.is_none(), "nobody proposed anything");
    // The same object: field for field against the agent's row, except for
    // the record id — two runs raised two drafts.
    let (preview, kind, undo_tool, undo_args) = action_shape(&agents_action);
    let (tap_preview, tap_kind, tap_undo_tool, tap_undo_args) = action_shape(&tapped);
    assert_eq!(tap_preview, preview);
    assert_eq!(tap_kind, kind);
    assert_eq!(tap_undo_tool, undo_tool);
    assert_eq!(
        tap_undo_args["invoice"].as_str().is_some(),
        undo_args["invoice"].as_str().is_some()
    );
    assert_ne!(
        tapped.record_id, agents_action.record_id,
        "two runs, two drafts"
    );
}

/// A read inside the turn is an intent execution and leaves its row — with
/// nothing to preview, nothing to invert, and no card to join, because a read
/// changes nothing.
#[tokio::test]
async fn a_read_carries_no_preview_and_no_undo() {
    let h = harness("actions-read").await;
    northstar(&h).await;
    let agent = billing_agent(&h).await;
    let room = a_room_with(&h, "finance", &agent).await;

    let (model, _seen) = scripted_model(vec![
        wants("open_quotes", json!({}), "Let me look."),
        says("Nothing is open."),
    ])
    .await;
    use_model(&h, &model).await;
    ask_in_room(&h, &room, "@billing which quotes are open?").await;

    let looked = h
        .acc
        .agent_tool_runs(20)
        .await
        .unwrap()
        .into_iter()
        .find(|run| run.tool == "open_quotes")
        .expect("the read left its row");
    assert_eq!(looked.effect, "read");
    assert!(looked.preview.is_none());
    assert!(looked.undo_tool.is_none() && looked.undo_args.is_none());
    assert!(looked.proposal.is_none());
}

/// Another tenant sees none of it — not the rows, and not through the
/// directory with our agent id guessed exactly right.
#[tokio::test]
async fn another_tenant_reads_no_actions() {
    let h = harness("actions-tenant-a").await;
    northstar(&h).await;
    let agent = billing_agent(&h).await;
    let room = a_room_with(&h, "finance", &agent).await;
    let (model, _seen) = scripted_model(vec![wants(
        "create_invoice_draft",
        draft_args(),
        "Raising the draft.",
    )])
    .await;
    use_model(&h, &model).await;
    let spoken = ask_in_room(&h, &room, "@billing invoice Northstar for the consulting").await;
    let proposal = spoken["proposal"]["id"].as_str().unwrap().to_owned();
    let (status, decided) = post(
        &h.app,
        &h.token,
        &format!("/chat/proposals/{proposal}"),
        json!({ "approve": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{decided}");
    assert!(!h.acc.agent_tool_runs(20).await.unwrap().is_empty());

    let other = harness_on(std::sync::Arc::clone(&h.store), "actions-tenant-b").await;
    assert!(other.acc.agent_tool_runs(20).await.unwrap().is_empty());
    // Our agent's id names nothing of ours in their directory: their tenant
    // has no such agent, so the answer is the one an id never issued gets.
    let (status, body) = get(
        &other.app,
        &other.token,
        &format!("/chat/agents/{agent}/directory"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
}
