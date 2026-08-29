//! The scripted evaluation run (A4.7, ADR 0057) — the wave's exit gate.
//!
//! The evaluation set grows from the registry (`alo_ai::evaluation`): every
//! moved module, every verb, asked as its own `answers` question by its own
//! agent, in a real room against the real router and store with the scripted
//! model. Per verb the run proves what the 2026-08-28 run found missing:
//!
//! - the agent's prompt **offers** the verb its question is answered by;
//! - a **read answers in the room and never proposes**, is never turned away
//!   at its own agent's execution boundary, and its result — the record view,
//!   or on an empty workspace the executor's own words ("you have no sales
//!   board yet") — reaches the model as a numbered source;
//! - a **write proposes and never runs** — the proposal card in the room
//!   carries the verb, and no second mechanism appears anywhere.
//!
//! What each verb returned is recorded verbatim into a per-agent transcript
//! under `CARGO_TARGET_TMPDIR`, which is what STATE.md quotes. The
//! **real-model** run over the same set (plus `alo_ai::evaluation::STANDING`)
//! is the owner's, with the tenant's own provider — never a suite's.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashSet;
use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use alo_ai::Effect;
use alo_ai::agent_product::intent_spec;
use alo_ai::evaluation::{evaluation_set, placeholder_args};
use alo_store::AgentProduct;

use crate::common::model::{Seen, says, scripted_model, use_model, wants};
use crate::common::{Harness, harness, send};

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

/// The tenant's own agent for `handle`, from the seeded set.
async fn agent_id(h: &Harness, handle: &str) -> String {
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

/// Says something in the room and waits for the agent's reply to *this*
/// message. The feed is newest-first and a turn posts exactly one agent
/// message, so the reply is the newest message once the count has grown past
/// the question — the room is reused across the whole run, which is why "any
/// agent message anywhere" (the one-question suites' shortcut) would return
/// turn one's answer forever.
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
        if all.len() >= before + 2 {
            let newest = all.first().unwrap();
            if newest["authorKind"] == "agent" {
                return newest.clone();
            }
        }
        assert!(
            Instant::now() < deadline,
            "the agent never answered {question:?}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// The last message of the model's `n`th call — for a read, call 1 carries
/// the verb's result exactly as the model was shown it.
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

/// The `[n] tool result "verb" — …` block of an after-read message: from the
/// marker to the end of its paragraph — what the verb actually returned,
/// without the guidance around it.
fn result_line<'a>(after_read: &'a str, marker: &str) -> &'a str {
    after_read.find(marker).map_or(after_read, |at| {
        let block = &after_read[at..];
        block
            .split_once("\n\n")
            .map_or(block, |(paragraph, _)| paragraph)
    })
}

/// One line of transcript, on a char boundary, newlines flattened.
fn excerpt(text: &str) -> String {
    let flat = text.replace(['\n', '\r'], " ");
    if flat.chars().count() <= 300 {
        return flat;
    }
    let cut: String = flat.chars().take(300).collect();
    format!("{cut}…")
}

/// The wording of the two boundary refusals and the missing-verb error — the
/// three ways a verb's own question can fail to reach its executor, none of
/// which any case of this run may hit.
const NEVER_SHOWN: &[&str] = &[
    "is not a tool the",
    "unknown tool",
    "waits for you to approve",
];

/// Every verb of `product`, asked as its first `answers` question by its own
/// agent, one turn per verb in one room. Returns the transcript.
async fn run_product(tag: &str, product: AgentProduct) -> String {
    let h = harness(tag).await;
    let handle = alo_store::default_handle(product);
    let agent = agent_id(&h, handle).await;
    let room = a_room_with(&h, "evaluation", &agent).await;

    let mut asked: HashSet<&str> = HashSet::new();
    let mut transcript = String::new();
    for case in evaluation_set()
        .into_iter()
        .filter(|case| case.product == product)
    {
        // One turn per verb: the verb's first question stands for the rest,
        // which the owner's real-model run asks in full.
        if !asked.insert(case.verb) {
            continue;
        }
        let spec = intent_spec(case.verb).expect("a case's verb is registered");
        let args = placeholder_args(spec);
        let read = matches!(case.effect, Effect::Read);
        let script = if read {
            vec![
                wants(case.verb, args, "Let me look."),
                says(&format!("Answered from {}.", case.verb)),
            ]
        } else {
            vec![wants(case.verb, args, "I can do that — proposing it.")]
        };
        let (model, seen) = scripted_model(script).await;
        use_model(&h, &model).await;

        let question = format!("@{handle} {}", case.ask);
        let answer = ask_in_room(&h, &room, &question).await;

        let prompt = offered(&seen, 0);
        assert!(
            prompt.contains(&format!("- {}:", case.verb)),
            "@{handle} was asked {:?} but never offered {}",
            case.ask,
            case.verb
        );
        if read {
            assert!(
                answer["proposal"].is_null(),
                "{}: a read answers, never proposes: {answer}",
                case.verb
            );
            let after_read = shown(&seen, 1);
            for refusal in NEVER_SHOWN {
                assert!(
                    !after_read.contains(refusal),
                    "{} did not reach its own executor: {after_read}",
                    case.verb
                );
            }
            // The verb's result — the record view, or on an empty workspace
            // the executor's own words ("you have no sales board yet") —
            // came back to the model as a numbered source. That is the whole
            // wire path the 2026-08-28 run found missing.
            let marker = format!("tool result \"{}\"", case.verb);
            assert!(
                after_read.contains(&marker),
                "{}'s result never reached the model: {after_read}",
                case.verb
            );
            transcript.push_str(&format!(
                "{question}\n  {} → {}\n",
                case.verb,
                excerpt(result_line(&after_read, &marker))
            ));
        } else {
            assert!(
                !answer["proposal"].is_null(),
                "{}: a write proposes, never runs on its own: {answer}",
                case.verb
            );
            assert_eq!(answer["proposal"]["tool"], case.verb, "{answer}");
            transcript.push_str(&format!(
                "{question}\n  {} → proposed, waiting for approval\n",
                case.verb
            ));
        }
    }
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("agent-evaluation");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{handle}.md")), &transcript).unwrap();
    transcript
}

macro_rules! evaluate {
    ($name:ident, $tag:literal, $product:ident) => {
        #[tokio::test]
        async fn $name() {
            let transcript = run_product($tag, AgentProduct::$product).await;
            assert!(!transcript.is_empty(), "the run asked nothing");
        }
    };
}

evaluate!(agenda_answers_its_evaluation_set, "eval-agenda", Agenda);
evaluate!(billing_answers_its_evaluation_set, "eval-billing", Billing);
evaluate!(chat_answers_its_evaluation_set, "eval-chat", Chat);
evaluate!(crm_answers_its_evaluation_set, "eval-crm", Crm);
evaluate!(docs_answers_its_evaluation_set, "eval-docs", Docs);
evaluate!(drive_answers_its_evaluation_set, "eval-drive", Drive);
evaluate!(finance_answers_its_evaluation_set, "eval-finance", Finance);
evaluate!(hr_answers_its_evaluation_set, "eval-hr", Hr);
evaluate!(
    insights_answers_its_evaluation_set,
    "eval-insights",
    Insights
);
evaluate!(
    inventory_answers_its_evaluation_set,
    "eval-inventory",
    Inventory
);
evaluate!(mail_answers_its_evaluation_set, "eval-mail", Mail);
evaluate!(meet_answers_its_evaluation_set, "eval-meet", Meet);
evaluate!(
    projects_answers_its_evaluation_set,
    "eval-projects",
    Projects
);
evaluate!(sheets_answers_its_evaluation_set, "eval-sheets", Sheets);
evaluate!(sites_answers_its_evaluation_set, "eval-sites", Sites);
evaluate!(tasks_answers_its_evaluation_set, "eval-tasks", Tasks);
