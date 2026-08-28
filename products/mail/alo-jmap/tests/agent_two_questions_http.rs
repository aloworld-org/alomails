//! **The two questions, end to end** (A1.7) — the pair ADR 0034 and ADR 0047
//! are actually judged on, asked the way a person asks them and answered the
//! way a person reads them:
//!
//! - `@mail are we in contact with ABC?` — answered from the **correspondence**,
//!   with the asker's own messages numbered behind the answer, and nothing from
//!   another product's records among them.
//! - `@inventory is the X100 in stock?` — answered from the **stock record**,
//!   with **no button in between**: the figure arrives in the room, not a
//!   proposal to look it up.
//!
//! Everything here goes through the product's own path: the tenant's agents are
//! the ones `GET /chat/agents` seeds (A1.5, nobody registers a handle), the room
//! is made over HTTP, the agent joins it over HTTP, and the question is an
//! ordinary chat message. The records both answers come from are real rows —
//! ingested RFC 5322 messages and a stocked product with a receipt on its shelf
//! — written through the same store functions the Mail and Inventory screens
//! use.
//!
//! **No live model is ever called**, here or anywhere in this workspace's tests
//! (the loop's standing rail): the tenant's AI backend is the scripted local
//! socket in `common::model`, which hands back fixture completions in order and
//! records what it was asked. That recording is what makes "answered from the
//! record" checkable at all — the assertions below are about the bytes the model
//! was shown, which is where a grounded answer and a plausible guess differ.
//!
//! Each test prints the exchange it drove — the request, what the model was
//! shown, what it replied, and the message that landed in the room — so the
//! transcript in `docs/autonomy/agents/STATE.md` is copied out of a run rather
//! than written from memory. Run it with
//! `cargo nextest run -p alo-jmap --test agent_two_questions_http --no-capture`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use crate::common::model::{Seen, says, scripted_model, use_model, wants};
use crate::common::{Harness, harness, send};
use alo_store::inv_locations::{Location, LocationKind, LocationSeed};
use alo_store::inv_moves::{MoveReason, NewMove};
use alo_store::inv_reorder::NewReorderRule;
use alo_store::{
    AgentProduct, Contact, ContactField, ContactId, InvLocationId, MessageId, NewProduct,
};

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

/// The id of the tenant's own agent for `product`, out of the set a first look
/// at `GET /chat/agents` seeds (A1.5). Nothing here registers a handle: an agent
/// this test could not find is an agent a person could not find either.
async fn tenants_agent(h: &Harness, product: AgentProduct) -> String {
    let handle = alo_store::default_handle(product);
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

/// A room, with that agent in it — both over HTTP, as a person makes them.
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

/// Every message in the room, newest first as the route answers.
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
///
/// The turn runs off the request on purpose — nobody's own words wait on
/// inference — so the reply has to be waited for. The deadline is a ceiling on a
/// local socket answering instantly; blowing it is a real failure.
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

/// What the model was actually shown on call `n` — the user turn of the request
/// body the scripted backend recorded, which is where the grounding and the tool
/// results live.
fn shown(seen: &Seen, n: usize) -> String {
    turn_content(seen, n, false)
}

/// The system prompt of call `n` — who the agent was told it is, and which
/// tools it was offered (A1.2). A different message from the one above, and the
/// distinction matters: the offer is in the system turn, the evidence in the
/// user turn.
fn offered(seen: &Seen, n: usize) -> String {
    turn_content(seen, n, true)
}

fn turn_content(seen: &Seen, n: usize, system: bool) -> String {
    let asked = seen.lock().unwrap().clone();
    let messages = asked
        .get(n)
        .unwrap_or_else(|| panic!("the model was not called {} times", n + 1))["messages"]
        .as_array()
        .unwrap()
        .clone();
    let message = if system {
        messages.first().unwrap()
    } else {
        messages.last().unwrap()
    };
    message["content"].as_str().unwrap().to_owned()
}

/// Prints one exchange so the queue item's "record the actual request and
/// response" is a copy of a run rather than a claim about one.
fn transcript(title: &str, lines: &[String]) {
    println!("\n===== A1.7 TRANSCRIPT: {title} =====");
    for line in lines {
        println!("{line}");
    }
    println!("===== end: {title} =====\n");
}

// ---- the first question: @mail are we in contact with ABC? -------------------

/// Two real messages from ABC and one about something else, ingested as RFC 5322
/// exactly as delivery does. The third is the control: an answer that quoted it
/// would be answering from a search snippet rather than from the correspondence.
async fn correspondence_with_abc(h: &Harness) -> (MessageId, MessageId, MessageId) {
    let inbox = h.acc.inbox().await.unwrap();
    let first = h
        .acc
        .ingest(
            &inbox,
            b"From: orders@abc-supplies.test\r\nTo: us@example.test\r\n\
              Subject: ABC Supplies - your March delivery\r\n\
              Message-ID: <abc-march@abc-supplies.test>\r\n\
              Date: Mon, 3 Aug 2026 09:12:00 +0000\r\n\r\n\
              The pallets left our warehouse this morning.\r\n",
        )
        .await
        .unwrap();
    let second = h
        .acc
        .ingest(
            &inbox,
            b"From: ilse@abc-supplies.test\r\nTo: us@example.test\r\n\
              Subject: Re: our quote for ABC Supplies\r\n\
              Message-ID: <abc-quote@abc-supplies.test>\r\n\
              Date: Thu, 6 Aug 2026 14:40:00 +0000\r\n\r\n\
              Thanks for the revised quote - we will confirm on Friday.\r\n",
        )
        .await
        .unwrap();
    let other = h
        .acc
        .ingest(
            &inbox,
            b"From: pim@example.test\r\nTo: us@example.test\r\n\
              Subject: lunch on Friday\r\n\
              Message-ID: <lunch@example.test>\r\n\
              Date: Fri, 7 Aug 2026 11:00:00 +0000\r\n\r\n\
              The usual place at one?\r\n",
        )
        .await
        .unwrap();
    // The address book is Mail's own too (A1.3), so the person behind the
    // correspondence is grounding as well as the messages.
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
    (first, second, other)
}

/// **The first question, on the wire.** The Mail agent answers from the record
/// it is grounded in, the numbered sources behind the answer are the asker's own
/// messages, and no lookup and no button were involved.
#[tokio::test]
async fn mail_answers_are_we_in_contact_with_abc_from_the_correspondence() {
    let h = harness("agent-a17-mail").await;
    let (march, quote, lunch) = correspondence_with_abc(&h).await;
    const ANSWER: &str = "Yes — ABC Supplies. Ilse Vermeer wrote on 6 August about the revised quote \
         and will confirm on Friday [1], and their March delivery left the warehouse on the 3rd [2].";
    let (base, seen) = scripted_model(vec![says(ANSWER)]).await;
    use_model(&h, &base).await;
    let agent = tenants_agent(&h, AgentProduct::Mail).await;
    let channel = a_room_with(&h, "front desk", &agent).await;

    const QUESTION: &str = "@mail are we in contact with ABC?";
    let spoken = ask_in_room(&h, &channel, QUESTION).await;

    // What the room got: the answer, said by the agent, with nothing to tap.
    assert_eq!(spoken["body"], json!(ANSWER));
    assert_eq!(spoken["authorKind"], json!("agent"));
    for message in messages(&h, &channel).await {
        assert_eq!(
            message["proposal"],
            Value::Null,
            "an answer must never arrive with a button on it: {message}"
        );
    }

    // One call, so nothing was looked up: this answer is grounding, not a tool.
    assert_eq!(
        seen.lock().unwrap().len(),
        1,
        "a grounded answer costs one call"
    );
    let first = shown(&seen, 0);
    assert!(
        first.contains(QUESTION.trim_start_matches("@mail ")),
        "{first}"
    );
    // The correspondence is in front of it — both messages, by their subjects,
    // numbered, so the answer's `[1]` and `[2]` point at a message each and the
    // citation is checkable rather than decorative.
    assert!(
        first.contains("[1] email \"Re: our quote for ABC Supplies\""),
        "the quote thread is the newest correspondence and cites as [1]: {first}"
    );
    assert!(
        first.contains("[2] email \"ABC Supplies - your March delivery\""),
        "the March delivery cites as [2]: {first}"
    );
    assert!(
        first.contains("Ilse Vermeer"),
        "Mail grounds in its own address book too: {first}"
    );
    // …and nothing else of the asker's is: an unrelated message is the control.
    assert!(
        !first.contains("lunch on Friday"),
        "grounding is the question's correspondence, not the mailbox: {first}"
    );

    // "With the messages behind it": the numbered sources the model was shown
    // are these rows, read back through the same call the route makes, through
    // the asker's own door. Their ids are the ingested messages' ids.
    let ground = h
        .acc
        .agent_ground(AgentProduct::Mail, QUESTION, 8)
        .await
        .unwrap();
    let mail_ids: Vec<&str> = ground
        .iter()
        .filter(|hit| hit.kind == "message")
        .map(|hit| hit.id.as_str())
        .collect();
    assert_eq!(
        mail_ids,
        vec![quote.as_str(), march.as_str()],
        "the sources are the newest correspondence with ABC, and only that"
    );
    assert!(
        !mail_ids.contains(&lunch.as_str()),
        "the unrelated message must not be behind this answer"
    );

    // Nothing was executed, so nothing is audited as having run…
    assert!(
        h.acc.agent_tool_runs(50).await.unwrap().is_empty(),
        "no tool ran, so no run is recorded"
    );
    // …and the agent's record counts one answer and no lookup.
    let record = h.acc.agent_records().await.unwrap();
    let record = record.get(agent.as_str()).unwrap();
    assert_eq!(record.answers, 1);
    assert_eq!(record.reads, 0);
    assert_eq!(record.actions, 0);

    transcript(
        "@mail are we in contact with ABC?",
        &[
            format!("POST /chat/channels/{channel}/messages"),
            format!("     {}", json!({ "body": QUESTION })),
            "--- what the model was shown (call 1 of 1, user turn) ---".to_owned(),
            first,
            "--- what the model replied ---".to_owned(),
            says(ANSWER),
            "--- GET /chat/channels/{id}/messages, the agent's message ---".to_owned(),
            spoken.to_string(),
            format!("--- the messages behind it: {} ---", mail_ids.join(", ")),
        ],
    );
}

// ---- the second question: @inventory is the X100 in stock? -------------------

/// A stocked X100 with twelve on the warehouse shelf, watched at a minimum of
/// four — written through the store functions the Inventory screens use, so the
/// figure the agent reads is the figure the warehouse screen shows.
async fn twelve_x100_on_the_shelf(h: &Harness) -> InvLocationId {
    let seeded = h
        .acc
        .inv_locations_or_seed(
            &LocationSeed {
                stock: "Hoofdmagazijn".to_owned(),
                supplier: "Leveranciers".to_owned(),
                customer: "Klanten".to_owned(),
                adjustment: "Correcties".to_owned(),
                production: "Productie".to_owned(),
            },
            false,
        )
        .await
        .unwrap();
    let of = |kind: LocationKind| -> InvLocationId {
        seeded
            .iter()
            .find(|l: &&Location| l.kind == kind)
            .unwrap_or_else(|| panic!("the seed must write a {kind:?} location"))
            .id
            .clone()
    };
    let warehouse = of(LocationKind::Stock);
    let product = h
        .acc
        .create_billing_product(&NewProduct {
            name: "Vulcan X100 drill".to_owned(),
            unit: "piece".to_owned(),
            unit_price_cents: 24_900,
            vat_rate_bp: 2100,
            sku: "X100".to_owned(),
            stocked: true,
            purchase_price_cents: 14_500,
            ..Default::default()
        })
        .await
        .unwrap();
    h.acc
        .record_move(&NewMove {
            product_id: product.clone(),
            from_location_id: of(LocationKind::Supplier),
            to_location_id: warehouse.clone(),
            qty_milli: 12_000,
            reason: MoveReason::Purchase,
            reason_code: None,
            note: String::new(),
            reference: None,
            occurred_at: None,
        })
        .await
        .unwrap();
    h.acc
        .create_inv_reorder_rule(&NewReorderRule {
            product_id: product,
            location_id: warehouse.clone(),
            min_qty_milli: 4_000,
            target_qty_milli: 20_000,
            active: true,
        })
        .await
        .unwrap();
    warehouse
}

/// **The second question, on the wire, and the sentence the whole wave is for.**
/// The stock figure lands in the room as an answer. Nobody is asked to approve a
/// lookup — the tool ran inside the turn (ADR 0047) — and the number the agent
/// was given is the one on the shelf.
#[tokio::test]
async fn inventory_answers_is_the_x100_in_stock_with_no_button_in_between() {
    let h = harness("agent-a17-inv").await;
    let warehouse = twelve_x100_on_the_shelf(&h).await;
    const ANSWER: &str = "Yes — 12 on the shelf at Hoofdmagazijn, none on order and none promised out, \
         so 12 available. Your minimum there is 4, so you are above it.";
    let (base, seen) = scripted_model(vec![
        wants(
            "stock_answer",
            json!({ "product": "X100" }),
            "Let me check the stock.",
        ),
        says(ANSWER),
    ])
    .await;
    use_model(&h, &base).await;
    let agent = tenants_agent(&h, AgentProduct::Inventory).await;
    let channel = a_room_with(&h, "stockroom", &agent).await;

    const QUESTION: &str = "@inventory is the X100 in stock?";
    let spoken = ask_in_room(&h, &channel, QUESTION).await;

    // The figure, in the room, said once.
    assert_eq!(spoken["body"], json!(ANSWER));
    assert_eq!(spoken["authorKind"], json!("agent"));
    let room = messages(&h, &channel).await;
    assert_eq!(
        room.iter().filter(|m| m["authorKind"] == "agent").count(),
        1,
        "the agent said the sentence it was going to look something up, and then the answer: {room:?}"
    );
    // **No button in between.** Not on the answer, and not on anything else in
    // the room — the sentence A1.7 exists to prove.
    for message in &room {
        assert_eq!(
            message["proposal"],
            Value::Null,
            "asking what is in stock must never produce a proposal: {message}"
        );
    }

    // Two calls: the lookup, then the answer. The second one is holding the
    // stock record itself, with the figures that are actually on the shelf.
    assert_eq!(
        seen.lock().unwrap().len(),
        2,
        "a read costs exactly one further call"
    );
    let first = shown(&seen, 0);
    let second = shown(&seen, 1);
    let system = offered(&seen, 0);
    assert!(
        system.contains("- stock_answer:"),
        "the Inventory agent is offered its own reading tool: {system}"
    );
    assert!(
        !system.contains("- whats_on:"),
        "and only its own product's tools (A1.2): {system}"
    );
    // Its grounding is empty on purpose (A1.3): Inventory reaches its records
    // through the tool, so the first call carries the question and no snippets.
    assert!(
        first.contains("Sources:\n\n") || first.trim_end().ends_with("Sources:"),
        "the stock question is not answered from a search snippet: {first}"
    );
    assert!(
        second.contains("tool result \"stock_answer\""),
        "the tool's own result must be among the sources: {second}"
    );
    assert!(second.contains("\"kind\":\"stockAnswer\""), "{second}");
    assert!(
        second.contains("\"onHandQtyMilli\":12000"),
        "the shelf's own figure grounds the answer: {second}"
    );
    assert!(
        second.contains("\"availableQtyMilli\":12000"),
        "and the available figure the shortage rule uses: {second}"
    );
    assert!(second.contains("\"minQtyMilli\":4000"), "{second}");
    assert!(second.contains("\"belowMinimum\":false"), "{second}");
    assert!(
        second.contains(warehouse.as_str()),
        "the figure is attributed to the place it is on the shelf: {second}"
    );

    // Audited as a read — the agent's, the room's, and successful.
    let runs = h.acc.agent_tool_runs(50).await.unwrap();
    assert_eq!(runs.len(), 1, "{runs:?}");
    assert_eq!(runs[0].tool, "stock_answer");
    assert_eq!(runs[0].effect, "read");
    assert!(runs[0].ok);
    assert_eq!(
        runs[0].agent.as_ref().map(alo_store::ChatAgentId::as_str),
        Some(agent.as_str())
    );
    assert_eq!(
        runs[0]
            .channel
            .as_ref()
            .map(alo_store::ChatChannelId::as_str),
        Some(channel.as_str())
    );
    // …and visible in the agent's record as a lookup nobody approved.
    let record = h.acc.agent_records().await.unwrap();
    let record = record.get(agent.as_str()).unwrap();
    assert_eq!(record.reads, 1);
    assert_eq!(record.answers, 1);
    assert_eq!(record.actions, 0);

    transcript(
        "@inventory is the X100 in stock?",
        &[
            format!("POST /chat/channels/{channel}/messages"),
            format!("     {}", json!({ "body": QUESTION })),
            "--- what the model was shown (call 1 of 2, user turn) ---".to_owned(),
            first,
            "--- what the model replied (call 1) ---".to_owned(),
            wants(
                "stock_answer",
                json!({ "product": "X100" }),
                "Let me check the stock.",
            ),
            "--- what the model was shown (call 2 of 2, user turn) ---".to_owned(),
            second,
            "--- what the model replied (call 2) ---".to_owned(),
            says(ANSWER),
            "--- GET /chat/channels/{id}/messages, the agent's message ---".to_owned(),
            spoken.to_string(),
            format!(
                "--- audited: {} / {} / ok={} ---",
                runs[0].tool, runs[0].effect, runs[0].ok
            ),
        ],
    );
}
