//! Secret handling primitives: a redacting `Secret` newtype, the CSPRNG
//! token generator, the at-rest hash, and the one constant-time
//! comparison used everywhere a secret (or its hash) is checked.
//!
//! There is exactly one comparison primitive in this crate
//! ([`ct_eq`]); no secret is ever compared with `==`. Tokens and codes
//! are compared only via their SHA-256 hash, and that hash comparison is
//! constant-time.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ring::rand::{SecureRandom, SystemRandom};
use sha2::{Digest, Sha256};
use zeroize::Zeroize;

/// A low-level cryptographic failure (CSPRNG unavailable, or an argon2
/// parameter/hashing fault). Carries no detail — there is nothing safe to
/// say beyond "it failed".
#[derive(Debug)]
pub struct CryptoError;

impl std::fmt::Display for CryptoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("cryptographic operation failed")
    }
}

impl std::error::Error for CryptoError {}

/// A secret string (a token, a recovery code, a password) whose `Debug`
/// never reveals its contents and whose buffer is wiped on drop. It exists
/// so a secret cannot leak into a log line, an error, or a panic message
/// by accident — printing one shows `Secret(…)`.
#[derive(Clone)]
pub struct Secret(String);

impl Secret {
    /// Wraps a value as a secret.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The secret's bytes, for hashing or transmission to its owner. Named
    /// `reveal` so every call site is greppable.
    pub fn reveal(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(…)")
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl From<String> for Secret {
    fn from(value: String) -> Self {
        Self(value)
    }
}

/// Generates a fresh 32-byte (256-bit) cryptographically-random token,
/// URL-safe base64. A weak token is a security hole, so an RNG failure is
/// a hard error, never a predictable fallback.
///
/// # Errors
/// [`CryptoError`] if the system CSPRNG is unavailable.
pub fn random_token() -> Result<Secret, CryptoError> {
    let mut bytes = [0u8; 32];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| CryptoError)?;
    let token = URL_SAFE_NO_PAD.encode(bytes);
    bytes.zeroize();
    Ok(Secret::new(token))
}

/// Fills `buf` with cryptographically-random bytes.
///
/// # Errors
/// [`CryptoError`] if the system CSPRNG is unavailable.
pub fn random_bytes(buf: &mut [u8]) -> Result<(), CryptoError> {
    SystemRandom::new().fill(buf).map_err(|_| CryptoError)
}

/// The at-rest hash of a token or recovery code: hex-encoded SHA-256.
/// Opaque tokens are stored and looked up only as this hash — the store
/// never holds the token itself, and reversing the hash is infeasible.
pub fn hash_at_rest(secret: &str) -> String {
    let digest = Sha256::digest(secret.as_bytes());
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        // Infallible: writing to a String never errors.
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Constant-time equality of two byte slices — the only equality check
/// applied to a secret or a secret's hash in this crate. Unequal lengths
/// return `false` (length is not a secret); equal lengths compare without a
/// data-dependent early exit, via the purpose-built `subtle` crate.
pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    use subtle::ConstantTimeEq;
    a.ct_eq(b).into()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn debug_redacts() {
        let s = Secret::new("super-secret-token");
        assert_eq!(format!("{s:?}"), "Secret(…)");
        assert!(!format!("{s:?}").contains("super-secret"));
    }

    #[test]
    fn hash_is_stable_and_hex() {
        let h = hash_at_rest("abc");
        // SHA-256("abc")
        assert_eq!(
            h,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn ct_eq_matches_semantics() {
        assert!(ct_eq(b"same", b"same"));
        assert!(!ct_eq(b"same", b"diff"));
        assert!(!ct_eq(b"short", b"longer"));
    }

    #[test]
    fn tokens_are_unique_and_urlsafe() {
        let a = random_token().unwrap();
        let b = random_token().unwrap();
        assert_ne!(a.reveal(), b.reveal());
        assert_eq!(a.reveal().len(), 43); // 32 bytes → 43 base64url chars
        assert!(
            a.reveal()
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        );
    }
}
