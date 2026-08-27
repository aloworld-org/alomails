//! M6.1 scripted wire transcripts: the canonical SMTP submission exchange
//! (STARTTLS, AUTH, 8BITMIME — and the honest SMTPUTF8 refusal) and the
//! SASL XOAUTH2 dialog, driven by a real rustls client over a real socket
//! and recorded line by line. Each test asserts the exchange it captures.
//! When `ALO_WIRE_TRANSCRIPTS` names a directory, the transcript is written
//! there — `scripts/wire-transcripts.sh` runs these tests and splices the
//! output into `docs/interop.md`. Harness mirrors `submission_tls.rs`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use alo_identity::{Identity, IdentityConfig};
use alo_smtp::server::{self, Runtime};
use alo_smtp::spool::Spool;
use alo_smtp::tls;
use alo_store::{BlobStore, Store};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;
use tokio_rustls::rustls::{self, ClientConfig};

const HOSTNAME: &str = "mx.alo.test";
const IO_TIMEOUT: Duration = Duration::from_secs(10);

/// The database this suite runs against (`alo_test_db` refuses the
/// product's own database; suites create and drop their own).
fn database_url() -> String {
    alo_test_db::url()
}

/// Fast argon2 for tests: AUTH behaviour is parameter-independent and
/// production-strength hashing in a debug build outruns the socket timeout
/// (the same trade `submission_tls.rs` documents).
fn fast_config() -> IdentityConfig {
    let mut c = IdentityConfig::new("https://id.test");
    c.argon2_m_kib = 8;
    c.argon2_t = 1;
    c.argon2_p = 1;
    c
}

/// A per-test store carrying a fresh transcript user with a set password;
/// returns `(store, email)`.
async fn store_with_user(tag: &str) -> (Arc<Store>, String) {
    let store = Arc::new(
        Store::connect(&database_url(), BlobStore::in_memory(25 * 1024 * 1024))
            .await
            .unwrap(),
    );
    store.migrate().await.unwrap();
    let tenant = store.create_tenant(&format!("wire-{tag}")).await.unwrap();
    let email = format!("wire-{tag}-{tenant}@alo.test");
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&email)
        .await
        .unwrap();
    let identity = Identity::new(Arc::clone(&store), fast_config()).unwrap();
    identity
        .set_password(&tenant, &user, &email, "s3cret")
        .await
        .unwrap();
    (store, email)
}

/// Spawns a STARTTLS submission listener over `store`; returns address and
/// spool.
async fn spawn_submission(store: Arc<Store>) -> (SocketAddr, Arc<Spool>, tempfile::TempDir) {
    let identity = Identity::new(store, fast_config()).unwrap();
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
        false,
        25 * 1024 * 1024,
        100,
        256,
    ));
    tokio::spawn(async move {
        let _ = server::serve(listener, runtime).await;
    });
    (addr, spool, dir)
}

/// A test-only verifier accepting the server's self-signed certificate.
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
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }
    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
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

/// A recording line-oriented SMTP client over any async stream. The log
/// survives the STARTTLS upgrade via `into_parts`/`with_log`.
struct Rec<S> {
    reader: BufReader<S>,
    log: Vec<String>,
}

impl<S: AsyncReadExt + AsyncWriteExt + Unpin> Rec<S> {
    fn new(stream: S) -> Self {
        Self::with_log(stream, Vec::new())
    }

    fn with_log(stream: S, log: Vec<String>) -> Self {
        Self {
            reader: BufReader::new(stream),
            log,
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
        let data = format!("{line}\r\n");
        tokio::time::timeout(IO_TIMEOUT, self.reader.get_mut().write_all(data.as_bytes()))
            .await
            .expect("write timed out")
            .expect("write failed");
        self.reader.get_mut().flush().await.expect("flush");
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
                let s = String::from_utf8_lossy(&line).trim_end().to_owned();
                self.log.push(format!("S: {s}"));
                return s;
            }
        }
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

    async fn cmd(&mut self, line: &str) -> String {
        self.send(line).await;
        self.read_reply().await
    }

    fn into_parts(self) -> (S, Vec<String>) {
        (self.reader.into_inner(), self.log)
    }
}

/// Writes the captured transcript when `ALO_WIRE_TRANSCRIPTS` names a
/// directory; first line is the section title, `normalize` stabilises
/// run-specific values.
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

/// The canonical submission session: EHLO, STARTTLS, EHLO again, AUTH
/// PLAIN, a MAIL→DATA transaction declaring BODY=8BITMIME with an 8-bit
/// UTF-8 body — and the honest refusal of the unadvertised SMTPUTF8
/// parameter (RFC 6531 is deliberately not offered yet).
#[tokio::test]
async fn submission_transcript() {
    let (store, email) = store_with_user("submit").await;
    let (addr, spool, _dir) = spawn_submission(store).await;

    let tcp = TcpStream::connect(addr).await.unwrap();
    let mut plain = Rec::new(tcp);
    assert!(plain.read_reply().await.starts_with("220 "));

    plain.send("EHLO client.example").await;
    let ehlo = plain.read_reply().await;
    assert!(ehlo.starts_with("250"), "{ehlo}");
    let pre_tls: Vec<String> = plain.log.clone();
    assert!(
        pre_tls.iter().any(|l| l.contains("STARTTLS")),
        "{pre_tls:?}"
    );
    assert!(
        pre_tls.iter().any(|l| l.contains("8BITMIME")),
        "{pre_tls:?}"
    );
    assert!(
        !pre_tls.iter().any(|l| l.contains("SMTPUTF8")),
        "SMTPUTF8 must not be advertised: {pre_tls:?}"
    );

    assert!(plain.cmd("STARTTLS").await.starts_with("220 "));
    let (tcp, mut log) = plain.into_parts();
    log.push("  (TLS handshake; the session state resets)".to_owned());
    let server_name = rustls::pki_types::ServerName::try_from("localhost").unwrap();
    let tls: TlsStream<TcpStream> = tls_connector().connect(server_name, tcp).await.unwrap();
    let mut sec = Rec::with_log(tls, log);

    sec.send("EHLO client.example").await;
    let ehlo2 = sec.read_reply().await;
    assert!(ehlo2.starts_with("250"), "{ehlo2}");

    let auth = B64.encode(format!("\u{0}{email}\u{0}s3cret"));
    sec.send_shown(
        &format!("AUTH PLAIN {auth}"),
        "AUTH PLAIN <base64 of \"\\0alice@alo.test\\0<password>\">",
    )
    .await;
    assert!(sec.read_reply().await.starts_with("235 "));

    assert!(
        sec.cmd(&format!("MAIL FROM:<{email}> BODY=8BITMIME"))
            .await
            .starts_with("250 ")
    );
    assert!(
        sec.cmd("RCPT TO:<bob@example.org>")
            .await
            .starts_with("250 ")
    );
    assert!(sec.cmd("DATA").await.starts_with("354 "));
    for line in [
        "Subject: Zahlen fürs Quartal",
        "",
        "Zwölf Boxkämpfer jagen Viktor quer über den großen Sylter Deich.",
        ".",
    ] {
        sec.send(line).await;
    }
    let queued = sec.read_reply().await;
    assert!(queued.starts_with("250 OK: queued as "), "{queued}");
    let id = queued.rsplit(' ').next().unwrap().to_owned();

    // The 8-bit body reached the spool byte-intact.
    let (envelope, message) = spool.read(&id).unwrap();
    assert_eq!(envelope.mail_from.as_deref(), Some(email.as_str()));
    let text = String::from_utf8(message).unwrap();
    assert!(text.contains("Zwölf Boxkämpfer"), "{text}");

    // SMTPUTF8 is not advertised, so the parameter is refused (555).
    sec.note("SMTPUTF8 (RFC 6531) is not offered; the parameter is refused");
    assert!(
        sec.cmd(&format!("MAIL FROM:<{email}> SMTPUTF8"))
            .await
            .starts_with("555 ")
    );
    assert!(sec.cmd("QUIT").await.starts_with("221 "));

    save(
        "smtp-submission",
        "SMTP submission: STARTTLS, AUTH PLAIN, 8BITMIME transaction, SMTPUTF8 refusal",
        &sec.log,
        &[(email, "alice@alo.test"), (id, "<spool-id>")],
    );
}

/// SASL XOAUTH2 on submission: advertised after TLS, a live bearer
/// authenticates via initial response, a revoked one runs the 334
/// error-status dialog to 535.
#[tokio::test]
async fn xoauth2_transcript() {
    let (store, email) = store_with_user("xo").await;
    let identity = Identity::new(Arc::clone(&store), fast_config()).unwrap();
    let (tenant, user) = store.account_by_email(&email).await.unwrap().unwrap();
    let token = identity
        .issue_access_token(&tenant, &user, None, "openid email profile")
        .await
        .unwrap();
    let blob = B64.encode(format!(
        "user={email}\u{1}auth=Bearer {}\u{1}\u{1}",
        token.reveal()
    ));
    let shown = format!("<base64 of \"user={email}^Aauth=Bearer <token>^A^A\">");
    let (addr, _spool, _dir) = spawn_submission(store).await;

    let connect = |addr: SocketAddr| async move {
        let tcp = TcpStream::connect(addr).await.unwrap();
        let mut plain = Rec::new(tcp);
        assert!(plain.read_reply().await.starts_with("220 "));
        plain.send("EHLO client.example").await;
        plain.read_reply().await;
        assert!(plain.cmd("STARTTLS").await.starts_with("220 "));
        let (tcp, mut log) = plain.into_parts();
        log.push("  (TLS handshake)".to_owned());
        let name = rustls::pki_types::ServerName::try_from("localhost").unwrap();
        let tls: TlsStream<TcpStream> = tls_connector().connect(name, tcp).await.unwrap();
        let mut sec = Rec::with_log(tls, log);
        sec.send("EHLO client.example").await;
        sec.read_reply().await;
        sec
    };

    let mut sec = connect(addr).await;
    assert!(
        sec.log.iter().any(|l| l.contains("XOAUTH2")),
        "XOAUTH2 advertised after TLS: {:?}",
        sec.log
    );
    sec.send_shown(
        &format!("AUTH XOAUTH2 {blob}"),
        &format!("AUTH XOAUTH2 {shown}"),
    )
    .await;
    assert!(sec.read_reply().await.starts_with("235 "));
    assert!(sec.cmd("QUIT").await.starts_with("221 "));

    identity.revoke_access_token(token.reveal()).await.unwrap();
    let mut sec2 = connect(addr).await;
    sec2.note("the same token, after revocation");
    sec2.send_shown(
        &format!("AUTH XOAUTH2 {blob}"),
        &format!("AUTH XOAUTH2 {shown}"),
    )
    .await;
    let challenge = sec2.read_reply().await;
    let status = challenge.strip_prefix("334 ").expect("error-status dialog");
    let decoded = String::from_utf8(B64.decode(status.trim()).unwrap()).unwrap();
    assert!(decoded.contains("\"status\":\"401\""), "{decoded}");
    sec2.note(&format!("decoded challenge: {decoded}"));
    sec2.send("").await;
    assert!(sec2.read_reply().await.starts_with("535 "));

    let mut log = sec.log;
    log.extend(sec2.log);
    save(
        "smtp-xoauth2",
        "SMTP submission SASL XOAUTH2: bearer login, revoked-token error dialog",
        &log,
        &[(email, "alice@alo.test")],
    );
}
