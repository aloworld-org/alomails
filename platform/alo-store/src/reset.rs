//! Self-service password reset (ADR 0018 follow-up) — the persistent recovery
//! mailbox for a personal account, plus the short-lived reset-in-progress
//! state. Mirrors [`crate::signup`]: cross-tenant control-plane data (no tenant
//! owns a reset), so these are `Store` methods using the runtime `sqlx::query*`
//! kept out of the offline query cache.

use crate::error::{Result, StoreError};
use crate::store::Store;

/// A pending password reset that has not yet expired.
#[derive(Debug, Clone)]
pub struct PendingReset {
    /// The account address, lowercased.
    pub address: String,
    /// The external mailbox the code was sent to.
    pub recovery_email: String,
    /// SHA-256 at-rest hash of the address-salted reset code.
    pub code_hash: String,
    /// Verify attempts so far.
    pub attempts: i32,
}

impl Store {
    /// Records the recovery mailbox for a provisioned account, so a forgotten
    /// password can later be reset by mailing a code to it. Idempotent per
    /// address (re-provisioning the same address refreshes it).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn set_account_recovery(
        &self,
        address: &str,
        tenant_id: &str,
        user_id: &str,
        recovery_email: &str,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO account_recovery (address, tenant_id, user_id, recovery_email) \
             VALUES (lower($1), $2, $3, $4) \
             ON CONFLICT (address) DO UPDATE SET \
                 tenant_id = EXCLUDED.tenant_id, \
                 user_id = EXCLUDED.user_id, \
                 recovery_email = EXCLUDED.recovery_email",
        )
        .bind(address)
        .bind(tenant_id)
        .bind(user_id)
        .bind(recovery_email)
        .execute(self.pool())
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }

    /// The recovery mailbox on file for `address`, if any. `None` means the
    /// account is unknown or predates recovery capture — the reset surface
    /// treats both the same (a silent no-op) so it never leaks which addresses
    /// exist.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn account_recovery_email(&self, address: &str) -> Result<Option<String>> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT recovery_email FROM account_recovery WHERE address = lower($1)",
        )
        .bind(address)
        .fetch_optional(self.pool())
        .await
        .map_err(StoreError::Db)?;
        Ok(row.map(|(e,)| e))
    }

    /// Records (or refreshes) a pending reset for `address`, resetting the
    /// attempt counter and expiry. A re-request replaces the prior row (a new
    /// code supersedes the old one).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn upsert_pending_reset(
        &self,
        address: &str,
        recovery_email: &str,
        code_hash: &str,
        ttl_secs: i64,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO pending_resets \
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

    /// The un-expired pending reset for `address`, if any.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn pending_reset(&self, address: &str) -> Result<Option<PendingReset>> {
        let row = sqlx::query_as::<_, (String, String, String, i32)>(
            "SELECT address, recovery_email, code_hash, attempts \
             FROM pending_resets WHERE address = lower($1) AND expires_at > now()",
        )
        .bind(address)
        .fetch_optional(self.pool())
        .await
        .map_err(StoreError::Db)?;
        Ok(row.map(
            |(address, recovery_email, code_hash, attempts)| PendingReset {
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
    pub async fn bump_reset_attempts(&self, address: &str) -> Result<i32> {
        let row: Option<(i32,)> = sqlx::query_as(
            "UPDATE pending_resets SET attempts = attempts + 1 \
             WHERE address = lower($1) RETURNING attempts",
        )
        .bind(address)
        .fetch_optional(self.pool())
        .await
        .map_err(StoreError::Db)?;
        Ok(row.map(|(a,)| a).unwrap_or(0))
    }

    /// Removes the pending reset for `address` (idempotent).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn delete_pending_reset(&self, address: &str) -> Result<()> {
        sqlx::query("DELETE FROM pending_resets WHERE address = lower($1)")
            .bind(address)
            .execute(self.pool())
            .await
            .map_err(StoreError::Db)?;
        Ok(())
    }

    /// Deletes every expired pending reset, returning the count. Cheap; safe to
    /// call opportunistically to keep the table bounded.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn reap_expired_resets(&self) -> Result<u64> {
        let done = sqlx::query("DELETE FROM pending_resets WHERE expires_at < now()")
            .execute(self.pool())
            .await
            .map_err(StoreError::Db)?;
        Ok(done.rows_affected())
    }
}
