//! The `/sites/{id}/pages/{pid}/password` surface (ADR 0036, S2.06a), driven
//! through the real router over a real Postgres.
//!
//! `alo-store`'s own suite proves the model (hashing, rotation, tenant scope,
//! what survives a deleted page) and `alo-sites` proves the gate. What this
//! pins is the edge the protect/remove screen will speak to: the auth guard,
//! the exact JSON it reads, the refusals it must show verbatim, and the one
//! rule this surface adds — a password goes in and never comes back out.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use crate::common::{Harness, harness, harness_on, send};

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

async fn put(app: &Router, token: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    send(app, with_json("PUT", uri, Some(token), body)).await
}

async fn delete(app: &Router, token: &str, uri: &str) -> (StatusCode, Value) {
    send(app, with_json("DELETE", uri, Some(token), Value::Null)).await
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

/// A subdomain unique to this harness run — the namespace is global.
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

/// A site with a home page and a second page, returning both ids.
async fn site_with_pages(h: &Harness, tag: &str) -> (String, String, String) {
    let (status, site) = send(
        &h.app,
        with_json(
            "POST",
            "/sites",
            Some(&h.token),
            json!({ "name": "Roastery", "subdomain": sub(tag, h) }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "create site: {site}");
    let site_id = site["id"].as_str().unwrap().to_owned();
    let mut pages = Vec::new();
    for (title, slug, home) in [("Home", "", true), ("Prices", "prices", false)] {
        let (status, page) = send(
            &h.app,
            with_json(
                "POST",
                &format!("/sites/{site_id}/pages"),
                Some(&h.token),
                json!({ "title": title, "slug": slug, "home": home }),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "create page: {page}");
        pages.push(page["id"].as_str().unwrap().to_owned());
    }
    (site_id, pages[0].clone(), pages[1].clone())
}

#[tokio::test]
async fn password_routes_require_a_bearer_token() {
    let h = harness("site-password-401").await;
    let attempts = [
        ("GET", "/sites/some-id/passwords", None),
        ("GET", "/sites/some-id/pages/some-page/password", None),
        (
            "PUT",
            "/sites/some-id/pages/some-page/password",
            Some(json!({ "password": "a good password" })),
        ),
        ("DELETE", "/sites/some-id/pages/some-page/password", None),
    ];
    for (method, uri, body) in attempts {
        let (status, problem) = send(
            &h.app,
            with_json(method, uri, None, body.unwrap_or(Value::Null)),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "{method} {uri}: {problem}"
        );
    }
}

/// The arc the protect/remove screen needs: a page that carries no password,
/// one that does, a changed password, and a lifted one — plus the refusals.
#[tokio::test]
async fn a_page_is_protected_changed_and_freed_again() {
    let h = harness("site-password").await;
    let (site, home, prices) = site_with_pages(&h, "pw").await;

    // Nothing protected is an answer, not a failure.
    let (status, body) = get(&h.app, &h.token, &format!("/sites/{site}/passwords")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["pages"], json!([]));
    let (status, body) = get(
        &h.app,
        &h.token,
        &format!("/sites/{site}/pages/{prices}/password"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "protected": false }));

    // The rules come back in the store's own words, for the screen to show.
    let (status, problem) = put(
        &h.app,
        &h.token,
        &format!("/sites/{site}/pages/{prices}/password"),
        json!({ "password": "short" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        problem["detail"],
        json!("a page password must be at least 8 characters")
    );
    let (status, problem) = put(
        &h.app,
        &h.token,
        &format!("/sites/{site}/pages/{prices}/password"),
        json!({ "secret": "a good password" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        problem["detail"]
            .as_str()
            .unwrap()
            .contains("password must"),
        "the refusal names the shape it wants: {problem}"
    );
    assert!(
        !problem.to_string().contains("a good password"),
        "a refusal never echoes what was sent: {problem}"
    );

    // Protect it.
    let (status, body) = put(
        &h.app,
        &h.token,
        &format!("/sites/{site}/pages/{prices}/password"),
        json!({ "password": "kaneelstokjes 2026" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["protected"], json!(true));
    assert_eq!(body["pageId"], json!(prices));
    let created = body["createdAt"].as_str().unwrap().to_owned();
    assert!(
        !body.to_string().contains("kaneelstokjes"),
        "the password is never in an answer: {body}"
    );

    // One read tells the page list which pages carry a password.
    let (status, body) = get(&h.app, &h.token, &format!("/sites/{site}/passwords")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["pages"].as_array().unwrap().len(), 1);
    assert_eq!(body["pages"][0]["pageId"], json!(prices));
    let (status, body) = get(
        &h.app,
        &h.token,
        &format!("/sites/{site}/pages/{home}/password"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "protected": false }), "only that one page");

    // Changing it keeps the protection and moves only the moment.
    let (status, changed) = put(
        &h.app,
        &h.token,
        &format!("/sites/{site}/pages/{prices}/password"),
        json!({ "password": "een heel ander wachtwoord" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(changed["createdAt"], json!(created));
    assert_eq!(changed["protected"], json!(true));

    // Lifting it is idempotent — the second call states the same world.
    let (status, body) = delete(
        &h.app,
        &h.token,
        &format!("/sites/{site}/pages/{prices}/password"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "protected": false }));
    let (status, body) = delete(
        &h.app,
        &h.token,
        &format!("/sites/{site}/pages/{prices}/password"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "protected": false }));
    let (status, body) = get(&h.app, &h.token, &format!("/sites/{site}/passwords")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["pages"], json!([]));

    // An unknown page of a site the caller owns is a clean not-found on write
    // and simply unprotected on read.
    let (status, problem) = put(
        &h.app,
        &h.token,
        &format!("/sites/{site}/pages/invented-page/password"),
        json!({ "password": "kaneelstokjes 2026" }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{problem}");
    let (status, body) = get(
        &h.app,
        &h.token,
        &format!("/sites/{site}/pages/invented-page/password"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "protected": false }));
}

/// Another tenant's page cannot be protected, read, changed, or freed — every
/// verb answers exactly like an id that never existed, and their protection is
/// untouched by everything that was tried.
#[tokio::test]
async fn another_tenants_page_password_is_out_of_reach() {
    let (first, store) = {
        let h = harness("site-password-a").await;
        let store = Arc::clone(&h.store);
        (h, store)
    };
    let second = harness_on(store, "site-password-b").await;
    let (their_site, _their_home, their_page) = site_with_pages(&second, "pwb").await;
    let (our_site, _our_home, our_page) = site_with_pages(&first, "pwa").await;
    let (status, _) = put(
        &second.app,
        &second.token,
        &format!("/sites/{their_site}/pages/{their_page}/password"),
        json!({ "password": "their own password" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let uri = format!("/sites/{their_site}/pages/{their_page}/password");
    let (status, problem) = get(&first.app, &first.token, &uri).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{problem}");
    let (status, problem) = get(
        &first.app,
        &first.token,
        &format!("/sites/{their_site}/passwords"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{problem}");
    let (status, problem) = put(
        &first.app,
        &first.token,
        &uri,
        json!({ "password": "our password now" }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{problem}");
    let (status, problem) = delete(&first.app, &first.token, &uri).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{problem}");
    // Naming their page under our own site does not smuggle it across either.
    let (status, problem) = put(
        &first.app,
        &first.token,
        &format!("/sites/{our_site}/pages/{their_page}/password"),
        json!({ "password": "our password now" }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{problem}");
    let (status, body) = delete(
        &first.app,
        &first.token,
        &format!("/sites/{our_site}/pages/{their_page}/password"),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "lifting a password we never set: {body}"
    );

    // Theirs is exactly as they left it.
    let (status, body) = get(
        &second.app,
        &second.token,
        &format!("/sites/{their_site}/passwords"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["pages"].as_array().unwrap().len(), 1);
    assert_eq!(body["pages"][0]["pageId"], json!(their_page));
    // And ours never became protected by any of it.
    let (status, body) = get(
        &first.app,
        &first.token,
        &format!("/sites/{our_site}/pages/{our_page}/password"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "protected": false }));
}
