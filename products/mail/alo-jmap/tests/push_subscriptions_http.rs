//! `/settings/push-subscriptions` and the Web Push send path (mail M5.3).
//!
//! Two halves, both provable without a browser:
//! - the routes: every operation scoped by the token's `(tenant, user)`,
//!   another tenant's user gets the same clean 404 as an absent id, and a
//!   non-HTTPS endpoint is refused at the door;
//! - the wire: a real local HTTP server stands in for the push service, a
//!   real client keypair decrypts what the dispatcher sent, and what it
//!   reads is the RFC 8620 `StateChange` object — ids and type names,
//!   never content. A 410 from the service deletes the subscription.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use alo_jmap::push::StateChangeMsg;
use alo_jmap::push_notify::WebPush;
use alo_jmap::web_push::VapidKeys;
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use common::{get, harness, harness_on, send};
use ring::{aead, agreement, hkdf, rand, signature};
use serde_json::{Value, json};

/// POSTs JSON to `path` with the given bearer token.
async fn post_json(app: &Router, token: &str, path: &str, body: Value) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("POST")
        .uri(path)
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    send(app, req).await
}

/// DELETEs `path` with the given bearer token.
async fn delete(app: &Router, token: &str, path: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("DELETE")
        .uri(path)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    send(app, req).await
}

/// A subscription body in the W3C `PushSubscription.toJSON()` shape.
fn subscription_json(endpoint: &str, p256dh: &str, auth: &str) -> Value {
    json!({ "endpoint": endpoint, "keys": { "p256dh": p256dh, "auth": auth } })
}

#[tokio::test]
async fn subscribe_list_and_remove_round_trip() {
    let h = harness("push-crud").await;

    // Without a VAPID key configured the surface says so — the settings
    // screen shows "unavailable" instead of an opt-in that cannot work.
    let (status, body) = get(&h.app, &h.token, "/settings/push-subscriptions").await;
    assert!(status.is_success());
    assert_eq!(body["enabled"], json!(false), "{body}");
    assert_eq!(body["publicKey"], Value::Null, "{body}");
    assert_eq!(body["subscriptions"].as_array().unwrap().len(), 0);

    // Subscribing still works while sending is dark: the row is the opt-in.
    let (status, created) = post_json(
        &h.app,
        &h.token,
        "/settings/push-subscriptions",
        subscription_json("https://push.example/send/dev-1", "pk", "auth"),
    )
    .await;
    assert!(status.is_success(), "created: {status} {created}");
    let id = created["id"].as_str().unwrap().to_owned();

    // Re-subscribing the same endpoint refreshes, never duplicates.
    let (_s, again) = post_json(
        &h.app,
        &h.token,
        "/settings/push-subscriptions",
        subscription_json("https://push.example/send/dev-1", "pk-2", "auth-2"),
    )
    .await;
    assert_eq!(again["id"], json!(id), "{again}");

    let (_s, body) = get(&h.app, &h.token, "/settings/push-subscriptions").await;
    let list = body["subscriptions"].as_array().unwrap();
    assert_eq!(list.len(), 1, "{body}");
    assert_eq!(
        list[0]["endpoint"],
        json!("https://push.example/send/dev-1")
    );
    assert!(list[0]["createdAt"].is_string(), "{body}");
    // The key material never comes back out.
    assert!(!body.to_string().contains("p256dh"), "{body}");

    let (status, _b) = delete(
        &h.app,
        &h.token,
        &format!("/settings/push-subscriptions/{id}"),
    )
    .await;
    assert!(status.is_success());
    let (_s, body) = get(&h.app, &h.token, "/settings/push-subscriptions").await;
    assert_eq!(body["subscriptions"].as_array().unwrap().len(), 0);
    // Removing it again is the same 404 as never having existed.
    let (status, _b) = delete(
        &h.app,
        &h.token,
        &format!("/settings/push-subscriptions/{id}"),
    )
    .await;
    assert_eq!(status.as_u16(), 404);
}

#[tokio::test]
async fn a_bad_subscription_is_refused() {
    let h = harness("push-invalid").await;

    // Not a URL at all.
    let (status, _b) = post_json(
        &h.app,
        &h.token,
        "/settings/push-subscriptions",
        subscription_json("not a url", "pk", "auth"),
    )
    .await;
    assert_eq!(status.as_u16(), 422);

    // Plain http to a non-loopback host: the server would refuse to POST
    // there later, so it refuses to store it now.
    let (status, _b) = post_json(
        &h.app,
        &h.token,
        "/settings/push-subscriptions",
        subscription_json("http://push.example/send/x", "pk", "auth"),
    )
    .await;
    assert_eq!(status.as_u16(), 422);

    // Missing key material.
    let (status, _b) = post_json(
        &h.app,
        &h.token,
        "/settings/push-subscriptions",
        json!({ "endpoint": "https://push.example/send/x" }),
    )
    .await;
    assert_eq!(status.as_u16(), 422);
}

#[tokio::test]
async fn another_tenant_cannot_see_or_remove_them() {
    // Two tenants on ONE store handle, the way production runs.
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .connect(&common::database_url())
        .await
        .unwrap();
    let store = Arc::new(alo_store::Store::new(
        pool,
        alo_store::BlobStore::in_memory(1024 * 1024),
    ));
    store.migrate().await.unwrap();
    let a = harness_on(Arc::clone(&store), "push-tenant-a").await;
    let b = harness_on(Arc::clone(&store), "push-tenant-b").await;

    let (_s, created) = post_json(
        &a.app,
        &a.token,
        "/settings/push-subscriptions",
        subscription_json("https://push.example/send/a-phone", "pk", "auth"),
    )
    .await;
    let id = created["id"].as_str().unwrap();

    // B's list is B's — A's device is not in it.
    let (status, body) = get(&b.app, &b.token, "/settings/push-subscriptions").await;
    assert!(status.is_success());
    assert_eq!(
        body["subscriptions"].as_array().unwrap().len(),
        0,
        "another tenant's devices must not appear: {body}"
    );

    // B removing A's id gets the same clean 404 as an unknown id — and the
    // row survives, because nothing was deleted.
    let (status, _b2) = delete(
        &b.app,
        &b.token,
        &format!("/settings/push-subscriptions/{id}"),
    )
    .await;
    assert_eq!(status.as_u16(), 404, "cross-tenant delete must be a 404");
    let (_s, body) = get(&a.app, &a.token, "/settings/push-subscriptions").await;
    assert_eq!(body["subscriptions"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn every_route_requires_a_token() {
    let h = harness("push-unauth").await;
    for (method, path) in [
        ("GET", "/settings/push-subscriptions"),
        ("POST", "/settings/push-subscriptions"),
        ("DELETE", "/settings/push-subscriptions/some-id"),
    ] {
        let req = Request::builder()
            .method(method)
            .uri(path)
            .header("content-type", "application/json")
            .body(Body::from("{}"))
            .unwrap();
        let (status, _b) = send(&h.app, req).await;
        assert_eq!(status.as_u16(), 401, "{method} {path}");
    }
}

/// What one captured delivery to the fake push service looked like.
#[derive(Clone)]
struct Captured {
    authorization: String,
    content_encoding: String,
    ttl: String,
    topic: String,
    body: Vec<u8>,
}

/// A local HTTP server standing in for a browser push service: records
/// every POST and answers `status`. Returns its base URL.
async fn fake_push_service(status: StatusCode, log: Arc<Mutex<Vec<Captured>>>) -> String {
    use axum::extract::State;
    use axum::routing::post;
    async fn capture(
        State((log, status)): State<(Arc<Mutex<Vec<Captured>>>, StatusCode)>,
        headers: axum::http::HeaderMap,
        body: axum::body::Bytes,
    ) -> StatusCode {
        let h = |name: &str| {
            headers
                .get(name)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_owned()
        };
        log.lock().unwrap().push(Captured {
            authorization: h("authorization"),
            content_encoding: h("content-encoding"),
            ttl: h("ttl"),
            topic: h("topic"),
            body: body.to_vec(),
        });
        status
    }
    let app = Router::new()
        .route("/send/{device}", post(capture))
        .with_state((log, status));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

/// Decrypts an `aes128gcm` push body with the browser-side keys (RFC 8291
/// §3.4 from the receiver's seat) and returns the plaintext.
fn browser_decrypt(
    ua_private: agreement::EphemeralPrivateKey,
    ua_public: &[u8],
    auth: &[u8; 16],
    body: &[u8],
) -> Vec<u8> {
    fn derive(salt: &[u8], ikm: &[u8], info: &[u8], out: &mut [u8]) {
        struct Len(usize);
        impl hkdf::KeyType for Len {
            fn len(&self) -> usize {
                self.0
            }
        }
        hkdf::Salt::new(hkdf::HKDF_SHA256, salt)
            .extract(ikm)
            .expand(&[info], Len(out.len()))
            .unwrap()
            .fill(out)
            .unwrap();
    }
    let salt = &body[..16];
    assert_eq!(u32::from_be_bytes(body[16..20].try_into().unwrap()), 4096);
    let keyid_len = usize::from(body[20]);
    assert_eq!(keyid_len, 65, "keyid is the server's ephemeral P-256 point");
    let as_public = body[21..21 + keyid_len].to_vec();
    let ciphertext = &body[21 + keyid_len..];

    let peer = agreement::UnparsedPublicKey::new(&agreement::ECDH_P256, as_public.clone());
    let record = agreement::agree_ephemeral(ua_private, &peer, |ecdh_secret| {
        let mut key_info = Vec::new();
        key_info.extend_from_slice(b"WebPush: info\x00");
        key_info.extend_from_slice(ua_public);
        key_info.extend_from_slice(&as_public);
        let mut ikm = [0u8; 32];
        derive(auth, ecdh_secret, &key_info, &mut ikm);
        let mut cek = [0u8; 16];
        derive(salt, &ikm, b"Content-Encoding: aes128gcm\x00", &mut cek);
        let mut nonce = [0u8; 12];
        derive(salt, &ikm, b"Content-Encoding: nonce\x00", &mut nonce);
        let key = aead::LessSafeKey::new(aead::UnboundKey::new(&aead::AES_128_GCM, &cek).unwrap());
        let mut buf = ciphertext.to_vec();
        key.open_in_place(
            aead::Nonce::assume_unique_for_key(nonce),
            aead::Aad::empty(),
            &mut buf,
        )
        .unwrap()
        .to_vec()
    })
    .unwrap();
    // Strip the single-record 0x02 delimiter.
    assert_eq!(*record.last().unwrap(), 0x02);
    record[..record.len() - 1].to_vec()
}

/// Waits until `check` passes or a deadline expires.
async fn eventually<F: Fn() -> bool>(what: &str, check: F) {
    for _ in 0..200 {
        if check() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("timed out waiting for {what}");
}

#[tokio::test]
async fn a_state_change_reaches_a_closed_apps_device_encrypted_and_content_free() {
    let mut h = harness("push-e2e").await;

    // This deployment holds a VAPID key; the dispatcher is wired to the
    // SAME hub the app's routes publish to.
    let keys = VapidKeys::new(
        &VapidKeys::generate_key_b64().unwrap(),
        "mailto:owner@example.test",
    )
    .unwrap();
    let web_push = Arc::new(WebPush::new(keys));
    let state = {
        let mut s = alo_jmap::app_state(Arc::clone(&h.store), h.identity.clone(), "https://test");
        s.web_push = Some(Arc::clone(&web_push));
        s
    };
    alo_jmap::push_notify::wire(&state.push, Arc::clone(&h.store), Arc::clone(&web_push));
    h.app = alo_jmap::app(state.clone());

    // The advertised public key is the one the VAPID tokens verify against.
    let (_s, body) = get(&h.app, &h.token, "/settings/push-subscriptions").await;
    assert_eq!(body["enabled"], json!(true), "{body}");
    let advertised = body["publicKey"].as_str().unwrap().to_owned();

    // A browser-side subscription: real P-256 keys, a real auth secret.
    let rng = rand::SystemRandom::new();
    let ua_private = agreement::EphemeralPrivateKey::generate(&agreement::ECDH_P256, &rng).unwrap();
    let ua_public = ua_private.compute_public_key().unwrap().as_ref().to_vec();
    let auth: [u8; 16] = *b"test-auth-secret";
    let log = Arc::new(Mutex::new(Vec::new()));
    let service = fake_push_service(StatusCode::CREATED, Arc::clone(&log)).await;
    let endpoint = format!("{service}/send/desk");
    let (status, _b) = post_json(
        &h.app,
        &h.token,
        "/settings/push-subscriptions",
        subscription_json(
            &endpoint,
            &URL_SAFE_NO_PAD.encode(&ua_public),
            &URL_SAFE_NO_PAD.encode(auth),
        ),
    )
    .await;
    assert!(status.is_success(), "subscribed: {status}");

    // Something changes in this account — no EventSource is open anywhere,
    // which is exactly the situation Web Push exists for.
    state.push.publish(
        h.tenant.as_str(),
        StateChangeMsg {
            account_id: h.account_id.clone(),
            types: vec![
                alo_store::changes::TYPE_EMAIL,
                alo_store::changes::TYPE_MAILBOX,
            ],
            state: "state-7".to_owned(),
        },
    );

    eventually("the push delivery", || !log.lock().unwrap().is_empty()).await;
    let captured = log.lock().unwrap()[0].clone();

    // The envelope: aes128gcm, a TTL, a collapse topic.
    assert_eq!(captured.content_encoding, "aes128gcm");
    assert_eq!(captured.ttl, "86400");
    assert_eq!(captured.topic, "alo-state");

    // The VAPID token verifies against the advertised key and names this
    // push service as its audience (RFC 8292).
    let token = captured
        .authorization
        .strip_prefix("vapid t=")
        .unwrap()
        .split(", k=")
        .next()
        .unwrap()
        .to_owned();
    let k = captured.authorization.split(", k=").nth(1).unwrap();
    assert_eq!(k, advertised);
    let mut parts = token.split('.');
    let (jh, jc, js) = (
        parts.next().unwrap(),
        parts.next().unwrap(),
        parts.next().unwrap(),
    );
    signature::UnparsedPublicKey::new(
        &signature::ECDSA_P256_SHA256_FIXED,
        URL_SAFE_NO_PAD.decode(&advertised).unwrap(),
    )
    .verify(
        format!("{jh}.{jc}").as_bytes(),
        &URL_SAFE_NO_PAD.decode(js).unwrap(),
    )
    .expect("VAPID signature verifies");
    let claims: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(jc).unwrap()).unwrap();
    assert_eq!(claims["aud"].as_str().unwrap(), service);
    assert_eq!(claims["sub"], "mailto:owner@example.test");

    // The payload decrypts with the BROWSER's keys and carries the
    // StateChange object — type names, the account id, an opaque state
    // string — and nothing else. No subject, no sender, no body.
    let plaintext = browser_decrypt(ua_private, &ua_public, &auth, &captured.body);
    let payload: Value = serde_json::from_slice(&plaintext).unwrap();
    assert_eq!(payload["@type"], json!("StateChange"), "{payload}");
    let changed = payload["changed"].as_object().unwrap();
    assert_eq!(changed.len(), 1, "{payload}");
    let account = changed.get(&h.account_id).unwrap().as_object().unwrap();
    assert_eq!(account.len(), 2, "{payload}");
    assert_eq!(account["Email"], json!("state-7"));
    assert_eq!(account["Mailbox"], json!("state-7"));
    assert_eq!(
        payload.as_object().unwrap().len(),
        2,
        "nothing but the type tag and the changed map rides in a push payload: {payload}"
    );
}

#[tokio::test]
async fn a_gone_endpoint_removes_the_subscription() {
    let mut h = harness("push-gone").await;
    let keys = VapidKeys::new(
        &VapidKeys::generate_key_b64().unwrap(),
        "mailto:owner@example.test",
    )
    .unwrap();
    let web_push = Arc::new(WebPush::new(keys));
    let state = {
        let mut s = alo_jmap::app_state(Arc::clone(&h.store), h.identity.clone(), "https://test");
        s.web_push = Some(Arc::clone(&web_push));
        s
    };
    alo_jmap::push_notify::wire(&state.push, Arc::clone(&h.store), Arc::clone(&web_push));
    h.app = alo_jmap::app(state.clone());

    // The push service answers 410 Gone: the browser unsubscribed.
    let rng = rand::SystemRandom::new();
    let ua_private = agreement::EphemeralPrivateKey::generate(&agreement::ECDH_P256, &rng).unwrap();
    let ua_public = ua_private.compute_public_key().unwrap().as_ref().to_vec();
    let log = Arc::new(Mutex::new(Vec::new()));
    let service = fake_push_service(StatusCode::GONE, Arc::clone(&log)).await;
    let (status, _b) = post_json(
        &h.app,
        &h.token,
        "/settings/push-subscriptions",
        subscription_json(
            &format!("{service}/send/stale"),
            &URL_SAFE_NO_PAD.encode(&ua_public),
            &URL_SAFE_NO_PAD.encode(*b"test-auth-secret"),
        ),
    )
    .await;
    assert!(status.is_success());

    state.push.publish(
        h.tenant.as_str(),
        StateChangeMsg {
            account_id: h.account_id.clone(),
            types: vec![alo_store::changes::TYPE_EMAIL],
            state: "s1".to_owned(),
        },
    );

    eventually("the delivery attempt", || !log.lock().unwrap().is_empty()).await;
    // The dead device is dropped: the next state change has nowhere to go
    // and the owner's list no longer shows it.
    let ts = h.store.for_tenant(h.tenant.clone());
    let mut cleaned = false;
    for _ in 0..200 {
        if ts.push_deliveries(&h.user).await.unwrap().is_empty() {
            cleaned = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(cleaned, "a 410 from the push service must delete the row");
}
