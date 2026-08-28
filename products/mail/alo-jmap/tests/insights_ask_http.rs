//! `POST /insights/ask` — ask-to-chart on the wire (BI1.07).
//!
//! **No live model is ever called here.** The tenant's AI backend is a scripted
//! local socket that hands back fixture completions in order, which is what
//! makes the two-turn shape testable at all: a first reply the write gate
//! refuses, a repair turn that must carry the validator's own sentence, and the
//! corrected spec evaluated against real invoices in a real Postgres.
//!
//! What the suite is for is the properties the design note promises and unit
//! tests cannot reach:
//!
//! - the proposal is a **proposal** — asking stores nothing, and the same spec
//!   becomes a tile only through the ordinary pin route;
//! - the preview figures are the figures the invoice underneath carries;
//! - a model that cannot be corrected produces a typed `422` and no tile;
//! - a tenant with no AI configured gets a `503` and the rest of Insights is
//!   untouched;
//! - the route is closed to an unauthenticated caller like every other.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use crate::common::{Harness, harness, send};

// ---- a scripted, local, offline "model" -------------------------------------

/// The request bodies the fake backend has been sent, in order.
type Seen = Arc<Mutex<Vec<Value>>>;

/// A minimal OpenAI-compatible chat-completions endpoint on localhost that
/// answers `script` in order (the last entry repeats), recording what it was
/// asked. It speaks just enough HTTP/1.1 for `reqwest`, in the shape
/// `tests/junk_training.rs` already uses for Rspamd.
async fn scripted_model(script: Vec<String>) -> (String, Seen) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let seen: Seen = Arc::new(Mutex::new(Vec::new()));
    let record = Arc::clone(&seen);
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                return;
            };
            let record = Arc::clone(&record);
            let script = script.clone();
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let mut chunk = [0u8; 8192];
                let body = loop {
                    let Ok(n) = sock.read(&mut chunk).await else {
                        return;
                    };
                    if n == 0 {
                        return;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") else {
                        continue;
                    };
                    let head = String::from_utf8_lossy(&buf[..end]).into_owned();
                    let length: usize = head
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse().unwrap_or(0))
                        })
                        .unwrap_or(0);
                    if buf.len() >= end + 4 + length {
                        break buf[end + 4..end + 4 + length].to_vec();
                    }
                };
                let turn = {
                    let mut seen = record.lock().unwrap();
                    seen.push(serde_json::from_slice(&body).unwrap_or(Value::Null));
                    seen.len() - 1
                };
                let content = script
                    .get(turn)
                    .or_else(|| script.last())
                    .cloned()
                    .unwrap_or_default();
                let answer =
                    json!({ "choices": [{ "message": { "role": "assistant", "content": content } }] })
                        .to_string();
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    answer.len(),
                    answer
                );
                let _ = sock.write_all(response.as_bytes()).await;
                let _ = sock.flush().await;
            });
        }
    });
    (format!("http://{addr}"), seen)
}

/// Points the tenant's default AI provider at `base_url`.
///
/// The provider id carries the tenant, because these suites share one Postgres
/// and a provider id is unique across it — the tenant guard on the upsert would
/// otherwise turn a second tenant's write into a silent no-op, which is the
/// guard working exactly as intended.
async fn use_model(h: &Harness, base_url: &str) {
    let id = format!("ai-{}", h.tenant.as_str());
    h.acc
        .upsert_ai_provider(
            &id,
            "openai",
            "scripted",
            base_url,
            "test-model",
            None,
            true,
        )
        .await
        .unwrap();
    h.acc.set_default_ai_provider(&id).await.unwrap();
}

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

async fn ask(h: &Harness, question: &str) -> (StatusCode, Value) {
    post(&h.app, &h.token, "/insights/ask", json!({ "q": question })).await
}

/// Issues one invoice and answers its net in cents, so a preview can be checked
/// against the document it is drawn from.
async fn an_issued_invoice(h: &Harness, price: i64) -> i64 {
    let (status, body) = post(
        &h.app,
        &h.token,
        "/billing/customers",
        json!({
            "name": "Acme GmbH",
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
    let customer = body["customer"]["id"].as_str().unwrap().to_owned();

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
    net
}

// ---- fixture completions -----------------------------------------------------

/// What a well-behaved model replies with — the whole envelope, in a fence,
/// which is how models actually send JSON.
fn good_reply() -> String {
    "Here you go:\n```json\n{\"schema_version\":1,\"dataset\":\"billing.documents\",\
     \"measure\":{\"id\":\"net\",\"agg\":\"sum\"},\"period\":{\"kind\":\"all\"},\
     \"viz\":\"number\"}\n```"
        .to_owned()
}

/// A first attempt the write gate refuses: a measure the catalog has not got.
fn invented_measure() -> String {
    "{\"schema_version\":1,\"dataset\":\"billing.documents\",\
     \"measure\":{\"id\":\"profit\",\"agg\":\"sum\"},\"period\":{\"kind\":\"all\"},\
     \"viz\":\"number\"}"
        .to_owned()
}

// ---- the arc -----------------------------------------------------------------

#[tokio::test]
async fn a_question_becomes_a_proposal_a_person_pins() {
    let h = harness("insights-ask-arc").await;
    common::seed_default_chart(&h.acc).await;
    let net = an_issued_invoice(&h, 25_000).await;
    let (base, seen) = scripted_model(vec![good_reply()]).await;
    use_model(&h, &base).await;

    let (status, body) = ask(&h, "how much have we billed in total?").await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // The proposal is the canonical spec, its drawing, and the width it wants.
    assert_eq!(body["spec"]["dataset"], "billing.documents");
    assert_eq!(body["spec"]["measure"]["id"], "net");
    assert_eq!(body["viz"], "number");
    assert_eq!(body["span"], 1);
    assert_eq!(body["repaired"], false);
    // And the preview is the figure the invoice underneath carries, to the cent.
    assert_eq!(body["series"]["series"][0]["points"][0]["value"], net);
    assert_eq!(body["series"]["unit"]["kind"], "money");

    // One turn was needed, it carried the catalog and the question, and it went
    // to the tenant's own configured backend.
    let turns = seen.lock().unwrap().clone();
    assert_eq!(turns.len(), 1);
    let system = turns[0]["messages"][0]["content"].as_str().unwrap();
    assert!(system.contains("billing.documents"), "no catalog");
    assert!(system.contains("DO NOT USE"), "id filters not refused");
    assert!(
        turns[0]["messages"][1]["content"]
            .as_str()
            .unwrap()
            .contains("how much have we billed in total?")
    );

    // Asking stored nothing: the tenant still has only the seeded overview, and
    // nothing is pinned to it that it was not seeded with.
    let (_, boards) = get(&h.app, &h.token, "/insights/dashboards").await;
    assert_eq!(boards["dashboards"].as_array().map(Vec::len), Some(1));

    // The proposal becomes a tile the ordinary way — the same write gate, the
    // caption the reader chose — and the pinned tile answers the same figure.
    let board = boards["dashboards"][0]["id"].as_str().unwrap().to_owned();
    let (status, pinned) = post(
        &h.app,
        &h.token,
        &format!("/insights/dashboards/{board}/tiles"),
        json!({
            "title": "how much have we billed in total?",
            "spec": body["spec"],
            "span": body["span"],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{pinned}");
    assert_eq!(pinned["tile"]["readable"], true);
    assert_eq!(pinned["tile"]["viz"], "number");
    let tile = pinned["tile"]["id"].as_str().unwrap().to_owned();
    let (status, figures) = get(&h.app, &h.token, &format!("/insights/tiles/{tile}/data")).await;
    assert_eq!(status, StatusCode::OK, "{figures}");
    assert_eq!(figures["series"][0]["points"][0]["value"], net);
}

#[tokio::test]
async fn a_refused_spec_earns_exactly_one_repair_turn_carrying_the_refusal() {
    let h = harness("insights-ask-repair").await;
    common::seed_default_chart(&h.acc).await;
    an_issued_invoice(&h, 10_000).await;
    let (base, seen) = scripted_model(vec![invented_measure(), good_reply()]).await;
    use_model(&h, &base).await;

    let (status, body) = ask(&h, "what did we bill?").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["repaired"], true, "the correction is reported");
    assert_eq!(body["spec"]["measure"]["id"], "net");

    let turns = seen.lock().unwrap().clone();
    assert_eq!(turns.len(), 2, "one repair, never two");
    // The repair turn is the first conversation plus the model's own bad reply
    // and the validator's sentence — a correction, not a fresh guess.
    let messages = turns[1]["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 4);
    assert_eq!(messages[2]["role"], "assistant");
    assert!(messages[2]["content"].as_str().unwrap().contains("profit"));
    let correction = messages[3]["content"].as_str().unwrap();
    assert!(correction.contains("refused"), "{correction}");
    assert!(correction.contains("profit"), "{correction}");
}

#[tokio::test]
async fn a_model_that_cannot_be_corrected_refuses_and_pins_nothing() {
    let h = harness("insights-ask-refusal").await;
    common::seed_default_chart(&h.acc).await;
    let (base, seen) = scripted_model(vec![invented_measure()]).await;
    use_model(&h, &base).await;

    let (status, body) = ask(&h, "what did we bill?").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    let detail = body["detail"].as_str().unwrap_or_default();
    assert!(detail.contains("no chart could be built"), "{detail}");
    // The reason a user is given is the one that survived the correction.
    assert!(detail.contains("profit"), "{detail}");
    assert_eq!(seen.lock().unwrap().len(), 2, "two turns, then a refusal");

    // Nothing was stored: the tenant has its seeded board and no more.
    let (_, boards) = get(&h.app, &h.token, "/insights/dashboards").await;
    assert_eq!(boards["dashboards"].as_array().map(Vec::len), Some(1));
    let board = boards["dashboards"][0]["id"].as_str().unwrap().to_owned();
    let (_, contents) = get(&h.app, &h.token, &format!("/insights/dashboards/{board}")).await;
    let tiles = contents["tiles"].as_array().map(Vec::len).unwrap_or(0);
    assert_eq!(tiles, 7, "the seeded overview, and nothing the ask added");
}

#[tokio::test]
async fn a_model_that_says_it_cannot_chart_the_question_is_believed_at_once() {
    let h = harness("insights-ask-cannot").await;
    common::seed_default_chart(&h.acc).await;
    let (base, seen) = scripted_model(vec![
        r#"{"error":"That is not a question about this data."}"#.to_owned(),
    ])
    .await;
    use_model(&h, &base).await;

    let (status, body) = ask(&h, "what is the weather tomorrow?").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        body["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("not a question about this data"),
    );
    // A stated refusal is an answer, not something to correct: one turn only.
    assert_eq!(seen.lock().unwrap().len(), 1);
}

// ---- the guards --------------------------------------------------------------

#[tokio::test]
async fn a_tenant_without_a_model_is_told_so_and_the_rest_of_insights_still_works() {
    let h = harness("insights-ask-off").await;
    common::seed_default_chart(&h.acc).await;

    let (status, body) = ask(&h, "revenue by month").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(body["detail"], "ai-unavailable");

    // The gallery and the boards are untouched by the ask being off — the
    // manual surface is the whole product minus this one control.
    let (status, gallery) = get(&h.app, &h.token, "/insights/gallery").await;
    assert_eq!(status, StatusCode::OK, "{gallery}");
    assert!(!gallery["entries"].as_array().unwrap().is_empty());
    let (status, boards) = get(&h.app, &h.token, "/insights/dashboards").await;
    assert_eq!(status, StatusCode::OK, "{boards}");
}

#[tokio::test]
async fn a_question_that_is_missing_or_too_long_never_reaches_a_model() {
    let h = harness("insights-ask-bounds").await;
    common::seed_default_chart(&h.acc).await;
    let (base, seen) = scripted_model(vec![good_reply()]).await;
    use_model(&h, &base).await;

    for body in [json!({}), json!({ "q": "   " })] {
        let (status, answer) = post(&h.app, &h.token, "/insights/ask", body).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{answer}");
        assert!(
            answer["detail"]
                .as_str()
                .unwrap_or_default()
                .contains("q is required")
        );
    }

    let (status, answer) = ask(&h, &"a".repeat(501)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{answer}");
    assert!(
        answer["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("at most 500 characters")
    );

    assert_eq!(seen.lock().unwrap().len(), 0, "nothing was sent anywhere");
}

#[tokio::test]
async fn the_ask_refuses_an_unauthenticated_caller() {
    let h = harness("insights-ask-401").await;
    common::seed_default_chart(&h.acc).await;
    let (base, seen) = scripted_model(vec![good_reply()]).await;
    use_model(&h, &base).await;

    let req = Request::builder()
        .method("POST")
        .uri("/insights/ask")
        .header("content-type", "application/json")
        .body(Body::from(json!({ "q": "revenue by month" }).to_string()))
        .unwrap();
    let (status, _) = send(&h.app, req).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(seen.lock().unwrap().len(), 0, "no model was called");
}
