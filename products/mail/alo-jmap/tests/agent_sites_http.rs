//! **The Website agent, end to end** (A2.1) — the four sentences ADR 0034 and
//! ADR 0047 leave a website agent to prove, each asked the way a person asks it
//! and answered the way a person reads it:
//!
//! - `@sites what are our opening hours on the website?` — answered from the
//!   **published** site, with the draft saying something else on purpose, and
//!   **no button in between**;
//! - a page the agent drafts waits for a tap, lands in the **draft**, and is not
//!   on the internet when it gets there;
//! - **publishing is proposed, never silent**: the site goes live on the owner's
//!   own approval and on nothing else;
//! - an edit changes the **words** and leaves the wiring — a link's target —
//!   exactly as it was, and what a visitor reads does not change until a
//!   publish.
//!
//! Everything goes through the product's own path: the tenant's agents are the
//! ones `GET /chat/agents` seeds (A1.5), the room is made over HTTP, the agent
//! joins it over HTTP, and the question is an ordinary chat message. The site is
//! real rows written through the same store functions the `/sites` screens use.
//!
//! **No live model is ever called**, here or anywhere in this workspace's tests
//! (the loop's standing rail): the tenant's AI backend is the scripted local
//! socket in `common::model`, which hands back fixture completions in order and
//! records what it was asked. That recording is what makes "answered from the
//! published site" checkable at all — the assertions below are about the bytes
//! the model was shown, which is where a grounded answer and a plausible guess
//! differ.
//!
//! Run the transcripts with
//! `cargo nextest run -p alo-jmap --test agent_sites_http --no-capture`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use alo_store::{AgentProduct, SectionsEnvelope, SiteId, SitePageId, SiteStatus};
use common::model::{Seen, says, scripted_model, use_model, wants};
use common::{Harness, harness, harness_on, send};

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

/// The id of the tenant's own Website agent, out of the set a first look at
/// `GET /chat/agents` seeds (A1.5). Nothing here registers a handle: an agent
/// this test could not find is an agent a person could not find either.
async fn the_website_agent(h: &Harness) -> String {
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

/// What the model was actually shown on call `n` — the user turn of the request
/// body the scripted backend recorded, which is where the grounding and the tool
/// results live.
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
    println!("\n===== A2.1 TRANSCRIPT: {title} =====");
    for line in lines {
        println!("{line}");
    }
    println!("===== end: {title} =====\n");
}

// ---- the website under test --------------------------------------------------

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

/// The published home page's own words. The draft is edited to disagree with
/// them below, which is what makes "answered from the published site" a
/// checkable claim rather than a hopeful one.
const PUBLISHED_HOURS: &str = "We open at seven and close at four, Tuesday to Sunday.";
const DRAFT_HOURS: &str = "We open at nine and close at noon, weekdays only.";

fn home_sections(hours: &str) -> Value {
    json!({
        "schema_version": alo_store::SECTIONS_SCHEMA_VERSION,
        "sections": [
            {
                "type": "hero",
                "heading": "Juniper Bakery",
                "subheading": "Sourdough, every morning",
                "primary_cta": { "label": "Visit us", "href": "/visit" },
            },
            {
                "type": "features",
                "heading": "The bakery",
                "items": [
                    { "title": "Opening hours", "body": hours },
                    { "title": "Where we are", "body": "On the corner of Sint-Jansplein." },
                ],
            },
        ],
    })
}

/// A bakery with one published page. Returns the site and its home page.
async fn a_bakery(h: &Harness) -> (SiteId, SitePageId) {
    let site = h
        .acc
        .create_site("Juniper Bakery", &address(h, "juniper"))
        .await
        .unwrap();
    let home = h
        .acc
        .create_site_page(&site, "Home", "", true)
        .await
        .unwrap();
    h.acc
        .set_page_sections(&site, &home, home_sections(PUBLISHED_HOURS))
        .await
        .unwrap();
    h.acc
        .set_page_seo(
            &site,
            &home,
            None,
            Some("A bakery on Sint-Jansplein baking sourdough every single morning."),
        )
        .await
        .unwrap();
    (site, home)
}

/// The same bakery, live, with the **draft** since changed to say something
/// else — and a second page that was never published at all.
async fn a_published_bakery(h: &Harness) -> (SiteId, SitePageId) {
    let (site, home) = a_bakery(h).await;
    h.acc.publish_site(&site).await.unwrap();

    // Everything after this point is draft-only, and none of it may reach an
    // answer: the copy a visitor loads is the copy of the publish above.
    h.acc
        .set_page_sections(&site, &home, home_sections(DRAFT_HOURS))
        .await
        .unwrap();
    let unpublished = h
        .acc
        .create_site_page(&site, "Careers", "careers", false)
        .await
        .unwrap();
    h.acc
        .set_page_sections(
            &site,
            &unpublished,
            json!({
                "schema_version": alo_store::SECTIONS_SCHEMA_VERSION,
                "sections": [{
                    "type": "hero",
                    "heading": "Work with us",
                    "subheading": "Bakers wanted, opening hours negotiable.",
                }],
            }),
        )
        .await
        .unwrap();
    (site, home)
}

/// The sections of a page as they stand in the draft.
async fn sections_of(h: &Harness, site: &SiteId, page: &SitePageId) -> SectionsEnvelope {
    let page = h.acc.site_page(site, page).await.unwrap().unwrap();
    SectionsEnvelope::from_value(page.sections).unwrap()
}

// ---- the question, on the wire -----------------------------------------------

/// **The wave's question, end to end.** What lands in the room is the answer,
/// grounded in the pages a visitor can actually load — and the draft, which says
/// something else entirely, is nowhere in it.
#[tokio::test]
async fn the_website_agent_answers_from_the_published_site_with_no_button_in_between() {
    let h = harness("agent-a21-answer").await;
    let (site, _home) = a_published_bakery(&h).await;
    const ANSWER: &str =
        "Your site says you open at seven and close at four, Tuesday to Sunday [1].";
    let (base, seen) = scripted_model(vec![
        wants(
            "site_answer",
            json!({ "question": "opening hours" }),
            "Let me look at what the site says.",
        ),
        says(ANSWER),
    ])
    .await;
    use_model(&h, &base).await;
    let agent = the_website_agent(&h).await;
    let channel = a_room_with(&h, "the website", &agent).await;

    const QUESTION: &str = "@sites what are our opening hours on the website?";
    let spoken = ask_in_room(&h, &channel, QUESTION).await;

    // The answer, in the room, said once.
    assert_eq!(spoken["body"], json!(ANSWER));
    assert_eq!(spoken["authorKind"], json!("agent"));
    let room = messages(&h, &channel).await;
    assert_eq!(
        room.iter().filter(|m| m["authorKind"] == "agent").count(),
        1
    );
    // **No button in between** — not on the answer, and not on anything else in
    // the room. Asking what the site says is a lookup, not a change.
    for message in &room {
        assert_eq!(
            message["proposal"],
            Value::Null,
            "asking what the site says must never produce a proposal: {message}"
        );
    }

    // Two calls: the lookup, then the answer. The second is holding the
    // published page itself.
    assert_eq!(
        seen.lock().unwrap().len(),
        2,
        "a read costs one further call"
    );
    let first = shown(&seen, 0);
    let second = shown(&seen, 1);
    let system = offered(&seen, 0);
    assert!(
        system.contains("- site_answer:"),
        "the Website agent is offered its own reading tool: {system}"
    );
    assert!(
        !system.contains("- stock_answer:"),
        "and only its own product's tools (A1.2): {system}"
    );
    // Its grounding is empty on purpose (A1.3): Sites reaches its records
    // through the tool, so the first call carries the question and no snippets.
    assert!(
        first.contains("Sources:\n\n") || first.trim_end().ends_with("Sources:"),
        "the question is not answered from a search snippet: {first}"
    );
    assert!(
        second.contains("tool result \"site_answer\""),
        "the tool's own result must be among the sources: {second}"
    );
    assert!(second.contains("\"kind\":\"siteAnswer\""), "{second}");
    // **The published words, and not the draft's.** This is the assertion the
    // test exists for.
    assert!(
        second.contains("close at four"),
        "the answer is grounded in what a visitor can load: {second}"
    );
    assert!(
        !second.contains("close at noon"),
        "the draft must not reach an answer: {second}"
    );
    assert!(
        !second.contains("Bakers wanted"),
        "a page that was never published must not reach an answer: {second}"
    );
    assert!(second.contains("\"live\":true"), "{second}");
    assert!(
        second.contains("\"kind\":\"page\""),
        "the passage carries the citation a visitor could follow: {second}"
    );

    // Audited as a read — the agent's, the room's, and successful.
    let runs = h.acc.agent_tool_runs(50).await.unwrap();
    assert_eq!(runs.len(), 1, "{runs:?}");
    assert_eq!(runs[0].tool, "site_answer");
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
    let record = h.acc.agent_records().await.unwrap();
    let record = record.get(agent.as_str()).unwrap();
    assert_eq!(record.reads, 1);
    assert_eq!(record.answers, 1);
    assert_eq!(record.actions, 0);

    // Nothing was written, and nothing became public: a question about the site
    // leaves the site exactly as it was.
    assert_eq!(
        h.acc.site(&site).await.unwrap().unwrap().status,
        SiteStatus::Live
    );
    assert_eq!(h.acc.site_pages(&site).await.unwrap().len(), 2);

    transcript(
        QUESTION,
        &[
            format!("POST /chat/channels/{channel}/messages"),
            format!("     {}", json!({ "body": QUESTION })),
            "--- what the model was shown (call 1 of 2, user turn) ---".to_owned(),
            first,
            "--- what the model replied (call 1) ---".to_owned(),
            wants(
                "site_answer",
                json!({ "question": "opening hours" }),
                "Let me look at what the site says.",
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

// ---- the writes: drafted, approved, and still not public ----------------------

/// A page the agent drafts waits for a tap; when the tap comes it lands in the
/// **draft** and nowhere else. The site was live before and after, and what a
/// visitor can read did not change.
#[tokio::test]
async fn a_drafted_page_waits_for_a_tap_and_is_not_on_the_internet_when_it_lands() {
    let h = harness("agent-a21-draft").await;
    let (site, _home) = a_published_bakery(&h).await;
    let (base, _seen) = scripted_model(vec![wants(
        "site_page_draft",
        json!({
            "title": "Workshops",
            "heading": "Bread workshops",
            "intro": "Saturdays, in the bakery",
            "sections": [
                { "heading": "Sourdough for beginners", "body": "Three hours, everything provided." },
            ],
        }),
        "I'll draft a Workshops page for you to look at.",
    )])
    .await;
    use_model(&h, &base).await;
    let agent = the_website_agent(&h).await;
    let channel = a_room_with(&h, "the website", &agent).await;

    let spoken = ask_in_room(&h, &channel, "@sites draft a page about our workshops").await;

    // The room sees the sentence with the change hanging off it, pending.
    assert_eq!(spoken["proposal"]["tool"], json!("site_page_draft"));
    assert_eq!(spoken["proposal"]["state"], json!("pending"));
    assert_eq!(spoken["proposal"]["askedBy"], json!(h.user.as_str()));
    let proposal = spoken["proposal"]["id"].as_str().unwrap().to_owned();

    // Nothing happened while it waited. Not the page…
    assert_eq!(
        h.acc.site_pages(&site).await.unwrap().len(),
        2,
        "a write must not run before it is approved"
    );
    // …and not an audit row either.
    assert!(h.acc.agent_tool_runs(50).await.unwrap().is_empty());

    let decided = approve(&h, &proposal).await;
    assert_eq!(decided["state"], json!("approved"));
    assert_eq!(decided["result"]["result"]["kind"], json!("sitePageDraft"));
    // The word the whole item turns on, in the result the client renders.
    assert_eq!(decided["result"]["result"]["public"], json!(false));
    assert_eq!(decided["result"]["result"]["blocks"], json!(2));

    // The page is in the draft, with the words the model gave and an address
    // derived from its title.
    let pages = h.acc.site_pages(&site).await.unwrap();
    assert_eq!(pages.len(), 3);
    let drafted = pages.iter().find(|page| page.title == "Workshops").unwrap();
    assert_eq!(drafted.slug, "workshops");
    assert!(!drafted.is_home, "an agent never claims the home page");
    let sections = sections_of(&h, &site, &drafted.id).await;
    assert_eq!(sections.sections.len(), 2);
    let hero = serde_json::to_value(&sections.sections[0]).unwrap();
    assert_eq!(hero["type"], json!("hero"));
    assert_eq!(hero["heading"], json!("Bread workshops"));
    assert_eq!(hero["subheading"], json!("Saturdays, in the bakery"));
    let grid = serde_json::to_value(&sections.sections[1]).unwrap();
    assert_eq!(grid["items"][0]["title"], json!("Sourdough for beginners"));

    // **And it is not on the internet.** The published corpus — what the public
    // service serves and what `site_answer` reads — does not contain it.
    let corpus = h.acc.site_grounding_corpus(&site).await.unwrap();
    assert_eq!(
        corpus.len(),
        1,
        "the publish still holds one page: {corpus:?}"
    );
    assert!(
        !corpus.iter().any(|doc| doc.text.contains("Three hours")),
        "a drafted page must not reach the internet without a publish: {corpus:?}"
    );

    // Audited as a write the asker approved.
    let runs = h.acc.agent_tool_runs(50).await.unwrap();
    assert_eq!(runs.len(), 1, "{runs:?}");
    assert_eq!(runs[0].tool, "site_page_draft");
    assert_eq!(runs[0].effect, "write");
    assert!(runs[0].ok);
    assert_eq!(
        h.acc.agent_records().await.unwrap()[agent.as_str()].reads,
        0
    );
}

/// **Publishing is proposed, never silent.** The site is not live while the
/// proposal waits, whatever the agent said it would do; the owner's own tap is
/// what puts it on the internet.
#[tokio::test]
async fn publishing_waits_for_the_owner_and_only_then_is_the_site_live() {
    let h = harness("agent-a21-publish").await;
    let (site, _home) = a_bakery(&h).await;
    let (base, _seen) = scripted_model(vec![wants(
        "site_publish",
        json!({}),
        "I'll put the site online — approve it and it goes live.",
    )])
    .await;
    use_model(&h, &base).await;
    let agent = the_website_agent(&h).await;
    let channel = a_room_with(&h, "the website", &agent).await;

    let spoken = ask_in_room(&h, &channel, "@sites put the site online").await;

    assert_eq!(spoken["proposal"]["tool"], json!("site_publish"));
    assert_eq!(spoken["proposal"]["state"], json!("pending"));
    let proposal = spoken["proposal"]["id"].as_str().unwrap().to_owned();

    // While it waits, the site is a draft and there is nothing on the internet
    // to read — which is exactly what `site_answer` would have found.
    assert_eq!(
        h.acc.site(&site).await.unwrap().unwrap().status,
        SiteStatus::Draft
    );
    assert!(
        h.acc.site_grounding_corpus(&site).await.unwrap().is_empty(),
        "nothing may be public before the tap"
    );
    assert!(h.acc.agent_tool_runs(50).await.unwrap().is_empty());

    let decided = approve(&h, &proposal).await;
    assert_eq!(decided["state"], json!("approved"));
    assert_eq!(decided["result"]["result"]["kind"], json!("sitePublish"));
    assert_eq!(decided["result"]["result"]["public"], json!(true));
    assert_eq!(decided["result"]["result"]["pages"], json!(1));

    assert_eq!(
        h.acc.site(&site).await.unwrap().unwrap().status,
        SiteStatus::Live
    );
    let corpus = h.acc.site_grounding_corpus(&site).await.unwrap();
    assert_eq!(corpus.len(), 1);
    assert!(corpus[0].text.contains("close at four"));

    let runs = h.acc.agent_tool_runs(50).await.unwrap();
    assert_eq!(runs.len(), 1, "{runs:?}");
    assert_eq!(runs[0].tool, "site_publish");
    assert_eq!(runs[0].effect, "write");
    assert!(runs[0].ok);
    assert_eq!(
        runs[0].agent.as_ref().map(alo_store::ChatAgentId::as_str),
        Some(agent.as_str())
    );
}

/// **An agent edits the words, never the wiring** — and what a visitor reads
/// does not change until somebody publishes.
#[tokio::test]
async fn an_approved_edit_rewrites_the_copy_and_leaves_the_link_alone() {
    let h = harness("agent-a21-edit").await;
    let (site, home) = a_bakery(&h).await;
    h.acc.publish_site(&site).await.unwrap();
    let (base, _seen) = scripted_model(vec![wants(
        "site_page_edit",
        json!({
            "page": "Home",
            "seo_description": "Sourdough baked every morning on Sint-Jansplein, open from seven.",
            "copy": [
                { "index": 0, "type": "hero", "pointer": "/heading", "text": "Juniper Bakery, since 1998" },
                { "index": 0, "type": "hero", "pointer": "/primary_cta/label", "text": "Come and see us" },
            ],
        }),
        "I'll reword the front page.",
    )])
    .await;
    use_model(&h, &base).await;
    let agent = the_website_agent(&h).await;
    let channel = a_room_with(&h, "the website", &agent).await;

    let spoken = ask_in_room(&h, &channel, "@sites reword the front page a little").await;
    let proposal = spoken["proposal"]["id"].as_str().unwrap().to_owned();
    assert_eq!(spoken["proposal"]["tool"], json!("site_page_edit"));

    // Untouched while it waits.
    let before = sections_of(&h, &site, &home).await;
    assert_eq!(
        serde_json::to_value(&before.sections[0]).unwrap()["heading"],
        json!("Juniper Bakery")
    );

    let decided = approve(&h, &proposal).await;
    assert_eq!(decided["result"]["result"]["kind"], json!("sitePageEdit"));
    assert_eq!(decided["result"]["result"]["rewritten"], json!(2));
    assert_eq!(decided["result"]["result"]["retitled"], json!(false));
    assert_eq!(decided["result"]["result"]["seoChanged"], json!(true));
    assert_eq!(decided["result"]["result"]["public"], json!(false));

    let after = sections_of(&h, &site, &home).await;
    let hero = serde_json::to_value(&after.sections[0]).unwrap();
    assert_eq!(hero["heading"], json!("Juniper Bakery, since 1998"));
    assert_eq!(hero["primary_cta"]["label"], json!("Come and see us"));
    // **The wiring came through untouched.** The one property the pointer rule
    // exists for: an agent may rewrite the words on a button and never where it
    // sends somebody.
    assert_eq!(hero["primary_cta"]["href"], json!("/visit"));
    // Everything it was not asked about is as it was.
    assert_eq!(hero["subheading"], json!("Sourdough, every morning"));
    let grid = serde_json::to_value(&after.sections[1]).unwrap();
    assert_eq!(grid["items"][0]["body"], json!(PUBLISHED_HOURS));

    let page = h.acc.site_page(&site, &home).await.unwrap().unwrap();
    assert_eq!(page.title, "Home", "an unstated title is not cleared");
    assert!(
        page.seo_description.unwrap().contains("open from seven"),
        "the description the edit stated is the one stored"
    );
    assert!(
        page.seo_title.is_none(),
        "writing one SEO field must not clear the other"
    );

    // And the internet still reads the old wording: an edit is not a publish.
    let corpus = h.acc.site_grounding_corpus(&site).await.unwrap();
    assert!(
        corpus[0].text.contains("Juniper Bakery") && !corpus[0].text.contains("since 1998"),
        "a visitor reads the publish, not the draft: {corpus:?}"
    );

    let runs = h.acc.agent_tool_runs(50).await.unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].tool, "site_page_edit");
    assert_eq!(runs[0].effect, "write");
    assert_eq!(
        runs[0].agent.as_ref().map(alo_store::ChatAgentId::as_str),
        Some(agent.as_str())
    );
}

// ---- the tenant --------------------------------------------------------------

/// **Law 1: a Website agent reaches no other tenant's website.**
///
/// The second workspace is deliberately a mirror — the same site name, asked in
/// the same words — so nothing here can pass because the other tenant happened
/// to hold something different. The refusal is what the model is shown, and the
/// bakery's own words are nowhere in the turn.
#[tokio::test]
async fn a_website_agent_reaches_no_other_tenants_site() {
    let h = harness("agent-a21-isoa").await;
    let other = harness_on(Arc::clone(&h.store), "agent-a21-isob").await;
    let (theirs, _home) = a_published_bakery(&h).await;

    // The other tenant has a website of its own, and it is not this one.
    let mine = other
        .acc
        .create_site("Kestrel Studio", &address(&other, "kestrel"))
        .await
        .unwrap();
    let home = other
        .acc
        .create_site_page(&mine, "Home", "", true)
        .await
        .unwrap();
    other
        .acc
        .set_page_sections(
            &mine,
            &home,
            json!({
                "schema_version": alo_store::SECTIONS_SCHEMA_VERSION,
                "sections": [{ "type": "hero", "heading": "Kestrel Studio" }],
            }),
        )
        .await
        .unwrap();
    other.acc.publish_site(&mine).await.unwrap();

    // Nothing of the first tenant's site is readable through the second's door,
    // whatever id is used — the store's own answer, before any agent is asked.
    assert!(other.acc.site(&theirs).await.unwrap().is_none());
    assert!(other.acc.site_pages(&theirs).await.unwrap().is_empty());
    assert!(matches!(
        other.acc.site_grounding_corpus(&theirs).await,
        Err(alo_store::StoreError::NotFound)
    ));

    // And the agent, asked by name for the other tenant's site, is refused and
    // says so — the refusal reaches the model, the bakery's words do not.
    let (base, seen) = scripted_model(vec![
        wants(
            "site_answer",
            json!({ "question": "opening hours", "site": "Juniper Bakery" }),
            "Let me look at that site.",
        ),
        says("There's no website called Juniper Bakery in this workspace."),
    ])
    .await;
    use_model(&other, &base).await;
    let agent = the_website_agent(&other).await;
    let channel = a_room_with(&other, "the website", &agent).await;

    let spoken = ask_in_room(
        &other,
        &channel,
        "@sites what are the opening hours on Juniper Bakery?",
    )
    .await;
    assert_eq!(
        spoken["body"],
        json!("There's no website called Juniper Bakery in this workspace.")
    );

    let second = shown(&seen, 1);
    assert!(
        second.contains("no website of yours is called Juniper Bakery"),
        "the model is told it was refused, and why: {second}"
    );
    assert!(
        !second.contains("close at four") && !second.contains("Sint-Jansplein"),
        "not one word of another tenant's site may appear: {second}"
    );

    // Audited as an attempt that did not succeed — an audit that records only
    // what worked hides exactly the rows worth reading.
    let runs = other.acc.agent_tool_runs(50).await.unwrap();
    assert_eq!(runs.len(), 1, "{runs:?}");
    assert_eq!(runs[0].tool, "site_answer");
    assert!(!runs[0].ok);
    // The first tenant's own audit is untouched by any of it.
    assert!(h.acc.agent_tool_runs(50).await.unwrap().is_empty());
}
