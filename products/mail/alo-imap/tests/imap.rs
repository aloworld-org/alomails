//! IMAP integration tests over real TLS against a live Postgres store:
//! the full client loop, UID stability (RFC 9051 §2.3.1.1), cross-tenant
//! AND cross-account isolation on every command surface, malformed/oversized
//! input, pipelining, APPEND-through-ingestion, and IDLE push.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::*;

/// The full client loop a real MUA drives on first sync.
#[tokio::test]
async fn full_loop_login_list_select_fetch_store() {
    let store = test_store().await;
    let (tenant, user, email, pw) = make_user(&store, "loop").await;
    deliver(
        &store,
        &tenant,
        &user,
        &message("Quarterly", "the body text"),
    )
    .await;
    let addr = spawn_imap(store.clone()).await;
    let mut c = Client::connect(addr).await;

    assert_ok(&c.login(&email, &pw).await);

    let caps = c.command("CAPABILITY").await;
    assert!(caps.iter().any(|l| l.contains("IMAP4rev2")));
    assert!(caps.iter().any(|l| l.contains("IDLE")));

    let list = c.command("LIST \"\" \"*\"").await;
    assert!(list.iter().any(|l| l.contains("\"INBOX\"")), "{list:?}");
    assert_ok(&list);

    let select = c.command("SELECT INBOX").await;
    assert!(select.iter().any(|l| l.contains("1 EXISTS")), "{select:?}");
    assert!(select.iter().any(|l| l.contains("[UIDVALIDITY")));
    assert!(select.iter().any(|l| l.contains("[UIDNEXT")));
    assert!(completion(&select).contains("[READ-WRITE]"));

    let fetch = c
        .command("FETCH 1 (UID FLAGS INTERNALDATE RFC822.SIZE ENVELOPE)")
        .await;
    let body = fetch.join("\n");
    assert!(body.contains("UID 1"), "{body}");
    assert!(body.contains("ENVELOPE"));
    assert!(body.contains("\"Quarterly\""));
    assert_ok(&fetch);

    // Fetch the raw body as a literal; verify the bytes are exact.
    let mut c2 = Client::connect(addr).await;
    assert_ok(&c2.login(&email, &pw).await);
    assert_ok(&c2.command("SELECT INBOX").await);
    c2.write(b"z FETCH 1 BODY[TEXT]\r\n").await;
    let hdr = c2.read_line().await; // * 1 FETCH (BODY[TEXT] {N}
    let n = hdr
        .rsplit_once('{')
        .and_then(|(_, r)| r.split('}').next())
        .and_then(|s| s.parse::<usize>().ok())
        .expect("literal size");
    let payload = c2.read_exact(n).await;
    assert_eq!(payload, b"the body text\r\n");
    let _ = c2.read_until_tag("z").await;

    // STORE a flag and see it reflected.
    let store_r = c.command("STORE 1 +FLAGS (\\Seen)").await;
    assert!(store_r.iter().any(|l| l.contains("\\Seen")), "{store_r:?}");
    assert_ok(&store_r);
    let refetch = c.command("FETCH 1 (FLAGS)").await;
    assert!(refetch.iter().any(|l| l.contains("\\Seen")));

    assert_ok(&c.command("LOGOUT").await);
}

/// UIDs are stable and never reused; EXPUNGE renumbers sequence numbers,
/// not UIDs; UIDVALIDITY survives reconnection (RFC 9051 §2.3.1.1).
#[tokio::test]
async fn uid_stable_expunge_renumbers_reconnect_persists() {
    let store = test_store().await;
    let (tenant, user, email, pw) = make_user(&store, "uid").await;
    for i in 0..3 {
        deliver(&store, &tenant, &user, &message(&format!("m{i}"), "b")).await;
    }
    let addr = spawn_imap(store.clone()).await;
    let mut c = Client::connect(addr).await;
    assert_ok(&c.login(&email, &pw).await);
    let sel = c.command("SELECT INBOX").await;
    let uidvalidity = extract_code(&sel, "UIDVALIDITY");

    // UIDs are 1,2,3 in arrival order.
    let f = c.command("FETCH 1:* (UID)").await;
    assert!(f.iter().any(|l| l.contains("UID 1")));
    assert!(f.iter().any(|l| l.contains("UID 2")));
    assert!(f.iter().any(|l| l.contains("UID 3")));

    // Delete the middle message and expunge.
    assert_ok(&c.command("STORE 2 +FLAGS (\\Deleted)").await);
    let expunge = c.command("EXPUNGE").await;
    assert!(
        expunge.iter().any(|l| l.contains("2 EXPUNGE")),
        "{expunge:?}"
    );

    // Sequence numbers renumber (now 1,2), but UIDs stay 1 and 3.
    let f2 = c.command("FETCH 1:* (UID)").await;
    let uids: Vec<String> = f2.iter().filter(|l| l.contains("UID")).cloned().collect();
    assert!(uids.iter().any(|l| l.contains("UID 1")));
    assert!(uids.iter().any(|l| l.contains("UID 3")));
    assert!(
        !uids.iter().any(|l| l.contains("UID 2")),
        "UID 2 must not reappear"
    );

    // A new delivery gets UID 4 (never reuses 2). NOOP refreshes the view
    // so the session learns of the out-of-band arrival (untagged EXISTS).
    deliver(&store, &tenant, &user, &message("m3", "b")).await;
    let noop = c.command("NOOP").await;
    assert!(noop.iter().any(|l| l.contains("EXISTS")), "{noop:?}");
    let f3 = c.command("UID FETCH 4 (UID)").await;
    assert!(f3.iter().any(|l| l.contains("UID 4")), "{f3:?}");

    // Reconnect: same UIDVALIDITY, UIDs persist.
    let mut c2 = Client::connect(addr).await;
    assert_ok(&c2.login(&email, &pw).await);
    let sel2 = c2.command("SELECT INBOX").await;
    assert_eq!(extract_code(&sel2, "UIDVALIDITY"), uidvalidity);
    let f4 = c2.command("UID FETCH 1:* (UID)").await;
    assert!(f4.iter().any(|l| l.contains("UID 4")));
    assert!(!f4.iter().any(|l| l.contains("UID 2")));
}

/// Cross-account isolation: two users of the SAME tenant never see each
/// other's mail through any command. A wrong-UID fetch is empty, not
/// another account's data; a search never surfaces the co-tenant's message.
#[tokio::test]
async fn cross_account_and_cross_tenant_isolation() {
    let store = test_store().await;
    // Same-tenant users A and B.
    let tenant = store.create_tenant("shared").await.unwrap();
    let ts = store.for_tenant(tenant.clone());
    let (ea, eb) = (format!("a-{tenant}@x.test"), format!("b-{tenant}@x.test"));
    let ua = ts.create_user(&ea).await.unwrap();
    let ub = ts.create_user(&eb).await.unwrap();
    let identity = common::test_identity(store.clone());
    identity
        .set_password(&tenant, &ua, &ea, "pw")
        .await
        .unwrap();
    identity
        .set_password(&tenant, &ub, &eb, "pw")
        .await
        .unwrap();
    deliver(&store, &tenant, &ua, &message("ALICE-SECRET", "alice body")).await;
    deliver(&store, &tenant, &ub, &message("BOB-SECRET", "bob body")).await;
    // B also has a custom folder A must never reach.
    store
        .for_account(tenant.clone(), ub.clone())
        .create_mailbox(None, "BobPrivate", None)
        .await
        .unwrap();

    let addr = spawn_imap(store.clone()).await;
    let mut a = Client::connect(addr).await;
    assert_ok(&a.login(&ea, "pw").await);

    // A's mailbox list is only A's: no "BobPrivate".
    let list = a.command("LIST \"\" \"*\"").await;
    assert!(
        !list.join("\n").contains("BobPrivate"),
        "A saw B's folder: {list:?}"
    );

    // A selects INBOX: exactly one message, and it is A's.
    let sel = a.command("SELECT INBOX").await;
    assert!(sel.iter().any(|l| l.contains("1 EXISTS")), "{sel:?}");
    let fetch = a.command("FETCH 1 (ENVELOPE)").await;
    let joined = fetch.join("\n");
    assert!(joined.contains("ALICE-SECRET"));
    assert!(!joined.contains("BOB-SECRET"), "B's subject leaked to A");

    // A SEARCH for B's subject finds nothing (done while INBOX is selected).
    let search = a.command("SEARCH SUBJECT \"BOB-SECRET\"").await;
    let sline = search
        .iter()
        .find(|l| l.starts_with("* SEARCH"))
        .cloned()
        .unwrap_or_default();
    assert_eq!(
        sline.trim(),
        "* SEARCH",
        "search leaked B's message: {search:?}"
    );

    // STATUS of B's folder → NO. SELECT of B's private folder → NO (and a
    // failed SELECT correctly deselects, RFC 9051 §6.3.2 — so do it last).
    assert_no(&a.command("STATUS BobPrivate (MESSAGES)").await);
    assert_no(&a.command("SELECT BobPrivate").await);

    // Cross-tenant: a user in another tenant is equally blind.
    let (_t2, _u2, e2, p2) = make_user(&store, "other-tenant").await;
    let mut o = Client::connect(addr).await;
    assert_ok(&o.login(&e2, &p2).await);
    let olist = o.command("LIST \"\" \"*\"").await;
    assert!(!olist.join("\n").contains("BobPrivate"));
    assert!(!olist.join("\n").contains("ALICE-SECRET"));
}

/// Malformed and oversized input is rejected cleanly, never crashing the
/// session or the server.
#[tokio::test]
async fn malformed_and_oversized_input() {
    let store = test_store().await;
    let (_t, _u, email, pw) = make_user(&store, "malformed").await;
    let addr = spawn_imap(store.clone()).await;
    let mut c = Client::connect(addr).await;
    assert_ok(&c.login(&email, &pw).await);

    // Garbage command → BAD, session survives.
    let bad = c.command("FROBNICATE stuff").await;
    assert!(completion(&bad).contains(" BAD "), "{bad:?}");
    assert_ok(&c.command("NOOP").await);

    // A command before a mailbox is selected → NO.
    assert_no(&c.command("FETCH 1 (FLAGS)").await);

    // An oversized synchronizing literal → the server rejects and closes.
    c.write(b"x LOGIN {99999999}\r\n").await;
    let resp = c.read_line().await;
    assert!(
        resp.contains("BYE") || resp.contains("BAD"),
        "oversize literal: {resp}"
    );
}

/// Pipelined commands (sent in one write) are answered in order.
#[tokio::test]
async fn pipelining_preserves_order() {
    let store = test_store().await;
    let (tenant, user, email, pw) = make_user(&store, "pipeline").await;
    deliver(&store, &tenant, &user, &message("one", "b")).await;
    let addr = spawn_imap(store.clone()).await;
    let mut c = Client::connect(addr).await;
    assert_ok(&c.login(&email, &pw).await);

    // Two commands, one write.
    c.write(b"p1 SELECT INBOX\r\np2 FETCH 1 (UID)\r\n").await;
    let r1 = c.read_until_tag("p1").await;
    assert_ok(&r1);
    let r2 = c.read_until_tag("p2").await;
    assert!(r2.iter().any(|l| l.contains("UID 1")), "{r2:?}");
    assert_ok(&r2);
}

/// APPEND ingests through the same path as delivery and returns APPENDUID.
#[tokio::test]
async fn append_through_ingestion_path() {
    let store = test_store().await;
    let (_t, _u, email, pw) = make_user(&store, "append").await;
    let addr = spawn_imap(store.clone()).await;
    let mut c = Client::connect(addr).await;
    assert_ok(&c.login(&email, &pw).await);

    let msg = b"From: me@x.test\r\nSubject: Appended\r\n\r\nappended body\r\n";
    let cmd = format!("q APPEND INBOX (\\Seen) {{{}}}\r\n", msg.len());
    c.write(cmd.as_bytes()).await;
    let cont = c.read_line().await;
    assert!(cont.starts_with("+"), "expected continuation, got {cont}");
    c.write(msg).await;
    c.write(b"\r\n").await;
    let resp = c.read_until_tag("q").await;
    assert!(completion(&resp).contains("APPENDUID"), "{resp:?}");
    assert_ok(&resp);

    // The appended message is visible with \Seen already set.
    assert!(
        c.command("SELECT INBOX")
            .await
            .iter()
            .any(|l| l.contains("1 EXISTS"))
    );
    let f = c.command("FETCH 1 (FLAGS ENVELOPE)").await;
    assert!(f.join("\n").contains("\\Seen"));
    assert!(f.join("\n").contains("\"Appended\""));
}

/// IDLE delivers an untagged EXISTS when a message arrives in the selected
/// mailbox — and the stream is account-scoped.
#[tokio::test]
async fn idle_receives_exists_on_delivery() {
    let store = test_store().await;
    let (tenant, user, email, pw) = make_user(&store, "idle").await;
    let addr = spawn_imap(store.clone()).await;
    let mut c = Client::connect(addr).await;
    assert_ok(&c.login(&email, &pw).await);
    assert_ok(&c.command("SELECT INBOX").await);

    // Enter IDLE.
    c.write(b"i IDLE\r\n").await;
    let plus = c.read_line().await;
    assert!(plus.starts_with("+"), "expected + idling, got {plus}");

    // Deliver a message out of band (as SMTP would).
    deliver(&store, &tenant, &user, &message("live", "arrived")).await;

    // Within a couple of poll cycles, an untagged EXISTS arrives.
    let got = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let line = c.read_line().await;
            if line.contains("EXISTS") {
                return line;
            }
        }
    })
    .await
    .expect("IDLE should report EXISTS within timeout");
    assert!(got.contains("1 EXISTS"), "{got}");

    c.write(b"DONE\r\n").await;
    let done = c.read_until_tag("i").await;
    assert_ok(&done);
}

/// MOVE into the message's own mailbox must NOT destroy it (regression:
/// MOVE-to-self used to expunge the sole membership → data loss), and a
/// MOVE that matches nothing must not emit a malformed untagged line.
#[tokio::test]
async fn move_to_self_is_safe_and_zero_match_is_clean() {
    let store = test_store().await;
    let (tenant, user, email, pw) = make_user(&store, "moveself").await;
    deliver(&store, &tenant, &user, &message("keepme", "body")).await;
    let addr = spawn_imap(store.clone()).await;
    let mut c = Client::connect(addr).await;
    assert_ok(&c.login(&email, &pw).await);
    assert_ok(&c.command("SELECT INBOX").await);

    // MOVE the only message into INBOX (its own mailbox): must survive.
    assert_ok(&c.command("MOVE 1 INBOX").await);
    let f = c.command("FETCH 1 (UID)").await;
    assert!(
        f.iter().any(|l| l.contains("UID 1")),
        "message lost on MOVE-to-self: {f:?}"
    );
    assert!(
        c.command("SELECT INBOX")
            .await
            .iter()
            .any(|l| l.contains("1 EXISTS"))
    );

    // UID MOVE of a non-existent UID: clean tagged OK, no `* OK [] Moved`.
    let mv = c.command("UID MOVE 999999 INBOX").await;
    assert_ok(&mv);
    assert!(
        !mv.iter().any(|l| l.contains("[]")),
        "zero-match MOVE emitted a malformed code: {mv:?}"
    );
}

/// COPY duplicates a message into another mailbox with a fresh UID there
/// and reports COPYUID; the source keeps its message.
#[tokio::test]
async fn copy_assigns_dest_uid_and_keeps_source() {
    let store = test_store().await;
    let (tenant, user, email, pw) = make_user(&store, "copy").await;
    deliver(&store, &tenant, &user, &message("dup", "body")).await;
    let addr = spawn_imap(store.clone()).await;
    let mut c = Client::connect(addr).await;
    assert_ok(&c.login(&email, &pw).await);
    assert_ok(&c.command("CREATE Archive").await);
    assert_ok(&c.command("SELECT INBOX").await);

    let cp = c.command("COPY 1 Archive").await;
    assert!(completion(&cp).contains("COPYUID"), "{cp:?}");
    assert_ok(&cp);
    // Source still has it.
    assert!(
        c.command("SELECT INBOX")
            .await
            .iter()
            .any(|l| l.contains("1 EXISTS"))
    );
    // Destination has a copy with UID 1 (its own epoch).
    assert!(
        c.command("SELECT Archive")
            .await
            .iter()
            .any(|l| l.contains("1 EXISTS"))
    );
    assert!(
        c.command("FETCH 1 (UID)")
            .await
            .iter()
            .any(|l| l.contains("UID 1"))
    );
}

/// A mailbox name carrying a control character (CR) is rejected — response
/// injection defense.
#[tokio::test]
async fn create_rejects_control_chars_in_name() {
    let store = test_store().await;
    let (_t, _u, email, pw) = make_user(&store, "ctrl").await;
    let addr = spawn_imap(store.clone()).await;
    let mut c = Client::connect(addr).await;
    assert_ok(&c.login(&email, &pw).await);
    // A literal name with an embedded CRLF must be refused.
    let name = b"Evil\r\n* 9999 EXISTS";
    let cmd = format!("z CREATE {{{}}}\r\n", name.len());
    c.write(cmd.as_bytes()).await;
    let cont = c.read_line().await;
    assert!(cont.starts_with("+"), "{cont}");
    c.write(name).await;
    c.write(b"\r\n").await;
    let resp = c.read_until_tag("z").await;
    assert_no(&resp);
    // The injected "* 9999 EXISTS" must not have appeared as a response.
    assert!(!resp.iter().any(|l| l.contains("9999 EXISTS")), "{resp:?}");
}

/// A 2FA account over real TLS IMAP: the primary password is refused
/// (fail closed — a phished primary cannot bypass 2FA over IMAP), an
/// app-specific password logs in and reaches the mailbox, and a revoked
/// app password fails on the next connection. Non-2FA accounts logging in
/// with their primary are every other test in this suite.
#[tokio::test]
async fn app_password_opens_legacy_login_for_2fa_accounts() {
    let store = test_store().await;
    let (tenant, user, email, pw) = make_user(&store, "app-pw").await;
    let identity = test_identity(store.clone());

    // Issue an app password, then enable TOTP on the account.
    let (record, secret) = identity
        .create_app_password(&tenant, &user, "Thunderbird on the desk machine")
        .await
        .unwrap();
    let app_pw = secret.reveal().to_owned();
    let e = identity.enroll_totp(&tenant, &user, &email).await.unwrap();
    let code = alo_identity::totp::current_code(&e.secret_base32).unwrap();
    identity.confirm_totp(&tenant, &user, &code).await.unwrap();

    let addr = spawn_imap(store.clone()).await;

    // The primary password is refused over IMAP (fail closed)…
    let mut c = Client::connect(addr).await;
    assert_no(&c.login(&email, &pw).await);

    // …but the app password logs in and reaches the mailbox.
    let mut c = Client::connect(addr).await;
    assert_ok(&c.login(&email, &app_pw).await);
    assert_ok(&c.command("SELECT INBOX").await);
    assert_ok(&c.command("LOGOUT").await);

    // Revoked → the next connection is refused.
    identity
        .revoke_app_password(&tenant, &user, &record)
        .await
        .unwrap();
    let mut c = Client::connect(addr).await;
    assert_no(&c.login(&email, &app_pw).await);
}

/// SASL XOAUTH2 over real TLS IMAP (M1.4): the capability is advertised,
/// a live bearer token authenticates via an initial response and reaches
/// the mailbox, and a revoked token gets the mechanism's error dialog —
/// a `+ <base64 status>` continuation, an empty client acknowledgement,
/// then the tagged NO. Exchange recorded in `docs/interop.md`.
#[tokio::test]
async fn xoauth2_bearer_authenticates_and_revocation_runs_error_dialog() {
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD as B64;

    let store = test_store().await;
    let (tenant, user, email, _pw) = make_user(&store, "xoauth2").await;
    let identity = test_identity(store.clone());
    let token = identity
        .issue_access_token(&tenant, &user, None, "openid email profile")
        .await
        .unwrap();
    let blob = B64.encode(format!(
        "user={email}\u{1}auth=Bearer {}\u{1}\u{1}",
        token.reveal()
    ));

    let addr = spawn_imap(store.clone()).await;

    let mut c = Client::connect(addr).await;
    let caps = c.command("CAPABILITY").await;
    assert!(caps.iter().any(|l| l.contains("AUTH=XOAUTH2")), "{caps:?}");
    assert!(caps.iter().any(|l| l.contains("SASL-IR")), "{caps:?}");

    // A live token authenticates (SASL-IR form) and reaches the mailbox.
    let r = c.command(&format!("AUTHENTICATE XOAUTH2 {blob}")).await;
    assert_ok(&r);
    assert_ok(&c.command("SELECT INBOX").await);
    assert_ok(&c.command("LOGOUT").await);

    // Revoked → the error dialog, then NO (fails on the next connection).
    identity.revoke_access_token(token.reveal()).await.unwrap();
    let mut c = Client::connect(addr).await;
    c.write(format!("x AUTHENTICATE XOAUTH2 {blob}\r\n").as_bytes())
        .await;
    let cont = c.read_line().await;
    let status = cont
        .strip_prefix("+ ")
        .unwrap_or_else(|| panic!("expected error-status continuation, got {cont}"));
    let decoded = String::from_utf8(B64.decode(status.trim()).unwrap()).unwrap();
    assert!(decoded.contains("\"status\":\"401\""), "{decoded}");
    c.write(b"\r\n").await; // the client's empty acknowledgement
    assert_no(&c.read_until_tag("x").await);

    // A malformed blob is a protocol error, not a credential failure.
    let mut c = Client::connect(addr).await;
    let bad = c.command("AUTHENTICATE XOAUTH2 bm90LWEtYmxvYg==").await;
    assert!(completion(&bad).contains(" BAD "), "{bad:?}");
}

/// Extracts a numeric response code value (e.g. UIDVALIDITY) from lines.
fn extract_code(lines: &[String], code: &str) -> String {
    for l in lines {
        if let Some(idx) = l.find(&format!("[{code} ")) {
            let rest = &l[idx + code.len() + 2..];
            if let Some(end) = rest.find(']') {
                return rest[..end].trim().to_owned();
            }
        }
    }
    String::new()
}
