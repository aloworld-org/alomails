//! One-click unsubscribe over the wire: a message carrying List-Unsubscribe
//! surfaces `alo:listUnsubscribe` on the full email, and the /jmap/unsubscribe
//! route refuses anything that isn't a real RFC 8058 one-click (no header, or
//! mailto-only) — the actual outbound POST is exercised by the egress unit
//! tests, not here (no external server in the harness).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common::{api, harness, send};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

/// POST a JSON body to a route with the harness's bearer token.
async fn post_json(app: &axum::Router, token: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("POST")
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    send(app, req).await
}

#[tokio::test]
async fn list_unsubscribe_surfaces_on_the_full_email() {
    let h = harness("unsub-get").await;
    let mid = h
        .acc
        .deliver(
            b"From: news@shop.example\r\nSubject: Weekly deals\r\n\
              List-Unsubscribe: <https://shop.example/u?id=7>, <mailto:stop@shop.example>\r\n\
              List-Unsubscribe-Post: List-Unsubscribe=One-Click\r\n\r\nDeals inside\r\n",
        )
        .await
        .unwrap();

    // Full fetch (body requested) → the parsed options appear.
    let (status, body) = api(
        &h.app,
        &h.token,
        json!({
            "using": ["urn:ietf:params:jmap:core", "urn:ietf:params:jmap:mail"],
            "methodCalls": [[
                "Email/get",
                { "accountId": h.account_id, "ids": [mid.to_string()],
                  "fetchTextBodyValues": true },
                "g"
            ]]
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let u = &body["methodResponses"][0][1]["list"][0]["alo:listUnsubscribe"];
    assert_eq!(u["http"], json!("https://shop.example/u?id=7"));
    assert_eq!(u["mailto"], json!("mailto:stop@shop.example"));
    assert_eq!(u["oneClick"], json!(true));
}

#[tokio::test]
async fn header_only_fetch_omits_unsubscribe() {
    // The list view (no body) must not carry the field — it's full-email only.
    let h = harness("unsub-hdr").await;
    let mid = h
        .acc
        .deliver(b"From: n@shop.example\r\nSubject: x\r\nList-Unsubscribe: <mailto:s@shop.example>\r\n\r\nb\r\n")
        .await
        .unwrap();
    let (_s, body) = api(
        &h.app,
        &h.token,
        json!({
            "using": ["urn:ietf:params:jmap:core", "urn:ietf:params:jmap:mail"],
            "methodCalls": [[
                "Email/get",
                { "accountId": h.account_id, "ids": [mid.to_string()], "properties": ["subject"] },
                "g"
            ]]
        }),
    )
    .await;
    assert!(
        body["methodResponses"][0][1]["list"][0]
            .get("alo:listUnsubscribe")
            .is_none()
    );
}

#[tokio::test]
async fn route_refuses_messages_without_one_click() {
    let h = harness("unsub-post").await;

    // A message with only a mailto: unsubscribe — no one-click → 400.
    let mailto_only = h
        .acc
        .deliver(b"From: n@shop.example\r\nSubject: x\r\nList-Unsubscribe: <mailto:s@shop.example>\r\n\r\nb\r\n")
        .await
        .unwrap();
    let (status, _b) = post_json(
        &h.app,
        &h.token,
        "/jmap/unsubscribe",
        json!({ "emailId": mailto_only.to_string() }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "mailto-only is not one-click"
    );

    // A message with no List-Unsubscribe at all → 400.
    let plain = h
        .acc
        .deliver(b"From: a@b\r\nSubject: hi\r\n\r\nbody\r\n")
        .await
        .unwrap();
    let (status, _b) = post_json(
        &h.app,
        &h.token,
        "/jmap/unsubscribe",
        json!({ "emailId": plain.to_string() }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // An unknown message id → 404.
    let (status, _b) = post_json(
        &h.app,
        &h.token,
        "/jmap/unsubscribe",
        json!({ "emailId": "nope" }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
