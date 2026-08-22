//! Snooze round-trip: snoozing moves a message out of the Inbox into the
//! Snoozed mailbox with a wake time; the sweeper returns due messages to the
//! Inbox (unread) and clears the wake time. Runs against a throwaway Postgres;
//! skips cleanly when none is available.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use alo_store::{BlobStore, Store};
use sqlx::postgres::PgPoolOptions;

/// The database this suite runs against.
///
/// Delegates to `alo_test_db`, which refuses the database the product
/// runs on: suites create and drop their own, they never write into `alo`.
fn database_url() -> String {
    alo_test_db::url()
}

const RAW: &[u8] = b"From: a@sender.test\r\nTo: b@owner.test\r\nSubject: snooze me\r\n\r\nbody\r\n";

#[tokio::test]
async fn snooze_moves_out_of_inbox_and_the_sweeper_brings_it_back() {
    let Ok(pool) = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url())
        .await
    else {
        eprintln!("SKIP: no database at {}", database_url());
        return;
    };
    let store = Arc::new(Store::new(pool, BlobStore::in_memory(8 * 1024 * 1024)));
    store.migrate().await.unwrap();

    let tenant = store.create_tenant("snooze-t").await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user("owner@snooze.test")
        .await
        .unwrap();
    let acc = store.for_account(tenant.clone(), user.clone());

    let inbox = acc.inbox().await.unwrap();
    let mid = acc.ingest(&inbox, RAW).await.unwrap();
    assert!(
        acc.mailboxes_of_message(&mid)
            .await
            .unwrap()
            .contains(&inbox)
    );

    // --- snooze into the future: leaves the Inbox, lands in Snoozed ---
    let future = 4_102_444_800_i64; // 2100-01-01
    acc.snooze(std::slice::from_ref(&mid), &inbox, future)
        .await
        .unwrap();
    let boxes = acc.mailboxes_of_message(&mid).await.unwrap();
    assert!(!boxes.contains(&inbox), "should have left the Inbox");
    assert!(!boxes.is_empty(), "should be in the Snoozed mailbox");

    // The sweeper must NOT wake a message that isn't due yet.
    assert_eq!(store.sweep_snoozes().await.unwrap(), 0);
    assert!(
        !acc.mailboxes_of_message(&mid)
            .await
            .unwrap()
            .contains(&inbox)
    );

    // --- re-snooze into the past, then sweep: back to the Inbox, unread ---
    acc.snooze(std::slice::from_ref(&mid), &inbox, 1_000_000_000_i64)
        .await
        .unwrap(); // 2001
    // mark it read first, so we can prove the sweeper returns it unread
    acc.set_keyword(&mid, "$seen", true).await.unwrap();

    let woken = store.sweep_snoozes().await.unwrap();
    assert_eq!(woken, 1, "the due message should be woken");
    let after = acc.mailboxes_of_message(&mid).await.unwrap();
    assert!(after.contains(&inbox), "should be back in the Inbox");
    assert_eq!(after.len(), 1, "should be only in the Inbox now");

    // The wake time is cleared, so a second sweep finds nothing.
    assert_eq!(
        store.sweep_snoozes().await.unwrap(),
        0,
        "wake time should be cleared"
    );

    store.delete_tenant(&tenant).await.unwrap();
}
