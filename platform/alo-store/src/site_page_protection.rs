//! Pages the internet can reach but only with a password (ADR 0036, S2.06a) —
//! the tenant-facing half: set a password on a page, change it, lift it, and
//! see which pages carry one.
//!
//! Three properties this module is responsible for.
//!
//! - **The plaintext is never stored and never read back.** A password is
//!   hashed with argon2id ([`hash_site_page_password`]) and only the PHC string
//!   is written; no read on any door returns it, so "show me the password"
//!   has no implementation to leak — the owner can only replace it.
//! - **Protection is about the page that is online now, not about a publish.**
//!   The row lives beside the site rather than inside the immutable snapshot
//!   set, so setting, changing, or lifting a password takes effect on the next
//!   public request. Rejected alternative: freezing protection into
//!   `site_page_snapshots` with everything else a publish freezes — consistent
//!   with the rest of the model, but it would leave a leaked password working
//!   until the owner happened to republish, which is the wrong failure
//!   direction for a security control.
//! - **Removing the draft page does not remove the protection.** Deleting a
//!   page does not unpublish its snapshot: the published set keeps serving that
//!   page until the next publish. The row therefore keys off the page identity
//!   and hangs off the *site* (migration `0303`), so the still-served snapshot
//!   stays closed. Protection ends when somebody lifts it, or with the site.
//!
//! The public gate never sees the hash: every write derives an opaque
//! [`SitePageProtection::version`] from it, and `alo-sites` mints visitor
//! sessions against that token ([`crate::site_public_protection`]). Changing or
//! lifting the password rotates (or removes) the version, which ends every
//! session that was opened with the old one.

use argon2::Argon2;
use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};
use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::{SiteId, SitePageId};

/// Shortest password a page may be protected with. Short enough that a small
/// business can say it over the phone, long enough that the argon2 cost is not
/// the only thing standing between a guesser and the page.
pub const SITE_PAGE_PASSWORD_MIN_CHARS: usize = 8;

/// Longest password accepted. A bound on work, not on ambition: a passphrase
/// well past this length adds nothing an attacker can exploit, and unbounded
/// input is unbounded hashing.
pub const SITE_PAGE_PASSWORD_MAX_CHARS: usize = 128;

/// What a tenant sees about one protected page: that it is protected and when
/// that was last decided. Deliberately carries neither the password nor its
/// hash — no read on any door returns those.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SitePageProtection {
    /// The page this password protects, in every language it is published in.
    pub page: SitePageId,
    /// When the page was first protected.
    pub created_at: OffsetDateTime,
    /// When the password was last set — the moment older visitor sessions
    /// stopped working.
    pub updated_at: OffsetDateTime,
}

/// Hashes a page password to an argon2id PHC string after checking the rules.
///
/// Shared with the public door so both halves agree on what a password is.
/// The cost is argon2's default parameters (the same family the credential
/// authority uses); callers on a request path should keep it off the async
/// executor.
///
/// # Errors
/// [`StoreError::Validation`] naming the broken rule in words a person can act
/// on; [`StoreError::Crypto`] if hashing itself fails.
pub fn hash_site_page_password(password: &str) -> Result<String> {
    validate_site_page_password(password)?;
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|_| StoreError::Crypto)
}

/// Checks a candidate password against a stored PHC string. `false` for a
/// mismatch and for a stored value that cannot be parsed — a broken row denies
/// access rather than granting it.
#[must_use]
pub fn verify_site_page_password(password: &str, phc: &str) -> bool {
    match PasswordHash::new(phc) {
        Ok(parsed) => Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

/// The rules a page password must satisfy, stated once for both doors.
///
/// # Errors
/// [`StoreError::Validation`] naming the broken rule.
pub fn validate_site_page_password(password: &str) -> Result<()> {
    let length = password.chars().count();
    if length < SITE_PAGE_PASSWORD_MIN_CHARS {
        return Err(StoreError::Validation(format!(
            "a page password must be at least {SITE_PAGE_PASSWORD_MIN_CHARS} characters"
        )));
    }
    if length > SITE_PAGE_PASSWORD_MAX_CHARS {
        return Err(StoreError::Validation(format!(
            "a page password may be at most {SITE_PAGE_PASSWORD_MAX_CHARS} characters"
        )));
    }
    if password.trim().is_empty() {
        return Err(StoreError::Validation(
            "a page password must be more than spaces".to_owned(),
        ));
    }
    if password.chars().any(char::is_control) {
        return Err(StoreError::Validation(
            "a page password may not contain control characters".to_owned(),
        ));
    }
    Ok(())
}

/// The opaque session token derived from a stored hash. It is a one-way
/// function of the PHC string (which already contains a random salt), so it
/// changes on every password write and reveals nothing about the password —
/// which is what lets the public service hold it while the hash stays here.
#[must_use]
pub(crate) fn protection_version(phc: &str) -> String {
    let digest = Sha256::digest(phc.as_bytes());
    URL_SAFE_NO_PAD.encode(&digest[..16])
}

impl AccountStore {
    /// Protects `page` of `site` with `password`, replacing any password it
    /// already had. The new password is live on the next public request, and
    /// every visitor session opened with the previous one stops working.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the page is not this tenant's page on that
    /// site; [`StoreError::Validation`] naming a broken password rule;
    /// [`StoreError::Crypto`] on a hashing failure; [`StoreError::Db`].
    pub async fn set_site_page_password(
        &self,
        site: &SiteId,
        page: &SitePageId,
        password: &str,
    ) -> Result<SitePageProtection> {
        validate_site_page_password(password)?;
        // The page must be the tenant's own page on that site before anything
        // is hashed: an unknown id costs a lookup, not an argon2 run.
        let known: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM site_pages WHERE tenant_id = $1 AND site_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(page.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if known.is_none() {
            return Err(StoreError::NotFound);
        }
        let owned = password.to_owned();
        // argon2 is deliberately expensive; hashing it on the async executor
        // would stall every other request this worker thread is driving.
        let phc = tokio::task::spawn_blocking(move || hash_site_page_password(&owned))
            .await
            .map_err(|_| StoreError::Crypto)??;
        let version = protection_version(&phc);
        let row = sqlx::query_as::<_, ProtectionRow>(
            "INSERT INTO site_page_passwords \
                 (tenant_id, site_id, page_id, password_hash, version) \
             VALUES ($1, $2, $3, $4, $5) \
             ON CONFLICT (tenant_id, site_id, page_id) DO UPDATE \
                 SET password_hash = EXCLUDED.password_hash, \
                     version = EXCLUDED.version, \
                     updated_at = now() \
             RETURNING page_id, created_at, updated_at",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(page.as_str())
        .bind(&phc)
        .bind(&version)
        .fetch_one(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(row.into_protection())
    }

    /// Lifts the password on `page`, making it public again on the next
    /// request. Idempotent: a page that carries no password is already in the
    /// asked-for state, so this answers `Ok(())` rather than an error the
    /// caller would have to explain.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the site is not this tenant's;
    /// [`StoreError::Db`].
    pub async fn remove_site_page_password(&self, site: &SiteId, page: &SitePageId) -> Result<()> {
        // A page may have been deleted while its snapshot is still served, so
        // the site — not the page — is what has to resolve here.
        let site_known: Option<(String,)> =
            sqlx::query_as("SELECT id FROM sites WHERE tenant_id = $1 AND id = $2")
                .bind(self.tenant.as_str())
                .bind(site.as_str())
                .fetch_optional(&self.pool)
                .await
                .map_err(StoreError::Db)?;
        if site_known.is_none() {
            return Err(StoreError::NotFound);
        }
        sqlx::query(
            "DELETE FROM site_page_passwords \
             WHERE tenant_id = $1 AND site_id = $2 AND page_id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(page.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }

    /// Whether one page of the tenant's site is protected. A foreign site,
    /// a foreign page, and an unprotected page all read as `None` — the
    /// answer is about this tenant's own content or it is nothing.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn site_page_protection(
        &self,
        site: &SiteId,
        page: &SitePageId,
    ) -> Result<Option<SitePageProtection>> {
        let row = sqlx::query_as::<_, ProtectionRow>(
            "SELECT page_id, created_at, updated_at FROM site_page_passwords \
             WHERE tenant_id = $1 AND site_id = $2 AND page_id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(page.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(row.map(ProtectionRow::into_protection))
    }

    /// Every protected page of the tenant's `site`, so one read tells a screen
    /// which pages carry a password. A foreign site reads as an empty list.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn site_page_protections(&self, site: &SiteId) -> Result<Vec<SitePageProtection>> {
        let rows = sqlx::query_as::<_, ProtectionRow>(
            "SELECT page_id, created_at, updated_at FROM site_page_passwords \
             WHERE tenant_id = $1 AND site_id = $2 ORDER BY page_id",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows
            .into_iter()
            .map(ProtectionRow::into_protection)
            .collect())
    }
}

#[derive(sqlx::FromRow)]
struct ProtectionRow {
    page_id: String,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl ProtectionRow {
    fn into_protection(self) -> SitePageProtection {
        SitePageProtection {
            page: SitePageId::new(self.page_id),
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn password_rules_name_what_is_wrong() {
        assert!(validate_site_page_password("longenough").is_ok());
        assert!(validate_site_page_password(&"a".repeat(SITE_PAGE_PASSWORD_MAX_CHARS)).is_ok());
        let short = validate_site_page_password("short")
            .unwrap_err()
            .to_string();
        assert!(short.contains("at least 8"), "{short}");
        let long = validate_site_page_password(&"a".repeat(SITE_PAGE_PASSWORD_MAX_CHARS + 1))
            .unwrap_err()
            .to_string();
        assert!(long.contains("at most 128"), "{long}");
        assert!(
            validate_site_page_password("         ").is_err(),
            "spaces only"
        );
        assert!(
            validate_site_page_password("with\u{0}null").is_err(),
            "control characters"
        );
    }

    #[test]
    fn hashing_hides_the_password_and_verification_is_exact() {
        let phc = hash_site_page_password("open sesame please").unwrap();
        assert!(!phc.contains("sesame"), "the hash is not the plaintext");
        assert!(phc.starts_with("$argon2id$"), "{phc}");
        assert!(verify_site_page_password("open sesame please", &phc));
        assert!(!verify_site_page_password("open sesame pleas", &phc));
        assert!(!verify_site_page_password("", &phc));
        assert!(
            !verify_site_page_password("anything", "not-a-phc-string"),
            "an unparseable stored hash denies, never grants"
        );
    }

    #[test]
    fn every_write_mints_a_different_session_version() {
        let first = hash_site_page_password("open sesame please").unwrap();
        let again = hash_site_page_password("open sesame please").unwrap();
        assert_ne!(
            protection_version(&first),
            protection_version(&again),
            "the same password re-set must still end old sessions"
        );
        assert_eq!(protection_version(&first), protection_version(&first));
        assert!(
            !protection_version(&first).is_empty() && !first.contains(&protection_version(&first)),
            "the version is derived from the hash, not a slice of it"
        );
    }
}
