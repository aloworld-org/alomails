//! Concurrent-session correctness: two clients select the same mailbox;
//! one flags and expunges, and the other sees the correct untagged
//! `FETCH (FLAGS ...)` and `EXPUNGE` updates on its next command.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::*;

#[tokio::test]
async fn second_session_sees_flag_and_expunge_updates() {
    let store = test_store().await;
    let (tenant, user, email, pw) = make_user(&store, "concurrent").await;
    for i in 0..3 {
        deliver(&store, &tenant, &user, &message(&format!("m{i}"), "b")).await;
    }
    let addr = spawn_imap(store.clone()).await;

    let mut a = Client::connect(addr).await;
    let mut b = Client::connect(addr).await;
    assert_ok(&a.login(&email, &pw).await);
    assert_ok(&b.login(&email, &pw).await);
    assert_ok(&a.command("SELECT INBOX").await);
    assert_ok(&b.command("SELECT INBOX").await);

    // A marks message 1 \Seen.
    assert_ok(&a.command("STORE 1 +FLAGS (\\Seen)").await);
    // B's NOOP surfaces the flag change as an untagged FETCH.
    let bn = b.command("NOOP").await;
    assert!(
        bn.iter()
            .any(|l| l.contains("FETCH") && l.contains("\\Seen")),
        "B did not see the flag change: {bn:?}"
    );

    // A deletes and expunges message 2.
    assert_ok(&a.command("STORE 2 +FLAGS (\\Deleted)").await);
    assert_ok(&a.command("EXPUNGE").await);
    // B's NOOP surfaces the EXPUNGE.
    let bn2 = b.command("NOOP").await;
    assert!(
        bn2.iter().any(|l| l.contains("EXPUNGE")),
        "B did not see the expunge: {bn2:?}"
    );

    // Both sessions now agree the mailbox holds 2 messages.
    let ba = b.command("FETCH 1:* (UID)").await;
    let uids: Vec<&String> = ba.iter().filter(|l| l.contains("UID")).collect();
    assert_eq!(uids.len(), 2, "{ba:?}");
}
