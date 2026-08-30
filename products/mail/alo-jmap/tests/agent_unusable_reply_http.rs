//! A model that answers, but not with a decision — on the wire (A10.1).
//!
//! The 2026-08-30 real-model evaluation (`docs/autonomy/agents/STATE.md`) found
//! Ask alo planning correctly and then failing every one of its steps, while the
//! same agent asked directly answered in two seconds. The room was told "I
//! couldn't reach the model", which was false in both halves: the provider had
//! just been used for the plan, and the actual failure was an
//! `InferenceError::Empty` — a reply that did not parse as the decision
//! envelope.
//!
//! Two things follow, and this file is where both are held:
//!
//! * **One unusable reply no longer ends a run.** The turn asks again, once,
//!   with the contract restated. That is why orchestration looked broken while a
//!   single question looked fine: a plan spends a model call on the plan and one
//!   on every step, so it met the failure several times as often — and until
//!   this retry existed, every one of them was fatal to the whole run.
//! * **The two failures get two sentences.** A provider that could not be
//!   reached and a model that answered in prose are different events, and a room
//!   told the wrong one sends whoever reads it looking at the network.
//!
//! And one measurement, kept as a test because it was the first thing the
//! diagnosis needed: a delegated step is shown the *same prompt* as the same
//! question asked directly. The prompt was never the difference.
//!
//! No live model is called here: the tenant's AI backend is the scripted local
//! socket in `common::model`, which records what it was shown.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use crate::common::model::{Seen, says, scripted_model, use_model};
use crate::common::{Harness, harness, send};
use alo_store::{AgentProduct, ChatAgentId, ChatChannelId};

// ---- the wire ---------------------------------------------------------------

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

/// What a model says when it has not read the output contract: a helpful
/// sentence, no envelope. This is the shape the evaluation actually met.
const PROSE: &str = "Sure — I can look that up for you. Which customer did you mean?";

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

/// A room, and one agent of `product` in it.
async fn a_room_with(
    h: &Harness,
    name: &str,
    handle: &str,
    product: AgentProduct,
) -> (String, ChatAgentId) {
    let (status, body) = post(
        &h.app,
        &h.token,
        "/chat/channels",
        json!({ "name": name, "visibility": "public" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let channel = body["id"].as_str().unwrap().to_owned();
    let agent = h
        .acc
        .create_agent(handle, handle, Some("knows its own product"), product)
        .await
        .unwrap();
    h.acc
        .add_agent_to_channel(&ChatChannelId::new(channel.clone()), &agent)
        .await
        .unwrap();
    (channel, agent)
}

/// One product agent, defined but in no room — a run puts it there itself.
async fn an_agent(h: &Harness, handle: &str, product: AgentProduct) -> ChatAgentId {
    h.acc
        .create_agent(handle, handle, Some("knows its own product"), product)
        .await
        .unwrap()
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

/// Say something in the room and wait until `done` is true of what is in it.
/// The turn runs off the request, so everything here is waited for; the
/// deadline is a ceiling, and blowing it is a real failure.
async fn ask_and_wait(
    h: &Harness,
    channel: &str,
    question: &str,
    done: impl Fn(&[Value]) -> bool,
) -> Vec<Value> {
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/chat/channels/{channel}/messages"),
        json!({ "body": question }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let all = messages(h, channel).await;
        if done(&all) {
            return all;
        }
        assert!(
            Instant::now() < deadline,
            "the turn never got there: {}",
            json!(all)
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// The body of the last thing an agent said.
fn last_agent_line(all: &[Value]) -> String {
    all.iter()
        .rfind(|m| m["authorKind"] == "agent")
        .map(|m| m["body"].as_str().unwrap_or_default().to_owned())
        .unwrap_or_default()
}

fn said_by<'a>(all: &'a [Value], agent: &ChatAgentId) -> Vec<&'a Value> {
    all.iter()
        .filter(|m| m["authorKind"] == "agent" && m["author"] == json!(agent.as_str()))
        .collect()
}

/// The system prompt the model was shown on its `n`th call.
fn system_of(seen: &Seen, n: usize) -> String {
    seen.lock().unwrap()[n]["messages"][0]["content"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// The user turn of the `n`th call — the request, the sources, the offer.
fn user_of(seen: &Seen, n: usize) -> String {
    seen.lock().unwrap()[n]["messages"][1]["content"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn calls(seen: &Seen) -> usize {
    seen.lock().unwrap().len()
}

/// True once the room holds an agent message — any of them.
fn an_agent_spoke(all: &[Value]) -> bool {
    all.iter().any(|m| m["authorKind"] == "agent")
}

// ---- the retry ---------------------------------------------------------------

/// **One unusable reply is asked again, and the turn answers.** The model's
/// first reply is a friendly sentence with no envelope in it; the second is the
/// answer, and that is what the room gets.
#[tokio::test]
async fn a_reply_that_is_not_a_decision_is_asked_again_and_the_turn_answers() {
    let h = harness("unusableretry").await;
    let (base, seen) = scripted_model(vec![
        PROSE.to_owned(),
        says("You have three open quotes [1]."),
    ])
    .await;
    use_model(&h, &base).await;
    let (channel, billing) = a_room_with(&h, "money", "billing", AgentProduct::Billing).await;

    let all = ask_and_wait(
        &h,
        &channel,
        "@billing which quotes are open?",
        an_agent_spoke,
    )
    .await;

    // The room got the answer, not an excuse.
    let spoke = said_by(&all, &billing);
    assert_eq!(spoke.len(), 1, "{}", json!(all));
    assert_eq!(spoke[0]["body"], json!("You have three open quotes [1]."));

    // Exactly one further call, and it restated the contract without quoting
    // anything back — the reply may hold somebody's records (law #1).
    assert_eq!(calls(&seen), 2, "one ask, one retry");
    let again = user_of(&seen, 1);
    assert!(
        again.contains("Your previous reply could not be used"),
        "{again}"
    );
    assert!(again.contains("which quotes are open?"), "{again}");
    assert!(
        !again.contains("Which customer did you mean?"),
        "the model's own words must not be fed back: {again}"
    );
    // The system prompt is untouched by the retry: same contract, same tools.
    assert_eq!(system_of(&seen, 0), system_of(&seen, 1));
}

/// **A model that never answers with a decision is not a model that could not
/// be reached.** The room is told which of the two happened, and the turn stops
/// after one retry rather than asking round in a circle.
#[tokio::test]
async fn a_model_that_answers_unusably_twice_says_it_was_reached() {
    let h = harness("unusablesays").await;
    let (base, seen) = scripted_model(vec![PROSE.to_owned(), PROSE.to_owned()]).await;
    use_model(&h, &base).await;
    let (channel, billing) = a_room_with(&h, "money", "billing", AgentProduct::Billing).await;

    let all = ask_and_wait(
        &h,
        &channel,
        "@billing which quotes are open?",
        an_agent_spoke,
    )
    .await;

    let line = said_by(&all, &billing)[0]["body"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(line.contains("I reached the model"), "{line}");
    assert!(line.contains("wasn't something I could act on"), "{line}");
    assert!(
        !line.contains("couldn't reach the model"),
        "the provider was reached: {line}"
    );
    // Two calls: the ask and its one retry. A retry loop would be a room that
    // spends a workspace's inference budget on a model that will not comply.
    assert_eq!(calls(&seen), 2);
    // Nothing was proposed, and the model's own words never reached the room.
    assert!(all.iter().all(|m| m["proposal"].is_null()));
    assert!(!line.contains("Which customer did you mean?"));
}

/// **The defect the evaluation found, in the shape it found it**: @alo plans, and
/// step one's model answers with prose. Before A10.1 the run died there and the
/// room was told the model could not be reached. Now the step is asked again and
/// the plan finishes.
#[tokio::test]
async fn an_orchestrated_step_recovers_from_one_unusable_reply() {
    let h = harness("orchretry").await;
    let (channel, alo) = a_room_with(&h, "ops", "alo", AgentProduct::Workspace).await;
    let billing = an_agent(&h, "billing", AgentProduct::Billing).await;

    let (base, seen) = scripted_model(vec![
        plans(&[("billing", "what did we quote Northstar Foods?")]),
        PROSE.to_owned(),
        says("We quoted Northstar Foods 7,865.00 EUR on QUO-2026-00001 [1]."),
    ])
    .await;
    use_model(&h, &base).await;

    let all = ask_and_wait(
        &h,
        &channel,
        "@alo what did we quote Northstar Foods?",
        |all| {
            all.iter()
                .any(|m| m["body"].as_str().unwrap_or_default().contains("7,865.00"))
        },
    )
    .await;

    // The step answered under its own name, after the plan.
    let answered = said_by(&all, &billing);
    assert_eq!(answered.len(), 1, "{}", json!(all));
    assert!(
        answered[0]["body"]
            .as_str()
            .unwrap()
            .contains("QUO-2026-00001")
    );
    // Ask alo said the plan and nothing else — no excuse, no "unreachable".
    let plan = said_by(&all, &alo);
    assert_eq!(plan.len(), 1, "{}", json!(plan));
    assert!(plan[0]["body"].as_str().unwrap().starts_with("Here's how"));

    // The plan, the step, and the step's one retry.
    assert_eq!(calls(&seen), 3);
    assert!(user_of(&seen, 2).contains("Your previous reply could not be used"));

    // The goal record agrees with the room: it ran to the end.
    let (status, goals) = get(&h.app, &h.token, &format!("/chat/channels/{channel}/goals")).await;
    assert_eq!(status, StatusCode::OK, "{goals}");
    assert_eq!(goals["goals"][0]["status"], json!("done"), "{goals}");
    assert_eq!(goals["goals"][0]["cursor"], json!(1));
}

/// …and when the second ask is unusable too, the run ends — saying what
/// actually happened, in the room and on the goal record, so an operator reading
/// either one is not sent to look at the network.
#[tokio::test]
async fn a_step_that_never_answers_ends_the_run_with_the_true_reason() {
    let h = harness("orchunusable").await;
    let (channel, alo) = a_room_with(&h, "ops", "alo", AgentProduct::Workspace).await;
    let billing = an_agent(&h, "billing", AgentProduct::Billing).await;
    let tasks = an_agent(&h, "tasks", AgentProduct::Tasks).await;

    let (base, seen) = scripted_model(vec![
        plans(&[
            ("billing", "what did we quote Northstar Foods?"),
            ("tasks", "add a follow-up"),
        ]),
        PROSE.to_owned(),
        PROSE.to_owned(),
    ])
    .await;
    use_model(&h, &base).await;

    let all = ask_and_wait(&h, &channel, "@alo chase Northstar", |all| {
        all.iter()
            .any(|m| m["body"].as_str().unwrap_or_default().contains("I reached"))
    })
    .await;

    let line = last_agent_line(&all);
    assert!(line.contains("I reached the model"), "{line}");
    assert!(!line.contains("couldn't reach the model"), "{line}");
    // It was Ask alo that said it, the failed step said nothing, and the step
    // behind it never ran.
    assert_eq!(said_by(&all, &alo).len(), 2, "the plan and the reason");
    assert!(said_by(&all, &billing).is_empty());
    assert!(said_by(&all, &tasks).is_empty(), "step two must not run");
    assert_eq!(calls(&seen), 3, "the plan, step one, and its one retry");

    // The goal says which failure it was — the room scrolls away, this does not.
    let (status, goals) = get(&h.app, &h.token, &format!("/chat/channels/{channel}/goals")).await;
    assert_eq!(status, StatusCode::OK, "{goals}");
    assert_eq!(goals["goals"][0]["status"], json!("failed"), "{goals}");
    assert_eq!(
        goals["goals"][0]["note"],
        json!("the model's reply could not be used")
    );
}

/// **The prompt was never the difference.** The first hypothesis about the
/// evaluation's finding was that `delegate_turn` builds a different prompt from
/// a room turn — a different tool list, a different offer, a different shape.
/// It does not: the same question asked directly and asked as a step of a plan
/// shows the model the same system prompt and the same user turn, down to the
/// sources and the handoff offer, differing only in the words of the request.
///
/// Kept as a test because the next person to read the journal will want to know
/// it was measured rather than assumed — and because the day it stops being
/// true, a delegated step stops being "the same question, asked by a plan".
#[tokio::test]
async fn a_delegated_step_is_shown_the_same_prompt_as_the_question_asked_directly() {
    let h = harness("stepprompt").await;
    let (channel, _alo) = a_room_with(&h, "ops", "alo", AgentProduct::Workspace).await;
    let billing = an_agent(&h, "billing", AgentProduct::Billing).await;
    h.acc
        .add_agent_to_channel(&ChatChannelId::new(channel.clone()), &billing)
        .await
        .unwrap();
    // A second product agent, so both turns have somebody to be offered: the
    // handoff offer is part of what is being compared, and an empty one would
    // compare equal for the wrong reason.
    an_agent(&h, "tasks", AgentProduct::Tasks).await;

    let asked = "which quotes are open?";
    let (base, seen) = scripted_model(vec![
        says("Three [1]."),
        plans(&[("billing", asked)]),
        says("Three [1]."),
    ])
    .await;
    use_model(&h, &base).await;

    // Asked directly…
    ask_and_wait(&h, &channel, &format!("@billing {asked}"), |all| {
        all.iter().any(|m| m["authorKind"] == "agent")
    })
    .await;
    // …and asked again through a plan, with the step carrying the same words.
    ask_and_wait(&h, &channel, "@alo which quotes are open?", |all| {
        all.iter().filter(|m| m["authorKind"] == "agent").count() >= 3
    })
    .await;
    assert_eq!(calls(&seen), 3, "the direct turn, the plan, the step");

    // The same agent, so the same prompt: same tools, same guidance, same
    // output contract. Not "similar" — identical.
    assert_eq!(
        system_of(&seen, 0),
        system_of(&seen, 2),
        "a step is the same agent's turn"
    );

    // And the same user turn: the date, the (empty) sources and the handoff
    // offer are the same; only the request line differs, because the direct ask
    // carries the handle the person typed.
    let without_request = |text: &str| {
        text.lines()
            .filter(|line| !line.starts_with("Request:"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let direct = user_of(&seen, 0);
    let step = user_of(&seen, 2);
    assert_eq!(without_request(&direct), without_request(&step), "{direct}");
    assert!(direct.contains(&format!("Request: @billing {asked}")));
    assert!(step.contains(&format!("Request: {asked}")));
    // Including the offer, which a step is made exactly as a room turn is: the
    // asker's own roster, minus the agent taking the turn.
    assert!(step.contains("@tasks (the tasks agent)"), "{step}");
    assert!(!step.contains("@billing"), "never offered itself: {step}");
}
