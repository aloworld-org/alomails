//! Behavioural tests for review/audit fixes: multibyte-subject ingest
//! (no panic), the keyword cap, cross-user filing denial, and FTS.
//! Runs against real Postgres.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::{Page, SEEN, StoreError};

#[tokio::test]
async fn ingest_with_multibyte_subject_does_not_panic() {
    // Regression (cold-review HIGH): a multibyte Subject used to panic
    // base_subject on the ingest path. This must ingest cleanly.
    let store = common::test_store().await;
    let (ts, _user, inbox) = common::fresh_account(&store, "utf8").await;
    let raw =
        "From: a@example.test\r\nSubject: a€ ☕ 😀 Re: thing\r\nMessage-ID: <u@x>\r\n\r\nbody\r\n";
    ts.ingest(&inbox, raw.as_bytes()).await.unwrap();
    assert_eq!(
        ts.list_mailbox(&inbox, Page::default())
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn keyword_count_and_length_are_capped() {
    let store = common::test_store().await;
    let (ts, _user, inbox) = common::fresh_account(&store, "kwcap").await;
    let m = common::deliver(&ts, &inbox, "<k@x>", &[], "hi").await;

    let mut ok = 0;
    for i in 0..70 {
        match ts.set_keyword(&m, &format!("kw{i}"), true).await {
            Ok(()) => ok += 1,
            Err(StoreError::Conflict(_)) => break,
            Err(e) => panic!("unexpected error {e:?}"),
        }
    }
    assert_eq!(ok, 64, "capped at MAX_KEYWORDS distinct keywords");
    assert_eq!(ts.keywords(&m).await.unwrap().len(), 64);

    // An over-long keyword is rejected before it is stored.
    assert!(matches!(
        ts.set_keyword(&m, &"x".repeat(200), true).await,
        Err(StoreError::Conflict(_))
    ));
}

#[tokio::test]
async fn cannot_file_message_into_another_users_mailbox() {
    // Cross-user (within one tenant) filing is denied with NotFound.
    let store = common::test_store().await;
    let tenant = store.create_tenant("cross-user").await.unwrap();
    let ts = store.for_tenant(tenant.clone());
    let u1 = ts.create_user("u1@example.test").await.unwrap();
    let u2 = ts.create_user("u2@example.test").await.unwrap();
    let a = store.for_account(tenant.clone(), u1);
    let b = store.for_account(tenant, u2);
    let ib1 = a.inbox().await.unwrap();
    let ib2 = b.inbox().await.unwrap();
    let m1 = a
        .ingest(&ib1, b"From: a@example.test\r\nSubject: s\r\n\r\nb\r\n")
        .await
        .unwrap();

    // a's own message cannot be filed into b's mailbox (foreign mailbox).
    assert!(matches!(
        a.add_to_mailbox(&m1, &ib2).await,
        Err(StoreError::NotFound)
    ));
    // a cannot ingest into b's mailbox either.
    assert!(matches!(
        a.ingest(&ib2, b"From: a@example.test\r\n\r\nb\r\n").await,
        Err(StoreError::NotFound)
    ));
}

#[tokio::test]
async fn oversize_ingest_is_rejected_before_work() {
    let store = common::test_store().await;
    let (ts, _user, inbox) = common::fresh_account(&store, "oversize").await;
    // The test blob store ceiling is 25 MiB; exceed it.
    let big = vec![b'x'; 26 * 1024 * 1024];
    assert!(matches!(
        ts.ingest(&inbox, &big).await,
        Err(StoreError::TooLarge { .. })
    ));
    assert!(
        ts.list_mailbox(&inbox, Page::default())
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn full_text_search_matches_subject() {
    let store = common::test_store().await;
    let (ts, _user, inbox) = common::fresh_account(&store, "fts").await;
    common::deliver(&ts, &inbox, "<s1@x>", &[], "Quarterly revenue report").await;
    common::deliver(&ts, &inbox, "<s2@x>", &[], "Lunch plans").await;

    let hits = ts.search("quarterly", Page::default()).await.unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].subject, "Quarterly revenue report");
    // A term in neither message returns nothing (still bounded, no error).
    assert!(
        ts.search("zzznomatch", Page::default())
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn set_seen_toggles_unread_counter() {
    let store = common::test_store().await;
    let (ts, _user, inbox) = common::fresh_account(&store, "seen").await;
    let m = common::deliver(&ts, &inbox, "<m@x>", &[], "hi").await;
    assert_eq!(ts.mailbox(&inbox).await.unwrap().unread_messages, 1);
    ts.set_keyword(&m, SEEN, true).await.unwrap();
    assert_eq!(ts.mailbox(&inbox).await.unwrap().unread_messages, 0);
    ts.set_keyword(&m, SEEN, false).await.unwrap();
    assert_eq!(ts.mailbox(&inbox).await.unwrap().unread_messages, 1);
}

#[tokio::test]
async fn changes_pagination_never_drops_a_modseq_group() {
    // Regression (cold-review HIGH 1): several objects of one type can
    // share a modseq (one transaction records many). A maxChanges cut
    // inside that group must not advance the state past it and silently
    // drop the siblings. Here $seen records Mailbox/updated for BOTH the
    // inbox and a folder at one modseq; paging one-at-a-time must still
    // surface both.
    let store = common::test_store().await;
    let (ts, _user, inbox) = common::fresh_account(&store, "chgroup").await;
    let m = common::deliver(&ts, &inbox, "<m@x>", &[], "hi").await;
    let folder = ts.create_mailbox(None, "F", None).await.unwrap();
    ts.add_to_mailbox(&m, &folder).await.unwrap();
    let s0: i64 = ts.state().await.unwrap().parse().unwrap();

    ts.set_keyword(&m, SEEN, true).await.unwrap();

    let mut seen = std::collections::HashSet::new();
    let mut since = s0;
    for _ in 0..8 {
        let c = ts
            .changes(alo_store::changes::TYPE_MAILBOX, since, 1)
            .await
            .unwrap();
        for id in c.updated.iter().chain(c.created.iter()) {
            seen.insert(id.clone());
        }
        since = c.new_state;
        if !c.has_more {
            break;
        }
    }
    assert!(
        seen.contains(&inbox.to_string()) && seen.contains(&folder.to_string()),
        "both mailboxes in the split group must surface across pages"
    );
}
