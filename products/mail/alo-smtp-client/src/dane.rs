//! DANE certificate matching for outbound TLS (RFC 6698 + RFC 7672).
//!
//! The queue looks up `_25._tcp.<mx-host>` TLSA over a
//! DNSSEC-validating resolver; when a **secure, usable** record set
//! exists, TLS stops being opportunistic: STARTTLS is mandatory and the
//! presented end-entity certificate must match one TLSA record. This
//! module owns the matching — the [`TlsaRecord`] model, the DANE-EE
//! rustls verifier, and the connector that installs it.
//!
//! Scope (recorded in the design doc): DANE-EE(3) only. PKIX-TA(0) and
//! PKIX-EE(1) are prohibited for SMTP by RFC 7672 §3.1.3; DANE-TA(2)
//! needs chain building and is a follow-up — a record set with only
//! such usages is *unusable*, which per RFC 7672 §2.2 still makes TLS
//! mandatory (unauthenticated). Per §3.1.1, name checks and expiry are
//! deliberately not applied under DANE-EE: the DNSSEC-secured TLSA
//! binding replaces them.

use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, SignatureScheme};
use sha2::{Digest, Sha256, Sha512};
use tokio_rustls::TlsConnector;

/// TLSA certificate usage: DANE-EE (RFC 6698 §2.1.1, value 3) — the
/// only usage this client authenticates.
pub const USAGE_DANE_EE: u8 = 3;
/// TLSA certificate usage DANE-TA (value 2) — recognised but not yet
/// authenticated (unusable; forces encrypted-but-unauthenticated TLS).
pub const USAGE_DANE_TA: u8 = 2;

/// One TLSA record (RFC 6698 §2.1), as fetched — securely — by the
/// caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TlsaRecord {
    /// Certificate usage (0–3); only DANE-EE(3) is authenticated here.
    pub usage: u8,
    /// Selector: 0 = full certificate, 1 = SubjectPublicKeyInfo.
    pub selector: u8,
    /// Matching type: 0 = exact, 1 = SHA-256, 2 = SHA-512.
    pub matching: u8,
    /// The certificate association data to match against.
    pub data: Vec<u8>,
}

impl TlsaRecord {
    /// Whether this client can *authenticate* with the record:
    /// DANE-EE(3) with a selector and matching type we implement.
    pub fn is_usable(&self) -> bool {
        self.usage == USAGE_DANE_EE && self.selector <= 1 && self.matching <= 2
    }

    /// Whether the presented end-entity certificate satisfies this
    /// record (RFC 6698 §2.1). Unusable records never match.
    pub fn matches_leaf(&self, leaf_der: &[u8]) -> bool {
        if !self.is_usable() {
            return false;
        }
        let subject: &[u8] = match self.selector {
            0 => leaf_der,
            1 => match extract_spki(leaf_der) {
                Some(spki) => spki,
                None => return false, // unparseable cert cannot authenticate
            },
            _ => return false,
        };
        match self.matching {
            0 => subject == self.data.as_slice(),
            1 => Sha256::digest(subject).as_slice() == self.data.as_slice(),
            2 => Sha512::digest(subject).as_slice() == self.data.as_slice(),
            _ => false,
        }
    }
}

/// Whether any record in the set matches the presented leaf.
pub fn leaf_matches_any(records: &[TlsaRecord], leaf_der: &[u8]) -> bool {
    records.iter().any(|record| record.matches_leaf(leaf_der))
}

/// Extracts the SubjectPublicKeyInfo TLV (header included — that is
/// what selector 1 hashes, RFC 6698 §2.1.2) from a DER certificate.
///
/// Minimal, bounds-checked walking of the fixed X.509 prefix
/// (RFC 5280 §4.1): `Certificate → tbsCertificate → [0] version?,
/// serialNumber, signature, issuer, validity, subject, SPKI`. The
/// bytes come from an untrusted TLS peer — any deviation returns
/// `None` (which fails the match), never panics.
fn extract_spki(cert_der: &[u8]) -> Option<&[u8]> {
    let (cert_body, _) = der_tlv(cert_der, 0x30)?;
    let (tbs, _) = der_tlv(cert_body, 0x30)?;
    let mut rest = tbs;
    // Explicit [0] version — optional (absent in v1 certificates).
    if rest.first() == Some(&0xA0) {
        rest = der_skip(rest, 0xA0)?;
    }
    rest = der_skip(rest, 0x02)?; // serialNumber
    rest = der_skip(rest, 0x30)?; // signature AlgorithmIdentifier
    rest = der_skip(rest, 0x30)?; // issuer Name
    rest = der_skip(rest, 0x30)?; // validity
    rest = der_skip(rest, 0x30)?; // subject Name
    let (_, consumed) = der_tlv(rest, 0x30)?; // subjectPublicKeyInfo
    rest.get(..consumed)
}

/// Reads one DER TLV of `tag` at the start of `input`, returning
/// (value, bytes_consumed). Short and long-form lengths; rejects
/// truncation and indefinite lengths. (Same shape as the DKIM SPKI
/// walker in `alo-auth-mail` — both parse hostile DER, neither wants
/// a full ASN.1 dependency for a fixed structure.)
fn der_tlv(input: &[u8], tag: u8) -> Option<(&[u8], usize)> {
    if input.first()? != &tag {
        return None;
    }
    let len_byte = *input.get(1)?;
    let (len, header) = if len_byte & 0x80 == 0 {
        (len_byte as usize, 2)
    } else {
        let n = (len_byte & 0x7f) as usize;
        if n == 0 || n > 4 {
            return None;
        }
        let mut len = 0usize;
        for i in 0..n {
            len = (len << 8) | *input.get(2 + i)? as usize;
        }
        (len, 2 + n)
    };
    let end = header.checked_add(len)?;
    let value = input.get(header..end)?;
    Some((value, end))
}

/// Skips one DER TLV of `tag`, returning the bytes after it.
fn der_skip(input: &[u8], tag: u8) -> Option<&[u8]> {
    let (_, consumed) = der_tlv(input, tag)?;
    input.get(consumed..)
}

/// A rustls verifier implementing DANE-EE (RFC 7672 §3.1.1): the
/// end-entity certificate must match one of the (DNSSEC-secured) TLSA
/// records; PKIX chains, names, and validity periods are not consulted
/// — the TLSA binding is the trust anchor. Handshake signatures are
/// still verified normally, so possession of the matching key is
/// proven, not just asserted.
#[derive(Debug)]
struct DaneEeVerifier {
    records: Vec<TlsaRecord>,
    provider: Arc<rustls::crypto::CryptoProvider>,
}

impl ServerCertVerifier for DaneEeVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        if leaf_matches_any(&self.records, end_entity.as_ref()) {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General(
                "DANE: server certificate matches no TLSA record".to_owned(),
            ))
        }
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
            &self.provider.signature_verification_algorithms,
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
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Builds a per-connection TLS connector that authenticates the peer
/// against `records` (DANE-EE). `None` if the TLS provider failed to
/// initialise — the caller treats that as a transient failure.
pub(crate) fn dane_connector(records: Vec<TlsaRecord>) -> Option<TlsConnector> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .ok()?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(DaneEeVerifier { records, provider }))
        .with_no_client_auth();
    Some(TlsConnector::from(Arc::new(config)))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn test_cert() -> (Vec<u8>, Vec<u8>) {
        let cert = rcgen::generate_simple_self_signed(vec!["mx.test".to_owned()]).unwrap();
        (
            cert.cert.der().to_vec(),
            cert.key_pair.public_key_der(), // SPKI DER, straight from rcgen
        )
    }

    fn rec(usage: u8, selector: u8, matching: u8, data: Vec<u8>) -> TlsaRecord {
        TlsaRecord {
            usage,
            selector,
            matching,
            data,
        }
    }

    #[test]
    fn spki_extraction_matches_rcgen() {
        let (cert, spki) = test_cert();
        assert_eq!(extract_spki(&cert), Some(spki.as_slice()));
        assert_eq!(extract_spki(b"not a certificate"), None);
        assert_eq!(extract_spki(&[]), None);
    }

    #[test]
    fn dane_ee_matching_all_selector_matching_combinations() {
        let (cert, spki) = test_cert();
        // 3 0 0 / 3 0 1 / 3 0 2 — full certificate.
        assert!(rec(3, 0, 0, cert.clone()).matches_leaf(&cert));
        assert!(rec(3, 0, 1, Sha256::digest(&cert).to_vec()).matches_leaf(&cert));
        assert!(rec(3, 0, 2, Sha512::digest(&cert).to_vec()).matches_leaf(&cert));
        // 3 1 0 / 3 1 1 / 3 1 2 — SPKI (the form seen in the wild).
        assert!(rec(3, 1, 0, spki.clone()).matches_leaf(&cert));
        assert!(rec(3, 1, 1, Sha256::digest(&spki).to_vec()).matches_leaf(&cert));
        assert!(rec(3, 1, 2, Sha512::digest(&spki).to_vec()).matches_leaf(&cert));
    }

    #[test]
    fn wrong_hash_and_wrong_cert_do_not_match() {
        let (cert, _) = test_cert();
        let (other_cert, other_spki) = test_cert();
        assert!(!rec(3, 1, 1, Sha256::digest(&other_spki).to_vec()).matches_leaf(&cert));
        assert!(!rec(3, 0, 1, Sha256::digest(&other_cert).to_vec()).matches_leaf(&cert));
        assert!(!rec(3, 1, 1, vec![0u8; 32]).matches_leaf(&cert));
    }

    #[test]
    fn non_ee_usages_are_unusable_and_never_match() {
        let (cert, spki) = test_cert();
        for usage in [0u8, 1, 2] {
            let record = rec(usage, 1, 1, Sha256::digest(&spki).to_vec());
            assert!(!record.is_usable());
            assert!(!record.matches_leaf(&cert), "usage {usage} must not match");
        }
        // Unknown selector / matching types are unusable, not panics.
        assert!(!rec(3, 2, 1, vec![]).is_usable());
        assert!(!rec(3, 1, 3, vec![]).is_usable());
    }

    #[test]
    fn set_matching_takes_any_hit() {
        let (cert, spki) = test_cert();
        let records = vec![
            rec(3, 1, 1, vec![0u8; 32]),                  // miss
            rec(3, 1, 1, Sha256::digest(&spki).to_vec()), // hit
        ];
        assert!(leaf_matches_any(&records, &cert));
        assert!(!leaf_matches_any(&records[..1], &cert));
        assert!(!leaf_matches_any(&[], &cert));
    }
}
