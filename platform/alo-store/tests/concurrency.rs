//! Concurrent flag/counter tests: a mailbox counter must never drift
//! from reality under simultaneous updates. Runs against real Postgres
//! on a multi-threaded runtime.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::SEEN;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_mark_seen_keeps_unread_consistent() {
    let store = common::test_store().await;
    let (ts, _user, inbox) = common::fresh_account(&store, "conc-seen").await;
    let message = common::deliver(&ts, &inbox, "<m@x>", &[], "hi").await;
    assert_eq!(ts.mailbox(&inbox).await.unwrap().unread_messages, 1);

    // 32 concurrent "mark seen": exactly one must move the counter.
    let mut handles = Vec::new();
    for _ in 0..32 {
        let ts = ts.clone();
        let message = message.clone();
        handles.push(tokio::spawn(async move {
            ts.set_keyword(&message, SEEN, true).await.unwrap();
        }));
    }
    for h in handles {
        h.await.unwrap();
    }
    assert_eq!(
        ts.mailbox(&inbox).await.unwrap().unread_messages,
        0,
        "unread must not go negative or stay >0 under concurrent marks"
    );
    assert_eq!(ts.keywords(&message).await.unwrap(), vec![SEEN.to_owned()]);

    // 32 concurrent "unmark seen": counter returns to exactly 1.
    let mut handles = Vec::new();
    for _ in 0..32 {
        let ts = ts.clone();
        let message = message.clone();
        handles.push(tokio::spawn(async move {
            ts.set_keyword(&message, SEEN, false).await.unwrap();
        }));
    }
    for h in handles {
        h.await.unwrap();
    }
    assert_eq!(ts.mailbox(&inbox).await.unwrap().unread_messages, 1);
    assert!(ts.keywords(&message).await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_add_to_mailbox_is_idempotent() {
    let store = common::test_store().await;
    let (ts, _user, inbox) = common::fresh_account(&store, "conc-add").await;
    let message = common::deliver(&ts, &inbox, "<m@x>", &[], "hi").await;
    let folder = ts.create_mailbox(None, "Folder", None).await.unwrap();

    // 32 concurrent adds of the same message: total settles at 1.
    let mut handles = Vec::new();
    for _ in 0..32 {
        let ts = ts.clone();
        let message = message.clone();
        let folder = folder.clone();
        handles.push(tokio::spawn(async move {
            ts.add_to_mailbox(&message, &folder).await.unwrap();
        }));
    }
    for h in handles {
        h.await.unwrap();
    }
    let mb = ts.mailbox(&folder).await.unwrap();
    assert_eq!(mb.total_messages, 1, "idempotent add → total exactly 1");
    assert_eq!(mb.unread_messages, 1);
    assert_eq!(
        ts.list_mailbox(&folder, alo_store::Page::default())
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn set_seen_racing_membership_never_drifts_or_underflows() {
    // Regression (cold-review MEDIUM): set_keyword($seen) interleaved with
    // add/remove must not drift the unread counter via a stale delta. The
    // message-row FOR UPDATE lock serializes them; whatever the ordering,
    // counters must equal reality and never go negative (else the CHECK
    // constraint fails the transaction).
    let store = common::test_store().await;
    let (ts, _user, inbox) = common::fresh_account(&store, "conc-race").await;
    let message = common::deliver(&ts, &inbox, "<m@x>", &[], "hi").await;
    let folder = ts.create_mailbox(None, "Folder", None).await.unwrap();

    let mut handles = Vec::new();
    for i in 0..48 {
        let ts = ts.clone();
        let message = message.clone();
        let folder = folder.clone();
        handles.push(tokio::spawn(async move {
            match i % 3 {
                0 => {
                    let _ = ts.set_keyword(&message, alo_store::SEEN, i % 2 == 0).await;
                }
                1 => {
                    let _ = ts.add_to_mailbox(&message, &folder).await;
                }
                _ => {
                    let _ = ts.remove_from_mailbox(&message, &folder).await;
                }
            }
        }));
    }
    for h in handles {
        h.await.unwrap();
    }

    // Final state must be internally consistent for every mailbox.
    let seen = ts
        .keywords(&message)
        .await
        .unwrap()
        .contains(&alo_store::SEEN.to_owned());
    for mb_id in [&inbox, &folder] {
        let mb = ts.mailbox(mb_id).await.unwrap();
        assert!(
            mb.total_messages >= 0 && mb.unread_messages >= 0,
            "counters never negative"
        );
        let present = ts
            .list_mailbox(mb_id, alo_store::Page::default())
            .await
            .unwrap()
            .len() as i64;
        assert_eq!(mb.total_messages, present, "total matches membership");
        let expected_unread = if seen { 0 } else { present };
        assert_eq!(
            mb.unread_messages, expected_unread,
            "unread matches reality"
        );
    }
}
