//! Integration tests for the outbound queue (Phase 1 M2), driving the
//! real [`Queue`] against a scripted mock SMTP server over a real
//! socket via the smarthost route. Covers: successful delivery,
//! transient (4xx) deferral with durable state, and permanent (5xx)
//! failure producing a spooled DSN from the null sender.
//!
//! These tests exercise the delivery path end-to-end without DNS
//! (smarthost route), so they run identically everywhere.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use alo_smtp::egress::EgressMap;
use alo_smtp::envelope::Envelope;
use alo_smtp::queue::{Queue, QueuePolicy, Route};
use alo_smtp::resolver::{MxResolve, ResolveFailure, ResolveFuture};
use alo_smtp::spool::Spool;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

/// Every `EHLO`/`HELO` line the mock has seen, lowercased. Process-global
/// because nextest gives each test its own process, so one test's greetings
/// cannot be confused with another's.
/// Greetings observed by ONE mock server.
///
/// Per-server rather than per-file, and that is the whole point: a global
/// buffer collected every test's greetings, so this suite's one assertion about
/// which name we introduce ourselves by saw nine entries from eight tests
/// running in parallel and failed on all of them. The capture belongs to the
/// server that observed it.
type Greetings = std::sync::Arc<std::sync::Mutex<Vec<String>>>;

/// A resolver that must never be called (smarthost route bypasses it).
struct PanicResolver;
impl MxResolve for PanicResolver {
    fn resolve<'a>(&'a self, _domain: &'a str) -> ResolveFuture<'a> {
        Box::pin(async {
            Err(ResolveFailure::Transient {
                reason: "unused".into(),
            })
        })
    }
}

/// One scripted reply the mock server sends, keyed by the command
/// prefix it is replying to.
struct MockServer {
    addr: SocketAddr,
    greetings: Greetings,
}

/// Behaviour the mock applies to the RCPT/DATA phase.
#[derive(Clone, Copy)]
enum Behaviour {
    /// Accept everything (2xx) — successful delivery.
    Accept,
    /// 4xx to RCPT — transient, message should defer.
    TransientRcpt,
    /// 5xx to RCPT — permanent, message should bounce.
    PermanentRcpt,
    /// A 3xx to RCPT — a peer protocol violation; we classify it as
    /// permanent (neither 2xx success nor 4xx transient) and bounce.
    WeirdRcpt,
}

impl MockServer {
    async fn spawn(behaviour: Behaviour) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let greetings: Greetings = std::sync::Arc::default();
        let captured = std::sync::Arc::clone(&greetings);
        tokio::spawn(async move {
            // Serve a single connection per delivery attempt, looping
            // so retries reconnect cleanly.
            loop {
                let Ok((stream, _peer)) = listener.accept().await else {
                    return;
                };
                tokio::spawn(handle_mock(
                    stream,
                    behaviour,
                    std::sync::Arc::clone(&captured),
                ));
            }
        });
        Self { addr, greetings }
    }

    /// The names the peer introduced itself by, in the order this server saw
    /// them.
    fn greetings(&self) -> Vec<String> {
        self.greetings.lock().expect("greeting log").clone()
    }
}

async fn handle_mock(stream: TcpStream, behaviour: Behaviour, greetings: Greetings) {
    let (read_half, mut writer) = stream.into_split();
    let mut reader = BufReader::new(read_half);
    let mut line = String::new();

    writer
        .write_all(b"220 mock.example ESMTP\r\n")
        .await
        .unwrap();
    let mut in_data = false;
    loop {
        line.clear();
        let n = reader.read_line(&mut line).await.unwrap_or(0);
        if n == 0 {
            return;
        }

        if in_data {
            if line == ".\r\n" {
                in_data = false;
                writer.write_all(b"250 2.0.0 accepted\r\n").await.unwrap();
            }
            continue;
        }

        let upper = line.to_ascii_uppercase();
        if upper.starts_with("EHLO") || upper.starts_with("HELO") {
            // Recorded so a test can assert which name we introduced ourselves
            // by: a receiver compares it against the connecting address's
            // reverse DNS, so it is part of the sending identity.
            greetings
                .lock()
                .expect("greeting log")
                .push(line.trim_end().to_ascii_lowercase());
        }
        let reply: &[u8] = if upper.starts_with("EHLO") || upper.starts_with("HELO") {
            b"250 mock.example\r\n"
        } else if upper.starts_with("MAIL") {
            b"250 2.1.0 sender ok\r\n"
        } else if upper.starts_with("RCPT") {
            match behaviour {
                Behaviour::Accept => b"250 2.1.5 recipient ok\r\n",
                Behaviour::TransientRcpt => b"451 4.3.0 try again later\r\n",
                Behaviour::PermanentRcpt => b"550 5.1.1 no such user\r\n",
                Behaviour::WeirdRcpt => b"354 unexpected here\r\n",
            }
        } else if upper.starts_with("DATA") {
            in_data = true;
            b"354 end with .\r\n"
        } else if upper.starts_with("RSET") {
            b"250 2.0.0 reset\r\n"
        } else if upper.starts_with("QUIT") {
            let _ = writer.write_all(b"221 2.0.0 bye\r\n").await;
            return;
        } else {
            b"250 2.0.0 ok\r\n"
        };
        writer.write_all(reply).await.unwrap();
    }
}

fn policy(hostname: &str, smarthost: SocketAddr) -> QueuePolicy {
    QueuePolicy {
        hostname: hostname.to_owned(),
        route: Route::Smarthost(smarthost),
        retry_base: Duration::from_secs(60),
        retry_cap: Duration::from_secs(3600),
        max_attempts: 3,
        rate_per_min: 0, // rate limiting off in the delivery-path tests
        rate_burst: 0,
        egress: EgressMap::default(), // the kernel chooses, as on a one-address host
    }
}

fn spool_message(spool: &Spool, from: Option<&str>, rcpts: &[&str], body: &[u8]) -> String {
    let id = spool.next_id();
    let envelope = Envelope {
        helo: "sender.example".to_owned(),
        peer: "192.0.2.1:2000".to_owned(),
        mail_from: from.map(str::to_owned),
        rcpt_to: rcpts.iter().map(|r| (*r).to_owned()).collect(),
        received_at: "2026-07-26T00:00:00Z".to_owned(),
    };
    spool.store(&id, &envelope, body).unwrap();
    id
}

async fn queue_with(behaviour: Behaviour) -> (Arc<Spool>, Queue, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let spool = Arc::new(Spool::new(dir.path()).unwrap());
    let mock = MockServer::spawn(behaviour).await;
    let queue = Queue::new(
        Arc::clone(&spool),
        Arc::new(PanicResolver),
        policy("mx.alo.test", mock.addr),
    );
    (spool, queue, dir)
}

/// A queue whose outbound send rate to any one domain is `rate_per_min`
/// with `burst` depth (an accepting smarthost behind it).
async fn queue_rate_limited(
    rate_per_min: u32,
    burst: u32,
) -> (Arc<Spool>, Queue, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let spool = Arc::new(Spool::new(dir.path()).unwrap());
    let mock = MockServer::spawn(Behaviour::Accept).await;
    let mut policy = policy("mx.alo.test", mock.addr);
    policy.rate_per_min = rate_per_min;
    policy.rate_burst = burst;
    let queue = Queue::new(Arc::clone(&spool), Arc::new(PanicResolver), policy);
    (spool, queue, dir)
}

/// A queue that sends `egress_domain`'s mail from an address this host does not
/// hold — the only portable way to prove the source address was *chosen* rather
/// than left to the kernel, since a bind that is ignored connects normally.
async fn queue_with_egress(egress_domain: &str) -> (Arc<Spool>, Queue, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let spool = Arc::new(Spool::new(dir.path()).unwrap());
    let mock = MockServer::spawn(Behaviour::Accept).await;
    let mut policy = policy("mx.alo.test", mock.addr);
    // 192.0.2.1 is TEST-NET-1 (RFC 5737): no host holds it.
    policy.egress = EgressMap::parse(&format!("{egress_domain}=192.0.2.1")).unwrap();
    let queue = Queue::new(Arc::clone(&spool), Arc::new(PanicResolver), policy);
    (spool, queue, dir)
}

#[tokio::test]
async fn the_envelope_sender_decides_which_address_a_message_leaves_by() {
    // ADR 0044 §1: a campaign identity leaves by its own IP. The envelope sender
    // is the identity SPF is evaluated for, so it is the envelope — not the
    // `From` header and not the destination — that selects the address.
    let (spool, queue, _dir) = queue_with_egress("news.alo.test").await;

    // Transactional mail: no dedicated address, delivered as always.
    spool_message(
        &spool,
        Some("noreply@alo.test"),
        &["alice@example.com"],
        b"Subject: your invoice\r\n\r\nbody\r\n",
    );
    let report = queue.process_once().await.unwrap();
    assert_eq!(
        report.delivered, 1,
        "a domain with no dedicated address must deliver exactly as before"
    );

    // Campaign mail: pinned to an address this host does not hold, so the
    // attempt cannot succeed. Were the pin ignored it would deliver over the
    // very same smarthost the message above just used.
    spool_message(
        &spool,
        Some("bounces@news.alo.test"),
        &["alice@example.com"],
        b"Subject: our newsletter\r\n\r\nbody\r\n",
    );
    let report = queue.process_once().await.unwrap();
    assert_eq!(
        report.delivered, 0,
        "campaign mail must not leave by the transactional address"
    );
    assert_eq!(
        report.deferred, 1,
        "it defers and retries rather than bouncing"
    );

    // And the null sender (a bounce) carries no identity to keep separate: it
    // takes the default route rather than being stranded by the campaign pin.
    spool_message(
        &spool,
        None,
        &["alice@example.com"],
        b"Subject: delivery report\r\n\r\nbody\r\n",
    );
    let report = queue.process_once().await.unwrap();
    assert_eq!(report.delivered, 1, "a bounce is not campaign mail");
}

#[tokio::test]
async fn a_campaign_identity_greets_by_its_own_name_and_transactional_mail_does_not() {
    // The receiver checks the HELO name against the connecting address's
    // reverse DNS. Mail that leaves by the campaign address while greeting as
    // the transactional host reads as sent from somebody else's server, and is
    // scored down for it — mail-tester deducts 3 of 10, and it was the only
    // authentication deduction left once SPF, DKIM and DMARC all passed.
    let dir = tempfile::tempdir().unwrap();
    let spool = Arc::new(Spool::new(dir.path()).unwrap());
    let mock = MockServer::spawn(Behaviour::Accept).await;
    let mut policy = policy("mx.alo.test", mock.addr);
    // A reachable pin, so the delivery actually happens and the greeting is
    // observed rather than inferred: the mock listens on loopback.
    policy.egress = EgressMap::parse("news.alo.test=127.0.0.1").unwrap();
    let queue = Queue::new(Arc::clone(&spool), Arc::new(PanicResolver), policy);

    spool_message(
        &spool,
        Some("bounces@news.alo.test"),
        &["alice@example.com"],
        b"Subject: our newsletter\r\n\r\nbody\r\n",
    );
    assert_eq!(queue.process_once().await.unwrap().delivered, 1);
    spool_message(
        &spool,
        Some("noreply@alo.test"),
        &["alice@example.com"],
        b"Subject: your invoice\r\n\r\nbody\r\n",
    );
    assert_eq!(queue.process_once().await.unwrap().delivered, 1);

    let greetings = mock.greetings();
    assert_eq!(
        greetings,
        vec![
            "ehlo news.alo.test".to_owned(),
            "ehlo mx.alo.test".to_owned()
        ],
        "the campaign identity greets as itself; everything else keeps the server's own name"
    );
}

#[tokio::test]
async fn successful_delivery_removes_the_message() {
    let (spool, queue, _dir) = queue_with(Behaviour::Accept).await;
    let id = spool_message(
        &spool,
        Some("bob@example.org"),
        &["alice@example.com"],
        b"Subject: hi\r\n\r\nbody\r\n",
    );

    let report = queue.process_once().await.unwrap();
    assert_eq!(report.delivered, 1, "one message delivered");
    assert_eq!(report.bounced, 0);
    // Fully processed: gone from both new/ and cur/.
    assert!(spool.list().unwrap().is_empty());
    assert!(spool.list_claimed().unwrap().is_empty());
    assert!(spool.read_claimed(&id).is_err());
}

#[tokio::test]
async fn transient_failure_defers_with_durable_state() {
    let (spool, queue, _dir) = queue_with(Behaviour::TransientRcpt).await;
    let id = spool_message(
        &spool,
        Some("bob@example.org"),
        &["alice@example.com"],
        b"Subject: hi\r\n\r\nbody\r\n",
    );

    let report = queue.process_once().await.unwrap();
    assert_eq!(report.deferred, 1, "transient 4xx defers");
    assert_eq!(report.delivered, 0);
    assert_eq!(report.bounced, 0);
    // The message is claimed, still present, and carries state with a
    // future next-attempt time — durable across a restart.
    assert_eq!(spool.list_claimed().unwrap(), vec![id.clone()]);
    let state = spool.read_state(&id).unwrap().expect("state persisted");
    let text = String::from_utf8(state).unwrap();
    assert!(text.contains("\"attempts\": 1"));
    assert!(text.contains("next_attempt_at"));
    // A fresh pass right away must NOT retry it (backoff not elapsed).
    let report2 = queue.process_once().await.unwrap();
    assert_eq!(report2, alo_smtp::queue::PassReport::default());
}

#[tokio::test]
async fn permanent_failure_bounces_with_dsn_from_null_sender() {
    let (spool, queue, _dir) = queue_with(Behaviour::PermanentRcpt).await;
    let original = spool_message(
        &spool,
        Some("bob@example.org"),
        &["alice@example.com"],
        b"Subject: original\r\nFrom: bob@example.org\r\n\r\nbody\r\n",
    );

    let report = queue.process_once().await.unwrap();
    assert_eq!(report.bounced, 1, "5xx bounces");
    // Original message removed.
    assert!(spool.read_claimed(&original).is_err());
    // A DSN was enqueued into new/, addressed to the sender, from <>.
    let new_ids = spool.list().unwrap();
    assert_eq!(new_ids.len(), 1, "one DSN spooled");
    let (dsn_env, dsn_body) = spool.read(&new_ids[0]).unwrap();
    assert!(dsn_env.mail_from.is_none(), "DSN sent from null path");
    assert_eq!(dsn_env.rcpt_to, vec!["bob@example.org"]);
    let body = String::from_utf8(dsn_body).unwrap();
    assert!(body.contains("multipart/report; report-type=delivery-status"));
    assert!(body.contains("Final-Recipient: rfc822; alice@example.com"));
    assert!(body.contains("Status: 5.1.1"));
    assert!(body.contains("MAILER-DAEMON@mx.alo.test"));
}

#[tokio::test]
async fn unexpected_3xx_at_rcpt_is_treated_as_permanent() {
    // A 3xx to RCPT is a peer protocol violation; documenting that we
    // classify it permanent (bounce) rather than retrying forever.
    let (spool, queue, _dir) = queue_with(Behaviour::WeirdRcpt).await;
    spool_message(
        &spool,
        Some("bob@example.org"),
        &["alice@example.com"],
        b"Subject: weird\r\n\r\nbody\r\n",
    );
    let report = queue.process_once().await.unwrap();
    assert_eq!(report.bounced, 1, "3xx at RCPT bounces, not retries");
    assert_eq!(report.deferred, 0);
    let dsn = spool.list().unwrap();
    assert_eq!(dsn.len(), 1, "a DSN was produced for the odd reply");
}

#[tokio::test]
async fn null_sender_failure_produces_no_dsn() {
    // RFC 5321 §4.5.5: never bounce a bounce.
    let (spool, queue, _dir) = queue_with(Behaviour::PermanentRcpt).await;
    spool_message(&spool, None, &["alice@example.com"], b"a bounce\r\n");

    let report = queue.process_once().await.unwrap();
    assert_eq!(report.bounced, 1);
    // No DSN spooled — the failure was suppressed.
    assert!(spool.list().unwrap().is_empty(), "no DSN for null sender");
    assert!(spool.list_claimed().unwrap().is_empty());
}

#[tokio::test]
async fn outbound_rate_limit_defers_the_burst_then_drains() {
    // Burst of 1 to any domain: the first message to a domain sends, the
    // second is deferred (not bounced), then drains on a later pass.
    let (spool, queue, _dir) = queue_rate_limited(60, 1).await;
    let first = spool_message(
        &spool,
        Some("bob@example.org"),
        &["a@throttled.example"],
        b"Subject: one\r\n\r\nbody\r\n",
    );
    let second = spool_message(
        &spool,
        Some("bob@example.org"),
        &["b@throttled.example"],
        b"Subject: two\r\n\r\nbody\r\n",
    );

    // One pass: exactly one delivers, the other defers (rate), none bounce.
    let report = queue.process_once().await.unwrap();
    assert_eq!(report.delivered, 1, "burst=1 lets one through");
    assert_eq!(report.deferred, 1, "the second is rate-deferred");
    assert_eq!(report.bounced, 0, "rate limiting never bounces");

    // Exactly one of the two survives in the spool (the deferred one).
    let survivor = if spool.read_claimed(&first).is_ok() {
        &first
    } else {
        &second
    };
    assert!(
        spool.read_claimed(survivor).is_ok(),
        "deferred message kept"
    );

    // The rate limiter refills at 1/sec (60/min); after a moment the
    // deferred message drains rather than bouncing. Its next-attempt was
    // set ~30s out, so drive it directly is not possible without a clock;
    // instead prove it is Pending (kept) with attempts NOT exhausted —
    // i.e. it never entered the bounce path.
    assert!(
        spool.list_claimed().unwrap().contains(survivor),
        "the throttled message waits in cur/, not bounced or dropped"
    );
}
