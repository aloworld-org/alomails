//! M6.1 scripted wire transcripts: the canonical IMAP, IMAP-XOAUTH2 and
//! POP3 exchanges, driven over real TLS sockets and recorded line by line.
//! Each test asserts the exchange it captures, so a transcript can never go
//! green while drifting from the behaviour it documents. When
//! `ALO_WIRE_TRANSCRIPTS` names a directory, the trimmed transcript is
//! written there — `scripts/wire-transcripts.sh` runs these tests and
//! splices the output into `docs/interop.md`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use common::{
    danger, deliver, make_user, message, spawn_imap, spawn_pop3, test_identity, test_store,
};
use rustls::pki_types::ServerName;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;

/// A recording TLS line client: every line sent is logged `C:`, every line
/// read is logged `S:`. `send_shown` logs a redacted form instead of the
/// secret-bearing bytes actually written.
struct Rec {
    stream: TlsStream<TcpStream>,
    buf: Vec<u8>,
    log: Vec<String>,
}

impl Rec {
    async fn connect(addr: SocketAddr) -> Self {
        let tcp = TcpStream::connect(addr).await.unwrap();
        let connector = TlsConnector::from(Arc::new(danger::client_config()));
        let name = ServerName::try_from("localhost").unwrap();
        let stream = connector.connect(name, tcp).await.unwrap();
        Self {
            stream,
            buf: Vec::new(),
            log: Vec::new(),
        }
    }

    /// An annotation line in the transcript (not wire bytes).
    fn note(&mut self, s: &str) {
        self.log.push(format!("  ({s})"));
    }

    async fn send(&mut self, line: &str) {
        let shown = line.to_owned();
        self.send_shown(line, &shown).await;
    }

    async fn send_shown(&mut self, line: &str, shown: &str) {
        self.log.push(format!("C: {shown}"));
        self.stream
            .write_all(format!("{line}\r\n").as_bytes())
            .await
            .unwrap();
        self.stream.flush().await.unwrap();
    }

    async fn read_line(&mut self) -> String {
        loop {
            if let Some(pos) = self.buf.iter().position(|&b| b == b'\n') {
                let raw: Vec<u8> = self.buf.drain(..=pos).collect();
                let mut s = String::from_utf8_lossy(&raw).into_owned();
                while s.ends_with('\n') || s.ends_with('\r') {
                    s.pop();
                }
                self.log.push(format!("S: {s}"));
                return s;
            }
            let mut tmp = [0u8; 4096];
            let n = self.stream.read(&mut tmp).await.unwrap();
            assert!(n != 0, "connection closed mid-read; log: {:?}", self.log);
            self.buf.extend_from_slice(&tmp[..n]);
        }
    }

    /// Reads lines until the tagged completion; returns all of them.
    async fn until_tag(&mut self, tag: &str) -> Vec<String> {
        let mut lines = Vec::new();
        loop {
            let line = self.read_line().await;
            let done = line.starts_with(&format!("{tag} "));
            lines.push(line);
            if done {
                return lines;
            }
        }
    }

    async fn command(&mut self, tag: &str, cmd: &str) -> Vec<String> {
        self.send(&format!("{tag} {cmd}")).await;
        self.until_tag(tag).await
    }

    /// Reads a POP3 multiline body (lines until the lone `.`).
    async fn read_multiline(&mut self) -> Vec<String> {
        let mut lines = Vec::new();
        loop {
            let line = self.read_line().await;
            if line == "." {
                return lines;
            }
            lines.push(line);
        }
    }
}

fn assert_completes_ok(lines: &[String]) {
    let last = lines.last().map(String::as_str).unwrap_or("");
    assert!(last.contains(" OK"), "expected OK completion: {lines:?}");
}

/// Writes the captured transcript when `ALO_WIRE_TRANSCRIPTS` names a
/// directory. The first line is the section title the assembly script uses;
/// `normalize` replaces run-specific values with stable placeholders so
/// regenerated transcripts diff cleanly.
fn save(name: &str, title: &str, log: &[String], normalize: &[(String, &str)]) {
    let Some(dir) = std::env::var_os("ALO_WIRE_TRANSCRIPTS") else {
        return;
    };
    let mut text = format!("{title}\n");
    for line in log {
        let mut l = line.clone();
        for (from, to) in normalize {
            l = l.replace(from.as_str(), to);
        }
        text.push_str(&l);
        text.push('\n');
    }
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(std::path::Path::new(&dir).join(format!("{name}.txt")), text).unwrap();
}

/// The daily-driver IMAP session: LOGIN, SELECT, FETCH, STORE, IDLE (with a
/// delivery arriving mid-IDLE), LOGOUT — over implicit TLS.
#[tokio::test]
async fn imap_session_transcript() {
    let store = test_store().await;
    let (tenant, user, email, pw) = make_user(&store, "wire").await;
    deliver(
        &store,
        &tenant,
        &user,
        &message("Quarterly figures", "The numbers are attached."),
    )
    .await;
    let addr = spawn_imap(store.clone()).await;

    let mut c = Rec::connect(addr).await;
    let greeting = c.read_line().await;
    assert!(greeting.starts_with("* OK"), "{greeting}");

    c.send_shown(
        &format!("a1 LOGIN \"{email}\" \"{pw}\""),
        &format!("a1 LOGIN \"{email}\" \"<password>\""),
    )
    .await;
    assert_completes_ok(&c.until_tag("a1").await);

    let select = c.command("a2", "SELECT INBOX").await;
    assert_completes_ok(&select);
    assert!(select.iter().any(|l| l.contains("1 EXISTS")), "{select:?}");

    let fetch = c
        .command("a3", "FETCH 1 (UID FLAGS RFC822.SIZE ENVELOPE)")
        .await;
    assert_completes_ok(&fetch);
    assert!(
        fetch.iter().any(|l| l.contains("Quarterly figures")),
        "{fetch:?}"
    );

    let store_r = c.command("a4", "STORE 1 +FLAGS (\\Seen)").await;
    assert_completes_ok(&store_r);
    assert!(store_r.iter().any(|l| l.contains("\\Seen")), "{store_r:?}");

    // IDLE: the continuation, then a message delivered out of band (as SMTP
    // would) surfaces as an untagged EXISTS without any client command.
    c.send("a5 IDLE").await;
    let cont = c.read_line().await;
    assert!(cont.starts_with('+'), "{cont}");
    deliver(&store, &tenant, &user, &message("Live update", "arrived")).await;
    c.note("a second message is delivered while the connection idles");
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if c.read_line().await.contains("2 EXISTS") {
                break;
            }
        }
    })
    .await
    .expect("IDLE reports EXISTS");
    c.send("DONE").await;
    assert_completes_ok(&c.until_tag("a5").await);

    assert_completes_ok(&c.command("a6", "LOGOUT").await);

    save(
        "imap",
        "IMAP over implicit TLS: LOGIN / SELECT / FETCH / STORE / IDLE",
        &c.log,
        &[(email, "alice@example.test")],
    );
}

/// SASL XOAUTH2 on IMAP: the capability, a SASL-IR login with a live bearer
/// token, and the mechanism's error dialog for a revoked one.
#[tokio::test]
async fn imap_xoauth2_transcript() {
    let store = test_store().await;
    let (tenant, user, email, _pw) = make_user(&store, "wire-xo").await;
    let identity = test_identity(store.clone());
    let token = identity
        .issue_access_token(&tenant, &user, None, "openid email profile")
        .await
        .unwrap();
    let blob = B64.encode(format!(
        "user={email}\u{1}auth=Bearer {}\u{1}\u{1}",
        token.reveal()
    ));
    let shown = format!("<base64 of \"user={email}^Aauth=Bearer <token>^A^A\">");
    let addr = spawn_imap(store.clone()).await;

    let mut c = Rec::connect(addr).await;
    c.read_line().await;
    let caps = c.command("a1", "CAPABILITY").await;
    assert!(caps.iter().any(|l| l.contains("AUTH=XOAUTH2")), "{caps:?}");
    assert!(caps.iter().any(|l| l.contains("SASL-IR")), "{caps:?}");

    // SASL-IR: the blob rides the AUTHENTICATE line itself.
    c.send_shown(
        &format!("a2 AUTHENTICATE XOAUTH2 {blob}"),
        &format!("a2 AUTHENTICATE XOAUTH2 {shown}"),
    )
    .await;
    assert_completes_ok(&c.until_tag("a2").await);
    assert_completes_ok(&c.command("a3", "SELECT INBOX").await);
    assert_completes_ok(&c.command("a4", "LOGOUT").await);

    // Revoked → the mechanism's error dialog: a continuation carrying a
    // base64 JSON status, the client's empty acknowledgement, the tagged NO.
    identity.revoke_access_token(token.reveal()).await.unwrap();
    let mut c2 = Rec::connect(addr).await;
    c2.read_line().await;
    c2.note("the same token, after revocation");
    c2.send_shown(
        &format!("a1 AUTHENTICATE XOAUTH2 {blob}"),
        &format!("a1 AUTHENTICATE XOAUTH2 {shown}"),
    )
    .await;
    let cont = c2.read_line().await;
    let status = cont.strip_prefix("+ ").expect("error-status continuation");
    let decoded = String::from_utf8(B64.decode(status.trim()).unwrap()).unwrap();
    assert!(decoded.contains("\"status\":\"401\""), "{decoded}");
    c2.note(&format!("decoded continuation: {decoded}"));
    c2.send("").await;
    let no = c2.until_tag("a1").await;
    assert!(no.last().unwrap().contains(" NO"), "{no:?}");

    let mut log = c.log;
    log.extend(c2.log);
    save(
        "imap-xoauth2",
        "IMAP SASL XOAUTH2: capability, SASL-IR login, revoked-token error dialog",
        &log,
        &[(email, "alice@example.test")],
    );
}

/// The full POP3 flow over implicit TLS: USER/PASS, STAT, LIST, RETR, DELE,
/// QUIT (which commits the deletion).
#[tokio::test]
async fn pop3_transcript() {
    let store = test_store().await;
    let (tenant, user, email, pw) = make_user(&store, "wire-pop").await;
    deliver(
        &store,
        &tenant,
        &user,
        &message("Quarterly figures", "The numbers are attached."),
    )
    .await;
    let addr = spawn_pop3(store.clone()).await;

    let mut c = Rec::connect(addr).await;
    assert!(c.read_line().await.starts_with("+OK"));
    c.send(&format!("USER {email}")).await;
    assert!(c.read_line().await.starts_with("+OK"));
    c.send_shown(&format!("PASS {pw}"), "PASS <password>").await;
    assert!(c.read_line().await.starts_with("+OK"));

    c.send("STAT").await;
    assert!(c.read_line().await.starts_with("+OK 1 "));
    c.send("LIST").await;
    assert!(c.read_line().await.starts_with("+OK"));
    assert_eq!(c.read_multiline().await.len(), 1);
    c.send("RETR 1").await;
    assert!(c.read_line().await.starts_with("+OK"));
    let msg = c.read_multiline().await;
    assert!(
        msg.join("\n").contains("The numbers are attached."),
        "{msg:?}"
    );
    c.send("DELE 1").await;
    assert!(c.read_line().await.starts_with("+OK"));
    c.send("QUIT").await;
    assert!(c.read_line().await.starts_with("+OK"));

    save(
        "pop3",
        "POP3 over implicit TLS: USER / PASS / STAT / LIST / RETR / DELE / QUIT",
        &c.log,
        &[(email, "alice@example.test")],
    );
}
