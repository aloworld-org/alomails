//! Opportunistic STARTTLS for outbound delivery (RFC 3207).
//!
//! Delivery to another MTA is *opportunistic*: if the peer advertises
//! `STARTTLS` we upgrade and deliver encrypted; the server certificate is not
//! verified, because MX certificates are routinely self-signed or name-
//! mismatched and the only alternative would be cleartext. Strict verification
//! (MTA-STS / DANE) is layered on top separately; this module provides the
//! encrypted channel that those policies require to exist at all.

use std::io;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};
use std::task::{Context, Poll};

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, SignatureScheme};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;

/// One outbound transport: plaintext until STARTTLS upgrades it, then TLS.
/// `Taken` is a transient placeholder used only while swapping the plaintext
/// socket out for its TLS-wrapped self during the upgrade.
pub enum MaybeTls {
    /// Cleartext TCP.
    Plain(TcpStream),
    /// TLS-wrapped after a successful STARTTLS upgrade.
    Tls(Box<TlsStream<TcpStream>>),
    /// Momentary hole during the upgrade swap — never read or written.
    Taken,
}

fn taken() -> io::Error {
    io::Error::new(io::ErrorKind::BrokenPipe, "stream taken during TLS upgrade")
}

impl AsyncRead for MaybeTls {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut() {
            MaybeTls::Plain(s) => Pin::new(s).poll_read(cx, buf),
            MaybeTls::Tls(s) => Pin::new(s.as_mut()).poll_read(cx, buf),
            MaybeTls::Taken => Poll::Ready(Err(taken())),
        }
    }
}

impl AsyncWrite for MaybeTls {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            MaybeTls::Plain(s) => Pin::new(s).poll_write(cx, buf),
            MaybeTls::Tls(s) => Pin::new(s.as_mut()).poll_write(cx, buf),
            MaybeTls::Taken => Poll::Ready(Err(taken())),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            MaybeTls::Plain(s) => Pin::new(s).poll_flush(cx),
            MaybeTls::Tls(s) => Pin::new(s.as_mut()).poll_flush(cx),
            MaybeTls::Taken => Poll::Ready(Err(taken())),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            MaybeTls::Plain(s) => Pin::new(s).poll_shutdown(cx),
            MaybeTls::Tls(s) => Pin::new(s.as_mut()).poll_shutdown(cx),
            MaybeTls::Taken => Poll::Ready(Err(taken())),
        }
    }
}

/// A rustls verifier that accepts any server certificate. Sound for
/// *opportunistic* delivery TLS only — see the module docs.
#[derive(Debug)]
struct AcceptAnyServerCert(Arc<rustls::crypto::CryptoProvider>);

impl ServerCertVerifier for AcceptAnyServerCert {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
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
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

fn build_connector() -> Result<TlsConnector, rustls::Error> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AcceptAnyServerCert(provider)))
        .with_no_client_auth();
    Ok(TlsConnector::from(Arc::new(config)))
}

/// The process-wide opportunistic TLS connector, built once. `None` only if the
/// TLS provider itself failed to initialise (treated as a transient delivery
/// failure by the caller, never a panic).
pub fn connector() -> Option<TlsConnector> {
    static CONNECTOR: OnceLock<Option<TlsConnector>> = OnceLock::new();
    CONNECTOR.get_or_init(|| build_connector().ok()).clone()
}
