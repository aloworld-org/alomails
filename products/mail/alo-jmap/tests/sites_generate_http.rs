//! Sites AI generation and page-edit proposals through the real router and Postgres.
//!
//! The model is a scripted localhost fixture server; this suite never calls an
//! external AI service. It pins draft-only atomic persistence, the typed
//! unconfigured branch, invalid-output rollback, authentication, and the
//! mandatory wrong-tenant boundary.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use common::{Harness, harness, harness_on, send};

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
