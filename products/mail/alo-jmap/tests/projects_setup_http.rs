//! Real-router proof for reviewed, retry-safe project setup.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use crate::common::{Harness, harness, send};

async fn request(
    app: &Router,
    token: &str,
    method: &str,
    uri: &str,
    body: Value,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    send(app, request).await
}

async fn project(h: &Harness, name: &str) -> String {
    let (status, body) = request(
        &h.app,
        &h.token,
        "POST",
        "/projects",
        json!({ "name": name }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["id"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn confirmation_creates_every_selected_resource_once_and_reads_it_back() {
    let h = harness("project-setup-http").await;
    let project = project(&h, "Premium rollout").await;
    let uri = format!("/projects/{project}/setup");
    let plan = json!({
        "createFilesSpace": true,
        "createChatRoom": true,
        "kickoff": {
            "startsAt": "2026-09-03T08:00:00Z",
            "endsAt": "2026-09-03T09:00:00Z",
            "timezone": "Europe/Berlin"
        },
        "starterTasks": ["Confirm scope", "Prepare kickoff"]
    });

    let (status, first) = request(&h.app, &h.token, "POST", &uri, plan.clone()).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    for field in ["spaceId", "chatChannelId", "kickoffEventId"] {
        assert!(first["setup"][field].is_string(), "{field}: {first}");
    }
    assert_eq!(
        first["setup"]["starterTaskIds"].as_array().map(Vec::len),
        Some(2)
    );

    let (status, retry) = request(&h.app, &h.token, "POST", &uri, plan).await;
    assert_eq!(status, StatusCode::OK, "{retry}");
    assert_eq!(retry["setup"], first["setup"]);

    let (status, read) = request(&h.app, &h.token, "GET", &uri, json!({})).await;
    assert_eq!(status, StatusCode::OK, "{read}");
    assert_eq!(read["setup"], first["setup"]);
}

#[tokio::test]
async fn another_tenants_project_is_not_discoverable_or_mutable() {
    let owner = harness("project-setup-owner").await;
    let neighbour = harness("project-setup-neighbour").await;
    let project = project(&owner, "Private delivery").await;
    let uri = format!("/projects/{project}/setup");

    for method in ["GET", "POST"] {
        let (status, body) = request(
            &neighbour.app,
            &neighbour.token,
            method,
            &uri,
            json!({ "createFilesSpace": true }),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method}: {body}");
    }
}

#[tokio::test]
async fn empty_confirmation_is_rejected_without_creating_setup() {
    let h = harness("project-setup-empty").await;
    let project = project(&h, "Unconfigured delivery").await;
    let uri = format!("/projects/{project}/setup");

    let (status, body) = request(&h.app, &h.token, "POST", &uri, json!({})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");

    let (status, body) = request(&h.app, &h.token, "GET", &uri, json!({})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["setup"].is_null(), "{body}");
}
