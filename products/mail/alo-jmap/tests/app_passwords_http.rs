//! `/settings/app-passwords` — the self-service routes the settings screen
//! owns app passwords through (mail M1.3): create (the one response the
//! secret ever appears in), list (records only, never a secret), revoke
//! (immediate, same-tenant-same-user only).
//!
//! The part worth proving on the wire is the boundary: every operation is
//! scoped by the token's `(tenant, user)`, so another tenant's user holding
//! a stolen id gets the same clean 404 as an id that never existed — and
//! the revoked credential actually stops authenticating, which only the
//! identity seam underneath can say.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use common::{get, harness, harness_on, send};
use serde_json::{Value, json};

/// POSTs JSON to `path` with the given bearer token.
async fn post_json(app: &Router, token: &str, path: &str, body: Value) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("POST")
        .uri(path)
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    send(app, req).await
}

/// DELETEs `path` with the given bearer token.
async fn delete(app: &Router, token: &str, path: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("DELETE")
        .uri(path)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    send(app, req).await
}

#[tokio::test]
async fn create_shows_the_secret_once_and_the_list_never_does() {
    let h = harness("apw-create").await;

    let (status, body) = post_json(
        &h.app,
        &h.token,
        "/settings/app-passwords",
        json!({ "name": "Thunderbird on the desk machine" }),
    )
    .await;
    assert!(status.is_success(), "created: {status} {body}");
    let secret = body["secret"].as_str().unwrap();
    // The generated shape a person transcribes: xxxx-xxxx-xxxx-xxxx.
    assert_eq!(secret.len(), 19, "{body}");
    assert!(
        secret.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
        "{body}"
    );
    let id = body["id"].as_str().unwrap().to_owned();

    // The secret from the wire is the credential the legacy seam accepts —
    // the create response is not just display, it is the working password.
    let principal = h
        .identity
        .verify_app_password(&h.email, secret)
        .await
        .unwrap()
        .expect("the created secret authenticates");
    assert_eq!(principal.user, h.user);
    assert_eq!(principal.tenant, h.tenant);

    // The list carries the record, never the secret (not even hashed).
    let (status, body) = get(&h.app, &h.token, "/settings/app-passwords").await;
    assert!(status.is_success());
    let list = body["appPasswords"].as_array().unwrap();
    assert_eq!(list.len(), 1, "{body}");
    assert_eq!(list[0]["id"], json!(id), "{body}");
    assert_eq!(
        list[0]["name"],
        json!("Thunderbird on the desk machine"),
        "{body}"
    );
    assert!(list[0]["createdAt"].is_string(), "{body}");
    // Just verified above — the list must show it as used.
    assert!(list[0]["lastUsedAt"].is_string(), "{body}");
    assert!(
        !body.to_string().contains(secret),
        "the secret must never appear in a list response",
    );
}

#[tokio::test]
async fn a_nameless_password_is_refused() {
    let h = harness("apw-noname").await;

    let (status, _b) = post_json(&h.app, &h.token, "/settings/app-passwords", json!({})).await;
    assert_eq!(status.as_u16(), 422, "no name: {status}");

    let (status, _b) = post_json(
        &h.app,
        &h.token,
        "/settings/app-passwords",
        json!({ "name": "   " }),
    )
    .await;
    assert_eq!(status.as_u16(), 422, "blank name: {status}");

    let (status, _b) = post_json(
        &h.app,
        &h.token,
        "/settings/app-passwords",
        json!({ "name": "x".repeat(101) }),
    )
    .await;
    assert_eq!(status.as_u16(), 422, "overlong name: {status}");
}

#[tokio::test]
async fn revoking_stops_the_credential_on_the_next_connection() {
    let h = harness("apw-revoke").await;

    let (_s, created) = post_json(
        &h.app,
        &h.token,
        "/settings/app-passwords",
        json!({ "name": "old laptop" }),
    )
    .await;
    let id = created["id"].as_str().unwrap();
    let secret = created["secret"].as_str().unwrap();

    let (status, body) = delete(&h.app, &h.token, &format!("/settings/app-passwords/{id}")).await;
    assert!(status.is_success(), "revoked: {status} {body}");

    // Gone from the list, and — the part revocation is for — no longer a
    // working credential at the seam legacy protocols authenticate through.
    let (_s, body) = get(&h.app, &h.token, "/settings/app-passwords").await;
    assert_eq!(body["appPasswords"].as_array().unwrap().len(), 0, "{body}");
    assert!(
        h.identity
            .verify_app_password(&h.email, secret)
            .await
            .unwrap()
            .is_none(),
        "a revoked app password must not authenticate",
    );

    // Revoking it again is the same 404 as never having existed.
    let (status, _b) = delete(&h.app, &h.token, &format!("/settings/app-passwords/{id}")).await;
    assert_eq!(status.as_u16(), 404);
}

#[tokio::test]
async fn another_tenant_cannot_see_or_revoke_them() {
    // Two tenants on ONE store handle, the way production runs — the only
    // arrangement in which a cross-tenant reach could even be attempted.
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .connect(&common::database_url())
        .await
        .unwrap();
    let store = Arc::new(alo_store::Store::new(
        pool,
        alo_store::BlobStore::in_memory(1024 * 1024),
    ));
    store.migrate().await.unwrap();
    let a = harness_on(Arc::clone(&store), "apw-tenant-a").await;
    let b = harness_on(Arc::clone(&store), "apw-tenant-b").await;

    let (_s, created) = post_json(
        &a.app,
        &a.token,
        "/settings/app-passwords",
        json!({ "name": "phone" }),
    )
    .await;
    let id = created["id"].as_str().unwrap();
    let secret = created["secret"].as_str().unwrap();

    // B's list is B's — A's record is not in it.
    let (status, body) = get(&b.app, &b.token, "/settings/app-passwords").await;
    assert!(status.is_success());
    assert_eq!(
        body["appPasswords"].as_array().unwrap().len(),
        0,
        "another tenant's records must not appear: {body}"
    );

    // B revoking A's id gets the same clean 404 as an unknown id — and the
    // credential keeps working, because nothing was deleted.
    let (status, _b2) = delete(&b.app, &b.token, &format!("/settings/app-passwords/{id}")).await;
    assert_eq!(status.as_u16(), 404, "cross-tenant revoke must be a 404");
    assert!(
        a.identity
            .verify_app_password(&a.email, secret)
            .await
            .unwrap()
            .is_some(),
        "a foreign revoke attempt must not have deleted the credential",
    );
}

#[tokio::test]
async fn every_route_requires_a_token() {
    let h = harness("apw-unauth").await;

    let req = Request::builder()
        .method("GET")
        .uri("/settings/app-passwords")
        .body(Body::empty())
        .unwrap();
    let (status, _b) = send(&h.app, req).await;
    assert_eq!(status.as_u16(), 401);

    let req = Request::builder()
        .method("POST")
        .uri("/settings/app-passwords")
        .header("content-type", "application/json")
        .body(Body::from(json!({ "name": "x" }).to_string()))
        .unwrap();
    let (status, _b) = send(&h.app, req).await;
    assert_eq!(status.as_u16(), 401);

    let req = Request::builder()
        .method("DELETE")
        .uri("/settings/app-passwords/some-id")
        .body(Body::empty())
        .unwrap();
    let (status, _b) = send(&h.app, req).await;
    assert_eq!(status.as_u16(), 401);
}
