//! A demo IMAP server for manual/openssl evidence. Seeds a fresh
//! `demo-<tenant>@alo.test` account (password `demo-pass`) with one
//! message, serves implicit TLS on 127.0.0.1:9993, and delivers a second
//! message four seconds after start so an IDLE client sees a live untagged
//! EXISTS. The delayed delivery uses the store's ingestion path — the same
//! path inbound SMTP will call once local delivery (M5) lands.
//!
//! Run: `DATABASE_URL=... cargo run -p alo-imap --example serve_demo`
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::Duration;

use alo_identity::{Identity, IdentityConfig};
use alo_imap::Config;
use alo_store::{BlobStore, Store};

#[tokio::main]
async fn main() {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
    let store = Arc::new(
        Store::connect(&url, BlobStore::in_memory(25 * 1024 * 1024))
            .await
            .expect("connect"),
    );
    store.migrate().await.expect("migrate");
    let identity = Identity::new(Arc::clone(&store), IdentityConfig::new("https://id.test"))
        .expect("identity");

    let tenant = store.create_tenant("imap-demo").await.unwrap();
    let ts = store.for_tenant(tenant.clone());
    let email = format!("demo-{tenant}@alo.test");
    let user = ts.create_user(&email).await.unwrap();
    identity
        .set_password(&tenant, &user, &email, "demo-pass")
        .await
        .unwrap();
    let acc = store.for_account(tenant.clone(), user.clone());
    acc.deliver(
        b"From: Alice <alice@example.com>\r\nTo: demo@alo.test\r\n\
          Subject: Welcome to alo\r\nMessage-ID: <welcome@alo.test>\r\n\r\n\
          Hello from the IMAP shim.\r\n",
    )
    .await
    .unwrap();

    println!("DEMO_EMAIL={email}");
    println!("DEMO_PASS=demo-pass");
    println!("DEMO_ADDR=127.0.0.1:9993");
    println!("READY");

    // Deliver a second message while the client is idling.
    let live = acc.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(8)).await;
        let _ = live
            .deliver(
                b"From: Bob <bob@example.com>\r\nSubject: Live arrival\r\n\r\n\
                  delivered while the client was idling\r\n",
            )
            .await;
    });

    let cfg = Config {
        imaps_addr: Some("127.0.0.1:9993".parse().unwrap()),
        hostname: "localhost".to_owned(),
        allow_self_signed: true,
        ..Config::default()
    };
    alo_imap::serve(cfg, store, identity).await.unwrap();
}
