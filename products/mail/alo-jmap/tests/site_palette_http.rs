//! The section palette on the wire (ADR 0042 §4, S3.01d) — driven through the
//! real router over a real Postgres.
//!
//! `alo-store`'s own suite proves the seeding rules and pins them as goldens.
//! What this suite pins is the **edge**: that the palette is built from the
//! caller's own website and nobody else's, that a tile previews as HTML
//! rendered by the public renderer, that a tile with nothing to show says so
//! rather than answering an empty page — and the arc the item exists for:
//! reading the palette and dropping one of its tiles at a chosen position,
//! through the section op every other gesture uses.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use serde_json::{Value, json};
use tower::ServiceExt;

use common::{Harness, harness, harness_on, send};

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
        .method("GET")
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    send(app, req).await
}

/// A GET whose body is a document rather than JSON — the tile's picture.
async fn get_text(app: &Router, token: &str, uri: &str) -> (StatusCode, String, String) {
    let req = Request::builder()
        .method("GET")
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();
    let status = response.status();
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024 * 1024)
        .await
        .unwrap();
    (
        status,
        content_type,
        String::from_utf8(bytes.to_vec()).unwrap(),
    )
}

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

/// A site with one home page, named as the tenant named it.
async fn site_and_page(h: &Harness, name: &str, tag: &str) -> (String, String) {
    let site = created_id(
        "site",
        post(
            &h.app,
            &h.token,
            "/sites",
            json!({ "name": name, "subdomain": sub(tag, h) }),
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
    (site, page)
}

/// One tile out of a palette body.
fn tile<'a>(body: &'a Value, kind: &str) -> &'a Value {
    body["items"]
        .as_array()
        .expect("palette items")
        .iter()
        .find(|item| item["kind"] == json!(kind))
        .unwrap_or_else(|| panic!("no {kind} tile in the palette"))
}

#[tokio::test]
async fn the_palette_offers_every_type_seeded_from_the_callers_own_website() {
    let h = harness("sites-palette").await;
    let (site, page) = site_and_page(&h, "Nordwind Coffee Roasters", "pl").await;
    let palette = format!("/sites/{site}/pages/{page}/palette");

    // A website with nothing on it yet: what it always has is its name, its
    // pages and a way to be written to. Everything else asks rather than
    // invents.
    let (status, body) = get(&h.app, &h.token, &palette).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["items"].as_array().unwrap().len(), 17);
    assert_eq!(tile(&body, "hero")["ready"], json!(true));
    assert_eq!(
        tile(&body, "hero")["section"]["heading"],
        json!("Nordwind Coffee Roasters")
    );
    assert_eq!(tile(&body, "hero")["section"]["type"], json!("hero"));
    assert_eq!(
        tile(&body, "nav")["section"]["links"][0]["href"],
        json!("/")
    );
    assert_eq!(tile(&body, "testimonials")["ready"], json!(false));
    assert_eq!(tile(&body, "testimonials")["needs"], json!("writing"));
    assert_eq!(tile(&body, "gallery")["needs"], json!("picture"));
    assert_eq!(tile(&body, "catalog")["needs"], json!("catalog"));
    assert_eq!(tile(&body, "custom_code")["needs"], json!("code"));
    // The shop door is always offerable: the section stores words alone, and
    // the shop reads what is on sale live (S3.04f2).
    assert_eq!(tile(&body, "tickets")["ready"], json!(true));
    assert_eq!(tile(&body, "tickets")["section"]["type"], json!("tickets"));
    // A tile that is not ready carries no section for the editor to drop.
    assert_eq!(tile(&body, "catalog")["section"], Value::Null);

    // Write something. The palette is a read of the website, so what the owner
    // put on the page is what the next tile is made of.
    let sections = format!("/sites/{site}/pages/{page}/sections");
    let (status, _) = post(
        &h.app,
        &h.token,
        &sections,
        json!({ "section": { "type": "testimonials", "items": [
            { "quote": "The only beans I buy.", "author": "Ines Kortekaas" }
        ] } }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = post(
        &h.app,
        &h.token,
        &sections,
        json!({ "section": { "type": "gallery", "images": [
            { "blob_id": "9hK3vQ2mR8pT1xWz4bC5dg", "alt": "Roasting drum mid-batch" }
        ] } }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = post(
        &h.app,
        &h.token,
        &sections,
        json!({ "section": { "type": "hero", "heading": "Coffee roasted the morning it ships",
                             "subheading": "Small-batch roastery on the harbour" } }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (_, body) = get(&h.app, &h.token, &palette).await;
    assert_eq!(tile(&body, "testimonials")["ready"], json!(true));
    assert_eq!(
        tile(&body, "testimonials")["section"]["items"][0]["author"],
        json!("Ines Kortekaas")
    );
    assert_eq!(tile(&body, "gallery")["ready"], json!(true));
    assert_eq!(
        tile(&body, "gallery")["section"]["images"][0]["alt"],
        json!("Roasting drum mid-batch")
    );
    // A picture plus a line of their own makes the words-beside-a-picture block
    // possible too — and the line is the one under their own headline, never a
    // sentence this build wrote.
    assert_eq!(tile(&body, "text_image")["ready"], json!(true));
    assert_eq!(
        tile(&body, "text_image")["section"]["body"],
        json!("Small-batch roastery on the harbour")
    );
    // The banner is the website's name, not a second copy of the headline it
    // already carries.
    assert_eq!(
        tile(&body, "hero")["section"]["heading"],
        json!("Nordwind Coffee Roasters")
    );
}

#[tokio::test]
async fn a_tile_previews_as_the_page_it_would_produce() {
    let h = harness("sites-palette-preview").await;
    let (site, page) = site_and_page(&h, "Nordwind Coffee Roasters", "pp").await;
    let preview = format!("/sites/{site}/pages/{page}/palette");

    let (status, content_type, html) =
        get_text(&h.app, &h.token, &format!("{preview}/hero/preview")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(content_type.starts_with("text/html"), "{content_type}");
    // The tenant's own name, rendered by the public renderer, in a complete
    // self-contained document — the tile is a picture of the real thing.
    assert!(html.starts_with("<!doctype html>"), "{}", &html[..80]);
    assert!(html.contains("Nordwind Coffee Roasters"), "{html}");
    assert!(html.contains("<style"), "the stylesheet is inlined");
    // Not the editable render: a section that does not exist yet has no
    // coordinate to rewrite copy at.
    assert!(!html.contains("data-alo-text"), "{html}");

    // A tile with nothing of the tenant's own to show says so, and an unknown
    // type is a 404 like every other unresolvable id on this surface.
    let (status, _) = get(&h.app, &h.token, &format!("{preview}/pricing/preview")).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, _) = get(&h.app, &h.token, &format!("{preview}/parallax/preview")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// The arc the item exists for: read the palette, take a tile, drop it at a
/// position — through the one section op every other gesture uses, so it lands
/// in the undo history and the schema gate like anything else.
#[tokio::test]
async fn a_tile_can_be_dropped_between_two_sections() {
    let h = harness("sites-palette-drop").await;
    let (site, page) = site_and_page(&h, "Nordwind Coffee Roasters", "pd").await;
    let sections = format!("/sites/{site}/pages/{page}/sections");
    for section in [
        json!({ "type": "hero", "heading": "Coffee roasted the morning it ships" }),
        json!({ "type": "faq", "items": [
            { "question": "Do you ship abroad?", "answer": "Across the EU, yes." }
        ] }),
    ] {
        let (status, body) = post(&h.app, &h.token, &sections, json!({ "section": section })).await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }

    let (_, palette) = get(
        &h.app,
        &h.token,
        &format!("/sites/{site}/pages/{page}/palette"),
    )
    .await;
    let seeded = tile(&palette, "contact_form")["section"].clone();
    assert!(seeded.is_object(), "the contact tile is always ready");

    let (status, body) = post(
        &h.app,
        &h.token,
        &sections,
        json!({ "section": seeded, "index": 1 }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let kinds: Vec<&str> = body["sections"]["sections"]
        .as_array()
        .unwrap()
        .iter()
        .map(|section| section["type"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["hero", "contact_form", "faq"]);
    // Dropped between two sections, and wired: the write path made the form
    // record this contact block submits to (S1.16c2).
    assert!(
        body["sections"]["sections"][1]["form_id"].is_string(),
        "{body}"
    );
}

/// Mandatory: the palette is a read of a website, so another tenant's website
/// must be invisible on both routes — the same `404` a mistyped id gets.
#[tokio::test]
async fn another_tenants_palette_is_invisible() {
    let owner = harness("sites-palette-owner").await;
    let outsider = harness_on(Arc::clone(&owner.store), "sites-palette-outsider").await;
    let (site, page) = site_and_page(&owner, "Nordwind Coffee Roasters", "po").await;

    let palette = format!("/sites/{site}/pages/{page}/palette");
    let (status, _) = get(&outsider.app, &outsider.token, &palette).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = get(
        &outsider.app,
        &outsider.token,
        &format!("{palette}/hero/preview"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // And neither route answers at all without a token.
    for uri in [palette.clone(), format!("{palette}/hero/preview")] {
        let req = Request::builder()
            .method("GET")
            .uri(&uri)
            .body(Body::empty())
            .unwrap();
        let (status, _) = send(&owner.app, req).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{uri}");
    }
}
