//! The MAPI-over-HTTP handshake, driven through the real router against a live
//! store: `Connect` establishes a Session Context and `Disconnect` ends it, an
//! unauthenticated caller gets a challenge and nothing else, and **one tenant
//! cannot end another tenant's session**.
//!
//! Only the network is absent — the router, the credential door, the session
//! store and the binary codecs are all the real ones. A test against stubs
//! could not tell whether Outlook would get a usable answer.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use alo_identity::{Identity, IdentityConfig};
use alo_mapi::response::ResponseCode;
use alo_mapi::session::{SESSION_COOKIE, cookie_value};
use alo_mapi::{MapiState, SessionStore, router};
use alo_store::{BlobStore, Store, TenantId};
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;

fn database_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://alo:alo-dev-only@127.0.0.1:5433/alo".to_owned())
}

/// A tenant with one user who has a password, and the router in front of them.
struct Harness {
    app: Router,
    tenant: TenantId,
    email: String,
    password: String,
}

async fn harness(tag: &str) -> Harness {
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url())
        .await
        .expect("test database");
    let store = Arc::new(Store::new(pool, BlobStore::in_memory(8 * 1024 * 1024)));
    let identity = Identity::new(Arc::clone(&store), IdentityConfig::new("https://id.test"))
        .expect("identity");

    let (tenant, email, password) = seed(&store, &identity, tag).await;
    let app = router(MapiState {
        identity,
        sessions: Arc::new(SessionStore::new()),
        dn_prefix: "/o=alo".to_owned(),
    });
    Harness {
        app,
        tenant,
        email,
        password,
    }
}

/// Creates a tenant with one credentialed user. Returns the tenant, the login
/// name and the password.
async fn seed(store: &Arc<Store>, identity: &Identity, tag: &str) -> (TenantId, String, String) {
    // A unique tenant per test: the shared test database means a fixed address
    // would collide with a parallel run rather than with a real conflict.
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let tenant = store
        .create_tenant(&format!("mapi-{tag}-{stamp}"))
        .await
        .expect("tenant");
    let email = format!("{tag}{stamp}@mapi.test");
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&email)
        .await
        .expect("user");
    identity
        .set_password(&tenant, &user, &email, "correct-horse-battery")
        .await
        .expect("password");
    (tenant, email, "correct-horse-battery".to_owned())
}

/// A `Connect` request body laid out as [MS-OXCMAPIHTTP] §2.2.4.1.1 specifies.
fn connect_body(user_dn: &str) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(user_dn.as_bytes());
    out.push(0);
    out.extend_from_slice(&0u32.to_le_bytes()); // Flags
    out.extend_from_slice(&65001u32.to_le_bytes()); // DefaultCodePage
    out.extend_from_slice(&1033u32.to_le_bytes()); // LcidSort
    out.extend_from_slice(&1033u32.to_le_bytes()); // LcidString
    out.extend_from_slice(&0u32.to_le_bytes()); // AuxiliaryBufferSize
    out
}

fn basic(email: &str, password: &str) -> String {
    format!("Basic {}", BASE64.encode(format!("{email}:{password}")))
}

/// Sends one request and returns the status, the headers and the body bytes.
async fn send(
    app: &Router,
    request_type: &str,
    auth: Option<&str>,
    cookie: Option<&str>,
    body: Vec<u8>,
) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/mapi/emsmdb")
        .header("X-RequestType", request_type)
        .header("X-RequestId", "req-42")
        .header("X-ClientInfo", "test-client")
        .header(header::CONTENT_TYPE, "application/mapi-http");
    if let Some(auth) = auth {
        builder = builder.header(header::AUTHORIZATION, auth);
    }
    if let Some(cookie) = cookie {
        builder = builder.header("cookie", cookie);
    }
    let response = app
        .clone()
        .oneshot(builder.body(Body::from(body)).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = to_bytes(response.into_body(), 256 * 1024)
        .await
        .unwrap()
        .to_vec();
    (status, headers, bytes)
}

fn response_code(headers: &axum::http::HeaderMap) -> u32 {
    headers
        .get("X-ResponseCode")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
        .expect("a response code")
}

/// The cookie value the server set for the session context.
fn session_cookie(headers: &axum::http::HeaderMap) -> String {
    headers
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find_map(|v| cookie_value(v, SESSION_COOKIE))
        .expect("a session cookie")
}

#[tokio::test]
async fn connect_then_disconnect_is_the_whole_handshake() {
    let h = harness("arc").await;
    let auth = basic(&h.email, &h.password);

    let (status, headers, body) = send(
        &h.app,
        "Connect",
        Some(&auth),
        None,
        connect_body("/o=alo/cn=tester"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(response_code(&headers), ResponseCode::Success.code());
    assert_eq!(headers.get("X-RequestType").unwrap(), "Connect");
    // The request id is echoed so the client can match this to what it sent.
    assert_eq!(headers.get("X-RequestId").unwrap(), "req-42");
    assert_eq!(
        headers.get(header::CONTENT_TYPE).unwrap(),
        "application/mapi-http"
    );
    // The client is told how to pace itself and when the context dies.
    assert!(headers.get("X-PendingPeriod").is_some());
    assert!(headers.get("X-ExpirationInfo").is_some());

    // The framing, then a StatusCode of zero as the specification requires.
    let framing = b"PROCESSING\r\nDONE\r\n";
    assert!(
        body.starts_with(framing),
        "{:?}",
        &body[..body.len().min(40)]
    );
    let split = body
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("a blank line before the body");
    let payload = &body[split + 4..];
    assert_eq!(&payload[0..4], &0u32.to_le_bytes(), "StatusCode MUST be 0");

    // Now end it, quoting the cookie we were given.
    let token = session_cookie(&headers);
    let (status, headers, _) = send(
        &h.app,
        "Disconnect",
        Some(&auth),
        Some(&format!("{SESSION_COOKIE}={token}")),
        Vec::new(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response_code(&headers), ResponseCode::Success.code());

    // Ending it twice is not an error — a client retrying a Disconnect it
    // already completed is behaving correctly.
    let (_, headers, _) = send(
        &h.app,
        "Disconnect",
        Some(&auth),
        Some(&format!("{SESSION_COOKIE}={token}")),
        Vec::new(),
    )
    .await;
    assert_eq!(response_code(&headers), ResponseCode::Success.code());
}

/// No credential, a wrong one, and a well-formed one for a user who does not
/// exist all get the same answer: a challenge and nothing else. Nothing here
/// tells a prober which addresses are real.
#[tokio::test]
async fn an_unauthenticated_caller_learns_nothing() {
    let h = harness("auth").await;

    let attempts = [
        None,
        Some(basic(&h.email, "wrong-password")),
        Some(basic("nobody@mapi.test", "correct-horse-battery")),
        Some("Bearer not-basic-at-all".to_owned()),
        Some("Basic !!!not-base64!!!".to_owned()),
    ];
    for attempt in attempts {
        let (status, headers, body) = send(
            &h.app,
            "Connect",
            attempt.as_deref(),
            None,
            connect_body("/o=alo/cn=tester"),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{attempt:?}");
        assert_eq!(
            headers.get(header::WWW_AUTHENTICATE).unwrap(),
            r#"Basic realm="alo""#
        );
        // No session was handed out, and no protocol detail leaked.
        assert!(headers.get(header::SET_COOKIE).is_none());
        assert!(body.is_empty(), "a rejected caller was told something");
    }
}

/// **The wrong-tenant test.** A perfectly valid credential from tenant B must
/// not end tenant A's Session Context by quoting its cookie. The cookie is
/// unguessable, but unguessable is not an authorisation model.
#[tokio::test]
async fn one_tenant_cannot_end_another_tenants_session() {
    let h = harness("iso-a").await;
    // A second tenant with its own credentialed user, sharing the router — so
    // the only thing separating them is the check under test.
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url())
        .await
        .expect("test database");
    let store = Arc::new(Store::new(pool, BlobStore::in_memory(8 * 1024 * 1024)));
    let identity = Identity::new(Arc::clone(&store), IdentityConfig::new("https://id.test"))
        .expect("identity");
    let (other_tenant, other_email, other_password) = seed(&store, &identity, "iso-b").await;
    assert_ne!(h.tenant, other_tenant, "the tenants must really differ");

    // Tenant A connects and gets a context.
    let (_, headers, _) = send(
        &h.app,
        "Connect",
        Some(&basic(&h.email, &h.password)),
        None,
        connect_body("/o=alo/cn=a"),
    )
    .await;
    let stolen = session_cookie(&headers);

    // Tenant B authenticates successfully and quotes A's cookie.
    let (status, headers, _) = send(
        &h.app,
        "Disconnect",
        Some(&basic(&other_email, &other_password)),
        Some(&format!("{SESSION_COOKIE}={stolen}")),
        Vec::new(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        response_code(&headers),
        ResponseCode::NoPrivilege.code(),
        "another tenant ended this session"
    );

    // ...and A's context is still alive, which is the part that matters.
    let (_, headers, _) = send(
        &h.app,
        "Disconnect",
        Some(&basic(&h.email, &h.password)),
        Some(&format!("{SESSION_COOKIE}={stolen}")),
        Vec::new(),
    )
    .await;
    assert_eq!(response_code(&headers), ResponseCode::Success.code());
}

/// A malformed body is refused by code, not by an HTTP error — the protocol
/// carries failures in `X-ResponseCode` on a `200`.
#[tokio::test]
async fn a_malformed_connect_body_is_refused_by_code() {
    let h = harness("body").await;
    let auth = basic(&h.email, &h.password);

    for body in [
        Vec::new(),                             // nothing at all
        b"/o=alo/cn=never-terminates".to_vec(), // no NUL
        b"/o=alo\0\x01\x02".to_vec(),           // dies inside Flags
    ] {
        let (status, headers, _) = send(&h.app, "Connect", Some(&auth), None, body).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            response_code(&headers),
            ResponseCode::InvalidRequestBody.code()
        );
        assert!(headers.get(header::SET_COOKIE).is_none());
    }
}

/// The stages that are not built say so, rather than answering with a
/// plausible empty success that would have Outlook believe the mailbox is
/// empty. When `Execute` lands, this test changes in that commit.
#[tokio::test]
async fn the_unbuilt_request_types_refuse_honestly() {
    let h = harness("stage").await;
    let auth = basic(&h.email, &h.password);

    for request_type in ["Execute", "NotificationWait"] {
        let (status, headers, _) = send(&h.app, request_type, Some(&auth), None, Vec::new()).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            response_code(&headers),
            ResponseCode::EndpointDisabled.code(),
            "{request_type} answered as though it worked"
        );
    }

    // An unknown request type is a different failure, and says so.
    let (_, headers, _) = send(&h.app, "Nonsense", Some(&auth), None, Vec::new()).await;
    assert_eq!(
        response_code(&headers),
        ResponseCode::InvalidRequestType.code()
    );
}

/// The address book endpoint answers rather than 404s: Autodiscover names it,
/// and a client that found nothing would retry until it timed out.
#[tokio::test]
async fn the_address_book_endpoint_refuses_instead_of_vanishing() {
    let h = harness("nspi").await;
    let response = h
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/mapi/nspi")
                .header("X-RequestType", "Bind")
                .header("X-RequestId", "req-1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_code(response.headers()),
        ResponseCode::EndpointDisabled.code()
    );
}
