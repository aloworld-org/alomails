//! Pending self-service signups (ADR 0018, slice 3) — the short-lived,
//! pre-account state for an address someone is verifying. A row here is not a
//! tenant or a user; provisioning happens only on successful verification
//! (`alo-identity::signup`), so an unverified attempt leaves no account.
//!
//! Cross-tenant control-plane data (no tenant owns a pending signup), so these
//! are `Store` methods using the runtime `sqlx::query*` (kept out of the
//! offline query cache, like the rest of `control`).

use crate::error::{Result, StoreError};
use crate::store::Store;

/// A pending signup that has not yet expired.
#[derive(Debug, Clone)]
pub struct PendingSignup {
    /// The claimed address, lowercased.
    pub address: String,
    /// The external mailbox the code was sent to.
    pub recovery_email: String,
    /// SHA-256 at-rest hash of the address-salted verification code.
    pub code_hash: String,
    /// Verify attempts so far.
    pub attempts: i32,
}

impl Store {
    /// Records (or refreshes) a pending signup for `address`, resetting the
    /// attempt counter and expiry. A re-begin for the same address replaces
    /// the prior row (a new code supersedes the old one).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn upsert_pending_signup(
        &self,
        address: &str,
        recovery_email: &str,
        code_hash: &str,
        ttl_secs: i64,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO pending_signups \
                 (address, recovery_email, code_hash, attempts, expires_at) \
             VALUES (lower($1), $2, $3, 0, now() + make_interval(secs => $4)) \
             ON CONFLICT (address) DO UPDATE SET \
                 recovery_email = EXCLUDED.recovery_email, \
                 code_hash = EXCLUDED.code_hash, \
                 attempts = 0, \
                 created_at = now(), \
                 expires_at = EXCLUDED.expires_at",
        )
        .bind(address)
        .bind(recovery_email)
        .bind(code_hash)
        .bind(ttl_secs as f64)
        .execute(self.pool())
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }

    /// The un-expired pending signup for `address`, if any.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn pending_signup(&self, address: &str) -> Result<Option<PendingSignup>> {
        let row = sqlx::query_as::<_, (String, String, String, i32)>(
            "SELECT address, recovery_email, code_hash, attempts \
             FROM pending_signups WHERE address = lower($1) AND expires_at > now()",
        )
        .bind(address)
        .fetch_optional(self.pool())
        .await
        .map_err(StoreError::Db)?;
        Ok(row.map(
            |(address, recovery_email, code_hash, attempts)| PendingSignup {
                address,
                recovery_email,
                code_hash,
                attempts,
            },
        ))
    }

    /// Increments and returns the attempt counter for `address` (0 if the row
    /// is gone). Used to cap online guessing of the short code.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn bump_signup_attempts(&self, address: &str) -> Result<i32> {
        let row: Option<(i32,)> = sqlx::query_as(
            "UPDATE pending_signups SET attempts = attempts + 1 \
             WHERE address = lower($1) RETURNING attempts",
        )
        .bind(address)
        .fetch_optional(self.pool())
        .await
        .map_err(StoreError::Db)?;
        Ok(row.map(|(a,)| a).unwrap_or(0))
    }

    /// Removes the pending signup for `address` (idempotent).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn delete_pending_signup(&self, address: &str) -> Result<()> {
        sqlx::query("DELETE FROM pending_signups WHERE address = lower($1)")
            .bind(address)
            .execute(self.pool())
            .await
            .map_err(StoreError::Db)?;
        Ok(())
    }

    /// Deletes every expired pending signup, returning the count. Cheap; safe
    /// to call opportunistically to keep the table bounded.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn reap_expired_signups(&self) -> Result<u64> {
        let done = sqlx::query("DELETE FROM pending_signups WHERE expires_at < now()")
            .execute(self.pool())
            .await
            .map_err(StoreError::Db)?;
        Ok(done.rows_affected())
    }
}
