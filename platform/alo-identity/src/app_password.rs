//! App-specific passwords: server-generated credentials for legacy mail
//! clients (IMAP/POP3 `LOGIN`, SMTP `AUTH`) that cannot carry a second
//! factor. The crypto lives here — CSPRNG generation, argon2id hashing
//! under the same parameter contract as account passwords, and a verify
//! that pays the dummy hash on the unknown-user path — while the store
//! beneath persists only the PHC hash and never sees a secret.
//!
//! **This module is mechanism, not policy.** Whether a given account may
//! use an app password on a given protocol (and the fail-closed rule for
//! 2FA accounts' primary passwords) is decided at the legacy auth seam,
//! not here.

use alo_store::{AppPasswordId, AppPasswordRow, TenantId, UserId};

use crate::secret::{self, CryptoError, Secret};
use crate::{Identity, IdentityError, Principal, Result};

/// Generated secrets are 16 random lowercase letters — ~75 bits, far past
/// online-guessing reach even before argon2 and the seam's backoff.
const SECRET_LETTERS: usize = 16;

/// Generates a fresh app-password secret: [`SECRET_LETTERS`] random
/// lowercase letters, displayed in dash-separated groups of four
/// (`swrn-qmxz-...`) so a human can transcribe it into a client's password
/// field. Letters only, because that field is sometimes typed by hand on a
/// phone; the entropy budget is carried by length, not symbols.
///
/// # Errors
/// [`CryptoError`] if the system CSPRNG is unavailable — a hard error,
/// never a weaker fallback.
pub fn generate_app_password() -> std::result::Result<Secret, CryptoError> {
    let mut letters = Vec::with_capacity(SECRET_LETTERS);
    while letters.len() < SECRET_LETTERS {
        let mut buf = [0u8; 32];
        secret::random_bytes(&mut buf)?;
        for byte in buf {
            // Rejection sampling: 234 = 26 * 9 is the largest multiple of 26
            // in a byte's range, so accepted bytes map to letters uniformly.
            if byte < 234 && letters.len() < SECRET_LETTERS {
                letters.push(b'a' + byte % 26);
            }
        }
    }
    let mut display = String::with_capacity(SECRET_LETTERS + SECRET_LETTERS / 4);
    for (i, letter) in letters.iter().enumerate() {
        if i > 0 && i % 4 == 0 {
            display.push('-');
        }
        display.push(char::from(*letter));
    }
    Ok(Secret::new(display))
}

/// The canonical form an app password is hashed and verified in: ASCII
/// letters and digits only, lowercased. Display grouping, and whatever a
/// client or a clipboard adds around it (dashes, spaces), never changes
/// what the secret *is* — so a password pasted with its dashes and one
/// typed without them both verify.
fn canonical(presented: &str) -> String {
    presented
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

impl Identity {
    /// Creates a named app password for `(tenant, user)`: generates the
    /// secret, argon2id-hashes its canonical form, and stores only the
    /// hash. The secret is returned exactly once — it is not retrievable,
    /// by anyone, ever again.
    ///
    /// # Errors
    /// [`IdentityError::Crypto`] on an RNG or hashing failure;
    /// [`IdentityError::Store`] if the user is not in the tenant, the name
    /// is invalid, or the per-user cap is reached.
    pub async fn create_app_password(
        &self,
        tenant: &TenantId,
        user: &UserId,
        name: &str,
    ) -> Result<(AppPasswordId, Secret)> {
        let secret = generate_app_password().map_err(|_| IdentityError::Crypto)?;
        let hash = {
            let _permit = self.argon2_slots.acquire().await;
            self.passwords
                .hash(&canonical(secret.reveal()))
                .map_err(|_| IdentityError::Crypto)?
        };
        let id = self
            .store
            .for_tenant(tenant.clone())
            .create_app_password(user, name, &hash)
            .await?;
        Ok((id, secret))
    }

    /// Verifies a presented app password for a login username, resolving
    /// to a scope-less [`Principal`] on success and stamping the record's
    /// `last_used_at`. `None` on any failure, indistinguishably: an
    /// unknown username, a user with no app passwords, and a wrong secret
    /// all pay at least one argon2 pass, so timing never says whether the
    /// user exists (the same dummy-hash seam as
    /// [`Identity::authenticate_password`]). A user with several app
    /// passwords pays one verify per stored hash — bounded by the store's
    /// per-user cap, and revealing at most how many devices a user the
    /// caller already knows exists has enrolled.
    ///
    /// A revoked app password's row is deleted, so it stops verifying on
    /// the next connection. This is the bare credential check; per-username
    /// backoff and protocol policy live at the legacy auth seam that calls
    /// it.
    ///
    /// # Errors
    /// [`IdentityError::Store`] on a persistence failure.
    pub async fn verify_app_password(
        &self,
        username: &str,
        presented: &str,
    ) -> Result<Option<Principal>> {
        let rows = self
            .store
            .app_password_credentials_by_username(username)
            .await?;
        let presented = canonical(presented);
        let _permit = self.argon2_slots.acquire().await;
        if rows.is_empty() {
            // Unknown user (or none issued): burn one argon2 cost so the
            // answer costs the same as a wrong secret.
            let _ = self.passwords.verify_or_dummy(&presented, None);
            return Ok(None);
        }
        for row in rows {
            if self.passwords.verify(&presented, &row.password_hash) {
                self.store.touch_app_password(&row.id).await?;
                return Ok(Some(Principal::protocol(row.tenant, row.user)));
            }
        }
        Ok(None)
    }

    /// A user's app passwords for the settings list — the records, never a
    /// hash or a secret. Thin pass-through kept here so callers own app
    /// passwords through one authority.
    ///
    /// # Errors
    /// [`IdentityError::Store`] if the user is not in the tenant, or on a
    /// persistence failure.
    pub async fn list_app_passwords(
        &self,
        tenant: &TenantId,
        user: &UserId,
    ) -> Result<Vec<AppPasswordRow>> {
        Ok(self
            .store
            .for_tenant(tenant.clone())
            .list_app_passwords(user)
            .await?)
    }

    /// Revokes one app password immediately: the row (and its hash) is
    /// deleted, and the credential stops verifying on the next connection.
    ///
    /// # Errors
    /// [`IdentityError::Store`] if no such record belongs to
    /// `(tenant, user)`, or on a persistence failure.
    pub async fn revoke_app_password(
        &self,
        tenant: &TenantId,
        user: &UserId,
        id: &AppPasswordId,
    ) -> Result<()> {
        self.store
            .for_tenant(tenant.clone())
            .revoke_app_password(user, id)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn generated_secrets_are_grouped_lowercase_letters() {
        let s = generate_app_password().unwrap();
        let shown = s.reveal();
        // xxxx-xxxx-xxxx-xxxx: 16 letters in 4 groups.
        assert_eq!(shown.len(), 19);
        for (i, c) in shown.chars().enumerate() {
            if i % 5 == 4 {
                assert_eq!(c, '-', "group separator expected in {shown}");
            } else {
                assert!(c.is_ascii_lowercase(), "unexpected char in {shown}");
            }
        }
    }

    #[test]
    fn generated_secrets_are_unique() {
        let a = generate_app_password().unwrap();
        let b = generate_app_password().unwrap();
        assert_ne!(a.reveal(), b.reveal());
    }

    #[test]
    fn canonical_strips_grouping_and_case_only() {
        assert_eq!(canonical("abcd-efgh-ijkl-mnop"), "abcdefghijklmnop");
        assert_eq!(canonical("ABCD efgh\tijkl mnop"), "abcdefghijklmnop");
        // Non-ASCII never silently maps onto the letter space.
        assert_eq!(canonical("äbcd"), "bcd");
    }
}
