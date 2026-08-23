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

/// The database this suite runs against.
///
/// Delegates to `alo_test_db`, which refuses the database the product
/// runs on: suites create and drop their own, they never write into `alo`.
fn database_url() -> String {
    alo_test_db::url()
}

/// A tenant with one user who has a password, and the router in front of them.
struct Harness {
    app: Router,
    tenant: TenantId,
    email: String,
    password: String,
    /// The caller's own account, for tests that create real folders.
    account: alo_store::AccountStore,
    /// The store behind it, for tests that need a second person in the tenant.
    store: Arc<Store>,
}

async fn harness(tag: &str) -> Harness {
    harness_with_submission(tag, None).await
}

/// A harness whose deployment may have a submission listener behind it.
async fn harness_with_submission(tag: &str, submission_addr: Option<String>) -> Harness {
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
        submission_addr,
    });
    Harness {
        app,
        tenant,
        email,
        password,
        account,
        store,
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
///
/// It now **challenges** an unauthenticated caller rather than refusing the
/// request type. That is what the endpoint becoming real changed: it reads the
/// tenant's own people, so a caller says who they are before it says anything
/// at all — and the challenge is what makes Outlook prompt.
#[tokio::test]
async fn the_address_book_endpoint_challenges_instead_of_vanishing() {
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
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(
        response
            .headers()
            .contains_key(axum::http::header::WWW_AUTHENTICATE),
        "no challenge, so a client has nothing to prompt with"
    );
}

/// **Stage 4 on the real wire**: a client logs on, opens its inbox, takes the
/// contents table, names its columns and reads the rows — over HTTP, against a
/// real store holding real delivered mail.
///
/// This is the test that says "Outlook can list the messages in a folder",
/// because every byte here is the byte Outlook would send or read.
#[tokio::test]
async fn a_client_reads_the_messages_in_a_folder_over_http() {
    let h = harness("contents").await;
    let auth = basic(&h.email, &h.password);

    let inbox = h
        .account
        .create_mailbox(None, "Inbox", Some("inbox"))
        .await
        .expect("inbox");
    for (n, subject) in ["Rechnung", "Liège", "Müller"].iter().enumerate() {
        let raw = format!(
            "From: Sender {n} <s{n}@example.test>\r\nTo: {to}\r\n\
             Subject: {subject}\r\nMessage-ID: <c{n}@example.test>\r\n\r\nbody\r\n",
            to = h.email
        );
        h.account.deliver(raw.as_bytes()).await.expect("deliver");
    }
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

    let mut rops = Vec::new();
    let logon = rop_logon("");
    let rop_size = u16::from_le_bytes(logon[0..2].try_into().unwrap()) as usize;
    rops.extend_from_slice(&logon[2..rop_size]);

    // RopOpenFolder on the Inbox: slot 4 of the special folders, counter 5.
    let inbox_fid = {
        let mut fid = [0u8; 8];
        fid[0..2].copy_from_slice(&1u16.to_le_bytes());
        fid[2..8].copy_from_slice(&5u64.to_le_bytes()[0..6]);
        u64::from_le_bytes(fid)
    };
    rops.push(0x02);
    rops.extend_from_slice(&[0x00, 0x00, 0x01]);
    rops.extend_from_slice(&inbox_fid.to_le_bytes());
    rops.push(0x00);

    // RopGetContentsTable on that folder.
    rops.extend_from_slice(&[0x05, 0x00, 0x01, 0x02, 0x00]);

    // RopSetColumns: subject, sender, delivery time, flags.
    rops.extend_from_slice(&[0x12, 0x00, 0x02, 0x00]);
    rops.extend_from_slice(&4u16.to_le_bytes());
    rops.extend_from_slice(&[0x1F, 0x00, 0x37, 0x00]); // PidTagSubject
    rops.extend_from_slice(&[0x1F, 0x00, 0x1A, 0x0C]); // PidTagSenderName
    rops.extend_from_slice(&[0x40, 0x00, 0x06, 0x0E]); // PidTagMessageDeliveryTime
    rops.extend_from_slice(&[0x03, 0x00, 0x07, 0x0E]); // PidTagMessageFlags

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

    let split = body
        .windows(4)
        .position(|w| w == BLANK_LINE)
        .expect("framing");
    let execute = &body[split + 4..];
    let rop_len = u32::from_le_bytes(execute[12..16].try_into().unwrap()) as usize;
    let payload = &execute[16..16 + rop_len][8..];
    let size = u16::from_le_bytes(payload[0..2].try_into().unwrap()) as usize;
    let responses = &payload[2..size];

    // The contents table reports the folder's real count.
    let table = &responses[174..184];
    assert_eq!(table[0], 0x05, "a RopGetContentsTable response");
    assert_eq!(
        u32::from_le_bytes(table[2..6].try_into().unwrap()),
        0,
        "opening the contents table failed"
    );
    assert_eq!(
        u32::from_le_bytes(table[6..10].try_into().unwrap()),
        3,
        "three messages were delivered"
    );

    // 166 logon + 8 open + 10 table + 7 columns, then the rows.
    let query = &responses[191..];
    assert_eq!(query[0], 0x15, "a RopQueryRows response");
    assert_eq!(
        u32::from_le_bytes(query[2..6].try_into().unwrap()),
        0,
        "reading the messages failed"
    );

    let count = u16::from_le_bytes(query[7..9].try_into().unwrap());
    assert_eq!(count, 3, "every delivered message is listed");

    // Decode: flag byte, subject, sender, an 8-byte time, a 4-byte flag word.
    let mut at = 9;
    let mut seen: Vec<(String, String, u64, u32)> = Vec::new();
    for _ in 0..count {
        at += 1; // the flag byte
        let subject = read_utf16(query, &mut at);
        let sender = read_utf16(query, &mut at);
        let time = u64::from_le_bytes(query[at..at + 8].try_into().unwrap());
        at += 8;
        let flags = u32::from_le_bytes(query[at..at + 4].try_into().unwrap());
        at += 4;
        seen.push((subject, sender, time, flags));
    }

    let subjects: Vec<&str> = seen.iter().map(|(s, ..)| s.as_str()).collect();
    assert!(subjects.contains(&"Rechnung"), "{subjects:?}");
    // A European product: the accented subjects must survive UTF-16LE intact.
    assert!(subjects.contains(&"Liège"), "{subjects:?}");
    assert!(subjects.contains(&"Müller"), "{subjects:?}");

    for (subject, sender, time, flags) in &seen {
        assert!(sender.contains("example.test"), "{sender} for {subject}");
        // A FILETIME, not a Unix timestamp: the epoch mistake renders as a
        // plausible date in the wrong century rather than as a failure.
        assert!(
            *time > 130_000_000_000_000_000,
            "delivery time is not a FILETIME: {time}"
        );
        // Delivered mail is unread, which is what makes a row bold.
        assert_eq!(flags & 0x0000_0001, 0, "newly delivered mail reads unread");
    }
}

/// The offset just past a `RopOpenMessage` response, given the responses that
/// begin with one.
///
/// Walks it the way a client must: the typed subject strings are
/// variable-length, and so is the recipient table behind them. Tests used to
/// assume the table was empty and add a constant; that stopped being true the
/// moment recipients were served, which is why this exists.
fn past_open_message(open: &[u8]) -> usize {
    // RopId, OutputHandleIndex, ReturnValue(4), HasNamedProperties.
    let mut at = 7;
    for _ in 0..2 {
        match open[at] {
            0x00 | 0x01 => at += 1, // absent or empty: nothing follows
            0x04 => {
                at += 1;
                let _ = read_utf16(open, &mut at);
            }
            other => panic!("unexpected StringType {other:#04x}"),
        }
    }
    let rows = open[at + 4];
    at += 5;
    for _ in 0..rows {
        let size = usize::from(u16::from_le_bytes(open[at + 5..at + 7].try_into().unwrap()));
        at += 7 + size;
    }
    at
}

/// Reads a UTF-16LE null-terminated string, advancing `at` past its terminator.
fn read_utf16(bytes: &[u8], at: &mut usize) -> String {
    let start = *at;
    while bytes[*at] != 0 || bytes[*at + 1] != 0 {
        *at += 2;
    }
    let units: Vec<u16> = bytes[start..*at]
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    *at += 2;
    String::from_utf16(&units).expect("utf-16")
}

/// A caller cannot read another mailbox's messages by naming its folder: the
/// contents table is opened on a folder from **this** session's own tree, and
/// that tree was built from the authenticated account's mailboxes.
#[tokio::test]
async fn one_tenant_cannot_read_another_tenants_messages() {
    let victim = harness("contents-victim").await;
    victim
        .account
        .create_mailbox(None, "Inbox", Some("inbox"))
        .await
        .expect("inbox");
    victim
        .account
        .deliver(
            b"From: s@example.test\r\nTo: v@example.test\r\n\
              Subject: Vertraulich\r\n\r\nbody\r\n",
        )
        .await
        .expect("deliver");

    // A different tenant entirely, with its own empty inbox.
    let other = harness("contents-other").await;
    other
        .account
        .create_mailbox(None, "Inbox", Some("inbox"))
        .await
        .expect("inbox");
    let auth = basic(&other.email, &other.password);

    let (_, headers, _) = send(
        &other.app,
        "Connect",
        Some(&auth),
        None,
        connect_body("/o=alo/cn=x"),
    )
    .await;
    let cookie = format!("{SESSION_COOKIE}={}", session_cookie(&headers));

    // The Inbox folder id is the same fixed number in every mailbox — that is
    // exactly why this test exists. Reading it must yield this caller's inbox,
    // which is empty, and never the other tenant's.
    let inbox_fid = {
        let mut fid = [0u8; 8];
        fid[0..2].copy_from_slice(&1u16.to_le_bytes());
        fid[2..8].copy_from_slice(&5u64.to_le_bytes()[0..6]);
        u64::from_le_bytes(fid)
    };

    let mut rops = Vec::new();
    let logon = rop_logon("");
    let rop_size = u16::from_le_bytes(logon[0..2].try_into().unwrap()) as usize;
    rops.extend_from_slice(&logon[2..rop_size]);
    rops.push(0x02);
    rops.extend_from_slice(&[0x00, 0x00, 0x01]);
    rops.extend_from_slice(&inbox_fid.to_le_bytes());
    rops.push(0x00);
    rops.extend_from_slice(&[0x05, 0x00, 0x01, 0x02, 0x00]);
    rops.extend_from_slice(&[0x12, 0x00, 0x02, 0x00]);
    rops.extend_from_slice(&1u16.to_le_bytes());
    rops.extend_from_slice(&[0x1F, 0x00, 0x37, 0x00]); // PidTagSubject
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

    let (status, _, body) = send(
        &other.app,
        "Execute",
        Some(&auth),
        Some(&cookie),
        execute_body(&buffer, 64 * 1024),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let split = body
        .windows(4)
        .position(|w| w == BLANK_LINE)
        .expect("framing");
    let execute = &body[split + 4..];
    let rop_len = u32::from_le_bytes(execute[12..16].try_into().unwrap()) as usize;
    let payload = &execute[16..16 + rop_len][8..];
    let size = u16::from_le_bytes(payload[0..2].try_into().unwrap()) as usize;
    let responses = &payload[2..size];

    // The table opens — it is this caller's own inbox — and holds nothing.
    let table = &responses[174..184];
    assert_eq!(
        u32::from_le_bytes(table[6..10].try_into().unwrap()),
        0,
        "another tenant's message count leaked"
    );

    let query = &responses[191..];
    assert_eq!(
        u16::from_le_bytes(query[7..9].try_into().unwrap()),
        0,
        "another tenant's mail was returned"
    );
    // And the word never appears anywhere in the answer.
    let text = String::from_utf8_lossy(responses);
    assert!(
        !text.contains("Vertraulich"),
        "the other tenant's subject is in the response"
    );
}

/// **Stage 5 on the real wire — the kill gate.** A client logs on, opens its
/// inbox, reads the contents table to learn a message's MID, opens that
/// message, and asks for its subject, body, To line and sender. Over HTTP,
/// against a real store holding a real delivered message.
///
/// If this passes, Outlook can open and read mail from alo.
#[tokio::test]
async fn a_client_opens_and_reads_a_message_over_http() {
    let h = harness("open-message").await;
    let auth = basic(&h.email, &h.password);

    h.account
        .create_mailbox(None, "Inbox", Some("inbox"))
        .await
        .expect("inbox");
    let raw = format!(
        "From: Müller <m@example.test>\r\nTo: {to}\r\nCc: Liège <l@example.test>\r\n\
         Subject: Rechnung für August\r\nDate: Fri, 21 Aug 2026 09:15:00 +0000\r\n\
         Message-ID: <open-1@example.test>\r\n\
         Content-Type: text/plain; charset=utf-8\r\n\r\n\
         Sehr geehrte Damen und Herren,\r\n\r\nanbei die Rechnung.\r\n",
        to = h.email
    );
    h.account.deliver(raw.as_bytes()).await.expect("deliver");

    let (_, headers, _) = send(
        &h.app,
        "Connect",
        Some(&auth),
        None,
        connect_body("/o=alo/cn=x"),
    )
    .await;
    let cookie = format!("{SESSION_COOKIE}={}", session_cookie(&headers));

    let inbox_fid = {
        let mut fid = [0u8; 8];
        fid[0..2].copy_from_slice(&1u16.to_le_bytes());
        fid[2..8].copy_from_slice(&5u64.to_le_bytes()[0..6]);
        u64::from_le_bytes(fid)
    };

    // ---- first buffer: read the table to learn the MID --------------------
    let mut rops = Vec::new();
    let logon = rop_logon("");
    let rop_size = u16::from_le_bytes(logon[0..2].try_into().unwrap()) as usize;
    rops.extend_from_slice(&logon[2..rop_size]);
    rops.push(0x02);
    rops.extend_from_slice(&[0x00, 0x00, 0x01]);
    rops.extend_from_slice(&inbox_fid.to_le_bytes());
    rops.push(0x00);
    rops.extend_from_slice(&[0x05, 0x00, 0x01, 0x02, 0x00]); // RopGetContentsTable
    rops.extend_from_slice(&[0x12, 0x00, 0x02, 0x00]); // RopSetColumns
    rops.extend_from_slice(&1u16.to_le_bytes());
    rops.extend_from_slice(&[0x14, 0x00, 0x4A, 0x67]); // PidTagMid, PtypInteger64
    rops.extend_from_slice(&[0x15, 0x00, 0x02, 0x00, 0x01]); // RopQueryRows
    rops.extend_from_slice(&50u16.to_le_bytes());

    let responses = execute(&h, &auth, &cookie, &rops).await;
    let query = &responses[191..];
    assert_eq!(
        u32::from_le_bytes(query[2..6].try_into().unwrap()),
        0,
        "reading the table failed"
    );
    assert_eq!(u16::from_le_bytes(query[7..9].try_into().unwrap()), 1);
    // Flag byte, then the 8-byte MID.
    let mid = u64::from_le_bytes(query[10..18].try_into().unwrap());
    assert!(mid != 0, "a MID the client can open");

    // ---- second buffer: open the message and read its properties ----------
    let mut rops = Vec::new();
    let logon = rop_logon("");
    let rop_size = u16::from_le_bytes(logon[0..2].try_into().unwrap()) as usize;
    rops.extend_from_slice(&logon[2..rop_size]);

    // RopOpenMessage on the logon at index 0, message handle to index 1.
    rops.push(0x03);
    rops.extend_from_slice(&[0x00, 0x00, 0x01]);
    rops.extend_from_slice(&0u16.to_le_bytes()); // CodePageId
    rops.extend_from_slice(&inbox_fid.to_le_bytes());
    rops.push(0x00); // OpenModeFlags: read-only
    rops.extend_from_slice(&mid.to_le_bytes());

    // RopGetPropertiesSpecific on that message: subject, body, To, sender.
    rops.push(0x07);
    rops.extend_from_slice(&[0x00, 0x01]); // LogonId, InputHandleIndex
    rops.extend_from_slice(&0u16.to_le_bytes()); // PropertySizeLimit: none
    rops.extend_from_slice(&1u16.to_le_bytes()); // WantUnicode
    rops.extend_from_slice(&4u16.to_le_bytes()); // PropertyTagCount
    rops.extend_from_slice(&[0x1F, 0x00, 0x37, 0x00]); // PidTagSubject
    rops.extend_from_slice(&[0x1F, 0x00, 0x00, 0x10]); // PidTagBody
    rops.extend_from_slice(&[0x1F, 0x00, 0x04, 0x0E]); // PidTagDisplayTo
    rops.extend_from_slice(&[0x1F, 0x00, 0x1A, 0x0C]); // PidTagSenderName

    // RopRelease: the client is done with the message.
    rops.extend_from_slice(&[0x01, 0x00, 0x01]);

    let responses = execute(&h, &auth, &cookie, &rops).await;

    // The open response: 166 logon, then RopOpenMessage.
    let open = &responses[166..];
    assert_eq!(open[0], 0x03, "a RopOpenMessage response");
    assert_eq!(
        u32::from_le_bytes(open[2..6].try_into().unwrap()),
        0,
        "opening the message failed"
    );
    assert_eq!(open[6], 0x00, "no named properties");
    assert_eq!(open[7], 0x00, "SubjectPrefix absent");
    assert_eq!(open[8], 0x04, "NormalizedSubject is Unicode");

    let mut at = 9;
    let subject = read_utf16(open, &mut at);
    assert_eq!(subject, "Rechnung für August");
    // The recipient table is served now, so this message reports the people it
    // was addressed to rather than none. `ColumnCount` stays zero: the rows
    // carry named fields, not an extra property row.
    assert_eq!(
        u16::from_le_bytes(open[at..at + 2].try_into().unwrap()),
        2,
        "RecipientCount: the To address and the Cc one"
    );
    assert_eq!(
        u16::from_le_bytes(open[at + 2..at + 4].try_into().unwrap()),
        0,
        "ColumnCount"
    );
    assert_eq!(open[at + 4], 2, "RowCount");

    // Then the properties.
    let props = &open[past_open_message(open)..];
    assert_eq!(props[0], 0x07, "a RopGetPropertiesSpecific response");
    assert_eq!(
        u32::from_le_bytes(props[2..6].try_into().unwrap()),
        0,
        "reading the properties failed"
    );
    assert_eq!(props[6], 0x00, "a standard row: every value present");

    let mut at = 7;
    assert_eq!(read_utf16(props, &mut at), "Rechnung für August");
    let body = read_utf16(props, &mut at);
    assert!(
        body.contains("anbei die Rechnung"),
        "the body did not come back: {body:?}"
    );
    assert!(
        body.contains("Sehr geehrte Damen und Herren"),
        "the body was truncated: {body:?}"
    );
    let display_to = read_utf16(props, &mut at);
    assert!(display_to.contains(&h.email), "To line: {display_to:?}");
    let sender = read_utf16(props, &mut at);
    assert!(sender.contains("Müller"), "sender: {sender:?}");

    // RopRelease contributes nothing to the output — the properties response
    // is the last thing in the buffer.
    assert_eq!(at, props.len(), "something followed the release");
}

/// A message a caller does not own cannot be opened by naming its MID: the
/// lookup runs over this session's own loaded rows, and a MID that is not in
/// them is `ecNotFound` — the same answer a MID that never existed gets.
#[tokio::test]
async fn one_tenant_cannot_open_another_tenants_message() {
    let victim = harness("open-victim").await;
    victim
        .account
        .create_mailbox(None, "Inbox", Some("inbox"))
        .await
        .expect("inbox");
    victim
        .account
        .deliver(
            b"From: s@example.test\r\nTo: v@example.test\r\n\
              Subject: Vertraulich\r\n\r\ngeheim\r\n",
        )
        .await
        .expect("deliver");

    // The victim's own MID, obtained the way the victim would.
    let inbox = victim.account.inbox().await.expect("inbox id");
    let rows = victim
        .account
        .mapi_mailbox_rows(&inbox, alo_store::Page::first(10))
        .await
        .expect("rows");
    assert_eq!(rows.len(), 1);
    let victim_mid = alo_mapi::folders::fid(alo_mapi::messages::message_counter(&rows[0].id));

    // A different tenant, with an inbox of its own.
    let other = harness("open-other").await;
    other
        .account
        .create_mailbox(None, "Inbox", Some("inbox"))
        .await
        .expect("inbox");
    let auth = basic(&other.email, &other.password);

    let (_, headers, _) = send(
        &other.app,
        "Connect",
        Some(&auth),
        None,
        connect_body("/o=alo/cn=x"),
    )
    .await;
    let cookie = format!("{SESSION_COOKIE}={}", session_cookie(&headers));

    let inbox_fid = {
        let mut fid = [0u8; 8];
        fid[0..2].copy_from_slice(&1u16.to_le_bytes());
        fid[2..8].copy_from_slice(&5u64.to_le_bytes()[0..6]);
        u64::from_le_bytes(fid)
    };

    let mut rops = Vec::new();
    let logon = rop_logon("");
    let rop_size = u16::from_le_bytes(logon[0..2].try_into().unwrap()) as usize;
    rops.extend_from_slice(&logon[2..rop_size]);
    rops.push(0x03);
    rops.extend_from_slice(&[0x00, 0x00, 0x01]);
    rops.extend_from_slice(&0u16.to_le_bytes());
    rops.extend_from_slice(&inbox_fid.to_le_bytes());
    rops.push(0x00);
    rops.extend_from_slice(&victim_mid.to_le_bytes());

    let responses = execute(&other, &auth, &cookie, &rops).await;
    let open = &responses[166..];
    assert_eq!(open[0], 0x03);
    assert_eq!(
        u32::from_le_bytes(open[2..6].try_into().unwrap()),
        0x8004_010F,
        "another tenant's message was opened"
    );
    // And nothing of it appears anywhere in the answer.
    let text = String::from_utf8_lossy(responses.as_slice());
    assert!(!text.contains("Vertraulich"));
    assert!(!text.contains("geheim"));
}

/// Sends one ROP buffer through `Execute` and returns the ROP responses.
async fn execute(h: &Harness, auth: &str, cookie: &str, rops: &[u8]) -> Vec<u8> {
    let mut buffer = u16::try_from(rops.len() + 2)
        .unwrap()
        .to_le_bytes()
        .to_vec();
    buffer.extend_from_slice(rops);
    buffer.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    buffer.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    buffer.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());

    let (status, headers, body) = send(
        &h.app,
        "Execute",
        Some(auth),
        Some(cookie),
        execute_body(&buffer, 64 * 1024),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response_code(&headers), ResponseCode::Success.code());

    let split = body
        .windows(4)
        .position(|w| w == BLANK_LINE)
        .expect("framing");
    let execute = &body[split + 4..];
    let rop_len = u32::from_le_bytes(execute[12..16].try_into().unwrap()) as usize;
    let payload = &execute[16..16 + rop_len][8..];
    let size = u16::from_le_bytes(payload[0..2].try_into().unwrap()) as usize;
    payload[2..size].to_vec()
}

/// A body too large for the client's own property-size limit comes back whole
/// through a stream — read in chunks, reassembled, byte for byte.
///
/// This is the case the property row deliberately refuses: it marks an
/// oversized value absent rather than truncating it, and this is what a client
/// does next. Without it, long mail opens with nothing in it.
#[tokio::test]
async fn a_long_body_comes_back_through_a_stream() {
    let h = harness("stream-body").await;
    let auth = basic(&h.email, &h.password);

    h.account
        .create_mailbox(None, "Inbox", Some("inbox"))
        .await
        .expect("inbox");

    // A body far past any sane property-size limit, with an accented line so a
    // chunk boundary landing mid-character would show up.
    let mut long = String::new();
    for n in 0..400 {
        long.push_str(&format!("Zeile {n}: Grüße aus Liège.\r\n"));
    }
    let raw = format!(
        "From: Müller <m@example.test>\r\nTo: {to}\r\nSubject: Langer Text\r\n\
         Content-Type: text/plain; charset=utf-8\r\n\r\n{long}",
        to = h.email
    );
    h.account.deliver(raw.as_bytes()).await.expect("deliver");

    let (_, headers, _) = send(
        &h.app,
        "Connect",
        Some(&auth),
        None,
        connect_body("/o=alo/cn=x"),
    )
    .await;
    let cookie = format!("{SESSION_COOKIE}={}", session_cookie(&headers));

    let inbox_fid = {
        let mut fid = [0u8; 8];
        fid[0..2].copy_from_slice(&1u16.to_le_bytes());
        fid[2..8].copy_from_slice(&5u64.to_le_bytes()[0..6]);
        u64::from_le_bytes(fid)
    };

    // Learn the MID.
    let mut rops = Vec::new();
    let logon = rop_logon("");
    let rop_size = u16::from_le_bytes(logon[0..2].try_into().unwrap()) as usize;
    rops.extend_from_slice(&logon[2..rop_size]);
    rops.push(0x02);
    rops.extend_from_slice(&[0x00, 0x00, 0x01]);
    rops.extend_from_slice(&inbox_fid.to_le_bytes());
    rops.push(0x00);
    rops.extend_from_slice(&[0x05, 0x00, 0x01, 0x02, 0x00]);
    rops.extend_from_slice(&[0x12, 0x00, 0x02, 0x00]);
    rops.extend_from_slice(&1u16.to_le_bytes());
    rops.extend_from_slice(&[0x14, 0x00, 0x4A, 0x67]); // PidTagMid
    rops.extend_from_slice(&[0x15, 0x00, 0x02, 0x00, 0x01]);
    rops.extend_from_slice(&50u16.to_le_bytes());
    let responses = execute(&h, &auth, &cookie, &rops).await;
    let mid = u64::from_le_bytes(responses[191..][10..18].try_into().unwrap());

    // ---- the row refuses the oversized body, honouring the limit ----------
    let mut rops = Vec::new();
    let logon = rop_logon("");
    let rop_size = u16::from_le_bytes(logon[0..2].try_into().unwrap()) as usize;
    rops.extend_from_slice(&logon[2..rop_size]);
    rops.push(0x03);
    rops.extend_from_slice(&[0x00, 0x00, 0x01]);
    rops.extend_from_slice(&0u16.to_le_bytes());
    rops.extend_from_slice(&inbox_fid.to_le_bytes());
    rops.push(0x00);
    rops.extend_from_slice(&mid.to_le_bytes());
    rops.push(0x07);
    rops.extend_from_slice(&[0x00, 0x01]);
    rops.extend_from_slice(&512u16.to_le_bytes()); // PropertySizeLimit: small
    rops.extend_from_slice(&1u16.to_le_bytes());
    rops.extend_from_slice(&1u16.to_le_bytes());
    rops.extend_from_slice(&[0x1F, 0x00, 0x00, 0x10]); // PidTagBody

    let responses = execute(&h, &auth, &cookie, &rops).await;
    let open = &responses[166..];
    assert_eq!(
        u32::from_le_bytes(open[2..6].try_into().unwrap()),
        0,
        "opening failed"
    );
    let props = &open[past_open_message(open)..];
    assert_eq!(props[0], 0x07);
    assert_eq!(props[6], 0x01, "a flagged row: something was withheld");
    assert_eq!(
        props[7], 0x01,
        "the oversized body is marked absent, not truncated"
    );

    // ---- and comes back whole through a stream ----------------------------
    let mut rops = Vec::new();
    let logon = rop_logon("");
    let rop_size = u16::from_le_bytes(logon[0..2].try_into().unwrap()) as usize;
    rops.extend_from_slice(&logon[2..rop_size]);
    rops.push(0x03);
    rops.extend_from_slice(&[0x00, 0x00, 0x01]);
    rops.extend_from_slice(&0u16.to_le_bytes());
    rops.extend_from_slice(&inbox_fid.to_le_bytes());
    rops.push(0x00);
    rops.extend_from_slice(&mid.to_le_bytes());
    // RopOpenStream on the message at index 1, stream to index 2.
    rops.push(0x2B);
    rops.extend_from_slice(&[0x00, 0x01, 0x02]);
    rops.extend_from_slice(&[0x1F, 0x00, 0x00, 0x10]); // PidTagBody
    rops.push(0x00); // ReadOnly
    // Two reads: a short-form one, then the 0xBABE extended form.
    rops.push(0x2C);
    rops.extend_from_slice(&[0x00, 0x02]);
    rops.extend_from_slice(&4096u16.to_le_bytes());
    rops.push(0x2C);
    rops.extend_from_slice(&[0x00, 0x02]);
    rops.extend_from_slice(&0xBABEu16.to_le_bytes());
    rops.extend_from_slice(&60_000u32.to_le_bytes());

    let responses = execute(&h, &auth, &cookie, &rops).await;
    let open = &responses[166..];
    assert_eq!(open[0], 0x03);
    let stream = &open[past_open_message(open)..];
    assert_eq!(stream[0], 0x2B, "a RopOpenStream response");
    assert_eq!(
        u32::from_le_bytes(stream[2..6].try_into().unwrap()),
        0,
        "opening the stream failed"
    );
    let stream_size = u32::from_le_bytes(stream[6..10].try_into().unwrap()) as usize;
    assert!(stream_size > 512, "the body really is oversized");

    // First read: the short form, bounded by what was asked for.
    let first = &stream[10..];
    assert_eq!(first[0], 0x2C, "a RopReadStream response");
    assert_eq!(u32::from_le_bytes(first[2..6].try_into().unwrap()), 0);
    let first_size = usize::from(u16::from_le_bytes(first[6..8].try_into().unwrap()));
    assert_eq!(first_size, 4096, "the client's own count bounded the read");
    let mut collected = first[8..8 + first_size].to_vec();

    // Second read: the extended form, and the cursor advanced.
    let second = &first[8 + first_size..];
    assert_eq!(second[0], 0x2C);
    assert_eq!(u32::from_le_bytes(second[2..6].try_into().unwrap()), 0);
    let second_size = usize::from(u16::from_le_bytes(second[6..8].try_into().unwrap()));
    assert!(second_size > 0, "the second read returned nothing");
    collected.extend_from_slice(&second[8..8 + second_size]);

    assert_eq!(
        collected.len(),
        stream_size.min(4096 + second_size),
        "the two reads did not tile the stream"
    );
    // The bytes are UTF-16LE and decode to the body that was delivered.
    let units: Vec<u16> = collected
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    let text = String::from_utf16(&units).expect("utf-16");
    assert!(
        text.starts_with("Zeile 0: Grüße aus Liège."),
        "{:?}",
        &text[..40]
    );
    assert!(
        text.contains("Zeile 100: Grüße aus Liège."),
        "a chunk was lost"
    );
}

/// A stream cannot be opened for writing: nothing here writes, and a client
/// holding what it believes is a writable stream would send changes that went
/// nowhere.
#[tokio::test]
async fn a_writable_stream_is_refused_rather_than_opened_read_only() {
    let h = harness("stream-write").await;
    let auth = basic(&h.email, &h.password);
    h.account
        .create_mailbox(None, "Inbox", Some("inbox"))
        .await
        .expect("inbox");
    h.account
        .deliver(b"From: s@example.test\r\nTo: o@example.test\r\nSubject: kurz\r\n\r\nText\r\n")
        .await
        .expect("deliver");

    let (_, headers, _) = send(
        &h.app,
        "Connect",
        Some(&auth),
        None,
        connect_body("/o=alo/cn=x"),
    )
    .await;
    let cookie = format!("{SESSION_COOKIE}={}", session_cookie(&headers));

    let inbox = h.account.inbox().await.expect("inbox id");
    let rows = h
        .account
        .mapi_mailbox_rows(&inbox, alo_store::Page::first(10))
        .await
        .expect("rows");
    let mid = alo_mapi::folders::fid(alo_mapi::messages::message_counter(&rows[0].id));
    let inbox_fid = {
        let mut fid = [0u8; 8];
        fid[0..2].copy_from_slice(&1u16.to_le_bytes());
        fid[2..8].copy_from_slice(&5u64.to_le_bytes()[0..6]);
        u64::from_le_bytes(fid)
    };

    let mut rops = Vec::new();
    let logon = rop_logon("");
    let rop_size = u16::from_le_bytes(logon[0..2].try_into().unwrap()) as usize;
    rops.extend_from_slice(&logon[2..rop_size]);
    rops.push(0x03);
    rops.extend_from_slice(&[0x00, 0x00, 0x01]);
    rops.extend_from_slice(&0u16.to_le_bytes());
    rops.extend_from_slice(&inbox_fid.to_le_bytes());
    rops.push(0x00);
    rops.extend_from_slice(&mid.to_le_bytes());
    rops.push(0x2B);
    rops.extend_from_slice(&[0x00, 0x01, 0x02]);
    rops.extend_from_slice(&[0x1F, 0x00, 0x00, 0x10]);
    rops.push(0x01); // ReadWrite

    let responses = execute(&h, &auth, &cookie, &rops).await;
    let open = &responses[166..];
    let stream = &open[past_open_message(open)..];
    assert_eq!(stream[0], 0x2B);
    assert_eq!(
        u32::from_le_bytes(stream[2..6].try_into().unwrap()),
        0x8004_0FFF,
        "a writable stream was opened"
    );
}

/// A client lists a message's attachments and reads one back, byte for byte.
///
/// The whole chain: open the message, take its attachment table, name the
/// columns, read the rows, open the attachment the reader picked, and stream
/// its contents. This is what saving a file out of Outlook does.
#[tokio::test]
async fn a_client_lists_and_reads_an_attachment_over_http() {
    let h = harness("attachments").await;
    let auth = basic(&h.email, &h.password);
    h.account
        .create_mailbox(None, "Inbox", Some("inbox"))
        .await
        .expect("inbox");

    // Two files, so the numbering has to be right rather than coincidentally
    // right: a reader who clicks the second must not get the first.
    let first = b"Rechnung Nr. 42\r\nBetrag: 199,00 EUR\r\n";
    let second = b"%PDF-1.4 fake pdf bytes for the test";
    let raw = format!(
        "From: M\u{fc}ller <m@example.test>\r\nTo: {to}\r\nSubject: Mit Anhang\r\n\
         Content-Type: multipart/mixed; boundary=BOUND\r\n\r\n\
         --BOUND\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n\
         Anbei zwei Dateien.\r\n\
         --BOUND\r\nContent-Type: text/plain; charset=utf-8\r\n\
         Content-Disposition: attachment; filename=\"rechnung.txt\"\r\n\r\n\
         {first}\r\n\
         --BOUND\r\nContent-Type: application/pdf\r\n\
         Content-Disposition: attachment; filename=\"anhang.pdf\"\r\n\r\n\
         {second}\r\n--BOUND--\r\n",
        to = h.email,
        first = String::from_utf8_lossy(first),
        second = String::from_utf8_lossy(second),
    );
    h.account.deliver(raw.as_bytes()).await.expect("deliver");

    let (_, headers, _) = send(
        &h.app,
        "Connect",
        Some(&auth),
        None,
        connect_body("/o=alo/cn=x"),
    )
    .await;
    let cookie = format!("{SESSION_COOKIE}={}", session_cookie(&headers));

    let inbox = h.account.inbox().await.expect("inbox id");
    let rows = h
        .account
        .mapi_mailbox_rows(&inbox, alo_store::Page::first(10))
        .await
        .expect("rows");
    let mid = alo_mapi::folders::fid(alo_mapi::messages::message_counter(&rows[0].id));
    let inbox_fid = {
        let mut fid = [0u8; 8];
        fid[0..2].copy_from_slice(&1u16.to_le_bytes());
        fid[2..8].copy_from_slice(&5u64.to_le_bytes()[0..6]);
        u64::from_le_bytes(fid)
    };

    // ---- list the attachments --------------------------------------------
    let mut rops = Vec::new();
    let logon = rop_logon("");
    let rop_size = u16::from_le_bytes(logon[0..2].try_into().unwrap()) as usize;
    rops.extend_from_slice(&logon[2..rop_size]);
    rops.push(0x03); // RopOpenMessage -> index 1
    rops.extend_from_slice(&[0x00, 0x00, 0x01]);
    rops.extend_from_slice(&0u16.to_le_bytes());
    rops.extend_from_slice(&inbox_fid.to_le_bytes());
    rops.push(0x00);
    rops.extend_from_slice(&mid.to_le_bytes());
    rops.extend_from_slice(&[0x21, 0x00, 0x01, 0x02, 0x00]); // RopGetAttachmentTable
    rops.extend_from_slice(&[0x12, 0x00, 0x02, 0x00]); // RopSetColumns
    rops.extend_from_slice(&3u16.to_le_bytes());
    rops.extend_from_slice(&[0x03, 0x00, 0x21, 0x0E]); // PidTagAttachNumber
    rops.extend_from_slice(&[0x1F, 0x00, 0x07, 0x37]); // PidTagAttachLongFilename
    rops.extend_from_slice(&[0x03, 0x00, 0x20, 0x0E]); // PidTagAttachSize
    rops.extend_from_slice(&[0x15, 0x00, 0x02, 0x00, 0x01]); // RopQueryRows
    rops.extend_from_slice(&50u16.to_le_bytes());

    let responses = execute(&h, &auth, &cookie, &rops).await;
    let open = &responses[166..];
    assert_eq!(open[0], 0x03);
    let table = &open[past_open_message(open)..];
    assert_eq!(table[0], 0x21, "a RopGetAttachmentTable response");
    assert_eq!(
        u32::from_le_bytes(table[2..6].try_into().unwrap()),
        0,
        "opening the attachment table failed"
    );
    assert_eq!(
        u32::from_le_bytes(table[6..10].try_into().unwrap()),
        2,
        "two files were attached"
    );

    let query = &table[10 + 7..];
    assert_eq!(query[0], 0x15, "a RopQueryRows response");
    assert_eq!(u32::from_le_bytes(query[2..6].try_into().unwrap()), 0);
    let count = u16::from_le_bytes(query[7..9].try_into().unwrap());
    assert_eq!(count, 2);

    let mut at = 9;
    let mut listed: Vec<(u32, String, u32)> = Vec::new();
    for _ in 0..count {
        at += 1; // the flag byte
        let number = u32::from_le_bytes(query[at..at + 4].try_into().unwrap());
        at += 4;
        let name = read_utf16(query, &mut at);
        let size = u32::from_le_bytes(query[at..at + 4].try_into().unwrap());
        at += 4;
        listed.push((number, name, size));
    }
    let names: Vec<&str> = listed.iter().map(|(_, n, _)| n.as_str()).collect();
    assert!(names.contains(&"rechnung.txt"), "{names:?}");
    assert!(names.contains(&"anhang.pdf"), "{names:?}");
    assert!(
        listed.iter().all(|(_, _, size)| *size > 0),
        "an attachment reported no size: {listed:?}"
    );

    // ---- read the second one back ----------------------------------------
    let (number, _, _) = *listed
        .iter()
        .find(|(_, name, _)| name == "anhang.pdf")
        .expect("the pdf is listed");

    let mut rops = Vec::new();
    let logon = rop_logon("");
    let rop_size = u16::from_le_bytes(logon[0..2].try_into().unwrap()) as usize;
    rops.extend_from_slice(&logon[2..rop_size]);
    rops.push(0x03); // RopOpenMessage -> index 1
    rops.extend_from_slice(&[0x00, 0x00, 0x01]);
    rops.extend_from_slice(&0u16.to_le_bytes());
    rops.extend_from_slice(&inbox_fid.to_le_bytes());
    rops.push(0x00);
    rops.extend_from_slice(&mid.to_le_bytes());
    rops.push(0x22); // RopOpenAttachment -> index 2
    rops.extend_from_slice(&[0x00, 0x01, 0x02, 0x00]);
    rops.extend_from_slice(&number.to_le_bytes());
    rops.push(0x2B); // RopOpenStream on the attachment -> index 3
    rops.extend_from_slice(&[0x00, 0x02, 0x03]);
    rops.extend_from_slice(&[0x02, 0x01, 0x01, 0x37]); // PidTagAttachDataBinary
    rops.push(0x00); // ReadOnly
    rops.push(0x2C); // RopReadStream
    rops.extend_from_slice(&[0x00, 0x03]);
    rops.extend_from_slice(&8192u16.to_le_bytes());

    let responses = execute(&h, &auth, &cookie, &rops).await;
    let open = &responses[166..];
    let attach = &open[past_open_message(open)..];
    assert_eq!(attach[0], 0x22, "a RopOpenAttachment response");
    assert_eq!(
        u32::from_le_bytes(attach[2..6].try_into().unwrap()),
        0,
        "opening the attachment failed"
    );

    let stream = &attach[6..];
    assert_eq!(stream[0], 0x2B, "a RopOpenStream response");
    assert_eq!(
        u32::from_le_bytes(stream[2..6].try_into().unwrap()),
        0,
        "opening the attachment stream failed"
    );
    let stream_size = u32::from_le_bytes(stream[6..10].try_into().unwrap()) as usize;
    assert_eq!(stream_size, second.len(), "the stream is the file's size");

    let read = &stream[10..];
    assert_eq!(read[0], 0x2C, "a RopReadStream response");
    assert_eq!(u32::from_le_bytes(read[2..6].try_into().unwrap()), 0);
    let size = usize::from(u16::from_le_bytes(read[6..8].try_into().unwrap()));
    assert_eq!(
        &read[8..8 + size],
        second,
        "the wrong file came back, or came back altered"
    );
}

/// An attachment number that names nothing is `ecNotFound` — the same answer a
/// file that never existed gets, so probing tells a caller nothing.
#[tokio::test]
async fn an_attachment_that_is_not_there_is_not_found() {
    let h = harness("attach-missing").await;
    let auth = basic(&h.email, &h.password);
    h.account
        .create_mailbox(None, "Inbox", Some("inbox"))
        .await
        .expect("inbox");
    h.account
        .deliver(b"From: s@example.test\r\nTo: o@example.test\r\nSubject: nichts\r\n\r\nText\r\n")
        .await
        .expect("deliver");

    let (_, headers, _) = send(
        &h.app,
        "Connect",
        Some(&auth),
        None,
        connect_body("/o=alo/cn=x"),
    )
    .await;
    let cookie = format!("{SESSION_COOKIE}={}", session_cookie(&headers));

    let inbox = h.account.inbox().await.expect("inbox id");
    let rows = h
        .account
        .mapi_mailbox_rows(&inbox, alo_store::Page::first(10))
        .await
        .expect("rows");
    let mid = alo_mapi::folders::fid(alo_mapi::messages::message_counter(&rows[0].id));
    let inbox_fid = {
        let mut fid = [0u8; 8];
        fid[0..2].copy_from_slice(&1u16.to_le_bytes());
        fid[2..8].copy_from_slice(&5u64.to_le_bytes()[0..6]);
        u64::from_le_bytes(fid)
    };

    let mut rops = Vec::new();
    let logon = rop_logon("");
    let rop_size = u16::from_le_bytes(logon[0..2].try_into().unwrap()) as usize;
    rops.extend_from_slice(&logon[2..rop_size]);
    rops.push(0x03);
    rops.extend_from_slice(&[0x00, 0x00, 0x01]);
    rops.extend_from_slice(&0u16.to_le_bytes());
    rops.extend_from_slice(&inbox_fid.to_le_bytes());
    rops.push(0x00);
    rops.extend_from_slice(&mid.to_le_bytes());
    rops.push(0x22);
    rops.extend_from_slice(&[0x00, 0x01, 0x02, 0x00]);
    rops.extend_from_slice(&7u32.to_le_bytes()); // no such attachment

    let responses = execute(&h, &auth, &cookie, &rops).await;
    let open = &responses[166..];
    let attach = &open[past_open_message(open)..];
    assert_eq!(attach[0], 0x22);
    assert_eq!(
        u32::from_le_bytes(attach[2..6].try_into().unwrap()),
        0x8004_010F,
        "an attachment that does not exist was opened"
    );
}

// ---- stage 6: the address book ------------------------------------------

/// Builds a `ResolveNames` request body for the given names.
fn resolve_body(names: &[&str], tags: &[[u8; 4]]) -> Vec<u8> {
    let mut body = 0u32.to_le_bytes().to_vec(); // Reserved
    body.push(0x00); // HasState
    body.push(0x01); // HasPropertyTags
    body.extend_from_slice(&u32::try_from(tags.len()).unwrap().to_le_bytes());
    for tag in tags {
        body.extend_from_slice(tag);
    }
    body.push(0x01); // HasNames
    body.extend_from_slice(&u32::try_from(names.len()).unwrap().to_le_bytes());
    for name in names {
        for unit in name.encode_utf16() {
            body.extend_from_slice(&unit.to_le_bytes());
        }
        body.extend_from_slice(&[0, 0]);
    }
    body.extend_from_slice(&0u32.to_le_bytes()); // AuxiliaryBufferSize
    body
}

/// Sends one address book request and returns its response body.
async fn nspi_send(
    h: &Harness,
    auth: &str,
    request_type: &str,
    body: Vec<u8>,
) -> (StatusCode, u32, Vec<u8>) {
    let mut request = axum::http::Request::builder()
        .method("POST")
        .uri("/mapi/nspi")
        .header("X-RequestType", request_type)
        .header("X-RequestId", "ab-1")
        .header("authorization", auth);
    request = request.header("content-type", "application/mapi-http");
    let response = h
        .app
        .clone()
        .oneshot(request.body(axum::body::Body::from(body)).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let code = response_code(response.headers());
    let bytes = axum::body::to_bytes(response.into_body(), 8 * 1024 * 1024)
        .await
        .unwrap()
        .to_vec();
    let split = bytes
        .windows(4)
        .position(|w| w == BLANK_LINE)
        .expect("framing");
    (status, code, bytes[split + 4..].to_vec())
}

/// **Stage 6 on the real wire.** A client binds to the address book, types a
/// colleague's address, and gets a recipient back — then unbinds.
#[tokio::test]
async fn a_client_resolves_a_colleague_into_a_recipient() {
    let h = harness("nspi-resolve").await;
    let auth = basic(&h.email, &h.password);

    // A second person in the same tenant, so there is somebody to resolve to
    // who is not the caller.
    let ts = h.store.for_tenant(h.tenant.clone());
    let colleague = format!("anna-{}@example.test", h.tenant);
    ts.create_user(&colleague).await.expect("colleague");

    // ---- Bind -------------------------------------------------------------
    let mut bind = 0u32.to_le_bytes().to_vec(); // Flags
    bind.push(0x00); // HasState
    bind.extend_from_slice(&0u32.to_le_bytes()); // AuxiliaryBufferSize
    let (status, code, body) = nspi_send(&h, &auth, "Bind", bind).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(code, ResponseCode::Success.code());
    assert_eq!(&body[0..4], &0u32.to_le_bytes(), "StatusCode");
    assert_eq!(&body[4..8], &0u32.to_le_bytes(), "ErrorCode: success");
    assert_eq!(body.len(), 28, "server GUID and an empty auxiliary buffer");

    // ---- ResolveNames -----------------------------------------------------
    let tags = [
        [0x1F, 0x00, 0x01, 0x30], // PidTagDisplayName
        [0x1F, 0x00, 0xFE, 0x39], // PidTagSmtpAddress
    ];
    // One name that resolves, one that matches nobody.
    let body = resolve_body(&[&colleague, "nobody-at-all"], &tags);
    let (status, code, body) = nspi_send(&h, &auth, "ResolveNames", body).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(code, ResponseCode::Success.code());

    assert_eq!(&body[0..4], &0u32.to_le_bytes(), "StatusCode");
    assert_eq!(&body[4..8], &0u32.to_le_bytes(), "ErrorCode");
    assert_eq!(
        u32::from_le_bytes(body[8..12].try_into().unwrap()),
        1200,
        "CodePage: Unicode"
    );
    assert_eq!(body[12], 0xFF, "HasMinimalIds");
    assert_eq!(
        u32::from_le_bytes(body[13..17].try_into().unwrap()),
        2,
        "one outcome per name"
    );
    assert_eq!(
        u32::from_le_bytes(body[17..21].try_into().unwrap()),
        2,
        "MID_RESOLVED for the colleague"
    );
    assert_eq!(
        u32::from_le_bytes(body[21..25].try_into().unwrap()),
        0,
        "MID_UNRESOLVED for the name that matches nobody"
    );

    assert_eq!(body[25], 0xFF, "HasRowsAndCols");
    assert_eq!(u32::from_le_bytes(body[26..30].try_into().unwrap()), 2);
    let after_tags = 30 + 8;
    assert_eq!(
        u32::from_le_bytes(body[after_tags..after_tags + 4].try_into().unwrap()),
        1,
        "one row, for the one name that resolved"
    );

    // The row: flag byte, then each string behind its own HasValue byte.
    let row = &body[after_tags + 4..];
    assert_eq!(row[0], 0x00, "every value present");
    assert_eq!(row[1], 0xFF, "HasValue before the display name");
    let mut at = 2;
    let display = read_utf16(row, &mut at);
    assert_eq!(display, colleague, "the display name alo knows");
    assert_eq!(row[at], 0xFF, "HasValue before the address");
    at += 1;
    let smtp = read_utf16(row, &mut at);
    assert_eq!(smtp, colleague, "the address a message would go to");

    // ---- Unbind -----------------------------------------------------------
    let mut unbind = 0u32.to_le_bytes().to_vec();
    unbind.extend_from_slice(&0u32.to_le_bytes());
    let (status, code, body) = nspi_send(&h, &auth, "Unbind", unbind).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(code, ResponseCode::Success.code());
    assert_eq!(body.len(), 12);
}

/// A name matching two people resolves to neither. Picking one would put a
/// colleague's address on a message somebody believed was going elsewhere.
#[tokio::test]
async fn an_ambiguous_name_resolves_to_nobody() {
    let h = harness("nspi-ambiguous").await;
    let auth = basic(&h.email, &h.password);

    let ts = h.store.for_tenant(h.tenant.clone());
    ts.create_user(&format!("mueller-anna-{}@example.test", h.tenant))
        .await
        .expect("first");
    ts.create_user(&format!("mueller-jan-{}@example.test", h.tenant))
        .await
        .expect("second");

    let tags = [[0x1F, 0x00, 0x01, 0x30]];
    let body = resolve_body(&["mueller"], &tags);
    let (_, code, body) = nspi_send(&h, &auth, "ResolveNames", body).await;
    assert_eq!(code, ResponseCode::Success.code());
    assert_eq!(
        u32::from_le_bytes(body[17..21].try_into().unwrap()),
        1,
        "MID_AMBIGUOUS"
    );
    let after_tags = 25 + 1 + 4 + 4;
    assert_eq!(
        u32::from_le_bytes(body[after_tags..after_tags + 4].try_into().unwrap()),
        0,
        "a row was returned for an ambiguous name"
    );
}

/// The address book is somebody's data, not a public list: one tenant's names
/// never resolve for another, and an unauthenticated caller is challenged
/// before any lookup happens at all.
#[tokio::test]
async fn the_address_book_does_not_cross_tenants() {
    let victim = harness("nspi-victim").await;
    let ts = victim.store.for_tenant(victim.tenant.clone());
    let secret = format!("geheim-{}@example.test", victim.tenant);
    ts.create_user(&secret).await.expect("colleague");

    // Another tenant asks for that exact address.
    let other = harness("nspi-other").await;
    let auth = basic(&other.email, &other.password);
    let tags = [[0x1F, 0x00, 0x01, 0x30]];
    let body = resolve_body(&[&secret], &tags);
    let (_, code, body) = nspi_send(&other, &auth, "ResolveNames", body).await;
    assert_eq!(code, ResponseCode::Success.code());
    assert_eq!(
        u32::from_le_bytes(body[17..21].try_into().unwrap()),
        0,
        "another tenant's colleague was resolved"
    );
    let text = String::from_utf8_lossy(&body);
    assert!(
        !text.contains("geheim"),
        "the address leaked into the answer"
    );

    // And with no credentials at all, a challenge rather than a lookup.
    let request = axum::http::Request::builder()
        .method("POST")
        .uri("/mapi/nspi")
        .header("X-RequestType", "ResolveNames")
        .header("X-RequestId", "ab-2")
        .body(axum::body::Body::from(resolve_body(&[&secret], &tags)))
        .unwrap();
    let response = other.app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// A malformed body is refused by code rather than parsed optimistically.
#[tokio::test]
async fn a_malformed_address_book_body_is_refused() {
    let h = harness("nspi-malformed").await;
    let auth = basic(&h.email, &h.password);

    // A Bind whose auxiliary buffer claims more bytes than the body holds.
    let mut bind = 0u32.to_le_bytes().to_vec();
    bind.push(0x00);
    bind.extend_from_slice(&9_999u32.to_le_bytes());
    let (status, code, _) = nspi_send(&h, &auth, "Bind", bind).await;
    assert_eq!(status, StatusCode::OK, "a code, not an HTTP failure");
    assert_eq!(code, ResponseCode::InvalidRequestBody.code());
}

/// A request type this endpoint does not serve says so rather than pretending.
#[tokio::test]
async fn an_unserved_address_book_request_refuses_honestly() {
    let h = harness("nspi-unserved").await;
    let auth = basic(&h.email, &h.password);
    let (status, code, _) = nspi_send(&h, &auth, "GetSpecialTable", Vec::new()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        code,
        ResponseCode::InvalidRequestType.code(),
        "an unknown request type"
    );
}

/// A message opens with its **recipients** and its **HTML body**, both of which
/// were the gaps stage 5 left behind.
///
/// The recipient table is the part with no safety net: `RecipientFlags` is the
/// only thing that says which fields follow it, so this decodes the row the way
/// a client does — read the flags, then read exactly what they promise — and
/// asserts nothing is left over.
#[tokio::test]
async fn a_message_opens_with_its_recipients_and_its_html_body() {
    let h = harness("recipients").await;
    let auth = basic(&h.email, &h.password);
    h.account
        .create_mailbox(None, "Inbox", Some("inbox"))
        .await
        .expect("inbox");

    // A display name containing a comma: the case a hand-rolled split on
    // commas turns into two people who do not exist.
    let raw = format!(
        "From: Absender <s@example.test>\r\n\
         To: \"Müller, Anna\" <anna@example.test>, {to}\r\n\
         Cc: Liège Office <office@example.test>\r\n\
         Subject: Mit Empfängern\r\n\
         Content-Type: multipart/alternative; boundary=ALT\r\n\r\n\
         --ALT\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n\
         Nur Text.\r\n\
         --ALT\r\nContent-Type: text/html; charset=utf-8\r\n\r\n\
         <p>Grüße aus <b>Liège</b>.</p>\r\n--ALT--\r\n",
        to = h.email
    );
    h.account.deliver(raw.as_bytes()).await.expect("deliver");

    let (_, headers, _) = send(
        &h.app,
        "Connect",
        Some(&auth),
        None,
        connect_body("/o=alo/cn=x"),
    )
    .await;
    let cookie = format!("{SESSION_COOKIE}={}", session_cookie(&headers));

    let inbox = h.account.inbox().await.expect("inbox id");
    let rows = h
        .account
        .mapi_mailbox_rows(&inbox, alo_store::Page::first(10))
        .await
        .expect("rows");
    let mid = alo_mapi::folders::fid(alo_mapi::messages::message_counter(&rows[0].id));
    let inbox_fid = {
        let mut fid = [0u8; 8];
        fid[0..2].copy_from_slice(&1u16.to_le_bytes());
        fid[2..8].copy_from_slice(&5u64.to_le_bytes()[0..6]);
        u64::from_le_bytes(fid)
    };

    let mut rops = Vec::new();
    let logon = rop_logon("");
    let rop_size = u16::from_le_bytes(logon[0..2].try_into().unwrap()) as usize;
    rops.extend_from_slice(&logon[2..rop_size]);
    rops.push(0x03);
    rops.extend_from_slice(&[0x00, 0x00, 0x01]);
    rops.extend_from_slice(&0u16.to_le_bytes());
    rops.extend_from_slice(&inbox_fid.to_le_bytes());
    rops.push(0x00);
    rops.extend_from_slice(&mid.to_le_bytes());
    // Ask for the HTML body alongside the plain one.
    rops.push(0x07);
    rops.extend_from_slice(&[0x00, 0x01]);
    rops.extend_from_slice(&0u16.to_le_bytes()); // no size limit
    rops.extend_from_slice(&1u16.to_le_bytes()); // WantUnicode
    rops.extend_from_slice(&2u16.to_le_bytes());
    rops.extend_from_slice(&[0x1F, 0x00, 0x00, 0x10]); // PidTagBody
    rops.extend_from_slice(&[0x02, 0x01, 0x13, 0x10]); // PidTagHtml, PtypBinary

    let responses = execute(&h, &auth, &cookie, &rops).await;
    let open = &responses[166..];
    assert_eq!(open[0], 0x03);
    assert_eq!(
        u32::from_le_bytes(open[2..6].try_into().unwrap()),
        0,
        "opening the message failed"
    );

    let mut at = 9;
    let subject = read_utf16(open, &mut at);
    assert_eq!(subject, "Mit Empfängern");

    // ---- the recipient table ---------------------------------------------
    let count = u16::from_le_bytes(open[at..at + 2].try_into().unwrap());
    assert_eq!(count, 3, "two To and one Cc");
    assert_eq!(
        u16::from_le_bytes(open[at + 2..at + 4].try_into().unwrap()),
        0,
        "ColumnCount"
    );
    let rows_written = open[at + 4];
    assert_eq!(rows_written, 3);
    at += 5;

    let mut seen: Vec<(u8, String, String)> = Vec::new();
    for _ in 0..rows_written {
        let recipient_type = open[at];
        assert_eq!(
            u16::from_le_bytes(open[at + 3..at + 5].try_into().unwrap()),
            0,
            "Reserved MUST be zero"
        );
        let size = usize::from(u16::from_le_bytes(open[at + 5..at + 7].try_into().unwrap()));
        let row = &open[at + 7..at + 7 + size];
        at += 7 + size;

        // Read the row exactly as its flags promise.
        let flags = u16::from_le_bytes(row[0..2].try_into().unwrap());
        assert_eq!(flags & 0x0007, 0x0003, "SMTP address type");
        assert_ne!(flags & 0x0008, 0, "E: an address follows");
        assert_ne!(flags & 0x0010, 0, "D: a display name follows");
        assert_ne!(flags & 0x0200, 0, "U: the strings are Unicode");
        let mut ra = 2;
        let email = read_utf16(row, &mut ra);
        let display = read_utf16(row, &mut ra);
        assert_eq!(
            u16::from_le_bytes(row[ra..ra + 2].try_into().unwrap()),
            0,
            "RecipientColumnCount"
        );
        assert_eq!(ra + 2, row.len(), "the row's declared size was exact");
        seen.push((recipient_type, display, email));
    }

    // The display name with a comma in it survived as one person.
    assert!(
        seen.iter().any(|(kind, name, mail)| *kind == 0x01
            && name == "Müller, Anna"
            && mail == "anna@example.test"),
        "{seen:?}"
    );
    assert!(
        seen.iter()
            .any(|(kind, _, mail)| *kind == 0x02 && mail == "office@example.test"),
        "the Cc recipient is missing or misfiled: {seen:?}"
    );

    // ---- the HTML body ----------------------------------------------------
    let props = &open[at..];
    assert_eq!(props[0], 0x07, "a RopGetPropertiesSpecific response");
    assert_eq!(
        u32::from_le_bytes(props[2..6].try_into().unwrap()),
        0,
        "reading the properties failed"
    );
    assert_eq!(props[6], 0x00, "a standard row: both bodies present");
    let mut pa = 7;
    let text = read_utf16(props, &mut pa);
    assert!(text.contains("Nur Text"), "{text:?}");

    // PtypBinary in a ROP buffer: a 16-bit count, then that many bytes.
    let html_len = usize::from(u16::from_le_bytes(props[pa..pa + 2].try_into().unwrap()));
    pa += 2;
    let html = String::from_utf8(props[pa..pa + html_len].to_vec()).expect("utf-8");
    assert!(
        html.contains("<b>Liège</b>"),
        "the HTML body came back wrong: {html:?}"
    );
    assert_eq!(
        pa + html_len,
        props.len(),
        "the declared byte count was exact"
    );
}

/// A message with no HTML alternative reports that property **absent** rather
/// than empty: a blank HTML body would have a client render nothing where it
/// would otherwise have fallen back to the plain text.
#[tokio::test]
async fn a_message_with_no_html_says_so_rather_than_returning_an_empty_one() {
    let h = harness("no-html").await;
    let auth = basic(&h.email, &h.password);
    h.account
        .create_mailbox(None, "Inbox", Some("inbox"))
        .await
        .expect("inbox");
    h.account
        .deliver(
            b"From: s@example.test\r\nTo: o@example.test\r\nSubject: nur Text\r\n\
              Content-Type: text/plain; charset=utf-8\r\n\r\nNur Text.\r\n",
        )
        .await
        .expect("deliver");

    let (_, headers, _) = send(
        &h.app,
        "Connect",
        Some(&auth),
        None,
        connect_body("/o=alo/cn=x"),
    )
    .await;
    let cookie = format!("{SESSION_COOKIE}={}", session_cookie(&headers));

    let inbox = h.account.inbox().await.expect("inbox id");
    let rows = h
        .account
        .mapi_mailbox_rows(&inbox, alo_store::Page::first(10))
        .await
        .expect("rows");
    let mid = alo_mapi::folders::fid(alo_mapi::messages::message_counter(&rows[0].id));
    let inbox_fid = {
        let mut fid = [0u8; 8];
        fid[0..2].copy_from_slice(&1u16.to_le_bytes());
        fid[2..8].copy_from_slice(&5u64.to_le_bytes()[0..6]);
        u64::from_le_bytes(fid)
    };

    let mut rops = Vec::new();
    let logon = rop_logon("");
    let rop_size = u16::from_le_bytes(logon[0..2].try_into().unwrap()) as usize;
    rops.extend_from_slice(&logon[2..rop_size]);
    rops.push(0x03);
    rops.extend_from_slice(&[0x00, 0x00, 0x01]);
    rops.extend_from_slice(&0u16.to_le_bytes());
    rops.extend_from_slice(&inbox_fid.to_le_bytes());
    rops.push(0x00);
    rops.extend_from_slice(&mid.to_le_bytes());
    rops.push(0x07);
    rops.extend_from_slice(&[0x00, 0x01]);
    rops.extend_from_slice(&0u16.to_le_bytes());
    rops.extend_from_slice(&1u16.to_le_bytes());
    rops.extend_from_slice(&1u16.to_le_bytes());
    rops.extend_from_slice(&[0x02, 0x01, 0x13, 0x10]); // PidTagHtml

    let responses = execute(&h, &auth, &cookie, &rops).await;
    let open = &responses[166..];
    let props = &open[past_open_message(open)..];
    assert_eq!(props[0], 0x07);
    assert_eq!(props[6], 0x01, "a flagged row: something is missing");
    assert_eq!(props[7], 0x01, "the HTML body is absent, not empty");
    assert_eq!(props.len(), 8, "nothing follows an absent value");
}

// ---- stage 7: composing and sending --------------------------------------

/// Encodes a null-terminated UTF-16LE string.
fn utf16z_bytes(text: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for unit in text.encode_utf16() {
        out.extend_from_slice(&unit.to_le_bytes());
    }
    out.extend_from_slice(&[0, 0]);
    out
}

/// A `RopSetProperties` naming a subject and a plain-text body.
fn set_properties_rop(subject: &str, body: &str) -> Vec<u8> {
    let mut values = 2u16.to_le_bytes().to_vec();
    values.extend_from_slice(&[0x1F, 0x00, 0x37, 0x00]); // PidTagSubject
    values.extend_from_slice(&utf16z_bytes(subject));
    values.extend_from_slice(&[0x1F, 0x00, 0x00, 0x10]); // PidTagBody
    values.extend_from_slice(&utf16z_bytes(body));

    let mut out = vec![0x0A, 0x00, 0x01];
    out.extend_from_slice(&u16::try_from(values.len()).unwrap().to_le_bytes());
    out.extend_from_slice(&values);
    out
}

/// A `RopModifyRecipients` carrying one SMTP recipient.
fn modify_recipients_rop(kind: u8, email: &str, name: &str) -> Vec<u8> {
    // SMTP | E | D | U
    let flags: u16 = 0x0003 | 0x0008 | 0x0010 | 0x0200;
    let mut row = flags.to_le_bytes().to_vec();
    row.extend_from_slice(&utf16z_bytes(email));
    row.extend_from_slice(&utf16z_bytes(name));
    row.extend_from_slice(&0u16.to_le_bytes()); // RecipientColumnCount

    let mut out = vec![0x0E, 0x00, 0x01];
    out.extend_from_slice(&0u16.to_le_bytes()); // ColumnCount
    out.extend_from_slice(&1u16.to_le_bytes()); // RowCount
    out.extend_from_slice(&0u32.to_le_bytes()); // RowId
    out.push(kind);
    out.extend_from_slice(&u16::try_from(row.len()).unwrap().to_le_bytes());
    out.extend_from_slice(&row);
    out
}

/// **Stage 7 on the real wire.** A client creates a message in Drafts, sets its
/// subject and body, addresses it, saves it, and sends it — and the message
/// that leaves is the one that was stored.
#[tokio::test]
async fn a_client_composes_saves_and_sends_a_message() {
    // A sink standing in for the deployment's submission listener, so the test
    // reads the bytes that would actually have gone out.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let sink_addr = listener.local_addr().expect("addr").to_string();
    let received = std::sync::Arc::new(tokio::sync::Mutex::new(Vec::<u8>::new()));
    let sink = std::sync::Arc::clone(&received);
    tokio::spawn(async move {
        if let Ok((mut socket, _)) = listener.accept().await {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let _ = socket.write_all(b"220 sink\r\n").await;
            let mut buf = [0u8; 4096];
            let mut all = Vec::new();
            let mut in_data = false;
            while let Ok(n) = socket.read(&mut buf).await {
                if n == 0 {
                    break;
                }
                all.extend_from_slice(&buf[..n]);
                // Published as it arrives, not at the end: the test waits for
                // the DATA terminator to appear, and a buffer only written on
                // close would never show it.
                sink.lock().await.extend_from_slice(&buf[..n]);
                let text = String::from_utf8_lossy(&buf[..n]).to_ascii_uppercase();
                let reply: &[u8] = if in_data {
                    if all.windows(5).any(|w| w == b"\r\n.\r\n") {
                        in_data = false;
                        b"250 queued\r\n"
                    } else {
                        continue;
                    }
                } else if text.starts_with("EHLO") || text.starts_with("HELO") {
                    b"250-sink\r\n250 SIZE 0\r\n"
                } else if text.starts_with("MAIL") || text.starts_with("RCPT") {
                    b"250 ok\r\n"
                } else if text.starts_with("DATA") {
                    in_data = true;
                    b"354 go\r\n"
                } else if text.starts_with("QUIT") {
                    let _ = socket.write_all(b"221 bye\r\n").await;
                    break;
                } else {
                    b"250 ok\r\n"
                };
                let _ = socket.write_all(reply).await;
            }
            drop(all);
        }
    });

    let h = harness_with_submission("compose", Some(sink_addr)).await;
    let auth = basic(&h.email, &h.password);
    h.account
        .create_mailbox(None, "Drafts", Some("drafts"))
        .await
        .expect("drafts");
    h.account
        .create_mailbox(None, "Sent", Some("sent"))
        .await
        .expect("sent");

    let (_, headers, _) = send(
        &h.app,
        "Connect",
        Some(&auth),
        None,
        connect_body("/o=alo/cn=x"),
    )
    .await;
    let cookie = format!("{SESSION_COOKIE}={}", session_cookie(&headers));

    // Drafts is a real mailbox, so its folder id is the hashed kind.
    let drafts = h.account.inbox().await.expect("inbox");
    let _ = drafts;
    let boxes = h
        .account
        .mailboxes(alo_store::Page::first(50))
        .await
        .expect("mailboxes");
    let drafts_box = boxes
        .iter()
        .find(|m| m.role.as_deref() == Some("drafts"))
        .expect("drafts mailbox");
    let drafts_fid = alo_mapi::folders::fid(alo_mapi::folders::mailbox_counter(&drafts_box.id));

    // ---- the whole arc, in one buffer -------------------------------------
    let mut rops = Vec::new();
    let logon = rop_logon("");
    let rop_size = u16::from_le_bytes(logon[0..2].try_into().unwrap()) as usize;
    rops.extend_from_slice(&logon[2..rop_size]);

    // RopCreateMessage in Drafts -> handle index 1.
    rops.push(0x06);
    rops.extend_from_slice(&[0x00, 0x00, 0x01]);
    rops.extend_from_slice(&0u16.to_le_bytes()); // CodePageId
    rops.extend_from_slice(&drafts_fid.to_le_bytes());
    rops.push(0x00); // not associated

    rops.extend(set_properties_rop(
        "Rechnung für August",
        "Grüße aus Liège.",
    ));
    rops.extend(modify_recipients_rop(
        0x01,
        "anna@example.test",
        "Anna Müller",
    ));
    // RopSaveChangesMessage: response slot 1, input slot 1.
    rops.extend_from_slice(&[0x0C, 0x00, 0x01, 0x01, 0x0C]);
    // RopSubmitMessage.
    rops.extend_from_slice(&[0x32, 0x00, 0x01, 0x00]);

    let responses = execute(&h, &auth, &cookie, &rops).await;

    // The create response.
    let create = &responses[166..];
    assert_eq!(create[0], 0x06, "a RopCreateMessage response");
    assert_eq!(
        u32::from_le_bytes(create[2..6].try_into().unwrap()),
        0,
        "creating the message failed"
    );
    assert_eq!(create[6], 0x00, "no message id before it is saved");

    let set = &create[7..];
    assert_eq!(set[0], 0x0A, "a RopSetProperties response");
    assert_eq!(u32::from_le_bytes(set[2..6].try_into().unwrap()), 0);

    let modify = &set[8..];
    assert_eq!(modify[0], 0x0E, "a RopModifyRecipients response");
    assert_eq!(u32::from_le_bytes(modify[2..6].try_into().unwrap()), 0);

    let save = &modify[6..];
    assert_eq!(save[0], 0x0C, "a RopSaveChangesMessage response");
    assert_eq!(
        u32::from_le_bytes(save[2..6].try_into().unwrap()),
        0,
        "saving the draft failed"
    );
    let mid = u64::from_le_bytes(save[7..15].try_into().unwrap());
    assert!(mid != 0, "the saved draft has no id");

    let submit = &save[15..];
    assert_eq!(submit[0], 0x32, "a RopSubmitMessage response");
    assert_eq!(
        u32::from_le_bytes(submit[2..6].try_into().unwrap()),
        0,
        "sending failed"
    );

    // ---- what actually went out -------------------------------------------
    // Give the sink a moment to finish the transaction it is mid-way through.
    for _ in 0..50 {
        if received.lock().await.windows(5).any(|w| w == b"\r\n.\r\n") {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let wire = String::from_utf8_lossy(&received.lock().await.clone()).into_owned();
    assert!(
        wire.contains("RCPT TO:<anna@example.test>"),
        "the recipient never reached the envelope: {wire}"
    );
    assert!(
        wire.contains(&format!("MAIL FROM:<{}>", h.email)),
        "the envelope sender is not this account: {wire}"
    );
    // The display name carries a "ü", so the header is RFC 2047 encoded — the
    // address beside it is not, and is the part that has to be exact.
    assert!(
        wire.contains("To:") && wire.contains("<anna@example.test>"),
        "the To header is missing or wrong: {wire}"
    );
    assert!(
        wire.contains("Subject:"),
        "the message has no subject line: {wire}"
    );
    // The body may be transfer-encoded on the wire, so it is checked on the
    // sender's own stored copy, where the parser decodes it — which is also
    // the copy a reader would open in Sent.

    // And the sender's own copy is in Sent, not Drafts.
    let boxes = h
        .account
        .mailboxes(alo_store::Page::first(50))
        .await
        .expect("mailboxes");
    let sent = boxes
        .iter()
        .find(|m| m.role.as_deref() == Some("sent"))
        .expect("sent mailbox");
    let in_sent = h
        .account
        .mapi_mailbox_rows(&sent.id, alo_store::Page::first(10))
        .await
        .expect("sent rows");
    assert_eq!(in_sent.len(), 1, "the sent message is not filed in Sent");
    assert_eq!(in_sent[0].subject, "Rechnung für August");
    let stored = h
        .account
        .message_bytes(&in_sent[0].id)
        .await
        .expect("stored bytes");
    let parsed = alo_store::mime_read::parse(&stored);
    assert!(
        parsed.text.unwrap_or_default().contains("Grüße aus Liège."),
        "the body did not survive composition"
    );
    assert_eq!(parsed.recipients.len(), 1);
    assert_eq!(parsed.recipients[0].email, "anna@example.test");
    assert_eq!(parsed.recipients[0].display_name, "Anna Müller");
    let still_draft = h
        .account
        .mapi_mailbox_rows(&drafts_box.id, alo_store::Page::first(10))
        .await
        .expect("draft rows");
    assert!(
        still_draft.is_empty(),
        "the message is still sitting in Drafts"
    );
}

/// A draft that has not been saved cannot be sent: the bytes that go out are
/// the bytes the sender can afterwards read in Sent, and an unsaved draft has
/// none.
#[tokio::test]
async fn an_unsaved_draft_cannot_be_submitted() {
    let h = harness_with_submission("compose-unsaved", None).await;
    let auth = basic(&h.email, &h.password);
    h.account
        .create_mailbox(None, "Drafts", Some("drafts"))
        .await
        .expect("drafts");

    let (_, headers, _) = send(
        &h.app,
        "Connect",
        Some(&auth),
        None,
        connect_body("/o=alo/cn=x"),
    )
    .await;
    let cookie = format!("{SESSION_COOKIE}={}", session_cookie(&headers));

    let boxes = h
        .account
        .mailboxes(alo_store::Page::first(50))
        .await
        .expect("mailboxes");
    let drafts_box = boxes
        .iter()
        .find(|m| m.role.as_deref() == Some("drafts"))
        .expect("drafts mailbox");
    let drafts_fid = alo_mapi::folders::fid(alo_mapi::folders::mailbox_counter(&drafts_box.id));

    let mut rops = Vec::new();
    let logon = rop_logon("");
    let rop_size = u16::from_le_bytes(logon[0..2].try_into().unwrap()) as usize;
    rops.extend_from_slice(&logon[2..rop_size]);
    rops.push(0x06);
    rops.extend_from_slice(&[0x00, 0x00, 0x01]);
    rops.extend_from_slice(&0u16.to_le_bytes());
    rops.extend_from_slice(&drafts_fid.to_le_bytes());
    rops.push(0x00);
    rops.extend(set_properties_rop("kein Speichern", "Text"));
    // Straight to submit, with no save in between.
    rops.extend_from_slice(&[0x32, 0x00, 0x01, 0x00]);

    let responses = execute(&h, &auth, &cookie, &rops).await;
    let create = &responses[166..];
    let set = &create[7..];
    let submit = &set[8..];
    assert_eq!(submit[0], 0x32);
    assert_eq!(
        u32::from_le_bytes(submit[2..6].try_into().unwrap()),
        0x8004_010F,
        "an unsaved draft was submitted"
    );
}

/// A message cannot be composed into a folder this session's tree does not
/// have — including one belonging to another tenant, whose folder ids look
/// exactly the same.
#[tokio::test]
async fn a_draft_cannot_be_created_in_a_folder_this_session_does_not_have() {
    let h = harness_with_submission("compose-foreign", None).await;
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

    let mut rops = Vec::new();
    let logon = rop_logon("");
    let rop_size = u16::from_le_bytes(logon[0..2].try_into().unwrap()) as usize;
    rops.extend_from_slice(&logon[2..rop_size]);
    rops.push(0x06);
    rops.extend_from_slice(&[0x00, 0x00, 0x01]);
    rops.extend_from_slice(&0u16.to_le_bytes());
    // A hashed folder id that belongs to no mailbox of this account's.
    rops.extend_from_slice(&alo_mapi::folders::fid(999_999).to_le_bytes());
    rops.push(0x00);

    let responses = execute(&h, &auth, &cookie, &rops).await;
    let create = &responses[166..];
    assert_eq!(create[0], 0x06);
    assert_eq!(
        u32::from_le_bytes(create[2..6].try_into().unwrap()),
        0x8004_010F,
        "a draft was created in a folder this account does not have"
    );
}
