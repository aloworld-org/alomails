//! Ask alo orchestrating the product agents, on the wire (ADR 0034, A3.1).
//!
//! Four properties, none of which can be seen from the store:
//!
//! * the **plan** is in the room before any of it runs, naming each agent and
//!   what it is being asked;
//! * each step runs as **that agent** — its prompt, its scope at the execution
//!   boundary, its name on the message — and not as Ask alo with everything;
//! * a run has **one approval surface**: it stops at the first step that wants
//!   to change something, and the steps behind it do not run;
//! * **Stop** ends a run between its steps rather than merely declining to post
//!   one answer.
//!
//! No live model is called: the tenant's AI backend is the scripted local
//! socket in `common::model`, which also records what it was shown — and what
//! the model was shown is where the scoping is actually visible.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use alo_store::{AgentProduct, AppModule, ChatAgentId, ChatChannelId};
use common::model::{Seen, says, scripted_model, scripted_model_paced, use_model, wants};
use common::{Harness, harness, send};

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

/// A room with Ask alo in it, and the product agents defined in the tenant but
/// **not** in the room — putting them there is the run's own job.
async fn a_room_with_ask_alo(h: &Harness) -> (String, ChatAgentId) {
    let (status, body) = post(
        &h.app,
        &h.token,
        "/chat/channels",
        json!({ "name": "ops", "visibility": "public" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let channel = body["id"].as_str().unwrap().to_owned();
    let alo = h
        .acc
        .create_agent(
            "alo",
            "alo",
            Some("asks the others"),
            AgentProduct::Workspace,
        )
        .await
        .unwrap();
    h.acc
        .add_agent_to_channel(&ChatChannelId::new(channel.clone()), &alo)
        .await
        .unwrap();
    (channel, alo)
}

/// One product agent, defined but in no room.
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
///
/// The run happens off the request, so everything here has to be waited for;
/// the deadline is a ceiling, and blowing it is a real failure rather than a
/// slow machine.
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
    wait_for(h, channel, done).await
}

/// Wait until `done` is true of the room's messages.
async fn wait_for(h: &Harness, channel: &str, done: impl Fn(&[Value]) -> bool) -> Vec<Value> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let all = messages(h, channel).await;
        if done(&all) {
            return all;
        }
        assert!(
            Instant::now() < deadline,
            "the run never got there: {}",
            json!(all)
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// The messages an agent said, in order.
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

/// What the model was asked on its `n`th call — the user turn, which carries
/// the sources and any tool results.
fn user_of(seen: &Seen, n: usize) -> String {
    seen.lock().unwrap()[n]["messages"][1]["content"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn calls(seen: &Seen) -> usize {
    seen.lock().unwrap().len()
}

/// **The whole shape of a run**: the plan is said first, each step answers under
/// its own agent's name, and the run stops at the step that wants to change
/// something — so there is exactly one thing to approve and the step behind it
/// never runs.
#[tokio::test]
async fn a_plan_is_said_first_and_the_run_stops_at_the_first_change() {
    let h = harness("orchplan").await;
    let (channel, alo) = a_room_with_ask_alo(&h).await;
    let inventory = an_agent(&h, "inventory", AgentProduct::Inventory).await;
    let tasks = an_agent(&h, "tasks", AgentProduct::Tasks).await;
    let mail = an_agent(&h, "mail", AgentProduct::Mail).await;

    let (base, seen) = scripted_model(vec![
        plans(&[
            ("inventory", "is the X100 in stock?"),
            ("tasks", "add a task to reorder the X100"),
            ("mail", "tell the supplier we are reordering"),
        ]),
        says("There are 42 X100 in stock."),
        wants(
            "create_task",
            json!({ "title": "Reorder the X100", "due": "2026-08-21" }),
            "I'll add a task to reorder the X100.",
        ),
    ])
    .await;
    use_model(&h, &base).await;

    // Waiting on the *last* thing the run says, not on the proposal: the
    // proposal row exists a moment before the sentence that explains it, and a
    // test that stops at the row reads a half-written run.
    let all = ask_and_wait(&h, &channel, "@alo restock the X100", |all| {
        all.iter().any(|m| {
            m["body"]
                .as_str()
                .unwrap_or_default()
                .contains("waits until you approve")
        })
    })
    .await;

    // The plan, before anything happened, naming every step's agent.
    let plan = said_by(&all, &alo);
    let heading = plan[0]["body"].as_str().unwrap();
    assert!(heading.starts_with("Here's how I'll do that:"), "{heading}");
    assert!(heading.contains("1. @inventory — is the X100 in stock?"));
    assert!(heading.contains("2. @tasks — add a task to reorder the X100"));
    assert!(heading.contains("3. @mail — tell the supplier we are reordering"));
    // …and it was said before either step spoke.
    let plan_seq = plan[0]["seq"].as_i64().unwrap();
    for spoke in said_by(&all, &inventory) {
        assert!(spoke["seq"].as_i64().unwrap() > plan_seq);
    }

    // Step one answered under the Inventory agent's own name.
    let answered = said_by(&all, &inventory);
    assert_eq!(answered.len(), 1);
    assert_eq!(answered[0]["body"], json!("There are 42 X100 in stock."));
    assert!(answered[0]["proposal"].is_null(), "a read wears no button");

    // Step two wants a change, so it is proposed — as the Tasks agent, which is
    // what makes the approval run at *its* scope and not at Ask alo's.
    let proposed = said_by(&all, &tasks);
    assert_eq!(proposed.len(), 1);
    assert_eq!(proposed[0]["proposal"]["tool"], json!("create_task"));
    assert_eq!(proposed[0]["proposal"]["state"], json!("pending"));

    // **One approval surface.** The third step never ran, and the room was told
    // why rather than left with a plan that quietly stopped.
    assert!(said_by(&all, &mail).is_empty(), "step three must not run");
    assert!(
        plan.iter().any(|m| m["body"]
            .as_str()
            .unwrap()
            .contains("waits until you approve")),
        "the room is told the rest is waiting: {}",
        json!(plan)
    );
    let pending: Vec<&Value> = all
        .iter()
        .filter(|m| m["proposal"]["state"] == json!("pending"))
        .collect();
    assert_eq!(pending.len(), 1, "exactly one thing to approve");

    // Three model calls: the plan, and one per step that ran. The fourth step's
    // turn was never taken, which is the property above stated in tokens.
    assert_eq!(calls(&seen), 3);

    // The planner chose an **agent**, not a tool: it was shown the roster and
    // no tool descriptions at all.
    let planner = system_of(&seen, 0);
    assert!(planner.contains("- @inventory: You are the alo Inventory agent"));
    assert!(planner.contains("- @tasks: You are the alo Tasks agent"));
    assert!(
        !planner.contains("create_task"),
        "the planner routes, {planner}"
    );
    // And the step was taken as the Inventory agent: its prompt, not alo's.
    let step = system_of(&seen, 1);
    assert!(step.starts_with("You are the alo Inventory agent"));
    assert!(!step.contains("- create_task:"), "another product's tool");
    assert!(user_of(&seen, 1).contains("is the X100 in stock?"));

    // The agents that took part are in the room now — an agent that answers in
    // a conversation is a participant in it.
    let (status, present) = get(
        &h.app,
        &h.token,
        &format!("/chat/channels/{channel}/agents"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{present}");
    let handles: Vec<&str> = present["agents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["handle"].as_str().unwrap())
        .collect();
    assert!(handles.contains(&"alo"));
    assert!(handles.contains(&"inventory"));
    assert!(handles.contains(&"tasks"));
    assert!(
        !handles.contains(&"mail"),
        "a step that never ran joins nothing"
    );

    // Approving it runs it, through the same boundary any proposal goes
    // through — and the proposal is the Tasks agent's message, so that is the
    // scope it runs at.
    let id = proposed[0]["proposal"]["id"].as_str().unwrap();
    let (status, done) = post(
        &h.app,
        &h.token,
        &format!("/chat/proposals/{id}"),
        json!({ "approve": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{done}");
    assert_eq!(done["state"], json!("approved"));
    assert!(!done["result"].is_null(), "the task was actually created");
}

/// **A step is refused another product's tool**, exactly as it would be if the
/// person had typed `@inventory` themselves — which is what proves the run
/// carries the delegate's scope rather than Ask alo's.
///
/// `whats_on` is the Agenda agent's, and Ask alo *is* offered it. If a
/// delegated step ran at the orchestrator's scope this lookup would succeed,
/// and the assertion below would find a diary instead of a refusal.
#[tokio::test]
async fn a_step_is_scoped_to_its_own_agent_and_not_to_ask_alo() {
    let h = harness("orchscope").await;
    let (channel, _alo) = a_room_with_ask_alo(&h).await;
    let inventory = an_agent(&h, "inventory", AgentProduct::Inventory).await;

    let (base, seen) = scripted_model(vec![
        plans(&[("inventory", "what is in stock this week?")]),
        wants(
            "whats_on",
            json!({ "from": "2026-08-15", "to": "2026-08-22" }),
            "Looking.",
        ),
        says("I can't read the diary — the Agenda agent can."),
    ])
    .await;
    use_model(&h, &base).await;

    let all = ask_and_wait(&h, &channel, "@alo what is in stock this week?", |all| {
        all.iter().any(|m| {
            m["body"]
                .as_str()
                .unwrap_or_default()
                .contains("Agenda agent can")
        })
    })
    .await;
    assert_eq!(said_by(&all, &inventory).len(), 1);

    // The refusal came back to the model as the lookup's result, naming the
    // agent that was actually asked. Ask alo would have been allowed it.
    let after = user_of(&seen, 2);
    assert!(
        after.contains("whats_on is not a tool the inventory agent has"),
        "the step ran at the delegate's scope: {after}"
    );
    assert!(after.contains("this lookup did not run"));
    // Nothing was executed, so nothing was proposed either.
    assert!(all.iter().all(|m| m["proposal"].is_null()));
}

/// **Stop ends the run between its steps.** The single-turn Stop only declined
/// to post one answer; here there is a plan in flight, and stopping has to keep
/// the rest of it from happening at all.
#[tokio::test]
async fn stop_ends_a_run_part_way_through_its_plan() {
    let h = harness("orchstop").await;
    let (channel, alo) = a_room_with_ask_alo(&h).await;
    let mail = an_agent(&h, "mail", AgentProduct::Mail).await;
    let tasks = an_agent(&h, "tasks", AgentProduct::Tasks).await;
    let inventory = an_agent(&h, "inventory", AgentProduct::Inventory).await;

    // Paced, so there is a run to interrupt rather than a race to win.
    let (base, seen) = scripted_model_paced(
        vec![
            plans(&[
                ("mail", "who wrote to us about the X100?"),
                ("tasks", "what is on my plate?"),
                ("inventory", "how many X100 are left?"),
            ]),
            says("Ilse at ABC Supplies wrote on Tuesday."),
            says("Three things, none overdue."),
            says("Forty-two."),
        ],
        Duration::from_millis(500),
    )
    .await;
    use_model(&h, &base).await;

    // Wait until the first step has actually answered: the plan is in the room,
    // one step is done, and the next is under way.
    let all = ask_and_wait(&h, &channel, "@alo catch me up on the X100", |all| {
        all.iter().any(|m| {
            m["body"]
                .as_str()
                .unwrap_or_default()
                .contains("ABC Supplies wrote")
        })
    })
    .await;
    assert_eq!(said_by(&all, &mail).len(), 1);

    let (status, running) = get(&h.app, &h.token, &format!("/chat/channels/{channel}/turns")).await;
    assert_eq!(status, StatusCode::OK, "{running}");
    let turn = running["turns"][0]["id"]
        .as_str()
        .expect("a turn is running");
    assert_eq!(running["turns"][0]["mine"], json!(true));
    let (status, _) = post(
        &h.app,
        &h.token,
        &format!("/chat/channels/{channel}/turns/{turn}/stop"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let all = wait_for(&h, &channel, |all| {
        all.iter().any(|m| {
            m["body"]
                .as_str()
                .unwrap_or_default()
                .starts_with("Stopped —")
        })
    })
    .await;

    // It said how far it got, and it did not get to the end.
    let stopped = said_by(&all, &alo)
        .into_iter()
        .find(|m| m["body"].as_str().unwrap().starts_with("Stopped —"))
        .unwrap();
    let line = stopped["body"].as_str().unwrap();
    assert!(line.ends_with(" of 3 steps."), "{line}");
    assert!(
        !line.contains("3 of 3"),
        "a finished run is not a stopped one"
    );

    // The last step never ran — which is the difference between stopping a run
    // and declining to post its final answer.
    assert!(said_by(&all, &inventory).is_empty());
    assert!(calls(&seen) < 4, "the last step's turn was never taken");
    // Nothing more arrives after it: the run is over, not paused.
    tokio::time::sleep(Duration::from_millis(800)).await;
    let settled = messages(&h, &channel).await;
    assert_eq!(settled.len(), all.len());
    assert!(said_by(&settled, &tasks).len() <= 1);
}

/// A workspace with no product agents to route to still has an assistant: Ask
/// alo falls back to the ordinary turn, with its own tools.
#[tokio::test]
async fn with_nobody_to_route_to_ask_alo_takes_an_ordinary_turn() {
    let h = harness("orchalone").await;
    let (channel, alo) = a_room_with_ask_alo(&h).await;

    let (base, seen) = scripted_model(vec![says("Nothing in your workspace mentions that.")]).await;
    use_model(&h, &base).await;

    let all = ask_and_wait(&h, &channel, "@alo what happened yesterday?", |all| {
        all.iter().any(|m| m["authorKind"] == "agent")
    })
    .await;
    let spoke = said_by(&all, &alo);
    assert_eq!(spoke.len(), 1);
    assert_eq!(
        spoke[0]["body"],
        json!("Nothing in your workspace mentions that.")
    );
    // One call, and it was the ordinary agent turn: the Workspace tool menu,
    // not a roster of nobody.
    assert_eq!(calls(&seen), 1);
    let prompt = system_of(&seen, 0);
    assert!(prompt.contains("- create_task:"));
    assert!(!prompt.contains("The agents you may ask"));
}

/// **A module this person cannot open has no agent to route to** (A1.5, A1.6).
///
/// The roster is read through the same module-gated list the agent picker uses,
/// so an agent an admin switched off is not describable to the planner and
/// cannot be named by a step. Orchestration is a new surface, and a new surface
/// is exactly where a gate gets forgotten — so it is asserted here rather than
/// assumed from the list route.
#[tokio::test]
async fn an_agent_of_a_denied_module_is_not_on_the_roster() {
    let h = harness("orchdenied").await;
    let (channel, alo) = a_room_with_ask_alo(&h).await;
    let inventory = an_agent(&h, "inventory", AgentProduct::Inventory).await;
    an_agent(&h, "tasks", AgentProduct::Tasks).await;

    // The admin switches Inventory off for the asker, through the store the
    // console writes through.
    let admin = h.ts.create_user("console@orchdenied.test").await.unwrap();
    h.ts.set_admin(&admin, true).await.unwrap();
    h.ts.set_module_access(&h.user, AppModule::Inventory, false, &admin)
        .await
        .unwrap();

    // The planner names it anyway — a model is not a permission system — and
    // the step is dropped before it can run, leaving the plan with the one
    // agent this person can actually reach.
    let (base, seen) = scripted_model(vec![
        plans(&[
            ("inventory", "how many X100 are left?"),
            ("tasks", "what is on my plate?"),
        ]),
        says("Three things, none overdue."),
    ])
    .await;
    use_model(&h, &base).await;

    let all = ask_and_wait(&h, &channel, "@alo where am I?", |all| {
        all.iter().any(|m| {
            m["body"]
                .as_str()
                .unwrap_or_default()
                .contains("none overdue")
        })
    })
    .await;
    assert!(
        said_by(&all, &inventory).is_empty(),
        "a denied module answered: {}",
        json!(all)
    );
    let plan = said_by(&all, &alo);
    assert!(!plan[0]["body"].as_str().unwrap().contains("@inventory"));
    assert!(plan[0]["body"].as_str().unwrap().contains("@tasks"));
    // It was never offered either, so the model could not have chosen it from
    // anything this workspace told it.
    assert!(!system_of(&seen, 0).contains("@inventory"));
    assert!(system_of(&seen, 0).contains("- @tasks:"));
    assert_eq!(calls(&seen), 2, "one plan, one step");
}

/// A request no agent covers is answered by Ask alo itself, with no plan and no
/// delegation — the right answer to "hello", and the reason the planner has an
/// answer envelope at all.
#[tokio::test]
async fn a_request_needing_no_agent_is_answered_by_ask_alo_itself() {
    let h = harness("orchhello").await;
    let (channel, alo) = a_room_with_ask_alo(&h).await;
    let mail = an_agent(&h, "mail", AgentProduct::Mail).await;

    let (base, seen) = scripted_model(vec![
        json!({ "kind": "answer", "answer": "Hello — ask me about your mail, tasks or stock." })
            .to_string(),
    ])
    .await;
    use_model(&h, &base).await;

    let all = ask_and_wait(&h, &channel, "@alo hello", |all| {
        all.iter().any(|m| m["authorKind"] == "agent")
    })
    .await;
    let spoke = said_by(&all, &alo);
    assert_eq!(spoke.len(), 1);
    assert!(spoke[0]["body"].as_str().unwrap().starts_with("Hello —"));
    assert!(!spoke[0]["body"].as_str().unwrap().contains("Here's how"));
    assert!(said_by(&all, &mail).is_empty(), "nobody was delegated to");
    assert_eq!(calls(&seen), 1);
    // It was the planner that answered, and it was shown the roster.
    assert!(system_of(&seen, 0).contains("- @mail: You are the alo Mail agent"));
}
