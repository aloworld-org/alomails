//! The Mail agent over its intents (AC.4, ADR 0058), on the wire: in a real
//! room, against the real router and store, with a scripted model.
//!
//! The correspondence half is proven end to end in `agent_correspondence_http`
//! and the address book in `agent_directory_http`; this suite holds what AC.4
//! adds and the rule the module rests on. What waits unread is answered from
//! the mailbox's own counters inside the turn, with no button in between; a
//! draft is only ever a previewed proposal that lands in the asker's own
//! Drafts once they approve — and never sends. And another tenant's mail does
//! not exist for this agent: a message id of theirs earns the words an
//! invented id earns, and not one word of their mail reaches this tenant's
//! model.
//!
//! **No live model is ever called** (the loop's standing rail): the tenant's
//! AI backend is the scripted local socket in `common::model`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::{Duration as Wait, Instant};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use alo_store::{AgentProduct, EmailFilter, EmailQuery, Page, SortDirection};

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

async fn mail_agent(h: &Harness) -> String {
    let handle = alo_store::default_handle(AgentProduct::Mail);
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

/// A public room by this name, with the given agent listening in it.
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
    let deadline = Instant::now() + Wait::from_secs(20);
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
        tokio::time::sleep(Wait::from_millis(50)).await;
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

/// One RFC 5322 message from `from`, delivered into the given mailbox — the
/// same ingest path real delivery uses, so it arrives unread.
async fn deliver(h: &Harness, mailbox: &alo_store::MailboxId, from: &str, subject: &str) {
    let us = h.email.clone();
    h.acc
        .ingest(
            mailbox,
            format!(
                "From: {from}\r\nTo: {us}\r\nSubject: {subject}\r\n\
                 Message-ID: <{}@wire.test>\r\n\
                 Date: Mon, 24 Aug 2026 09:12:00 +0000\r\n\r\n\
                 The body of {subject}.\r\n",
                subject.replace(' ', "-").to_lowercase()
            )
            .as_bytes(),
        )
        .await
        .unwrap();
}

/// The subjects now in the asker's Drafts, over the same store the screen
/// reads. An account that never drafted anything has no Drafts folder yet,
/// which is the same honest zero.
async fn draft_subjects(h: &Harness) -> Vec<String> {
    let Some(drafts) = h.acc.mailbox_by_role("drafts").await.unwrap() else {
        return Vec::new();
    };
    let found = h
        .acc
        .query_emails(&EmailQuery {
            filter: EmailFilter {
                in_mailbox: Some(drafts),
                ..EmailFilter::default()
            },
            sort: SortDirection::Desc,
            page: Page::first(10),
        })
        .await
        .unwrap();
    found.into_iter().map(|summary| summary.subject).collect()
}

#[tokio::test]
async fn whats_unread_is_answered_from_the_mailboxs_own_counters() {
    let h = harness("mail-intents-unread").await;
    let inbox = h.acc.inbox().await.unwrap();
    deliver(
        &h,
        &inbox,
        "orders@abc-supplies.test",
        "Your March delivery",
    )
    .await;
    deliver(&h, &inbox, "pim@example.test", "lunch on Friday").await;

    let agent = mail_agent(&h).await;
    let room = a_room_with(&h, "ask", &agent).await;
    let (model, seen) = scripted_model(vec![
        wants("unread_summary", json!({}), "Let me look at the mailbox."),
        says("Two emails wait unread in your Inbox."),
    ])
    .await;
    use_model(&h, &model).await;

    let answer = ask_in_room(&h, &room, "@mail how much unread mail do I have?").await;
    assert_eq!(answer["body"], "Two emails wait unread in your Inbox.");
    assert!(
        answer["proposal"].is_null(),
        "a read is answered, never proposed"
    );

    // The agent was offered its verbs — the kept twelve and the three AC.4
    // adds, reads and writes alike, from the intent registry.
    let prompt = offered(&seen, 0);
    for verb in [
        "correspondence",
        "message_read",
        "unread_summary",
        "thread_lookup",
        "who_i_emailed",
        "find_contact",
        "mark_read",
        "flag_email",
        "archive_email",
        "trash_email",
        "snooze_email",
        "draft_email",
        "draft_reply",
        "send_email",
        "move_to_folder",
    ] {
        assert!(
            prompt.contains(&format!("- {verb}:")),
            "the prompt does not offer {verb}"
        );
    }
    // The mailbox came back as its own counters: the Inbox by name, two
    // unread — the same numbers the mail screen's folder list shows.
    let sources = shown(&seen, 1);
    assert!(sources.contains("\"totalUnread\":2"), "{sources}");
    assert!(sources.contains("Inbox"), "{sources}");
    assert!(sources.contains("\"unread\":2"), "{sources}");
}

#[tokio::test]
async fn a_draft_waits_for_the_askers_tap_and_never_sends() {
    let h = harness("mail-intents-draft").await;

    let agent = mail_agent(&h).await;
    let room = a_room_with(&h, "ask", &agent).await;
    let (model, _seen) = scripted_model(vec![wants(
        "draft_email",
        json!({
            "to": "ilse@abc-supplies.test",
            "subject": "Our March prices",
            "body": "The March price holds until the end of September.",
        }),
        "I'll draft that to Ilse for you to review.",
    )])
    .await;
    use_model(&h, &model).await;

    let answer = ask_in_room(
        &h,
        &room,
        "@mail draft an email to ilse@abc-supplies.test saying the March price holds",
    )
    .await;
    assert!(
        !answer["proposal"].is_null(),
        "a write is a proposal: {answer}"
    );
    assert_eq!(answer["proposal"]["tool"], "draft_email");
    // Nothing ran without a tap: there is no draft anywhere.
    assert!(
        draft_subjects(&h).await.is_empty(),
        "a draft was saved before approval"
    );

    // The asker approves — and the draft is in their own Drafts with the
    // subject the proposal named, saved and NOT sent.
    let proposal = answer["proposal"]["id"].as_str().unwrap().to_owned();
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/chat/proposals/{proposal}"),
        json!({ "approve": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(draft_subjects(&h).await, ["Our March prices"]);
    // Sent stays empty — a draft never sends itself.
    let sent = h.acc.mailbox_by_role("sent").await.unwrap();
    if let Some(sent) = sent {
        let sent_mail = h
            .acc
            .query_emails(&EmailQuery {
                filter: EmailFilter {
                    in_mailbox: Some(sent),
                    ..EmailFilter::default()
                },
                sort: SortDirection::Desc,
                page: Page::first(10),
            })
            .await
            .unwrap();
        assert!(sent_mail.is_empty(), "approving a draft sent it");
    }
}

#[tokio::test]
async fn another_tenants_mail_does_not_exist_here() {
    let h = harness("mail-intents-iso").await;
    // Another tenant on the same store, with a message whose subject is theirs.
    let other = harness_on(Arc::clone(&h.store), "mail-intents-iso2").await;
    let their_inbox = other.acc.inbox().await.unwrap();
    let them = other.email.clone();
    let their_message = other
        .acc
        .ingest(
            &their_inbox,
            format!(
                "From: board@example.test\r\nTo: {them}\r\n\
                 Subject: the secret merger\r\n\
                 Message-ID: <secret@example.test>\r\n\
                 Date: Mon, 24 Aug 2026 09:12:00 +0000\r\n\r\n\
                 Nobody outside the board may know.\r\n"
            )
            .as_bytes(),
        )
        .await
        .unwrap();

    let agent = mail_agent(&h).await;
    let room = a_room_with(&h, "ask", &agent).await;
    let (model, seen) = scripted_model(vec![
        wants(
            "thread_lookup",
            json!({ "message": their_message.as_str() }),
            "Let me look the conversation up.",
        ),
        says("No message of yours has that id."),
    ])
    .await;
    use_model(&h, &model).await;

    let answer = ask_in_room(&h, &room, "@mail how did that conversation go?").await;
    assert_eq!(answer["body"], "No message of yours has that id.");
    // The other tenant's message earns the words an invented id earns —
    // indistinguishable on purpose — and not one word of their mail reaches
    // this tenant's model.
    let sources = shown(&seen, 1);
    assert!(
        sources.contains("no message of yours has that id"),
        "{sources}"
    );
    assert!(
        !sources.contains("the secret merger"),
        "another tenant's mail leaked into the sources: {sources}"
    );
}
