//! Audit-log persistence (ADR 0012, Law 3: kept out of `store.rs`). Every
//! administrative mutation is recorded here so a tenant admin can answer "who
//! changed this, and when". Writes are best-effort at the call site (an audit
//! failure must never fail the primary action) and carry no secret or body —
//! only an actor, a verb, a target identifier, and a short detail.
//!
//! New table (migration 0015) is not in the offline query cache, so these use
//! the runtime `sqlx::query*` path.

use time::OffsetDateTime;

use crate::error::Result;
use crate::id::{self, TenantId, UserId};
use crate::model::AuditEntry;
use crate::store::{Store, TenantStore};

impl Store {
    /// Records one audit entry for `tenant`. `actor_user_id` is the acting user
    /// when they belong to the tenant (a tenant admin); `actor_label` names the
    /// actor otherwise (e.g. `"operator"` for a platform-operator action). Both
    /// may be set. Deployment-global so the control plane can record actions on
    /// any tenant.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure (callers treat this as best-effort).
    pub async fn record_audit(
        &self,
        tenant: &TenantId,
        actor_user_id: Option<&UserId>,
        actor_label: Option<&str>,
        action: &str,
        target: Option<&str>,
        detail: Option<&str>,
    ) -> Result<()> {
        let audit_id = id::generate_token();
        sqlx::query(
            "INSERT INTO audit_log \
               (id, tenant_id, actor_user_id, actor_label, action, target, detail) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(audit_id)
        .bind(tenant.as_str())
        .bind(actor_user_id.map(UserId::as_str))
        .bind(actor_label)
        .bind(action)
        .bind(target)
        .bind(detail)
        .execute(self.pool())
        .await?;
        Ok(())
    }
}

impl TenantStore {
    /// The most recent audit entries for this tenant, newest first (capped at
    /// `limit`, itself bounded to a sane ceiling). The actor is resolved to an
    /// email when the id belongs to a current user of this tenant, else falls
    /// back to the recorded label.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn list_audit(&self, limit: i64) -> Result<Vec<AuditEntry>> {
        let capped = limit.clamp(1, 500);
        let rows = sqlx::query_as::<
            _,
            (
                String,
                Option<String>,
                String,
                Option<String>,
                Option<String>,
                OffsetDateTime,
            ),
        >(
            "SELECT a.id, COALESCE(u.email, a.actor_label) AS actor, \
                    a.action, a.target, a.detail, a.created_at \
             FROM audit_log a \
             LEFT JOIN users u ON u.id = a.actor_user_id AND u.tenant_id = a.tenant_id \
             WHERE a.tenant_id = $1 \
             ORDER BY a.created_at DESC \
             LIMIT $2",
        )
        .bind(self.tenant().as_str())
        .bind(capped)
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(
                |(id, actor, action, target, detail, created_at)| AuditEntry {
                    id,
                    actor,
                    action,
                    target,
                    detail,
                    created_at,
                },
            )
            .collect())
    }
}
