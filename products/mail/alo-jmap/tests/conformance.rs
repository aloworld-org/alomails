//! JMAP conformance: session, auth, the core method envelope, result
//! references, flags, and /changes. Against real Postgres.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common::{api, call, harness, send};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Map, Value, json};

fn obj(pairs: Vec<(String, Value)>) -> Value {
    Value::Object(pairs.into_iter().collect::<Map<_, _>>())
}

#[tokio::test]
async fn session_lists_account_capabilities_and_limits() {
    let h = harness("session").await;
    let req = Request::builder()
        .method("GET")
        .uri("/.well-known/jmap")
        .header("authorization", format!("Bearer {}", h.token))
        .body(Body::empty())
        .unwrap();
    let (status, body) = send(&h.app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["capabilities"]["urn:ietf:params:jmap:core"].is_object());
    assert!(body["capabilities"]["urn:ietf:params:jmap:mail"].is_object());
    assert!(body["accounts"][h.account_id.as_str()].is_object());
    assert_eq!(
        body["primaryAccounts"]["urn:ietf:params:jmap:mail"],
        json!(h.account_id)
    );
    assert_eq!(
        body["capabilities"]["urn:ietf:params:jmap:core"]["maxCallsInRequest"],
        json!(32)
    );
    assert!(body["apiUrl"].as_str().unwrap().ends_with("/jmap/api"));
}

#[tokio::test]
async fn missing_token_is_401() {
    let h = harness("noauth").await;
    let req = Request::builder()
        .method("GET")
        .uri("/.well-known/jmap")
        .body(Body::empty())
        .unwrap();
    let (status, _) = send(&h.app, req).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn token_endpoint_issues_and_rejects() {
    let h = harness("tok").await;
    let good = Request::builder()
        .method("POST")
        .uri("/auth/token")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({"username": h.email, "password": "s3cret-pw"}).to_string(),
        ))
        .unwrap();
    let (status, body) = send(&h.app, good).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["token"].is_string());
    assert_eq!(body["accountId"], json!(h.account_id));

    let bad = Request::builder()
        .method("POST")
        .uri("/auth/token")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({"username": h.email, "password": "WRONG"}).to_string(),
        ))
        .unwrap();
    let (status, _) = send(&h.app, bad).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn malformed_json_is_not_json_problem() {
    let h = harness("badjson").await;
    let req = Request::builder()
        .method("POST")
        .uri("/jmap/api")
        .header("authorization", format!("Bearer {}", h.token))
        .header("content-type", "application/json")
        .body(Body::from("{ not json "))
        .unwrap();
    let (status, body) = send(&h.app, req).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["type"], json!("urn:ietf:params:jmap:error:notJSON"));
}

#[tokio::test]
async fn unknown_capability_is_rejected() {
    let h = harness("cap").await;
    let body = json!({ "using": ["urn:example:bogus"], "methodCalls": [] });
    let (status, resp) = api(&h.app, &h.token, body).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        resp["type"],
        json!("urn:ietf:params:jmap:error:unknownCapability")
    );
}

#[tokio::test]
async fn unknown_method_is_error_invocation() {
    let h = harness("unknown").await;
    let (status, body) = api(
        &h.app,
        &h.token,
        call("Frobnicate/get", json!({"accountId": h.account_id})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let inv = &body["methodResponses"][0];
    assert_eq!(inv[0], json!("error"));
    assert_eq!(inv[1]["type"], json!("unknownMethod"));
    assert_eq!(inv[2], json!("c0"));
}

#[tokio::test]
async fn wrong_account_id_is_account_not_found() {
    let h = harness("acct").await;
    let (_status, body) = api(
        &h.app,
        &h.token,
        call(
            "Mailbox/get",
            json!({"accountId": "someone-else", "ids": Value::Null}),
        ),
    )
    .await;
    assert_eq!(body["methodResponses"][0][0], json!("error"));
    assert_eq!(
        body["methodResponses"][0][1]["type"],
        json!("accountNotFound")
    );
}

#[tokio::test]
async fn mailbox_get_all_shows_inbox_with_counters() {
    let h = harness("mbget").await;
    h.acc
        .deliver(b"From: a@x\r\nSubject: hi\r\n\r\nbody\r\n")
        .await
        .unwrap();
    let (status, body) = api(
        &h.app,
        &h.token,
        call(
            "Mailbox/get",
            json!({"accountId": h.account_id, "ids": Value::Null}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let resp = &body["methodResponses"][0];
    assert_eq!(resp[0], json!("Mailbox/get"));
    let list = resp[1]["list"].as_array().unwrap();
    assert!(list.iter().any(|m| m["role"] == json!("inbox")));
    assert!(list.iter().any(|m| m["unreadEmails"] == json!(1)));
}

#[tokio::test]
async fn query_then_get_via_result_reference() {
    let h = harness("qref").await;
    h.acc
        .deliver(b"From: alice@x\r\nSubject: Quarterly numbers\r\n\r\nhello body text\r\n")
        .await
        .unwrap();
    let request = json!({
        "using": ["urn:ietf:params:jmap:core", "urn:ietf:params:jmap:mail"],
        "methodCalls": [
            ["Email/query", {"accountId": h.account_id, "filter": {"text": "quarterly"}}, "q"],
            ["Email/get", {
                "accountId": h.account_id,
                "#ids": {"resultOf": "q", "name": "Email/query", "path": "/ids"},
                "fetchTextBodyValues": true
            }, "g"]
        ]
    });
    let (status, body) = api(&h.app, &h.token, request).await;
    assert_eq!(status, StatusCode::OK);
    let get = &body["methodResponses"][1];
    assert_eq!(get[0], json!("Email/get"));
    let list = get[1]["list"].as_array().unwrap();
    assert_eq!(list.len(), 1, "the referenced query id was fetched");
    assert_eq!(list[0]["subject"], json!("Quarterly numbers"));
    assert!(
        list[0]["bodyValues"]["text"]["value"]
            .as_str()
            .unwrap()
            .contains("hello body text")
    );
}

#[tokio::test]
async fn set_seen_updates_counter_and_shows_in_changes() {
    let h = harness("flag").await;
    let mid = h
        .acc
        .deliver(b"From: a@x\r\nSubject: s\r\n\r\nb\r\n")
        .await
        .unwrap();
    let state0 = h.acc.state().await.unwrap();

    // Email/set: mark $seen via a patch.
    let update = obj(vec![(mid.to_string(), json!({ "keywords/$seen": true }))]);
    let (status, body) = api(
        &h.app,
        &h.token,
        call(
            "Email/set",
            json!({"accountId": h.account_id, "update": update}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let setr = &body["methodResponses"][0][1];
    assert!(
        setr["updated"].get(mid.to_string()).is_some(),
        "id in updated"
    );
    assert_ne!(setr["newState"], setr["oldState"]);

    // Email/changes since state0 must list the message as updated.
    let (status, body) = api(
        &h.app,
        &h.token,
        call(
            "Email/changes",
            json!({"accountId": h.account_id, "sinceState": state0}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let ch = &body["methodResponses"][0][1];
    let updated = ch["updated"].as_array().unwrap();
    assert!(updated.iter().any(|v| v == &json!(mid.to_string())));

    // The counter dropped to 0 unread.
    let (_s, body) = api(
        &h.app,
        &h.token,
        call(
            "Mailbox/get",
            json!({"accountId": h.account_id, "ids": Value::Null}),
        ),
    )
    .await;
    let list = body["methodResponses"][0][1]["list"].as_array().unwrap();
    assert!(list.iter().any(|m| m["unreadEmails"] == json!(0)));
}

#[tokio::test]
async fn oversized_request_body_is_rejected_before_parse() {
    let h = harness("oversize").await;
    // 11 MiB > maxSizeRequestObject (10 MiB); bounded before parse.
    let big = vec![b'x'; 11 * 1024 * 1024];
    let req = Request::builder()
        .method("POST")
        .uri("/jmap/api")
        .header("authorization", format!("Bearer {}", h.token))
        .header("content-type", "application/json")
        .body(Body::from(big))
        .unwrap();
    let (status, _body) = send(&h.app, req).await;
    // The per-route body limit rejects it before buffering the whole body.
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn email_query_limit_is_enforced() {
    let h = harness("page").await;
    for i in 0..5 {
        h.acc
            .deliver(format!("From: a@x\r\nSubject: m{i}\r\n\r\nbody\r\n").as_bytes())
            .await
            .unwrap();
    }
    let (status, body) = api(
        &h.app,
        &h.token,
        call(
            "Email/query",
            json!({"accountId": h.account_id, "limit": 2}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["methodResponses"][0][1]["ids"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn concurrent_sets_converge() {
    // Two clients flag/mark the same message concurrently; both apply and
    // the tenant state converges (the modseq serializes them).
    let h = harness("converge").await;
    let mid = h
        .acc
        .deliver(b"From: a@x\r\nSubject: s\r\n\r\nb\r\n")
        .await
        .unwrap();
    let state0 = h.acc.state().await.unwrap();

    let seen = obj(vec![(mid.to_string(), json!({ "keywords/$seen": true }))]);
    let flagged = obj(vec![(
        mid.to_string(),
        json!({ "keywords/$flagged": true }),
    )]);
    let app1 = h.app.clone();
    let app2 = h.app.clone();
    let (t1, t2) = (h.token.clone(), h.token.clone());
    let id = h.account_id.clone();
    let (id1, id2) = (id.clone(), id);
    let (r1, r2) = tokio::join!(
        async move {
            api(
                &app1,
                &t1,
                call("Email/set", json!({"accountId": id1, "update": seen})),
            )
            .await
        },
        async move {
            api(
                &app2,
                &t2,
                call("Email/set", json!({"accountId": id2, "update": flagged})),
            )
            .await
        },
    );
    assert_eq!(r1.0, StatusCode::OK);
    assert_eq!(r2.0, StatusCode::OK);

    // Both keywords are present, and /changes from state0 shows the update.
    let kws = h.acc.keywords(&mid).await.unwrap();
    assert!(kws.contains(&"$seen".to_owned()) && kws.contains(&"$flagged".to_owned()));
    let (_s, body) = api(
        &h.app,
        &h.token,
        call(
            "Email/changes",
            json!({"accountId": h.account_id, "sinceState": state0}),
        ),
    )
    .await;
    let updated = body["methodResponses"][0][1]["updated"].as_array().unwrap();
    assert!(updated.iter().any(|v| v == &json!(mid.to_string())));
}

#[tokio::test]
async fn changes_with_garbage_state_is_cannot_calculate() {
    let h = harness("badstate").await;
    let (_s, body) = api(
        &h.app,
        &h.token,
        call(
            "Email/changes",
            json!({"accountId": h.account_id, "sinceState": "not-a-number"}),
        ),
    )
    .await;
    assert_eq!(body["methodResponses"][0][0], json!("error"));
    assert_eq!(
        body["methodResponses"][0][1]["type"],
        json!("cannotCalculateChanges")
    );
}
