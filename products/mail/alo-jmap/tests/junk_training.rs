//! End-to-end junk training: an `Email/set` move into the Junk folder
//! must POST the raw message to Rspamd's `/learnspam`, a move back out
//! `/learnham`, and a keyword-only update must train nothing. Driven
//! through the real JMAP router against Postgres, with a mock Rspamd
//! controller capturing what arrives on the wire.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use alo_jmap::junk_learn::JunkLearner;
use alo_jmap::state::AppState;
use common::{api, call, database_url, test_identity};
use serde_json::json;

/// The learn calls the mock controller has seen: `(path, body)`.
type Seen = Arc<Mutex<Vec<(String, Vec<u8>)>>>;

/// A mock Rspamd controller: records `(path, body)` of every POST and
/// answers `{"success":true}`.
async fn mock_rspamd() -> (std::net::SocketAddr, Seen) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let seen: Seen = Arc::new(Mutex::new(Vec::new()));
    let record = Arc::clone(&seen);
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                return;
            };
            let record = Arc::clone(&record);
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                // Read until the full body arrived (content-length).
                loop {
                    let Ok(n) = sock.read(&mut chunk).await else {
                        return;
                    };
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    if let Some(header_end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&buf[..header_end]).into_owned();
                        let length: usize = head
                            .lines()
                            .find_map(|l| {
                                l.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(|v| v.trim().parse().unwrap_or(0))
                            })
                            .unwrap_or(0);
                        if buf.len() >= header_end + 4 + length {
                            let path = head
                                .lines()
                                .next()
                                .and_then(|l| l.split(' ').nth(1))
                                .unwrap_or("")
                                .to_owned();
                            let body = buf[header_end + 4..header_end + 4 + length].to_vec();
                            record.lock().unwrap().push((path, body));
                            let reply = b"HTTP/1.1 200 OK\r\ncontent-length: 16\r\nconnection: close\r\n\r\n{\"success\":true}";
                            let _ = sock.write_all(reply).await;
                            return;
                        }
                    }
                }
            });
        }
    });
    (addr, seen)
}

/// Waits until the mock has recorded `n` learn calls (or panics).
async fn wait_for_learns(seen: &Seen, n: usize) {
    for _ in 0..100 {
        if seen.lock().unwrap().len() >= n {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!(
        "expected {n} learn calls, saw {:?}",
        seen.lock()
            .unwrap()
            .iter()
            .map(|(p, b)| (p.clone(), b.len()))
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn junk_moves_train_rspamd_and_keyword_changes_do_not() {
    let (mock_addr, seen) = mock_rspamd().await;

    // A harness with the learner pointed at the mock (AppState built by
    // hand — the env-driven path is covered by junk_learn's unit tests).
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url())
        .await
        .expect("connect to test postgres");
    let store = Arc::new(alo_store::Store::new(
        pool,
        alo_store::BlobStore::in_memory(50 * 1024 * 1024),
    ));
    store.migrate().await.unwrap();
    let tenant = store.create_tenant("jmap-junklearn").await.unwrap();
    let email = format!("junklearn-{tenant}@example.test");
    let ts = store.for_tenant(tenant.clone());
    let user = ts.create_user(&email).await.unwrap();
    let identity = test_identity(Arc::clone(&store));
    identity
        .set_password(&tenant, &user, &email, "s3cret-pw")
        .await
        .unwrap();
    let acc = store.for_account(tenant.clone(), user.clone());
    let token = identity
        .password_login(&email, "s3cret-pw", None)
        .await
        .unwrap()
        .expect("token issued")
        .0
        .reveal()
        .to_owned();
    let app = alo_jmap::app(AppState {
        turns: Default::default(),
        store: Arc::clone(&store),
        identity: identity.clone(),
        push: alo_jmap::push::PushHub::new(),
        limits: alo_jmap::state::Limits::default(),
        base_url: "http://test".into(),
        submission_addr: None,
        junk_learner: JunkLearner::new(format!("http://{mock_addr}"), None),
        personal_domains: Vec::new(),
        signup_limiter: alo_identity::ratelimit::RateLimiter::new(),
    });
    let account_id = user.to_string();

    // A message in the Inbox, and a Junk folder to move it into.
    let inbox = acc.inbox().await.unwrap();
    let junk = acc
        .create_mailbox(None, "Junk", Some("junk"))
        .await
        .unwrap();
    let raw = b"From: spammer@ext.test\r\nTo: victim@example.test\r\nSubject: junk train wire\r\n\r\nbuy things\r\n";
    let mid = acc.ingest(&inbox, raw).await.unwrap();

    // 1) Keyword-only update: no training.
    let (status, body) = api(
        &app,
        &token,
        call(
            "Email/set",
            json!({"accountId": account_id, "update": { mid.as_str(): {"keywords/$seen": true} }}),
        ),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(
        seen.lock().unwrap().is_empty(),
        "keyword change must not train"
    );

    // 2) Move into Junk → learnspam with the message bytes.
    let (status, body) = api(
        &app,
        &token,
        call(
            "Email/set",
            json!({"accountId": account_id, "update": { mid.as_str(): {
                "mailboxIds": { junk.to_string(): true }
            }}}),
        ),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert!(
        body.to_string().contains("updated"),
        "move must succeed: {body}"
    );
    wait_for_learns(&seen, 1).await;
    {
        let calls = seen.lock().unwrap();
        assert_eq!(calls[0].0, "/learnspam");
        assert!(
            String::from_utf8_lossy(&calls[0].1).contains("junk train wire"),
            "the raw message must be the learn body"
        );
    }

    // 3) Move back to the Inbox → learnham.
    let (status, body) = api(
        &app,
        &token,
        call(
            "Email/set",
            json!({"accountId": account_id, "update": { mid.as_str(): {
                "mailboxIds": { inbox.to_string(): true }
            }}}),
        ),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    wait_for_learns(&seen, 2).await;
    {
        let calls = seen.lock().unwrap();
        assert_eq!(calls[1].0, "/learnham");
    }

    store.delete_tenant(&tenant).await.unwrap();
}
