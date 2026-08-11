//! The per-site editor authorization matrix through the real HTTP router.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::UserId;
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use common::{Harness, harness, send};

fn subdomain(tag: &str, h: &Harness) -> String {
    let salt: String = h
        .tenant
        .as_str()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|value| value.to_ascii_lowercase())
        .take(20)
        .collect();
    format!("{tag}{salt}")
}

async fn colleague(h: &Harness) -> (String, UserId) {
    let email = format!("site-editor-{}@example.test", h.tenant);
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

#[tokio::test]
async fn an_editor_sees_and_changes_only_the_granted_site() {
    let h = harness("site-editor-scope").await;
    let (token, editor) = colleague(&h).await;
    let granted = h
        .acc
        .create_site("Granted", &subdomain("granted", &h))
        .await
        .unwrap();
    let other = h
        .acc
        .create_site("Other", &subdomain("other", &h))
        .await
        .unwrap();
    h.ts.grant_site_editor(&editor, &granted, &h.user)
        .await
        .unwrap();

    let (status, body) = request(&h.app, &token, "GET", "/sites", json!({})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let sites = body["sites"].as_array().unwrap();
    assert_eq!(sites.len(), 1);
    assert_eq!(sites[0]["id"], granted.as_str());

    let (status, body) = request(
        &h.app,
        &token,
        "PUT",
        &format!("/sites/{granted}"),
        json!({ "name": "Changed by editor" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        h.acc.site(&granted).await.unwrap().unwrap().name,
        "Changed by editor"
    );

    for uri in [format!("/sites/{other}"), "/sites/not-a-site".to_owned()] {
        let (status, body) = request(&h.app, &token, "GET", &uri, json!({})).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{uri}: {body}");
    }
    let (status, _) = request(
        &h.app,
        &token,
        "POST",
        "/sites",
        json!({ "name": "No", "subdomain": "not-allowed" }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn the_site_editor_role_opens_no_surrounding_workspace_surface() {
    let h = harness("site-editor-doors").await;
    let (token, editor) = colleague(&h).await;
    let site = h
        .acc
        .create_site("Granted", &subdomain("doors", &h))
        .await
        .unwrap();
    h.ts.grant_site_editor(&editor, &site, &h.user)
        .await
        .unwrap();

    for uri in [
        "/contacts",
        "/drive/list",
        "/calendar/calendars",
        "/tasks",
        "/billing/customers",
        "/crm/deals",
        "/admin/users",
        "/.well-known/jmap",
    ] {
        let (status, body) = request(&h.app, &token, "GET", uri, json!({})).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{uri}: {body}");
    }

    let (status, body) = request(&h.app, &token, "GET", &format!("/sites/{site}"), json!({})).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    h.ts.revoke_site_editor(&editor, &site).await.unwrap();
    let (status, _) = request(&h.app, &token, "GET", "/contacts", json!({})).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "revocation restores ordinary membership"
    );
}
