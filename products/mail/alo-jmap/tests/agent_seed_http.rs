//! The default agent set on the wire, and the module gate on the wire (A1.5).
//!
//! Two things a store test cannot show:
//!
//! - **Nobody registers a handle.** A brand-new tenant's very first
//!   `GET /chat/agents` already answers with an agent for every product, named
//!   in the language the client asked for, and the second call returns the same
//!   ids rather than a second set.
//! - **A denied module has no agent anywhere a client can look.** Not in the
//!   list, not by id, not in a room shared with a colleague who still has it —
//!   and naming it there costs **no model call at all**, which is what proves
//!   the refusal happened before the turn rather than inside the answer.
//!
//! No live model is called: the tenant's AI backend is the scripted local
//! socket in `common::model`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use alo_store::{ALL_AGENT_PRODUCTS, AppModule};
use common::model::{says, scripted_model, use_model};
use common::{Harness, harness, send};

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

/// The agents in a listing, by handle.
fn handles(body: &Value) -> Vec<String> {
    body["agents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["handle"].as_str().unwrap().to_owned())
        .collect()
}

fn find<'a>(body: &'a Value, handle: &str) -> Option<&'a Value> {
    body["agents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["handle"] == handle)
}

/// A second person of the same tenant, with their own token.
async fn colleague(h: &Harness, tag: &str) -> String {
    let email = format!("{tag}-{}@agentseed.test", h.tenant);
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

/// The item's first half, end to end: a tenant nobody has administered opens
/// the agent list and has its agents.
#[tokio::test]
async fn a_new_tenants_first_look_at_the_agent_list_is_a_full_one() {
    let h = harness("agent-seed").await;

    let (status, body) = get(&h.app, &h.token, "/chat/agents").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let first = handles(&body);
    assert_eq!(
        first.len(),
        ALL_AGENT_PRODUCTS.len(),
        "a fresh tenant answered with {first:?}"
    );
    for product in ALL_AGENT_PRODUCTS {
        let handle = alo_store::default_handle(product);
        let agent = find(&body, handle).unwrap_or_else(|| panic!("no @{handle} in {first:?}"));
        assert_eq!(agent["product"], product.as_str(), "@{handle}");
        assert!(
            agent["name"].as_str().is_some_and(|n| !n.trim().is_empty()),
            "@{handle} has no name"
        );
        assert!(
            agent["description"]
                .as_str()
                .is_some_and(|d| !d.trim().is_empty()),
            "@{handle} has no description, so its empty state says nothing"
        );
        assert_eq!(agent["disabled"], false);
    }
    // The English default: the rail's own words, so the agent and the app a
    // person clicks are recognisably the same thing.
    assert_eq!(find(&body, "crm").unwrap()["name"], "Sales");
    assert_eq!(find(&body, "hr").unwrap()["name"], "People");
    assert_eq!(find(&body, "alo").unwrap()["name"], "alo");

    // Asked again — including in another language — it is the same set. The
    // seed ran once; nothing retranslates a tenant's own agents.
    let (status, again) = get(&h.app, &h.token, "/chat/agents?lang=fr").await;
    assert_eq!(status, StatusCode::OK, "{again}");
    assert_eq!(handles(&again), first);
    assert_eq!(find(&again, "crm").unwrap()["name"], "Sales");
    let ids: Vec<&str> = body["agents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["id"].as_str().unwrap())
        .collect();
    let same: Vec<&str> = again["agents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, same, "the second read made a second set");
}

/// The language the first reader asks in is the one the tenant is written in.
#[tokio::test]
async fn the_first_reader_chooses_the_language_the_agents_are_named_in() {
    let h = harness("agent-seed-nl").await;

    let (status, body) = get(&h.app, &h.token, "/chat/agents?lang=nl-BE").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(find(&body, "hr").unwrap()["name"], "Mensen");
    assert_eq!(find(&body, "crm").unwrap()["name"], "Verkoop");
    assert_eq!(find(&body, "finance").unwrap()["name"], "Financiën");
    // The handles never translate: they are what people type.
    let mut spelled = handles(&body);
    spelled.sort();
    assert!(spelled.contains(&"sites".to_owned()));
    assert!(spelled.contains(&"alo".to_owned()));

    // A language we do not have falls back rather than refusing.
    let other = harness("agent-seed-de").await;
    let (status, body) = get(&other.app, &other.token, "/chat/agents?lang=de").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(find(&body, "hr").unwrap()["name"], "People");
}

/// The item's second half on the wire: a module switched off for one person
/// yields no agent in any client-visible surface, and naming it costs no turn.
#[tokio::test]
async fn a_denied_module_has_no_agent_on_the_wire_and_takes_no_turn() {
    let h = harness("agent-seed-denied").await;
    let (base, seen) = scripted_model(vec![says("I should never be asked.")]).await;
    use_model(&h, &base).await;

    // Anna and Ben share a room, and @inventory is a member of it.
    let (status, body) = get(&h.app, &h.token, "/chat/agents").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let inventory = find(&body, "inventory").unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let ben = colleague(&h, "ben").await;
    let (status, room) = post(
        &h.app,
        &h.token,
        "/chat/channels",
        json!({"kind": "channel", "name": "stock", "visibility": "public"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{room}");
    let room = room["id"].as_str().unwrap().to_owned();
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/chat/channels/{room}/agents"),
        json!({ "agent": inventory }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = post(
        &h.app,
        &ben,
        &format!("/chat/channels/{room}/join"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // The admin switches Inventory off for Anna. (She is the tenant's only
    // non-admin here; the console's own route needs an admin, and this test is
    // about what she can reach afterwards, so the switch is thrown through the
    // store the console writes through.)
    let admin = h.ts.create_user("console@agentseed.test").await.unwrap();
    h.ts.set_admin(&admin, true).await.unwrap();
    h.ts.set_module_access(&h.user, AppModule::Inventory, false, &admin)
        .await
        .unwrap();

    // 1. Gone from her list — and only that one.
    let (status, body) = get(&h.app, &h.token, "/chat/agents").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let hers = handles(&body);
    assert!(
        !hers.contains(&"inventory".to_owned()),
        "a denied module still lists an agent: {hers:?}"
    );
    assert_eq!(hers.len(), ALL_AGENT_PRODUCTS.len() - 1);

    // 2. Gone from the room she shares with Ben — but not from Ben's view of
    //    the same room.
    let (status, body) = get(&h.app, &h.token, &format!("/chat/channels/{room}/agents")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(find(&body, "inventory").is_none(), "{body}");
    let (status, bens) = get(&h.app, &ben, &format!("/chat/channels/{room}/agents")).await;
    assert_eq!(status, StatusCode::OK, "{bens}");
    assert!(find(&bens, "inventory").is_some(), "{bens}");

    // 3. Opening a one-to-one with it is 404 — the same answer an id that was
    //    never issued gets.
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/chat/agents/{inventory}/dm"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");

    // 4. Defining another Inventory agent is refused rather than made and
    //    hidden, and the refusal says why in the store's own words.
    let (status, body) = post(
        &h.app,
        &h.token,
        "/chat/agents",
        json!({"handle": "stockroom", "name": "Stockroom", "product": "inventory"}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        body["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("cannot open"),
        "{body}"
    );

    // 5. Naming it in the shared room answers nobody — and costs no model
    //    call, which is what proves the refusal came before the turn.
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/chat/channels/{room}/messages"),
        json!({"body": "@inventory is the X100 in stock?"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let (status, feed) = get(&h.app, &h.token, &format!("/chat/channels/{room}/messages")).await;
    assert_eq!(status, StatusCode::OK, "{feed}");
    let spoke = feed["messages"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["authorKind"] == "agent");
    assert!(!spoke, "a denied agent answered anyway: {feed}");
    assert!(
        seen.lock().unwrap().is_empty(),
        "a denied agent cost a model call: {:?}",
        seen.lock().unwrap()
    );

    // 6. Ben, who still has Inventory, is answered in the very same room by the
    //    very same agent — the gate is per person, not per room.
    let (status, body) = post(
        &h.app,
        &ben,
        &format!("/chat/channels/{room}/messages"),
        json!({"body": "@inventory is the X100 in stock?"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let deadline = Instant::now() + Duration::from_secs(20);
    let answered = loop {
        let (status, feed) = get(&h.app, &ben, &format!("/chat/channels/{room}/messages")).await;
        assert_eq!(status, StatusCode::OK, "{feed}");
        if let Some(m) = feed["messages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["authorKind"] == "agent")
        {
            break Some(m.clone());
        }
        if Instant::now() >= deadline {
            break None;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    let answered = answered.expect("the colleague who still has Inventory was not answered");
    assert_eq!(answered["author"], inventory.as_str());
    assert_eq!(answered["body"], "I should never be asked.");
    assert_eq!(seen.lock().unwrap().len(), 1, "his turn was the only one");
}
