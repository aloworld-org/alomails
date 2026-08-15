//! The assistant's Public knowledge collection on the wire (ADR 0040 §1,
//! item S3.02d), driven through the real router over a real Postgres.
//!
//! The store suite already proves the binding rules (readability, the cap,
//! duplicates, tenant walls). What this suite pins is the **door**: that the
//! routes exist under `/sites/{id}/chat-knowledge`, that every one of them —
//! and the assistant's settings routes beside them — belongs to the site's
//! owner and nobody else, and that a foreign tenant or a stranger meets a
//! clean 404/401 rather than a hint that the site exists.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::{DriveLocation, NewDriveFile, UserId};
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use bytes::Bytes;
use serde_json::{Value, json};

use common::{Harness, get, harness, harness_on, send};

async fn request(
    app: &Router,
    token: &str,
    method: &str,
    uri: &str,
    body: Value,
) -> (StatusCode, Value) {
    send(
        app,
        Request::builder()
            .method(method)
            .uri(uri)
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap(),
    )
    .await
}

/// A unique dns-safe subdomain per test tenant.
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

async fn create_site(h: &Harness, tag: &str) -> String {
    let (status, body) = request(
        &h.app,
        &h.token,
        "POST",
        "/sites",
        json!({ "name": "Knowledge shop", "subdomain": sub(tag, h) }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "create site failed: {body}");
    body["id"].as_str().expect("site id").to_owned()
}

/// One readable alo Doc in the tenant's Drive, created through the store the
/// way the Docs surface does it.
async fn create_doc(h: &Harness, name: &str, text: &str) -> String {
    let bytes = Bytes::from(
        json!([
            {"type": "paragraph", "content": [{"type": "text", "text": text, "styles": {}}]}
        ])
        .to_string(),
    );
    let size = i64::try_from(bytes.len()).unwrap();
    let blob = h
        .acc
        .put_blob(bytes, Some("application/json"))
        .await
        .unwrap();
    h.acc
        .drive_create_file(
            &DriveLocation::Personal,
            None,
            &NewDriveFile {
                name: name.to_owned(),
                blob_id: blob.as_str().to_owned(),
                size,
                kind: Some("doc".to_owned()),
                ..NewDriveFile::default()
            },
        )
        .await
        .unwrap()
        .as_str()
        .to_owned()
}

/// A second person in the same tenant, with their own login — no site grant
/// yet; tests add the roles they need.
async fn colleague(h: &Harness, tag: &str) -> (String, UserId) {
    let email = format!("{tag}-{}@example.test", h.tenant);
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
    (token, user)
}

#[tokio::test]
async fn the_owner_publishes_lists_and_withdraws_a_source() {
    let h = harness("chat-knowledge").await;
    let site = create_site(&h, "know").await;
    let doc = create_doc(&h, "Price list", "Day rate 900 euro").await;

    // Empty to start: the collection exists the moment the site does.
    let (status, body) = get(&h.app, &h.token, &format!("/sites/{site}/chat-knowledge")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["sources"].as_array().map(Vec::len), Some(0));

    // Publish: the answer is the stored binding, title included.
    let (status, added) = request(
        &h.app,
        &h.token,
        "POST",
        &format!("/sites/{site}/chat-knowledge"),
        json!({ "docNodeId": doc }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{added}");
    assert_eq!(added["docNodeId"].as_str(), Some(doc.as_str()));
    assert_eq!(added["title"].as_str(), Some("Price list"));
    assert_eq!(added["trashed"].as_bool(), Some(false));
    let source = added["id"].as_str().expect("source id").to_owned();

    // Listed, oldest first.
    let (status, body) = get(&h.app, &h.token, &format!("/sites/{site}/chat-knowledge")).await;
    assert_eq!(status, StatusCode::OK);
    let sources = body["sources"].as_array().expect("sources");
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0]["id"].as_str(), Some(source.as_str()));

    // Publishing the same document twice is one clear refusal.
    let (status, body) = request(
        &h.app,
        &h.token,
        "POST",
        &format!("/sites/{site}/chat-knowledge"),
        json!({ "docNodeId": doc }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");

    // Withdraw: the document leaves the assistant, Drive keeps it.
    let (status, body) = request(
        &h.app,
        &h.token,
        "DELETE",
        &format!("/sites/{site}/chat-knowledge/{source}"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (_, body) = get(&h.app, &h.token, &format!("/sites/{site}/chat-knowledge")).await;
    assert_eq!(body["sources"].as_array().map(Vec::len), Some(0));
    assert!(
        h.acc
            .drive_node(&alo_store::DriveNodeId::new(doc))
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn a_folder_is_refused_with_the_rule_spelled_out() {
    let h = harness("chat-knowledge-folder").await;
    let site = create_site(&h, "knowf").await;
    let folder = h
        .acc
        .drive_create_folder(&DriveLocation::Personal, None, "Internal")
        .await
        .unwrap();
    let (status, body) = request(
        &h.app,
        &h.token,
        "POST",
        &format!("/sites/{site}/chat-knowledge"),
        json!({ "docNodeId": folder.as_str() }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        body["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("readable"),
        "the refusal names the rule: {body}"
    );
}

#[tokio::test]
async fn a_foreign_tenant_and_a_stranger_see_nothing() {
    let h = harness("chat-knowledge-walls").await;
    let site = create_site(&h, "knoww").await;
    let doc = create_doc(&h, "Price list", "Day rate 900 euro").await;
    let other = harness_on(h.store.clone(), "chat-knowledge-walls-b").await;

    // The foreign tenant meets a 404 on every door — never a 403 that
    // concedes the site exists.
    for (method, uri, body) in [
        ("GET", format!("/sites/{site}/chat-knowledge"), json!({})),
        (
            "POST",
            format!("/sites/{site}/chat-knowledge"),
            json!({ "docNodeId": doc }),
        ),
        ("GET", format!("/sites/{site}/chat-settings"), json!({})),
        (
            "PUT",
            format!("/sites/{site}/chat-settings"),
            json!({ "enabled": true, "monthlyCeilingCents": 1000 }),
        ),
    ] {
        let (status, answer) = if method == "GET" {
            get(&other.app, &other.token, &uri).await
        } else {
            request(&other.app, &other.token, method, &uri, body).await
        };
        assert_eq!(status, StatusCode::NOT_FOUND, "{method} {uri}: {answer}");
    }

    // No token at all is a 401 before anything is read.
    let (status, _) = send(
        &h.app,
        Request::builder()
            .method("GET")
            .uri(format!("/sites/{site}/chat-knowledge"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn the_assistant_surface_is_the_owners_not_the_editors() {
    let h = harness("chat-knowledge-owner").await;
    let site = create_site(&h, "knowo").await;
    let doc = create_doc(&h, "Price list", "Day rate 900 euro").await;

    // A restricted site editor, invited to edit exactly this site. The grant
    // carries the SiteEditor tenant role with it.
    let (editor_token, editor) = colleague(&h, "site-editor").await;
    h.ts.grant_site_editor(&editor, &alo_store::SiteId::new(site.clone()), &h.user)
        .await
        .unwrap();

    // An uninvolved colleague in the same tenant, no site role at all.
    let (colleague_token, _) = colleague(&h, "bystander").await;

    for token in [&editor_token, &colleague_token] {
        let (status, body) = get(&h.app, token, &format!("/sites/{site}/chat-knowledge")).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "list: {body}");
        let (status, body) = request(
            &h.app,
            token,
            "POST",
            &format!("/sites/{site}/chat-knowledge"),
            json!({ "docNodeId": doc }),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "add: {body}");
        let (status, body) = request(
            &h.app,
            token,
            "PUT",
            &format!("/sites/{site}/chat-settings"),
            json!({ "enabled": true, "monthlyCeilingCents": 250000 }),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "settings: {body}");
    }

    // The refusals changed nothing: the owner still reads an empty
    // collection and the default, switched-off settings.
    let (_, body) = get(&h.app, &h.token, &format!("/sites/{site}/chat-knowledge")).await;
    assert_eq!(body["sources"].as_array().map(Vec::len), Some(0));
    let (_, body) = get(&h.app, &h.token, &format!("/sites/{site}/chat-settings")).await;
    assert_eq!(body["enabled"].as_bool(), Some(false));
}
