//! TLS setup for STARTTLS (RFC 3207) and implicit TLS submission.
//!
//! Uses rustls with the ring provider — pure Rust, no OpenSSL. In
//! production a PEM certificate and key are loaded from disk; for
//! development and tests a self-signed certificate is generated in
//! memory so the server is TLS-capable with zero configuration.

use std::path::Path;
use std::sync::Arc;

use rustls::ServerConfig;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio_rustls::TlsAcceptor;

use crate::error::SmtpError;

/// Builds a rustls acceptor, loading `cert_path`/`key_path` when both
/// are given, otherwise generating a self-signed certificate for
/// `hostname` (dev/test only — logged loudly).
///
/// # Errors
/// [`SmtpError::Tls`] when the PEM files cannot be read/parsed or the
/// certificate cannot be built.
pub fn build_acceptor(
    cert_path: Option<&Path>,
    key_path: Option<&Path>,
    hostname: &str,
    allow_self_signed: bool,
) -> Result<TlsAcceptor, SmtpError> {
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
            return Err(SmtpError::Tls {
                message: "no certificate configured and self-signed generation not permitted"
                    .to_owned(),
            });
        }
    };

    // Explicit provider avoids depending on a process-wide default
    // being installed (which would be a global-state race).
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|error| SmtpError::Tls {
            message: format!("selecting TLS protocol versions failed: {error}"),
        })?
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|error| SmtpError::Tls {
            message: format!("loading certificate/key failed: {error}"),
        })?;

    Ok(TlsAcceptor::from(Arc::new(config)))
}

fn load_pem(
    cert_path: &Path,
    key_path: &Path,
) -> Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>), SmtpError> {
    let tls_err = |message: String| SmtpError::Tls { message };

    // PEM parsing via rustls-pki-types (rustls-pemfile is unmaintained,
    // RUSTSEC-2025-0134; its API was folded into pki-types).
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
) -> Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>), SmtpError> {
    let sans = vec![hostname.to_owned(), "localhost".to_owned()];
    let key = rcgen::generate_simple_self_signed(sans).map_err(|error| SmtpError::Tls {
        message: format!("generating self-signed certificate failed: {error}"),
    })?;
    let cert_der = key.cert.der().clone();
    let key_der = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key.key_pair.serialize_der()));
    Ok((vec![cert_der], key_der))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn self_signed_acceptor_builds() {
        // With self-signed permitted, a generated cert yields a usable
        // acceptor with no config.
        let acceptor = build_acceptor(None, None, "mx.alo.test", true).unwrap();
        // Cheap smoke: the Arc'd config is present and cloneable.
        let _clone = acceptor.clone();
    }

    #[test]
    fn self_signed_refused_when_not_permitted() {
        // Production posture: no cert + no opt-in ⇒ hard error.
        match build_acceptor(None, None, "mx.alo.test", false) {
            Err(SmtpError::Tls { .. }) => {}
            Err(other) => panic!("expected Tls error, got {other:?}"),
            Ok(_) => panic!("self-signed must be refused without the opt-in"),
        }
    }

    #[test]
    fn missing_pem_files_error_cleanly() {
        // `TlsAcceptor` is not `Debug`, so match rather than unwrap_err.
        match build_acceptor(
            Some(Path::new("/nonexistent/cert.pem")),
            Some(Path::new("/nonexistent/key.pem")),
            "mx.alo.test",
            true,
        ) {
            Err(SmtpError::Tls { .. }) => {}
            Err(other) => panic!("expected Tls error, got {other:?}"),
            Ok(_) => panic!("expected an error for missing PEM files"),
        }
    }
}
