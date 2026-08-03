//! TOTP two-factor (RFC 6238 over RFC 4226 HOTP), plus single-use recovery
//! codes. HMAC-SHA1 is via `ring` (the one legitimate legacy-SHA1 use —
//! the TOTP standard mandates it). Codes are compared **constant-time**,
//! and a recovery code is stored only as its SHA-256 hash and consumed
//! atomically (single-use).

use ring::hmac;
use time::OffsetDateTime;

use alo_store::{TenantId, UserId};

use crate::secret::{self, Secret};
use crate::{Identity, IdentityError, Result};

/// TOTP time step (seconds) — the RFC 6238 default.
const STEP_SECS: u64 = 30;
/// Code length — the universal authenticator-app default.
const DIGITS: u32 = 6;
/// Accepted step drift on either side (±1 step ≈ ±30 s) for clock skew.
const DRIFT_STEPS: i64 = 1;
/// Number of recovery codes issued per enrollment.
const RECOVERY_CODE_COUNT: usize = 10;

/// The outcome of a 2FA check during login.
#[derive(Debug, PartialEq, Eq)]
pub enum TotpOutcome {
    /// The user has no enabled TOTP — 2FA is not required.
    NotEnrolled,
    /// A valid TOTP code or recovery code was presented.
    Verified,
    /// TOTP is enabled but the presented code was missing or wrong.
    Failed,
}

/// A fresh TOTP enrollment to show the user once.
pub struct TotpEnrollment {
    /// The shared secret, base32 (for manual entry).
    pub secret_base32: String,
    /// The `otpauth://` provisioning URI (render as a QR in the UI).
    pub provisioning_uri: String,
}

impl Identity {
    /// Begins TOTP enrollment for a user: generates a secret, stores it
    /// **disabled** (it cannot gate login until confirmed), and returns the
    /// provisioning URI + base32 secret to show once.
    ///
    /// # Errors
    /// [`IdentityError::Crypto`] on RNG failure; [`IdentityError::Store`] if
    /// the user is unknown.
    pub async fn enroll_totp(
        &self,
        tenant: &TenantId,
        user: &UserId,
        account_name: &str,
    ) -> Result<TotpEnrollment> {
        let mut secret = [0u8; 20];
        secret::random_bytes(&mut secret).map_err(|_| IdentityError::Crypto)?;
        self.store()
            .for_tenant(tenant.clone())
            .set_totp_secret(user, &secret)
            .await?;
        let base32 = base32_encode(&secret);
        let uri = provisioning_uri(issuer_label(&self.config().issuer), account_name, &base32);
        Ok(TotpEnrollment {
            secret_base32: base32,
            provisioning_uri: uri,
        })
    }

    /// Confirms enrollment: verifies a code against the pending secret and,
    /// on success, **enables** TOTP and issues a fresh set of single-use
    /// recovery codes (returned once). Returns `None` if the code is wrong
    /// or there is no pending enrollment.
    ///
    /// # Errors
    /// [`IdentityError::Crypto`]/[`IdentityError::Store`] on failure.
    pub async fn confirm_totp(
        &self,
        tenant: &TenantId,
        user: &UserId,
        code: &str,
    ) -> Result<Option<Vec<Secret>>> {
        let ts = self.store().for_tenant(tenant.clone());
        let Some(row) = ts.totp_of(user).await? else {
            return Ok(None);
        };
        if !verify_totp(&row.secret, code, now_unix()) {
            return Ok(None);
        }
        ts.enable_totp(user).await?;
        let codes = self.reset_recovery_codes(tenant, user).await?;
        Ok(Some(codes))
    }

    /// Disables TOTP for a user and clears their recovery codes.
    ///
    /// # Errors
    /// [`IdentityError::Store`] on a persistence failure.
    pub async fn disable_totp(&self, tenant: &TenantId, user: &UserId) -> Result<()> {
        let ts = self.store().for_tenant(tenant.clone());
        ts.clear_totp(user).await?;
        ts.set_recovery_codes(user, &[]).await?;
        Ok(())
    }

    /// Generates and stores a fresh set of recovery codes (replacing any
    /// existing), returning the plaintext once.
    ///
    /// # Errors
    /// [`IdentityError::Crypto`]/[`IdentityError::Store`] on failure.
    pub async fn reset_recovery_codes(
        &self,
        tenant: &TenantId,
        user: &UserId,
    ) -> Result<Vec<Secret>> {
        let mut codes = Vec::with_capacity(RECOVERY_CODE_COUNT);
        let mut hashes = Vec::with_capacity(RECOVERY_CODE_COUNT);
        for _ in 0..RECOVERY_CODE_COUNT {
            let mut raw = [0u8; 10];
            secret::random_bytes(&mut raw).map_err(|_| IdentityError::Crypto)?;
            let code = base32_encode(&raw);
            hashes.push(secret::hash_at_rest(&code));
            codes.push(Secret::new(code));
        }
        self.store()
            .for_tenant(tenant.clone())
            .set_recovery_codes(user, &hashes)
            .await?;
        Ok(codes)
    }

    /// Checks the second factor at login time. Returns
    /// [`TotpOutcome::NotEnrolled`] if the user has no enabled TOTP;
    /// otherwise verifies `code` as either a current TOTP code or a
    /// single-use recovery code (which it consumes on match).
    ///
    /// # Errors
    /// [`IdentityError::Store`] on a persistence failure.
    pub async fn check_second_factor(
        &self,
        tenant: &TenantId,
        user: &UserId,
        code: Option<&str>,
    ) -> Result<TotpOutcome> {
        let ts = self.store().for_tenant(tenant.clone());
        let Some(row) = ts.totp_of(user).await? else {
            return Ok(TotpOutcome::NotEnrolled);
        };
        if !row.enabled {
            return Ok(TotpOutcome::NotEnrolled);
        }
        let Some(code) = code else {
            return Ok(TotpOutcome::Failed);
        };
        if verify_totp(&row.secret, code, now_unix()) {
            return Ok(TotpOutcome::Verified);
        }
        // Not a TOTP code — try a single-use recovery code (constant-time
        // compare against each stored hash, then atomic consume).
        let presented_hash = secret::hash_at_rest(code);
        for (id, stored_hash) in ts.unused_recovery_codes(user).await? {
            if secret::ct_eq(presented_hash.as_bytes(), stored_hash.as_bytes())
                && ts.consume_recovery_code(&id).await?
            {
                return Ok(TotpOutcome::Verified);
            }
        }
        Ok(TotpOutcome::Failed)
    }

    /// Whether a user has TOTP enabled (gates the login flow).
    ///
    /// # Errors
    /// [`IdentityError::Store`] on a persistence failure.
    pub async fn totp_enabled(&self, tenant: &TenantId, user: &UserId) -> Result<bool> {
        Ok(self
            .store()
            .for_tenant(tenant.clone())
            .totp_of(user)
            .await?
            .map(|r| r.enabled)
            .unwrap_or(false))
    }
}

fn now_unix() -> u64 {
    // now_utc() is post-epoch; a negative value is not representable here.
    OffsetDateTime::now_utc().unix_timestamp().max(0) as u64
}

/// The RFC 4226 HOTP value for a counter.
fn hotp(secret: &[u8], counter: u64) -> u32 {
    let key = hmac::Key::new(hmac::HMAC_SHA1_FOR_LEGACY_USE_ONLY, secret);
    let tag = hmac::sign(&key, &counter.to_be_bytes());
    let digest = tag.as_ref();
    // Dynamic truncation (RFC 4226 §5.3). SHA-1 output is 20 bytes, so the
    // low nibble of the last byte is a valid offset into [0, 15].
    let offset = (digest[digest.len() - 1] & 0x0f) as usize;
    let bin = (u32::from(digest[offset] & 0x7f) << 24)
        | (u32::from(digest[offset + 1]) << 16)
        | (u32::from(digest[offset + 2]) << 8)
        | u32::from(digest[offset + 3]);
    bin % 10u32.pow(DIGITS)
}

/// Verifies a presented TOTP code at `unix_time`, accepting ±[`DRIFT_STEPS`]
/// for clock skew. Constant-time string comparison.
fn verify_totp(secret: &[u8], presented: &str, unix_time: u64) -> bool {
    let presented = presented.trim();
    if presented.len() != DIGITS as usize || !presented.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    let step = unix_time / STEP_SECS;
    let mut matched = false;
    for delta in -DRIFT_STEPS..=DRIFT_STEPS {
        let counter = step.wrapping_add_signed(delta);
        let expected = format!("{:0width$}", hotp(secret, counter), width = DIGITS as usize);
        // Compare every candidate (no early break) so timing does not leak
        // which step matched.
        matched |= secret::ct_eq(expected.as_bytes(), presented.as_bytes());
    }
    matched
}

/// The label used in the `otpauth://` issuer field — the issuer host,
/// without scheme, so an authenticator app shows a clean name.
fn issuer_label(issuer: &str) -> &str {
    issuer
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or(issuer)
}

/// Builds an `otpauth://totp/…` provisioning URI (the QR payload).
fn provisioning_uri(issuer: &str, account: &str, secret_base32: &str) -> String {
    let label = format!("{}:{}", pct(issuer), pct(account));
    format!(
        "otpauth://totp/{label}?secret={secret_base32}&issuer={}&algorithm=SHA1&digits={DIGITS}&period={STEP_SECS}",
        pct(issuer)
    )
}

/// Minimal percent-encoding for the URI label/issuer (encode the few chars
/// that would break the `otpauth` structure).
fn pct(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char);
            }
            other => {
                use std::fmt::Write as _;
                let _ = write!(out, "%{other:02X}");
            }
        }
    }
    out
}

/// The current TOTP code for a base32 secret. Exposed so an enrollment UI
/// (or a test) can compute the code the authenticator app would show now.
/// `None` if the secret is not valid base32.
pub fn current_code(secret_base32: &str) -> Option<String> {
    let secret = base32_decode(secret_base32)?;
    let counter = now_unix() / STEP_SECS;
    Some(format!(
        "{:0width$}",
        hotp(&secret, counter),
        width = DIGITS as usize
    ))
}

/// RFC 4648 base32 decode (uppercase, no padding). `None` on an invalid
/// character.
fn base32_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut buffer: u32 = 0;
    let mut bits: u32 = 0;
    for c in s.trim_end_matches('=').bytes() {
        let val = match c {
            b'A'..=b'Z' => c - b'A',
            b'2'..=b'7' => c - b'2' + 26,
            _ => return None,
        };
        buffer = (buffer << 5) | u32::from(val);
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    Some(out)
}

/// RFC 4648 base32 (uppercase, no padding).
fn base32_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut out = String::new();
    let mut buffer: u32 = 0;
    let mut bits: u32 = 0;
    for &byte in data {
        buffer = (buffer << 8) | u32::from(byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            let idx = ((buffer >> bits) & 0x1f) as usize;
            out.push(ALPHABET[idx] as char);
        }
    }
    if bits > 0 {
        let idx = ((buffer << (5 - bits)) & 0x1f) as usize;
        out.push(ALPHABET[idx] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn rfc6238_hotp_test_vectors() {
        // RFC 4226 Appendix D reference secret "12345678901234567890".
        let secret = b"12345678901234567890";
        // RFC 4226 truncated HOTP values for counters 0..=3.
        assert_eq!(format!("{:06}", hotp(secret, 0)), "755224");
        assert_eq!(format!("{:06}", hotp(secret, 1)), "287082");
        assert_eq!(format!("{:06}", hotp(secret, 2)), "359152");
        assert_eq!(format!("{:06}", hotp(secret, 3)), "969429");
    }

    #[test]
    fn totp_verifies_within_drift_and_rejects_outside() {
        let secret = b"12345678901234567890";
        let t = 59; // one step in
        let step = t / STEP_SECS;
        let good = format!("{:06}", hotp(secret, step));
        assert!(verify_totp(secret, &good, t));
        // A code from 3 steps away is outside the ±1 drift window.
        let far = format!("{:06}", hotp(secret, step + 3));
        assert!(!verify_totp(secret, &far, t));
        assert!(!verify_totp(secret, "000000", t));
        assert!(!verify_totp(secret, "abc", t));
    }

    #[test]
    fn base32_roundtrips_known_vector() {
        // RFC 4648 test vector: "foobar" → "MZXW6YTBOI".
        assert_eq!(base32_encode(b"foobar"), "MZXW6YTBOI");
        assert_eq!(base32_decode("MZXW6YTBOI").unwrap(), b"foobar");
        // Round-trips arbitrary bytes.
        let secret = [1u8, 2, 3, 250, 128, 64, 200, 5];
        assert_eq!(
            base32_decode(&base32_encode(&secret)).unwrap(),
            secret.to_vec()
        );
        assert!(base32_decode("!!!invalid").is_none());
    }

    #[test]
    fn provisioning_uri_is_well_formed() {
        let uri = provisioning_uri("id.alo.test", "alice@alo.test", "JBSWY3DPEHPK3PXP");
        assert!(uri.starts_with("otpauth://totp/id.alo.test:alice%40alo.test?"));
        assert!(uri.contains("secret=JBSWY3DPEHPK3PXP"));
        assert!(uri.contains("issuer=id.alo.test"));
        assert!(uri.contains("algorithm=SHA1"));
    }
}
