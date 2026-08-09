//! The form-notification sweep (ADR 0036, S1.16c1), driven end-to-end over
//! a real Postgres: a new site-form submission becomes ONE internally
//! delivered message in the site owner's own inbox — the right tenant's
//! inbox and nobody else's — exactly once, and hostile submission fields
//! can never inject headers into that message.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::sync::Arc;

use alo_sites::serve::{AppState as PublicAppState, app as public_app};
use alo_store::{BlobStore, Page, SiteFormId, SiteId, SitePublicStore};
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;

use common::{database_url, harness, harness_on, send};

async fn post(app: &Router, token: &str, path: &str, body: Value) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("POST")
        .uri(path)
        .header("authorization", format!("Bearer {token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    send(app, request).await
}

fn created_id(kind: &str, (status, body): (StatusCode, Value)) -> String {
    assert_eq!(status, StatusCode::OK, "create {kind} failed: {body}");
    body["id"].as_str().expect("created id").to_owned()
}

/// Splits a raw RFC 5322 message into (headers, base64-decoded body text).
fn open_message(raw: &[u8]) -> (String, String) {
    let raw = String::from_utf8_lossy(raw).into_owned();
    let (headers, body) = raw.split_once("\r\n\r\n").expect("header/body split");
    let body_bytes = B64
        .decode(body.replace(['\r', '\n'], ""))
        .expect("base64 body");
    (
        headers.to_owned(),
        String::from_utf8(body_bytes).expect("utf-8 body"),
    )
}

/// One test on purpose: the sweep is global, so concurrently running
/// scenarios in separate tests could claim (and deliver) each other's rows
/// mid-assertion. Sequenced here, every step's outcome is deterministic.
#[tokio::test]
async fn notifications_land_in_the_owning_inbox_only_and_resist_injection() {
    let a = harness("notifa").await;
    // Tenant B lives on the SAME store handle: production runs one process
    // (one blob store) for every tenant, and the sweep must serve them all.
    let b = harness_on(a.store.clone(), "notifb").await;

    // Unique subdomains: the compose Postgres is shared across runs.
    let sub = |tag: &str, t: &alo_store::TenantId| {
        let salt: String = t
            .as_str()
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .map(|c| c.to_ascii_lowercase())
            .take(20)
            .collect();
        format!("{tag}{salt}")
    };

    // The owner builds the complete form through the authenticated editor
    // surface: adding the section creates and links the underlying form.
    let site_a = created_id(
        "site",
        post(
            &a.app,
            &a.token,
            "/sites",
            json!({
                "name": "Alpha Roastery",
                "subdomain": sub("nfa", &a.tenant),
            }),
        )
        .await,
    );
    let page_a = created_id(
        "page",
        post(
            &a.app,
            &a.token,
            &format!("/sites/{site_a}/pages"),
            json!({ "title": "Home", "home": true }),
        )
        .await,
    );
    let (status, body) = post(
        &a.app,
        &a.token,
        &format!("/sites/{site_a}/pages/{page_a}/sections"),
        json!({ "section": {
            "type": "contact_form",
            "heading": "Contact",
            "success_message": "Thank you."
        }}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "add form section failed: {body}");
    let form_a = SiteFormId::new(
        body["sections"]["sections"][0]["form_id"]
            .as_str()
            .expect("linked form id"),
    );
    let site_a = SiteId::new(site_a);
    let (status, body) = post(
        &a.app,
        &a.token,
        &format!("/sites/{site_a}/publish"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "publish failed: {body}");

    // A public visitor then submits through the separate alo-sites router.
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url())
        .await
        .unwrap();
    let public = PublicAppState::new(
        SitePublicStore::new(pool, BlobStore::in_memory(1024 * 1024)),
        "sites.test".to_owned(),
        b"site-notify-local-analytics-secret",
    );
    let request = Request::builder()
        .method("POST")
        .uri(format!("/f/{form_a}"))
        .header(header::HOST, "alpha.sites.test")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header("x-forwarded-for", "203.0.113.44")
        .body(Body::from(
            "name=Ada+Lovelace&email=ada%40example.test&message=I+would+like+five+kilos+of+beans.&website=",
        ))
        .unwrap();
    let response = public_app(Arc::clone(&public))
        .oneshot(request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        a.acc
            .site_form_submissions(&site_a, &form_a)
            .await
            .unwrap()
            .len(),
        1,
        "the public post creates the owner's submission row"
    );
    assert!(
        b.acc
            .site_form_submissions(&site_a, &form_a)
            .await
            .unwrap()
            .is_empty(),
        "the linked form and submission remain invisible to another tenant"
    );

    let site_b = b
        .acc
        .create_site("Beta Studio", &sub("nfb", &b.tenant))
        .await
        .unwrap();
    let form_b = b.acc.create_site_form(&site_b, "Careers").await.unwrap();
    b.acc
        .add_site_form_submission(
            &site_b,
            &form_b,
            "Grace Hopper",
            "grace@example.test",
            "Beta-only submission words.",
        )
        .await
        .unwrap();

    // One sweep serves every tenant: each notification is built from the
    // claimed row's own context and delivered through that tenant's door.
    alo_jmap::site_notify::run_due(&a.store).await;

    let inbox_a = a.acc.inbox().await.unwrap();
    let listed_a = a.acc.list_mailbox(&inbox_a, Page::default()).await.unwrap();
    assert_eq!(listed_a.len(), 1, "owner A gets exactly one notification");
    let raw_a = a.acc.message_bytes(&listed_a[0].id).await.unwrap();
    let (headers_a, body_a) = open_message(&raw_a);
    assert!(
        headers_a.contains("Subject: New message from Ada Lovelace (Alpha Roastery)"),
        "subject names the sender and site: {headers_a}"
    );
    assert!(
        headers_a.contains("Reply-To: \"Ada Lovelace\" <ada@example.test>"),
        "reply goes to the visitor: {headers_a}"
    );
    assert!(
        headers_a.contains(&format!("To: {}", a.email)),
        "addressed to the owner: {headers_a}"
    );
    assert!(body_a.contains("I would like five kilos of beans."));
    assert!(body_a.contains("Ada Lovelace <ada@example.test>"));
    assert!(body_a.contains("\"Contact\" form on Alpha Roastery"));
    assert!(
        !body_a.contains("Beta-only"),
        "tenant A must never see tenant B's submission"
    );

    let inbox_b = b.acc.inbox().await.unwrap();
    let listed_b = b.acc.list_mailbox(&inbox_b, Page::default()).await.unwrap();
    assert_eq!(listed_b.len(), 1, "owner B gets exactly one notification");
    let (_, body_b) = open_message(&b.acc.message_bytes(&listed_b[0].id).await.unwrap());
    assert!(body_b.contains("Beta-only submission words."));
    assert!(
        !body_b.contains("five kilos"),
        "tenant B must never see tenant A's submission"
    );

    // Claimed means notified: another sweep delivers nothing new to anyone.
    alo_jmap::site_notify::run_due(&b.store).await;
    assert_eq!(
        a.acc
            .list_mailbox(&inbox_a, Page::default())
            .await
            .unwrap()
            .len(),
        1,
        "a submission is never delivered twice"
    );
    assert_eq!(
        b.acc
            .list_mailbox(&inbox_b, Page::default())
            .await
            .unwrap()
            .len(),
        1
    );

    // Hostile fields: a CR/LF-bearing name (the write gate bounds but does
    // not forbid inner whitespace) and a non-ASCII name both cross into the
    // message without injecting headers — CR/LF dies in the RFC 2047 path,
    // free text travels base64.
    a.acc
        .add_site_form_submission(
            &site_a,
            &form_a,
            "Eve\r\nX-Evil: injected",
            "eve@example.test",
            "Injection attempt body.",
        )
        .await
        .unwrap();
    a.acc
        .add_site_form_submission(
            &site_a,
            &form_a,
            "Åsa Ödegård",
            "asa@example.test",
            "Hej från Sverige!",
        )
        .await
        .unwrap();
    alo_jmap::site_notify::run_due(&a.store).await;

    let listed = a.acc.list_mailbox(&inbox_a, Page::default()).await.unwrap();
    assert_eq!(listed.len(), 3);
    for summary in &listed {
        let raw = a.acc.message_bytes(&summary.id).await.unwrap();
        let (headers, _) = open_message(&raw);
        // The hostile name may appear as inert TEXT inside a header value
        // (CR/LF sanitized to spaces); what must never exist is a header
        // LINE of the attacker's making.
        assert!(
            !headers.lines().any(|l| l.starts_with("X-Evil")),
            "submission fields must never become headers: {headers}"
        );
    }
    // The non-ASCII name arrives as an RFC 2047 encoded word on the wire and
    // reads back decoded in the stored subject.
    assert!(
        listed.iter().any(|m| m.subject.contains("Åsa Ödegård")),
        "non-ASCII sender names survive the encode/decode round trip"
    );
}
