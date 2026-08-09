//! The `/sites/*` edit surface (ADR 0036, S1.10), driven through the real
//! router over a real Postgres.
//!
//! `alo-store`'s own suites prove the storage; what this suite pins is the
//! **edge**: the auth guard, the status codes `docs/design/sites.md`
//! publishes (`404` for anything that doesn't resolve in the caller's
//! tenant, `422` with a rule-naming detail for every validation refusal,
//! subdomain-taken included), the index-addressed section operations, and —
//! mandatory — that another tenant's site is invisible and untouchable on
//! every verb.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::SiteId;
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use common::{Harness, harness, send};

// ---- request helpers ---------------------------------------------------------

fn with_json(method: &str, uri: &str, token: Option<&str>, body: Value) -> Request<Body> {
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    if let Some(token) = token {
        req = req.header("authorization", format!("Bearer {token}"));
    }
    req.body(Body::from(body.to_string())).unwrap()
}

async fn post(app: &Router, token: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    send(app, with_json("POST", uri, Some(token), body)).await
}

async fn put(app: &Router, token: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    send(app, with_json("PUT", uri, Some(token), body)).await
}

async fn get(app: &Router, token: &str, uri: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("GET")
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    send(app, req).await
}

async fn delete(app: &Router, token: &str, uri: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("DELETE")
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    send(app, req).await
}

/// A subdomain unique to this harness run: the tenant id is random per test,
/// so reruns against the shared database never collide in the global
/// subdomain namespace.
fn sub(tag: &str, h: &Harness) -> String {
    let salt: String = h
        .tenant
        .as_str()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .take(20)
        .collect();
    format!("{tag}{salt}")
}

fn created_id(kind: &str, (status, body): (StatusCode, Value)) -> String {
    assert_eq!(status, StatusCode::OK, "create {kind} failed: {body}");
    body["id"].as_str().expect("created id").to_owned()
}

fn hero() -> Value {
    json!({ "type": "hero", "heading": "Coffee roasted the morning it ships",
            "subheading": "Small-batch roastery on the harbour." })
}

fn cta() -> Value {
    json!({ "type": "cta", "heading": "Taste the difference",
            "button": { "label": "Order now", "href": "/order" } })
}

fn faq() -> Value {
    json!({ "type": "faq", "items": [
        { "question": "Do you ship abroad?", "answer": "Across the EU, yes." },
    ] })
}

// ---- auth guard --------------------------------------------------------------

#[tokio::test]
async fn every_route_family_requires_a_bearer_token() {
    let h = harness("sites-401").await;
    let attempts = [
        ("GET", "/sites".to_owned(), None),
        (
            "POST",
            "/sites".to_owned(),
            Some(json!({ "name": "X", "subdomain": "no-token" })),
        ),
        (
            "GET",
            "/sites/subdomain-check?subdomain=whatever".to_owned(),
            None,
        ),
        ("GET", "/sites/theme-presets".to_owned(), None),
        ("GET", "/sites/config".to_owned(), None),
        ("GET", "/sites/some-id/submissions".to_owned(), None),
        ("GET", "/sites/some-id/submissions.csv".to_owned(), None),
        (
            "PUT",
            "/sites/some-id/forms/form/submissions/submission".to_owned(),
            Some(json!({ "handled": true })),
        ),
        ("PUT", "/sites/some-id/theme".to_owned(), Some(json!({}))),
        ("POST", "/sites/some-id/publish".to_owned(), Some(json!({}))),
        (
            "POST",
            "/sites/some-id/pages".to_owned(),
            Some(json!({ "title": "T", "slug": "t" })),
        ),
        (
            "PUT",
            "/sites/some-id/pages/p/sections/0".to_owned(),
            Some(json!({ "section": {} })),
        ),
        ("GET", "/sites/some-id/pages/p/preview".to_owned(), None),
    ];
    for (method, uri, body) in attempts {
        let req = match body {
            Some(b) => with_json(method, &uri, None, b),
            None => Request::builder()
                .method(method)
                .uri(&uri)
                .body(Body::empty())
                .unwrap(),
        };
        let (status, _) = send(&h.app, req).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {uri}");
    }
}

// ---- form submissions inbox ------------------------------------------------

#[tokio::test]
async fn submissions_list_is_site_scoped_and_handled_is_one_write() {
    let h = harness("sites-submissions").await;
    let site = created_id(
        "site",
        post(
            &h.app,
            &h.token,
            "/sites",
            json!({ "name": "Inbox site", "subdomain": sub("inbox", &h) }),
        )
        .await,
    );
    let site_id = SiteId::new(&site);
    let form = h
        .acc
        .create_site_form(&site_id, "Contact us")
        .await
        .unwrap();
    let older = h
        .acc
        .add_site_form_submission(&site_id, &form, "Ada", "ada@example.test", "First")
        .await
        .unwrap();
    let newer = h
        .acc
        .add_site_form_submission(&site_id, &form, "Grace", "grace@example.test", "Second")
        .await
        .unwrap();

    let (status, body) = get(&h.app, &h.token, &format!("/sites/{site}/submissions")).await;
    assert_eq!(status, StatusCode::OK);
    let rows = body["submissions"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["id"], json!(newer.as_str()));
    assert_eq!(rows[0]["formName"], json!("Contact us"));
    assert_eq!(rows[0]["senderEmail"], json!("grace@example.test"));
    assert_eq!(rows[0]["handled"], json!(false));

    let (status, body) = put(
        &h.app,
        &h.token,
        &format!("/sites/{site}/forms/{form}/submissions/{older}"),
        json!({ "handled": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (_, body) = get(&h.app, &h.token, &format!("/sites/{site}/submissions")).await;
    let handled = body["submissions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == older.as_str())
        .unwrap();
    assert_eq!(handled["handled"], json!(true));
}

#[tokio::test]
async fn submissions_export_is_safe_complete_and_downloadable() {
    let h = harness("sites-submissions-csv").await;
    let subdomain = sub("export", &h);
    let site = created_id(
        "site",
        post(
            &h.app,
            &h.token,
            "/sites",
            json!({ "name": "Export site", "subdomain": subdomain }),
        )
        .await,
    );
    let site_id = SiteId::new(&site);
    let form = h
        .acc
        .create_site_form(&site_id, "=Risky, form")
        .await
        .unwrap();
    h.acc
        .add_site_form_submission(
            &site_id,
            &form,
            "+Visitor",
            "visitor@example.test",
            "Hello,\nplease call me",
        )
        .await
        .unwrap();

    let (status, headers, csv) =
        get_text(&h.app, &h.token, &format!("/sites/{site}/submissions.csv")).await;
    assert_eq!(status, StatusCode::OK, "{csv}");
    assert_eq!(
        headers
            .get("content-type")
            .and_then(|value| value.to_str().ok()),
        Some("text/csv; charset=utf-8")
    );
    let expected_disposition = format!("attachment; filename=\"submissions-{subdomain}.csv\"");
    assert_eq!(
        headers
            .get("content-disposition")
            .and_then(|value| value.to_str().ok()),
        Some(expected_disposition.as_str()),
    );
    assert_eq!(
        headers
            .get("cache-control")
            .and_then(|value| value.to_str().ok()),
        Some("no-store")
    );
    assert_eq!(
        headers
            .get("x-content-type-options")
            .and_then(|value| value.to_str().ok()),
        Some("nosniff")
    );
    assert!(csv.starts_with("receivedAt,form,senderName,senderEmail,message,status\r\n"));
    assert!(csv.contains("'=Risky, form"));
    assert!(csv.contains("'+Visitor"));
    assert!(csv.contains("\"Hello,\nplease call me\""));
    assert!(csv.ends_with(",needs reply\r\n"));
}

// ---- the site arc ------------------------------------------------------------

#[tokio::test]
async fn site_lifecycle_create_check_edit_delete() {
    let h = harness("sites-arc").await;
    let claimed = sub("arc", &h);

    // Fresh namespace: the label is free, then claimed.
    let (status, body) = get(
        &h.app,
        &h.token,
        &format!("/sites/subdomain-check?subdomain={claimed}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["available"], json!(true));

    let site = created_id(
        "site",
        post(
            &h.app,
            &h.token,
            "/sites",
            json!({ "name": "Harbour Roastery", "subdomain": claimed }),
        )
        .await,
    );

    let (status, body) = get(
        &h.app,
        &h.token,
        &format!("/sites/subdomain-check?subdomain={claimed}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["available"],
        json!(false),
        "claimed label still reads free"
    );

    // The list and the single read agree; a new site is a draft with no publish.
    let (status, body) = get(&h.app, &h.token, "/sites").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body["sites"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["id"] == json!(site))
    );
    let (status, body) = get(&h.app, &h.token, &format!("/sites/{site}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], json!("Harbour Roastery"));
    assert_eq!(body["status"], json!("draft"));
    assert_eq!(body["publish"], Value::Null);

    // Rename and move to a new subdomain in one PUT.
    let moved = sub("arcm", &h);
    let (status, _) = put(
        &h.app,
        &h.token,
        &format!("/sites/{site}"),
        json!({ "name": "Harbour Roastery bv", "subdomain": moved }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, body) = get(&h.app, &h.token, &format!("/sites/{site}")).await;
    assert_eq!(body["name"], json!("Harbour Roastery bv"));
    assert_eq!(body["subdomain"], json!(moved));

    // An empty PUT is a refusal, not a silent no-op.
    let (status, _) = put(&h.app, &h.token, &format!("/sites/{site}"), json!({})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    // Delete releases the claim.
    let (status, _) = delete(&h.app, &h.token, &format!("/sites/{site}")).await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = get(&h.app, &h.token, &format!("/sites/{site}")).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "deleted site still resolves: {body}"
    );
    let (_, body) = get(
        &h.app,
        &h.token,
        &format!("/sites/subdomain-check?subdomain={moved}"),
    )
    .await;
    assert_eq!(
        body["available"],
        json!(true),
        "delete did not release the subdomain"
    );
}

// ---- pages -------------------------------------------------------------------

#[tokio::test]
async fn page_lifecycle_slug_seo_home_and_order() {
    let h = harness("sites-pages").await;
    let site = created_id(
        "site",
        post(
            &h.app,
            &h.token,
            "/sites",
            json!({ "name": "Pages", "subdomain": sub("pg", &h) }),
        )
        .await,
    );
    let base = format!("/sites/{site}/pages");

    // Home page at the empty slug; a second page with a real slug.
    let home = created_id(
        "home page",
        post(
            &h.app,
            &h.token,
            &base,
            json!({ "title": "Home", "home": true }),
        )
        .await,
    );
    let about = created_id(
        "about page",
        post(
            &h.app,
            &h.token,
            &base,
            json!({ "title": "About", "slug": "about" }),
        )
        .await,
    );

    let (status, body) = get(&h.app, &h.token, &base).await;
    assert_eq!(status, StatusCode::OK);
    let pages = body["pages"].as_array().unwrap();
    assert_eq!(pages.len(), 2);
    assert_eq!(
        pages[0]["id"],
        json!(home),
        "home was created first, leads the nav"
    );
    assert_eq!(pages[0]["home"], json!(true));
    assert!(pages[0].get("sections").is_none(), "the list stays lean");

    // Title, slug, and SEO in one PUT; then a partial SEO update must keep
    // the other field, and a blank must clear it.
    let (status, _) = put(
        &h.app,
        &h.token,
        &format!("{base}/{about}"),
        json!({ "title": "Our story", "slug": "story",
                "seoTitle": "Our story — Pages", "seoDescription": "How it began." }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = put(
        &h.app,
        &h.token,
        &format!("{base}/{about}"),
        json!({ "seoTitle": "The story" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, body) = get(&h.app, &h.token, &format!("{base}/{about}")).await;
    assert_eq!(body["slug"], json!("story"));
    assert_eq!(body["seoTitle"], json!("The story"));
    assert_eq!(
        body["seoDescription"],
        json!("How it began."),
        "partial SEO update dropped the description"
    );
    let (status, _) = put(
        &h.app,
        &h.token,
        &format!("{base}/{about}"),
        json!({ "seoDescription": "" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, body) = get(&h.app, &h.token, &format!("{base}/{about}")).await;
    assert_eq!(
        body["seoDescription"],
        Value::Null,
        "blank did not clear the override"
    );

    // Reorder is the full permutation; the list follows it.
    let (status, _) = put(
        &h.app,
        &h.token,
        &format!("{base}/order"),
        json!({ "order": [about, home] }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, body) = get(&h.app, &h.token, &base).await;
    assert_eq!(body["pages"][0]["id"], json!(about));

    // Home moves: give the old home a slug first, then promote the other.
    let (status, _) = put(
        &h.app,
        &h.token,
        &format!("{base}/{home}"),
        json!({ "slug": "welcome" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = post(&h.app, &h.token, &format!("{base}/{about}/home"), json!({})).await;
    assert_eq!(status, StatusCode::OK, "promote failed: {body}");
    let (_, body) = get(&h.app, &h.token, &format!("{base}/{about}")).await;
    assert_eq!(body["home"], json!(true));

    let (status, _) = delete(&h.app, &h.token, &format!("{base}/{home}")).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = get(&h.app, &h.token, &format!("{base}/{home}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ---- sections ----------------------------------------------------------------

#[tokio::test]
async fn section_ops_add_update_move_remove_and_full_set() {
    let h = harness("sites-sections").await;
    let site = created_id(
        "site",
        post(
            &h.app,
            &h.token,
            "/sites",
            json!({ "name": "Sections", "subdomain": sub("sc", &h) }),
        )
        .await,
    );
    let page = created_id(
        "page",
        post(
            &h.app,
            &h.token,
            &format!("/sites/{site}/pages"),
            json!({ "title": "Home", "home": true }),
        )
        .await,
    );
    let sections = format!("/sites/{site}/pages/{page}/sections");

    // Append hero, append faq, insert cta between them.
    let (status, body) = post(&h.app, &h.token, &sections, json!({ "section": hero() })).await;
    assert_eq!(status, StatusCode::OK, "add hero: {body}");
    let (status, _) = post(&h.app, &h.token, &sections, json!({ "section": faq() })).await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = post(
        &h.app,
        &h.token,
        &sections,
        json!({ "section": cta(), "index": 1 }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let kinds: Vec<&str> = body["sections"]["sections"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["type"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["hero", "cta", "faq"]);

    // Update in place; the response is the canonical stored envelope.
    let (status, body) = put(
        &h.app,
        &h.token,
        &format!("{sections}/1"),
        json!({ "section": { "type": "cta", "heading": "New heading",
                             "button": { "label": "Go", "href": "/go" } } }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["sections"]["sections"][1]["heading"],
        json!("New heading")
    );

    // Move the faq to the front; then remove the hero (now index 1).
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("{sections}/2/move"),
        json!({ "to": 0 }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["sections"]["sections"][0]["type"], json!("faq"));
    let (status, body) = delete(&h.app, &h.token, &format!("{sections}/1")).await;
    assert_eq!(status, StatusCode::OK);
    let kinds: Vec<&str> = body["sections"]["sections"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["type"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["faq", "cta"]);

    // The page read returns what the ops built.
    let (_, body) = get(&h.app, &h.token, &format!("/sites/{site}/pages/{page}")).await;
    assert_eq!(body["sections"]["sections"].as_array().unwrap().len(), 2);

    // The atomic full set replaces the stack wholesale.
    let (status, body) = put(
        &h.app,
        &h.token,
        &sections,
        json!({ "schema_version": 1, "sections": [hero()] }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["sections"]["sections"].as_array().unwrap().len(), 1);

    // Out-of-range and malformed indexes are named refusals.
    let (status, _) = delete(&h.app, &h.token, &format!("{sections}/7")).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, _) = post(
        &h.app,
        &h.token,
        &format!("{sections}/one/move"),
        json!({ "to": 0 }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn adding_a_contact_section_creates_and_links_its_form() {
    let owner = harness("sites-contact-section").await;
    let outsider = harness("sites-contact-outsider").await;
    let site = created_id(
        "site",
        post(
            &owner.app,
            &owner.token,
            "/sites",
            json!({ "name": "Contact Co", "subdomain": sub("cf", &owner) }),
        )
        .await,
    );
    let page = created_id(
        "page",
        post(
            &owner.app,
            &owner.token,
            &format!("/sites/{site}/pages"),
            json!({ "title": "Home", "home": true }),
        )
        .await,
    );
    let sections = format!("/sites/{site}/pages/{page}/sections");

    let (status, body) = post(
        &owner.app,
        &owner.token,
        &sections,
        json!({ "section": {
            "type": "contact_form",
            "heading": "Talk to our team",
            "body": "We reply within one working day."
        }}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "add contact section: {body}");
    let linked = body["sections"]["sections"][0]["form_id"]
        .as_str()
        .expect("server-linked form id")
        .to_owned();
    let stored = owner.acc.site_forms(&SiteId::new(&site)).await.unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].id.as_str(), linked);
    assert_eq!(stored[0].name, "Talk to our team");

    // Section headings have a wider content cap than owner-facing form
    // names. A valid long Unicode heading is shortened by characters and
    // still produces a working linked form.
    let long_heading = "é".repeat(120);
    let (status, body) = post(
        &owner.app,
        &owner.token,
        &sections,
        json!({ "section": {
            "type": "contact_form",
            "heading": long_heading
        }}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "add long-heading form: {body}");
    let stored = owner.acc.site_forms(&SiteId::new(&site)).await.unwrap();
    assert_eq!(stored.len(), 2);
    assert_eq!(stored[1].name.chars().count(), 100);
    assert!(stored[1].name.chars().all(|character| character == 'é'));

    // A form id is never a cross-tenant capability: even if an outsider's
    // valid id is supplied, the owner's page refuses it and keeps its stack.
    let foreign_site = outsider
        .acc
        .create_site("Outside", &sub("cfo", &outsider))
        .await
        .unwrap();
    let foreign_form = outsider
        .acc
        .create_site_form(&foreign_site, "Outside form")
        .await
        .unwrap();
    let (status, _) = post(
        &owner.app,
        &owner.token,
        &sections,
        json!({ "section": {
            "type": "contact_form",
            "form_id": foreign_form.as_str()
        }}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (_, body) = get(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/pages/{page}"),
    )
    .await;
    assert_eq!(body["sections"]["sections"].as_array().unwrap().len(), 2);
    assert_eq!(body["sections"]["sections"][0]["form_id"], json!(linked));
}

// ---- publish + theme ---------------------------------------------------------

#[tokio::test]
async fn theme_gate_and_publish_flow() {
    let h = harness("sites-publish").await;
    let site = created_id(
        "site",
        post(
            &h.app,
            &h.token,
            "/sites",
            json!({ "name": "Live", "subdomain": sub("pub", &h) }),
        )
        .await,
    );

    // Publish preconditions: no pages, then no home page → named 422s.
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/sites/{site}/publish"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        body["detail"].as_str().unwrap().contains("no pages"),
        "{body}"
    );
    created_id(
        "page",
        post(
            &h.app,
            &h.token,
            &format!("/sites/{site}/pages"),
            json!({ "title": "Loose", "slug": "loose" }),
        )
        .await,
    );
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/sites/{site}/publish"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body["detail"].as_str().unwrap().contains("home"), "{body}");

    // Theme: an off-schema envelope is refused, a preset lands.
    let (status, _) = put(
        &h.app,
        &h.token,
        &format!("/sites/{site}/theme"),
        json!({ "schema_version": 1, "preset": "no-such-preset" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, body) = put(
        &h.app,
        &h.token,
        &format!("/sites/{site}/theme"),
        json!({ "schema_version": 1, "preset": "terra" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "theme set: {body}");
    let (_, body) = get(&h.app, &h.token, &format!("/sites/{site}")).await;
    assert_eq!(body["theme"]["preset"], json!("terra"));

    // With a home page the publish goes through and the site reads live.
    created_id(
        "home",
        post(
            &h.app,
            &h.token,
            &format!("/sites/{site}/pages"),
            json!({ "title": "Home", "home": true }),
        )
        .await,
    );
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/sites/{site}/publish"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "publish: {body}");
    let publish_id = body["publishId"].as_str().unwrap().to_owned();
    assert_eq!(body["status"], json!("live"));
    let (_, body) = get(&h.app, &h.token, &format!("/sites/{site}")).await;
    assert_eq!(body["status"], json!("live"));
    assert_eq!(body["publish"]["id"], json!(publish_id));

    // Unpublish flips back to draft; idempotent.
    for _ in 0..2 {
        let (status, body) = post(
            &h.app,
            &h.token,
            &format!("/sites/{site}/unpublish"),
            json!({}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], json!("draft"));
    }
    let (_, body) = get(&h.app, &h.token, &format!("/sites/{site}")).await;
    assert_eq!(body["status"], json!("draft"));
    assert_eq!(body["publish"], Value::Null);
}

// ---- validation --------------------------------------------------------------

#[tokio::test]
async fn rule_violations_answer_422_with_the_rule() {
    let h = harness("sites-422").await;

    // Subdomain rules on create and on the live check.
    for bad in ["ab", "-edge-", "UPPER", "www"] {
        let (status, body) = post(
            &h.app,
            &h.token,
            "/sites",
            json!({ "name": "X", "subdomain": bad }),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "create with {bad:?}: {body}"
        );
        assert!(
            body["detail"].is_string(),
            "no rule named for {bad:?}: {body}"
        );
    }
    let (status, _) = get(&h.app, &h.token, "/sites/subdomain-check?subdomain=www").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    // A taken subdomain answers taken/free only.
    let claimed = sub("tk", &h);
    created_id(
        "site",
        post(
            &h.app,
            &h.token,
            "/sites",
            json!({ "name": "First", "subdomain": claimed }),
        )
        .await,
    );
    let (status, body) = post(
        &h.app,
        &h.token,
        "/sites",
        json!({ "name": "Second", "subdomain": claimed }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["detail"], json!("subdomain is already taken"));

    // Page slug rules: reserved public path, then a duplicate.
    let site = created_id(
        "site",
        post(
            &h.app,
            &h.token,
            "/sites",
            json!({ "name": "Rules", "subdomain": sub("rl", &h) }),
        )
        .await,
    );
    let pages = format!("/sites/{site}/pages");
    let (status, _) = post(
        &h.app,
        &h.token,
        &pages,
        json!({ "title": "Blog", "slug": "blog" }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "reserved slug accepted"
    );
    created_id(
        "page",
        post(
            &h.app,
            &h.token,
            &pages,
            json!({ "title": "A", "slug": "a" }),
        )
        .await,
    );
    let (status, _) = post(
        &h.app,
        &h.token,
        &pages,
        json!({ "title": "A2", "slug": "a" }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "duplicate slug accepted"
    );

    // Sections: an unknown type and a content-rule violation both name the
    // refusal; malformed JSON is the 400 family, not a 500.
    let page = created_id(
        "page",
        post(
            &h.app,
            &h.token,
            &pages,
            json!({ "title": "Home", "home": true }),
        )
        .await,
    );
    let sections = format!("/sites/{site}/pages/{page}/sections");
    let (status, _) = post(
        &h.app,
        &h.token,
        &sections,
        json!({ "section": { "type": "carousel", "heading": "nope" } }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "unknown section type accepted"
    );
    let (status, body) = post(
        &h.app,
        &h.token,
        &sections,
        json!({ "section": { "type": "hero", "heading": "x",
                             "primary_cta": { "label": "x", "href": "javascript:alert(1)" } } }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "unsafe href accepted: {body}"
    );
    let req = Request::builder()
        .method("PUT")
        .uri(&sections)
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", h.token))
        .body(Body::from("{not json"))
        .unwrap();
    let (status, _) = send(&h.app, req).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

// ---- the wrong-tenant test (mandatory: CLAUDE.md law 1) ----------------------

#[tokio::test]
async fn another_tenants_site_is_invisible_on_every_route() {
    let a = harness("sites-tenant-a").await;
    let b = harness("sites-tenant-b").await;

    // Tenant B's site and page, built through B's own door.
    let b_site = created_id(
        "site",
        post(
            &b.app,
            &b.token,
            "/sites",
            json!({ "name": "B Marketing", "subdomain": sub("iso", &b) }),
        )
        .await,
    );
    let b_page = created_id(
        "page",
        post(
            &b.app,
            &b.token,
            &format!("/sites/{b_site}/pages"),
            json!({ "title": "B Home", "home": true }),
        )
        .await,
    );
    let b_site_id = SiteId::new(&b_site);
    let b_form = b
        .acc
        .create_site_form(&b_site_id, "Private form")
        .await
        .unwrap();
    let b_submission = b
        .acc
        .add_site_form_submission(
            &b_site_id,
            &b_form,
            "Private sender",
            "private@example.test",
            "Tenant B only",
        )
        .await
        .unwrap();

    // A's list never mentions it.
    let (status, body) = get(&a.app, &a.token, "/sites").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        !body.to_string().contains("B Marketing"),
        "list leaked: {body}"
    );

    // Every verb on B's ids answers A with the same 404 an invented id gets.
    let attempts: Vec<(&str, String, Value)> = vec![
        ("GET", format!("/sites/{b_site}"), json!({})),
        (
            "PUT",
            format!("/sites/{b_site}"),
            json!({ "name": "Taken Over" }),
        ),
        ("DELETE", format!("/sites/{b_site}"), json!({})),
        (
            "PUT",
            format!("/sites/{b_site}/theme"),
            json!({ "schema_version": 1, "preset": "terra" }),
        ),
        ("POST", format!("/sites/{b_site}/publish"), json!({})),
        ("POST", format!("/sites/{b_site}/unpublish"), json!({})),
        ("GET", format!("/sites/{b_site}/pages"), json!({})),
        ("GET", format!("/sites/{b_site}/submissions"), json!({})),
        ("GET", format!("/sites/{b_site}/submissions.csv"), json!({})),
        (
            "PUT",
            format!("/sites/{b_site}/forms/{b_form}/submissions/{b_submission}"),
            json!({ "handled": true }),
        ),
        (
            "POST",
            format!("/sites/{b_site}/pages"),
            json!({ "title": "Injected", "slug": "injected" }),
        ),
        (
            "PUT",
            format!("/sites/{b_site}/pages/order"),
            json!({ "order": [b_page.clone()] }),
        ),
        ("GET", format!("/sites/{b_site}/pages/{b_page}"), json!({})),
        (
            "GET",
            format!("/sites/{b_site}/pages/{b_page}/preview"),
            json!({}),
        ),
        (
            "PUT",
            format!("/sites/{b_site}/pages/{b_page}"),
            json!({ "title": "Defaced" }),
        ),
        (
            "DELETE",
            format!("/sites/{b_site}/pages/{b_page}"),
            json!({}),
        ),
        (
            "POST",
            format!("/sites/{b_site}/pages/{b_page}/home"),
            json!({}),
        ),
        (
            "PUT",
            format!("/sites/{b_site}/pages/{b_page}/sections"),
            json!({ "schema_version": 1, "sections": [] }),
        ),
        (
            "POST",
            format!("/sites/{b_site}/pages/{b_page}/sections"),
            json!({ "section": hero() }),
        ),
        (
            "PUT",
            format!("/sites/{b_site}/pages/{b_page}/sections/0"),
            json!({ "section": hero() }),
        ),
        (
            "POST",
            format!("/sites/{b_site}/pages/{b_page}/sections/0/move"),
            json!({ "to": 0 }),
        ),
        (
            "DELETE",
            format!("/sites/{b_site}/pages/{b_page}/sections/0"),
            json!({}),
        ),
    ];
    for (method, uri, body) in attempts {
        let (status, answer) = send(&a.app, with_json(method, &uri, Some(&a.token), body)).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method} {uri} → {answer}");
        assert!(
            !answer.to_string().contains("B Marketing") && !answer.to_string().contains("B Home"),
            "{method} {uri} leaked the record it refused: {answer}"
        );
    }

    // B's site is untouched by the barrage.
    let (status, body) = get(&b.app, &b.token, &format!("/sites/{b_site}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], json!("B Marketing"));
    let (_, body) = get(&b.app, &b.token, &format!("/sites/{b_site}/pages/{b_page}")).await;
    assert_eq!(body["title"], json!("B Home"));
    let forms = b.acc.site_forms(&b_site_id).await.unwrap();
    assert_eq!(
        forms.len(),
        1,
        "a foreign add-section request created a form"
    );
    let submissions = b
        .acc
        .site_form_submissions(&b_site_id, &b_form)
        .await
        .unwrap();
    assert_eq!(submissions.len(), 1);
    assert!(!submissions[0].handled, "foreign tenant marked it handled");
}

// ---- the draft preview (S1.13) -----------------------------------------------

/// GETs a route that answers a raw document rather than JSON.
async fn get_text(
    app: &Router,
    token: &str,
    uri: &str,
) -> (StatusCode, axum::http::HeaderMap, String) {
    use tower::ServiceExt;
    let req = Request::builder()
        .method("GET")
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = axum::body::to_bytes(resp.into_body(), 8 * 1024 * 1024)
        .await
        .unwrap();
    (status, headers, String::from_utf8(bytes.to_vec()).unwrap())
}

/// The preview answers the DRAFT as one self-contained HTML document: the
/// same renderer as public serving with the stylesheet inlined, following
/// every edit immediately — no publish involved.
#[tokio::test]
async fn preview_renders_the_draft_as_a_self_contained_document() {
    let h = harness("sites-preview").await;
    let claimed = sub("pv", &h);
    let site = created_id(
        "site",
        post(
            &h.app,
            &h.token,
            "/sites",
            json!({ "name": "Preview Roastery", "subdomain": claimed }),
        )
        .await,
    );
    let page = created_id(
        "page",
        post(
            &h.app,
            &h.token,
            &format!("/sites/{site}/pages"),
            json!({ "title": "Home", "home": true }),
        )
        .await,
    );
    let (status, _) = post(
        &h.app,
        &h.token,
        &format!("/sites/{site}/pages/{page}/sections"),
        json!({ "section": hero() }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let uri = format!("/sites/{site}/pages/{page}/preview");
    let (status, headers, html) = get_text(&h.app, &h.token, &uri).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get("content-type").unwrap(),
        "text/html; charset=utf-8"
    );
    // A draft has no cache life.
    assert_eq!(headers.get("cache-control").unwrap(), "no-store");
    assert!(html.starts_with("<!doctype html>"));
    assert!(html.contains("Coffee roasted the morning it ships"));
    // Self-contained: the stylesheet is inlined; the public asset path (which
    // does not resolve on this origin) is referenced nowhere.
    assert!(html.contains("<style>"));
    assert!(!html.contains("/assets/site.css"));
    // Canonical/OG advertise the site's future public origin.
    assert!(html.contains(&format!("https://{claimed}.alosites.com/")));

    // The preview follows the draft: an edit shows on the next fetch.
    let mut edited = hero();
    edited["heading"] = json!("Now even fresher");
    let (status, _) = put(
        &h.app,
        &h.token,
        &format!("/sites/{site}/pages/{page}/sections/0"),
        json!({ "section": edited }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, _, html) = get_text(&h.app, &h.token, &uri).await;
    assert!(html.contains("Now even fresher"));
    assert!(!html.contains("Coffee roasted the morning it ships"));
}

// ---- themes (S1.14) ----------------------------------------------------------

/// The preset listing the theme picker renders: at least the six the queue
/// requires, the default first, every palette token a hex color, every
/// preset named — static product data behind the same auth as the rest of
/// the edit surface.
#[tokio::test]
async fn theme_presets_list_the_shipped_palettes() {
    let h = harness("sites-presets").await;
    let (status, body) = get(&h.app, &h.token, "/sites/theme-presets").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let presets = body["presets"].as_array().expect("presets array");
    assert!(presets.len() >= 6, "at least six shipped presets");
    assert_eq!(presets[0]["id"], json!("north"), "the default leads");
    for preset in presets {
        assert!(!preset["name"].as_str().unwrap().is_empty());
        for token in [
            "background",
            "surface",
            "text",
            "mutedText",
            "primary",
            "onPrimary",
            "border",
        ] {
            let hex = preset["palette"][token].as_str().unwrap();
            assert!(
                hex.len() == 7 && hex.starts_with('#'),
                "{}: palette.{token} = {hex}",
                preset["id"]
            );
        }
        assert!(
            !preset["typography"]["headingFamily"]
                .as_str()
                .unwrap()
                .is_empty()
        );
        assert!(preset["typography"]["headingWeight"].is_u64());
    }
}

/// The config the publish UI composes its "goes live at" copy and live links
/// from: the deployment-wide sites domain. The suite (like the preview test
/// above) runs against the product default, `alosites.com`.
#[tokio::test]
async fn config_names_the_sites_domain() {
    let h = harness("sites-config").await;
    let (status, body) = get(&h.app, &h.token, "/sites/config").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["domain"], json!("alosites.com"));
}

/// The preview inlines the theme logo and section images as `data:` URIs
/// (the public image path does not resolve on the edit origin), while a
/// referenced blob that is not an image falls back to the public path
/// instead of inlining non-image bytes.
#[tokio::test]
async fn preview_inlines_theme_and_section_images() {
    let h = harness("sites-imgprev").await;
    let logo = h
        .acc
        .put_blob(
            axum::body::Bytes::from_static(b"logo-bytes"),
            Some("image/png"),
        )
        .await
        .unwrap();
    let not_an_image = h
        .acc
        .put_blob(
            axum::body::Bytes::from_static(b"plain text"),
            Some("text/plain"),
        )
        .await
        .unwrap();

    let claimed = sub("imgpv", &h);
    let site = created_id(
        "site",
        post(
            &h.app,
            &h.token,
            "/sites",
            json!({ "name": "Image Preview Co", "subdomain": claimed }),
        )
        .await,
    );
    let (status, _) = put(
        &h.app,
        &h.token,
        &format!("/sites/{site}/theme"),
        json!({ "schema_version": 1, "preset": "north", "logo": logo.as_str() }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let page = created_id(
        "page",
        post(
            &h.app,
            &h.token,
            &format!("/sites/{site}/pages"),
            json!({ "title": "Home", "home": true }),
        )
        .await,
    );
    let (status, _) = put(
        &h.app,
        &h.token,
        &format!("/sites/{site}/pages/{page}/sections"),
        json!({ "schema_version": 1, "sections": [
            { "type": "nav", "links": [] },
            { "type": "gallery", "images": [
                { "blob_id": not_an_image.as_str(), "alt": "" }
            ]}
        ]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let uri = format!("/sites/{site}/pages/{page}/preview");
    let (status, _, html) = get_text(&h.app, &h.token, &uri).await;
    assert_eq!(status, StatusCode::OK);
    // The logo is inlined: its bytes as a data URI ("logo-bytes" base64).
    assert!(html.contains("data:image/png;base64,bG9nby1ieXRlcw=="));
    assert!(!html.contains(&format!("/assets/img/{}", logo.as_str())));
    // The non-image blob is not inlined — public-path fallback.
    assert!(html.contains(&format!("/assets/img/{}", not_an_image.as_str())));
}
