//! A demo MX with inbound local delivery, for manual/wire evidence. Seeds a
//! fresh `demo-<tenant>@alo.test` account, serves on 127.0.0.1:2526, and
//! delivers received mail into the store (durable filesystem blobs). Prints
//! the demo address, then a message swaked/piped to it lands in the store.
//!
//! Run: `DATABASE_URL=... cargo run -p alo-smtp --example deliver_demo`
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use alo_smtp::local_delivery::LocalDelivery;
use alo_smtp::server::Runtime;
use alo_smtp::spool::Spool;
use alo_store::{BlobStore, Store};

#[tokio::main]
async fn main() {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
    let blob_dir = std::env::temp_dir().join("alo-deliver-demo-blobs");
    let store = Arc::new(
        Store::connect(&url, BlobStore::local(&blob_dir, 25 * 1024 * 1024).unwrap())
            .await
            .expect("store"),
    );
    store.migrate().await.expect("migrate");

    let tenant = store.create_tenant("deliver-demo").await.unwrap();
    let ts = store.for_tenant(tenant.clone());
    let email = format!("demo-{tenant}@alo.test");
    let user = ts.create_user(&email).await.unwrap();
    store.for_account(tenant, user).inbox().await.unwrap();

    let spool = Arc::new(Spool::new(std::env::temp_dir().join("alo-deliver-demo-spool")).unwrap());
    let acceptor =
        Arc::new(alo_smtp::tls::build_acceptor(None, None, "mx.alo.test", true).unwrap());
    let local = Arc::new(LocalDelivery::from_store(
        store,
        spool.clone(),
        "mx.alo.test".to_owned(),
    ));
    let runtime = Arc::new(
        Runtime::mx(
            "mx.alo.test",
            spool,
            acceptor,
            None,
            25 * 1024 * 1024,
            100,
            256,
        )
        .with_local_domains(vec!["alo.test".to_owned()])
        .with_local_delivery(Some(local)),
    );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:2526")
        .await
        .unwrap();
    println!("DEMO_EMAIL={email}");
    println!("DEMO_ADDR=127.0.0.1:2526");
    println!("READY");
    let _ = alo_smtp::server::serve(listener, runtime).await;
}
