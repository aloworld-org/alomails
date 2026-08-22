//! Integration-test harness: a live-Postgres store, an IMAP server on an
//! ephemeral implicit-TLS port, and a minimal TLS IMAP client that reads
//! real bytes off the wire. Mirrors the JMAP/SMTP suites' shape.
#![allow(clippy::unwrap_used, clippy::expect_used, dead_code)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use alo_identity::{Identity, IdentityConfig};
use alo_imap::stream::ImapStream;
use alo_imap::{Config, Session};
use alo_store::{BlobStore, Store, TenantId, UserId};
use rustls::pki_types::ServerName;
use sqlx::postgres::PgPoolOptions;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tokio_rustls::{TlsAcceptor, TlsConnector};

/// The database this suite runs against.
///
/// Delegates to `alo_test_db`, which refuses the database the product
/// runs on: suites create and drop their own, they never write into `alo`.
pub fn database_url() -> String {
    alo_test_db::url()
}

/// A migrated store on a small test-local pool.
pub async fn test_store() -> Arc<Store> {
    let pool = PgPoolOptions::new()
        .max_connections(6)
        .connect(&database_url())
        .await
        .expect("connect to test postgres (is compose up? is DATABASE_URL set?)");
    let store = Store::new(pool, BlobStore::in_memory(25 * 1024 * 1024));
    store.migrate().await.expect("run migrations");
    Arc::new(store)
}

/// Builds a test `Identity` over a store handle.
pub fn test_identity(store: Arc<Store>) -> Identity {
    Identity::new(store, IdentityConfig::new("https://id.test")).expect("identity")
}

/// Creates a tenant with one credentialed user; returns `(tenant, user,
/// email, password)`.
pub async fn make_user(store: &Arc<Store>, tag: &str) -> (TenantId, UserId, String, String) {
    let tenant = store.create_tenant(&format!("imap-{tag}")).await.unwrap();
    let ts = store.for_tenant(tenant.clone());
    // Random tenant id in the email keeps the global username index unique.
    let email = format!("{tag}-{tenant}@example.test");
    let user = ts.create_user(&email).await.unwrap();
    test_identity(Arc::clone(store))
        .set_password(&tenant, &user, &email, "s3cret-pw")
        .await
        .unwrap();
    (tenant, user, email, "s3cret-pw".to_owned())
}

/// Delivers a raw message into a user's inbox (simulating SMTP), returning
/// nothing — tests observe it over IMAP.
pub async fn deliver(store: &Store, tenant: &TenantId, user: &UserId, raw: &[u8]) {
    store
        .for_account(tenant.clone(), user.clone())
        .deliver(raw)
        .await
        .unwrap();
}

/// A synthetic RFC 5322 message with a chosen subject/body.
pub fn message(subject: &str, body: &str) -> Vec<u8> {
    format!(
        "From: sender@example.test\r\nTo: rcpt@example.test\r\n\
         Subject: {subject}\r\nMessage-ID: <{subject}@example.test>\r\n\r\n{body}\r\n"
    )
    .into_bytes()
}

/// A short-idle test config.
pub fn test_config() -> Config {
    Config {
        idle_timeout: Duration::from_secs(60),
        ..Config::default()
    }
}

/// Binds an ephemeral implicit-TLS IMAP listener and returns its address.
pub async fn spawn_imap(store: Arc<Store>) -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let acceptor: TlsAcceptor =
        alo_imap::tls::build_acceptor(None, None, "localhost", true).expect("acceptor");
    let cfg = Arc::new(test_config());
    let identity = test_identity(Arc::clone(&store));
    tokio::spawn(async move {
        loop {
            let (tcp, _) = match listener.accept().await {
                Ok(v) => v,
                Err(_) => break,
            };
            let acceptor = acceptor.clone();
            let cfg = cfg.clone();
            let store = store.clone();
            let identity = identity.clone();
            tokio::spawn(async move {
                if let Ok(tls) = acceptor.accept(tcp).await {
                    let session =
                        Session::new(ImapStream::Tls(Box::new(tls)), cfg, store, identity, None);
                    let _ = session.run().await;
                }
            });
        }
    });
    addr
}

/// Binds an ephemeral **cleartext** IMAP listener offering STARTTLS.
pub async fn spawn_imap_starttls(store: Arc<Store>) -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let acceptor: TlsAcceptor =
        alo_imap::tls::build_acceptor(None, None, "localhost", true).expect("acceptor");
    let cfg = Arc::new(test_config());
    let identity = test_identity(Arc::clone(&store));
    tokio::spawn(async move {
        loop {
            let Ok((tcp, _)) = listener.accept().await else {
                break;
            };
            let cfg = cfg.clone();
            let store = store.clone();
            let identity = identity.clone();
            let acceptor = acceptor.clone();
            tokio::spawn(async move {
                let session =
                    Session::new(ImapStream::Plain(tcp), cfg, store, identity, Some(acceptor));
                let _ = session.run().await;
            });
        }
    });
    addr
}

/// Binds an ephemeral implicit-TLS POP3 listener.
pub async fn spawn_pop3(store: Arc<Store>) -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let acceptor: TlsAcceptor =
        alo_imap::tls::build_acceptor(None, None, "localhost", true).expect("acceptor");
    let cfg = Arc::new(test_config());
    let identity = test_identity(Arc::clone(&store));
    tokio::spawn(async move {
        loop {
            let Ok((tcp, _)) = listener.accept().await else {
                break;
            };
            let cfg = cfg.clone();
            let store = store.clone();
            let identity = identity.clone();
            let acceptor = acceptor.clone();
            tokio::spawn(async move {
                if let Ok(tls) = acceptor.accept(tcp).await {
                    let session = alo_imap::pop3::Pop3Session::new(
                        ImapStream::Tls(Box::new(tls)),
                        cfg,
                        store,
                        identity,
                    );
                    let _ = session.run().await;
                }
            });
        }
    });
    addr
}

/// Connects a raw TLS stream (no greeting assertion) — for POP3, whose
/// greeting is `+OK`, not `* OK`.
pub async fn connect_tls(addr: SocketAddr) -> TlsStream<TcpStream> {
    let tcp = TcpStream::connect(addr).await.unwrap();
    let connector = TlsConnector::from(Arc::new(danger::client_config()));
    let name = ServerName::try_from("localhost").unwrap();
    connector.connect(name, tcp).await.unwrap()
}

/// A TLS IMAP test client that reads real bytes.
pub struct Client {
    stream: TlsStream<TcpStream>,
    buf: Vec<u8>,
    tag: u32,
}

impl Client {
    /// Connects and consumes the greeting line.
    pub async fn connect(addr: SocketAddr) -> Self {
        let tcp = TcpStream::connect(addr).await.unwrap();
        let connector = TlsConnector::from(Arc::new(danger::client_config()));
        let name = ServerName::try_from("localhost").unwrap();
        let stream = connector.connect(name, tcp).await.unwrap();
        let mut c = Self {
            stream,
            buf: Vec::new(),
            tag: 0,
        };
        let greeting = c.read_line().await;
        assert!(greeting.starts_with("* OK"), "greeting: {greeting}");
        c
    }

    /// Wraps an already-connected TLS stream without reading a greeting
    /// (used for POP3, whose greeting differs).
    pub fn attach(stream: TlsStream<TcpStream>) -> Self {
        Self {
            stream,
            buf: Vec::new(),
            tag: 0,
        }
    }

    /// Reads a POP3 multiline response body (lines until a lone `.`),
    /// returning the de-stuffed lines (excluding the terminator).
    pub async fn read_multiline(&mut self) -> Vec<String> {
        let mut lines = Vec::new();
        loop {
            let line = self.read_line().await;
            if line == "." {
                return lines;
            }
            lines.push(line.strip_prefix('.').map(str::to_owned).unwrap_or(line));
        }
    }

    /// Reads one CRLF-terminated line (without the CRLF).
    pub async fn read_line(&mut self) -> String {
        loop {
            if let Some(pos) = self.buf.iter().position(|&b| b == b'\n') {
                let line: Vec<u8> = self.buf.drain(..=pos).collect();
                let mut s = String::from_utf8_lossy(&line).into_owned();
                while s.ends_with('\n') || s.ends_with('\r') {
                    s.pop();
                }
                return s;
            }
            let mut tmp = [0u8; 4096];
            let n = self.stream.read(&mut tmp).await.unwrap();
            if n == 0 {
                return String::from_utf8_lossy(&std::mem::take(&mut self.buf)).into_owned();
            }
            self.buf.extend_from_slice(&tmp[..n]);
        }
    }

    /// Reads exactly `n` bytes (for verifying literal payloads).
    pub async fn read_exact(&mut self, n: usize) -> Vec<u8> {
        while self.buf.len() < n {
            let mut tmp = [0u8; 8192];
            let r = self.stream.read(&mut tmp).await.unwrap();
            if r == 0 {
                break;
            }
            self.buf.extend_from_slice(&tmp[..r]);
        }
        self.buf.drain(..n.min(self.buf.len())).collect()
    }

    /// Writes raw bytes.
    pub async fn write(&mut self, bytes: &[u8]) {
        self.stream.write_all(bytes).await.unwrap();
        self.stream.flush().await.unwrap();
    }

    /// Sends a tagged command and returns all response lines up to and
    /// including the tagged completion. The tag is auto-generated.
    pub async fn command(&mut self, cmd: &str) -> Vec<String> {
        self.tag += 1;
        let tag = format!("a{}", self.tag);
        self.write(format!("{tag} {cmd}\r\n").as_bytes()).await;
        self.read_until_tag(&tag).await
    }

    /// Sends a command with an explicit tag.
    pub async fn command_tagged(&mut self, tag: &str, cmd: &str) -> Vec<String> {
        self.write(format!("{tag} {cmd}\r\n").as_bytes()).await;
        self.read_until_tag(tag).await
    }

    /// Reads lines until one begins with `<tag> `.
    pub async fn read_until_tag(&mut self, tag: &str) -> Vec<String> {
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

    /// Logs in over the (already-TLS) connection.
    pub async fn login(&mut self, user: &str, pass: &str) -> Vec<String> {
        self.command(&format!("LOGIN \"{user}\" \"{pass}\"")).await
    }
}

/// Returns the tagged completion line (last line) of a response.
pub fn completion(lines: &[String]) -> &str {
    lines.last().map(String::as_str).unwrap_or("")
}

/// Asserts the tagged completion is `OK`.
pub fn assert_ok(lines: &[String]) {
    let last = completion(lines);
    assert!(
        last.contains(" OK "),
        "expected OK, got: {last} (full: {lines:?})"
    );
}

/// Asserts the tagged completion is `NO`.
pub fn assert_no(lines: &[String]) {
    let last = completion(lines);
    assert!(
        last.contains(" NO "),
        "expected NO, got: {last} (full: {lines:?})"
    );
}

pub mod danger {
    use std::sync::Arc;

    use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
    use rustls::crypto::ring::default_provider;
    use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
    use rustls::{ClientConfig, DigitallySignedStruct, Error, SignatureScheme};

    /// A client config that trusts any certificate (test self-signed only).
    pub fn client_config() -> ClientConfig {
        ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(NoVerify))
            .with_no_client_auth()
    }

    #[derive(Debug)]
    struct NoVerify;

    impl ServerCertVerifier for NoVerify {
        fn verify_server_cert(
            &self,
            _end_entity: &CertificateDer<'_>,
            _intermediates: &[CertificateDer<'_>],
            _server_name: &ServerName<'_>,
            _ocsp_response: &[u8],
            _now: UnixTime,
        ) -> Result<ServerCertVerified, Error> {
            Ok(ServerCertVerified::assertion())
        }
        fn verify_tls12_signature(
            &self,
            _message: &[u8],
            _cert: &CertificateDer<'_>,
            _dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, Error> {
            Ok(HandshakeSignatureValid::assertion())
        }
        fn verify_tls13_signature(
            &self,
            _message: &[u8],
            _cert: &CertificateDer<'_>,
            _dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, Error> {
            Ok(HandshakeSignatureValid::assertion())
        }
        fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
            default_provider()
                .signature_verification_algorithms
                .supported_schemes()
        }
    }
}
