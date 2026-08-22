//! Tenancy proof for the MAPI contents read (Law 1: isolation is tested, not
//! assumed).
//!
//! `mapi_mailbox_rows` takes a mailbox id and returns the messages in it. A
//! mailbox id is an opaque string a caller could hold from anywhere, so the
//! question this suite answers is what happens when one account passes
//! another's: the answer must be an empty list, indistinguishable from an empty
//! mailbox, and never that account's mail.
//!
//! Two users in one tenant and a user in a second exercise both halves of the
//! `(tenant, user)` predicate — a same-tenant colleague is refused by the user
//! half, and a foreign tenant by the tenant half. Runs against a throwaway
//! Postgres; skips cleanly when none is available.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use alo_store::model::Page;
use alo_store::{BlobStore, Store};
use sqlx::postgres::PgPoolOptions;

/// The database this suite runs against.
///
/// Delegates to `alo_test_db`, which refuses the database the product
/// runs on: suites create and drop their own, they never write into `alo`.
fn database_url() -> String {
    alo_test_db::url()
}

const RAW: &[u8] = b"From: sender@example.test\r\nTo: owner@example.test\r\n\
                     Subject: Rechnung\r\n\r\nbody\r\n";

#[tokio::test]
async fn a_mailboxs_messages_are_readable_only_by_the_account_that_owns_it() {
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

    // Tenant A with two users; tenant B with one.
    let ta = store.create_tenant("mapi-a").await.unwrap();
    let tsa = store.for_tenant(ta.clone());
    let alice = tsa.create_user("alice@mapi-a.test").await.unwrap();
    let bob = tsa.create_user("bob@mapi-a.test").await.unwrap();
    let tb = store.create_tenant("mapi-b").await.unwrap();
    let carol = store
        .for_tenant(tb.clone())
        .create_user("carol@mapi-b.test")
        .await
        .unwrap();

    let alice_store = store.for_account(ta.clone(), alice.clone());
    let bob_store = store.for_account(ta.clone(), bob.clone());
    let carol_store = store.for_account(tb.clone(), carol.clone());

    // Alice has a mailbox with one message in it.
    let inbox = alice_store.inbox().await.unwrap();
    alice_store.deliver(RAW).await.unwrap();

    let mine = alice_store
        .mapi_mailbox_rows(&inbox, Page::first(50))
        .await
        .unwrap();
    assert_eq!(mine.len(), 1, "the owner sees her own message");
    assert_eq!(mine[0].subject, "Rechnung");

    // A colleague in the same tenant, holding Alice's mailbox id, sees
    // nothing. Not an error — an empty list, which is what an empty mailbox
    // looks like, so nothing here confirms that the mailbox exists.
    let colleague = bob_store
        .mapi_mailbox_rows(&inbox, Page::first(50))
        .await
        .unwrap();
    assert!(colleague.is_empty(), "a colleague sees no message of hers");

    // And a user in another tenant, likewise.
    let stranger = carol_store
        .mapi_mailbox_rows(&inbox, Page::first(50))
        .await
        .unwrap();
    assert!(stranger.is_empty(), "another tenant sees nothing at all");
}

#[tokio::test]
async fn the_read_state_and_attachment_flag_are_the_accounts_own() {
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

    let tenant = store.create_tenant("mapi-flags").await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user("flags@mapi.test")
        .await
        .unwrap();
    let account = store.for_account(tenant.clone(), user.clone());

    let inbox = account.inbox().await.unwrap();
    let message = account.deliver(RAW).await.unwrap();

    // Delivered mail starts unread, which is what makes a row bold.
    let before = account
        .mapi_mailbox_rows(&inbox, Page::first(50))
        .await
        .unwrap();
    assert_eq!(before.len(), 1);
    assert!(!before[0].seen, "newly delivered mail is unread");
    assert!(!before[0].has_attachment);
    assert!(before[0].size > 0, "a real byte count, not a placeholder");

    // Marking it read moves the flag this read reports.
    account.set_keyword(&message, "$seen", true).await.unwrap();
    let after = account
        .mapi_mailbox_rows(&inbox, Page::first(50))
        .await
        .unwrap();
    assert!(after[0].seen, "the read state is the one the store holds");
}

#[tokio::test]
async fn the_page_bounds_what_one_request_can_pull() {
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

    let tenant = store.create_tenant("mapi-page").await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user("page@mapi.test")
        .await
        .unwrap();
    let account = store.for_account(tenant.clone(), user.clone());
    let inbox = account.inbox().await.unwrap();

    for n in 0..5 {
        let raw = format!(
            "From: s@example.test\r\nTo: o@example.test\r\nSubject: m{n}\r\n\
             Message-ID: <m{n}@example.test>\r\n\r\nbody\r\n"
        );
        account.deliver(raw.as_bytes()).await.unwrap();
    }

    // The ceiling is what makes a single Execute's cost independent of how
    // much mail somebody has.
    let page = account
        .mapi_mailbox_rows(&inbox, Page::first(2))
        .await
        .unwrap();
    assert_eq!(page.len(), 2, "the page bounds the read");

    let all = account
        .mapi_mailbox_rows(&inbox, Page::first(50))
        .await
        .unwrap();
    assert_eq!(all.len(), 5);
    // Newest first — the order a mail client shows by default.
    assert_eq!(all[0].subject, "m4");
    assert_eq!(all[4].subject, "m0");
}
