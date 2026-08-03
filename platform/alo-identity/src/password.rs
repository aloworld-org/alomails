//! argon2id password hashing and verification, with the anti-enumeration
//! **dummy verify**: an unknown user still pays one argon2 verification so
//! that *wrong password* and *no such user* are indistinguishable in time.
//! This closes the timing oracle the M3 TLS audit pinned to this milestone
//! (`docs/design/tls-and-submission.md`).

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};

use crate::config::IdentityConfig;
use crate::secret::CryptoError;

/// The configured argon2id hasher. Verification uses the parameters
/// embedded in each stored PHC string (so old hashes keep verifying);
/// hashing (real or dummy) uses these configured parameters.
pub struct Passwords {
    argon: Argon2<'static>,
}

impl Passwords {
    /// Builds a hasher from the configured argon2id parameters.
    ///
    /// # Errors
    /// [`CryptoError`] if the parameters are out of argon2's valid range.
    pub fn new(cfg: &IdentityConfig) -> Result<Self, CryptoError> {
        let params = Params::new(cfg.argon2_m_kib, cfg.argon2_t, cfg.argon2_p, None)
            .map_err(|_| CryptoError)?;
        Ok(Self {
            argon: Argon2::new(Algorithm::Argon2id, Version::V0x13, params),
        })
    }

    /// Hashes a password to a self-describing argon2id PHC string.
    ///
    /// # Errors
    /// [`CryptoError`] on a hashing failure (e.g. RNG unavailable).
    pub fn hash(&self, password: &str) -> Result<String, CryptoError> {
        let salt = SaltString::generate(&mut OsRng);
        self.argon
            .hash_password(password.as_bytes(), &salt)
            .map(|h| h.to_string())
            .map_err(|_| CryptoError)
    }

    /// Verifies a password against a stored PHC hash. Constant-time over the
    /// derived key (argon2's own comparison). `false` on any mismatch or a
    /// malformed stored hash.
    pub fn verify(&self, password: &str, phc: &str) -> bool {
        match PasswordHash::new(phc) {
            Ok(parsed) => Argon2::default()
                .verify_password(password.as_bytes(), &parsed)
                .is_ok(),
            Err(_) => false,
        }
    }

    /// Verifies a password when the user **may not exist**. If `stored` is
    /// `Some`, verifies normally. If `None` (unknown user), still performs
    /// one argon2 hash of the presented password and discards it, so the
    /// unknown-user path costs the same as a wrong-password path — no
    /// user-existence oracle in time. Always returns `false` for `None`.
    pub fn verify_or_dummy(&self, password: &str, stored: Option<&str>) -> bool {
        match stored {
            Some(phc) => self.verify(password, phc),
            None => {
                // Burn an equivalent argon2 cost. The result is discarded;
                // the point is the wall-clock cost, not the value.
                let _ = self.hash(password);
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn fast_passwords() -> Passwords {
        // Minimum argon2 params keep the unit test fast; the security
        // property under test (round-trip + rejection) is param-independent.
        let mut cfg = IdentityConfig::new("https://id.test");
        cfg.argon2_m_kib = 8;
        cfg.argon2_t = 1;
        cfg.argon2_p = 1;
        Passwords::new(&cfg).unwrap()
    }

    #[test]
    fn hash_verifies_and_rejects() {
        let p = fast_passwords();
        let h = p.hash("correct horse battery staple").unwrap();
        assert!(p.verify("correct horse battery staple", &h));
        assert!(!p.verify("wrong password", &h));
        assert!(!h.contains("correct horse")); // hash is not the plaintext
    }

    #[test]
    fn dummy_path_returns_false_and_runs() {
        let p = fast_passwords();
        assert!(!p.verify_or_dummy("anything", None));
    }

    #[test]
    fn malformed_hash_is_rejected_not_panicked() {
        let p = fast_passwords();
        assert!(!p.verify("x", "not-a-phc-string"));
    }
}
