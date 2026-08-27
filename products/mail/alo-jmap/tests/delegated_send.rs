//! Sending from a delegated shared mailbox (ADR 0017), proven on the wire —
//! the M2.2 shared-mailbox audit's send half. A delegate with a send grant
//! submits through the owner's account: `From:` stays the owner, `on_behalf`
//! discloses the acting delegate in a `Sender:` header on the wire copy only,
//! the sent copy lands in the OWNER's Sent (created on first use), and the
//! delegate's own mailbox receives nothing. The scheduled path (`/send-later`)
//! resolves the same delegation door, and the recorded acting delegate
//! survives to the sweep. Drives the REAL submission code against a REAL
//! Postgres store and a tiny in-process SMTP sink.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::sync::Arc;

use alo_identity::{Identity, IdentityConfig};
use alo_jmap::PushHub;
use alo_jmap::mime::{Addr, Outgoing, build};
use alo_jmap::state::{Account, AppState, Limits, resolve_target};
use alo_store::{AccountStore, BlobStore, MessageId, Store, TenantId, UserId};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

const NL: u8 = 10;
const DOT: u8 = 46;

/// The database this suite runs against (never the product's own).
fn database_url() -> String {
    alo_test_db::url()
}

fn undot(raw: &[u8]) -> Vec<u8> {
    let body = match raw.windows(5).position(|w| w == b"\r\n.\r\n") {
        Some(pos) => &raw[..pos + 2],
        None => raw,
    };
    let mut out = Vec::with_capacity(body.len());
    for seg in body.split_inclusive(|&b| b == NL) {
        if seg.first() == Some(&DOT) {
            out.extend_from_slice(&seg[1..]);
        } else {
            out.extend_from_slice(seg);
        }
    }
    out
}

/// A one-connection SMTP sink recording the envelope and DATA it is handed.
async fn spawn_sink() -> (String, tokio::task::JoinHandle<(Vec<String>, Vec<u8>)>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let handle = tokio::spawn(async move {
        let (stream, _peer) = listener.accept().await.unwrap();
        let (rd, mut wr) = stream.into_split();
        let mut reader = BufReader::new(rd);
        let mut rcpts = Vec::new();
        let mut data = Vec::new();
        wr.write_all(b"220 sink.test ESMTP\r\n").await.unwrap();
        let mut line = String::new();
        loop {
            line.clear();
            let n = reader.read_line(&mut line).await.unwrap();
            if n == 0 {
                break;
            }
            let cmd = line.trim_end();
            let upper = cmd.to_ascii_uppercase();
            if upper.starts_with("EHLO") || upper.starts_with("HELO") {
                wr.write_all(b"250 sink.test\r\n").await.unwrap();
            } else if upper.starts_with("MAIL FROM") {
                wr.write_all(b"250 2.1.0 ok\r\n").await.unwrap();
            } else if upper.starts_with("RCPT TO") {
                rcpts.push(cmd.to_owned());
                wr.write_all(b"250 2.1.5 ok\r\n").await.unwrap();
            } else if upper.starts_with("DATA") {
                wr.write_all(b"354 go\r\n").await.unwrap();
                loop {
                    let mut buf = [0u8; 4096];
                    let m = reader.read(&mut buf).await.unwrap();
                    if m == 0 {
                        break;
                    }
                    data.extend_from_slice(&buf[..m]);
                    if data.ends_with(b"\r\n.\r\n") {
                        break;
                    }
                }
                wr.write_all(b"250 2.0.0 queued\r\n").await.unwrap();
            } else if upper.starts_with("RSET") {
                wr.write_all(b"250 2.0.0 ok\r\n").await.unwrap();
            } else if upper.starts_with("QUIT") {
                wr.write_all(b"221 2.0.0 bye\r\n").await.unwrap();
                break;
            } else {
                wr.write_all(b"250 ok\r\n").await.unwrap();
            }
        }
        (rcpts, undot(&data))
    });
    (addr, handle)
}

fn app_state(store: &Arc<Store>, sink_addr: Option<String>) -> AppState {
    let identity =
        Identity::new(Arc::clone(store), IdentityConfig::new("https://id.test")).unwrap();
    AppState {
        media: None,
        turns: Default::default(),
        store: Arc::clone(store),
        identity,
        push: PushHub::new(),
        limits: Limits::default(),
        base_url: "http://test".into(),
        submission_addr: sink_addr,
        session_origins: Vec::new(),
        web_push: None,
        junk_learner: None,
        personal_domains: Vec::new(),
        signup_limiter: alo_identity::ratelimit::RateLimiter::new(),
    }
}

/// A signed-in (non-delegated) handle for `user`, as `authenticate` would
/// build it.
fn signed_in(store: &Arc<Store>, tenant: &TenantId, user: &UserId) -> Account {
    Account {
        tenant: tenant.clone(),
        user: user.clone(),
        acc: store.for_account(tenant.clone(), user.clone()),
        is_admin: false,
        roles: Vec::new(),
        denied_modules: Vec::new(),
        delegated: None,
    }
}

/// Owner fixtures: a Drafts mailbox holding one sendable draft, `From:` the
/// owner. Deliberately NO Sent mailbox — the audit found the first send from
/// a fresh shared mailbox had nowhere to be filed; Sent must appear on use.
async fn owner_draft(acc: &AccountStore, owner_email: &str, tag: &str) -> MessageId {
    let draft = Outgoing {
        from: Addr {
            name: None,
            email: owner_email.into(),
        },
        to: vec![Addr {
            name: None,
            email: "recipient@sink.test".into(),
        }],
        cc: Vec::new(),
        bcc: Vec::new(),
        subject: format!("shared mailbox {tag}"),
        in_reply_to: Vec::new(),
        references: Vec::new(),
        body_text: "sent by a delegate\n".into(),
        body_html: None,
        attachments: Vec::new(),
        message_id_domain: "sink.test".into(),
        message_id_token: format!("deleg{tag}"),
    };
    let raw = build(&draft);
    let drafts = acc
        .create_mailbox(None, "Drafts", Some("drafts"))
        .await
        .unwrap();
    let mid = acc.ingest(&drafts, &raw).await.unwrap();
    acc.set_keyword(&mid, "$draft", true).await.unwrap();
    mid
}

/// The owner's Sent mailbox id, asserting it exists (created on first use).
async fn sent_of(acc: &AccountStore) -> alo_store::MailboxId {
    acc.mailbox_by_role("sent")
        .await
        .unwrap()
        .expect("Sent must be created on first use by post_send")
}

async fn submit_as_delegate(
    state: &AppState,
    delegate: &Account,
    owner: &UserId,
    mid: &MessageId,
    owner_email: &str,
) -> Value {
    let target = resolve_target(delegate, state, owner.as_str())
        .await
        .expect("grant resolves the owner's mailbox");
    let args = json!({
        "accountId": owner.to_string(),
        "create": { "c1": {
            "emailId": mid.to_string(),
            "envelope": {
                "mailFrom": { "email": owner_email },
                "rcptTo": [ { "email": "recipient@sink.test" } ]
            }
        } }
    });
    alo_jmap::submission::set(&target, &args, state)
        .await
        .expect("EmailSubmission/set returned a method-level error")
}

#[tokio::test]
async fn on_behalf_send_discloses_the_delegate_and_files_to_owner_sent() {
    let Ok(pool) = sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url())
        .await
    else {
        eprintln!("SKIP: no database at {}", database_url());
        return;
    };
    let store = Arc::new(Store::new(pool, BlobStore::in_memory(50 * 1024 * 1024)));
    store.migrate().await.unwrap();

    let tenant = store.create_tenant("deleg-send-ob").await.unwrap();
    let ts = store.for_tenant(tenant.clone());
    let owner_email = format!("owner-{tenant}@sink.test").to_lowercase();
    let delegate_email = format!("delegate-{tenant}@sink.test").to_lowercase();
    let owner = ts.create_user(&owner_email).await.unwrap();
    let delegate = ts.create_user(&delegate_email).await.unwrap();
    let owner_acc = store.for_account(tenant.clone(), owner.clone());
    let mid = owner_draft(&owner_acc, &owner_email, "ob").await;

    ts.grant_delegate(&owner, &delegate, true, "on_behalf")
        .await
        .unwrap();

    let (sink_addr, sink) = spawn_sink().await;
    let state = app_state(&store, Some(sink_addr));
    let me = signed_in(&store, &tenant, &delegate);
    let resp = submit_as_delegate(&state, &me, &owner, &mid, &owner_email).await;
    assert!(resp["created"]["c1"].is_object(), "not sent: {resp}");

    let (rcpts, data) = sink.await.unwrap();
    let wire = String::from_utf8_lossy(&data);
    let headers = wire.split("\r\n\r\n").next().unwrap_or("");
    assert!(
        headers.contains(&format!("Sender: {delegate_email}")),
        "on-behalf must disclose the acting delegate on the wire:\n{headers}"
    );
    assert!(
        headers.contains(&format!("From: {owner_email}"))
            || headers.contains(&format!("<{owner_email}>")),
        "From stays the shared address:\n{headers}"
    );
    assert!(rcpts.iter().any(|r| r.contains("recipient@sink.test")));

    // The sent copy lands in the OWNER's Sent — created on first use — and the
    // stored copy stays as composed (the disclosure is a wire matter).
    let sent = sent_of(&owner_acc).await;
    let boxes = owner_acc.mailboxes_of_message(&mid).await.unwrap();
    assert!(
        boxes.iter().any(|b| b.as_str() == sent.as_str()),
        "sent copy must be filed into the owner's Sent"
    );
    let stored = owner_acc.message_bytes(&mid).await.unwrap();
    assert!(
        !String::from_utf8_lossy(&stored).contains("Sender:"),
        "the stored draft is never rewritten"
    );
    let keywords = owner_acc.keywords(&mid).await.unwrap();
    assert!(!keywords.iter().any(|k| k == "$draft"));
    assert!(keywords.iter().any(|k| k == "$seen"));

    // The delegate's own mailbox receives nothing.
    let delegate_acc = store.for_account(tenant.clone(), delegate.clone());
    assert!(
        delegate_acc.message(&mid).await.is_err(),
        "the delegate's personal account must hold no copy"
    );
    assert!(
        delegate_acc
            .mailbox_by_role("sent")
            .await
            .unwrap()
            .is_none(),
        "no Sent appears in the delegate's own account"
    );
}

#[tokio::test]
async fn send_as_adds_no_sender_header() {
    let Ok(pool) = sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url())
        .await
    else {
        eprintln!("SKIP: no database at {}", database_url());
        return;
    };
    let store = Arc::new(Store::new(pool, BlobStore::in_memory(50 * 1024 * 1024)));
    store.migrate().await.unwrap();

    let tenant = store.create_tenant("deleg-send-as").await.unwrap();
    let ts = store.for_tenant(tenant.clone());
    let owner_email = format!("owner-{tenant}@sink.test").to_lowercase();
    let delegate_email = format!("delegate-{tenant}@sink.test").to_lowercase();
    let owner = ts.create_user(&owner_email).await.unwrap();
    let delegate = ts.create_user(&delegate_email).await.unwrap();
    let owner_acc = store.for_account(tenant.clone(), owner.clone());
    let mid = owner_draft(&owner_acc, &owner_email, "as").await;

    ts.grant_delegate(&owner, &delegate, true, "as")
        .await
        .unwrap();

    let (sink_addr, sink) = spawn_sink().await;
    let state = app_state(&store, Some(sink_addr));
    let me = signed_in(&store, &tenant, &delegate);
    let resp = submit_as_delegate(&state, &me, &owner, &mid, &owner_email).await;
    assert!(resp["created"]["c1"].is_object(), "not sent: {resp}");

    let (_rcpts, data) = sink.await.unwrap();
    let wire = String::from_utf8_lossy(&data);
    let headers = wire.split("\r\n\r\n").next().unwrap_or("");
    assert!(
        !headers.to_ascii_lowercase().contains("\r\nsender:")
            && !headers.to_ascii_lowercase().starts_with("sender:"),
        "send-as is indistinguishable from the owner sending — no Sender:\n{headers}"
    );
    // Send-as files to the owner's Sent exactly as on-behalf does.
    let sent = sent_of(&owner_acc).await;
    let boxes = owner_acc.mailboxes_of_message(&mid).await.unwrap();
    assert!(boxes.iter().any(|b| b.as_str() == sent.as_str()));
}

/// The scheduled path, end to end over HTTP: a delegate schedules a send from
/// the shared mailbox through `/api/send-later` (the audit found this answered
/// 404 — the route never resolved the delegation), the acting delegate is
/// recorded on the schedule row, and the sweep puts the `Sender:` disclosure
/// on the wire and files the sent copy into the owner's Sent.
#[tokio::test]
async fn scheduled_on_behalf_send_survives_the_sweep() {
    let h = common::harness("sched-deleg").await;
    // h.user is the signed-in delegate; the owner is a second user.
    let owner_email = format!("sched-owner-{}@sink.test", h.tenant).to_lowercase();
    let owner = h.ts.create_user(&owner_email).await.unwrap();
    let owner_acc = h.store.for_account(h.tenant.clone(), owner.clone());
    let mid = owner_draft(&owner_acc, &owner_email, "sw").await;
    h.ts.grant_delegate(&owner, &h.user, true, "on_behalf")
        .await
        .unwrap();

    // Schedule for "now": valid to the route, due to the sweep.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let (status, body) = post(
        &h.app,
        &h.token,
        "/api/send-later",
        json!({
            "accountId": owner.to_string(),
            "emailId": mid.to_string(),
            "envelope": {
                "mailFrom": { "email": owner_email },
                "rcptTo": [ { "email": "recipient@sink.test" } ]
            },
            "sendAt": now
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "delegate can schedule: {body}");
    assert_eq!(body["scheduled"], json!(true));

    // The draft moved to the OWNER's Scheduled mailbox.
    let scheduled = owner_acc
        .mailbox_by_role("scheduled")
        .await
        .unwrap()
        .expect("scheduling creates the owner's Scheduled mailbox");
    let boxes = owner_acc.mailboxes_of_message(&mid).await.unwrap();
    assert!(boxes.iter().any(|b| b.as_str() == scheduled.as_str()));

    // Sweep: the recorded acting delegate reaches the wire.
    let (sink_addr, sink) = spawn_sink().await;
    let state = app_state(&h.store, Some(sink_addr));
    alo_jmap::submission::run_due_scheduled(&state).await;

    let (rcpts, data) = sink.await.unwrap();
    let wire = String::from_utf8_lossy(&data);
    let headers = wire.split("\r\n\r\n").next().unwrap_or("");
    assert!(
        headers.contains(&format!("Sender: {}", h.email)),
        "the acting delegate recorded at schedule time is disclosed on the wire:\n{headers}"
    );
    assert!(rcpts.iter().any(|r| r.contains("recipient@sink.test")));
    let sent = sent_of(&owner_acc).await;
    let boxes = owner_acc.mailboxes_of_message(&mid).await.unwrap();
    assert!(
        boxes.iter().any(|b| b.as_str() == sent.as_str()),
        "the swept send files into the owner's Sent"
    );
}

/// The delegation door on `/send-later` + `/send-later/cancel`: no grant is
/// the same 404 as an unknown draft (no oracle), a foreign tenant's delegate
/// is refused identically, a grant without send rights cannot schedule, a
/// read-only delegate cannot cancel, and a manage+send delegate can cancel —
/// returning the draft to the OWNER's Drafts.
#[tokio::test]
async fn send_later_enforces_the_delegation_door() {
    let h = common::harness("sched-authz").await;
    let owner_email = format!("authz-owner-{}@sink.test", h.tenant).to_lowercase();
    let owner = h.ts.create_user(&owner_email).await.unwrap();
    let owner_acc = h.store.for_account(h.tenant.clone(), owner.clone());
    let mid = owner_draft(&owner_acc, &owner_email, "az").await;
    // Far future so no concurrently running sweep can ever claim this row.
    let send_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 3000;
    let schedule_body = json!({
        "accountId": owner.to_string(),
        "emailId": mid.to_string(),
        "envelope": {
            "mailFrom": { "email": owner_email },
            "rcptTo": [ { "email": "recipient@sink.test" } ]
        },
        "sendAt": send_at
    });

    // No grant → 404, indistinguishable from an unknown id.
    let (status, _) = post(&h.app, &h.token, "/api/send-later", schedule_body.clone()).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "ungranted accountId is 404");

    // A delegate from ANOTHER tenant is refused identically (wrong-tenant).
    let h2 = common::harness_on(Arc::clone(&h.store), "sched-authz-b").await;
    let (status, _) = post(&h2.app, &h2.token, "/api/send-later", schedule_body.clone()).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "cross-tenant accountId is 404"
    );

    // Write access without a send grant cannot schedule a send.
    h.ts.grant_delegate(&owner, &h.user, true, "none")
        .await
        .unwrap();
    let (status, _) = post(&h.app, &h.token, "/api/send-later", schedule_body.clone()).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "no send grant → 403");

    // With a send grant the schedule lands.
    h.ts.grant_delegate(&owner, &h.user, true, "as")
        .await
        .unwrap();
    let (status, body) = post(&h.app, &h.token, "/api/send-later", schedule_body.clone()).await;
    assert_eq!(status, StatusCode::OK, "send grant schedules: {body}");

    // A read-only delegate may not cancel (a cancel is a mutation).
    h.ts.grant_delegate(&owner, &h.user, false, "none")
        .await
        .unwrap();
    let cancel_body = json!({ "accountId": owner.to_string(), "emailId": mid.to_string() });
    let (status, _) = post(
        &h.app,
        &h.token,
        "/api/send-later/cancel",
        cancel_body.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "read-only cannot cancel");

    // A manage delegate cancels; the draft returns to the OWNER's Drafts.
    h.ts.grant_delegate(&owner, &h.user, true, "as")
        .await
        .unwrap();
    let (status, body) = post(&h.app, &h.token, "/api/send-later/cancel", cancel_body).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["cancelled"], json!(true), "{body}");
    let drafts = owner_acc.mailbox_by_role("drafts").await.unwrap().unwrap();
    let boxes = owner_acc.mailboxes_of_message(&mid).await.unwrap();
    assert!(boxes.iter().any(|b| b.as_str() == drafts.as_str()));
}

async fn post(app: &axum::Router, token: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("POST")
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    common::send(app, req).await
}
