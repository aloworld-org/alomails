//! Ingestion crash-safety. The blob is written before the DB commit, so
//! a crash in between leaves an *invisible orphan blob* the GC reclaims —
//! never a visible message with a missing body, and nothing lost (the
//! sender retries). Runs against real Postgres.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::blob::hash_hex;
use alo_store::{Page, StoreError};
use bytes::Bytes;

#[tokio::test]
async fn orphan_blob_is_invisible_and_redelivery_is_not_lost() {
    let (store, blobs) = common::test_store_with_blobs().await;
    let (ts, _user, inbox) = common::fresh_account(&store, "crash").await;
    let raw = b"From: a@example.test\r\nSubject: crash\r\nMessage-ID: <c@x>\r\n\r\nbody\r\n";
    let hash = hash_hex(raw);

    // Simulate a crash AFTER the blob write but BEFORE the DB commit: the
    // bytes exist, but no message row references them.
    blobs
        .put(ts.tenant().as_str(), &hash, Bytes::copy_from_slice(raw))
        .await
        .unwrap();

    // The orphan is invisible — the tenant sees no half-delivered message.
    assert!(
        ts.list_mailbox(&inbox, Page::default())
            .await
            .unwrap()
            .is_empty(),
        "an orphan blob must not surface as a message"
    );

    // The sender retries; re-delivery succeeds and is now visible, reusing
    // the pre-existing content-addressed blob. Nothing was lost.
    let mid = ts.ingest(&inbox, raw).await.unwrap();
    assert_eq!(ts.message_bytes(&mid).await.unwrap().as_ref(), &raw[..]);
    assert_eq!(
        ts.list_mailbox(&inbox, Page::default())
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn rejected_ingest_writes_no_visible_state() {
    // A cross-tenant/invalid ingest is rejected by the ownership guard
    // BEFORE any blob or row is written — no orphan, no partial message.
    let store = common::test_store().await;
    let (ts, _user, inbox) = common::fresh_account(&store, "reject").await;
    let (_other_ts, _other_user, other_inbox) = common::fresh_account(&store, "reject-other").await;

    let result = ts
        .ingest(&other_inbox, b"From: a@example.test\r\n\r\nx")
        .await;
    assert!(matches!(result, Err(StoreError::NotFound)));

    // ts's own inbox is untouched.
    assert!(
        ts.list_mailbox(&inbox, Page::default())
            .await
            .unwrap()
            .is_empty()
    );
}
