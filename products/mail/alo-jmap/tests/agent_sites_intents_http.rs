//! The Website agent over its intents (AC.5, ADR 0058), on the wire: in a
//! real room, against the real router and store, with a scripted model.
//!
//! The published-answer half, the editing pair and the publish are proven end
//! to end in `agent_sites_http`; this suite holds what AC.5 adds — the site
//! as a *business* subject, and the module's move to the intent registry.
//! The page list is answered from the stored draft inside the turn, with no
//! button in between; drafting a page is only ever a previewed proposal, and
//! an approved draft is still not on the internet. And another tenant's site
//! does not exist for this agent: named outright, it earns the words a
//! workspace with no website earns, and not one of its page titles reaches
//! this tenant's model.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::{Duration as Wait, Instant};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use alo_store::{AgentProduct, SiteId};

use crate::common::model::{Seen, says, scripted_model, use_model, wants};
use crate::common::{Harness, harness, harness_on, send};

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

async fn sites_agent(h: &Harness) -> String {
    let handle = alo_store::default_handle(AgentProduct::Sites);
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

/// A public room by this name, with the given agent listening in it.
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

/// Says something in the room and waits for the agent's reply.
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
    let deadline = Instant::now() + Wait::from_secs(20);
    loop {
        let all = messages(h, channel).await;
        if let Some(message) = all
            .iter()
            .filter(|m| m["authorKind"] == "agent")
            .find(|_| all.len() > before + 1)
        {
            return message.clone();
        }
        assert!(Instant::now() < deadline, "the agent never spoke");
        tokio::time::sleep(Wait::from_millis(50)).await;
    }
}

/// The last message of the model's `n`th call — the numbered sources as the
/// model saw them, tool results included.
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

/// A subdomain nothing else on the shared test database can claim. Site
/// addresses are globally unique, and these suites share one Postgres.
fn address(h: &Harness, prefix: &str) -> String {
    let tail: String = h
        .tenant
        .as_str()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_lowercase();
    let tail: String = tail.chars().rev().take(12).collect();
    format!("{prefix}-{tail}")
}

/// A workshop site with a home page and one more page, drafted and never
/// published — the shape "what pages does the site have" is asked about.
async fn a_workshop(h: &Harness) -> SiteId {
    let site = h
        .acc
        .create_site("Elm Street Workshop", &address(h, "elm"))
        .await
        .unwrap();
    let home = h
        .acc
        .create_site_page(&site, "Home", "", true)
        .await
        .unwrap();
    h.acc
        .set_page_sections(
            &site,
            &home,
            json!({
                "schema_version": alo_store::SECTIONS_SCHEMA_VERSION,
                "sections": [{ "type": "hero", "heading": "Elm Street Workshop" }],
            }),
        )
        .await
        .unwrap();
    h.acc
        .create_site_page(&site, "Repairs", "repairs", false)
        .await
        .unwrap();
    site
}

/// The titles of a site's draft pages, straight off the store — what any
/// answer about the page list has to agree with.
async fn page_titles(h: &Harness, site: &SiteId) -> Vec<String> {
    h.acc
        .site_pages(site)
        .await
        .unwrap()
        .into_iter()
        .map(|page| page.title)
        .collect()
}

#[tokio::test]
async fn the_page_list_is_answered_from_the_stored_draft() {
    let h = harness("sites-intents-pages").await;
    a_workshop(&h).await;

    let agent = sites_agent(&h).await;
    let room = a_room_with(&h, "ask", &agent).await;
    let (model, seen) = scripted_model(vec![
        wants("site_pages", json!({}), "Let me look at the site."),
        says("The site has two pages: Home and Repairs [1]."),
    ])
    .await;
    use_model(&h, &model).await;

    let answer = ask_in_room(&h, &room, "@sites what pages does the site have?").await;
    assert_eq!(
        answer["body"],
        "The site has two pages: Home and Repairs [1]."
    );
    assert!(
        answer["proposal"].is_null(),
        "a read is answered, never proposed"
    );

    // The agent was offered its verbs — the seven kept and the four AC.5
    // reads, rendered from the intent registry.
    let prompt = offered(&seen, 0);
    for verb in [
        "site_answer",
        "site_pages",
        "site_status",
        "site_orders",
        "site_bookings",
        "site_page_read",
        "site_seo_review",
        "site_translation_status",
        "site_page_draft",
        "site_page_edit",
        "site_publish",
    ] {
        assert!(
            prompt.contains(&format!("- {verb}:")),
            "the prompt does not offer {verb}"
        );
    }
    // The stored draft came back as the route's own record: both titles, the
    // page count, and the home flag the page list renders.
    let sources = shown(&seen, 1);
    assert!(sources.contains("\"kind\":\"sitePages\""), "{sources}");
    assert!(sources.contains("\"total\":2"), "{sources}");
    assert!(sources.contains("Repairs"), "{sources}");
    assert!(sources.contains("\"home\":true"), "{sources}");
}

#[tokio::test]
async fn drafting_a_page_waits_for_the_askers_tap_and_is_still_not_public() {
    let h = harness("sites-intents-draft").await;
    let site = a_workshop(&h).await;

    let agent = sites_agent(&h).await;
    let room = a_room_with(&h, "ask", &agent).await;
    let (model, _seen) = scripted_model(vec![wants(
        "site_page_draft",
        json!({
            "title": "Our services",
            "heading": "What we repair",
            "sections": [{ "heading": "Bicycles", "body": "Wheels, brakes and gears." }],
        }),
        "I'll draft a services page.",
    )])
    .await;
    use_model(&h, &model).await;

    let answer = ask_in_room(&h, &room, "@sites draft a services page").await;
    assert!(
        !answer["proposal"].is_null(),
        "a write is a proposal: {answer}"
    );
    assert_eq!(answer["proposal"]["tool"], "site_page_draft");
    // Nothing ran without a tap: the draft still has its two pages.
    assert_eq!(page_titles(&h, &site).await, ["Home", "Repairs"]);

    // The asker approves — and the page is in the draft, off the store the
    // page list reads, while the site is still not on the internet at all.
    let proposal = answer["proposal"]["id"].as_str().unwrap().to_owned();
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/chat/proposals/{proposal}"),
        json!({ "approve": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        page_titles(&h, &site).await,
        ["Home", "Repairs", "Our services"]
    );
    let site = h.acc.site(&site).await.unwrap().unwrap();
    assert_ne!(
        site.status,
        alo_store::SiteStatus::Live,
        "an approved page draft must not publish anything"
    );
}

#[tokio::test]
async fn another_tenants_site_does_not_exist_here() {
    let h = harness("sites-intents-iso").await;
    // Another tenant on the same store, with a site whose page titles are
    // theirs alone.
    let other = harness_on(Arc::clone(&h.store), "sites-intents-iso2").await;
    let theirs = other
        .acc
        .create_site("Warroom", &address(&other, "warroom"))
        .await
        .unwrap();
    other
        .acc
        .create_site_page(&theirs, "The secret merger", "merger", true)
        .await
        .unwrap();

    let agent = sites_agent(&h).await;
    let room = a_room_with(&h, "ask", &agent).await;
    let (model, seen) = scripted_model(vec![
        wants(
            "site_pages",
            json!({ "site": "Warroom" }),
            "Let me look at the site.",
        ),
        says("There is no website in this workspace yet."),
    ])
    .await;
    use_model(&h, &model).await;

    let answer = ask_in_room(&h, &room, "@sites what pages does Warroom have?").await;
    assert_eq!(answer["body"], "There is no website in this workspace yet.");
    // The other tenant's site earns the words a siteless workspace earns —
    // indistinguishable on purpose — and not one of their page titles
    // reaches this tenant's model.
    let sources = shown(&seen, 1);
    assert!(
        sources.contains("there is no website in this workspace yet"),
        "{sources}"
    );
    assert!(
        !sources.contains("secret merger"),
        "another tenant's page titles leaked into the sources: {sources}"
    );
}
