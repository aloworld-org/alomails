//! The agent directory on the wire (A3.3) — what each agent is for, what it may
//! touch, and what it has done.
//!
//! Three properties, and the last two are the ones a unit test cannot show:
//!
//! - the roster describes every agent this tenant has, in the tenant's own
//!   words, with the registry's account of its reach beside it — and a fresh
//!   tenant's first look is a full one, because the directory seeds through the
//!   same call the agent list does;
//! - **a module switched off has no entry** anywhere a client can look: not in
//!   the roster, and asking for it by id is the same 404 an id that was never
//!   issued gets, so the directory is not an oracle for what a colleague has;
//! - **what an agent has done is the asker's own.** A run recorded for one
//!   person is absent from a colleague's entry for the same agent, tallies
//!   included.
//!
//! No live model is called: the tenant's AI backend is the scripted local
//! socket in `common::model`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use crate::common::model::{says, scripted_model, use_model, wants};
use crate::common::{Harness, harness, send};
use alo_store::{ALL_AGENT_PRODUCTS, AgentProduct, AppModule, ChatChannelId};

// ---- request helpers ---------------------------------------------------------

async fn get(app: &Router, token: &str, uri: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    send(app, req).await
}

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

fn find<'a>(body: &'a Value, handle: &str) -> Option<&'a Value> {
    body["agents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["handle"] == handle)
}

/// The tool names in one directory entry.
fn tools(entry: &Value) -> Vec<&str> {
    entry["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect()
}

/// A second person of the same tenant, with their own token.
async fn colleague(h: &Harness, tag: &str) -> String {
    let email = format!("{tag}-{}@agentdir.test", h.tenant);
    let user = h.ts.create_user(&email).await.unwrap();
    h.identity
        .set_password(&h.tenant, &user, &email, "s3cret-pw")
        .await
        .unwrap();
    h.identity
        .password_login(&email, "s3cret-pw", None)
        .await
        .unwrap()
        .expect("token issued")
        .0
        .reveal()
        .to_owned()
}

// ---- what each agent is for, and what it may touch ---------------------------

/// The roster, on a tenant nobody has administered: an entry per product, each
/// saying what asking it is good for and what it can reach.
#[tokio::test]
async fn the_directory_describes_every_agent_and_the_tools_it_may_actually_use() {
    let h = harness("agent-dir").await;

    let (status, body) = get(&h.app, &h.token, "/chat/agents/directory").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["agents"].as_array().unwrap().len(),
        ALL_AGENT_PRODUCTS.len(),
        "{body}"
    );

    for product in ALL_AGENT_PRODUCTS {
        let handle = alo_store::default_handle(product);
        let entry = find(&body, handle).unwrap_or_else(|| panic!("no @{handle} in {body}"));
        assert_eq!(entry["product"], product.as_str(), "@{handle}");
        // What it is for, in the tenant's own words rather than a prompt line.
        assert!(
            entry["name"].as_str().is_some_and(|n| !n.trim().is_empty()),
            "@{handle} has no name"
        );
        let description = entry["description"].as_str().unwrap_or_default();
        assert!(!description.trim().is_empty(), "@{handle} says nothing");
        assert!(
            !description.starts_with("You are"),
            "@{handle} is described with its system prompt: {description}"
        );
        // What it may touch: the registry's own list, with the effect that
        // decides whether a result lands in the room or waits for a tap.
        let listed = tools(entry);
        assert_eq!(
            listed.len(),
            alo_ai::tools_for(product).len(),
            "@{handle}: {listed:?}"
        );
        for tool in entry["tools"].as_array().unwrap() {
            let name = tool["name"].as_str().unwrap();
            assert!(
                alo_ai::offers(product, name),
                "@{handle} is described as reaching {name}, which it would be refused"
            );
            assert!(
                matches!(tool["effect"].as_str(), Some("read" | "write")),
                "@{handle}/{name} has no effect"
            );
        }
        // Which switch decides whether this person has it at all.
        assert_eq!(
            entry["gatedOn"],
            match product.module() {
                Some(module) => json!(module.as_str()),
                None => Value::Null,
            },
            "@{handle}"
        );
        // Nothing has been done yet, and that is a number rather than a silence.
        assert_eq!(entry["answers"], 0, "@{handle}");
        assert_eq!(entry["actions"], 0, "@{handle}");
        assert_eq!(entry["reads"], 0, "@{handle}");
    }

    // Stated plainly, because a directory that overstates a reach is the way
    // somebody learns to ask the wrong agent: correspondence is Mail's alone,
    // stock is Inventory's, and neither describes the other's.
    let mail = find(&body, "mail").unwrap();
    assert!(tools(mail).contains(&"correspondence"));
    assert!(!tools(mail).contains(&"stock_answer"));
    let inventory = find(&body, "inventory").unwrap();
    assert!(tools(inventory).contains(&"stock_answer"));
    assert!(!tools(inventory).contains(&"send_email"));
    // The read/write bit is per tool, not per agent: the Inventory agent both
    // answers and proposes.
    let effects: Vec<&str> = inventory["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["effect"].as_str().unwrap())
        .collect();
    assert!(
        effects.contains(&"read") && effects.contains(&"write"),
        "{effects:?}"
    );

    // The two products with no rail app of their own are gated on Drive's
    // switch, and the directory says so rather than leaving a client to know it.
    assert_eq!(find(&body, "sheets").unwrap()["gatedOn"], json!("drive"));
    assert_eq!(find(&body, "docs").unwrap()["gatedOn"], json!("drive"));
    assert_eq!(find(&body, "mail").unwrap()["gatedOn"], Value::Null);
    assert_eq!(find(&body, "alo").unwrap()["gatedOn"], Value::Null);
    // Ask alo is the one agent whose entry is every tool there is.
    assert_eq!(
        tools(find(&body, "alo").unwrap()).len(),
        alo_ai::tools_for(AgentProduct::Workspace).len()
    );

    // The tenant's language is the one it was seeded in, and the directory
    // reads the same set the agent list does rather than a second one.
    let (status, listed) = get(&h.app, &h.token, "/chat/agents").await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    let mut from_list: Vec<&str> = listed["agents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["id"].as_str().unwrap())
        .collect();
    let mut from_directory: Vec<&str> = body["agents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["id"].as_str().unwrap())
        .collect();
    from_list.sort_unstable();
    from_directory.sort_unstable();
    assert_eq!(from_list, from_directory, "two rosters");

    // Unauthenticated is 401 on both doors, before anything is read.
    let anonymous = Request::builder()
        .uri("/chat/agents/directory")
        .body(Body::empty())
        .unwrap();
    let (status, _) = send(&h.app, anonymous).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let id = find(&body, "mail").unwrap()["id"].as_str().unwrap();
    let anonymous = Request::builder()
        .uri(format!("/chat/agents/{id}/directory"))
        .body(Body::empty())
        .unwrap();
    let (status, _) = send(&h.app, anonymous).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

/// A fresh tenant that opens the directory *before* the agent list still has
/// its agents: the seed is the same call, so neither door is the privileged one.
#[tokio::test]
async fn the_directory_is_a_full_one_even_when_it_is_opened_first() {
    let h = harness("agent-dir-first").await;

    let (status, body) = get(&h.app, &h.token, "/chat/agents/directory?lang=nl").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["agents"].as_array().unwrap().len(),
        ALL_AGENT_PRODUCTS.len()
    );
    // Seeded in the language the first reader asked in — the directory is a
    // first read like any other.
    assert_eq!(find(&body, "hr").unwrap()["name"], json!("Mensen"));

    // And the agent list afterwards is the same set, not a second one.
    let (status, listed) = get(&h.app, &h.token, "/chat/agents?lang=fr").await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert_eq!(
        listed["agents"].as_array().unwrap().len(),
        ALL_AGENT_PRODUCTS.len()
    );
    assert_eq!(find(&listed, "hr").unwrap()["name"], json!("Mensen"));
}

/// A1.5's gate, asked of this surface: an agent of a module this person may not
/// open is not described to them, and asking for it by id says only "not found".
#[tokio::test]
async fn a_denied_module_has_no_directory_entry_and_no_answer_by_id() {
    let h = harness("agent-dir-denied").await;

    let (status, body) = get(&h.app, &h.token, "/chat/agents/directory").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let inventory = find(&body, "inventory").unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let mail = find(&body, "mail").unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();

    // Readable while she still has the module.
    let (status, entry) = get(
        &h.app,
        &h.token,
        &format!("/chat/agents/{inventory}/directory"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{entry}");
    assert!(tools(&entry).contains(&"stock_answer"));
    assert_eq!(entry["recent"].as_array().unwrap().len(), 0);

    // The admin switches Inventory off for her.
    let admin = h.ts.create_user("console@agentdir.test").await.unwrap();
    h.ts.set_admin(&admin, true).await.unwrap();
    h.ts.set_module_access(&h.user, AppModule::Inventory, false, &admin)
        .await
        .unwrap();

    let (status, body) = get(&h.app, &h.token, "/chat/agents/directory").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(find(&body, "inventory").is_none(), "{body}");
    assert_eq!(
        body["agents"].as_array().unwrap().len(),
        ALL_AGENT_PRODUCTS.len() - 1
    );
    // Everything else is still hers.
    assert!(find(&body, "mail").is_some(), "{body}");

    // By id it is 404 — and so is an id that was never issued, so the two
    // cannot be told apart.
    let (status, refused) = get(
        &h.app,
        &h.token,
        &format!("/chat/agents/{inventory}/directory"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{refused}");
    let (invented, _) = get(&h.app, &h.token, "/chat/agents/never-issued/directory").await;
    assert_eq!(invented, StatusCode::NOT_FOUND);
    // …while the one she still has answers normally.
    let (status, entry) = get(&h.app, &h.token, &format!("/chat/agents/{mail}/directory")).await;
    assert_eq!(status, StatusCode::OK, "{entry}");
}

// ---- what it has done --------------------------------------------------------

/// The third of the item, on the wire: a real turn runs a reading tool, and the
/// agent's own entry then shows it — the tally *and* the run behind it. A
/// colleague reading the same entry sees neither.
#[tokio::test]
async fn what_an_agent_has_done_is_in_its_entry_and_is_the_askers_own() {
    let h = harness("agent-dir-record").await;
    let (base, _seen) = scripted_model(vec![
        wants("catch_up_room", json!({ "room": "stock" }), "Let me look."),
        says("You asked about the X100 a moment ago [2]."),
    ])
    .await;
    use_model(&h, &base).await;

    // A room with a Chat agent in it, made the way the product makes them.
    let (status, room) = post(
        &h.app,
        &h.token,
        "/chat/channels",
        json!({ "name": "stock", "visibility": "public" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{room}");
    let channel = room["id"].as_str().unwrap().to_owned();
    let agent = h
        .acc
        .create_agent(
            "rooms",
            "Rooms",
            Some("what a room decided"),
            AgentProduct::Chat,
        )
        .await
        .unwrap();
    h.acc
        .add_agent_to_channel(&ChatChannelId::new(channel.clone()), &agent)
        .await
        .unwrap();

    // Nothing done yet.
    let (status, before) = get(
        &h.app,
        &h.token,
        &format!("/chat/agents/{}/directory", agent.as_str()),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{before}");
    assert_eq!(before["reads"], 0);
    assert_eq!(before["answers"], 0);
    assert_eq!(before["recent"].as_array().unwrap().len(), 0);
    assert_eq!(before["lastAt"], Value::Null);

    // One question, answered from a reading tool run inside the turn.
    let (status, said) = post(
        &h.app,
        &h.token,
        &format!("/chat/channels/{channel}/messages"),
        json!({ "body": "@rooms what did I just ask about?" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{said}");
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let (status, feed) = get(
            &h.app,
            &h.token,
            &format!("/chat/channels/{channel}/messages"),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{feed}");
        if feed["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["authorKind"] == "agent")
        {
            break;
        }
        assert!(Instant::now() < deadline, "the agent never spoke: {feed}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let (status, after) = get(
        &h.app,
        &h.token,
        &format!("/chat/agents/{}/directory", agent.as_str()),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{after}");
    assert_eq!(after["answers"], 1, "{after}");
    assert_eq!(after["reads"], 1, "{after}");
    assert_eq!(after["actions"], 0, "{after}");
    assert!(after["lastAt"].as_str().is_some_and(|s| !s.is_empty()));

    // The run behind the tally, said in the registry's own words.
    let recent = after["recent"].as_array().unwrap();
    assert_eq!(recent.len(), 1, "{after}");
    assert_eq!(recent[0]["tool"], json!("catch_up_room"));
    assert_eq!(recent[0]["effect"], json!("read"));
    assert_eq!(recent[0]["ok"], json!(true));
    assert_eq!(recent[0]["channel"], json!(channel));
    assert!(recent[0]["at"].as_str().is_some_and(|s| s.contains('T')));
    // …and never what it was asked *about*. A tool's arguments carry the body
    // of a draft, a person's name, the text of a document; a summary of what an
    // agent has been doing does not need to repeat any of it.
    assert!(
        recent[0].get("args").is_none(),
        "the directory repeated a tool's arguments: {}",
        recent[0]
    );

    // Ben can see the same agent — its description, its tools, its gate. What
    // it looked up for Anna is hers.
    let ben = colleague(&h, "ben").await;
    let (status, his) = get(
        &h.app,
        &ben,
        &format!("/chat/agents/{}/directory", agent.as_str()),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{his}");
    assert_eq!(his["handle"], json!("rooms"));
    assert_eq!(
        tools(&his),
        tools(&after),
        "the reach is the agent's, not the asker's"
    );
    assert_eq!(his["reads"], 0, "a colleague read another person's runs");
    assert_eq!(his["recent"].as_array().unwrap().len(), 0, "{his}");
    // The room is public, so what the agent *said* is legitimately his to
    // count — the two halves of the record have different scopes on purpose.
    assert_eq!(his["answers"], 1, "{his}");
}
