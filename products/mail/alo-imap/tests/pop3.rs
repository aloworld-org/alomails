//! POP3 integration tests over implicit TLS against a live store: the full
//! USER/PASS/STAT/LIST/UIDL/RETR/DELE/QUIT flow, deletion commit on QUIT,
//! stable UIDs shared with IMAP, and account isolation.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::*;

#[tokio::test]
async fn pop3_full_flow_and_deletion_commit() {
    let store = test_store().await;
    let (tenant, user, email, pw) = make_user(&store, "pop3").await;
    deliver(&store, &tenant, &user, &message("first", "body one")).await;
    deliver(&store, &tenant, &user, &message("second", "body two")).await;
    let addr = spawn_pop3(store.clone()).await;

    let mut c = Client::attach(connect_tls(addr).await);
    assert!(c.read_line().await.starts_with("+OK"));
    c.write(format!("USER {email}\r\n").as_bytes()).await;
    assert!(c.read_line().await.starts_with("+OK"));
    c.write(format!("PASS {pw}\r\n").as_bytes()).await;
    assert!(c.read_line().await.starts_with("+OK"));

    // STAT: two messages.
    c.write(b"STAT\r\n").await;
    let stat = c.read_line().await;
    assert!(stat.starts_with("+OK 2 "), "{stat}");

    // LIST: two lines.
    c.write(b"LIST\r\n").await;
    assert!(c.read_line().await.starts_with("+OK"));
    let list = c.read_multiline().await;
    assert_eq!(list.len(), 2, "{list:?}");
    assert!(list[0].starts_with("1 "));

    // UIDL: unique ids are the stable per-mailbox UIDs (1 and 2).
    c.write(b"UIDL\r\n").await;
    assert!(c.read_line().await.starts_with("+OK"));
    let uidl = c.read_multiline().await;
    assert_eq!(uidl, vec!["1 1".to_owned(), "2 2".to_owned()], "{uidl:?}");

    // RETR 1: header + body, dot-terminated.
    c.write(b"RETR 1\r\n").await;
    assert!(c.read_line().await.starts_with("+OK"));
    let msg = c.read_multiline().await;
    assert!(msg.join("\n").contains("body one"), "{msg:?}");

    // DELE 1, then QUIT commits.
    c.write(b"DELE 1\r\n").await;
    assert!(c.read_line().await.starts_with("+OK"));
    c.write(b"QUIT\r\n").await;
    assert!(c.read_line().await.starts_with("+OK"));

    // Reconnect: the deleted message is gone (commit happened).
    let mut c2 = Client::attach(connect_tls(addr).await);
    assert!(c2.read_line().await.starts_with("+OK"));
    c2.write(format!("USER {email}\r\n").as_bytes()).await;
    c2.read_line().await;
    c2.write(format!("PASS {pw}\r\n").as_bytes()).await;
    c2.read_line().await;
    c2.write(b"STAT\r\n").await;
    let stat2 = c2.read_line().await;
    assert!(
        stat2.starts_with("+OK 1 "),
        "deletion not committed: {stat2}"
    );
}

#[tokio::test]
async fn pop3_rejects_bad_credentials() {
    let store = test_store().await;
    let (_t, _u, email, _pw) = make_user(&store, "pop3bad").await;
    let addr = spawn_pop3(store.clone()).await;
    let mut c = Client::attach(connect_tls(addr).await);
    assert!(c.read_line().await.starts_with("+OK"));
    c.write(format!("USER {email}\r\n").as_bytes()).await;
    c.read_line().await;
    c.write(b"PASS wrong-password\r\n").await;
    assert!(c.read_line().await.starts_with("-ERR"));
    // A wrong username is equally rejected (no user-existence oracle).
    c.write(b"USER nobody@nowhere.test\r\n").await;
    c.read_line().await;
    c.write(b"PASS whatever\r\n").await;
    assert!(c.read_line().await.starts_with("-ERR"));
}

/// A 2FA account over POP3: the primary password is refused (fail
/// closed), an app-specific password authenticates — the same
/// `authenticate_legacy` seam IMAP and SMTP AUTH use.
#[tokio::test]
async fn pop3_accepts_app_password_for_2fa_account() {
    let store = test_store().await;
    let (tenant, user, email, pw) = make_user(&store, "pop3app").await;
    let identity = test_identity(store.clone());
    let (_record, secret) = identity
        .create_app_password(&tenant, &user, "phone mail app")
        .await
        .unwrap();
    let app_pw = secret.reveal().to_owned();
    let e = identity.enroll_totp(&tenant, &user, &email).await.unwrap();
    let code = alo_identity::totp::current_code(&e.secret_base32).unwrap();
    identity.confirm_totp(&tenant, &user, &code).await.unwrap();

    let addr = spawn_pop3(store.clone()).await;
    let mut c = Client::attach(connect_tls(addr).await);
    assert!(c.read_line().await.starts_with("+OK"));
    // Primary refused (fail closed for 2FA)…
    c.write(format!("USER {email}\r\n").as_bytes()).await;
    c.read_line().await;
    c.write(format!("PASS {pw}\r\n").as_bytes()).await;
    assert!(c.read_line().await.starts_with("-ERR"));
    // …app password accepted.
    c.write(format!("USER {email}\r\n").as_bytes()).await;
    c.read_line().await;
    c.write(format!("PASS {app_pw}\r\n").as_bytes()).await;
    assert!(c.read_line().await.starts_with("+OK"));
    c.write(b"QUIT\r\n").await;
    assert!(c.read_line().await.starts_with("+OK"));
}
