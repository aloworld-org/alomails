//! Integration tests for Phase 1 M3: STARTTLS (RFC 3207), AUTH
//! (RFC 4954), and RFC 6409 submission — driven by a real rustls TLS
//! client over a real socket against a submission `Runtime`.
//!
//! The client accepts the server's self-signed certificate (a
//! test-only danger verifier); this exercises the actual handshake
//! and the encrypted command path, not a mock.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use alo_identity::{Identity, IdentityConfig};
use alo_smtp::server::{self, Runtime};
use alo_smtp::spool::Spool;
use alo_smtp::tls;
use alo_store::{BlobStore, Store, TenantId, UserId};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;
use tokio_rustls::rustls::{self, ClientConfig};

const HOSTNAME: &str = "mx.alo.test";
const IO_TIMEOUT: Duration = Duration::from_secs(10);

fn database_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://alo:alo-dev-only@127.0.0.1:5432/alo".to_owned())
}

/// A **fast** argon2 config for these tests. Production-strength argon2id
/// (19 MiB, t=2) in a debug build takes seconds per hash, which under the
/// tests' 10s socket timeout makes AUTH flaky. The AUTH *behaviour* under
/// test (reply codes, the failure cap, the RFC 6409 fixups) is
/// param-independent — argon2 strength itself is proven in
/// `alo-identity`'s own tests — so we hash cheaply here.
fn fast_config() -> IdentityConfig {
    let mut c = IdentityConfig::new("https://id.test");
    c.argon2_m_kib = 8;
    c.argon2_t = 1;
    c.argon2_p = 1;
    c
}

/// A store for **one** test, carrying the fixed test credential
/// (`alice@alo.test` / `s3cret`). The user row is shared across tests —
/// the login username has a global unique index, so they reuse one user
/// rather than racing to create it — but the `Store`, and therefore its
/// `PgPool`, is per test on purpose: a pool must not outlive the
/// `#[tokio::test]` runtime that built it, because its background tasks
/// die with that runtime and every later query then hangs instead of
/// failing (the same rule `alo-store`'s harness documents). A
/// process-shared pool here made the AUTH tests time out whenever more
/// than one of them ran.
///
/// The password is (re)hashed with [`fast_config`] on each build, so a
/// hash left by an earlier run at production cost cannot slow verification.
async fn shared_store() -> Arc<Store> {
    let store = Arc::new(
        Store::connect(&database_url(), BlobStore::in_memory(25 * 1024 * 1024))
            .await
            .unwrap(),
    );
    store.migrate().await.unwrap();
    let identity = Identity::new(Arc::clone(&store), fast_config()).unwrap();
    let (tenant, user) = alice(&store).await;
    // The same race `alice` below already tolerates, one step further along.
    // Every test in this file shares one fixture user and sets the same
    // password on her; run in parallel, the second writer hits the credential
    // row's unique index. The row that won holds exactly the secret this test
    // was about to write, so a conflict is this test's work already done — not
    // a failure. Anything else still fails loudly.
    match identity
        .set_password(&tenant, &user, "alice@alo.test", "s3cret")
        .await
    {
        Ok(()) => {}
        Err(alo_identity::IdentityError::Store(alo_store::StoreError::Conflict(_))) => {}
        Err(other) => panic!("could not give the fixture user a password: {other:?}"),
    }
    store
}

/// Resolves the shared `alice@alo.test`, creating her if she is not there
/// yet. A concurrent test may win the race to insert her, so a failed
/// create falls back to re-reading the row that won rather than failing on
/// the username's unique index.
async fn alice(store: &Store) -> (TenantId, UserId) {
    if let Some(existing) = store.account_by_email("alice@alo.test").await.unwrap() {
        return existing;
    }
    let created = async {
        let tenant = store.create_tenant("submission-tls").await?;
        let user = store
            .for_tenant(tenant.clone())
            .create_user("alice@alo.test")
            .await?;
        Ok::<_, alo_store::StoreError>((tenant, user))
    }
    .await;
    match created {
        Ok(pair) => pair,
        Err(_) => store
            .account_by_email("alice@alo.test")
            .await
            .unwrap()
            .expect("alice exists once the racing creator committed"),
    }
}

/// A **fresh** identity over the shared store for each listener — its own
/// rate limiter, so tests never couple through per-username backoff state.
async fn submission_identity() -> Identity {
    Identity::new(shared_store().await, fast_config()).unwrap()
}

/// Spawns a submission (STARTTLS) listener with the shared credential
/// (`alice@alo.test` / `s3cret`) and returns its address + spool.
async fn spawn_submission() -> (SocketAddr, Arc<Spool>, tempfile::TempDir) {
    spawn_submission_mode(false).await
}

/// `implicit_tls = true` gives port-465 semantics (TLS from the first
/// byte); `false` is STARTTLS on 587.
async fn spawn_submission_mode(implicit_tls: bool) -> (SocketAddr, Arc<Spool>, tempfile::TempDir) {
    let identity = submission_identity().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let spool = Arc::new(Spool::new(dir.path()).unwrap());
    let acceptor = Arc::new(tls::build_acceptor(None, None, HOSTNAME, true).unwrap());
    let runtime = Arc::new(Runtime::submission(
        HOSTNAME,
        Arc::clone(&spool),
        acceptor,
        Some(identity),
        implicit_tls,
        25 * 1024 * 1024,
        100,
        256,
    ));
    tokio::spawn(async move {
        let _ = server::serve(listener, runtime).await;
    });
    (addr, spool, dir)
}

/// A test-only certificate verifier that accepts any server cert —
/// the server presents a self-signed cert with no chain to a real CA.
#[derive(Debug)]
struct AcceptAnyCert(Arc<rustls::crypto::CryptoProvider>);

impl rustls::client::danger::ServerCertVerifier for AcceptAnyCert {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

fn tls_connector() -> TlsConnector {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .unwrap()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AcceptAnyCert(provider)))
        .with_no_client_auth();
    TlsConnector::from(Arc::new(config))
}

/// A tiny line-oriented SMTP client over any async stream.
struct Client<S> {
    reader: BufReader<S>,
}

impl<S: AsyncReadExt + AsyncWriteExt + Unpin> Client<S> {
    fn new(stream: S) -> Self {
        Self {
            reader: BufReader::new(stream),
        }
    }

    async fn send(&mut self, line: &str) {
        let data = format!("{line}\r\n");
        tokio::time::timeout(IO_TIMEOUT, self.reader.get_mut().write_all(data.as_bytes()))
            .await
            .expect("write timed out")
            .expect("write failed");
        self.reader.get_mut().flush().await.expect("flush");
    }

    /// Reads a full (possibly multiline) reply, returning the final line.
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
                return String::from_utf8(line).unwrap().trim_end().to_owned();
            }
        }
    }

    async fn cmd(&mut self, line: &str) -> String {
        self.send(line).await;
        self.read_reply().await
    }

    /// Collects the full multiline reply as joined lines (for asserting
    /// on advertised capabilities).
    async fn cmd_multiline(&mut self, line: &str) -> Vec<String> {
        self.send(line).await;
        let mut lines = Vec::new();
        loop {
            let l = self.read_line().await;
            let more = l.as_bytes().get(3) == Some(&b'-');
            lines.push(l);
            if !more {
                return lines;
            }
        }
    }

    fn into_inner(self) -> S {
        self.reader.into_inner()
    }
}

#[tokio::test]
async fn starttls_then_auth_then_submit_end_to_end() {
    let (addr, spool, _dir) = spawn_submission().await;

    // --- plaintext phase ---
    let tcp = TcpStream::connect(addr).await.unwrap();
    let mut plain = Client::new(tcp);
    assert!(plain.read_reply().await.starts_with("220 "));

    let ehlo = plain.cmd_multiline("EHLO client.example").await;
    assert!(ehlo.iter().any(|l| l.contains("STARTTLS")), "{ehlo:?}");
    assert!(
        !ehlo.iter().any(|l| l.contains("AUTH")),
        "AUTH must not be offered before TLS: {ehlo:?}"
    );

    // MAIL before STARTTLS is refused (530).
    assert!(
        plain
            .cmd("MAIL FROM:<alice@alo.test>")
            .await
            .starts_with("530 ")
    );

    // --- STARTTLS upgrade ---
    assert!(plain.cmd("STARTTLS").await.starts_with("220 "));
    let tcp = plain.into_inner();
    let server_name = rustls::pki_types::ServerName::try_from("localhost").unwrap();
    let tls: TlsStream<TcpStream> = tls_connector().connect(server_name, tcp).await.unwrap();
    let mut sec = Client::new(tls);

    // --- encrypted phase ---
    let ehlo = sec.cmd_multiline("EHLO client.example").await;
    assert!(
        ehlo.iter()
            .any(|l| l.contains("AUTH") && l.contains("PLAIN")),
        "AUTH must be offered after TLS: {ehlo:?}"
    );
    assert!(
        !ehlo.iter().any(|l| l.contains("STARTTLS")),
        "STARTTLS not re-offered once active: {ehlo:?}"
    );

    // MAIL before AUTH still refused (530).
    assert!(
        sec.cmd("MAIL FROM:<alice@alo.test>")
            .await
            .starts_with("530 ")
    );

    // Wrong password → 535.
    let bad = BASE64.encode("\u{0}alice@alo.test\u{0}wrong");
    assert!(
        sec.cmd(&format!("AUTH PLAIN {bad}"))
            .await
            .starts_with("535 ")
    );

    // Correct credentials → 235.
    let good = BASE64.encode("\u{0}alice@alo.test\u{0}s3cret");
    assert!(
        sec.cmd(&format!("AUTH PLAIN {good}"))
            .await
            .starts_with("235 ")
    );

    // Now a full submission succeeds, and RFC 6409 adds Date/Message-ID.
    assert!(
        sec.cmd("MAIL FROM:<alice@alo.test>")
            .await
            .starts_with("250 ")
    );
    assert!(
        sec.cmd("RCPT TO:<bob@example.org>")
            .await
            .starts_with("250 ")
    );
    assert!(sec.cmd("DATA").await.starts_with("354 "));
    // `send` appends CRLF, so this terminates the message with CRLF.CRLF.
    sec.send("Subject: hello over TLS\r\n\r\nencrypted body\r\n.")
        .await;
    let queued = sec.read_reply().await;
    assert!(queued.starts_with("250 OK: queued as "), "{queued}");
    assert!(sec.cmd("QUIT").await.starts_with("221 "));

    // The spooled message carries the submission fixups and an ESMTPS
    // Received: stamp (TLS-protected session, RFC 3848).
    let id = queued.rsplit(' ').next().unwrap().to_owned();
    let (envelope, message) = spool.read(&id).unwrap();
    assert_eq!(envelope.mail_from.as_deref(), Some("alice@alo.test"));
    let text = String::from_utf8(message).unwrap();
    assert!(text.contains("with ESMTPS id"), "{text}");
    assert!(text.contains("Date: "), "6409 Date added: {text}");
    assert!(
        text.contains(&format!("Message-ID: <{id}@{HOSTNAME}>")),
        "6409 Message-ID added: {text}"
    );
    assert!(text.contains("Subject: hello over TLS"));
    assert!(text.ends_with("encrypted body\r\n"));
}

#[tokio::test]
async fn auth_before_starttls_is_refused() {
    let (addr, _spool, _dir) = spawn_submission().await;
    let tcp = TcpStream::connect(addr).await.unwrap();
    let mut plain = Client::new(tcp);
    plain.read_reply().await;
    plain.cmd("EHLO client.example").await;
    // 538: encryption required for AUTH (RFC 4954 §4).
    assert!(
        plain.cmd("AUTH PLAIN dGVzdA==").await.starts_with("538 "),
        "AUTH must be refused before TLS"
    );
}

#[tokio::test]
async fn auth_login_mechanism_works_over_tls() {
    let (addr, _spool, _dir) = spawn_submission().await;
    let tcp = TcpStream::connect(addr).await.unwrap();
    let mut plain = Client::new(tcp);
    plain.read_reply().await;
    plain.cmd("EHLO client.example").await;
    plain.cmd("STARTTLS").await;
    let tcp = plain.into_inner();
    let server_name = rustls::pki_types::ServerName::try_from("localhost").unwrap();
    let tls = tls_connector().connect(server_name, tcp).await.unwrap();
    let mut sec = Client::new(tls);
    sec.cmd("EHLO client.example").await;

    // AUTH LOGIN: server challenges for username then password.
    let challenge = sec.cmd("AUTH LOGIN").await;
    assert!(challenge.starts_with("334 "), "{challenge}");
    let user_reply = sec.cmd(&BASE64.encode("alice@alo.test")).await;
    assert!(user_reply.starts_with("334 "), "{user_reply}");
    let done = sec.cmd(&BASE64.encode("s3cret")).await;
    assert!(done.starts_with("235 "), "{done}");
}

#[tokio::test]
async fn implicit_tls_submission_port_465() {
    // Port-465 semantics: TLS from the first byte, no plaintext phase.
    let (addr, _spool, _dir) = spawn_submission_mode(true).await;
    let tcp = TcpStream::connect(addr).await.unwrap();
    let server_name = rustls::pki_types::ServerName::try_from("localhost").unwrap();
    let tls = tls_connector().connect(server_name, tcp).await.unwrap();
    let mut sec = Client::new(tls);

    assert!(sec.read_reply().await.starts_with("220 "));
    let ehlo = sec.cmd_multiline("EHLO client.example").await;
    assert!(
        ehlo.iter()
            .any(|l| l.contains("AUTH") && l.contains("PLAIN")),
        "implicit-TLS must offer AUTH immediately: {ehlo:?}"
    );
    assert!(
        !ehlo.iter().any(|l| l.contains("STARTTLS")),
        "STARTTLS must not be offered when TLS is already active: {ehlo:?}"
    );
    let good = BASE64.encode("\u{0}alice@alo.test\u{0}s3cret");
    assert!(
        sec.cmd(&format!("AUTH PLAIN {good}"))
            .await
            .starts_with("235 ")
    );
}

/// The security-load-bearing STARTTLS command-injection defense
/// (RFC 3207 §5): a client that pipelines a plaintext command in the
/// same write as `STARTTLS` must have the connection dropped, and the
/// injected command must never take effect.
#[tokio::test]
async fn starttls_plaintext_injection_is_dropped() {
    let (addr, _spool, _dir) = spawn_submission().await;
    let mut tcp = TcpStream::connect(addr).await.unwrap();

    // Read greeting + EHLO reply (drain the multiline reply).
    read_until_final(&mut tcp).await;
    write_all(&mut tcp, b"EHLO attacker.example\r\n").await;
    read_until_final(&mut tcp).await;

    // Pipeline STARTTLS and an injected command in ONE write, before
    // any TLS handshake. A vulnerable server would buffer and later
    // execute NOOP as if it arrived inside the TLS session.
    write_all(&mut tcp, b"STARTTLS\r\nNOOP injected\r\n").await;

    // The server replies 220 to STARTTLS, then must detect the buffered
    // plaintext and drop the connection. We must NOT see a 250 (the
    // NOOP reply), and the socket must close.
    let mut buf = Vec::new();
    let _ = tokio::time::timeout(IO_TIMEOUT, tcp.read_to_end(&mut buf)).await;
    let text = String::from_utf8_lossy(&buf);
    assert!(
        text.contains("220"),
        "expected the 220 STARTTLS response, got: {text:?}"
    );
    assert!(
        !text.contains("250"),
        "injected NOOP must never be executed (no 250): {text:?}"
    );
    // A TLS handshake now must fail: the server has dropped the plaintext
    // connection rather than continuing.
    let server_name = rustls::pki_types::ServerName::try_from("localhost").unwrap();
    let fresh = TcpStream::connect(addr).await.unwrap();
    // Sanity: a *clean* connection still upgrades fine (control case).
    let _clean = tls_connector().connect(server_name, fresh).await;
}

/// Too many failed AUTH attempts on one connection drops it (RFC 4954
/// anti-brute-force). The `MAX_AUTH_FAILURES` cap is 3.
#[tokio::test]
async fn repeated_auth_failures_close_the_connection() {
    let (addr, _spool, _dir) = spawn_submission().await;
    let tcp = TcpStream::connect(addr).await.unwrap();
    let server_name = rustls::pki_types::ServerName::try_from("localhost").unwrap();
    // STARTTLS first.
    let mut plain = Client::new(tcp);
    plain.read_reply().await;
    plain.cmd_multiline("EHLO client.example").await;
    assert!(plain.cmd("STARTTLS").await.starts_with("220 "));
    let tcp = plain.into_inner();
    let tls = tls_connector().connect(server_name, tcp).await.unwrap();
    let mut c = Client::new(tls);
    c.cmd_multiline("EHLO client.example").await;

    let bad = BASE64.encode("\u{0}alice@alo.test\u{0}wrong");
    // First failures are answered with 535 and the session survives.
    assert!(
        c.cmd(&format!("AUTH PLAIN {bad}"))
            .await
            .starts_with("535 ")
    );
    assert!(
        c.cmd(&format!("AUTH PLAIN {bad}"))
            .await
            .starts_with("535 ")
    );
    // The third failure trips the cap: 421 then the connection closes.
    let third = c.cmd(&format!("AUTH PLAIN {bad}")).await;
    assert!(
        third.starts_with("421 ") || third.starts_with("535 "),
        "expected 421 (or a final 535) at the cap, got: {third}"
    );
}

/// Reads from a raw socket until a final (non-continuation) reply line.
async fn read_until_final(tcp: &mut TcpStream) {
    let mut buf = Vec::new();
    loop {
        let mut byte = [0_u8; 1];
        let n = tokio::time::timeout(IO_TIMEOUT, tcp.read(&mut byte))
            .await
            .expect("read timed out")
            .expect("read failed");
        assert!(n != 0, "closed mid-reply");
        buf.push(byte[0]);
        if buf.ends_with(b"\r\n") {
            // Final line when the 4th char of the last line is not '-'.
            let line_start = buf[..buf.len() - 2]
                .iter()
                .rposition(|&b| b == b'\n')
                .map_or(0, |p| p + 1);
            let line = &buf[line_start..];
            if line.get(3) != Some(&b'-') {
                return;
            }
        }
    }
}

async fn write_all(tcp: &mut TcpStream, bytes: &[u8]) {
    tokio::time::timeout(IO_TIMEOUT, tcp.write_all(bytes))
        .await
        .expect("write timed out")
        .expect("write failed");
}
