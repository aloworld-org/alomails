//! Self-service password-reset HTTP surface (ADR 0018 follow-up): the
//! recovery-email + pending-reset store round-trips, the enumeration-safe
//! request step, and the verify → re-hash path with a correct code (and its
//! refusal + attempt cap on a wrong one), driven through the real router.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use alo_identity::{Identity, secret};
use alo_store::Store;
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use common::{harness, send};

fn reset_app(store: Arc<Store>, identity: Identity, domains: Vec<String>) -> Router {
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
        // No submission listener: `request` for a KNOWN address reaches the
        // send step and reports it unavailable (503), which still proves the
        // pending reset was stored first.
        submission_addr: None,
        // No extra front-end hosts in a harness: the session advertises the
        // configured base, which is what these tests assert against.
        session_origins: Vec::new(),
        mapi_http: false,
        junk_learner: None,
        personal_domains: domains,
        signup_limiter: alo_identity::ratelimit::RateLimiter::new(),
    })
}

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

/// Mirrors the route's address-salted code hashing so a test can seed a pending
/// reset with a known code.
fn code_hash(address: &str, code: &str) -> String {
    secret::hash_at_rest(&format!("{address}:{code}"))
}

#[tokio::test]
async fn recovery_and_pending_reset_store_roundtrip() {
    let h = harness("reset-store").await;
    let domain = unique_domain("store");
    let address = format!("john@{domain}");

    // account_recovery
    h.store
        .set_account_recovery(&address, "t1", "u1", "john@gmail.test")
        .await
        .unwrap();
    assert_eq!(
        h.store.account_recovery_email(&address).await.unwrap(),
        Some("john@gmail.test".to_owned())
    );
    assert!(
        h.store
            .account_recovery_email(&format!("nobody@{domain}"))
            .await
            .unwrap()
            .is_none()
    );

    // pending_resets
    h.store
        .upsert_pending_reset(
            &address,
            "john@gmail.test",
            &code_hash(&address, "123456"),
            600,
        )
        .await
        .unwrap();
    let pending = h.store.pending_reset(&address).await.unwrap().unwrap();
    assert_eq!(pending.recovery_email, "john@gmail.test");
    assert_eq!(pending.attempts, 0);
    assert_eq!(h.store.bump_reset_attempts(&address).await.unwrap(), 1);
    h.store.delete_pending_reset(&address).await.unwrap();
    assert!(h.store.pending_reset(&address).await.unwrap().is_none());
}

#[tokio::test]
async fn request_is_enumeration_safe() {
    let h = harness("reset-request").await;
    let domain = unique_domain("req");
    let app = reset_app(
        Arc::clone(&h.store),
        h.identity.clone(),
        vec![domain.clone()],
    );

    // Unknown address (no account, no recovery) → a silent "sent", and no
    // pending row is created (nothing to leak).
    let unknown = format!("ghost@{domain}");
    let (s, body) = send(
        &app,
        post("/reset/request", serde_json::json!({"address": unknown})),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(body["status"], "sent");
    assert!(h.store.pending_reset(&unknown).await.unwrap().is_none());

    // A non-personal domain is likewise a silent no-op.
    let (s, body) = send(
        &app,
        post(
            "/reset/request",
            serde_json::json!({"address": "x@notoffered.test"}),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["status"], "sent");

    // A KNOWN address (provisioned → recovery on file) reaches the send step;
    // with no submission listener that is 503, but the pending reset is stored
    // first so a retry can resend.
    h.identity
        .provision_personal(&domain, "realuser", "correct-horse-battery", "r@gmail.test")
        .await
        .unwrap();
    let address = format!("realuser@{domain}");
    let (s, _b) = send(
        &app,
        post("/reset/request", serde_json::json!({"address": address})),
    )
    .await;
    assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        h.store.pending_reset(&address).await.unwrap().is_some(),
        "pending reset stored before the send attempt"
    );
}

#[tokio::test]
async fn verify_resets_password_after_correct_code() {
    let h = harness("reset-verify").await;
    let domain = unique_domain("verify");
    let app = reset_app(
        Arc::clone(&h.store),
        h.identity.clone(),
        vec![domain.clone()],
    );
    let address = format!("resetme@{domain}");

    h.identity
        .provision_personal(&domain, "resetme", "old-password-xyz", "r@gmail.test")
        .await
        .unwrap();
    // Seed a pending reset as `request` would (bypassing the email send).
    h.store
        .upsert_pending_reset(
            &address,
            "r@gmail.test",
            &code_hash(&address, "424242"),
            600,
        )
        .await
        .unwrap();

    // Wrong code is refused.
    let (s, _b) = send(
        &app,
        post(
            "/reset/verify",
            serde_json::json!({"address": address, "code": "000000", "password": "brand-new-password"}),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // Correct code sets the new password and clears the pending row.
    let (s, body) = send(
        &app,
        post(
            "/reset/verify",
            serde_json::json!({"address": address, "code": "424242", "password": "brand-new-password"}),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(body["status"], "ok");
    assert!(
        h.store.pending_reset(&address).await.unwrap().is_none(),
        "pending cleared"
    );

    // The new password authenticates; the old one no longer does.
    assert!(
        h.identity
            .authenticate_password(&address, "brand-new-password")
            .await
            .unwrap()
            .is_some(),
        "new password works"
    );
    assert!(
        h.identity
            .authenticate_password(&address, "old-password-xyz")
            .await
            .unwrap()
            .is_none(),
        "old password rejected"
    );
}

#[tokio::test]
async fn verify_caps_attempts_then_burns_the_reset() {
    let h = harness("reset-cap").await;
    let domain = unique_domain("cap");
    let app = reset_app(
        Arc::clone(&h.store),
        h.identity.clone(),
        vec![domain.clone()],
    );
    let address = format!("bruteme@{domain}");
    h.store
        .upsert_pending_reset(
            &address,
            "r@gmail.test",
            &code_hash(&address, "999999"),
            600,
        )
        .await
        .unwrap();

    let mut saw_stop = false;
    for _ in 0..10 {
        let (s, _b) = send(
            &app,
            post(
                "/reset/verify",
                serde_json::json!({"address": address, "code": "000000", "password": "brand-new-password"}),
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
        h.store.pending_reset(&address).await.unwrap().is_none(),
        "reset burned"
    );
}
