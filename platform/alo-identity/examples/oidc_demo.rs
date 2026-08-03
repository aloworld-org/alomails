//! A demo OpenID Connect provider for manual/curl evidence. Seeds an
//! idempotent demo user, registers a public PKCE client, ensures a signing
//! key, and serves the provider on 127.0.0.1:7777. Prints a ready-to-run
//! PKCE verifier/challenge pair so a curl transcript can walk
//! discovery → authorize → token → userinfo.
//!
//! Run: `DATABASE_URL=... cargo run -p alo-identity --example oidc_demo`
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use alo_identity::{Identity, IdentityConfig};
use alo_store::{BlobStore, Store};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};

const ISSUER: &str = "http://127.0.0.1:7777";
const CLIENT: &str = "demo-web";
const REDIRECT: &str = "http://127.0.0.1:7777/callback";
const EMAIL: &str = "demo@alo.test";
const PASSWORD: &str = "demo-pass-1234";
const VERIFIER: &str = "demo-pkce-verifier-of-sufficient-length-0123456789abcdef";

#[tokio::main]
async fn main() {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
    let store = Arc::new(
        Store::connect(&url, BlobStore::in_memory(25 * 1024 * 1024))
            .await
            .expect("connect"),
    );
    store.migrate().await.expect("migrate");
    let identity =
        Identity::new(Arc::clone(&store), IdentityConfig::new(ISSUER)).expect("identity");
    identity.ensure_signing_key().await.expect("signing key");

    // Idempotent provisioning of the demo user + client.
    if identity
        .authenticate_password(EMAIL, PASSWORD)
        .await
        .expect("auth")
        .is_none()
    {
        identity
            .bootstrap_admin("oidc-demo", EMAIL, PASSWORD)
            .await
            .expect("bootstrap");
    }
    identity
        .register_public_client(CLIENT, "Demo Web", &[REDIRECT.to_owned()])
        .await
        .expect("register client");

    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(VERIFIER.as_bytes()));
    println!("OIDC demo provider on {ISSUER}");
    println!("  user     = {EMAIL} / {PASSWORD}");
    println!("  client   = {CLIENT}");
    println!("  redirect = {REDIRECT}");
    println!("  pkce_verifier  = {VERIFIER}");
    println!("  pkce_challenge = {challenge}");
    println!("READY");

    let listener = tokio::net::TcpListener::bind("127.0.0.1:7777")
        .await
        .unwrap();
    axum::serve(listener, alo_identity::router(identity))
        .await
        .unwrap();
}
