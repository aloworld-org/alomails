//! The Finance agent over its intents (AA.2, ADR 0058), on the wire: in a real
//! room, against the real router and store, with a scripted model.
//!
//! Before the move to intents the Finance agent could suggest categories and
//! read a VAT return, but "@finance how much have we invoiced this year, and
//! how much is unpaid?" had no verb to run. This suite holds the opposite: the
//! read runs inside the turn and answers from the receivables ledger of the
//! tenant's own journal — the books, not Billing's document list — a write
//! (approving an expense claim) is proposed, previewed and not run, and
//! another tenant's waiting claims are unreachable from the approvals queue.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use alo_store::{
    AccountStore, AgentProduct, CHART, ChartName, ChartSeed, EntryKind, FxSnapshot, NewEntry,
    NewPosting,
};
use time::{Date, Month};

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

async fn finance_agent(h: &Harness) -> String {
    let handle = alo_store::default_handle(AgentProduct::Finance);
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

fn on(year: i32, month: Month, day: u8) -> Date {
    Date::from_calendar_date(year, month, day).unwrap()
}

/// The default chart, named per tenant so a leak reads as the wrong tag rather
/// than as a plausible number.
async fn chart_for(acc: &AccountStore, tag: &str) {
    acc.fin_accounts_or_seed(
        &ChartSeed {
            names: CHART
                .iter()
                .map(|account| ChartName {
                    code: account.code.to_owned(),
                    name: format!("{tag} {}", account.code),
                })
                .collect(),
        },
        false,
    )
    .await
    .unwrap();
}

/// Posts one balanced entry by account code, in the accounting currency.
async fn post_entry(
    acc: &AccountStore,
    date: Date,
    kind: EntryKind,
    memo: &str,
    lines: &[(&str, i64)],
) {
    let chart = acc.fin_accounts(false).await.unwrap();
    let postings = lines
        .iter()
        .map(|&(code, cents)| {
            let id = chart
                .iter()
                .find(|account| account.code == code)
                .unwrap_or_else(|| panic!("the seeded chart holds {code}"))
                .id
                .clone();
            NewPosting::new(id, cents, cents)
        })
        .collect();
    acc.post_fin_entry(&NewEntry {
        entry_date: date,
        kind,
        source: None,
        memo: memo.to_owned(),
        reverses_entry_id: None,
        attachment_node_id: None,
        currency: "EUR".to_owned(),
        fx: FxSnapshot::identity("EUR", date),
        postings,
    })
    .await
    .unwrap_or_else(|e| panic!("{memo} should post: {e:?}"));
}

/// An admin whose books carry one invoice of 1210.00 gross and one payment of
/// 605.00 against it this year — invoiced 1210.00, paid 605.00, 605.00 open.
async fn books(tag: &str) -> Harness {
    let h = harness(tag).await;
    h.ts.set_admin(&h.user, true).await.unwrap();
    chart_for(&h.acc, tag).await;
    post_entry(
        &h.acc,
        on(2026, Month::March, 3),
        EntryKind::Invoice,
        "INV-2026-00001",
        &[("1100", 121_000), ("4000", -100_000), ("2100", -21_000)],
    )
    .await;
    post_entry(
        &h.acc,
        on(2026, Month::April, 4),
        EntryKind::Payment,
        "INV-2026-00001 part paid",
        &[("1000", 60_500), ("1100", -60_500)],
    )
    .await;
    h
}

/// A submitted claim of the harness user's own, waiting for a decision.
async fn a_waiting_claim(h: &Harness, merchant: &str) -> String {
    let (status, body) = post(
        &h.app,
        &h.token,
        "/finance/expenses",
        json!({ "spentOn": "2026-08-10", "grossCents": 11_900, "vatCents": 1_900,
                "vatRateBp": 1_900, "merchant": merchant, "method": "personal" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let id = body["expense"]["id"].as_str().unwrap().to_owned();
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/finance/expenses/{id}/submit"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    id
}

#[tokio::test]
async fn how_much_we_invoiced_is_answered_from_the_books() {
    let h = books("fin-intents-ledger").await;
    let agent = finance_agent(&h).await;
    let room = a_room_with(&h, "books", &agent).await;

    let (model, seen) = scripted_model(vec![
        wants("ledger_summary", json!({}), "Let me read the ledger."),
        says("Invoiced 1210.00 EUR this year; 605.00 EUR is unpaid [1]."),
    ])
    .await;
    use_model(&h, &model).await;

    let answer = ask_in_room(
        &h,
        &room,
        "@finance how much have we invoiced this year, and how much is unpaid?",
    )
    .await;
    assert_eq!(
        answer["body"],
        "Invoiced 1210.00 EUR this year; 605.00 EUR is unpaid [1]."
    );
    assert!(
        answer["proposal"].is_null(),
        "a read is answered, never proposed"
    );

    // The agent was offered its verbs — reads and writes alike, from the
    // intent registry.
    let prompt = offered(&seen, 0);
    for verb in [
        "ledger_summary",
        "vat_summary",
        "flag_anomalies",
        "unmatched_bank_lines",
        "expenses_awaiting",
        "account_balance",
        "categorise_transactions",
        "approve_expense",
    ] {
        assert!(
            prompt.contains(&format!("- {verb}:")),
            "the prompt does not offer {verb}"
        );
    }
    // The read's figures are the journal's, with the reading beside each
    // integer, and the entries behind them are in the sources.
    let sources = shown(&seen, 1);
    assert!(sources.contains("\"invoicedCents\":121000"), "{sources}");
    assert!(
        sources.contains("\"invoicedDisplay\":\"1210.00 EUR\""),
        "{sources}"
    );
    assert!(sources.contains("\"paidCents\":60500"), "{sources}");
    assert!(sources.contains("\"outstandingCents\":60500"), "{sources}");
    assert!(
        sources.contains("\"outstandingDisplay\":\"605.00 EUR\""),
        "{sources}"
    );
    assert!(sources.contains("INV-2026-00001"), "{sources}");
}

#[tokio::test]
async fn approving_a_claim_is_proposed_and_not_run() {
    let h = books("fin-intents-approve").await;
    let claim = a_waiting_claim(&h, "Bahn").await;
    let agent = finance_agent(&h).await;
    let room = a_room_with(&h, "books", &agent).await;

    let (model, _seen) = scripted_model(vec![wants(
        "approve_expense",
        json!({ "merchant": "Bahn" }),
        "I'll approve the rail claim.",
    )])
    .await;
    use_model(&h, &model).await;

    let answer = ask_in_room(&h, &room, "@finance approve the claim from Bahn").await;
    assert!(
        !answer["proposal"].is_null(),
        "a write is a proposal: {answer}"
    );
    assert_eq!(answer["proposal"]["tool"], "approve_expense");
    // The claim has not been decided: nothing ran without a tap.
    let (status, body) = get(&h.app, &h.token, &format!("/finance/expenses/{claim}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["expense"]["status"], "submitted", "{body}");
}

#[tokio::test]
async fn another_tenants_waiting_claims_are_unreachable() {
    let h = harness("fin-intents-iso-a").await;
    h.ts.set_admin(&h.user, true).await.unwrap();
    let other = harness_on(h.store.clone(), "fin-intents-iso-b").await;
    other.ts.set_admin(&other.user, true).await.unwrap();
    // Tenant B holds the waiting claim; its merchant names a pharmacy, which
    // is exactly the kind of word that must never cross a tenant wall.
    a_waiting_claim(&other, "Glasshouse Pharma").await;
    let agent = finance_agent(&h).await;
    let room = a_room_with(&h, "books", &agent).await;

    let (model, seen) = scripted_model(vec![
        wants("expenses_awaiting", json!({}), "Let me check the queue."),
        says("Nothing is waiting for a decision."),
    ])
    .await;
    use_model(&h, &model).await;

    let answer = ask_in_room(&h, &room, "@finance which expenses are waiting?").await;
    assert_eq!(answer["body"], "Nothing is waiting for a decision.");
    // What the model was shown is tenant A's empty queue and none of tenant
    // B's record — not the merchant, not the amount, not the claimant.
    let sources = shown(&seen, 1);
    assert!(sources.contains("\"expenseCount\":0"), "{sources}");
    assert!(!sources.contains("Glasshouse Pharma"), "{sources}");
    assert!(!sources.contains("11900"), "{sources}");
    assert!(!sources.contains("fin-intents-iso-b"), "{sources}");
}
