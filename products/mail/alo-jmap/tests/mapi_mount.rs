//! The MAPI-over-HTTP endpoints are mounted **only** when the deployment serves
//! them (ADR 0051).
//!
//! This is the wiring assertion, and it matters in both directions. Mounted
//! when it should not be, an unfinished stage is reachable by anyone who can
//! authenticate. Absent when it should be there, Autodiscover points Outlook at
//! a URL that answers with the single-page app — which a mail client cannot
//! make sense of, and which it reports as a server that is simply broken.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use common::harness;
use tower::ServiceExt;

/// Sends a bare POST to a MAPI path through whichever router was built.
async fn probe(app: &axum::Router, path: &str) -> StatusCode {
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(path)
                .header("X-RequestType", "Connect")
                .header("X-RequestId", "probe")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
        .status()
}

/// The default: the endpoints do not exist at all.
///
/// Absent rather than present-and-refusing. A path that is not routed cannot be
/// probed for what it would have done, and a protocol stage that is not
/// finished should not be reachable by accident.
#[tokio::test]
async fn mapi_paths_do_not_exist_until_the_deployment_serves_them() {
    let h = harness("mapi-off").await;
    let state = alo_jmap::server::app_state(
        std::sync::Arc::clone(&h.store),
        h.identity.clone(),
        "https://mail.test",
    );
    assert!(!state.mapi_http, "the default must be off");
    let app = alo_jmap::server::app(state);

    for path in ["/mapi/emsmdb", "/mapi/emsmdb/", "/mapi/nspi"] {
        assert_eq!(
            probe(&app, path).await,
            StatusCode::NOT_FOUND,
            "{path} was reachable with the adapter off"
        );
    }
}

/// Switched on, the endpoints answer — and answer as MAPI, not as the app.
///
/// An unauthenticated `Connect` must reach the challenge, which is proof the
/// request got to the MAPI handler rather than to a catch-all.
#[tokio::test]
async fn mapi_paths_answer_when_the_deployment_serves_them() {
    let h = harness("mapi-on").await;
    let mut state = alo_jmap::server::app_state(
        std::sync::Arc::clone(&h.store),
        h.identity.clone(),
        "https://mail.test",
    );
    state.mapi_http = true;
    let app = alo_jmap::server::app(state);

    for path in ["/mapi/emsmdb", "/mapi/emsmdb/"] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(path)
                    .header("X-RequestType", "Connect")
                    .header("X-RequestId", "probe")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        // 401 with a challenge: the MAPI handler ran and asked for credentials.
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{path}");
        assert_eq!(
            response
                .headers()
                .get(axum::http::header::WWW_AUTHENTICATE)
                .unwrap(),
            r#"Basic realm="alo""#,
            "{path} answered, but not as MAPI"
        );
    }

    // The address book endpoint is a later stage and refuses by code rather
    // than vanishing, so Autodiscover can name it without stranding a client.
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/mapi/nspi")
                .header("X-RequestType", "Bind")
                .header("X-RequestId", "probe")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get("X-ResponseCode").unwrap(),
        // 16 — Endpoint Disabled.
        "16"
    );
}

/// `/mapi` is a protocol path, so it is **not** also mounted under `/api`.
///
/// The URL in the Autodiscover document is the contract: a client told
/// `/mapi/emsmdb/` will never look anywhere else, and a second mount would be
/// a second address for the same thing that nothing has been told about.
#[tokio::test]
async fn mapi_is_not_duplicated_under_the_api_prefix() {
    let h = harness("mapi-api").await;
    let mut state = alo_jmap::server::app_state(
        std::sync::Arc::clone(&h.store),
        h.identity.clone(),
        "https://mail.test",
    );
    state.mapi_http = true;
    let app = alo_jmap::server::app(state);

    assert_eq!(
        probe(&app, "/api/mapi/emsmdb").await,
        StatusCode::NOT_FOUND,
        "MAPI was mounted twice"
    );
}
