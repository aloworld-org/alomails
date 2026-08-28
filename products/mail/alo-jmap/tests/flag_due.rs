//! Flag due-date over the wire: POST /jmap/flag-due sets a date (and flags the
//! message), Email/get surfaces alo:flagDue, and clearing removes it.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use crate::common::{api, harness, send};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

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

async fn get_email(h: &common::Harness, mid: &str) -> Value {
    let (_s, body) = api(
        &h.app,
        &h.token,
        json!({
            "using": ["urn:ietf:params:jmap:core", "urn:ietf:params:jmap:mail"],
            "methodCalls": [[
                "Email/get",
                { "accountId": h.account_id, "ids": [mid], "properties": ["keywords"] },
                "g"
            ]]
        }),
    )
    .await;
    body["methodResponses"][0][1]["list"][0].clone()
}

#[tokio::test]
async fn set_surface_and_clear_flag_due() {
    let h = harness("flagdue").await;
    let mid = h
        .acc
        .deliver(b"From: a@x\r\nSubject: follow up\r\n\r\nbody\r\n")
        .await
        .unwrap();
    let mid = mid.to_string();

    // Set a due-date → 200; it also flags the message.
    let (status, _b) = post_json(
        &h.app,
        &h.token,
        "/jmap/flag-due",
        json!({ "emailId": mid, "dueAt": 1_798_761_600_i64 }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let email = get_email(&h, &mid).await;
    assert_eq!(email["alo:flagDue"], json!("2027-01-01T00:00:00Z"));
    assert_eq!(
        email["keywords"]["$flagged"],
        json!(true),
        "a due-date flags the message"
    );

    // Clear it → null; the flag itself stays (clearing the flag is separate).
    let (status, _b) = post_json(
        &h.app,
        &h.token,
        "/jmap/flag-due",
        json!({ "emailId": mid, "dueAt": Value::Null }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let email = get_email(&h, &mid).await;
    assert_eq!(email["alo:flagDue"], json!(null));
}

#[tokio::test]
async fn rejects_bad_input() {
    let h = harness("flagdue-bad").await;
    // Non-integer dueAt → 400.
    let (status, _b) = post_json(
        &h.app,
        &h.token,
        "/jmap/flag-due",
        json!({ "emailId": "x", "dueAt": "soon" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    // Unknown message → 404.
    let (status, _b) = post_json(
        &h.app,
        &h.token,
        "/jmap/flag-due",
        json!({ "emailId": "nope", "dueAt": 1_798_761_600_i64 }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
