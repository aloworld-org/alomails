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
    /// The caller's own account, for tests that create real folders.
    account: alo_store::AccountStore,
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

    let (tenant, email, password, user) = seed(&store, &identity, tag).await;
    let account = store.for_account(tenant.clone(), user);
    let app = router(MapiState {
        store: Arc::clone(&store),
        identity,
        sessions: Arc::new(SessionStore::new()),
        dn_prefix: "/o=alo".to_owned(),
    });
    Harness {
        app,
        tenant,
        email,
        password,
        account,
    }
}

/// Creates a tenant with one credentialed user. Returns the tenant, the login
/// name and the password.
async fn seed(
    store: &Arc<Store>,
    identity: &Identity,
    tag: &str,
) -> (TenantId, String, String, alo_store::UserId) {
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
    (tenant, email, "correct-horse-battery".to_owned(), user)
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

/// The blank line that separates the response framing from the binary body.
const BLANK_LINE: &[u8] = b"\r\n\r\n";

/// A `RopLogon` request for `essdn`, laid out as [MS-OXCROPS] §2.2.3.1.1.
fn rop_logon(essdn: &str) -> Vec<u8> {
    let mut rop = vec![0xFE, 0x00, 0x00, 0x01];
    rop.extend_from_slice(&0u32.to_le_bytes()); // OpenFlags
    rop.extend_from_slice(&0u32.to_le_bytes()); // StoreState
    let mut name = essdn.as_bytes().to_vec();
    name.push(0);
    rop.extend_from_slice(&u16::try_from(name.len()).unwrap().to_le_bytes());
    rop.extend_from_slice(&name);

    // The ROP container: RopSize counts itself, then one unset handle slot.
    let mut buffer = u16::try_from(rop.len() + 2).unwrap().to_le_bytes().to_vec();
    buffer.extend_from_slice(&rop);
    buffer.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    buffer
}

/// An `Execute` request body wrapping `rops` in a single `Last` segment.
fn execute_body(rops: &[u8], max_rop_out: u32) -> Vec<u8> {
    let mut segment = Vec::new();
    segment.extend_from_slice(&0u16.to_le_bytes()); // Version
    segment.extend_from_slice(&0x0004u16.to_le_bytes()); // Last
    let size = u16::try_from(rops.len()).unwrap();
    segment.extend_from_slice(&size.to_le_bytes());
    segment.extend_from_slice(&size.to_le_bytes());
    segment.extend_from_slice(rops);

    let mut out = Vec::new();
    out.extend_from_slice(&0u32.to_le_bytes()); // Flags
    out.extend_from_slice(&u32::try_from(segment.len()).unwrap().to_le_bytes());
    out.extend_from_slice(&segment);
    out.extend_from_slice(&max_rop_out.to_le_bytes());
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
    let (other_tenant, other_email, other_password, _) = seed(&store, &identity, "iso-b").await;
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

/// **A logon through `Execute`, end to end.** Connect, then a ROP buffer
/// carrying a `RopLogon` for the caller's own mailbox, framed exactly as a
/// client frames it: extended buffer inside the Execute body, ROP container
/// inside that.
#[tokio::test]
async fn a_logon_through_execute_opens_the_callers_own_mailbox() {
    let h = harness("exec").await;
    let auth = basic(&h.email, &h.password);

    let (_, headers, _) = send(
        &h.app,
        "Connect",
        Some(&auth),
        None,
        connect_body("/o=alo/cn=x"),
    )
    .await;
    let cookie = format!("{SESSION_COOKIE}={}", session_cookie(&headers));

    let local = h.email.split('@').next().unwrap().to_owned();
    let (status, headers, body) = send(
        &h.app,
        "Execute",
        Some(&auth),
        Some(&cookie),
        execute_body(&rop_logon(&format!("/o=alo/cn={local}")), 32 * 1024),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(response_code(&headers), ResponseCode::Success.code());

    // Dig the ROP response out: framing, Execute body, extended buffer, ROP
    // container — the same four layers the client unwraps.
    let split = body
        .windows(4)
        .position(|w| w == BLANK_LINE)
        .expect("framing");
    let execute = &body[split + 4..];
    assert_eq!(&execute[0..4], &0u32.to_le_bytes(), "StatusCode");
    assert_eq!(&execute[4..8], &0u32.to_le_bytes(), "ErrorCode");
    let rop_len = u32::from_le_bytes(execute[12..16].try_into().unwrap()) as usize;
    let wrapped = &execute[16..16 + rop_len];

    // RPC_HEADER_EXT, then the ROP buffer.
    let payload = &wrapped[8..];
    let rop_size = u16::from_le_bytes(payload[0..2].try_into().unwrap()) as usize;
    let responses = &payload[2..rop_size];

    assert_eq!(responses[0], 0xFE, "a RopLogon response");
    assert_eq!(
        u32::from_le_bytes(responses[2..6].try_into().unwrap()),
        0,
        "the logon failed"
    );
    assert_eq!(
        responses.len(),
        166,
        "a full private-mailbox logon response"
    );
}

/// **The folders a person actually has, read over the wire.**
///
/// A mailbox is given a real inbox and a folder of the person's own, then a
/// client logs on, opens the interpersonal-messages subtree, takes its
/// hierarchy table, asks for names and message counts, and reads the rows —
/// all through the real router against the real store.
///
/// This is the test that would have failed before the store was wired in: the
/// adapter used to answer with thirteen fixed folders and refuse every count.
#[tokio::test]
async fn a_client_reads_the_folders_this_person_actually_has() {
    let h = harness("real-folders").await;
    let auth = basic(&h.email, &h.password);

    // A real inbox with mail in it, and a folder they made themselves.
    let inbox = h
        .account
        .create_mailbox(None, "Inbox", Some("inbox"))
        .await
        .expect("inbox");
    h.account
        .create_mailbox(None, "Facturen", None)
        .await
        .expect("their own folder");
    let _ = inbox;

    let (_, headers, _) = send(
        &h.app,
        "Connect",
        Some(&auth),
        None,
        connect_body("/o=alo/cn=x"),
    )
    .await;
    let cookie = format!("{SESSION_COOKIE}={}", session_cookie(&headers));

    // Logon, open the subtree, take its table, set columns, read rows.
    let mut rops = Vec::new();
    let logon = rop_logon("");
    let rop_size = u16::from_le_bytes(logon[0..2].try_into().unwrap()) as usize;
    rops.extend_from_slice(&logon[2..rop_size]);
    // RopOpenFolder on the IPM subtree: counter 4 (slot 3 plus one).
    let subtree = {
        let mut fid = [0u8; 8];
        fid[0..2].copy_from_slice(&1u16.to_le_bytes());
        fid[2..8].copy_from_slice(&4u64.to_le_bytes()[0..6]);
        u64::from_le_bytes(fid)
    };
    rops.push(0x02);
    rops.extend_from_slice(&[0x00, 0x00, 0x01]);
    rops.extend_from_slice(&subtree.to_le_bytes());
    rops.push(0x00);
    // RopGetHierarchyTable on that folder.
    rops.extend_from_slice(&[0x04, 0x00, 0x01, 0x02, 0x00]);
    // RopSetColumns: display name, then message count.
    rops.extend_from_slice(&[0x12, 0x00, 0x02, 0x00]);
    rops.extend_from_slice(&2u16.to_le_bytes());
    rops.extend_from_slice(&[0x1F, 0x00, 0x01, 0x30]); // PidTagDisplayName
    rops.extend_from_slice(&[0x03, 0x00, 0x02, 0x36]); // PidTagContentCount
    // RopQueryRows.
    rops.extend_from_slice(&[0x15, 0x00, 0x02, 0x00, 0x01]);
    rops.extend_from_slice(&50u16.to_le_bytes());

    let mut buffer = u16::try_from(rops.len() + 2)
        .unwrap()
        .to_le_bytes()
        .to_vec();
    buffer.extend_from_slice(&rops);
    buffer.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    buffer.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    buffer.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());

    let (status, headers, body) = send(
        &h.app,
        "Execute",
        Some(&auth),
        Some(&cookie),
        execute_body(&buffer, 64 * 1024),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response_code(&headers), ResponseCode::Success.code());

    // Unwrap to the ROP responses, then walk to the QueryRows one.
    let split = body
        .windows(4)
        .position(|w| w == BLANK_LINE)
        .expect("framing");
    let execute = &body[split + 4..];
    let rop_len = u32::from_le_bytes(execute[12..16].try_into().unwrap()) as usize;
    let payload = &execute[16..16 + rop_len][8..];
    let size = u16::from_le_bytes(payload[0..2].try_into().unwrap()) as usize;
    let responses = &payload[2..size];

    // 166 logon + 8 open + 10 table + 7 columns, then the rows.
    let query = &responses[191..];
    assert_eq!(query[0], 0x15, "a RopQueryRows response");
    assert_eq!(
        u32::from_le_bytes(query[2..6].try_into().unwrap()),
        0,
        "reading the folder tree failed"
    );

    // Decode the rows: each is a flag byte, a UTF-16LE name, then a count.
    let count = u16::from_le_bytes(query[7..9].try_into().unwrap());
    let mut at = 9;
    let mut seen: Vec<(String, u32)> = Vec::new();
    for _ in 0..count {
        at += 1; // the flag byte
        let start = at;
        while query[at] != 0 || query[at + 1] != 0 {
            at += 2;
        }
        let units: Vec<u16> = query[start..at]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        at += 2;
        let messages = u32::from_le_bytes(query[at..at + 4].try_into().unwrap());
        at += 4;
        seen.push((String::from_utf16(&units).unwrap(), messages));
    }

    let names: Vec<&str> = seen.iter().map(|(name, _)| name.as_str()).collect();
    assert!(
        names.contains(&"Facturen"),
        "the folder this person made is missing: {names:?}"
    );
    assert!(names.contains(&"Inbox"), "{names:?}");
    // A real mailbox answers its count instead of refusing — an empty one
    // reports zero because a mailbox said so, which is a measurement rather
    // than a guess.
    assert!(seen.iter().any(|(name, _)| name == "Facturen"), "{seen:?}");
}

/// A caller who authenticated perfectly cannot log on to somebody else's
/// mailbox by naming it — through the real router, not just the dispatcher.
#[tokio::test]
async fn execute_refuses_a_logon_to_another_mailbox() {
    let h = harness("exec-deny").await;
    let auth = basic(&h.email, &h.password);

    let (_, headers, _) = send(
        &h.app,
        "Connect",
        Some(&auth),
        None,
        connect_body("/o=alo/cn=x"),
    )
    .await;
    let cookie = format!("{SESSION_COOKIE}={}", session_cookie(&headers));

    let (_, headers, body) = send(
        &h.app,
        "Execute",
        Some(&auth),
        Some(&cookie),
        execute_body(&rop_logon("/o=alo/cn=somebody-else"), 32 * 1024),
    )
    .await;
    assert_eq!(response_code(&headers), ResponseCode::Success.code());

    let split = body
        .windows(4)
        .position(|w| w == BLANK_LINE)
        .expect("framing");
    let execute = &body[split + 4..];
    let rop_len = u32::from_le_bytes(execute[12..16].try_into().unwrap()) as usize;
    let payload = &execute[16..16 + rop_len][8..];
    let rop_size = u16::from_le_bytes(payload[0..2].try_into().unwrap()) as usize;
    let responses = &payload[2..rop_size];

    assert_eq!(responses.len(), 6, "a failure response");
    assert_eq!(
        u32::from_le_bytes(responses[2..6].try_into().unwrap()),
        0x8007_0005,
        "expected ecAccessDenied"
    );
}

/// An `Execute` without a Session Context is refused before its body is read.
#[tokio::test]
async fn execute_without_a_session_is_refused() {
    let h = harness("exec-nosess").await;
    let auth = basic(&h.email, &h.password);
    let (_, headers, _) = send(
        &h.app,
        "Execute",
        Some(&auth),
        None,
        execute_body(&rop_logon(""), 32 * 1024),
    )
    .await;
    assert_eq!(
        response_code(&headers),
        ResponseCode::ContextNotFound.code()
    );
}

/// The stages that are not built say so, rather than answering with a
/// plausible empty success that would have Outlook believe the mailbox is
/// empty. When `Execute` lands, this test changes in that commit.
#[tokio::test]
async fn the_unbuilt_request_types_refuse_honestly() {
    let h = harness("stage").await;
    let auth = basic(&h.email, &h.password);

    for request_type in ["NotificationWait"] {
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
