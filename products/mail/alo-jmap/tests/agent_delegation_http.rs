//! One agent handing a sub-question to another inside its run, on the wire
//! (ADR 0057 §3, A5.1).
//!
//! The properties, none of which can be seen from a unit test:
//!
//! * the **room sees the handoff line** before the delegate's turn runs, and
//!   the delegate itself posts nothing — its answer is **folded in** as a
//!   numbered source the asking agent cites;
//! * the delegate's turn is an ordinary turn of its own: **its prompt, its
//!   grounding, its scope**, through the asker's account door;
//! * a handle the asker cannot see — another tenant's agent, an agent of a
//!   module switched off for them — is **dropped**, with no room line and no
//!   turn taken;
//! * the run is **bounded**: at most four handoffs, refusals included, and a
//!   chain no deeper than two;
//! * a **write a delegate wants is proposed by the delegate itself** (A5.2):
//!   it joins the room and posts its own sentence — the author of the message
//!   is the scope the approval later runs at, so it must be the delegate —
//!   and the run ends behind that one pending button, the asker's to tap;
//! * the delegate's whole data path is the **asker's** (A5.3): its grounding
//!   and the reads it executes carry the asker's own records and nobody
//!   else's — not a colleague's, not another tenant's — and nothing the run
//!   does lands in any room but the one the question was asked in;
//! * **a shared room is not a way round the module gate** (A5.3): an agent
//!   another member put in the very room the ask happens in is still dropped
//!   for an asker whose module was switched off — membership grants nothing,
//!   visibility is the only door — while the member who has the module can
//!   hand off to it in the same room.
//!
//! No live model is called: the tenant's AI backend is the scripted local
//! socket in `common::model`, which records what each turn was shown — and
//! what the model was shown is where the folding and the scoping are visible.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use crate::common::model::{Seen, says, scripted_model, use_model, wants};
use crate::common::{Harness, harness, harness_on, send};
use alo_store::{
    AccountStore, AgentProduct, AppModule, CalendarEvent, ChatAgentId, ChatChannelId, EventId,
};

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

/// The delegate envelope, as the model returns it.
fn delegates(to: &str, ask: &str) -> String {
    json!({ "kind": "delegate", "delegate": { "to": to, "ask": ask } }).to_string()
}

/// A room with one product agent in it — the agent that will be addressed.
/// Every other agent a test defines stays out of the room: a handoff needs no
/// membership of its target, only visibility.
async fn a_room_with(h: &Harness, handle: &str, product: AgentProduct) -> (String, ChatAgentId) {
    let (status, body) = post(
        &h.app,
        &h.token,
        "/chat/channels",
        json!({ "name": "ops", "visibility": "public" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let channel = body["id"].as_str().unwrap().to_owned();
    let agent = h
        .acc
        .create_agent(handle, handle, Some("its product's agent"), product)
        .await
        .unwrap();
    h.acc
        .add_agent_to_channel(&ChatChannelId::new(channel.clone()), &agent)
        .await
        .unwrap();
    (channel, agent)
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
async fn ask_and_wait(
    h: &Harness,
    channel: &str,
    question: &str,
    done: impl Fn(&[Value]) -> bool,
) -> Vec<Value> {
    ask_as(h, &h.token, channel, question, done).await
}

/// [`ask_and_wait`], as whoever holds `token` — the isolation tests need two
/// people asking in the same room.
async fn ask_as(
    h: &Harness,
    token: &str,
    channel: &str,
    question: &str,
    done: impl Fn(&[Value]) -> bool,
) -> Vec<Value> {
    let (status, body) = post(
        &h.app,
        token,
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

/// The handoff lines in the room, in order.
fn handoff_lines(all: &[Value]) -> Vec<String> {
    all.iter()
        .filter_map(|m| m["body"].as_str())
        .filter(|body| body.starts_with("I'm asking @"))
        .map(str::to_owned)
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
/// the sources, the folded answers, and the handoff offer.
fn user_of(seen: &Seen, n: usize) -> String {
    seen.lock().unwrap()[n]["messages"][1]["content"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn calls(seen: &Seen) -> usize {
    seen.lock().unwrap().len()
}

/// **The whole shape of a handoff**: the room sees who asked whom for what,
/// the delegate takes an ordinary turn of its own — its prompt, its reading
/// tool run for real — and its answer comes back to the asking agent as a
/// numbered source it cites. The delegate itself posts nothing.
#[tokio::test]
async fn a_handoff_runs_the_delegates_turn_and_folds_the_answer_in() {
    let h = harness("delegfold").await;
    let (channel, billing) = a_room_with(&h, "billing", AgentProduct::Billing).await;
    let tasks = an_agent(&h, "tasks", AgentProduct::Tasks).await;

    let (base, seen) = scripted_model(vec![
        delegates("tasks", "what is on my plate this week?"),
        wants("my_plate", json!({}), "Looking at the list."),
        says("Nothing is due this week."),
        says("Nothing stands in the way — @tasks says nothing is due [1]."),
    ])
    .await;
    use_model(&h, &base).await;

    let all = ask_and_wait(
        &h,
        &channel,
        "@billing is anything blocking the Northstar quote?",
        |all| {
            all.iter().any(|m| {
                m["body"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("Nothing stands in the way")
            })
        },
    )
    .await;

    // The room saw the handoff line, said by the asking agent, before the
    // answer — and the delegate itself said nothing at all.
    let lines = handoff_lines(&all);
    assert_eq!(
        lines,
        vec!["I'm asking @tasks: what is on my plate this week?"]
    );
    let spoke = said_by(&all, &billing);
    assert_eq!(spoke.len(), 2, "the handoff line and the answer");
    assert!(
        spoke[0]["body"]
            .as_str()
            .unwrap()
            .starts_with("I'm asking @tasks:")
    );
    assert!(said_by(&all, &tasks).is_empty(), "a delegate posts nothing");

    // Four model calls: the handoff decision, the delegate's read turn, the
    // delegate's answer, the asking agent's answer over the folded source.
    assert_eq!(calls(&seen), 4);

    // The offer named the roster; the delegate's turn was its own — its
    // prompt, its request — and its read actually ran before it answered.
    assert!(user_of(&seen, 0).contains("@tasks (the tasks agent)"));
    let delegate_prompt = system_of(&seen, 1);
    assert!(
        delegate_prompt.starts_with("You are the alo Tasks agent"),
        "{delegate_prompt}"
    );
    assert!(user_of(&seen, 1).contains("what is on my plate this week?"));
    let after_read = user_of(&seen, 2);
    assert!(after_read.contains("my_plate"));
    assert!(after_read.contains("result of a tool you just ran"));

    // The answer came back to the asking agent as a citable numbered source.
    let folded = user_of(&seen, 3);
    assert!(
        folded.contains("delegated answer \"@tasks\""),
        "the fold names who was asked: {folded}"
    );
    assert!(folded.contains("@tasks answered: Nothing is due this week."));

    // Reads answer, writes propose, handoffs fold: nothing here waits on a tap.
    assert!(all.iter().all(|m| m["proposal"].is_null()));
}

/// **Only agents the asker can see.** A handle from another tenant and the
/// handle of a module switched off for the asker meet the same fate: dropped
/// with a line the model can answer around — no handoff line in the room, no
/// delegate turn taken. This is the wrong-tenant test of the handoff surface.
#[tokio::test]
async fn a_handle_the_asker_cannot_see_is_dropped_without_a_room_line() {
    let h = harness("delegdrop").await;
    let (channel, billing) = a_room_with(&h, "billing", AgentProduct::Billing).await;
    // Tenant B's agent, on the same store — visible to nobody in tenant A.
    let other = harness_on(std::sync::Arc::clone(&h.store), "delegdropb").await;
    an_agent(&other, "ghost", AgentProduct::Inventory).await;
    // …and an agent of A's own whose module an admin switched off for the
    // asker, through the store the console writes through.
    an_agent(&h, "inventory", AgentProduct::Inventory).await;
    let admin = h.ts.create_user("console@delegdrop.test").await.unwrap();
    h.ts.set_admin(&admin, true).await.unwrap();
    h.ts.set_module_access(&h.user, AppModule::Inventory, false, &admin)
        .await
        .unwrap();

    let (base, seen) = scripted_model(vec![
        delegates("ghost", "is the X100 in stock?"),
        delegates("inventory", "is the X100 in stock?"),
        says("I couldn't reach anyone who can check the stock."),
    ])
    .await;
    use_model(&h, &base).await;

    let all = ask_and_wait(&h, &channel, "@billing can we fulfil the quote?", |all| {
        all.iter().any(|m| {
            m["body"]
                .as_str()
                .unwrap_or_default()
                .contains("couldn't reach anyone")
        })
    })
    .await;

    // Neither handle produced a handoff line or a delegate turn — the model
    // was told there is nobody by that name, and answered around it.
    assert!(handoff_lines(&all).is_empty(), "{}", json!(all));
    assert_eq!(said_by(&all, &billing).len(), 1);
    assert_eq!(calls(&seen), 3, "no delegate turn was ever taken");
    assert!(user_of(&seen, 1).contains("there is no @ghost"));
    assert!(user_of(&seen, 2).contains("there is no @inventory"));
    // The offer itself never named either: one is another tenant's, the other
    // is behind the module gate.
    assert!(!user_of(&seen, 0).contains("@ghost"));
    assert!(!user_of(&seen, 0).contains("@inventory"));
}

/// **At most four handoffs per run, refusals included.** The fifth is refused
/// without a model call for it, and the run ends saying so — bounded in code,
/// not in the prompt.
#[tokio::test]
async fn the_fifth_handoff_is_refused_and_the_run_ends() {
    let h = harness("delegcap").await;
    let (channel, billing) = a_room_with(&h, "billing", AgentProduct::Billing).await;
    an_agent(&h, "tasks", AgentProduct::Tasks).await;

    let script = vec![
        delegates("tasks", "part one?"),
        says("one"),
        delegates("tasks", "part two?"),
        says("two"),
        delegates("tasks", "part three?"),
        says("three"),
        delegates("tasks", "part four?"),
        says("four"),
        // The fifth handoff: refused by the budget, so this is the last call.
        delegates("tasks", "part five?"),
    ];
    let (base, seen) = scripted_model(script).await;
    use_model(&h, &base).await;

    let all = ask_and_wait(&h, &channel, "@billing reconcile everything", |all| {
        all.iter().any(|m| {
            m["body"]
                .as_str()
                .unwrap_or_default()
                .contains("ask the remaining part")
        })
    })
    .await;

    assert_eq!(handoff_lines(&all).len(), 4, "{}", json!(all));
    let last = said_by(&all, &billing).last().unwrap()["body"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(last.contains("as much as I'm allowed to"), "{last}");
    // Nine calls: four (decision + delegate answer) pairs and the refused
    // fifth decision. The fifth delegate turn was never taken.
    assert_eq!(calls(&seen), 9);
    assert!(all.iter().all(|m| m["proposal"].is_null()));
}

/// **A chain ends at depth two.** The turn two handoffs down is offered
/// nobody, and a stray envelope from it is dropped like an unknown handle —
/// so the third hop never happens and every answer still folds back up.
#[tokio::test]
async fn a_handoff_chain_ends_at_depth_two() {
    let h = harness("delegdeep").await;
    let (channel, billing) = a_room_with(&h, "billing", AgentProduct::Billing).await;
    an_agent(&h, "inventory", AgentProduct::Inventory).await;
    an_agent(&h, "tasks", AgentProduct::Tasks).await;

    let (base, seen) = scripted_model(vec![
        delegates("inventory", "can we ship the X100 order?"),
        delegates("tasks", "is there an open recount task?"),
        // The turn at depth two tries a third hop anyway.
        delegates("billing", "what did we quote?"),
        says("No open recount task."),
        says("Stock is fine and no recount is open."),
        says("All clear to ship."),
    ])
    .await;
    use_model(&h, &base).await;

    let all = ask_and_wait(&h, &channel, "@billing can we ship?", |all| {
        all.iter().any(|m| {
            m["body"]
                .as_str()
                .unwrap_or_default()
                .contains("All clear to ship")
        })
    })
    .await;

    // Two handoff lines — the third hop was never made.
    let lines = handoff_lines(&all);
    assert_eq!(lines.len(), 2, "{}", json!(all));
    assert!(lines[0].starts_with("I'm asking @inventory:"));
    assert!(lines[1].starts_with("I'm asking @tasks:"));

    // The offer is made at depths zero and one, and not at two; the stray
    // envelope from depth two met the same line an unknown handle does.
    assert!(user_of(&seen, 0).contains("You can hand off to:"));
    assert!(user_of(&seen, 1).contains("You can hand off to:"));
    assert!(!user_of(&seen, 2).contains("You can hand off to:"));
    assert!(user_of(&seen, 3).contains("there is no @billing"));

    // Each answer folded into the turn above it.
    assert!(user_of(&seen, 4).contains("@tasks answered: No open recount task."));
    assert!(user_of(&seen, 5).contains("@inventory answered: Stock is fine"));
    assert_eq!(calls(&seen), 6);
    assert_eq!(said_by(&all, &billing).len(), 2, "its line and its answer");
}

/// **A delegate's write lands on the asker's one approval surface** (A5.2).
/// The delegate joins the room and posts the proposal itself — under its own
/// id, which is the scope the approval later runs at — the run ends behind
/// that single pending button, and the asker's tap actually runs the change.
#[tokio::test]
async fn a_delegates_write_is_proposed_by_the_delegate_and_the_asker_approves_it() {
    let h = harness("delegwrite").await;
    let (channel, billing) = a_room_with(&h, "billing", AgentProduct::Billing).await;
    let tasks = an_agent(&h, "tasks", AgentProduct::Tasks).await;

    let (base, seen) = scripted_model(vec![
        delegates("tasks", "add a follow-up for the Northstar quote"),
        wants(
            "create_task",
            json!({ "title": "Follow up on the Northstar quote" }),
            "I'll add a follow-up task.",
        ),
    ])
    .await;
    use_model(&h, &base).await;

    let all = ask_and_wait(&h, &channel, "@billing chase the Northstar quote", |all| {
        all.iter()
            .any(|m| m["proposal"]["state"] == json!("pending"))
    })
    .await;

    // The room read the handoff line, then the delegate's own sentence with
    // the button on it — and nothing after: the run is over, waiting on the
    // one approval. The asking agent was never asked to comment on a change
    // that is not its to make.
    assert_eq!(
        handoff_lines(&all),
        vec!["I'm asking @tasks: add a follow-up for the Northstar quote"]
    );
    assert_eq!(
        said_by(&all, &billing).len(),
        1,
        "the handoff line and nothing after: {}",
        json!(all)
    );
    assert_eq!(calls(&seen), 2);

    // The proposal is the DELEGATE's message: tasks joined the room to say it,
    // so the approval runs at the Tasks agent's scope, not Billing's.
    let proposed = said_by(&all, &tasks);
    assert_eq!(proposed.len(), 1, "the delegate speaks its own proposal");
    assert_eq!(proposed[0]["body"], json!("I'll add a follow-up task."));
    assert_eq!(proposed[0]["proposal"]["tool"], json!("create_task"));
    assert_eq!(proposed[0]["proposal"]["state"], json!("pending"));
    let pending: Vec<&Value> = all
        .iter()
        .filter(|m| m["proposal"]["state"] == json!("pending"))
        .collect();
    assert_eq!(pending.len(), 1, "exactly one thing to approve");

    // The asker's tap runs it, through the same boundary every proposal
    // crosses — and only then does the task exist.
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

// ---- A5.3: the delegate's whole data path is the asker's ---------------------

/// The day every diary entry below is on, and the day the delegate is asked
/// about.
const DAY: &str = "2027-03-11";

/// One diary entry on this person's own calendar, on [`DAY`]. Same shape as
/// the A1.6 isolation suite's, because it is the same question being asked of
/// a deeper surface.
async fn diary(acc: &AccountStore, summary: &str) {
    use time::{Date, Month, Time};
    let calendar = acc.ensure_personal_calendar().await.unwrap();
    let start = Date::from_calendar_date(2027, Month::March, 11)
        .unwrap()
        .with_time(Time::from_hms(9, 0, 0).unwrap())
        .assume_utc();
    acc.create_event(&CalendarEvent {
        id: EventId::generate(),
        calendar_id: calendar,
        summary: summary.to_owned(),
        description: None,
        location: None,
        starts_at: start,
        ends_at: start + time::Duration::hours(1),
        all_day: false,
        recurrence: None,
        attendees: Vec::new(),
        exdates: Vec::new(),
        timezone: None,
        rdates: Vec::new(),
        recurrence_id: None,
        reminder_minutes: None,
        attendee_status: Vec::new(),
    })
    .await
    .unwrap();
}

/// A second person of the tenant: their own login, and their own door into
/// the store.
async fn colleague(h: &Harness, tag: &str) -> (String, AccountStore) {
    let email = format!("{tag}-{}@deleg.test", h.tenant);
    let user = h.ts.create_user(&email).await.unwrap();
    h.identity
        .set_password(&h.tenant, &user, &email, "s3cret-pw")
        .await
        .unwrap();
    let token = h
        .identity
        .password_login(&email, "s3cret-pw", None)
        .await
        .unwrap()
        .expect("token issued")
        .0
        .reveal()
        .to_owned();
    (token, h.store.for_account(h.tenant.clone(), user))
}

/// **A delegate reaches nothing the asker could not** (A5.3). Its grounding
/// and the read it executes inside its turn run through the asker's own
/// account door: the asker's diary entry is there, a colleague's private
/// entry and another tenant's are not — on the same day, matching the same
/// question, so what separates them is access and nothing else. And the run
/// stays in its room: a second channel the delegate is a member of gains
/// nothing.
#[tokio::test]
async fn a_delegates_grounding_and_reads_are_the_askers_and_nobody_elses() {
    let h = harness("delegiso").await;
    let (channel, billing) = a_room_with(&h, "billing", AgentProduct::Billing).await;
    let agenda = an_agent(&h, "agenda", AgentProduct::Agenda).await;

    // Three diaries, one day, one word: the asker's own, a colleague's the
    // asker may not read, and another tenant's on the same store.
    diary(&h.acc, "kestrel planning").await;
    let (_, ben) = colleague(&h, "ben").await;
    diary(&ben, "kestrel review with the board").await;
    let other = harness_on(std::sync::Arc::clone(&h.store), "delegisob").await;
    diary(&other.acc, "kestrel dinner with the board").await;

    // A second room with the delegate in it, which the run must not touch.
    let (status, staffing) = post(
        &h.app,
        &h.token,
        "/chat/channels",
        json!({ "name": "staffing", "visibility": "public" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{staffing}");
    let staffing = staffing["id"].as_str().unwrap().to_owned();
    h.acc
        .add_agent_to_channel(&ChatChannelId::new(staffing.clone()), &agenda)
        .await
        .unwrap();

    let (base, seen) = scripted_model(vec![
        delegates("agenda", "what is on the diary about the kestrel?"),
        wants(
            "whats_on",
            json!({ "from": DAY, "to": DAY }),
            "Checking the diary.",
        ),
        says("Kestrel planning, at nine."),
        says("The diary holds the kestrel planning at nine [1]."),
    ])
    .await;
    use_model(&h, &base).await;

    let all = ask_and_wait(&h, &channel, "@billing anything on the kestrel?", |all| {
        all.iter().any(|m| {
            m["body"]
                .as_str()
                .unwrap_or_default()
                .contains("planning at nine")
        })
    })
    .await;

    // The room saw the handoff and the answer; the delegate posted nothing.
    assert_eq!(
        handoff_lines(&all),
        vec!["I'm asking @agenda: what is on the diary about the kestrel?"]
    );
    assert!(
        said_by(&all, &agenda).is_empty(),
        "a delegate posts nothing"
    );
    assert_eq!(said_by(&all, &billing).len(), 2);
    assert_eq!(calls(&seen), 4);

    // The delegate's grounding (its first call) and the diary read it executed
    // (folded into its second) carry the asker's entry and nobody else's. The
    // negatives mean something because the positive is beside them: all three
    // entries are on the same day and match the same word.
    for call in [1, 2] {
        let shown = user_of(&seen, call);
        assert!(
            shown.contains("kestrel planning"),
            "call {call} must carry the asker's own diary: {shown}"
        );
        assert!(
            !shown.contains("kestrel review"),
            "call {call} must carry no colleague's diary: {shown}"
        );
        assert!(
            !shown.contains("kestrel dinner"),
            "call {call} must carry no other tenant's diary: {shown}"
        );
    }

    // The run happened in its room and nowhere else: the other channel the
    // delegate is a member of gained no message.
    assert!(
        messages(&h, &staffing).await.is_empty(),
        "a delegation never crosses into another channel"
    );
}

/// **A shared room is not a way round the module gate** (A5.3's "never across
/// a shared channel"). A colleague who has Inventory puts `@inventory` in the
/// very room the ask happens in; for an asker whose Inventory was switched
/// off, the handle still resolves to nobody — membership grants nothing,
/// the asker's own module-gated roster is the only door. The colleague's own
/// handoff in the same room is the paired positive: the drop is the gate
/// biting, not a handoff that never works.
#[tokio::test]
async fn a_shared_room_is_not_a_way_round_the_module_gate_for_a_handoff() {
    let h = harness("delegroom").await;
    let (channel, billing) = a_room_with(&h, "billing", AgentProduct::Billing).await;
    let inventory = an_agent(&h, "inventory", AgentProduct::Inventory).await;
    let room = ChatChannelId::new(channel.clone());

    // Carol joins the shared room and puts the Inventory agent in it herself.
    let (carol_token, carol) = colleague(&h, "carol").await;
    carol.join_channel(&room).await.unwrap();
    carol.add_agent_to_channel(&room, &inventory).await.unwrap();

    // An admin switches Inventory off for the asker — and only the asker.
    let admin = h.ts.create_user("console@delegroom.test").await.unwrap();
    h.ts.set_admin(&admin, true).await.unwrap();
    h.ts.set_module_access(&h.user, AppModule::Inventory, false, &admin)
        .await
        .unwrap();

    // The same room, to two people: the member list itself already differs.
    let mine: Vec<String> = h
        .acc
        .channel_agents(&room)
        .await
        .unwrap()
        .into_iter()
        .map(|agent| agent.handle)
        .collect();
    assert_eq!(mine, vec!["billing"], "the asker has no @inventory to see");
    let carols: Vec<String> = carol
        .channel_agents(&room)
        .await
        .unwrap()
        .into_iter()
        .map(|agent| agent.handle)
        .collect();
    assert_eq!(carols, vec!["billing", "inventory"]);

    let (base, seen) = scripted_model(vec![
        // The asker's run: the handle is dropped, no delegate turn is taken.
        delegates("inventory", "is the X100 in stock?"),
        says("I couldn't reach anyone who can check the stock."),
        // Carol's run in the same room: the same handle resolves and answers.
        delegates("inventory", "is the X100 in stock?"),
        says("The X100: twelve in stock."),
        says("Twelve on the shelf — @inventory checked [1]."),
    ])
    .await;
    use_model(&h, &base).await;

    let all = ask_and_wait(
        &h,
        &channel,
        "@billing can we fulfil the X100 order?",
        |all| {
            all.iter().any(|m| {
                m["body"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("couldn't reach anyone")
            })
        },
    )
    .await;
    // No handoff line, no delegate turn: the model was told there is nobody by
    // that name — the agent sitting in the room notwithstanding — and the
    // offer never named it either.
    assert!(handoff_lines(&all).is_empty(), "{}", json!(all));
    assert_eq!(calls(&seen), 2);
    assert!(user_of(&seen, 1).contains("there is no @inventory"));
    assert!(!user_of(&seen, 0).contains("@inventory"));

    let all = ask_as(
        &h,
        &carol_token,
        &channel,
        "@billing can we fulfil the X100 order?",
        |all| {
            all.iter().any(|m| {
                m["body"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("Twelve on the shelf")
            })
        },
    )
    .await;
    // For Carol the very same handle in the very same room is offered, asked,
    // and answers.
    assert_eq!(
        handoff_lines(&all),
        vec!["I'm asking @inventory: is the X100 in stock?"]
    );
    assert_eq!(calls(&seen), 5);
    assert!(user_of(&seen, 2).contains("@inventory"));
    assert_eq!(said_by(&all, &billing).len(), 3, "{}", json!(all));
}
