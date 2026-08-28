//! `GET`/`POST /settings/locale` — the server-synced interface language
//! (mail M4.2). The client reads it at sign-in and writes it when the
//! switcher changes, so the same person signs in speaking the same language
//! on every device.
//!
//! What matters on the wire: `null` and "never chosen" are the same thing
//! (browser detection stays the fallback), a malformed tag is refused rather
//! than stored, and the preference is scoped to the token's `(tenant, user)`
//! — one person's choice of German must never restyle a colleague, let alone
//! another tenant.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use std::sync::Arc;

use crate::common::{get, harness, harness_on, post_raw, send};
use axum::body::Body;
use axum::http::Request;
use serde_json::{Value, json};

#[tokio::test]
async fn the_choice_round_trips_and_null_clears_it() {
    let h = harness("locale-roundtrip").await;

    // Never chosen reads as null, not as an error and not as English — the
    // client's browser detection is the fallback and must stay in charge.
    let (status, body) = get(&h.app, &h.token, "/settings/locale").await;
    assert!(status.is_success(), "{status} {body}");
    assert_eq!(body["locale"], Value::Null, "{body}");

    let (status, _b) = post_raw(
        &h.app,
        &h.token,
        "/settings/locale",
        &json!({ "locale": "de" }).to_string(),
    )
    .await;
    assert!(status.is_success(), "{status}");
    let (_s, body) = get(&h.app, &h.token, "/settings/locale").await;
    assert_eq!(body["locale"], json!("de"), "{body}");

    // The last write wins — switching again on another device overwrites.
    let (status, _b) = post_raw(
        &h.app,
        &h.token,
        "/settings/locale",
        &json!({ "locale": "fr" }).to_string(),
    )
    .await;
    assert!(status.is_success());
    let (_s, body) = get(&h.app, &h.token, "/settings/locale").await;
    assert_eq!(body["locale"], json!("fr"), "{body}");

    // Null clears the choice back to browser detection.
    let (status, _b) = post_raw(
        &h.app,
        &h.token,
        "/settings/locale",
        &json!({ "locale": null }).to_string(),
    )
    .await;
    assert!(status.is_success());
    let (_s, body) = get(&h.app, &h.token, "/settings/locale").await;
    assert_eq!(body["locale"], Value::Null, "{body}");
}

#[tokio::test]
async fn a_regioned_tag_is_accepted_as_written() {
    // The server checks shape, not the shipped catalog: "pt-BR" must be
    // storable today so adding the language later needs no server release.
    let h = harness("locale-region").await;

    let (status, _b) = post_raw(
        &h.app,
        &h.token,
        "/settings/locale",
        &json!({ "locale": "pt-BR" }).to_string(),
    )
    .await;
    assert!(status.is_success(), "{status}");
    let (_s, body) = get(&h.app, &h.token, "/settings/locale").await;
    assert_eq!(body["locale"], json!("pt-BR"), "stored as written: {body}");
}

#[tokio::test]
async fn a_malformed_tag_is_refused_and_nothing_is_stored() {
    let h = harness("locale-garbage").await;

    let (_s, _b) = post_raw(
        &h.app,
        &h.token,
        "/settings/locale",
        &json!({ "locale": "de" }).to_string(),
    )
    .await;

    for bad in [
        json!({ "locale": "x" }),            // too short to be a language
        json!({ "locale": "-de" }),          // empty primary subtag
        json!({ "locale": "de-" }),          // empty trailing subtag
        json!({ "locale": "de_DE" }),        // underscore is POSIX, not BCP 47
        json!({ "locale": "de<script>" }),   // anything non-alphanumeric
        json!({ "locale": "a".repeat(36) }), // longer than a tag can be
        json!({ "locale": 7 }),              // not a string at all
    ] {
        let (status, body) = post_raw(&h.app, &h.token, "/settings/locale", &bad.to_string()).await;
        assert_eq!(status.as_u16(), 422, "refused {bad}: {status} {body}");
    }

    // Every refusal left the stored choice exactly as it was.
    let (_s, body) = get(&h.app, &h.token, "/settings/locale").await;
    assert_eq!(body["locale"], json!("de"), "{body}");
}

#[tokio::test]
async fn one_persons_choice_is_invisible_to_another_tenant() {
    // Two tenants on ONE store handle, the way production runs — the only
    // arrangement in which a cross-tenant read could even be attempted.
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
    let a = harness_on(Arc::clone(&store), "locale-tenant-a").await;
    let b = harness_on(Arc::clone(&store), "locale-tenant-b").await;

    let (status, _b) = post_raw(
        &a.app,
        &a.token,
        "/settings/locale",
        &json!({ "locale": "de" }).to_string(),
    )
    .await;
    assert!(status.is_success());

    // B still reads "never chosen" — and writing their own does not bleed back.
    let (_s, body) = get(&b.app, &b.token, "/settings/locale").await;
    assert_eq!(
        body["locale"],
        Value::Null,
        "A's choice leaked to B: {body}"
    );

    let (status, _b2) = post_raw(
        &b.app,
        &b.token,
        "/settings/locale",
        &json!({ "locale": "nl" }).to_string(),
    )
    .await;
    assert!(status.is_success());
    let (_s, body) = get(&a.app, &a.token, "/settings/locale").await;
    assert_eq!(
        body["locale"],
        json!("de"),
        "B's write reached A's row: {body}"
    );
}

#[tokio::test]
async fn a_colleague_in_the_same_tenant_keeps_their_own_language() {
    // The row is keyed (tenant, user): the tenant boundary alone would not
    // notice a query that forgot to bind the user, but the colleague would —
    // their whole interface flips language.
    let h = harness("locale-colleague").await;
    let email = format!("colleague-{}@example.test", h.tenant);
    let colleague = h.ts.create_user(&email).await.unwrap();
    h.identity
        .set_password(&h.tenant, &colleague, &email, "s3cret-pw")
        .await
        .unwrap();
    let colleague_token = h
        .identity
        .password_login(&email, "s3cret-pw", None)
        .await
        .unwrap()
        .expect("token issued")
        .0
        .reveal()
        .to_owned();

    let (status, _b) = post_raw(
        &h.app,
        &h.token,
        "/settings/locale",
        &json!({ "locale": "de" }).to_string(),
    )
    .await;
    assert!(status.is_success());

    let (_s, body) = get(&h.app, &colleague_token, "/settings/locale").await;
    assert_eq!(
        body["locale"],
        Value::Null,
        "one user's choice reached a colleague: {body}"
    );
}

#[tokio::test]
async fn both_routes_require_a_token() {
    let h = harness("locale-unauth").await;

    let req = Request::builder()
        .method("GET")
        .uri("/settings/locale")
        .body(Body::empty())
        .unwrap();
    let (status, _b) = send(&h.app, req).await;
    assert_eq!(status.as_u16(), 401);

    let req = Request::builder()
        .method("POST")
        .uri("/settings/locale")
        .header("content-type", "application/json")
        .body(Body::from(json!({ "locale": "de" }).to_string()))
        .unwrap();
    let (status, _b) = send(&h.app, req).await;
    assert_eq!(status.as_u16(), 401);
}
