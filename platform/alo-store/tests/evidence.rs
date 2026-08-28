//! Evidence generator (run explicitly, not in the gate): sets up two
//! labelled tenants with interleaved messages and prints each tenant's
//! ids and API-side view, so a psql transcript can show the same rows
//! interleaved in one table while the API shows each tenant only its own.
//!
//! Run: `cargo test -p alo-store --test evidence -- --ignored --nocapture`
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::Page;

#[tokio::test]
#[ignore = "evidence generator; run explicitly with --ignored"]
async fn interleaved_tenants_api_isolation() {
    let store = common::test_store().await;

    let a_tenant = store.create_tenant("EVIDENCE-ACME").await.unwrap();
    let b_tenant = store.create_tenant("EVIDENCE-BEACON").await.unwrap();
    let ua = store
        .for_tenant(a_tenant.clone())
        .create_user("alice@acme.example")
        .await
        .unwrap();
    let ub = store
        .for_tenant(b_tenant.clone())
        .create_user("bob@beacon.example")
        .await
        .unwrap();
    let a = store.for_account(a_tenant.clone(), ua);
    let b = store.for_account(b_tenant.clone(), ub);
    let ia = a.inbox().await.unwrap();
    let ib = b.inbox().await.unwrap();

    for s in [
        "ACME quarterly numbers",
        "ACME payroll run",
        "ACME board deck",
    ] {
        let raw = format!(
            "From: x@acme.example\r\nSubject: {s}\r\nMessage-ID: <{s}@acme>\r\n\r\nbody\r\n"
        );
        a.ingest(&ia, raw.as_bytes()).await.unwrap();
    }
    for s in ["BEACON launch plan", "BEACON investor update"] {
        let raw = format!(
            "From: y@beacon.example\r\nSubject: {s}\r\nMessage-ID: <{s}@beacon>\r\n\r\nbody\r\n"
        );
        b.ingest(&ib, raw.as_bytes()).await.unwrap();
    }

    println!("EVIDENCE_TENANT_A={a_tenant}");
    println!("EVIDENCE_TENANT_B={b_tenant}");
    println!("EVIDENCE_INBOX_A={ia}");
    println!("EVIDENCE_INBOX_B={ib}");

    println!("--- API view: tenant A (ACME) sees only its own inbox ---");
    for m in a.list_mailbox(&ia, Page::first(50)).await.unwrap() {
        println!("A  {}", m.subject);
    }
    println!("--- API view: tenant B (BEACON) sees only its own inbox ---");
    for m in b.list_mailbox(&ib, Page::first(50)).await.unwrap() {
        println!("B  {}", m.subject);
    }
    // A asking for B's inbox sees nothing.
    let cross = a.list_mailbox(&ib, Page::first(50)).await.unwrap();
    println!(
        "--- API view: tenant A asking for B's inbox id: {} rows ---",
        cross.len()
    );
    assert!(cross.is_empty());
}
