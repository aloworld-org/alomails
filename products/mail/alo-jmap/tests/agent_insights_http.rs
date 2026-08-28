//! **The Insights agent, end to end** (A2.4) — the three sentences ADR 0034 and
//! ADR 0047 leave a figures agent to prove, each asked the way a person asks it
//! and answered the way a person reads it:
//!
//! - `@insights how much have we billed?` — answered **from the figures**, with
//!   the measure, the dataset and the period the number came from, and **no
//!   button in between**;
//! - `@insights what changed between May and July?` — answered with what moved,
//!   biggest movement first, a method that appeared counted from zero rather
//!   than dropped, and the two periods' totals beside it;
//! - `@insights build me a report` — a board that **waits for a tap**, and when
//!   it lands it is an ordinary Insights board with ordinary tiles, readable
//!   over the same routes a hand-built one is.
//!
//! And the sentences the wave holds every agent to: a chart the validator
//! refuses never reaches a board, a comparison that has nothing to compare is
//! refused by name, and another tenant's figures are not in the answer.
//!
//! Everything goes through the product's own path: the tenant's agents are the
//! ones `GET /chat/agents` seeds (A1.5), the room is made over HTTP, the agent
//! joins it over HTTP, and the question is an ordinary chat message. The figures
//! underneath are real invoices and real payments, issued and recorded over the
//! ordinary billing routes.
//!
//! **No live model is ever called**, here or anywhere in this workspace's tests
//! (the loop's standing rail): the tenant's AI backend is the scripted local
//! socket in `common::model`. The assertions below are about the bytes the model
//! was *shown*, which is where a figure read out of the books and a plausible
//! guess differ.
//!
//! Run the transcript with
//! `cargo nextest run -p alo-jmap --test agent_insights_http --no-capture`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use alo_store::AgentProduct;
use common::model::{Seen, says, scripted_model, use_model, wants};
use common::{Harness, harness, send};

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

/// The id of the tenant's own Insights agent, out of the set a first look at
/// `GET /chat/agents` seeds (A1.5). Nothing here registers a handle: an agent
/// this test could not find is an agent a person could not find either.
async fn the_insights_agent(h: &Harness) -> String {
    let handle = alo_store::default_handle(AgentProduct::Insights);
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

/// The asker's own tap on a proposal — the only thing that makes a change happen.
async fn approve(h: &Harness, proposal: &str) -> Value {
    let (status, decided) = post(
        &h.app,
        &h.token,
        &format!("/chat/proposals/{proposal}"),
        json!({ "approve": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{decided}");
    decided
}

/// One approved tool run over the ordinary approval route — `POST
/// /ai/agent/execute`, the same path the command palette's button takes, which
/// is an approval the caller gave with their own session.
///
/// The tests that use it are about **arguments and refusals**, which a chat turn
/// cannot vary as finely as they need; the chat path itself is what the room
/// exercises.
async fn run(h: &Harness, tool: &str, args: Value) -> (StatusCode, Value) {
    post(
        &h.app,
        &h.token,
        "/ai/agent/execute",
        json!({ "tool": tool, "args": args }),
    )
    .await
}

/// The `detail` of a refusal, which is the sentence the client shows.
fn why(body: &Value) -> String {
    body["detail"].as_str().unwrap_or_default().to_owned()
}

/// What the model was shown on call `n` — the user turn of the recorded request,
/// which is where the grounding and the tool results live.
fn shown(seen: &Seen, n: usize) -> String {
    turn_content(seen, n, false)
}

/// The system prompt of call `n` — who the agent was told it is, and which tools
/// it was offered (A1.2).
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
    println!("\n===== A2.4 TRANSCRIPT: {title} =====");
    for line in lines {
        println!("{line}");
    }
    println!("===== end: {title} =====\n");
}

// ---- the books underneath -----------------------------------------------------

/// A customer of this tenant's, over the ordinary route.
async fn a_customer(h: &Harness, name: &str) -> String {
    let (status, body) = post(
        &h.app,
        &h.token,
        "/billing/customers",
        json!({
            "name": name,
            "addressLine1": "Hauptstraße 1",
            "postalCode": "10115",
            "city": "Berlin",
            "country": "DE",
            "paymentTermsDays": 14,
            "currency": "EUR",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["customer"]["id"].as_str().unwrap().to_owned()
}

/// One issued invoice, and the net it carries in cents — the figure every
/// answer below is checked against.
async fn an_issued_invoice(h: &Harness, customer: &str, price: i64) -> (String, i64) {
    let (status, body) = post(
        &h.app,
        &h.token,
        "/billing/invoices",
        json!({ "customerId": customer, "lines": [
            { "description": "Consulting", "unit": "hour", "qtyMilli": 1_000,
              "unitPriceCents": price, "vatRateBp": 2_100 },
        ] }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let id = body["invoice"]["id"].as_str().unwrap().to_owned();
    let net = body["invoice"]["totals"]["netCents"].as_i64().unwrap();
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/billing/invoices/{id}/issue"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    (id, net)
}

/// Money that arrived on a stated day, by a stated method — which is what makes
/// two periods with real figures in both of them possible.
async fn a_payment(h: &Harness, invoice: &str, cents: i64, on: &str, method: &str) {
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/billing/invoices/{invoice}/payments"),
        json!({ "amountCents": cents, "paidOn": on, "method": method }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

// ---- the specifications the scripted model "writes" ---------------------------

/// Everything billed, ever: one figure, no breakdown.
fn everything_billed() -> Value {
    json!({
        "schema_version": 1,
        "dataset": "billing.documents",
        "measure": { "id": "net", "agg": "sum" },
        "period": { "kind": "all" },
        "viz": "number",
    })
}

/// Money that arrived in one month, by how it arrived.
fn payments_by_method(from: &str, to: &str) -> Value {
    json!({
        "schema_version": 1,
        "dataset": "billing.payments",
        "measure": { "id": "amount", "agg": "sum" },
        "dimension": { "id": "method" },
        "period": { "kind": "range", "from": from, "to": to },
        "viz": "bar",
    })
}

// ---- the three questions ------------------------------------------------------

/// **"How much have we billed?"** — the sentence A2.4 is named after. The agent
/// looks up the vocabulary, asks the question of the books, and answers with the
/// figure; there is no button in between, because adding up what a business
/// billed does not change anything.
#[tokio::test]
async fn the_insights_agent_answers_from_the_numbers_with_no_button_in_between() {
    let h = harness("agent-a24-answer").await;
    common::seed_default_chart(&h.acc).await;
    let customer = a_customer(&h, "Acme GmbH").await;
    let (_, net) = an_issued_invoice(&h, &customer, 100_000).await;
    assert_eq!(net, 100_000, "the invoice underneath");

    const ANSWER: &str =
        "You have billed 100000 cents net in total (EUR), across every period [2].";
    let (base, seen) = scripted_model(vec![
        wants(
            "insight_catalog",
            json!({}),
            "Let me look up what I can measure.",
        ),
        wants(
            "insight_answer",
            json!({ "spec": everything_billed() }),
            "Let me add up what you have billed.",
        ),
        says(ANSWER),
    ])
    .await;
    use_model(&h, &base).await;
    let agent = the_insights_agent(&h).await;
    let channel = a_room_with(&h, "the numbers", &agent).await;

    const QUESTION: &str = "@insights how much have we billed?";
    let spoken = ask_in_room(&h, &channel, QUESTION).await;

    assert_eq!(spoken["body"], json!(ANSWER));
    assert_eq!(spoken["authorKind"], json!("agent"));
    // **No button in between** — not on the answer, and not on anything else in
    // the room. A question about the figures is a lookup, not a change.
    let room = messages(&h, &channel).await;
    for message in &room {
        assert_eq!(
            message["proposal"],
            Value::Null,
            "asking what the figures say must never produce a proposal: {message}"
        );
    }

    assert_eq!(
        seen.lock().unwrap().len(),
        3,
        "two reads cost two further calls"
    );
    let system = offered(&seen, 0);
    assert!(
        system.contains("- insight_catalog:") && system.contains("- insight_answer:"),
        "the Insights agent is offered its own reading tools: {system}"
    );
    assert!(
        !system.contains("- create_invoice_draft:") && !system.contains("- doc_rewrite:"),
        "and only its own product's tools (A1.2): {system}"
    );
    // The vocabulary reached the model, generated from the catalog itself.
    let after_catalog = shown(&seen, 1);
    assert!(
        after_catalog.contains("tool result \"insight_catalog\""),
        "{after_catalog}"
    );
    assert!(after_catalog.contains("\"kind\":\"insightCatalog\""));
    assert!(
        after_catalog.contains("billing.documents") && after_catalog.contains("\"id\":\"net\""),
        "the menu names the dataset and the measure the next call uses: {after_catalog}"
    );
    // …whole, rather than cut off mid-word by the turn's result bound.
    assert!(
        !after_catalog.contains("result truncated"),
        "the catalog must fit in one tool result: {after_catalog}"
    );

    // **The figure, with the question it answers.** This is the assertion the
    // item is named after: a number a person can check against their own books.
    let after_answer = shown(&seen, 2);
    assert!(
        after_answer.contains("\"kind\":\"insightAnswer\""),
        "{after_answer}"
    );
    assert!(
        after_answer.contains("\"value\":100000"),
        "the net of the issued invoice, in cents: {after_answer}"
    );
    assert!(
        after_answer.contains("\"measure\":\"net\"")
            && after_answer.contains("\"dataset\":\"billing.documents\""),
        "a figure never travels without its question: {after_answer}"
    );
    assert!(
        after_answer.contains("\"kind\":\"money\"")
            && after_answer.contains("\"currency\":\"EUR\""),
        "cents, in a named currency: {after_answer}"
    );

    // Audited as reads — the agent's, the room's, and successful.
    let runs = h.acc.agent_tool_runs(50).await.unwrap();
    assert_eq!(runs.len(), 2, "{runs:?}");
    for record in &runs {
        assert_eq!(record.effect, "read");
        assert!(record.ok);
    }
    let record = h.acc.agent_records().await.unwrap();
    let record = record.get(agent.as_str()).unwrap();
    assert_eq!(record.reads, 2);
    assert_eq!(record.answers, 1);
    assert_eq!(record.actions, 0);

    // Nothing was written: asking what the figures say leaves the tenant with
    // the board it had, which is none.
    let (status, boards) = get(&h.app, &h.token, "/insights/dashboards").await;
    assert_eq!(status, StatusCode::OK, "{boards}");
    assert!(
        boards["dashboards"]
            .as_array()
            .unwrap()
            .iter()
            .all(|board| board["name"] != json!("Revenue")),
        "a question pinned nothing: {boards}"
    );

    transcript(
        QUESTION,
        &[
            format!("POST /chat/channels/{channel}/messages"),
            format!("     {}", json!({ "body": QUESTION })),
            "--- what the model was shown (call 2 of 3, user turn) ---".to_owned(),
            after_catalog,
            "--- what the model was shown (call 3 of 3, user turn) ---".to_owned(),
            after_answer,
            "--- what the model replied (call 3) ---".to_owned(),
            says(ANSWER),
            format!("--- GET /chat/channels/{channel}/messages, the agent's message ---"),
            json!({
                "authorKind": spoken["authorKind"],
                "body": spoken["body"],
                "proposal": spoken["proposal"],
            })
            .to_string(),
            format!(
                "--- audited: {} / {} / ok={} ---",
                runs[1].tool, runs[1].effect, runs[1].ok
            ),
        ],
    );
}

/// **"What changed?"** — the same question over two months, answered with what
/// moved and by how much. A method that was not there in the earlier month
/// counts from zero rather than being dropped, which is the movement being
/// asked about.
#[tokio::test]
async fn the_insights_agent_explains_a_change_biggest_movement_first() {
    let h = harness("agent-a24-change").await;
    common::seed_default_chart(&h.acc).await;
    let customer = a_customer(&h, "Acme GmbH").await;
    let (invoice, _) = an_issued_invoice(&h, &customer, 100_000).await;
    a_payment(&h, &invoice, 40_000, "2026-05-10", "transfer").await;
    a_payment(&h, &invoice, 10_000, "2026-07-10", "transfer").await;
    a_payment(&h, &invoice, 60_000, "2026-07-20", "card").await;

    const ANSWER: &str = "Between May and July, card went from 0 to 60000 cents and transfer fell \
                          from 40000 to 10000 — 70000 in July against 40000 in May [2].";
    let (base, seen) = scripted_model(vec![
        wants(
            "insight_catalog",
            json!({}),
            "Let me look up what I can measure.",
        ),
        wants(
            "insight_change",
            json!({
                "spec": payments_by_method("2026-07-01", "2026-07-31"),
                "against": { "kind": "range", "from": "2026-05-01", "to": "2026-05-31" },
            }),
            "Let me compare the two months.",
        ),
        says(ANSWER),
    ])
    .await;
    use_model(&h, &base).await;
    let agent = the_insights_agent(&h).await;
    let channel = a_room_with(&h, "the numbers", &agent).await;

    const QUESTION: &str = "@insights what changed in the money coming in between May and July?";
    let spoken = ask_in_room(&h, &channel, QUESTION).await;
    assert_eq!(spoken["body"], json!(ANSWER));
    for message in &messages(&h, &channel).await {
        assert_eq!(message["proposal"], Value::Null, "{message}");
    }

    let compared = shown(&seen, 2);
    assert!(
        compared.contains("\"kind\":\"insightChange\""),
        "{compared}"
    );
    // **Biggest movement first, either direction**, each with both figures.
    let movers: Value = {
        let start = compared.find("\"movers\":").expect("the movements");
        let rest = &compared[start + "\"movers\":".len()..];
        let end = rest.find("],").expect("the end of the movements") + 1;
        serde_json::from_str(&rest[..end]).expect("the movements are JSON")
    };
    assert_eq!(movers[0]["bucket"], json!("card"));
    assert_eq!(movers[0]["before"], json!(0));
    assert_eq!(movers[0]["now"], json!(60_000));
    assert_eq!(movers[0]["change"], json!(60_000));
    assert_eq!(movers[1]["bucket"], json!("transfer"));
    assert_eq!(movers[1]["before"], json!(40_000));
    assert_eq!(movers[1]["now"], json!(10_000));
    assert_eq!(movers[1]["change"], json!(-30_000));
    assert_eq!(movers.as_array().unwrap().len(), 2);
    // …and the two periods' own totals beside them, so a fall in one method is
    // not read as a fall overall.
    assert!(
        compared.contains("\"now\":70000") && compared.contains("\"before\":40000"),
        "the totals of both periods: {compared}"
    );
    // The earlier period travels with the answer: a movement between two
    // unnamed periods is not an explanation.
    assert!(
        compared.contains("2026-05-01") && compared.contains("2026-07-31"),
        "{compared}"
    );

    let runs = h.acc.agent_tool_runs(50).await.unwrap();
    assert_eq!(runs.len(), 2, "{runs:?}");
    assert_eq!(runs[0].tool, "insight_change");
    assert_eq!(runs[0].effect, "read");
    assert!(runs[0].ok);
    let _ = agent;

    transcript(
        QUESTION,
        &[
            format!("POST /chat/channels/{channel}/messages"),
            format!("     {}", json!({ "body": QUESTION })),
            "--- what the model was shown (call 3 of 3, user turn) ---".to_owned(),
            compared,
            "--- what the model replied (call 3) ---".to_owned(),
            says(ANSWER),
            format!(
                "--- audited: {} / {} / ok={} ---",
                runs[0].tool, runs[0].effect, runs[0].ok
            ),
        ],
    );
}

/// **"Build me a report"** — the one write. It waits for a tap, and what lands
/// is an ordinary board with ordinary tiles: the same routes a hand-built one is
/// read through, and the same specs a person could have built by hand.
#[tokio::test]
async fn a_report_waits_for_a_tap_and_lands_as_an_ordinary_board() {
    let h = harness("agent-a24-report").await;
    common::seed_default_chart(&h.acc).await;
    let customer = a_customer(&h, "Acme GmbH").await;
    an_issued_invoice(&h, &customer, 100_000).await;

    let charts = json!({
        "name": "Revenue",
        "charts": [
            { "title": "Billed, all time", "spec": everything_billed() },
            { "title": "Money in, July", "spec": payments_by_method("2026-07-01", "2026-07-31") },
        ],
    });
    let (base, seen) = scripted_model(vec![
        wants(
            "insight_catalog",
            json!({}),
            "Let me look up what I can measure.",
        ),
        wants(
            "insight_report",
            charts.clone(),
            "I will put together a Revenue report with two charts.",
        ),
        says("The report is ready to save."),
    ])
    .await;
    use_model(&h, &base).await;
    let agent = the_insights_agent(&h).await;
    let channel = a_room_with(&h, "the numbers", &agent).await;

    const QUESTION: &str = "@insights build me a revenue report";
    let spoken = ask_in_room(&h, &channel, QUESTION).await;
    let proposal = spoken["proposal"]["id"]
        .as_str()
        .expect("a write is proposed, never run")
        .to_owned();
    assert_eq!(spoken["proposal"]["tool"], json!("insight_report"));
    assert_eq!(
        spoken["body"],
        json!("I will put together a Revenue report with two charts.")
    );

    // **Nothing has happened yet.** No board of that name exists until the tap.
    let (_, before) = get(&h.app, &h.token, "/insights/dashboards").await;
    assert!(
        before["dashboards"]
            .as_array()
            .unwrap()
            .iter()
            .all(|board| board["name"] != json!("Revenue")),
        "a proposal is not a board: {before}"
    );

    let decided = approve(&h, &proposal).await;
    let result = &decided["result"]["result"];
    assert_eq!(result["kind"], json!("insightReport"));
    assert_eq!(result["report"]["name"], json!("Revenue"));
    assert_eq!(result["charts"].as_array().unwrap().len(), 2);
    assert_eq!(result["charts"][0]["title"], json!("Billed, all time"));
    assert_eq!(result["charts"][0]["viz"], json!("number"));
    assert_eq!(result["charts"][1]["viz"], json!("bar"));
    let board = result["report"]["id"].as_str().unwrap().to_owned();

    // **An ordinary board, read the ordinary way.** The tiles carry the specs
    // that were proposed, in the order they were proposed, and evaluate through
    // the same route the builder's preview uses.
    let (status, opened) = get(&h.app, &h.token, &format!("/insights/dashboards/{board}")).await;
    assert_eq!(status, StatusCode::OK, "{opened}");
    assert_eq!(opened["dashboard"]["name"], json!("Revenue"));
    let tiles = opened["tiles"].as_array().unwrap();
    assert_eq!(tiles.len(), 2, "{opened}");
    assert_eq!(tiles[0]["title"], json!("Billed, all time"));
    assert_eq!(tiles[0]["spec"], everything_billed());
    assert_eq!(
        tiles[0]["span"],
        json!(1),
        "a single figure is a small card"
    );
    assert_eq!(tiles[1]["title"], json!("Money in, July"));
    assert_eq!(tiles[1]["span"], json!(2), "a chart wants room to be read");

    let tile = tiles[0]["id"].as_str().unwrap();
    let (status, drawn) = get(&h.app, &h.token, &format!("/insights/tiles/{tile}/data")).await;
    assert_eq!(status, StatusCode::OK, "{drawn}");
    assert_eq!(drawn["series"][0]["points"][0]["value"], json!(100_000));

    // Audited: one read and one write, the write only after the tap.
    let runs = h.acc.agent_tool_runs(50).await.unwrap();
    assert_eq!(runs.len(), 2, "{runs:?}");
    assert_eq!(runs[0].tool, "insight_report");
    assert_eq!(runs[0].effect, "write");
    assert!(runs[0].ok);
    let record = h.acc.agent_records().await.unwrap();
    let record = record.get(agent.as_str()).unwrap();
    assert_eq!(record.actions, 1);

    transcript(
        QUESTION,
        &[
            format!("POST /chat/channels/{channel}/messages"),
            format!("     {}", json!({ "body": QUESTION })),
            "--- what the model proposed (call 2 of 2) ---".to_owned(),
            wants(
                "insight_report",
                charts,
                "I will put together a Revenue report with two charts.",
            ),
            format!("--- POST /chat/proposals/{proposal} {{\"approve\":true}} ---"),
            result.to_string(),
            format!("--- GET /insights/dashboards/{board} ---"),
            opened.to_string(),
            format!(
                "--- the model was called {} times ---",
                seen.lock().unwrap().len()
            ),
        ],
    );
}

// ---- what the tools refuse ----------------------------------------------------

/// The refusals, over the ordinary approval route: a specification the validator
/// will not have, a comparison with nothing to compare, and a report that would
/// have pinned a chart nobody can answer.
///
/// Each is a `422` carrying the sentence a person reads and a model can correct
/// itself from — and **none of them writes anything**.
#[tokio::test]
async fn a_question_the_catalog_cannot_answer_is_refused_by_name_and_pins_nothing() {
    let h = harness("agent-a24-refusals").await;
    common::seed_default_chart(&h.acc).await;
    let customer = a_customer(&h, "Acme GmbH").await;
    an_issued_invoice(&h, &customer, 100_000).await;

    // A measure this build has not got. The validator's own sentence survives
    // the trip, which is what a second attempt needs.
    let mut invented = everything_billed();
    invented["measure"]["id"] = json!("profit");
    let (status, body) = run(&h, "insight_answer", json!({ "spec": invented.clone() })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(why(&body).contains("profit"), "{}", why(&body));

    // A question with no specification at all.
    let (status, body) = run(&h, "insight_answer", json!({})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(why(&body).contains("spec is required"), "{}", why(&body));

    // A change broken down by date: two periods bucketed by month share no
    // bucket keys, so there is nothing to compare and the refusal says so.
    let mut over_dates = everything_billed();
    over_dates["dimension"] = json!({ "id": "issue_date", "grain": "month" });
    over_dates["viz"] = json!("line");
    let (status, body) = run(
        &h,
        "insight_change",
        json!({
            "spec": over_dates,
            "against": { "kind": "range", "from": "2026-05-01", "to": "2026-05-31" },
        }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(why(&body).contains("category"), "{}", why(&body));

    // …and a change with no earlier period is not a change.
    let (status, body) = run(&h, "insight_change", json!({ "spec": everything_billed() })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(why(&body).contains("earlier period"), "{}", why(&body));

    // A report whose second chart cannot be answered: refused **by its own
    // title**, and no board is created — the whole point of validating every
    // chart before writing anything.
    let (status, body) = run(
        &h,
        "insight_report",
        json!({
            "name": "Revenue",
            "charts": [
                { "title": "Billed, all time", "spec": everything_billed() },
                { "title": "Profit", "spec": invented },
            ],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(why(&body).contains("chart 2 (Profit)"), "{}", why(&body));
    assert!(why(&body).contains("profit"), "{}", why(&body));

    // A report with no charts at all, and one with more than a person asked
    // for, are refused the same way.
    let (status, body) = run(
        &h,
        "insight_report",
        json!({ "name": "Revenue", "charts": [] }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(why(&body).contains("at least one chart"), "{}", why(&body));
    let many: Vec<Value> = (0..9)
        .map(|n| json!({ "title": format!("Chart {n}"), "spec": everything_billed() }))
        .collect();
    let (status, body) = run(
        &h,
        "insight_report",
        json!({ "name": "Revenue", "charts": many }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(why(&body).contains("at most 8"), "{}", why(&body));

    // Nothing above wrote a thing.
    let (status, boards) = get(&h.app, &h.token, "/insights/dashboards").await;
    assert_eq!(status, StatusCode::OK, "{boards}");
    assert!(
        boards["dashboards"]
            .as_array()
            .unwrap()
            .iter()
            .all(|board| board["name"] != json!("Revenue")),
        "a refused report must leave no board behind: {boards}"
    );
}

/// **Another tenant's figures are not in the answer** (law #1). Two tenants, two
/// sets of books, one specification — and each gets its own total, never the
/// sum. A ChartSpec has no field that could name a tenant, and this is the test
/// that says so on the wire rather than in a comment.
#[tokio::test]
async fn the_figures_are_the_askers_tenants_and_nobody_elses() {
    let a = harness("agent-a24-iso-a").await;
    common::seed_default_chart(&a.acc).await;
    let b = harness("agent-a24-iso-b").await;
    common::seed_default_chart(&b.acc).await;
    let customer_a = a_customer(&a, "Acme GmbH").await;
    an_issued_invoice(&a, &customer_a, 100_000).await;
    let customer_b = a_customer(&b, "Beta BV").await;
    an_issued_invoice(&b, &customer_b, 77_700).await;

    let (status, mine) = run(&a, "insight_answer", json!({ "spec": everything_billed() })).await;
    assert_eq!(status, StatusCode::OK, "{mine}");
    let figure = |body: &Value| body["result"]["series"][0]["points"][0]["value"].clone();
    assert_eq!(figure(&mine), json!(100_000), "{mine}");

    let (status, theirs) = run(&b, "insight_answer", json!({ "spec": everything_billed() })).await;
    assert_eq!(status, StatusCode::OK, "{theirs}");
    assert_eq!(figure(&theirs), json!(77_700), "{theirs}");

    // A board one tenant's agent builds is not in the other's Insights.
    let (status, built) = run(
        &a,
        "insight_report",
        json!({
            "name": "A's revenue",
            "charts": [{ "title": "Billed", "spec": everything_billed() }],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{built}");
    let board = built["result"]["report"]["id"].as_str().unwrap().to_owned();
    let (status, theirs) = get(&b.app, &b.token, "/insights/dashboards").await;
    assert_eq!(status, StatusCode::OK, "{theirs}");
    assert!(
        theirs["dashboards"]
            .as_array()
            .unwrap()
            .iter()
            .all(|dash| dash["id"] != json!(board) && dash["name"] != json!("A's revenue")),
        "another tenant's board must not be listed: {theirs}"
    );
    let (status, opened) = get(&b.app, &b.token, &format!("/insights/dashboards/{board}")).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "another tenant's board must not be readable: {opened}"
    );
}
