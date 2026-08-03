//! A demo JMAP server for manual/curl evidence. Seeds an idempotent
//! `demo@alo.test` account (password `demo-pass`) with one message,
//! then serves on 127.0.0.1:8090.
//!
//! Run: `DATABASE_URL=... cargo run -p alo-jmap --example serve_demo`
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use alo_identity::{Identity, IdentityConfig};
use alo_store::{BlobStore, Store};

#[tokio::main]
async fn main() {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
    let store = Arc::new(
        Store::connect(&url, BlobStore::in_memory(50 * 1024 * 1024))
            .await
            .expect("connect"),
    );
    store.migrate().await.expect("migrate");
    let identity = Identity::new(
        Arc::clone(&store),
        IdentityConfig::new("http://127.0.0.1:8090"),
    )
    .expect("identity");

    let email = "demo@alo.test";
    if identity
        .password_login(email, "demo-pass", None)
        .await
        .expect("login")
        .is_none()
    {
        let tenant = store.create_tenant("demo").await.unwrap();
        let ts = store.for_tenant(tenant.clone());
        let user = ts.create_user(email).await.unwrap();
        identity
            .set_password(&tenant, &user, email, "demo-pass")
            .await
            .unwrap();
        let acc = store.for_account(tenant, user);
        acc.deliver(
            b"From: Alice <alice@example.com>\r\nTo: demo@alo.test\r\n\
              Subject: Welcome to alo\r\nMessage-ID: <welcome@alo.test>\r\n\r\n\
              Hello from the JMAP API.\r\n",
        )
        .await
        .unwrap();
    }

    let addr = "127.0.0.1:8090".parse().unwrap();
    let state = alo_jmap::app_state(Arc::clone(&store), identity, "http://127.0.0.1:8090");
    println!("alo-jmap demo on http://127.0.0.1:8090 (demo@alo.test / demo-pass)");
    alo_jmap::serve(addr, state).await.unwrap();
}
