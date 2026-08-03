//! Mail settings persistence (Law 3: kept out of `account.rs`/`store.rs`): a
//! per-user signature and the tenant-wide organization footer. Both are HTML
//! fragments the compose surface inserts; an unset value is the empty string.
//!
//! New table/column land in migration 0017 and are not in the offline query
//! cache, so these use the runtime `sqlx::query*` path.

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::TenantId;
use crate::store::Store;

impl AccountStore {
    /// This user's mail signature (HTML), or empty if unset.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn signature(&self) -> Result<String> {
        let sig: Option<String> = sqlx::query_scalar(
            "SELECT signature FROM user_settings WHERE tenant_id = $1 AND user_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .fetch_optional(&self.pool)
        .await?;
        Ok(sig.unwrap_or_default())
    }

    /// Sets this user's mail signature (HTML). Upsert; `updated_at` is bumped.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn set_signature(&self, signature: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO user_settings (tenant_id, user_id, signature) VALUES ($1, $2, $3) \
             ON CONFLICT (tenant_id, user_id) DO UPDATE SET signature = $3, updated_at = now()",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(signature)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// This user's mail filter rules as an opaque JSON string (the structured
    /// form the settings UI edits), or `"[]"` if unset. alo-jmap owns the
    /// rule model; the store only persists the text.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn filters(&self) -> Result<String> {
        let json: Option<String> = sqlx::query_scalar(
            "SELECT filters FROM user_settings WHERE tenant_id = $1 AND user_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .fetch_optional(&self.pool)
        .await?;
        Ok(json.unwrap_or_else(|| "[]".to_owned()))
    }

    /// Persists this user's mail filter rules (opaque JSON). Upsert. Does not
    /// touch the Sieve script — the caller rebuilds the managed script after.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn set_filters(&self, filters_json: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO user_settings (tenant_id, user_id, filters) VALUES ($1, $2, $3) \
             ON CONFLICT (tenant_id, user_id) DO UPDATE SET filters = $3, updated_at = now()",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(filters_json)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Persists the out-of-office state only (no Sieve script side effect), so a
    /// caller that regenerates a *combined* managed script (filters + vacation)
    /// controls script installation itself. See [`set_out_of_office`] for the
    /// self-contained variant.
    ///
    /// [`set_out_of_office`]: Self::set_out_of_office
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn set_out_of_office_state(
        &self,
        enabled: bool,
        subject: &str,
        message: &str,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO user_settings (tenant_id, user_id, ooo_enabled, ooo_subject, ooo_message) \
             VALUES ($1, $2, $3, $4, $5) \
             ON CONFLICT (tenant_id, user_id) DO UPDATE \
             SET ooo_enabled = $3, ooo_subject = $4, ooo_message = $5, updated_at = now()",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(enabled)
        .bind(subject)
        .bind(message)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// This user's out-of-office state: `(enabled, subject, message)`.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn out_of_office(&self) -> Result<(bool, String, String)> {
        let row = sqlx::query_as::<_, (bool, String, String)>(
            "SELECT ooo_enabled, ooo_subject, ooo_message FROM user_settings \
             WHERE tenant_id = $1 AND user_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.unwrap_or((false, String::new(), String::new())))
    }

    /// Sets the out-of-office auto-reply. When `enabled`, installs and activates
    /// a managed `out-of-office` Sieve `vacation` script (the existing vacation
    /// machinery then delivers the reply, with its per-correspondent
    /// suppression); when disabled, deactivates and removes it. The state is
    /// also persisted so the settings UI can show it.
    ///
    /// # Errors
    /// [`StoreError::Conflict`] if the generated script fails to compile (should
    /// not happen — the message is escaped); [`StoreError::Db`] on failure.
    pub async fn set_out_of_office(
        &self,
        enabled: bool,
        subject: &str,
        message: &str,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO user_settings (tenant_id, user_id, ooo_enabled, ooo_subject, ooo_message) \
             VALUES ($1, $2, $3, $4, $5) \
             ON CONFLICT (tenant_id, user_id) DO UPDATE \
             SET ooo_enabled = $3, ooo_subject = $4, ooo_message = $5, updated_at = now()",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(enabled)
        .bind(subject)
        .bind(message)
        .execute(&self.pool)
        .await?;

        if enabled {
            self.put_sieve_script(OOO_SCRIPT, &ooo_script(subject, message))
                .await?;
            self.activate_sieve_script(Some(OOO_SCRIPT)).await?;
        } else {
            // Deactivate before delete — `delete_sieve_script` refuses an active
            // script (it must never leave delivery with a dangling active row).
            self.activate_sieve_script(None).await?;
            // Best-effort remove; absent is fine.
            if let Err(error) = self.delete_sieve_script(OOO_SCRIPT).await
                && !matches!(error, StoreError::NotFound)
            {
                return Err(error);
            }
        }
        Ok(())
    }
}

/// The name of the managed out-of-office Sieve script.
const OOO_SCRIPT: &str = "out-of-office";

/// Builds a `vacation` Sieve script from the user's subject + message, escaping
/// the two characters special to a Sieve quoted string (`\` and `"`). A
/// 7-day per-correspondent suppression window is used.
fn ooo_script(subject: &str, message: &str) -> String {
    let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let subject_arg = if subject.trim().is_empty() {
        String::new()
    } else {
        format!(" :subject \"{}\"", esc(subject))
    };
    format!(
        "require [\"vacation\"];\nvacation :days 7{subject_arg} \"{}\";",
        esc(message)
    )
}

impl Store {
    /// The tenant's organization footer (HTML), appended to outgoing mail, or
    /// empty if unset.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn org_footer(&self, tenant: &TenantId) -> Result<String> {
        let footer: Option<String> =
            sqlx::query_scalar("SELECT org_footer FROM tenants WHERE id = $1")
                .bind(tenant.as_str())
                .fetch_optional(self.pool())
                .await?;
        Ok(footer.unwrap_or_default())
    }

    /// Sets the tenant's organization footer (HTML). Admin-set (ADR 0012 gate
    /// enforced by the caller).
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the tenant does not exist;
    /// [`StoreError::Db`] on failure.
    pub async fn set_org_footer(&self, tenant: &TenantId, footer: &str) -> Result<()> {
        let done = sqlx::query("UPDATE tenants SET org_footer = $2 WHERE id = $1")
            .bind(tenant.as_str())
            .bind(footer)
            .execute(self.pool())
            .await?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }
}
