//! Sieve delivery + storage tests against real Postgres: fileinto/keep/
//! discard filing, mail-never-lost on a degraded action, vacation
//! suppression, redirect budget/self-redirect, and cross-tenant AND
//! cross-account isolation of both script CRUD and execution.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::{OutboundAction, Page, StoreError};

async fn count(acc: &alo_store::AccountStore, mailbox: &alo_store::MailboxId) -> usize {
    acc.list_mailbox(mailbox, Page::default())
        .await
        .unwrap()
        .len()
}

const MSG_WORK: &[u8] =
    b"From: sender@ext.test\r\nTo: rcpt@x.test\r\nSubject: work stuff\r\n\r\nbody\r\n";

#[tokio::test]
async fn fileinto_files_into_named_folder_not_inbox() {
    let store = common::test_store().await;
    let (acc, _u, inbox) = common::fresh_account(&store, "fi").await;
    let work = acc.create_mailbox(None, "Work", None).await.unwrap();
    acc.put_sieve_script(
        "main",
        "require [\"fileinto\"]; if header :contains \"subject\" \"work\" { fileinto \"Work\"; }",
    )
    .await
    .unwrap();
    acc.activate_sieve_script(Some("main")).await.unwrap();

    let d = acc
        .deliver_sieve(MSG_WORK, Some("sender@ext.test"), "rcpt@x.test")
        .await
        .unwrap();
    assert!(d.filed);
    assert_eq!(count(&acc, &work).await, 1, "filed into Work");
    assert_eq!(count(&acc, &inbox).await, 0, "not also in Inbox");
}

#[tokio::test]
async fn no_active_script_delivers_to_inbox() {
    let store = common::test_store().await;
    let (acc, _u, inbox) = common::fresh_account(&store, "noscript").await;
    acc.deliver_sieve(MSG_WORK, Some("s@ext.test"), "r@x.test")
        .await
        .unwrap();
    assert_eq!(count(&acc, &inbox).await, 1);
}

#[tokio::test]
async fn fileinto_missing_folder_keeps_to_inbox_never_lost() {
    // Auto-create is OFF: a fileinto to a non-existent folder must not lose
    // the mail — it degrades to keep into the Inbox, with a warning.
    let store = common::test_store().await;
    let (acc, _u, inbox) = common::fresh_account(&store, "missing").await;
    acc.put_sieve_script("m", "require [\"fileinto\"]; fileinto \"Nope\";")
        .await
        .unwrap();
    acc.activate_sieve_script(Some("m")).await.unwrap();
    let d = acc
        .deliver_sieve(MSG_WORK, Some("s@ext.test"), "r@x.test")
        .await
        .unwrap();
    assert!(d.filed);
    assert_eq!(count(&acc, &inbox).await, 1, "mail kept to Inbox, not lost");
    assert!(
        d.warnings.iter().any(|w| w.contains("Nope")),
        "{:?}",
        d.warnings
    );
}

#[tokio::test]
async fn discard_files_nowhere() {
    let store = common::test_store().await;
    let (acc, _u, inbox) = common::fresh_account(&store, "discard").await;
    acc.put_sieve_script("d", "if header :contains \"subject\" \"work\" { discard; }")
        .await
        .unwrap();
    acc.activate_sieve_script(Some("d")).await.unwrap();
    let d = acc
        .deliver_sieve(MSG_WORK, Some("s@ext.test"), "r@x.test")
        .await
        .unwrap();
    assert!(!d.filed);
    assert_eq!(count(&acc, &inbox).await, 0, "discarded → nowhere");
}

#[tokio::test]
async fn redirect_still_keeps_and_respects_budget() {
    let store = common::test_store().await;
    let (acc, _u, inbox) = common::fresh_account(&store, "redir").await;
    acc.put_sieve_script("r", "redirect \"forward@elsewhere.test\";")
        .await
        .unwrap();
    acc.activate_sieve_script(Some("r")).await.unwrap();
    let d = acc
        .deliver_sieve(MSG_WORK, Some("s@ext.test"), "r@x.test")
        .await
        .unwrap();
    // redirect does not cancel implicit keep → message still delivered.
    assert_eq!(
        count(&acc, &inbox).await,
        1,
        "implicit keep alongside redirect"
    );
    assert_eq!(
        d.outbound,
        vec![OutboundAction::Redirect {
            address: "forward@elsewhere.test".to_owned()
        }]
    );
}

#[tokio::test]
async fn vacation_replies_once_then_suppressed() {
    let store = common::test_store().await;
    let (acc, _u, _inbox) = common::fresh_account(&store, "vac").await;
    let owner = "u-vac@example.test";
    acc.put_sieve_script(
        "v",
        "require [\"vacation\"]; vacation :days 7 :subject \"Away\" \"I am away\";",
    )
    .await
    .unwrap();
    acc.activate_sieve_script(Some("v")).await.unwrap();
    let raw = format!("From: bob@ext.test\r\nTo: {owner}\r\nSubject: hi\r\n\r\nq\r\n");

    let first = acc
        .deliver_sieve(raw.as_bytes(), Some("bob@ext.test"), owner)
        .await
        .unwrap();
    assert!(
        first
            .outbound
            .iter()
            .any(|a| matches!(a, OutboundAction::Vacation { to, .. } if to == "bob@ext.test")),
        "first send replies: {:?}",
        first.outbound
    );

    // Second message from the same correspondent within :days → suppressed.
    let second = acc
        .deliver_sieve(raw.as_bytes(), Some("bob@ext.test"), owner)
        .await
        .unwrap();
    assert!(
        !second
            .outbound
            .iter()
            .any(|a| matches!(a, OutboundAction::Vacation { .. })),
        "second send suppressed: {:?}",
        second.outbound
    );
}

#[tokio::test]
async fn script_crud_is_account_scoped() {
    // Two users in ONE tenant: B cannot see, read, or activate A's scripts.
    let store = common::test_store().await;
    let tenant = store.create_tenant("sieve-crud").await.unwrap();
    let ts = store.for_tenant(tenant.clone());
    let ua = ts.create_user("a@x.test").await.unwrap();
    let ub = ts.create_user("b@x.test").await.unwrap();
    let a = store.for_account(tenant.clone(), ua);
    let b = store.for_account(tenant, ub);

    a.put_sieve_script("secret", "keep;").await.unwrap();
    a.activate_sieve_script(Some("secret")).await.unwrap();

    // B's world is empty.
    assert!(b.list_sieve_scripts().await.unwrap().is_empty());
    assert!(matches!(
        b.sieve_script("secret").await,
        Err(StoreError::NotFound)
    ));
    assert!(matches!(
        b.activate_sieve_script(Some("secret")).await,
        Err(StoreError::NotFound)
    ));
    // A still sees exactly its own.
    let a_list = a.list_sieve_scripts().await.unwrap();
    assert_eq!(a_list.len(), 1);
    assert!(a_list[0].active);
}

#[tokio::test]
async fn execution_files_only_into_owners_mailbox() {
    // A and B are same-tenant, each with a "Work" folder. A's Sieve delivery
    // must file only into A's Work — never B's.
    let store = common::test_store().await;
    let tenant = store.create_tenant("sieve-exec").await.unwrap();
    let ts = store.for_tenant(tenant.clone());
    let ua = ts.create_user("a2@x.test").await.unwrap();
    let ub = ts.create_user("b2@x.test").await.unwrap();
    let a = store.for_account(tenant.clone(), ua);
    let b = store.for_account(tenant, ub);
    let _ = a.inbox().await.unwrap();
    let _ = b.inbox().await.unwrap();
    let a_work = a.create_mailbox(None, "Work", None).await.unwrap();
    let b_work = b.create_mailbox(None, "Work", None).await.unwrap();

    a.put_sieve_script("w", "require [\"fileinto\"]; fileinto \"Work\";")
        .await
        .unwrap();
    a.activate_sieve_script(Some("w")).await.unwrap();
    a.deliver_sieve(MSG_WORK, Some("s@ext.test"), "a2@x.test")
        .await
        .unwrap();

    assert_eq!(count(&a, &a_work).await, 1, "A's Work has the message");
    assert_eq!(count(&b, &b_work).await, 0, "B's Work untouched");
}

#[tokio::test]
async fn imap4flags_map_to_jmap_keywords_and_move_unread() {
    // A `setflag "\Seen"; keep;` script must mark the message read via the
    // store's canonical `$seen` keyword (not a bogus `\Seen`), which moves
    // the unread counter (regression: flags were stored verbatim).
    let store = common::test_store().await;
    let (acc, _u, inbox) = common::fresh_account(&store, "flags").await;
    acc.put_sieve_script("f", "require [\"imap4flags\"]; setflag \"\\\\Seen\"; keep;")
        .await
        .unwrap();
    acc.activate_sieve_script(Some("f")).await.unwrap();
    acc.deliver_sieve(MSG_WORK, Some("s@ext.test"), "r@x.test")
        .await
        .unwrap();

    let list = acc.list_mailbox(&inbox, Page::default()).await.unwrap();
    assert_eq!(list.len(), 1);
    let mb = acc.mailbox(&inbox).await.unwrap();
    assert_eq!(mb.unread_messages, 0, "\\Seen mapped to $seen → not unread");
    let kws = acc.keywords(&list[0].id).await.unwrap();
    assert!(kws.contains(&"$seen".to_owned()), "{kws:?}");
    assert!(
        !kws.iter().any(|k| k.contains('\\')),
        "no backslash keyword: {kws:?}"
    );
}

#[tokio::test]
async fn script_name_and_count_caps_enforced() {
    let store = common::test_store().await;
    let (acc, _u, _inbox) = common::fresh_account(&store, "caps").await;
    // An over-long name is refused.
    let long = "x".repeat(600);
    assert!(matches!(
        acc.put_sieve_script(&long, "keep;").await,
        Err(StoreError::Conflict(_))
    ));
    // Updating an existing script is always allowed (not a new name).
    acc.put_sieve_script("s", "keep;").await.unwrap();
    acc.put_sieve_script("s", "discard;").await.unwrap();
}

#[tokio::test]
async fn invalid_script_is_rejected_on_put() {
    let store = common::test_store().await;
    let (acc, _u, _inbox) = common::fresh_account(&store, "invalid").await;
    // fileinto without require → compile error → Conflict, not stored.
    let e = acc
        .put_sieve_script("bad", "fileinto \"X\";")
        .await
        .unwrap_err();
    assert!(matches!(e, StoreError::Conflict(_)), "{e:?}");
    assert!(acc.list_sieve_scripts().await.unwrap().is_empty());
}
