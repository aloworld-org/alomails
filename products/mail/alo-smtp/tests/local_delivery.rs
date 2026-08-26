//! End-to-end: a message received over real SMTP is delivered into the
//! account store with Sieve at the boundary, and shows up in the recipient's
//! mailbox. The evidence the Sieve and IMAP milestones owed. Runs against
//! the live Postgres (DATABASE_URL / compose 5432).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use alo_smtp::local_delivery::{DeliveryOutcome, LocalDelivery};
use alo_smtp::server;
use alo_smtp::spool::Spool;
use alo_store::{BlobStore, MailboxId, Page, Store};
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

const HOSTNAME: &str = "mx.alo.test";
const IO_TIMEOUT: Duration = Duration::from_secs(10);

/// The database this suite runs against.
///
/// Delegates to `alo_test_db`, which refuses the database the product
/// runs on: suites create and drop their own, they never write into `alo`.
fn database_url() -> String {
    alo_test_db::url()
}

/// A store shared between the SMTP server (delivery) and the test (asserts),
/// so the DB and blob backend are the same instance.
async fn test_store() -> Arc<Store> {
    let store = Store::connect(&database_url(), BlobStore::in_memory(50 * 1024 * 1024))
        .await
        .expect("connect store");
    store.migrate().await.expect("migrate");
    Arc::new(store)
}

/// The server + the store it delivers into, plus a spool for outbound.
struct Harness {
    addr: SocketAddr,
    spool: Arc<Spool>,
    _dir: tempfile::TempDir,
}

async fn spawn(store: Arc<Store>) -> Harness {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let spool = Arc::new(Spool::new(dir.path()).unwrap());
    let acceptor =
        Arc::new(alo_smtp::tls::build_acceptor(None, None, HOSTNAME, true).expect("tls"));
    let local = Arc::new(LocalDelivery::from_store(
        store.clone(),
        spool.clone(),
        HOSTNAME.to_owned(),
    ));
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
        .with_local_domains(vec!["alo.test".to_owned()])
        .with_local_delivery(Some(local)),
    );
    tokio::spawn(async move {
        let _ = server::serve(listener, runtime).await;
    });
    Harness {
        addr,
        spool,
        _dir: dir,
    }
}

/// Provisions a user with a globally-unique local address; returns
/// `(AccountStore, email, inbox_id)`.
async fn make_user(store: &Store, tag: &str) -> (alo_store::AccountStore, String, MailboxId) {
    let tenant = store.create_tenant(&format!("ld-{tag}")).await.unwrap();
    let ts = store.for_tenant(tenant.clone());
    let email = format!("{tag}-{tenant}@alo.test");
    let user = ts.create_user(&email).await.unwrap();
    let acc = store.for_account(tenant, user);
    let inbox = acc.inbox().await.unwrap();
    (acc, email, inbox)
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
    /// Sends a full message body ending with the lone-dot terminator.
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

async fn count(acc: &alo_store::AccountStore, mb: &MailboxId) -> usize {
    acc.list_mailbox(mb, Page::default()).await.unwrap().len()
}

#[tokio::test]
async fn message_over_smtp_lands_in_the_mailbox_and_unknown_rcpt_is_550() {
    let store = test_store().await;
    let (alice, email, inbox) = make_user(&store, "alice").await;
    let h = spawn(store.clone()).await;
    let mut c = Client::connect(h.addr).await;
    c.hello().await;
    assert!(
        c.cmd("MAIL FROM:<sender@ext.test>")
            .await
            .starts_with("250")
    );

    // Unknown local user → 550 5.1.1 at RCPT (not after DATA).
    let unknown = c.cmd("RCPT TO:<nobody@alo.test>").await;
    assert!(
        unknown.starts_with("550") && unknown.contains("5.1.1"),
        "{unknown}"
    );

    // Known local user → 250.
    assert!(
        c.cmd(&format!("RCPT TO:<{email}>"))
            .await
            .starts_with("250")
    );
    let data = c
        .data("From: sender@ext.test\r\nSubject: hello over smtp\r\n\r\nbody\r\n")
        .await;
    assert!(data.starts_with("250"), "{data}");
    let _ = c.cmd("QUIT").await;

    // The message is in Alice's Inbox.
    let msgs = alice.list_mailbox(&inbox, Page::default()).await.unwrap();
    assert_eq!(msgs.len(), 1, "message delivered to store");
    assert_eq!(msgs[0].subject, "hello over smtp");
    // isolation: never seen by anyone else.
    let (_bob, _be, bob_inbox) = make_user(&store, "bob").await;
    assert_eq!(count(&_bob, &bob_inbox).await, 0);
}

#[tokio::test]
async fn sieve_fileinto_and_subaddress_route_at_the_boundary() {
    let store = test_store().await;
    let (alice, email, inbox) = make_user(&store, "afil").await;
    let work = alice.create_mailbox(None, "Work", None).await.unwrap();
    alice
        .put_sieve_script(
            "main",
            "require [\"fileinto\",\"subaddress\"]; \
             if header :contains \"subject\" \"work\" { fileinto \"Work\"; } \
             elsif address :detail :is \"to\" \"urgent\" { fileinto \"Work\"; }",
        )
        .await
        .unwrap();
    alice.activate_sieve_script(Some("main")).await.unwrap();
    let h = spawn(store.clone()).await;

    // A subject-matching message → Work.
    let mut c = Client::connect(h.addr).await;
    c.hello().await;
    c.cmd("MAIL FROM:<s@ext.test>").await;
    assert!(
        c.cmd(&format!("RCPT TO:<{email}>"))
            .await
            .starts_with("250")
    );
    assert!(
        c.data("From: s@ext.test\r\nSubject: work item\r\n\r\nx\r\n")
            .await
            .starts_with("250")
    );

    // A +urgent subaddress message → Work (RCPT is user+urgent@).
    let plus = email.replace('@', "+urgent@");
    let mut c2 = Client::connect(h.addr).await;
    c2.hello().await;
    c2.cmd("MAIL FROM:<s@ext.test>").await;
    assert!(
        c2.cmd(&format!("RCPT TO:<{plus}>"))
            .await
            .starts_with("250")
    );
    assert!(
        c2.data(&format!(
            "From: s@ext.test\r\nSubject: plain\r\nTo: {plus}\r\n\r\nx\r\n"
        ))
        .await
        .starts_with("250")
    );

    assert_eq!(count(&alice, &work).await, 2, "both routed to Work");
    assert_eq!(count(&alice, &inbox).await, 0, "nothing left in Inbox");
}

#[tokio::test]
async fn vacation_queues_an_auto_reply_through_the_spool() {
    let store = test_store().await;
    let (alice, email, _inbox) = make_user(&store, "avac").await;
    alice
        .put_sieve_script(
            "v",
            "require [\"vacation\"]; vacation :days 7 :subject \"Away\" \"On holiday\";",
        )
        .await
        .unwrap();
    alice.activate_sieve_script(Some("v")).await.unwrap();
    let h = spawn(store.clone()).await;

    let mut c = Client::connect(h.addr).await;
    c.hello().await;
    c.cmd("MAIL FROM:<friend@ext.test>").await;
    assert!(
        c.cmd(&format!("RCPT TO:<{email}>"))
            .await
            .starts_with("250")
    );
    assert!(
        c.data(&format!(
            "From: friend@ext.test\r\nTo: {email}\r\nSubject: hi\r\n\r\nx\r\n"
        ))
        .await
        .starts_with("250")
    );

    // The auto-reply was enqueued for the outbound queue (to the friend).
    let ids = h.spool.list().unwrap();
    let mut found = false;
    for id in ids {
        let (env, body) = h.spool.read(&id).unwrap();
        if env.rcpt_to.iter().any(|r| r == "friend@ext.test") {
            let text = String::from_utf8_lossy(&body);
            assert!(text.contains("Auto-Submitted: auto-replied"), "{text}");
            assert!(env.mail_from.is_none(), "vacation uses a null return-path");
            found = true;
        }
    }
    assert!(found, "vacation auto-reply not enqueued");
}

#[tokio::test]
async fn delivered_body_survives_a_store_restart() {
    // The durable filesystem blob backend: a delivered body must be readable
    // after a fresh store instance reopens the same DB + blob dir (a restart).
    let blobdir = tempfile::tempdir().unwrap();
    let store_of = |dir: std::path::PathBuf| async move {
        let s = Store::connect(
            &database_url(),
            BlobStore::local(&dir, 50 * 1024 * 1024).unwrap(),
        )
        .await
        .unwrap();
        s.migrate().await.unwrap();
        Arc::new(s)
    };
    let store1 = store_of(blobdir.path().to_path_buf()).await;
    let (alice, email, inbox) = make_user(&store1, "durable").await;
    let spooldir = tempfile::tempdir().unwrap();
    let spool = Arc::new(Spool::new(spooldir.path()).unwrap());
    let ld = LocalDelivery::from_store(store1.clone(), spool, HOSTNAME.to_owned());
    assert_eq!(
        ld.deliver(
            b"From: s@ext.test\r\nSubject: durable\r\n\r\nkeep me across restart\r\n",
            Some("s@ext.test"),
            std::slice::from_ref(&email),
        )
        .await,
        DeliveryOutcome::Delivered
    );
    let mid = alice.list_mailbox(&inbox, Page::default()).await.unwrap()[0]
        .id
        .clone();
    drop(alice);
    drop(store1);

    // "Restart": a fresh store over the same DB + same blob directory.
    let store2 = store_of(blobdir.path().to_path_buf()).await;
    let (t, u) = store2.account_by_email(&email).await.unwrap().unwrap();
    let bytes = store2.for_account(t, u).message_bytes(&mid).await.unwrap();
    assert!(
        String::from_utf8_lossy(&bytes).contains("keep me across restart"),
        "delivered body must survive a restart (durable blob store)"
    );
}

#[tokio::test]
async fn list_address_fans_out_per_member_and_a_leaver_stops_getting_mail() {
    // One tenant, two members, one list address. A single inbound message
    // fans out to one copy per member THROUGH each member's own Sieve
    // script — and the envelope recipient a member's script sees is the
    // LIST address, not their personal one. After a member leaves, the next
    // message no longer reaches them.
    let store = test_store().await;
    let tenant = store.create_tenant("ld-list").await.unwrap();
    let ts = store.for_tenant(tenant.clone());
    let alice_email = format!("lista-{tenant}@alo.test");
    let bob_email = format!("listb-{tenant}@alo.test");
    let alice_id = ts.create_user(&alice_email).await.unwrap();
    let bob_id = ts.create_user(&bob_email).await.unwrap();
    let alice = store.for_account(tenant.clone(), alice_id.clone());
    let bob = store.for_account(tenant.clone(), bob_id.clone());
    let alice_inbox = alice.inbox().await.unwrap();
    let bob_inbox = bob.inbox().await.unwrap();

    let group = ts.create_group("Everyone").await.unwrap();
    ts.add_group_member(&group, &alice_id).await.unwrap();
    ts.add_group_member(&group, &bob_id).await.unwrap();
    let list = format!("all-{tenant}@alo.test");
    ts.set_group_address(&group, Some(&list)).await.unwrap();

    // Bob's Sieve files on the ENVELOPE recipient being the list address —
    // the assertion that members can filter list mail as list mail.
    let bob_list_mb = bob.create_mailbox(None, "List", None).await.unwrap();
    bob.put_sieve_script(
        "list",
        &format!(
            "require [\"envelope\",\"fileinto\"]; \
             if envelope :is \"to\" \"{list}\" {{ fileinto \"List\"; }}"
        ),
    )
    .await
    .unwrap();
    bob.activate_sieve_script(Some("list")).await.unwrap();

    let h = spawn(store.clone()).await;
    let mut c = Client::connect(h.addr).await;
    c.hello().await;
    assert!(c.cmd("MAIL FROM:<s@ext.test>").await.starts_with("250"));
    assert!(
        c.cmd(&format!("RCPT TO:<{list}>")).await.starts_with("250"),
        "a list address with members is a valid recipient"
    );
    assert!(
        c.data("From: s@ext.test\r\nSubject: to the list\r\n\r\nx\r\n")
            .await
            .starts_with("250")
    );

    assert_eq!(count(&alice, &alice_inbox).await, 1, "one copy for Alice");
    assert_eq!(
        count(&bob, &bob_list_mb).await,
        1,
        "Bob's copy was filed by his Sieve on the list address as envelope recipient"
    );
    assert_eq!(count(&bob, &bob_inbox).await, 0, "not duplicated to Inbox");

    // Bob leaves; the next message stops reaching him immediately.
    ts.remove_group_member(&group, &bob_id).await.unwrap();
    let mut c2 = Client::connect(h.addr).await;
    c2.hello().await;
    c2.cmd("MAIL FROM:<s@ext.test>").await;
    assert!(
        c2.cmd(&format!("RCPT TO:<{list}>"))
            .await
            .starts_with("250")
    );
    assert!(
        c2.data("From: s@ext.test\r\nSubject: after leaving\r\n\r\nx\r\n")
            .await
            .starts_with("250")
    );
    assert_eq!(
        count(&alice, &alice_inbox).await,
        2,
        "Alice keeps receiving"
    );
    assert_eq!(
        count(&bob, &bob_list_mb).await + count(&bob, &bob_inbox).await,
        1,
        "no new copy for Bob after he left"
    );
}

#[tokio::test]
async fn a_memberless_list_is_refused_at_rcpt() {
    // A list with nobody behind it is not a deliverable destination: the MX
    // says so at RCPT time (550), rather than accepting and black-holing.
    let store = test_store().await;
    let tenant = store.create_tenant("ld-empty-list").await.unwrap();
    let ts = store.for_tenant(tenant.clone());
    let group = ts.create_group("Ghosts").await.unwrap();
    let list = format!("ghosts-{tenant}@alo.test");
    ts.set_group_address(&group, Some(&list)).await.unwrap();

    let h = spawn(store.clone()).await;
    let mut c = Client::connect(h.addr).await;
    c.hello().await;
    assert!(c.cmd("MAIL FROM:<s@ext.test>").await.starts_with("250"));
    let reply = c.cmd(&format!("RCPT TO:<{list}>")).await;
    assert!(
        reply.starts_with("550"),
        "a memberless list must be refused, got: {reply}"
    );
}

#[tokio::test]
async fn transient_store_failure_defers_rather_than_loses() {
    // deliver() to a recipient that cannot be resolved returns Transient,
    // which the DATA handler maps to 451 — the sender retries, mail is not
    // lost. (The live RCPT path rejects unknowns at 550; this exercises the
    // DATA-time defer for a resolution failure.)
    let store = test_store().await;
    let dir = tempfile::tempdir().unwrap();
    let spool = Arc::new(Spool::new(dir.path()).unwrap());
    let ld = LocalDelivery::from_store(store.clone(), spool, HOSTNAME.to_owned());
    let outcome = ld
        .deliver(
            b"From: x@y\r\n\r\nx\r\n",
            Some("x@y"),
            &["ghost@alo.test".to_owned()],
        )
        .await;
    assert_eq!(outcome, DeliveryOutcome::Transient);
}
