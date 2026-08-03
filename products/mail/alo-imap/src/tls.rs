//! TLS setup for implicit TLS (993/995) and STARTTLS (143).
//!
//! Deliberately mirrors `alo-smtp`'s `tls.rs` rather than sharing a
//! crate (see `docs/design/imap-pop3-shims.md` — the shared-transport
//! question). rustls with the ring provider, pure Rust. A PEM cert/key is
//! loaded from disk in production; a self-signed cert is generated for
//! dev/test when permitted.

use std::path::Path;
use std::sync::Arc;

use rustls::ServerConfig;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio_rustls::TlsAcceptor;

use crate::error::ImapError;

/// Builds a rustls acceptor, loading `cert_path`/`key_path` when both are
/// given, else generating a self-signed certificate for `hostname` (dev/
/// test only — logged loudly) when `allow_self_signed`.
///
/// # Errors
/// [`ImapError::Tls`] when the PEM files cannot be read/parsed, the
/// certificate cannot be built, or self-signed is needed but not allowed.
pub fn build_acceptor(
    cert_path: Option<&Path>,
    key_path: Option<&Path>,
    hostname: &str,
    allow_self_signed: bool,
) -> Result<TlsAcceptor, ImapError> {
    let (certs, key) = match (cert_path, key_path) {
        (Some(cert), Some(key)) => load_pem(cert, key)?,
        _ if allow_self_signed => {
            tracing::warn!(
                %hostname,
                "no TLS certificate configured; generating a self-signed cert (development only)"
            );
            generate_self_signed(hostname)?
        }
        _ => {
            return Err(ImapError::Tls {
                message: "no certificate configured and self-signed generation not permitted"
                    .to_owned(),
            });
        }
    };

    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|error| ImapError::Tls {
            message: format!("selecting TLS protocol versions failed: {error}"),
        })?
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|error| ImapError::Tls {
            message: format!("loading certificate/key failed: {error}"),
        })?;

    Ok(TlsAcceptor::from(Arc::new(config)))
}

fn load_pem(
    cert_path: &Path,
    key_path: &Path,
) -> Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>), ImapError> {
    let tls_err = |message: String| ImapError::Tls { message };

    let certs = CertificateDer::pem_file_iter(cert_path)
        .map_err(|e| {
            tls_err(format!(
                "reading certificates in {}: {e}",
                cert_path.display()
            ))
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| {
            tls_err(format!(
                "parsing certificates in {}: {e}",
                cert_path.display()
            ))
        })?;
    if certs.is_empty() {
        return Err(tls_err(format!(
            "no certificates found in {}",
            cert_path.display()
        )));
    }

    let key = PrivateKeyDer::from_pem_file(key_path)
        .map_err(|e| tls_err(format!("parsing key in {}: {e}", key_path.display())))?;

    Ok((certs, key))
}

fn generate_self_signed(
    hostname: &str,
) -> Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>), ImapError> {
    let sans = vec![hostname.to_owned(), "localhost".to_owned()];
    let key = rcgen::generate_simple_self_signed(sans).map_err(|error| ImapError::Tls {
        message: format!("generating self-signed certificate failed: {error}"),
    })?;
    let cert_der = key.cert.der().clone();
    let key_der = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key.key_pair.serialize_der()));
    Ok((vec![cert_der], key_der))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn self_signed_acceptor_builds() {
        let acceptor = build_acceptor(None, None, "imap.alo.test", true).unwrap();
        let _clone = acceptor.clone();
    }

    #[test]
    fn self_signed_refused_when_not_permitted() {
        match build_acceptor(None, None, "imap.alo.test", false) {
            Err(ImapError::Tls { .. }) => {}
            Err(other) => panic!("expected Tls error, got {other:?}"),
            Ok(_) => panic!("self-signed must be refused without the opt-in"),
        }
    }
}
