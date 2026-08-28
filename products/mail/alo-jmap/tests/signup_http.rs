//! Personal signup HTTP surface (ADR 0018, slice 3): the pending-signup store
//! round-trip, availability reporting, and the verify → provision path with a
//! correct code (and its refusal + attempt cap on a wrong one), all driven
//! through the real router.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::common::{harness, send};
use alo_identity::{Identity, secret};
use alo_store::Store;
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};

/// A router whose AppState offers `domains` for personal signup and has no
/// submission listener (so `begin`'s send step is exercised as unavailable).
fn signup_app(store: Arc<Store>, identity: Identity, domains: Vec<String>) -> Router {
    use alo_jmap::push::PushHub;
    use alo_jmap::state::{AppState, Limits};
    alo_jmap::app(AppState {
        media: None,
        turns: Default::default(),
        store,
        identity,
        push: PushHub::new(),
        limits: Limits::default(),
        base_url: "http://test".into(),
        submission_addr: None,
        // No extra front-end hosts in a harness: the session advertises the
        // configured base, which is what these tests assert against.
        session_origins: Vec::new(),
        web_push: None,
        junk_learner: None,
        personal_domains: domains,
        signup_limiter: alo_identity::ratelimit::RateLimiter::new(),
    })
}

/// A personal domain unique to this run (the global login-username index means
/// `localpart@domain` must not collide across reruns of the shared test DB).
fn unique_domain(tag: &str) -> String {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{tag}{n}.alomails.test")
}

fn post(uri: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

/// Mirrors the route's address-salted code hashing so a test can seed a
/// pending signup with a known code.
fn code_hash(address: &str, code: &str) -> String {
    secret::hash_at_rest(&format!("{address}:{code}"))
}

#[tokio::test]
async fn pending_signup_store_roundtrip() {
    let h = harness("signup-store").await;
    let domain = unique_domain("store");
    let address = format!("john@{domain}");

    h.store
        .upsert_pending_signup(
            &address,
            "john@gmail.test",
            &code_hash(&address, "123456"),
            600,
        )
        .await
        .unwrap();
    let pending = h.store.pending_signup(&address).await.unwrap().unwrap();
    assert_eq!(pending.recovery_email, "john@gmail.test");
    assert_eq!(pending.attempts, 0);

    assert_eq!(h.store.bump_signup_attempts(&address).await.unwrap(), 1);
    assert_eq!(h.store.bump_signup_attempts(&address).await.unwrap(), 2);

    h.store.delete_pending_signup(&address).await.unwrap();
    assert!(h.store.pending_signup(&address).await.unwrap().is_none());
}

#[tokio::test]
async fn domains_lists_configured_and_empty_when_off() {
    let h = harness("signup-domains").await;
    let domain = unique_domain("dom");

    // Configured: the domain is listed.
    let on = signup_app(
        Arc::clone(&h.store),
        h.identity.clone(),
        vec![domain.clone()],
    );
    let req = Request::builder()
        .method("GET")
        .uri("/signup/domains")
        .body(Body::empty())
        .unwrap();
    let (s, body) = send(&on, req).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["domains"][0], domain);

    // Dormant (no domains): an empty list, so the UI hides signup.
    let off = signup_app(Arc::clone(&h.store), h.identity.clone(), Vec::new());
    let req = Request::builder()
        .method("GET")
        .uri("/signup/domains")
        .body(Body::empty())
        .unwrap();
    let (_s, body) = send(&off, req).await;
    assert_eq!(body["domains"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn available_reports_status() {
    let h = harness("signup-avail").await;
    let domain = unique_domain("avail");
    let app = signup_app(
        Arc::clone(&h.store),
        h.identity.clone(),
        vec![domain.clone()],
    );

    // A fresh, valid personal address is available.
    let (s, body) = send(
        &app,
        post(
            "/signup/available",
            serde_json::json!({"address": format!("freename@{domain}")}),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["available"], true);
    assert_eq!(body["reason"], "ok");

    // A reserved localpart is not.
    let (_s, body) = send(
        &app,
        post(
            "/signup/available",
            serde_json::json!({"address": format!("postmaster@{domain}")}),
        ),
    )
    .await;
    assert_eq!(body["available"], false);
    assert_eq!(body["reason"], "reserved");

    // A domain we don't offer is not.
    let (_s, body) = send(
        &app,
        post(
            "/signup/available",
            serde_json::json!({"address": "someone@notoffered.test"}),
        ),
    )
    .await;
    assert_eq!(body["reason"], "unavailable_domain");

    // An address already provisioned is taken.
    h.identity
        .provision_personal(
            &domain,
            "occupied",
            "correct-horse-battery",
            "recover@example.test",
        )
        .await
        .unwrap();
    let (_s, body) = send(
        &app,
        post(
            "/signup/available",
            serde_json::json!({"address": format!("occupied@{domain}")}),
        ),
    )
    .await;
    assert_eq!(body["reason"], "taken");
}

#[tokio::test]
async fn verify_provisions_after_correct_code() {
    let h = harness("signup-verify").await;
    let domain = unique_domain("verify");
    let app = signup_app(
        Arc::clone(&h.store),
        h.identity.clone(),
        vec![domain.clone()],
    );
    let address = format!("newuser@{domain}");

    // Seed a pending signup as `begin` would (bypassing the email send).
    h.store
        .upsert_pending_signup(
            &address,
            "me@gmail.test",
            &code_hash(&address, "424242"),
            600,
        )
        .await
        .unwrap();

    // Wrong code is refused …
    let (s, _b) = send(
        &app,
        post(
            "/signup/verify",
            serde_json::json!({
                "address": address, "code": "000000", "password": "correct-horse-battery"
            }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // … the correct code provisions the account and clears the pending row.
    let (s, body) = send(
        &app,
        post(
            "/signup/verify",
            serde_json::json!({
                "address": address, "code": "424242", "password": "correct-horse-battery"
            }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(body["email"], address);

    let resolved = h.store.account_by_email(&address).await.unwrap();
    assert!(resolved.is_some(), "account provisioned and resolvable");
    assert!(
        h.store.pending_signup(&address).await.unwrap().is_none(),
        "pending cleared"
    );
}

#[tokio::test]
async fn verify_caps_attempts_then_burns_the_signup() {
    let h = harness("signup-cap").await;
    let domain = unique_domain("cap");
    let app = signup_app(
        Arc::clone(&h.store),
        h.identity.clone(),
        vec![domain.clone()],
    );
    let address = format!("bruteme@{domain}");
    h.store
        .upsert_pending_signup(
            &address,
            "me@gmail.test",
            &code_hash(&address, "999999"),
            600,
        )
        .await
        .unwrap();

    // Hammer wrong codes; each is a 400 until the cap trips a hard 429 and the
    // pending row is destroyed so the short code can't be ground down further.
    let mut saw_stop = false;
    for _ in 0..10 {
        let (s, _b) = send(
            &app,
            post(
                "/signup/verify",
                serde_json::json!({
                    "address": address, "code": "000000", "password": "correct-horse-battery"
                }),
            ),
        )
        .await;
        if s == StatusCode::TOO_MANY_REQUESTS {
            saw_stop = true;
            break;
        }
        assert_eq!(s, StatusCode::BAD_REQUEST, "wrong code before the cap");
    }
    assert!(saw_stop, "the attempt cap tripped a 429");
    assert!(
        h.store.pending_signup(&address).await.unwrap().is_none(),
        "signup burned"
    );
}

#[tokio::test]
async fn begin_refuses_bad_domain_and_reserved_before_sending() {
    let h = harness("signup-begin").await;
    let domain = unique_domain("begin");
    let app = signup_app(
        Arc::clone(&h.store),
        h.identity.clone(),
        vec![domain.clone()],
    );

    // A domain we don't offer → 400, no pending row.
    let (s, _b) = send(
        &app,
        post(
            "/signup/begin",
            serde_json::json!({
                "address": "x@notoffered.test", "recoveryEmail": "r@gmail.test"
            }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // A reserved localpart → 400.
    let (s, _b) = send(
        &app,
        post(
            "/signup/begin",
            serde_json::json!({
                "address": format!("admin@{domain}"), "recoveryEmail": "r@gmail.test"
            }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // A valid address with no submission listener configured → 503, but the
    // pending row is stored so a retry can resend once sending is available.
    let addr = format!("realperson@{domain}");
    let (s, _b) = send(
        &app,
        post(
            "/signup/begin",
            serde_json::json!({
                "address": addr, "recoveryEmail": "r@gmail.test"
            }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        h.store.pending_signup(&addr).await.unwrap().is_some(),
        "pending stored for resend"
    );
}
