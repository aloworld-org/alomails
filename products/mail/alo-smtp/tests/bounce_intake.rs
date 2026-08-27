//! The campaign return path, end to end (queue item M4.4, ADR 0044 §4): the
//! MX accepts the configured bounce address, an RFC 3464 report of a settled
//! permanent failure suppresses the bounced address in exactly the tenants
//! whose campaign mail went to it, a transient report is recorded and not
//! acted on, and a non-DSN message to the address is stored, never crashed
//! on. Runs against a throwaway Postgres (see `alo_test_db`); the hard-bounce
//! arc goes over a real SMTP connection.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use alo_smtp::bounce_intake::intake_campaign_bounce;
use alo_smtp::local_delivery::LocalDelivery;
use alo_smtp::server;
use alo_smtp::spool::Spool;
use alo_store::{
    AccountStore, AudiencePage, BlobStore, CAMPAIGN_BOUNCE_MESSAGE_MAX, CampaignContent,
    ConsentSource, NewCampaign, NewCampaignConsent, NewCustomer, Store, SuppressionReason,
    TenantStore,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

const HOSTNAME: &str = "mx.alo.test";
/// The configured return path — deployment config in production
/// (`ALO_SMTP_CAMPAIGN_RETURN_PATH`), wired directly here.
const RETURN_PATH: &str = "bounces@news.alo.test";
const IO_TIMEOUT: Duration = Duration::from_secs(10);

fn database_url() -> String {
    alo_test_db::url()
}

async fn test_store() -> Arc<Store> {
    let store = Store::connect(&database_url(), BlobStore::in_memory(50 * 1024 * 1024))
        .await
        .expect("connect store");
    store.migrate().await.expect("migrate");
    Arc::new(store)
}

/// A tenant whose campaign actually mailed `address` — or only enrolled it,
/// when `mailed` is false. The distinction is the mapping under test: only a
/// `sent` ledger row makes a tenant suppressible by a bounce of the address.
async fn tenant_with_campaign(
    store: &Store,
    tag: &str,
    address: &str,
    mailed: bool,
) -> (AccountStore, TenantStore) {
    let tenant = store.create_tenant(&format!("bounce-{tag}")).await.unwrap();
    let ts = store.for_tenant(tenant.clone());
    let user = ts.create_user(&format!("{tag}@bounce.test")).await.unwrap();
    let acc = store.for_account(tenant, user);
    acc.create_billing_customer(&NewCustomer {
        name: format!("Customer of {tag}"),
        country: "DE".to_owned(),
        email: Some(address.to_owned()),
        ..Default::default()
    })
    .await
    .unwrap();
    acc.record_campaign_consent(&NewCampaignConsent {
        address,
        source: ConsentSource::Manual,
        statement: "Asked for the newsletter at the counter",
        source_ref: None,
        occurred_at: None,
    })
    .await
    .unwrap();
    let campaign = acc
        .create_campaign(&NewCampaign {
            subject: "Spring letter",
            preheader: None,
            topic: "Monthly Newsletter",
            content: CampaignContent::empty(),
        })
        .await
        .unwrap();
    let send = acc.open_campaign_send(&campaign.id).await.unwrap();
    let mut after = None;
    loop {
        let page = acc
            .enrol_campaign_send_page(
                &send.id,
                &AudiencePage {
                    after: after.clone(),
                    limit: 50,
                },
            )
            .await
            .unwrap();
        match page.next_cursor {
            None => break,
            Some(cursor) => after = Some(cursor),
        }
    }
    if mailed {
        assert!(
            acc.mark_campaign_recipient_sent(&send.id, address)
                .await
                .unwrap(),
            "the fixture's mail must count as sent"
        );
    }
    (acc, ts)
}

/// A fabricated RFC 3464 delivery-status report — the shape any provider's
/// bounce arrives in, built by hand because the sender of a real bounce is
/// not us.
fn fabricated_dsn(address: &str, action: &str, status: &str) -> String {
    format!(
        "From: MAILER-DAEMON@their-mx.example\r\n\
         To: {RETURN_PATH}\r\n\
         Subject: Undelivered Mail Returned to Sender\r\n\
         Content-Type: multipart/report; report-type=delivery-status; boundary=\"=_b\"\r\n\
         \r\n\
         --=_b\r\n\
         Content-Type: text/plain\r\n\
         \r\n\
         The mail system could not deliver your message.\r\n\
         --=_b\r\n\
         Content-Type: message/delivery-status\r\n\
         \r\n\
         Reporting-MTA: dns; their-mx.example\r\n\
         \r\n\
         Final-Recipient: rfc822; {address}\r\n\
         Action: {action}\r\n\
         Status: {status}\r\n\
         Diagnostic-Code: smtp; 550 5.1.1 no such user\r\n\
         --=_b--\r\n"
    )
}

struct Harness {
    addr: SocketAddr,
}

async fn spawn(store: Arc<Store>) -> Harness {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let spool = Arc::new(Spool::new(dir.path()).unwrap());
    let acceptor =
        Arc::new(alo_smtp::tls::build_acceptor(None, None, HOSTNAME, true).expect("tls"));
    let local = Arc::new(
        LocalDelivery::from_store(store.clone(), spool.clone(), HOSTNAME.to_owned())
            .with_campaign_return_path(Some(RETURN_PATH.to_owned())),
    );
    let runtime = Arc::new(
        server::Runtime::mx(
            HOSTNAME,
            spool.clone(),
            acceptor,
            None,
            25 * 1024 * 1024,
            100,
            256,
        )
        .with_local_domains(vec!["alo.test".to_owned(), "news.alo.test".to_owned()])
        .with_local_delivery(Some(local)),
    );
    tokio::spawn(async move {
        let _ = server::serve(listener, runtime).await;
        drop(dir);
    });
    Harness { addr }
}

struct Client {
    reader: BufReader<tokio::net::tcp::OwnedReadHalf>,
    writer: tokio::net::tcp::OwnedWriteHalf,
}

impl Client {
    async fn connect(addr: SocketAddr) -> Self {
        let stream = TcpStream::connect(addr).await.unwrap();
        let (r, w) = stream.into_split();
        let mut c = Self {
            reader: BufReader::new(r),
            writer: w,
        };
        assert!(c.read_reply().await.starts_with("220 "));
        c
    }
    async fn read_reply(&mut self) -> String {
        loop {
            let mut line = Vec::new();
            loop {
                let mut b = [0u8; 1];
                let n = tokio::time::timeout(IO_TIMEOUT, self.reader.read(&mut b))
                    .await
                    .expect("read timeout")
                    .unwrap();
                assert!(n != 0, "closed mid-reply: {line:?}");
                line.push(b[0]);
                if line.ends_with(b"\r\n") {
                    break;
                }
            }
            if line.get(3) == Some(&b'-') {
                continue;
            }
            return String::from_utf8_lossy(&line).trim_end().to_owned();
        }
    }
    async fn cmd(&mut self, c: &str) -> String {
        self.writer
            .write_all(format!("{c}\r\n").as_bytes())
            .await
            .unwrap();
        self.writer.flush().await.unwrap();
        self.read_reply().await
    }
    async fn data(&mut self, body: &str) -> String {
        assert!(self.cmd("DATA").await.starts_with("354"));
        self.writer.write_all(body.as_bytes()).await.unwrap();
        self.writer.write_all(b"\r\n.\r\n").await.unwrap();
        self.writer.flush().await.unwrap();
        self.read_reply().await
    }
    async fn hello(&mut self) {
        assert!(
            self.cmd(&format!("EHLO {HOSTNAME}"))
                .await
                .starts_with("250")
        );
    }
}

/// The whole arc on the wire: the MX accepts the configured address (null
/// sender, the way a real bounce arrives), the report suppresses the bounced
/// address in the tenant whose campaign mailed it — and in nobody else. The
/// tenant that merely *enrolled* the address without mailing it is the
/// tenancy edge a fabricated report would otherwise exploit.
#[tokio::test]
async fn a_hard_dsn_over_smtp_suppresses_exactly_the_tenants_that_mailed_the_address() {
    let store = test_store().await;
    let bounced = "ann@lead.test";
    let (_a, tenant_a) = tenant_with_campaign(&store, "mailed", bounced, true).await;
    let (_b, tenant_b) = tenant_with_campaign(&store, "other", "ben@lead.test", true).await;
    let (_c, tenant_c) = tenant_with_campaign(&store, "enrolled", bounced, false).await;

    let h = spawn(store.clone()).await;
    let mut c = Client::connect(h.addr).await;
    c.hello().await;
    assert!(c.cmd("MAIL FROM:<>").await.starts_with("250"));
    // The bounce address is deliverable by configuration…
    assert!(
        c.cmd(&format!("RCPT TO:<{RETURN_PATH}>"))
            .await
            .starts_with("250")
    );
    // …and only that address: the domain gained no catch-all.
    let stranger = c.cmd("RCPT TO:<postmaster-not-us@news.alo.test>").await;
    assert!(stranger.starts_with("550"), "{stranger}");
    let accepted = c.data(&fabricated_dsn(bounced, "failed", "5.1.1")).await;
    assert!(accepted.starts_with("250"), "{accepted}");
    let _ = c.cmd("QUIT").await;

    // The tenant that mailed the address may never mail it again.
    let suppression = tenant_a
        .campaign_suppression_for(bounced)
        .await
        .unwrap()
        .expect("the mailing tenant is suppressed");
    assert_eq!(suppression.reason, SuppressionReason::HardBounce);
    assert_eq!(suppression.source_ref.as_deref(), Some("dsn 5.1.1"));

    // The tenant that mailed somebody else, and the tenant that only
    // enrolled the address, are untouched.
    assert!(
        tenant_b
            .campaign_suppression_for(bounced)
            .await
            .unwrap()
            .is_none(),
        "a bounce must not leak into a tenant that never mailed the address"
    );
    assert!(
        tenant_b
            .campaign_suppression_for("ben@lead.test")
            .await
            .unwrap()
            .is_none(),
        "the neighbour's own recipient is not the bounced one"
    );
    assert!(
        tenant_c
            .campaign_suppression_for(bounced)
            .await
            .unwrap()
            .is_none(),
        "an enrolment that never left cannot have bounced"
    );
}

/// A transient report is a receipt, not an act: soft failures retry, and only
/// a settled one suppresses (ADR 0044 §4).
#[tokio::test]
async fn a_soft_bounce_is_recorded_and_suppresses_nobody() {
    let store = test_store().await;
    let bounced = "ann-soft@lead.test";
    let (_a, tenant_a) = tenant_with_campaign(&store, "soft", bounced, true).await;

    let dsn = fabricated_dsn(bounced, "failed", "4.2.2");
    let id = intake_campaign_bounce(&store, dsn.as_bytes())
        .await
        .unwrap();

    let receipt = store.campaign_bounce(&id).await.unwrap().unwrap();
    assert_eq!(receipt.verdict, "soft");
    assert_eq!(receipt.recipient.as_deref(), Some(bounced));
    assert_eq!(receipt.status.as_deref(), Some("4.2.2"));
    assert_eq!(receipt.suppressed, 0);
    assert!(
        tenant_a
            .campaign_suppression_for(bounced)
            .await
            .unwrap()
            .is_none(),
        "a transient failure must not suppress"
    );
}

/// The return path of a public address receives whatever the internet sends
/// it. Anything that is not a delivery-status report is stored whole — the
/// operator's diagnosis starts from the bytes — and nothing crashes.
#[tokio::test]
async fn a_non_dsn_message_is_stored_not_crashed_on() {
    let store = test_store().await;
    let message = b"From: someone@ext.test\r\nSubject: hello?\r\n\r\nis this thing on\r\n";
    let id = intake_campaign_bounce(&store, message).await.unwrap();

    let receipt = store.campaign_bounce(&id).await.unwrap().unwrap();
    assert_eq!(receipt.verdict, "none");
    assert_eq!(receipt.recipient, None);
    assert_eq!(receipt.status, None);
    assert_eq!(receipt.suppressed, 0);
    assert_eq!(receipt.message, message.to_vec());
    assert_eq!(receipt.message_size, i64::try_from(message.len()).unwrap());

    // Bytes that are not even a message get the same shrug.
    let id = intake_campaign_bounce(&store, &[0xFF, 0xFE, 0x00])
        .await
        .unwrap();
    let receipt = store.campaign_bounce(&id).await.unwrap().unwrap();
    assert_eq!(receipt.verdict, "none");
}

/// RFC 3464 reports are unauthenticated by nature: a fabricated one naming an
/// address nobody's campaign mailed must silence nobody — the receipt still
/// records that it arrived and acted nowhere.
#[tokio::test]
async fn a_hard_bounce_for_an_address_nobody_mailed_suppresses_nowhere() {
    let store = test_store().await;
    let dsn = fabricated_dsn("stranger@lead.test", "failed", "5.1.1");
    let id = intake_campaign_bounce(&store, dsn.as_bytes())
        .await
        .unwrap();

    let receipt = store.campaign_bounce(&id).await.unwrap().unwrap();
    assert_eq!(receipt.verdict, "hard");
    assert_eq!(receipt.recipient.as_deref(), Some("stranger@lead.test"));
    assert_eq!(receipt.suppressed, 0);
}

/// A provider that returns the entire original message can exceed the store's
/// cap: the receipt keeps the head and records the true wire size beside it,
/// so truncation is visible rather than silent.
#[tokio::test]
async fn an_oversized_message_is_stored_truncated_with_its_true_size() {
    let store = test_store().await;
    let mut message = b"Subject: big\r\n\r\n".to_vec();
    message.resize(CAMPAIGN_BOUNCE_MESSAGE_MAX + 4096, b'x');
    let id = intake_campaign_bounce(&store, &message).await.unwrap();

    let receipt = store.campaign_bounce(&id).await.unwrap().unwrap();
    assert_eq!(receipt.message.len(), CAMPAIGN_BOUNCE_MESSAGE_MAX);
    assert_eq!(
        receipt.message_size,
        i64::try_from(message.len()).unwrap(),
        "the true size on the wire stays readable beside the truncated bytes"
    );
    assert_eq!(&receipt.message, &message[..CAMPAIGN_BOUNCE_MESSAGE_MAX]);
}
