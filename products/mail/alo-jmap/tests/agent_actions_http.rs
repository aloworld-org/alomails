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

/// The caller's own actions on the wire, newest first.
async fn listed_actions(h: &Harness) -> Vec<Value> {
    let (status, body) = get(&h.app, &h.token, "/ai/actions").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["actions"].as_array().unwrap().clone()
}

/// The newest listed action for one tool, with its wire id.
async fn newest_listed(h: &Harness, tool: &str) -> Value {
    listed_actions(h)
        .await
        .into_iter()
        .find(|action| action["tool"] == tool)
        .unwrap_or_else(|| panic!("no {tool} action listed"))
}

/// **An agent is undone with the button that undoes a person** (A8.2): the
/// approved proposal's action row carries its inverse, one POST runs it, the
/// draft is gone, and the undo left its own action row — a person's act, on
/// the record like everything else. Undoing twice refuses at the executor,
/// because the record it names no longer exists.
#[tokio::test]
async fn an_agents_action_is_undone_with_the_button_that_undoes_a_person() {
    let h = harness("actions-undo-agent").await;
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
    let invoice = decided["result"]["result"]["invoice"]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    // The action list is the button's surface: the row says it can be undone.
    let action = newest_listed(&h, "create_invoice_draft").await;
    assert_eq!(action["undoable"], true, "{action}");
    let action_id = action["id"].as_str().unwrap().to_owned();

    let (status, undone) = post(
        &h.app,
        &h.token,
        &format!("/ai/actions/{action_id}/undo"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{undone}");
    assert_eq!(undone["ok"], true, "{undone}");

    // The draft is gone from the record.
    let (status, body) = get(&h.app, &h.token, &format!("/billing/invoices/{invoice}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");

    // The undo is itself an action: the person's own, pointing at the same
    // record, with no inverse of its own.
    let undo_row = newest_listed(&h, "discard_invoice_draft").await;
    assert_eq!(undo_row["effect"], "write");
    assert_eq!(undo_row["ok"], true);
    assert_eq!(undo_row["record"]["kind"], "invoice");
    assert_eq!(undo_row["record"]["id"], invoice);
    assert_eq!(undo_row["undoable"], false, "an undo has no undo");

    // Twice is refused where the truth lives: the draft no longer exists.
    let (status, again) = post(
        &h.app,
        &h.token,
        &format!("/ai/actions/{action_id}/undo"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{again}");
}

/// The same button undoes a person's own tap — no model, no proposal, the
/// palette's action row carries the same inverse.
#[tokio::test]
async fn a_persons_tap_is_undone_with_the_same_button() {
    let h = harness("actions-undo-tap").await;
    northstar(&h).await;

    let (status, body) = post(
        &h.app,
        &h.token,
        "/ai/agent/execute",
        json!({ "tool": "create_invoice_draft", "args": draft_args() }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let invoice = body["result"]["invoice"]["id"].as_str().unwrap().to_owned();

    let action = newest_listed(&h, "create_invoice_draft").await;
    assert!(action["channel"].is_null(), "a palette tap is in no room");
    let action_id = action["id"].as_str().unwrap().to_owned();
    let (status, undone) = post(
        &h.app,
        &h.token,
        &format!("/ai/actions/{action_id}/undo"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{undone}");
    let (status, body) = get(&h.app, &h.token, &format!("/billing/invoices/{invoice}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
}

/// A recorded payment is taken back through its declared inverse: the row's
/// undo names the payment, one POST removes it, and the invoice is owed
/// again — the reversal in the ledger, nothing edited.
#[tokio::test]
async fn a_recorded_payment_is_undone_and_the_invoice_owed_again() {
    let h = harness("actions-undo-payment").await;
    northstar(&h).await;
    // Issuing posts to the ledger, so the tenant needs its chart of accounts.
    crate::common::seed_default_chart(&h.acc).await;

    // Draft, issue, pay — every step the person's own tap, so every step is
    // an action row.
    let (status, body) = post(
        &h.app,
        &h.token,
        "/ai/agent/execute",
        json!({ "tool": "create_invoice_draft", "args": draft_args() }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, issued) = post(
        &h.app,
        &h.token,
        "/ai/agent/execute",
        json!({ "tool": "issue_invoice", "args": { "customer": "Northstar Foods BV" } }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{issued}");
    let invoice = issued["result"]["invoice"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let number = issued["result"]["invoice"]["number"]
        .as_str()
        .unwrap()
        .to_owned();
    let (status, paid) = post(
        &h.app,
        &h.token,
        "/ai/agent/execute",
        json!({ "tool": "record_payment", "args": { "invoice": number } }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{paid}");

    let action = newest_listed(&h, "record_payment").await;
    assert_eq!(action["undoable"], true, "{action}");
    let action_id = action["id"].as_str().unwrap().to_owned();
    let (status, undone) = post(
        &h.app,
        &h.token,
        &format!("/ai/actions/{action_id}/undo"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{undone}");

    // Owed again: the money is not there, and the document says so.
    let (status, body) = get(&h.app, &h.token, &format!("/billing/invoices/{invoice}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["invoice"]["status"], "issued", "{body}");
    assert_eq!(
        body["invoice"]["settlement"]["paidCents"], 0,
        "the payment is gone: {body}"
    );
}

/// An undo needs an inverse and your own row: a read has nothing to take
/// back, and another tenant undoing our action id — guessed exactly right —
/// gets the answer an id never issued gets.
#[tokio::test]
async fn an_undo_needs_an_inverse_and_your_own_row() {
    let h = harness("actions-undo-bounds").await;
    northstar(&h).await;

    // A read leaves a row with no inverse — the button refuses it plainly.
    let (status, body) = post(
        &h.app,
        &h.token,
        "/ai/agent/execute",
        json!({ "tool": "open_quotes", "args": {} }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let read_row = newest_listed(&h, "open_quotes").await;
    assert_eq!(read_row["undoable"], false);
    let read_id = read_row["id"].as_str().unwrap().to_owned();
    let (status, refused) = post(
        &h.app,
        &h.token,
        &format!("/ai/actions/{read_id}/undo"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");

    // A write with an undo, for the other tenant to try.
    let (status, body) = post(
        &h.app,
        &h.token,
        "/ai/agent/execute",
        json!({ "tool": "create_invoice_draft", "args": draft_args() }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let action = newest_listed(&h, "create_invoice_draft").await;
    let action_id = action["id"].as_str().unwrap().to_owned();

    let other = harness_on(std::sync::Arc::clone(&h.store), "actions-undo-bounds-b").await;
    let (status, body) = post(
        &other.app,
        &other.token,
        &format!("/ai/actions/{action_id}/undo"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    // Nothing ran: our draft still exists.
    let invoice = action["record"]["id"].as_str().unwrap();
    let (status, body) = get(&h.app, &h.token, &format!("/billing/invoices/{invoice}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

/// **An open proposal is handed to an agent** (A8.2): "@billing, you finish
/// this." Handing is the asker's decision; the execution is attributed to the
/// agent it was handed to, on the asker's behalf, settling the same card. An
/// agent whose product does not own the verb refuses **before** the card is
/// decided, so a mis-hand leaves it open.
#[tokio::test]
async fn an_open_proposal_is_handed_to_an_agent() {
    let h = harness("actions-hand").await;
    northstar(&h).await;
    let agent = billing_agent(&h).await;
    let room = a_room_with(&h, "finance", &agent).await;
    // A second agent of another product, in the same room, for the mis-hand.
    let tasks_handle = alo_store::default_handle(AgentProduct::Tasks);
    let (status, body) = get(&h.app, &h.token, "/chat/agents").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let tasks_agent = body["agents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["handle"] == tasks_handle)
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/chat/channels/{room}/agents"),
        json!({ "agent": tasks_agent }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (model, _seen) = scripted_model(vec![wants(
        "create_invoice_draft",
        draft_args(),
        "Shall I raise the draft?",
    )])
    .await;
    use_model(&h, &model).await;
    let spoken = ask_in_room(&h, &room, "@billing invoice Northstar for the consulting").await;
    let proposal = spoken["proposal"]["id"].as_str().unwrap().to_owned();

    // Handed to the wrong product's agent: refused, and the card stays open.
    let (status, refused) = post(
        &h.app,
        &h.token,
        &format!("/chat/proposals/{proposal}/hand"),
        json!({ "agent": tasks_agent }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");

    // Handed to the agent whose verb it is: decided, executed, attributed.
    let (status, handed) = post(
        &h.app,
        &h.token,
        &format!("/chat/proposals/{proposal}/hand"),
        json!({ "agent": agent }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{handed}");
    assert_eq!(handed["state"], "approved", "{handed}");
    assert_eq!(handed["handedTo"], agent, "{handed}");
    let invoice = handed["result"]["result"]["invoice"]["id"]
        .as_str()
        .expect("the handoff answered the raised draft")
        .to_owned();

    // One action record saying both: the agent acted, on the asker's behalf,
    // settling that card.
    let action = newest_draft_action(&h).await;
    assert_eq!(
        action.agent.as_ref().map(alo_store::ChatAgentId::as_str),
        Some(agent.as_str()),
        "the execution is the handed agent's"
    );
    assert_eq!(
        action
            .proposal
            .as_ref()
            .map(alo_store::ChatProposalId::as_str),
        Some(proposal.as_str())
    );
    assert_eq!(action.record_id.as_deref(), Some(invoice.as_str()));

    // Handing a settled card again is the same refusal a second tap gets.
    let (status, again) = post(
        &h.app,
        &h.token,
        &format!("/chat/proposals/{proposal}/hand"),
        json!({ "agent": agent }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{again}");
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
