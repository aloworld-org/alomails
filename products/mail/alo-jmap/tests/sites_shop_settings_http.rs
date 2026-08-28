//! The site's shop-settings routes through the real router and Postgres
//! (S3.05b3, ADR 0041): the flat delivery rate the shop-setup approval
//! screen applies a proposed configuration's shipping through.
//!
//! Pinned here: the unset rate answering the zero the public checkout would
//! actually charge, a `PUT` answering the stored row, the store's refusal
//! sentence travelling verbatim, and a foreign tenant's knock reading exactly
//! like a site that never existed.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use crate::common::{Harness, get, harness, harness_on, send};

/// A subdomain unique to this harness run — the namespace is global.
fn sub(tag: &str, h: &Harness) -> String {
    let salt: String = h
        .tenant
        .as_str()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .take(16)
        .collect();
    format!("{tag}{salt}")
}

async fn put(app: &Router, token: Option<&str>, uri: &str, body: Value) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method("PUT")
        .uri(uri)
        .header("content-type", "application/json");
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    send(app, request.body(Body::from(body.to_string())).unwrap()).await
}

#[tokio::test]
async fn the_unset_rate_is_zero_and_a_put_answers_the_stored_row() {
    let h = harness("shopset-roundtrip").await;
    let site = h
        .acc
        .create_site("Shop", &sub("shopset", &h))
        .await
        .unwrap();
    let path = format!("/sites/{site}/shop-settings");

    let (status, answer) = get(&h.app, &h.token, &path).await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(answer["shippingCents"], 0);

    let (status, answer) = put(
        &h.app,
        Some(&h.token),
        &path,
        json!({ "shippingCents": 450 }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(answer["shippingCents"], 450);

    // The answer was the stored row, not an echo: read it back.
    let (status, answer) = get(&h.app, &h.token, &path).await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(answer["shippingCents"], 450);

    // Stated-free is a real rate, not an unset one.
    let (status, answer) = put(&h.app, Some(&h.token), &path, json!({ "shippingCents": 0 })).await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(answer["shippingCents"], 0);
}

#[tokio::test]
async fn a_rate_the_store_refuses_travels_as_its_own_sentence() {
    let h = harness("shopset-refused").await;
    let site = h
        .acc
        .create_site("Shop", &sub("shopsetref", &h))
        .await
        .unwrap();
    let path = format!("/sites/{site}/shop-settings");

    let (status, problem) = put(
        &h.app,
        Some(&h.token),
        &path,
        json!({ "shippingCents": -1 }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{problem}");
    assert!(
        problem["detail"]
            .as_str()
            .unwrap()
            .contains("shipping must be between"),
        "{problem}"
    );

    // A body without the one field has nothing to say: 400, and the rate is
    // untouched.
    let (status, problem) = put(&h.app, Some(&h.token), &path, json!({})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{problem}");
    let (_, answer) = get(&h.app, &h.token, &path).await;
    assert_eq!(answer["shippingCents"], 0);
}

#[tokio::test]
async fn a_foreign_tenant_reads_exactly_like_a_site_that_never_existed() {
    let a = harness("shopset-owner").await;
    let b = harness_on(Arc::clone(&a.store), "shopset-stranger").await;
    let site = a
        .acc
        .create_site("Shop", &sub("shopsetiso", &a))
        .await
        .unwrap();
    a.acc
        .set_site_shop_shipping_cents(&site, 900)
        .await
        .unwrap();
    let path = format!("/sites/{site}/shop-settings");

    // No token at all: 401 before anything resolves.
    let request = Request::builder()
        .method("GET")
        .uri(&path)
        .body(Body::empty())
        .unwrap();
    let (status, _) = send(&a.app, request).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Tenant B, real token, A's site: the same 404 an invented id gets —
    // never the rate, never a hint the site exists.
    let (status, problem) = get(&b.app, &b.token, &path).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{problem}");
    let (status, problem) = put(&b.app, Some(&b.token), &path, json!({ "shippingCents": 1 })).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{problem}");
    let (missing_status, missing) =
        get(&b.app, &b.token, "/sites/no-such-site/shop-settings").await;
    assert_eq!(missing_status, StatusCode::NOT_FOUND);
    assert_eq!(problem, missing);

    // And A's rate is exactly where it was.
    let (_, answer) = get(&a.app, &a.token, &path).await;
    assert_eq!(answer["shippingCents"], 900);
}
