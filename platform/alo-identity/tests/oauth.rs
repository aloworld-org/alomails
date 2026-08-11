//! OAuth 2.0 / OIDC provider conformance over the real router (driven
//! in-process via `tower::oneshot`): discovery, the authorization-code +
//! PKCE happy path, and the negative cases that matter for security —
//! wrong PKCE verifier, code replay, unregistered redirect, PKCE-downgrade,
//! bad credentials, refresh rotation + replay, and revocation.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_identity::router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use common::{make_user, setup};
use sha2::{Digest, Sha256};
use tower::ServiceExt;

const CLIENT: &str = "alo-web-test";
const REDIRECT: &str = "https://app.alo.test/callback";
const VERIFIER: &str = "a-high-entropy-code-verifier-of-sufficient-length-1234567890";

fn challenge_for(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

fn form(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(k, v)| format!("{}={}", enc(k), enc(v)))
        .collect::<Vec<_>>()
        .join("&")
}

fn enc(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

async fn post_form(
    app: &axum::Router,
    path: &str,
    body: String,
) -> (StatusCode, Vec<u8>, Option<String>) {
    let req = Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/x-www-form-urlencoded")
        .body(Body::from(body))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let location = resp
        .headers()
        .get("location")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
        .await
        .unwrap();
    (status, bytes.to_vec(), location)
}

async fn get(
    app: &axum::Router,
    path: &str,
    bearer: Option<&str>,
) -> (StatusCode, serde_json::Value) {
    let mut b = Request::builder().method("GET").uri(path);
    if let Some(t) = bearer {
        b = b.header("authorization", format!("Bearer {t}"));
    }
    let resp = app
        .clone()
        .oneshot(b.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
        .await
        .unwrap();
    let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

/// Extracts the `code` query parameter from a redirect Location.
fn code_from(location: &str) -> String {
    let q = location.split_once('?').unwrap().1;
    q.split('&')
        .find_map(|kv| kv.strip_prefix("code="))
        .unwrap()
        .to_owned()
}

async fn authorize_ok(app: &axum::Router, email: &str, password: &str) -> String {
    let (status, _b, loc) = post_form(
        app,
        "/oauth/authorize",
        form(&[
            ("client_id", CLIENT),
            ("redirect_uri", REDIRECT),
            ("response_type", "code"),
            ("scope", "openid email profile offline_access"),
            ("state", "xyz"),
            ("code_challenge", &challenge_for(VERIFIER)),
            ("code_challenge_method", "S256"),
            ("nonce", "n-123"),
            ("username", email),
            ("password", password),
        ]),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER, "authorize should redirect");
    let loc = loc.expect("redirect location");
    assert!(loc.starts_with(REDIRECT), "redirect to the registered URI");
    assert!(loc.contains("state=xyz"), "state echoed: {loc}");
    code_from(&loc)
}

async fn setup_app() -> (axum::Router, common::TestUser) {
    let (store, id) = setup().await;
    let u = make_user(&store, &id, "oauth").await;
    id.register_public_client(CLIENT, "Web", &[REDIRECT.to_owned()])
        .await
        .unwrap();
    (router(id), u)
}

const RS_SECRET: &str = "resource-server-shared-secret-abc123";

async fn setup_app_with_introspect() -> (axum::Router, common::TestUser) {
    let (store, id) = common::setup_with_introspect(RS_SECRET).await;
    let u = make_user(&store, &id, "intro").await;
    id.register_public_client(CLIENT, "Web", &[REDIRECT.to_owned()])
        .await
        .unwrap();
    (router(id), u)
}

#[tokio::test]
async fn discovery_and_jwks_are_published() {
    let (app, _u) = setup_app().await;
    let (status, doc) = get(&app, "/.well-known/openid-configuration", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(doc["issuer"], common::ISSUER);
    assert_eq!(doc["code_challenge_methods_supported"][0], "S256");
    assert_eq!(doc["id_token_signing_alg_values_supported"][0], "EdDSA");
    assert!(
        doc["authorization_endpoint"]
            .as_str()
            .unwrap()
            .ends_with("/oauth/authorize")
    );

    let (status, jwks) = get(&app, "/oauth/jwks", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(jwks["keys"][0]["kty"], "OKP");
    assert_eq!(jwks["keys"][0]["crv"], "Ed25519");
    assert!(jwks["keys"][0]["x"].as_str().is_some());
}

#[tokio::test]
async fn full_auth_code_pkce_flow() {
    let (app, u) = setup_app().await;
    let code = authorize_ok(&app, &u.email, &u.password).await;

    // Exchange the code with the correct verifier.
    let (status, body, _l) = post_form(
        &app,
        "/oauth/token",
        form(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", REDIRECT),
            ("client_id", CLIENT),
            ("code_verifier", VERIFIER),
        ]),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "token exchange ok");
    let tok: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let access = tok["access_token"].as_str().unwrap().to_owned();
    assert_eq!(tok["token_type"], "Bearer");
    assert!(
        tok["refresh_token"].as_str().is_some(),
        "offline_access → refresh"
    );
    let id_token = tok["id_token"].as_str().unwrap();
    // The ID token is a 3-part EdDSA JWT with our subject.
    let parts: Vec<&str> = id_token.split('.').collect();
    assert_eq!(parts.len(), 3);
    let header: serde_json::Value =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[0]).unwrap()).unwrap();
    assert_eq!(header["alg"], "EdDSA");
    let claims: serde_json::Value =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[1]).unwrap()).unwrap();
    assert_eq!(claims["sub"], u.user.as_str());
    assert_eq!(claims["nonce"], "n-123");
    assert_eq!(claims["email"], u.email);

    // userinfo with the access token returns the subject + scoped claims.
    let (status, ui) = get(&app, "/oauth/userinfo", Some(&access)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ui["sub"], u.user.as_str());
    assert_eq!(ui["email"], u.email);

    // Revocation: after revoke, userinfo is 401.
    let (status, _b, _l) = post_form(&app, "/oauth/revoke", form(&[("token", &access)])).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _ui) = get(&app, "/oauth/userinfo", Some(&access)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "revoked token refused");
}

async fn post_form_auth(
    app: &axum::Router,
    path: &str,
    body: String,
    bearer: Option<&str>,
) -> (StatusCode, serde_json::Value) {
    let mut b = Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/x-www-form-urlencoded");
    if let Some(t) = bearer {
        b = b.header("authorization", format!("Bearer {t}"));
    }
    let resp = app
        .clone()
        .oneshot(b.body(Body::from(body)).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
    )
}

/// RFC 7662 introspection is the SSO seam standalone products use: it must
/// return the **tenant** (which `userinfo` omits), guard on the resource-server
/// secret, and never leak validity to an unauthenticated caller.
#[tokio::test]
async fn token_introspection_returns_tenant_and_is_guarded() {
    let (app, u) = setup_app_with_introspect().await;
    let code = authorize_ok(&app, &u.email, &u.password).await;
    let (status, body, _l) = post_form(
        &app,
        "/oauth/token",
        form(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", REDIRECT),
            ("client_id", CLIENT),
            ("code_verifier", VERIFIER),
        ]),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let tok: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let access = tok["access_token"].as_str().unwrap().to_owned();

    // With the RS secret, a live token resolves to its principal — tenant + sub.
    let (status, doc) = post_form_auth(
        &app,
        "/oauth/introspect",
        form(&[("token", &access)]),
        Some(RS_SECRET),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(doc["active"], true);
    assert_eq!(doc["sub"], u.user.as_str());
    assert_eq!(
        doc["tenant"],
        u.tenant.as_str(),
        "the tenant userinfo omits"
    );
    assert_eq!(doc["username"], u.email);

    // A bogus token is a normal {active:false}, not an error, not an oracle.
    let (status, doc) = post_form_auth(
        &app,
        "/oauth/introspect",
        form(&[("token", "not-a-real-token")]),
        Some(RS_SECRET),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(doc["active"], false);
    assert!(
        doc["tenant"].is_null(),
        "no principal leaked for an invalid token"
    );

    // Wrong RS secret → 401; missing → 401. The endpoint is not a public oracle.
    let (status, _d) = post_form_auth(
        &app,
        "/oauth/introspect",
        form(&[("token", &access)]),
        Some("wrong"),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _d) =
        post_form_auth(&app, "/oauth/introspect", form(&[("token", &access)]), None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // After revocation the same token introspects inactive.
    let (status, _b, _l) = post_form(&app, "/oauth/revoke", form(&[("token", &access)])).await;
    assert_eq!(status, StatusCode::OK);
    let (status, doc) = post_form_auth(
        &app,
        "/oauth/introspect",
        form(&[("token", &access)]),
        Some(RS_SECRET),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(doc["active"], false, "revoked token is inactive");
}

/// When no RS secret is configured, introspection does not exist (404) — it is
/// off by default, never an accidentally-public oracle.
#[tokio::test]
async fn token_introspection_disabled_without_secret() {
    let (app, _u) = setup_app().await;
    let (status, _d) = post_form_auth(
        &app,
        "/oauth/introspect",
        form(&[("token", "x")]),
        Some("anything"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn wrong_pkce_verifier_is_rejected() {
    let (app, u) = setup_app().await;
    let code = authorize_ok(&app, &u.email, &u.password).await;
    let (status, body, _l) = post_form(
        &app,
        "/oauth/token",
        form(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", REDIRECT),
            ("client_id", CLIENT),
            (
                "code_verifier",
                "the-wrong-verifier-entirely-000000000000000000",
            ),
        ]),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let err: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(err["error"], "invalid_grant");
}

#[tokio::test]
async fn authorization_code_is_single_use() {
    let (app, u) = setup_app().await;
    let code = authorize_ok(&app, &u.email, &u.password).await;
    let exchange = || {
        post_form(
            &app,
            "/oauth/token",
            form(&[
                ("grant_type", "authorization_code"),
                ("code", &code),
                ("redirect_uri", REDIRECT),
                ("client_id", CLIENT),
                ("code_verifier", VERIFIER),
            ]),
        )
    };
    let (first, _b, _l) = exchange().await;
    assert_eq!(first, StatusCode::OK);
    // Replaying the same code is refused.
    let (second, body, _l) = exchange().await;
    assert_eq!(second, StatusCode::BAD_REQUEST);
    let err: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(err["error"], "invalid_grant");
}

#[tokio::test]
async fn unregistered_redirect_uri_is_refused_before_redirect() {
    let (app, u) = setup_app().await;
    let (status, _b, loc) = post_form(
        &app,
        "/oauth/authorize",
        form(&[
            ("client_id", CLIENT),
            ("redirect_uri", "https://evil.example/steal"),
            ("response_type", "code"),
            ("scope", "openid"),
            ("code_challenge", &challenge_for(VERIFIER)),
            ("code_challenge_method", "S256"),
            ("username", &u.email),
            ("password", &u.password),
        ]),
    )
    .await;
    // A mismatched redirect must NOT redirect (no code to an attacker URI).
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(loc.is_none());
}

#[tokio::test]
async fn pkce_is_mandatory_s256() {
    let (app, u) = setup_app().await;
    // No PKCE challenge → redirected error (invalid_request), not a code.
    let (status, _b, loc) = post_form(
        &app,
        "/oauth/authorize",
        form(&[
            ("client_id", CLIENT),
            ("redirect_uri", REDIRECT),
            ("response_type", "code"),
            ("scope", "openid"),
            ("username", &u.email),
            ("password", &u.password),
        ]),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let loc = loc.unwrap();
    assert!(loc.contains("error=invalid_request"), "{loc}");
    assert!(!loc.contains("code="), "no code without PKCE: {loc}");
}

#[tokio::test]
async fn bad_credentials_do_not_redirect() {
    let (app, u) = setup_app().await;
    let (status, _b, loc) = post_form(
        &app,
        "/oauth/authorize",
        form(&[
            ("client_id", CLIENT),
            ("redirect_uri", REDIRECT),
            ("response_type", "code"),
            ("scope", "openid"),
            ("code_challenge", &challenge_for(VERIFIER)),
            ("code_challenge_method", "S256"),
            ("username", &u.email),
            ("password", "definitely-wrong"),
        ]),
    )
    .await;
    // Unauthenticated user is a 401, never a redirect back to the RP.
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(loc.is_none());
}

#[tokio::test]
async fn refresh_token_rotates_and_replay_revokes() {
    let (app, u) = setup_app().await;
    let code = authorize_ok(&app, &u.email, &u.password).await;
    let (_s, body, _l) = post_form(
        &app,
        "/oauth/token",
        form(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", REDIRECT),
            ("client_id", CLIENT),
            ("code_verifier", VERIFIER),
        ]),
    )
    .await;
    let tok: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let refresh1 = tok["refresh_token"].as_str().unwrap().to_owned();

    // Use the refresh token → new access + a rotated refresh token.
    let (status, body, _l) = post_form(
        &app,
        "/oauth/token",
        form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", &refresh1),
            ("client_id", CLIENT),
        ]),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let rot: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let refresh2 = rot["refresh_token"].as_str().unwrap().to_owned();
    assert_ne!(refresh1, refresh2, "refresh token rotates on use");
    let access2 = rot["access_token"].as_str().unwrap().to_owned();

    // Replaying the OLD refresh token is a replay: refused, and the chain
    // (including the just-issued access token) is revoked.
    let (status, body, _l) = post_form(
        &app,
        "/oauth/token",
        form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", &refresh1),
            ("client_id", CLIENT),
        ]),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let err: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(err["error"], "invalid_grant");

    // The rotated access token is now revoked (chain revocation on replay).
    let (status, _ui) = get(&app, "/oauth/userinfo", Some(&access2)).await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "replay must revoke the whole token chain"
    );
}

#[tokio::test]
async fn concurrent_refresh_use_lets_only_one_win() {
    let (app, u) = setup_app().await;
    let code = authorize_ok(&app, &u.email, &u.password).await;
    let (_s, body, _l) = post_form(
        &app,
        "/oauth/token",
        form(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", REDIRECT),
            ("client_id", CLIENT),
            ("code_verifier", VERIFIER),
        ]),
    )
    .await;
    let tok: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let refresh = tok["refresh_token"].as_str().unwrap().to_owned();

    // Fire two redemptions of the SAME refresh token concurrently. The
    // atomic guarded rotate must let exactly one win; the loser is refused
    // (the store's `rotate_refresh_token` gate, not the fast pre-check).
    let body1 = form(&[
        ("grant_type", "refresh_token"),
        ("refresh_token", &refresh),
        ("client_id", CLIENT),
    ]);
    let body2 = body1.clone();
    let (r1, r2) = tokio::join!(
        post_form(&app, "/oauth/token", body1),
        post_form(&app, "/oauth/token", body2),
    );
    let oks = [r1.0, r2.0].iter().filter(|s| s.is_success()).count();
    let bads = [r1.0, r2.0]
        .iter()
        .filter(|s| **s == StatusCode::BAD_REQUEST)
        .count();
    assert_eq!(oks, 1, "exactly one concurrent refresh may succeed");
    assert_eq!(bads, 1, "the other must be refused as a replay");
}
