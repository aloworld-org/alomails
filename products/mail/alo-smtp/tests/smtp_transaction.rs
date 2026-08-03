//! Integration tests for the full mail transaction (Phase 1 M1):
//! MAIL FROM → RCPT TO → DATA → spooled message, plus every error
//! path, speaking real bytes over a real socket.
//!
//! Like `smtp_wire.rs`, tests run against `ALO_SMTP_TEST_ADDR`
//! when set (CI's composed stack). Assertions that must inspect the
//! spool directory or need a custom config (tiny size limits) only
//! run in local mode — the wire-level assertions run in both.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use alo_smtp::server;
use alo_smtp::spool::Spool;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

const TEST_HOSTNAME: &str = "mx.alo.test";
const IO_TIMEOUT: Duration = Duration::from_secs(10);

fn external_addr() -> Option<SocketAddr> {
    std::env::var("ALO_SMTP_TEST_ADDR")
        .ok()
        .map(|s| s.parse().expect("ALO_SMTP_TEST_ADDR must be host:port"))
}

/// A locally spawned server whose spool we can inspect.
struct LocalServer {
    addr: SocketAddr,
    spool_dir: PathBuf,
    // Held so the tempdir outlives the server.
    _dir_guard: tempfile::TempDir,
}

async fn spawn_local(max_message_size: usize) -> LocalServer {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let addr = listener.local_addr().expect("local addr");
    let dir = tempfile::tempdir().expect("temp spool dir");
    let spool_dir = dir.path().to_path_buf();
    let spool = Arc::new(Spool::new(&spool_dir).expect("spool init"));
    let acceptor = Arc::new(
        alo_smtp::tls::build_acceptor(None, None, TEST_HOSTNAME, true).expect("tls acceptor"),
    );
    let runtime = Arc::new(server::Runtime::mx(
        TEST_HOSTNAME,
        spool,
        acceptor,
        None,
        max_message_size,
        100,
        256,
    ));
    tokio::spawn(async move {
        let _ = server::serve(listener, runtime).await;
    });
    LocalServer {
        addr,
        spool_dir,
        _dir_guard: dir,
    }
}

struct Client {
    reader: BufReader<tokio::net::tcp::OwnedReadHalf>,
    writer: tokio::net::tcp::OwnedWriteHalf,
}

impl Client {
    async fn connect_to(addr: SocketAddr) -> Self {
        let stream = tokio::time::timeout(IO_TIMEOUT, TcpStream::connect(addr))
            .await
            .expect("connect timed out")
            .expect("connect failed");
        let (read_half, writer) = stream.into_split();
        let mut client = Self {
            reader: BufReader::new(read_half),
            writer,
        };
        let greeting = client.read_reply().await;
        assert!(greeting.starts_with("220 "), "greeting: {greeting}");
        client
    }

    async fn send(&mut self, bytes: &[u8]) {
        tokio::time::timeout(IO_TIMEOUT, self.writer.write_all(bytes))
            .await
            .expect("write timed out")
            .expect("write failed");
        self.writer.flush().await.expect("flush failed");
    }

    /// Reads a complete reply, consuming continuation lines of a
    /// multiline reply (RFC 5321 §4.2.1) and returning the final line.
    async fn read_reply(&mut self) -> String {
        loop {
            let line = self.read_line().await;
            // A 4th char of '-' marks a continuation; anything else
            // (space, or end of a short line) is the final line.
            if line.as_bytes().get(3) == Some(&b'-') {
                continue;
            }
            return line;
        }
    }

    async fn read_line(&mut self) -> String {
        let mut line = Vec::new();
        loop {
            let mut byte = [0_u8; 1];
            let n = tokio::time::timeout(IO_TIMEOUT, self.reader.read(&mut byte))
                .await
                .expect("read timed out")
                .expect("read failed");
            assert!(n != 0, "connection closed mid-reply; got {line:?}");
            line.push(byte[0]);
            if line.ends_with(b"\r\n") {
                return String::from_utf8(line)
                    .expect("reply must be UTF-8")
                    .trim_end()
                    .to_owned();
            }
        }
    }

    /// Sends a command line and returns the reply.
    async fn cmd(&mut self, command: &str) -> String {
        self.send(format!("{command}\r\n").as_bytes()).await;
        self.read_reply().await
    }

    async fn assert_closed(&mut self) {
        let mut byte = [0_u8; 1];
        let n = tokio::time::timeout(IO_TIMEOUT, self.reader.read(&mut byte))
            .await
            .expect("read timed out")
            .expect("read after close failed");
        assert_eq!(n, 0, "expected server to close the connection");
    }
}

/// Full happy path; in local mode, also verifies the spooled entry:
/// envelope contents, Received: stamp, dot-unstuffing, CRLF endings.
#[tokio::test]
async fn full_transaction_spools_message_with_received_header() {
    let (addr, local) = match external_addr() {
        Some(addr) => (addr, None),
        None => {
            let server = spawn_local(25 * 1024 * 1024).await;
            (server.addr, Some(server))
        }
    };
    let mut c = Client::connect_to(addr).await;

    assert!(c.cmd("EHLO wire-test.example").await.starts_with("250"));
    assert!(
        c.cmd("MAIL FROM:<bob@example.org>")
            .await
            .starts_with("250 ")
    );
    assert!(
        c.cmd("RCPT TO:<alice@example.com>")
            .await
            .starts_with("250 ")
    );
    assert!(
        c.cmd("RCPT TO:<carol@example.com>")
            .await
            .starts_with("250 ")
    );
    assert!(c.cmd("DATA").await.starts_with("354 "));
    c.send(b"Subject: test\r\n\r\nplain line\r\n..stuffed line\r\n.\r\n")
        .await;
    let accepted = c.read_reply().await;
    assert!(
        accepted.starts_with("250 OK: queued as "),
        "expected queued reply, got: {accepted}"
    );
    assert!(c.cmd("QUIT").await.starts_with("221 "));
    c.assert_closed().await;

    // Spool inspection is only possible against a local server.
    if let Some(server) = local {
        let id = accepted.rsplit(' ').next().unwrap().to_owned();
        let spool = Spool::new(&server.spool_dir).unwrap();
        let (envelope, message) = spool.read(&id).unwrap();

        assert_eq!(envelope.helo, "wire-test.example");
        assert_eq!(envelope.mail_from.as_deref(), Some("bob@example.org"));
        assert_eq!(
            envelope.rcpt_to,
            vec!["alice@example.com", "carol@example.com"]
        );

        let text = String::from_utf8(message).unwrap();
        // RFC 5321 §4.4: our Received: stamp leads the message.
        assert!(text.starts_with("Received: from wire-test.example"));
        assert!(text.contains(&format!("with ESMTP id {id};")));
        // RFC 5321 §4.5.2: "..stuffed" arrived, ".stuffed" is stored.
        assert!(text.contains("\r\n.stuffed line\r\n"));
        assert!(!text.contains(".." /* no stuffing may survive */));
    }
}

#[tokio::test]
async fn out_of_order_commands_get_503() {
    let addr = match external_addr() {
        Some(a) => a,
        None => spawn_local(25 * 1024 * 1024).await.addr,
    };
    let mut c = Client::connect_to(addr).await;

    // RFC 5321 §4.1.4 ordering, each without its prerequisite.
    assert!(
        c.cmd("MAIL FROM:<bob@example.org>")
            .await
            .starts_with("503 ")
    );
    assert!(c.cmd("EHLO wire-test.example").await.starts_with("250"));
    assert!(
        c.cmd("RCPT TO:<alice@example.com>")
            .await
            .starts_with("503 ")
    );
    assert!(c.cmd("DATA").await.starts_with("503 "));
    // RSET aborts a real transaction.
    assert!(
        c.cmd("MAIL FROM:<bob@example.org>")
            .await
            .starts_with("250 ")
    );
    assert!(
        c.cmd("RCPT TO:<alice@example.com>")
            .await
            .starts_with("250 ")
    );
    assert!(c.cmd("RSET").await.starts_with("250 "));
    assert!(c.cmd("DATA").await.starts_with("503 "));
    assert!(c.cmd("QUIT").await.starts_with("221 "));
}

#[tokio::test]
async fn addresses_null_quoted_postmaster_and_bad() {
    let addr = match external_addr() {
        Some(a) => a,
        None => spawn_local(25 * 1024 * 1024).await.addr,
    };
    let mut c = Client::connect_to(addr).await;
    c.cmd("EHLO wire-test.example").await;

    // Null reverse-path (bounces, §4.5.5).
    assert!(c.cmd("MAIL FROM:<>").await.starts_with("250 "));
    // Quoted local part (§4.1.2).
    assert!(
        c.cmd("RCPT TO:<\"john smith\"@example.com>")
            .await
            .starts_with("250 ")
    );
    // Postmaster without domain (§4.1.1.3).
    assert!(c.cmd("RCPT TO:<postmaster>").await.starts_with("250 "));
    c.cmd("RSET").await;
    // Bad syntax → 501, session intact.
    assert!(c.cmd("MAIL FROM:<no-at-sign>").await.starts_with("501 "));
    assert!(
        c.cmd("MAIL FROM:<bob@example.org>")
            .await
            .starts_with("250 ")
    );
    // Unadvertised ESMTP parameter → 555 (§4.1.1.11).
    assert!(
        c.cmd("RCPT TO:<alice@example.com> NOTIFY=NEVER")
            .await
            .starts_with("555 ")
    );
    assert!(c.cmd("QUIT").await.starts_with("221 "));
}

/// Local-only: needs a server configured with a tiny size limit.
#[tokio::test]
async fn oversize_message_gets_552_and_session_survives() {
    if external_addr().is_some() {
        eprintln!("skipped against external server: needs custom size limit");
        return;
    }
    let server = spawn_local(2048).await;
    let mut c = Client::connect_to(server.addr).await;

    c.cmd("EHLO wire-test.example").await;
    c.cmd("MAIL FROM:<bob@example.org>").await;
    c.cmd("RCPT TO:<alice@example.com>").await;
    assert!(c.cmd("DATA").await.starts_with("354 "));
    let mut body = Vec::new();
    for _ in 0..200 {
        body.extend_from_slice(b"0123456789012345678901234567890123456789\r\n");
    }
    body.extend_from_slice(b".\r\n");
    c.send(&body).await;
    assert!(c.read_reply().await.starts_with("552 "));

    // The limit is enforced during read and the stream was drained:
    // the session must survive into a fresh, working transaction.
    assert!(
        c.cmd("MAIL FROM:<bob@example.org>")
            .await
            .starts_with("250 ")
    );
    assert!(
        c.cmd("RCPT TO:<alice@example.com>")
            .await
            .starts_with("250 ")
    );
    assert!(c.cmd("DATA").await.starts_with("354 "));
    c.send(b"tiny\r\n.\r\n").await;
    assert!(c.read_reply().await.starts_with("250 OK: queued"));
    assert!(c.cmd("QUIT").await.starts_with("221 "));

    // Exactly one message may exist in the spool.
    let spool = Spool::new(&server.spool_dir).unwrap();
    assert_eq!(spool.list().unwrap().len(), 1);
}

#[tokio::test]
async fn bare_lf_inside_data_is_rejected_and_connection_closed() {
    let addr = match external_addr() {
        Some(a) => a,
        None => spawn_local(25 * 1024 * 1024).await.addr,
    };
    let mut c = Client::connect_to(addr).await;

    c.cmd("EHLO wire-test.example").await;
    c.cmd("MAIL FROM:<bob@example.org>").await;
    c.cmd("RCPT TO:<alice@example.com>").await;
    assert!(c.cmd("DATA").await.starts_with("354 "));
    // SMTP-smuggling probe: bare LF inside content.
    c.send(b"good\r\nsmuggled\n.\r\n").await;
    assert!(c.read_reply().await.starts_with("500 "));
    c.assert_closed().await;
}

#[tokio::test]
async fn service_commands_on_the_wire() {
    let addr = match external_addr() {
        Some(a) => a,
        None => spawn_local(25 * 1024 * 1024).await.addr,
    };
    let mut c = Client::connect_to(addr).await;
    c.cmd("HELO old-client.example").await;

    assert!(c.cmd("NOOP").await.starts_with("250 "));
    assert!(c.cmd("VRFY alice").await.starts_with("252 "));
    assert!(c.cmd("HELP").await.starts_with("502 "));
    assert!(c.cmd("QUIT").await.starts_with("221 "));
    c.assert_closed().await;
}
