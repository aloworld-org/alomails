//! Sites AI generation and page-edit proposals through the real router and Postgres.
//!
//! The model is a scripted localhost fixture server; this suite never calls an
//! external AI service. It pins draft-only atomic persistence, the typed
//! unconfigured branch, invalid-output rollback, authentication, and the
//! mandatory wrong-tenant boundary.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::sync::{Arc, Mutex};

use alo_sites::serve::{AppState as PublicAppState, app as public_app};
use alo_store::{BlobStore, Page, SitePublicStore};
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use tokio::time::{Duration, sleep};
use tower::ServiceExt;

use common::{Harness, database_url, harness, harness_on, send};

type Seen = Arc<Mutex<Vec<Value>>>;

async fn scripted_model(script: Vec<String>) -> (String, Seen) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let seen: Seen = Arc::new(Mutex::new(Vec::new()));
    let record = Arc::clone(&seen);
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let record = Arc::clone(&record);
            let script = script.clone();
            tokio::spawn(async move {
                let mut buffer = Vec::new();
                let mut chunk = [0_u8; 8192];
                let body = loop {
                    let Ok(read) = socket.read(&mut chunk).await else {
                        return;
                    };
                    if read == 0 {
                        return;
                    }
                    buffer.extend_from_slice(&chunk[..read]);
                    let Some(end) = buffer.windows(4).position(|window| window == b"\r\n\r\n")
                    else {
                        continue;
                    };
                    let head = String::from_utf8_lossy(&buffer[..end]).into_owned();
                    let length = head
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|value| value.trim().parse().unwrap_or(0))
                        })
                        .unwrap_or(0);
                    if buffer.len() >= end + 4 + length {
                        break buffer[end + 4..end + 4 + length].to_vec();
                    }
                };
                let turn = {
                    let mut requests = record.lock().unwrap();
                    requests.push(serde_json::from_slice(&body).unwrap_or(Value::Null));
                    requests.len() - 1
                };
                let content = script
                    .get(turn)
                    .or_else(|| script.last())
                    .cloned()
                    .unwrap_or_default();
                let answer = json!({
                    "choices": [{ "message": { "role": "assistant", "content": content } }]
                })
                .to_string();
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    answer.len(),
                    answer
                );
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.flush().await;
            });
        }
    });
    (format!("http://{addr}"), seen)
}

async fn use_model(harness: &Harness, base_url: &str) {
    let id = format!("sites-generation-{}", harness.tenant);
    harness
        .acc
        .upsert_ai_provider(
            &id,
            "openai",
            "scripted",
            base_url,
            "fixture-model",
            None,
            true,
        )
        .await
        .unwrap();
    harness.acc.set_default_ai_provider(&id).await.unwrap();
}

async fn post(app: &Router, token: Option<&str>, uri: &str, body: Value) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json");
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    send(app, request.body(Body::from(body.to_string())).unwrap()).await
}

async fn get(app: &Router, token: &str, uri: &str) -> (StatusCode, Value) {
    send(
        app,
        Request::builder()
            .uri(uri)
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await
}

async fn put(app: &Router, token: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    send(
        app,
        Request::builder()
            .method("PUT")
            .uri(uri)
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap(),
    )
    .await
}

fn subdomain(tag: &str, h: &Harness) -> String {
    let suffix: String = h
        .tenant
        .as_str()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|character| character.to_ascii_lowercase())
        .take(20)
        .collect();
    format!("{tag}-{suffix}")
}

fn valid_fixture(subdomain: &str) -> String {
    include_str!("../../../../platform/alo-ai/tests/fixtures/sites/valid_full_site.json")
        .replace("juniper-bakery", subdomain)
}

async fn public_text(state: &Arc<PublicAppState>, request: Request<Body>) -> (StatusCode, String) {
    let response = public_app(Arc::clone(state))
        .oneshot(request)
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

#[tokio::test]
async fn generation_requires_auth_and_unconfigured_is_typed_without_writes() {
    let h = harness("sites-generate-unconfigured").await;
    let description = json!({ "description": "A neighbourhood bakery" });

    let (status, body) = post(&h.app, None, "/sites/generate", description.clone()).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");

    let (status, body) = post(&h.app, Some(&h.token), "/sites/generate", description).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(body["reason"], "unconfigured");
    assert!(body["detail"].as_str().unwrap().contains("blank site"));

    let (status, body) = get(&h.app, &h.token, "/sites").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["sites"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn fixture_creates_one_complete_draft_hidden_from_another_tenant() {
    let a = harness("sites-generate-fixture").await;
    let b = harness_on(Arc::clone(&a.store), "sites-generate-other").await;
    let generated_subdomain = subdomain("generated", &a);
    let (base_url, seen) = scripted_model(vec![valid_fixture(&generated_subdomain)]).await;
    use_model(&a, &base_url).await;

    let (status, body) = post(
        &a.app,
        Some(&a.token),
        "/sites/generate",
        json!({ "description": "A small Utrecht bakery with a contact page" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["site"]["status"], "draft");
    assert_eq!(body["site"]["subdomain"], generated_subdomain);
    assert_eq!(body["pages"].as_array().unwrap().len(), 2);
    assert_eq!(
        body["pages"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|page| page["home"] == true)
            .count(),
        1
    );
    assert!(body["pages"][0]["sections"]["sections"].is_array());
    assert_eq!(seen.lock().unwrap().len(), 1);

    let site = body["site"]["id"].as_str().unwrap();
    let (status, own) = get(&a.app, &a.token, &format!("/sites/{site}")).await;
    assert_eq!(status, StatusCode::OK, "{own}");
    assert!(own["publish"].is_null());

    // Mandatory wrong-tenant proof at the HTTP boundary.
    let (status, _) = get(&b.app, &b.token, &format!("/sites/{site}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = get(&b.app, &b.token, &format!("/sites/{site}/pages")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, other_sites) = get(&b.app, &b.token, "/sites").await;
    assert_eq!(status, StatusCode::OK, "{other_sites}");
    assert!(other_sites["sites"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn invalid_fixture_gets_one_repair_then_rolls_back_everything() {
    let h = harness("sites-generate-invalid").await;
    let invalid = "{\"schema_version\":1}".to_owned();
    let (base_url, seen) = scripted_model(vec![invalid.clone(), invalid]).await;
    use_model(&h, &base_url).await;

    let (status, body) = post(
        &h.app,
        Some(&h.token),
        "/sites/generate",
        json!({ "description": "A bakery" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["reason"], "invalid_generation");
    assert!(
        body["detail"]
            .as_str()
            .unwrap()
            .contains("Nothing was changed")
    );
    assert_eq!(seen.lock().unwrap().len(), 2);

    let (status, sites) = get(&h.app, &h.token, "/sites").await;
    assert_eq!(status, StatusCode::OK, "{sites}");
    assert!(sites["sites"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn page_edit_is_a_reviewable_proposal_until_approved_and_tenant_scoped() {
    let a = harness("sites-edit-fixture").await;
    let b = harness_on(Arc::clone(&a.store), "sites-edit-other").await;
    let (status, site) = post(
        &a.app,
        Some(&a.token),
        "/sites",
        json!({ "name": "Edit fixture", "subdomain": subdomain("edit", &a) }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{site}");
    let site_id = site["id"].as_str().unwrap();
    let pages_uri = format!("/sites/{site_id}/pages");
    let (status, page) = post(
        &a.app,
        Some(&a.token),
        &pages_uri,
        json!({ "title": "Home", "slug": "", "home": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{page}");
    let page_id = page["id"].as_str().unwrap();
    let sections_uri = format!("/sites/{site_id}/pages/{page_id}/sections");
    let original = json!({
        "schema_version": 1,
        "sections": [{
            "type": "hero",
            "heading": "Old heading",
            "subheading": null,
            "image": null,
            "primary_cta": null,
            "secondary_cta": null
        }]
    });
    let (status, body) = put(&a.app, &a.token, &sections_uri, original.clone()).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let proposal_fixture = json!({
        "schema_version": 1,
        "operations": [{
            "op": "rewrite_copy",
            "target": { "index": 0, "type": "hero" },
            "pointer": "/heading",
            "text": "A clearer welcome"
        }]
    })
    .to_string();
    let unscoped_fixture = json!({
        "schema_version": 1,
        "operations": [{
            "op": "set_prop",
            "target": { "index": 0, "type": "hero" },
            "pointer": "/heading",
            "value": "Changed through the wrong operation"
        }]
    })
    .to_string();
    let (base_url, seen) = scripted_model(vec![
        proposal_fixture.clone(),
        proposal_fixture,
        unscoped_fixture,
    ])
    .await;
    use_model(&a, &base_url).await;
    let edit_uri = format!("/sites/{site_id}/pages/{page_id}/ai-edits");

    let (status, proposed) = post(
        &a.app,
        Some(&a.token),
        &edit_uri,
        json!({ "instruction": "Make the welcome clearer" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{proposed}");
    assert_eq!(proposed["proposal"]["operations"][0]["op"], "rewrite_copy");
    let proposed_preview = proposed["previewHtml"].as_str().unwrap();
    assert!(proposed_preview.contains("A clearer welcome"));
    assert!(!proposed_preview.contains("Old heading"));
    assert_eq!(seen.lock().unwrap().len(), 1);

    // The same route accepts a structured per-field copy action, scopes the
    // model to that exact string leaf, and still returns a no-write proposal.
    let copy_request = json!({
        "copy": {
            "target": { "index": 0, "type": "hero" },
            "pointer": "/heading",
            "action": "shorter"
        }
    });
    let (status, copy_proposed) =
        post(&a.app, Some(&a.token), &edit_uri, copy_request.clone()).await;
    assert_eq!(status, StatusCode::OK, "{copy_proposed}");
    assert_eq!(
        copy_proposed["proposal"]["operations"][0]["pointer"],
        "/heading"
    );
    {
        let requests = seen.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert!(
            requests[1]["messages"][1]["content"]
                .as_str()
                .unwrap()
                .contains("Make it shorter")
        );
    }

    // Even a schema-valid model answer is refused when it changes anything
    // other than the one selected copy leaf.
    let (status, refused) = post(
        &a.app,
        Some(&a.token),
        &edit_uri,
        json!({
            "copy": {
                "target": { "index": 0, "type": "hero" },
                "pointer": "/heading",
                "action": "tone",
                "tone": "warm and direct"
            }
        }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
    assert_eq!(refused["reason"], "invalid_proposal");

    // Proposing writes nothing.
    let page_uri = format!("/sites/{site_id}/pages/{page_id}");
    let (status, unchanged) = get(&a.app, &a.token, &page_uri).await;
    assert_eq!(status, StatusCode::OK, "{unchanged}");
    assert_eq!(
        unchanged["sections"]["sections"].as_array().unwrap().len(),
        1
    );
    assert_eq!(
        unchanged["sections"]["sections"][0]["heading"],
        "Old heading"
    );

    // Mandatory wrong-tenant proof for both the proposal and persistence doors.
    let (status, _) = post(&b.app, Some(&b.token), &edit_uri, copy_request).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = put(
        &b.app,
        &b.token,
        &edit_uri,
        json!({ "proposal": proposed["proposal"].clone() }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, applied) = put(
        &a.app,
        &a.token,
        &edit_uri,
        json!({ "proposal": proposed["proposal"].clone() }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{applied}");
    assert_eq!(
        applied["sections"]["sections"][0]["heading"],
        "A clearer welcome"
    );
    let (status, stored) = get(&a.app, &a.token, &page_uri).await;
    assert_eq!(status, StatusCode::OK, "{stored}");
    assert_eq!(
        stored["sections"]["sections"][0]["heading"],
        "A clearer welcome"
    );
}

#[tokio::test]
async fn generated_site_reaches_public_form_notification_and_owner_inbox() {
    let owner = harness("sites-final-forms").await;
    let outsider = harness_on(Arc::clone(&owner.store), "sites-final-forms-other").await;
    let generated_subdomain = subdomain("final-forms", &owner);
    let (base_url, _) = scripted_model(vec![valid_fixture(&generated_subdomain)]).await;
    use_model(&owner, &base_url).await;

    // Fixture-backed AI generation persists one private, complete draft. Its
    // contact section is linked to a real form inside the same transaction.
    let (status, generated) = post(
        &owner.app,
        Some(&owner.token),
        "/sites/generate",
        json!({ "description": "A bakery with a public contact form" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{generated}");
    let site = generated["site"]["id"].as_str().unwrap();
    let pages = generated["pages"].as_array().unwrap();
    let home = pages.iter().find(|page| page["home"] == true).unwrap();
    let contact = pages.iter().find(|page| page["slug"] == "contact").unwrap();
    let home_id = home["id"].as_str().unwrap();
    let form_id = contact["sections"]["sections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|section| section["type"] == "contact_form")
        .and_then(|section| section["form_id"].as_str())
        .expect("generated contact section has a working form");

    // The owner edits a section and picks a different shipped theme before
    // publishing. These are the same authenticated doors the web editor uses.
    let (status, edited) = put(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/pages/{home_id}/sections/1"),
        json!({ "section": {
            "type": "hero",
            "heading": "Bread ready when Utrecht wakes",
            "subheading": "Baked before sunrise and served all morning.",
            "image": null,
            "primary_cta": { "label": "Visit the bakery", "href": "/contact" },
            "secondary_cta": null
        }}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{edited}");
    assert_eq!(
        edited["sections"]["sections"][1]["heading"],
        "Bread ready when Utrecht wakes"
    );
    let (status, theme) = put(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/theme"),
        json!({ "schema_version": 1, "preset": "midnight" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{theme}");
    let (status, published) = post(
        &owner.app,
        Some(&owner.token),
        &format!("/sites/{site}/publish"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{published}");

    // The separate anonymous service sees the committed publish by Host and
    // serves both the edited snapshot and the selected theme stylesheet.
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url())
        .await
        .unwrap();
    let public = PublicAppState::new(
        SitePublicStore::new(pool, BlobStore::in_memory(1024 * 1024)),
        "sites.test".to_owned(),
        b"sites-final-forms-analytics-secret",
    );
    let host = format!("{generated_subdomain}.sites.test");
    let (status, html) = public_text(
        &public,
        Request::builder()
            .uri("/")
            .header(header::HOST, &host)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{html}");
    assert!(html.contains("Bread ready when Utrecht wakes"));
    let (status, css) = public_text(
        &public,
        Request::builder()
            .uri("/assets/site.css")
            .header(header::HOST, &host)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{css}");
    assert!(css.contains("#f0b653"), "midnight theme reached public CSS");

    // A visitor submits through the rendered form target. The authenticated
    // submissions route (the UI's data source) and internal inbox see the
    // same tenant-owned row; the outsider sees neither.
    let (status, response) = public_text(
        &public,
        Request::builder()
            .method("POST")
            .uri(format!("/f/{form_id}"))
            .header(header::HOST, &host)
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .header("x-forwarded-for", "203.0.113.132")
            .body(Body::from(
                "name=Ada+Lovelace&email=ada%40example.test&message=Please+reserve+two+loaves.&website=",
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{response}");
    assert!(response.contains("Message sent"));

    let submissions_path = format!("/sites/{site}/submissions");
    let (status, submissions) = get(&owner.app, &owner.token, &submissions_path).await;
    assert_eq!(status, StatusCode::OK, "{submissions}");
    assert_eq!(submissions["submissions"].as_array().unwrap().len(), 1);
    assert_eq!(submissions["submissions"][0]["senderName"], "Ada Lovelace");
    assert_eq!(
        submissions["submissions"][0]["message"],
        "Please reserve two loaves."
    );
    let (status, _) = get(&outsider.app, &outsider.token, &submissions_path).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    alo_jmap::site_notify::run_due(&owner.store).await;
    let inbox = owner.acc.inbox().await.unwrap();
    let mut messages = Vec::new();
    for _ in 0..20 {
        messages = owner
            .acc
            .list_mailbox(&inbox, Page::default())
            .await
            .unwrap();
        if !messages.is_empty() {
            break;
        }
        sleep(Duration::from_millis(25)).await;
    }
    assert_eq!(
        messages.len(),
        1,
        "owner receives one internal notification"
    );
    assert!(messages[0].subject.contains("Ada Lovelace"));
    let outsider_inbox = outsider.acc.inbox().await.unwrap();
    assert!(
        outsider
            .acc
            .list_mailbox(&outsider_inbox, Page::default())
            .await
            .unwrap()
            .is_empty(),
        "another tenant receives no message"
    );
}
