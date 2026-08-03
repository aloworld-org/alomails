//! Integration tests that speak real bytes over a real socket
//! (protocol skill: tests at the wire level, not into the parser).
//!
//! Two modes, one test suite:
//! - default: each test spawns the server in-process on an ephemeral
//!   loopback port;
//! - `ALO_SMTP_TEST_ADDR=host:port`: tests run against an external
//!   instance instead — this is how CI exercises the composed Docker
//!   stack with the same assertions.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use alo_smtp::server;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

const TEST_HOSTNAME: &str = "mx.alo.test";
const IO_TIMEOUT: Duration = Duration::from_secs(10);

/// Address to test against: external if `ALO_SMTP_TEST_ADDR` is
/// set, otherwise a freshly spawned in-process server with a
/// throwaway spool.
async fn target_addr() -> SocketAddr {
    if let Ok(external) = std::env::var("ALO_SMTP_TEST_ADDR") {
        return external
            .parse()
            .expect("ALO_SMTP_TEST_ADDR must be host:port");
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let addr = listener.local_addr().expect("local addr");
    let spool_dir = tempfile::tempdir().expect("temp spool dir");
    let spool = Arc::new(alo_smtp::spool::Spool::new(spool_dir.path()).expect("spool init"));
    let acceptor = Arc::new(
        alo_smtp::tls::build_acceptor(None, None, TEST_HOSTNAME, true).expect("tls acceptor"),
    );
    let runtime = Arc::new(server::Runtime::mx(
        TEST_HOSTNAME,
        spool,
        acceptor,
        None,
        25 * 1024 * 1024,
        100,
        256,
    ));
    tokio::spawn(async move {
        // Keep the tempdir alive for the server's lifetime.
        let _spool_dir = spool_dir;
        let _ = server::serve(listener, runtime).await;
    });
    addr
}

struct Client {
    reader: BufReader<tokio::net::tcp::OwnedReadHalf>,
    writer: tokio::net::tcp::OwnedWriteHalf,
}

impl Client {
    async fn connect() -> Self {
        let addr = target_addr().await;
        let stream = tokio::time::timeout(IO_TIMEOUT, TcpStream::connect(addr))
            .await
            .expect("connect timed out")
            .expect("connect failed");
        let (read_half, writer) = stream.into_split();
        Self {
            reader: BufReader::new(read_half),
            writer,
        }
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
                let text = String::from_utf8(line).expect("reply must be UTF-8");
                return text.trim_end().to_owned();
            }
        }
    }

    /// Asserts the connection is closed (read returns 0 bytes).
    async fn assert_closed(&mut self) {
        let mut byte = [0_u8; 1];
        let n = tokio::time::timeout(IO_TIMEOUT, self.reader.read(&mut byte))
            .await
            .expect("read timed out")
            .expect("read after close failed");
        assert_eq!(n, 0, "expected server to close the connection");
    }
}

#[tokio::test]
async fn greeting_ehlo_quit_full_path() {
    let mut client = Client::connect().await;

    let greeting = client.read_reply().await;
    assert!(
        greeting.starts_with("220 "),
        "expected 220 greeting, got: {greeting}"
    );

    client.send(b"EHLO wire-test.example\r\n").await;
    let ehlo = client.read_reply().await;
    assert!(ehlo.starts_with("250"), "expected 250, got: {ehlo}");

    client.send(b"QUIT\r\n").await;
    let quit = client.read_reply().await;
    assert!(quit.starts_with("221 "), "expected 221, got: {quit}");

    client.assert_closed().await;
}

#[tokio::test]
async fn unknown_command_gets_500_and_session_survives() {
    let mut client = Client::connect().await;
    client.read_reply().await; // greeting

    client.send(b"BOGUS\r\n").await;
    let reply = client.read_reply().await;
    assert!(reply.starts_with("500 "), "expected 500, got: {reply}");

    // Error paths must not corrupt the session (RFC 5321 §4.2.4).
    client.send(b"QUIT\r\n").await;
    assert!(client.read_reply().await.starts_with("221 "));
}

#[tokio::test]
async fn ehlo_without_domain_gets_501() {
    let mut client = Client::connect().await;
    client.read_reply().await;

    client.send(b"EHLO\r\n").await;
    let reply = client.read_reply().await;
    assert!(reply.starts_with("501 "), "expected 501, got: {reply}");

    client.send(b"QUIT\r\n").await;
    assert!(client.read_reply().await.starts_with("221 "));
}

#[tokio::test]
async fn bare_lf_is_rejected_with_500() {
    let mut client = Client::connect().await;
    client.read_reply().await;

    // Bare LF: SMTP-smuggling probe (RFC 5321 §2.3.8).
    client.send(b"EHLO smuggle.example\n").await;
    let reply = client.read_reply().await;
    assert!(reply.starts_with("500 "), "expected 500, got: {reply}");

    client.send(b"QUIT\r\n").await;
    assert!(client.read_reply().await.starts_with("221 "));
}

#[tokio::test]
async fn over_512_octet_line_gets_500_and_session_survives() {
    let mut client = Client::connect().await;
    client.read_reply().await;

    // RFC 5321 §4.5.3.1.4: command line limit is 512 octets w/ CRLF.
    let mut long = vec![b'A'; 600];
    long.extend_from_slice(b"\r\n");
    client.send(&long).await;
    let reply = client.read_reply().await;
    assert!(reply.starts_with("500 "), "expected 500, got: {reply}");

    client.send(b"QUIT\r\n").await;
    assert!(client.read_reply().await.starts_with("221 "));
}
