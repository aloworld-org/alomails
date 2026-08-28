//! **The Mail agent's answer half, on the wire** (A2.8) — the three
//! correspondence questions asked the way a person asks them, answered from the
//! mailbox rather than from whatever a search happened to match:
//!
//! - `@mail are we in contact with ABC Supplies?`
//! - `@mail who last replied to ABC Supplies?`
//! - `@mail what did we promise ABC Supplies?`
//!
//! The distinction the item exists for is invisible in the sentence the agent
//! says and obvious in what it was shown: A1.7 answered the first of these from
//! **retrieval** — two messages whose subject lines matched the words in the
//! question — and could not have answered the other two at all, because a
//! snippet has no direction and no body. Every assertion below is therefore
//! about the bytes the model was given: that they came from a tool result, that
//! the result carries the message ids, and that a message it never opened is
//! marked as one it never opened.
//!
//! Everything goes through the product's own path: the tenant's agents are the
//! ones `GET /chat/agents` seeds (A1.5), the room and the join are HTTP, and the
//! question is an ordinary chat message. The correspondence is real rows —
//! RFC 5322 messages ingested into the Inbox and the Sent folder exactly as
//! delivery and the submission path put them there.
//!
//! **No live model is ever called** (the loop's standing rail): the tenant's AI
//! backend is the scripted local socket in `common::model`.
//!
//! Run it with `cargo nextest run -p alo-jmap --test agent_correspondence_http
//! --no-capture` to print the transcripts.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use crate::common::model::{Seen, says, scripted_model, use_model, wants};
use crate::common::{Harness, harness, send};
use alo_store::{AgentProduct, Contact, ContactField, ContactId, MailboxId, MessageId};

// ---- request helpers ---------------------------------------------------------

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

/// The id of the tenant's own Mail agent, out of the set a first look at
/// `GET /chat/agents` seeds (A1.5). Nothing here registers a handle.
async fn the_mail_agent(h: &Harness) -> String {
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

/// A room, with the Mail agent in it — both over HTTP, as a person makes them.
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

/// Says something in the room and waits for the agent's reply. The turn runs
/// off the request — nobody's own words wait on inference — so the reply has to
/// be waited for; blowing the deadline on a local socket is a real failure.
async fn ask_in_room(h: &Harness, channel: &str, question: &str) -> Value {
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
        let spoken = messages(h, channel)
            .await
            .into_iter()
            .find(|m| m["authorKind"] == "agent");
        if let Some(message) = spoken {
            return message;
        }
        assert!(Instant::now() < deadline, "the agent never spoke");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// What the model was shown on call `n` — the user turn, where the grounding and
/// the tool results live.
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

/// Just the part of a shown turn that came from `tool` — the numbered sources
/// above it are the ordinary retrieval every Mail turn still gets, and telling
/// the two apart is the whole distinction this item is about.
fn tool_result<'a>(shown: &'a str, tool: &str) -> &'a str {
    shown
        .split_once(&format!("tool result \"{tool}\""))
        .unwrap_or_else(|| panic!("no {tool} result among the sources: {shown}"))
        .1
}

/// The system prompt of call `n` — which tools the agent was offered (A1.2).
fn offered(seen: &Seen, n: usize) -> String {
    let asked = seen.lock().unwrap().clone();
    asked[n]["messages"].as_array().unwrap()[0]["content"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// Prints one exchange so the queue item's "record the actual request and
/// response" is a copy of a run rather than a claim about one.
fn transcript(title: &str, lines: &[String]) {
    println!("\n===== A2.8 TRANSCRIPT: {title} =====");
    for line in lines {
        println!("{line}");
    }
    println!("===== end: {title} =====\n");
}

/// Every message in the room carries no button. Said once, asserted in every
/// test: a lookup that arrives as a proposal is the bug ADR 0047 removed.
async fn nothing_to_tap(h: &Harness, channel: &str) {
    for message in messages(h, channel).await {
        assert_eq!(
            message["proposal"],
            Value::Null,
            "an answer must never arrive with a button on it: {message}"
        );
    }
}

// ---- the correspondence ------------------------------------------------------

/// The Sent mailbox, made the way the submission path finds it.
async fn sent_folder(h: &Harness) -> MailboxId {
    let existing = h
        .acc
        .mailboxes(alo_store::Page::first(200))
        .await
        .unwrap()
        .into_iter()
        .find(|m| m.role.as_deref() == Some("sent"));
    match existing {
        Some(box_) => box_.id,
        None => h
            .acc
            .create_mailbox(None, "Sent", Some("sent"))
            .await
            .unwrap(),
    }
}

/// What passed between this account and ABC Supplies: two from them, one from
/// us in between, and one message about something else entirely.
///
/// The last is the control. An answer that quoted it would be answering from
/// the mailbox rather than from the correspondence, which is the same class of
/// mistake as answering from a search snippet.
struct Exchange {
    march: MessageId,
    ours: MessageId,
    quote: MessageId,
    lunch: MessageId,
}

async fn correspondence_with_abc(h: &Harness) -> Exchange {
    // The person behind the company, in the asker's own address book. This is
    // what makes "ABC Supplies" reach `ilse@abc-supplies.test` at all: nobody
    // asking about a company types the hyphen their mailbox stores.
    h.acc
        .create_contact(&Contact {
            id: ContactId::generate(),
            display_name: "Ilse Vermeer".to_owned(),
            first_name: Some("Ilse".to_owned()),
            last_name: Some("Vermeer".to_owned()),
            emails: vec![ContactField {
                kind: Some("work".to_owned()),
                value: "ilse@abc-supplies.test".to_owned(),
            }],
            phones: Vec::new(),
            organization: Some("ABC Supplies".to_owned()),
            job_title: Some("Account manager".to_owned()),
            notes: None,
        })
        .await
        .unwrap();
    let inbox = h.acc.inbox().await.unwrap();
    let sent = sent_folder(h).await;
    let us = h.email.clone();
    let march = h
        .acc
        .ingest(
            &inbox,
            format!(
                "From: orders@abc-supplies.test\r\nTo: {us}\r\n\
                 Subject: ABC Supplies - your March delivery\r\n\
                 Message-ID: <abc-march@abc-supplies.test>\r\n\
                 Date: Mon, 3 Aug 2026 09:12:00 +0000\r\n\r\n\
                 The pallets left our warehouse this morning.\r\n"
            )
            .as_bytes(),
        )
        .await
        .unwrap();
    // Ours — filed in Sent exactly as the submission path files a sent message.
    // This is the half a search snippet cannot see: without it there is no
    // direction, and "who last replied" has no answer.
    let ours = h
        .acc
        .ingest(
            &sent,
            format!(
                "From: {us}\r\nTo: ilse@abc-supplies.test\r\n\
                 Subject: Re: our quote for ABC Supplies\r\n\
                 Message-ID: <our-quote@example.test>\r\n\
                 Date: Wed, 5 Aug 2026 08:30:00 +0000\r\n\r\n\
                 We will hold the March price until the end of September, \
                 and we will deliver within five working days of your order.\r\n"
            )
            .as_bytes(),
        )
        .await
        .unwrap();
    let quote = h
        .acc
        .ingest(
            &inbox,
            format!(
                "From: Ilse Vermeer <ilse@abc-supplies.test>\r\nTo: {us}\r\n\
                 Subject: Re: our quote for ABC Supplies\r\n\
                 Message-ID: <abc-quote@abc-supplies.test>\r\n\
                 Date: Thu, 6 Aug 2026 14:40:00 +0000\r\n\r\n\
                 Thanks for the revised quote - we will confirm on Friday.\r\n"
            )
            .as_bytes(),
        )
        .await
        .unwrap();
    let lunch = h
        .acc
        .ingest(
            &inbox,
            format!(
                "From: pim@example.test\r\nTo: {us}\r\n\
                 Subject: lunch on Friday\r\n\
                 Message-ID: <lunch@example.test>\r\n\
                 Date: Fri, 7 Aug 2026 11:00:00 +0000\r\n\r\n\
                 The usual place at one?\r\n"
            )
            .as_bytes(),
        )
        .await
        .unwrap();
    Exchange {
        march,
        ours,
        quote,
        lunch,
    }
}

// ---- "are we in contact with X" and "who last replied" -----------------------

/// **The first two questions, on the wire.** One lookup answers both: the
/// exchange comes back in both directions, newest first, and the result says in
/// so many words whether there is contact and whose the last word was — so the
/// answer is cited to messages and not to a snippet, and no button appears.
#[tokio::test]
async fn the_mail_agent_answers_from_the_exchange_and_says_who_replied_last() {
    let h = harness("agent-a28-contact").await;
    let exchange = correspondence_with_abc(&h).await;
    const ANSWER: &str = "Yes — Ilse Vermeer at ABC Supplies. She replied last, on 6 August, \
         about the revised quote; before that you wrote to her on the 5th.";
    let (base, seen) = scripted_model(vec![
        wants(
            "correspondence",
            json!({ "who": "ABC Supplies" }),
            "Let me look at the exchange.",
        ),
        says(ANSWER),
    ])
    .await;
    use_model(&h, &base).await;
    let agent = the_mail_agent(&h).await;
    let channel = a_room_with(&h, "front desk", &agent).await;

    const QUESTION: &str = "@mail are we in contact with ABC Supplies, and who replied last?";
    let spoken = ask_in_room(&h, &channel, QUESTION).await;
    assert_eq!(spoken["body"], json!(ANSWER));
    assert_eq!(spoken["authorKind"], json!("agent"));
    nothing_to_tap(&h, &channel).await;

    // The Mail agent is offered its own answer half, and only its own product's
    // tools (A1.2) — the prompt half of the item.
    let system = offered(&seen, 0);
    assert!(system.contains("- correspondence:"), "{system}");
    assert!(system.contains("- message_read:"), "{system}");
    assert!(!system.contains("- stock_answer:"), "{system}");

    // Two calls: the lookup, then the answer. The second one holds the exchange.
    assert_eq!(
        seen.lock().unwrap().len(),
        2,
        "a read costs exactly one further call"
    );
    let second = shown(&seen, 1);
    let result = tool_result(&second, "correspondence");
    assert!(result.contains("\"kind\":\"correspondence\""), "{second}");
    // The two questions, answered as facts in the payload rather than left to
    // be inferred from the shape of a list.
    assert!(result.contains("\"inContact\":true"), "{second}");
    assert!(result.contains("\"lastReplyBy\":\"them\""), "{second}");
    // The company name reached the company's mail: the address book turned
    // "ABC Supplies" into the domain their people write from, which is said in
    // the payload rather than left as a coincidence.
    assert!(
        result.contains("\"lookedFor\":[\"ABC Supplies\",\"abc-supplies.test\"]"),
        "the words asked for, and what the address book says they mean: {second}"
    );

    // **Cited to the messages**: every message of the exchange is there by id,
    // both directions.
    for id in [&exchange.quote, &exchange.ours, &exchange.march] {
        assert!(
            result.contains(id.as_str()),
            "the exchange must carry {} by id: {second}",
            id.as_str()
        );
    }
    assert!(
        result.contains("\"direction\":\"us\""),
        "the message we sent is marked as ours: {second}"
    );

    // **And this is the difference the item is for.** The turn's ordinary
    // retrieval — the numbered sources above the tool result — is whatever a
    // full-text search of the mailbox happened to rank for the words in the
    // question, and what it ranks is not asserted here because it is a search
    // and it moves. The lookup is not a search: the exchange is what passed
    // between these two parties, so the unrelated invitation is absent from it
    // by construction and not by luck.
    assert!(
        !result.contains("lunch on Friday") && !result.contains(exchange.lunch.as_str()),
        "the mailbox is not the correspondence: {second}"
    );

    // Audited as a read — the agent's, the room's, and successful (ADR 0047 §4).
    let runs = h.acc.agent_tool_runs(50).await.unwrap();
    assert_eq!(runs.len(), 1, "{runs:?}");
    assert_eq!(runs[0].tool, "correspondence");
    assert_eq!(runs[0].effect, "read");
    assert!(runs[0].ok);
    let record = h.acc.agent_records().await.unwrap();
    let record = record.get(agent.as_str()).unwrap();
    assert_eq!(record.reads, 1);
    assert_eq!(record.answers, 1);
    assert_eq!(record.actions, 0);

    transcript(
        "@mail are we in contact with ABC Supplies, and who replied last?",
        &[
            format!("POST /chat/channels/{channel}/messages"),
            format!("     {}", json!({ "body": QUESTION })),
            "--- what the model replied (call 1 of 2) ---".to_owned(),
            wants(
                "correspondence",
                json!({ "who": "ABC Supplies" }),
                "Let me look at the exchange.",
            ),
            "--- what the model was shown (call 2 of 2, user turn) ---".to_owned(),
            second,
            "--- what the model replied (call 2) ---".to_owned(),
            says(ANSWER),
            "--- the agent's message ---".to_owned(),
            spoken.to_string(),
        ],
    );
}

// ---- "what did we promise them" ----------------------------------------------

/// **The third question, on the wire.** It cannot be answered from a subject
/// line, so the turn does what a person would: finds the exchange, then opens
/// the message that carries the promise and quotes what it actually says.
#[tokio::test]
async fn what_we_promised_is_read_out_of_the_message_that_says_it() {
    let h = harness("agent-a28-promise").await;
    let exchange = correspondence_with_abc(&h).await;
    const ANSWER: &str = "You told them on 5 August that the March price holds until the end of \
         September and that you would deliver within five working days of their order.";
    let (base, seen) = scripted_model(vec![
        wants(
            "correspondence",
            json!({ "who": "ABC Supplies", "about": "quote" }),
            "Let me find what was said to them.",
        ),
        // The id is the one the first result handed back — nothing here invents
        // one, which is exactly the constraint the tool's description states.
        wants(
            "message_read",
            json!({ "message": exchange.ours.as_str() }),
            "Reading what you wrote.",
        ),
        says(ANSWER),
    ])
    .await;
    use_model(&h, &base).await;
    let agent = the_mail_agent(&h).await;
    let channel = a_room_with(&h, "sales", &agent).await;

    const QUESTION: &str = "@mail what did we promise ABC Supplies?";
    let spoken = ask_in_room(&h, &channel, QUESTION).await;
    assert_eq!(spoken["body"], json!(ANSWER));
    nothing_to_tap(&h, &channel).await;

    assert_eq!(seen.lock().unwrap().len(), 3, "two reads, then the answer");
    let second = shown(&seen, 1);
    let third = shown(&seen, 2);
    let exchange_result = tool_result(&second, "correspondence");
    let read_result = tool_result(&third, "message_read");
    // The narrowing was honoured: `about` is reported back, and the message
    // that does not match the words is not in the exchange at all.
    assert!(exchange_result.contains("\"about\":\"quote\""), "{second}");
    assert!(
        !exchange_result.contains(exchange.march.as_str()),
        "the March delivery says nothing about the quote: {second}"
    );
    // The promise itself is in the third call, out of the message body — not
    // out of the subject line, and not out of a preview of somebody else's
    // message.
    assert!(read_result.contains("\"kind\":\"messageRead\""), "{third}");
    assert!(
        read_result.contains("within five working days"),
        "the words the agent answers with are the message's own: {third}"
    );
    assert!(
        read_result.contains(exchange.ours.as_str()),
        "and the message is named by the id it was read from: {third}"
    );
    // The quote reply is in the exchange because it matches the words; it is
    // there as a message, not as a claim about what it says.
    assert!(
        exchange_result.contains(exchange.quote.as_str()),
        "{second}"
    );

    // Both lookups audited, neither approved by anybody.
    let runs = h.acc.agent_tool_runs(50).await.unwrap();
    let mut tools: Vec<&str> = runs.iter().map(|run| run.tool.as_str()).collect();
    tools.sort_unstable();
    assert_eq!(tools, ["correspondence", "message_read"]);
    assert!(runs.iter().all(|run| run.effect == "read" && run.ok));
    let record = h.acc.agent_records().await.unwrap();
    assert_eq!(record.get(agent.as_str()).unwrap().reads, 2);

    transcript(
        "@mail what did we promise ABC Supplies?",
        &[
            format!("POST /chat/channels/{channel}/messages"),
            format!("     {}", json!({ "body": QUESTION })),
            "--- what the model was shown (call 2 of 3, user turn) ---".to_owned(),
            second,
            "--- what the model was shown (call 3 of 3, user turn) ---".to_owned(),
            third,
            "--- what the model replied (call 3) ---".to_owned(),
            says(ANSWER),
            "--- the agent's message ---".to_owned(),
            spoken.to_string(),
        ],
    );
}

// ---- what a listed message is, and is not ------------------------------------

/// A message the lookup listed but did not open is marked as one it did not
/// open — the property the guidance rests on. Without it the model reads a
/// subject line and a body as the same kind of evidence, which is the failure
/// this whole item is about.
#[tokio::test]
async fn a_message_that_was_only_listed_says_so_and_carries_no_text() {
    let h = harness("agent-a28-listed").await;
    let inbox = h.acc.inbox().await.unwrap();
    let us = h.email.clone();
    // Five from the same correspondent: the newest three are opened, the rest
    // are listed. The oldest carries a sentence nothing may quote.
    for day in 1..=5u8 {
        h.acc
            .ingest(
                &inbox,
                format!(
                    "From: ilse@abc-supplies.test\r\nTo: {us}\r\n\
                     Subject: ABC note {day}\r\n\
                     Message-ID: <abc-{day}@abc-supplies.test>\r\n\
                     Date: 0{day} Aug 2026 09:00:00 +0000\r\n\r\n\
                     Body of note {day}: the secret word is aubergine{day}.\r\n"
                )
                .as_bytes(),
            )
            .await
            .unwrap();
    }
    let (base, seen) = scripted_model(vec![
        wants(
            "correspondence",
            json!({ "who": "abc-supplies.test" }),
            "Let me look.",
        ),
        says("They have written to you five times, most recently on 5 August."),
    ])
    .await;
    use_model(&h, &base).await;
    let agent = the_mail_agent(&h).await;
    let channel = a_room_with(&h, "desk", &agent).await;

    ask_in_room(&h, &channel, "@mail what has ABC been writing about?").await;
    let second = shown(&seen, 1);
    let result = tool_result(&second, "correspondence");
    assert!(result.contains("\"opened\":true"), "{second}");
    assert!(
        result.contains("\"opened\":false"),
        "beyond the nearest few, a message is listed and not read: {second}"
    );
    assert!(result.contains("\"openedAtMost\":3"), "{second}");
    // The three newest bodies are there…
    for day in 3..=5u8 {
        assert!(
            result.contains(&format!("aubergine{day}")),
            "note {day} is among the opened ones: {second}"
        );
    }
    // …and the ones that were only listed carry their subject and nothing else.
    for day in 1..=2u8 {
        assert!(
            result.contains(&format!("ABC note {day}")),
            "note {day} is listed: {second}"
        );
        assert!(
            !result.contains(&format!("aubergine{day}")),
            "note {day} was never opened, so its words must not be shown: {second}"
        );
    }
    nothing_to_tap(&h, &channel).await;
}

// ---- isolation ---------------------------------------------------------------

/// **The wrong tenant reaches nothing.** Two workspaces write to the same
/// company; each agent's exchange is its own, and a message id from the other
/// one is refused by name rather than read.
///
/// This is the property the whole answer half rests on: both tools run on the
/// asker's own account door, so an agent sees exactly the mail the person who
/// asked could open — never a colleague's and never another tenant's.
#[tokio::test]
async fn one_tenants_correspondence_is_never_another_tenants() {
    let ours = harness("agent-a28-mine").await;
    let theirs = harness("agent-a28-yours").await;
    let their_inbox = theirs.acc.inbox().await.unwrap();
    let their_message = theirs
        .acc
        .ingest(
            &their_inbox,
            format!(
                "From: ilse@abc-supplies.test\r\nTo: {}\r\n\
                 Subject: ABC Supplies - the other tenant's quote\r\n\
                 Message-ID: <abc-theirs@abc-supplies.test>\r\n\
                 Date: Thu, 6 Aug 2026 14:40:00 +0000\r\n\r\n\
                 This body belongs to somebody else's workspace.\r\n",
                theirs.email
            )
            .as_bytes(),
        )
        .await
        .unwrap();
    let mine = correspondence_with_abc(&ours).await;

    // Our own exchange, through our own palette (`Ask alo`, which is offered
    // every product's tools — so nothing here is narrowed by product scope and
    // the isolation is the store's alone).
    let (status, body) = post(
        &ours.app,
        &ours.token,
        "/ai/agent/execute",
        json!({ "tool": "correspondence", "args": { "who": "abc-supplies.test" } }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let listed = body["result"]["messages"].as_array().unwrap();
    let ids: Vec<&str> = listed
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(ids.contains(&mine.quote.as_str()), "{body}");
    assert!(
        !ids.contains(&their_message.as_str()),
        "another tenant's message must not be in this exchange: {body}"
    );
    assert!(
        !body.to_string().contains("somebody else's workspace"),
        "and none of its words either: {body}"
    );

    // …and naming it outright is a refusal, not a read. The store's scoping is
    // the check: another tenant's id, another user's id and an invented id are
    // equally not this account's message.
    for stranger in [their_message.as_str(), "not-an-id"] {
        let (status, body) = post(
            &ours.app,
            &ours.token,
            "/ai/agent/execute",
            json!({ "tool": "message_read", "args": { "message": stranger } }),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "reading {stranger} must be refused: {body}"
        );
        assert_eq!(
            body["detail"], "that is not one of your messages",
            "and refused in the same words whatever the id was: {body}"
        );
    }

    // The other tenant's own agent still reads its own message perfectly well —
    // so what failed above was the scoping and not the tool.
    let (status, body) = post(
        &theirs.app,
        &theirs.token,
        "/ai/agent/execute",
        json!({ "tool": "message_read", "args": { "message": their_message.as_str() } }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body["result"]["text"]
            .as_str()
            .unwrap()
            .contains("somebody else's workspace"),
        "{body}"
    );
}

// ---- arguments ---------------------------------------------------------------

/// The refusals a bad argument earns, in the executor's own words — which is
/// what the turn hands back to the model so it can correct itself rather than
/// answer around a lookup that silently did nothing.
#[tokio::test]
async fn a_lookup_with_nobody_named_is_refused_in_words_the_model_can_act_on() {
    let h = harness("agent-a28-args").await;
    for (tool, args, detail) in [
        ("correspondence", json!({}), "say who, by name or address"),
        (
            "correspondence",
            json!({ "who": "   " }),
            "say who, by name or address",
        ),
        (
            "message_read",
            json!({ "who": "ABC" }),
            "say which message, by the id a correspondence result gave",
        ),
    ] {
        let (status, body) = post(
            &h.app,
            &h.token,
            "/ai/agent/execute",
            json!({ "tool": tool, "args": args }),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
        assert_eq!(body["detail"], detail, "{body}");
    }
    // Nobody to be in contact with is an empty exchange rather than an error:
    // "no, you have never written to them" is an answer.
    let (status, body) = post(
        &h.app,
        &h.token,
        "/ai/agent/execute",
        json!({ "tool": "correspondence", "args": { "who": "nobody@nowhere.test" } }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["inContact"], json!(false));
    assert_eq!(body["result"]["lastReplyBy"], Value::Null);
    assert!(body["result"]["messages"].as_array().unwrap().is_empty());
}
