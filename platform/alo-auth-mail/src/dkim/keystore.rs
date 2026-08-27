//! DKIM signing-key management, addressed by `(domain, selector)` so
//! rotation is a first-class operation (publish a new selector, sign
//! with it, retire the old one — no code change).
//!
//! The [`KeyStore`] trait is the seam: [`FileKeyStore`] loads PEM keys
//! from disk today; a vault-backed store (per the product doc's
//! secrets rule) drops in later without touching the signer. Private
//! key material is permission-checked at load, never logged, and held
//! in zeroizing buffers.

use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use zeroize::Zeroizing;

/// The signing algorithm for a key (RFC 6376 / RFC 8463).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAlgorithm {
    /// `rsa-sha256`.
    RsaSha256,
    /// `ed25519-sha256` (RFC 8463).
    Ed25519Sha256,
}

impl KeyAlgorithm {
    /// The `a=` tag value used in the DKIM-Signature.
    pub fn tag(self) -> &'static str {
        match self {
            Self::RsaSha256 => "rsa-sha256",
            Self::Ed25519Sha256 => "ed25519-sha256",
        }
    }
}

/// A signing key: its algorithm and PKCS#8 DER bytes (zeroized on
/// drop). The DER is kept, not a parsed key, so the buffer is the only
/// long-lived copy of the secret and is wiped deterministically.
pub struct SigningKey {
    /// Algorithm this key signs with.
    pub algorithm: KeyAlgorithm,
    /// PKCS#8 DER-encoded private key.
    pub pkcs8_der: Zeroizing<Vec<u8>>,
}

impl std::fmt::Debug for SigningKey {
    /// Never print key material.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SigningKey")
            .field("algorithm", &self.algorithm)
            .field("pkcs8_der", &"<redacted>")
            .finish()
    }
}

/// Why a key could not be provided.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KeyStoreError {
    /// No key for the requested (domain, selector).
    #[error("no signing key for {selector}._domainkey.{domain}")]
    NotFound {
        /// Requested domain.
        domain: String,
        /// Requested selector.
        selector: String,
    },
    /// The key exists but could not be loaded (I/O, perms, parse). The
    /// message never contains key bytes.
    #[error("key for {selector}._domainkey.{domain} unusable: {reason}")]
    Unusable {
        /// Requested domain.
        domain: String,
        /// Requested selector.
        selector: String,
        /// Non-sensitive reason.
        reason: String,
    },
}

/// Boxed future for the object-safe async `get`.
pub type KeyFuture<'a> =
    Pin<Box<dyn Future<Output = Result<SigningKey, KeyStoreError>> + Send + 'a>>;

/// Looks up a signing key by `(domain, selector)`.
pub trait KeyStore: Send + Sync {
    /// Returns the signing key for the pair, or an error.
    fn get<'a>(&'a self, domain: &'a str, selector: &'a str) -> KeyFuture<'a>;
}

/// A key file registered in a [`FileKeyStore`].
#[derive(Debug, Clone)]
pub struct KeyFile {
    /// Path to the PKCS#8 PEM private key.
    pub path: PathBuf,
    /// The algorithm the key is for.
    pub algorithm: KeyAlgorithm,
}

/// Loads DKIM signing keys from PEM files on disk, keyed by
/// `(domain, selector)`. Paths are always configured explicitly —
/// never defaulted into the repo tree.
#[derive(Default)]
pub struct FileKeyStore {
    keys: HashMap<(String, String), KeyFile>,
}

impl FileKeyStore {
    /// Creates an empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a PEM key file for a `(domain, selector)`.
    #[must_use]
    pub fn with_key(
        mut self,
        domain: &str,
        selector: &str,
        path: impl Into<PathBuf>,
        algorithm: KeyAlgorithm,
    ) -> Self {
        self.keys.insert(
            (domain.to_ascii_lowercase(), selector.to_owned()),
            KeyFile {
                path: path.into(),
                algorithm,
            },
        );
        self
    }
}

impl KeyStore for FileKeyStore {
    fn get<'a>(&'a self, domain: &'a str, selector: &'a str) -> KeyFuture<'a> {
        let entry = self
            .keys
            .get(&(domain.to_ascii_lowercase(), selector.to_owned()))
            .cloned();
        Box::pin(async move {
            let key = entry.ok_or_else(|| KeyStoreError::NotFound {
                domain: domain.to_owned(),
                selector: selector.to_owned(),
            })?;
            load_pkcs8_pem(&key.path)
                .map(|der| SigningKey {
                    algorithm: key.algorithm,
                    pkcs8_der: der,
                })
                .map_err(|reason| KeyStoreError::Unusable {
                    domain: domain.to_owned(),
                    selector: selector.to_owned(),
                    reason,
                })
        })
    }
}

/// A freshly generated Ed25519 DKIM key (RFC 8463): a DNS-safe selector, the
/// 32-byte seed to store (private, zeroized), and the 32-byte raw public key to
/// publish in DNS.
pub struct GeneratedKey {
    /// A DNS-label-safe selector derived from the public key (stable, unique).
    pub selector: String,
    /// The Ed25519 secret seed — persist this, never log it.
    pub seed: Zeroizing<[u8; 32]>,
    /// The raw 32-byte public key, for the `p=` tag of the DNS record.
    pub public_raw: [u8; 32],
}

/// Generates a new Ed25519 DKIM signing key from the system CSPRNG, or `None`
/// on RNG failure. RSA is not generated in-process (the pure-Rust `rsa` crate
/// is forbidden — ADR 0008 / 0014); operators needing RSA supply it via the
/// file keystore. The selector is `fic<12 hex>` derived from the public key —
/// DNS-label-safe and unique per key, so rotation always uses a fresh selector.
pub fn generate_ed25519_key() -> Option<GeneratedKey> {
    use ring::rand::SecureRandom;
    use sha2::{Digest, Sha256};
    let mut seed = Zeroizing::new([0u8; 32]);
    ring::rand::SystemRandom::new().fill(seed.as_mut()).ok()?;
    let signing = ed25519_dalek::SigningKey::from_bytes(&seed);
    let public_raw = signing.verifying_key().to_bytes();
    let digest = Sha256::digest(public_raw);
    let selector = format!(
        "fic{}",
        digest[..6]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
    Some(GeneratedKey {
        selector,
        seed,
        public_raw,
    })
}

/// Rebuilds a [`SigningKey`] (PKCS#8 DER, which the signer consumes) from a
/// stored Ed25519 seed, or `None` if the seed is not 32 bytes or PKCS#8
/// encoding fails.
pub fn ed25519_signing_key_from_seed(seed: &[u8]) -> Option<SigningKey> {
    use ed25519_dalek::pkcs8::EncodePrivateKey;
    let seed: [u8; 32] = seed.try_into().ok()?;
    let signing = ed25519_dalek::SigningKey::from_bytes(&seed);
    let der = signing.to_pkcs8_der().ok()?;
    Some(SigningKey {
        algorithm: KeyAlgorithm::Ed25519Sha256,
        pkcs8_der: Zeroizing::new(der.as_bytes().to_vec()),
    })
}

/// Takes an Ed25519 PKCS#8 DER key apart into `(seed, public_raw)` — what the
/// store persists and what DNS publishes — or `None` when it is not one.
///
/// The inverse of [`ed25519_signing_key_from_seed`], for importing a key
/// somebody else generated. It lives here rather than in the importer because
/// this crate is the one that owns key encodings; a service that had to reach
/// for `ed25519_dalek` itself would be a second place the format is known.
pub fn ed25519_key_from_pkcs8(pkcs8_der: &[u8]) -> Option<(Zeroizing<Vec<u8>>, Vec<u8>)> {
    use ed25519_dalek::pkcs8::DecodePrivateKey;
    let key = ed25519_dalek::SigningKey::from_pkcs8_der(pkcs8_der).ok()?;
    let public = key.verifying_key().to_bytes().to_vec();
    Some((Zeroizing::new(key.to_bytes().to_vec()), public))
}

/// The algorithm a PKCS#8 private key actually signs with — read from the key
/// bytes, or `None` when it is neither RSA nor Ed25519.
///
/// For callers that must pair a key with its `a=` tag from configuration: a
/// declared algorithm the key cannot produce yields a signature that looks
/// fine locally and fails at every receiver, so the key itself is the only
/// authority worth asking.
pub fn algorithm_of_pkcs8(pkcs8_der: &[u8]) -> Option<KeyAlgorithm> {
    if super::rsa_public::spki_from_pkcs8(pkcs8_der).is_some() {
        return Some(KeyAlgorithm::RsaSha256);
    }
    if ed25519_key_from_pkcs8(pkcs8_der).is_some() {
        return Some(KeyAlgorithm::Ed25519Sha256);
    }
    None
}

/// A stored RSA signing key, from the PKCS#8 DER the store holds.
///
/// The counterpart of [`ed25519_signing_key_from_seed`] for a domain that signs
/// with both algorithms. Ed25519 keeps a 32-byte seed and rebuilds the key from
/// it; RSA has no such compact seed, so the stored bytes **are** the PKCS#8 DER
/// and this only validates them.
///
/// **Validated here rather than at signing time, deliberately.** This is the
/// same check `sign_rsa` makes, and making it at load turns an unusable key into
/// a key-store error naming the domain and selector - rather than a signing
/// failure on a message already on its way out.
pub fn rsa_signing_key_from_der(pkcs8_der: &[u8]) -> Option<SigningKey> {
    ring::signature::RsaKeyPair::from_pkcs8(pkcs8_der).ok()?;
    Some(SigningKey {
        algorithm: KeyAlgorithm::RsaSha256,
        pkcs8_der: Zeroizing::new(pkcs8_der.to_vec()),
    })
}

/// The DKIM DNS TXT record value for an Ed25519 public key (RFC 8463):
/// `v=DKIM1; k=ed25519; p=<base64>`.
pub fn ed25519_txt_record(public_raw: &[u8]) -> String {
    use base64::Engine;
    let p = base64::engine::general_purpose::STANDARD.encode(public_raw);
    format!("v=DKIM1; k=ed25519; p={p}")
}

/// The DKIM DNS TXT record value for an RSA public key (RFC 6376 §3.6.1):
/// `v=DKIM1; k=rsa; p=<base64 SubjectPublicKeyInfo>`. `public_raw` is the SPKI
/// DER — see [`super::rsa_public`] for why that encoding and not the PKCS#1 one.
pub fn rsa_txt_record(public_raw: &[u8]) -> String {
    use base64::Engine;
    let p = base64::engine::general_purpose::STANDARD.encode(public_raw);
    format!("v=DKIM1; k=rsa; p={p}")
}

/// The record to publish for a stored key, chosen by the algorithm tag the
/// store holds beside it (`"rsa"` / `"ed25519"`), or `None` for a tag this
/// build cannot render.
///
/// A domain that dual-signs has **one record per algorithm** and they are not
/// interchangeable: publishing an RSA key under `k=ed25519` yields a record
/// every verifier rejects, which reads as a signing bug rather than a DNS one.
/// So the tag is read from the row rather than assumed, and an unknown one
/// returns nothing rather than a guess.
pub fn txt_record_for(algorithm: &str, public_raw: &[u8]) -> Option<String> {
    match algorithm {
        "rsa" => Some(rsa_txt_record(public_raw)),
        "ed25519" => Some(ed25519_txt_record(public_raw)),
        _ => None,
    }
}

/// Committed test-fixture keys, for exercising the RSA signing path in tests:
/// RSA is never generated in-process (the pure-Rust `rsa` crate is forbidden —
/// ADR 0008 / 0014), so suites share this pair instead of each carrying a
/// copy. **Not a secret** — generated solely for tests, published in a public
/// repository, and never valid for any real domain.
#[cfg(any(test, feature = "test-fixtures"))]
pub mod fixture_keys {
    /// RSA-2048 PKCS#8 private key, base64 DER.
    pub const RSA_PKCS8_B64: &str = "MIIEvAIBADANBgkqhkiG9w0BAQEFAASCBKYwggSiAgEAAoIBAQCkPJ+lNnVsCtlXohG6bV1IqZ3LMmQvG3p3rCzmf3BfgyLBeZK1Y7vraGOIwOyUqqa2i2h0s05R3T4khdkqRPPTBpxzEXnHeqMMLfwxZ9pJznAMRdzTP5h0SbnkTqSTrEe9zRk9PIiwLfoFH2FvH0dP1YOcoJodo7nK8Jdip+KTeqMbuyDPlkOwFiXzNBhsijB+fBrioAJjJgZ3UJWggYVoEMqjvARO7nibYJMufh2q8HTb1/lhEKT377djacw07CBuJKep8cj+d685gS+0yO4aFOCNgAuYDIaeEjNs3+BIwtleU67TH9O8WCQQwEDJbmvPQvH6escBstHpH1Rp2RqjAgMBAAECggEAIkDkmPwJNG3004KhSO0LUh7SIrGRVIPaIIB/4tj8cmcyxgBZmUxCQsBEVh7KmN0YpSThNcm50XgfwFVMLDUyVzZfRd6EtBZ/UAh9Oz6qN89+7ghaHJL8dHB1/UhrqcL6OXs/wtZPudL0/MGFPDxdTqi86NtUGU9u7gUMxc43AsPxHGo//f6h0iemPo5o303t8BRY2PQyP1piMCbSZhbucEStNhLP+AoJn1C8vjSS8ahbL7WEwDPNAf2gyLpdFnpb3x7CsDKHXBLjI3G3kBtYKf2FxVpHaHk5Gyv+kDoaG3rWDAz0PFkQCA2e1dVTAMvvpDbin6Rnq84ikt8rM6SDgQKBgQDPVlBtf9gps0qOI4a7so+V29i8XzReXLlz1HbWnE0dNTflUe/vxzq7fxF2RBsgBfIvG/0jWdo2Wk6+J4t1jTRXvYk+ueRjhG50wfjD/q6jGTEzKg7Rp+BLXQnrxbBkiQEVmyeaaFZ+o5aR5JUJqn2ey5x2rtG+RTEZKq1447uvgQKBgQDKyKjYe/9R7RwyLGSoJRqxJcsaw6ReIHbwSnLWcF52gQVP/KnG4bIdmMbKmhHoyQNjcX9V12fHku6QQAOmh109j3j/pJkDGYUXBmuuQAa3RUtUQjALOQutupU7PacZunBZL5+iLp9ozDKc9h9rQru2ztfFQbIhchI4O8Hbnv4cIwKBgAE73EKqjhjBbmImJ/kZ+OzFYCbO0jr1hk0AKKziRTs1Q93jfPAKWXkgKnRvt1Gbd3N8USdSs4+7Pdi314adjoAvKo/q/0bwVM/xD4/rBhMGZVqOl4P74cPRC+wRQxl2D0GXqMasdEdQpea8W36xOwok4At8wtbFBqPFsz6S7F0BAoGAd73zjhnyU0NLMglqyqtWTqK9gylhpr9O3Gdp1lx2O03GgEv3SNw/HCD5yesehjIkkRUVFvBidMO7oWxbe3tVQKO21GYygFUSdN0yuqtOC+ycJb4LtqTR051ov5mRUaz46IEpp1AWi4CAppTjSqgWRkVvvigj8oH0ZkZLG8+Px7cCgYBjRbw4h3DZp4K+WmVzMaK5YWGaAK/CMsHcAsiYl2vmiIOaJfe1IJjroGFxWZQ6y7xTDm2cAVwH+D904goMvFpKbIFBfLg2zRT0X6nJDXm+lSpk/ekjmXmmIOZfD1fa0UVZ1oDozyCAZ5EkZXXfYqAWV1jpzB8BRmVzhUNaA/vB+Q==";
    /// The matching SubjectPublicKeyInfo public key, base64 DER — the `p=`
    /// of the DNS record that verifies what the private half signs.
    pub const RSA_SPKI_B64: &str = "MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEApDyfpTZ1bArZV6IRum1dSKmdyzJkLxt6d6ws5n9wX4MiwXmStWO762hjiMDslKqmtotodLNOUd0+JIXZKkTz0waccxF5x3qjDC38MWfaSc5wDEXc0z+YdEm55E6kk6xHvc0ZPTyIsC36BR9hbx9HT9WDnKCaHaO5yvCXYqfik3qjG7sgz5ZDsBYl8zQYbIowfnwa4qACYyYGd1CVoIGFaBDKo7wETu54m2CTLn4dqvB029f5YRCk9++3Y2nMNOwgbiSnqfHI/nevOYEvtMjuGhTgjYALmAyGnhIzbN/gSMLZXlOu0x/TvFgkEMBAyW5rz0Lx+nrHAbLR6R9UadkaowIDAQAB";
}

/// Loads a PKCS#8 PEM private key from disk, refusing group/world-readable
/// files. Returns a non-sensitive error string on failure — the key bytes never
/// appear in it.
///
/// Public so the operator key-import path applies exactly the same refusal as
/// the signer: a key the server would decline to load must not be installable
/// either.
///
/// # Errors
/// A human-readable reason: insecure permissions, unreadable file, or not a
/// PKCS#8 PEM private key.
pub fn load_pkcs8_pem(path: &Path) -> Result<Zeroizing<Vec<u8>>, String> {
    refuse_insecure_permissions(path)?;
    let pem = std::fs::read_to_string(path).map_err(|e| format!("read failed: {e}"))?;
    let der = pem_to_der(&pem).ok_or_else(|| "not a PKCS#8 PEM private key".to_owned())?;
    Ok(Zeroizing::new(der))
}

/// On Unix, refuse a private key that is readable by group or others
/// (mode & 0o077 != 0). On other platforms this is a no-op (dev only).
fn refuse_insecure_permissions(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let meta = std::fs::metadata(path).map_err(|e| format!("stat failed: {e}"))?;
        let mode = meta.permissions().mode();
        if mode & 0o077 != 0 {
            return Err(format!(
                "insecure permissions {:o} (must not be group/world-readable)",
                mode & 0o777
            ));
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

/// Extracts the DER body from a single PEM block. Deliberately minimal
/// (no external PEM crate for a secret path): find the base64 between
/// the BEGIN/END markers and decode it.
fn pem_to_der(pem: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    let begin = pem.find("-----BEGIN PRIVATE KEY-----")?;
    let after = &pem[begin..];
    let body_start = after.find('\n')? + 1;
    let end = after.find("-----END PRIVATE KEY-----")?;
    let b64: String = after[body_start..end]
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    base64::engine::general_purpose::STANDARD.decode(b64).ok()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    #[test]
    fn missing_key_is_not_found() {
        let store = FileKeyStore::new();
        let result = futures_lite_block(store.get("example.com", "sel"));
        assert!(matches!(result, Err(KeyStoreError::NotFound { .. })));
    }

    #[test]
    fn generated_ed25519_key_round_trips_and_signs() {
        let generated = generate_ed25519_key().expect("keygen");
        assert_eq!(generated.public_raw.len(), 32);
        // The seed rebuilds into a usable PKCS#8 signing key...
        let key = ed25519_signing_key_from_seed(generated.seed.as_ref()).expect("from seed");
        assert_eq!(key.algorithm, KeyAlgorithm::Ed25519Sha256);
        // ...and that DER is the same key (its public matches what we publish).
        use ed25519_dalek::pkcs8::DecodePrivateKey;
        let sk = ed25519_dalek::SigningKey::from_pkcs8_der(&key.pkcs8_der).expect("decode der");
        assert_eq!(sk.verifying_key().to_bytes(), generated.public_raw);
        // A wrong-length seed is rejected, not panicked on.
        assert!(ed25519_signing_key_from_seed(&[0u8; 16]).is_none());
    }

    #[test]
    fn ed25519_txt_record_is_well_formed() {
        let rec = ed25519_txt_record(&[0u8; 32]);
        assert!(rec.starts_with("v=DKIM1; k=ed25519; p="));
        assert!(rec.ends_with('='));
    }

    #[test]
    fn a_record_is_rendered_for_the_algorithm_the_key_actually_is() {
        // The two are not interchangeable: the same bytes under the wrong `k=`
        // is a record every verifier rejects.
        let ed = txt_record_for("ed25519", &[7u8; 32]).expect("ed25519 renders");
        let rsa = txt_record_for("rsa", &[7u8; 32]).expect("rsa renders");
        assert!(ed.contains("k=ed25519"));
        assert!(rsa.contains("k=rsa"));
        assert_ne!(ed, rsa);
        // Same key material, so only the tag differs — which is exactly why a
        // caller must not pick the renderer itself.
        assert_eq!(
            ed.replace("k=ed25519", "k=rsa"),
            rsa,
            "the two renderers must differ only in the algorithm tag"
        );
        // A tag this build cannot render is nothing, never a guess.
        assert_eq!(txt_record_for("rsa2048", &[7u8; 32]), None);
        assert_eq!(txt_record_for("", &[7u8; 32]), None);
        assert_eq!(txt_record_for("RSA", &[7u8; 32]), None);
    }

    #[test]
    fn debug_never_prints_key_bytes() {
        let key = SigningKey {
            algorithm: KeyAlgorithm::Ed25519Sha256,
            pkcs8_der: Zeroizing::new(vec![1, 2, 3, 4]),
        };
        let shown = format!("{key:?}");
        assert!(shown.contains("<redacted>"));
        assert!(!shown.contains("1, 2, 3, 4"));
    }

    #[test]
    fn pem_decode_roundtrip() {
        use base64::Engine;
        let der = vec![0x30, 0x2e, 0x02, 0x01, 0x00];
        let b64 = base64::engine::general_purpose::STANDARD.encode(&der);
        let pem = format!("-----BEGIN PRIVATE KEY-----\n{b64}\n-----END PRIVATE KEY-----\n");
        assert_eq!(pem_to_der(&pem), Some(der));
        assert_eq!(pem_to_der("garbage"), None);
    }

    /// Minimal synchronous block-on for these non-async unit tests.
    fn futures_lite_block<F: Future>(fut: F) -> F::Output {
        // The futures here never yield (pure map over an in-memory
        // Option), so a trivial poll loop suffices without a runtime.
        use std::task::{Context, Poll, Waker};
        let mut fut = Box::pin(fut);
        let waker = Waker::noop();
        let mut cx = Context::from_waker(waker);
        loop {
            if let Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
                return v;
            }
        }
    }
}
