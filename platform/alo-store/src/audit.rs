//! Audit-log persistence (ADR 0012, Law 3: kept out of `store.rs`). Every
//! administrative mutation is recorded here so a tenant admin can answer "who
//! changed this, and when". Writes are best-effort at the call site (an audit
//! failure must never fail the primary action) and carry no secret or body —
//! only an actor, a verb, a target identifier, and a short detail.
//!
//! The business modules (ADR 0035) write to the same log through
//! [`Store::record_entity_audit`], which additionally names the *record* an
//! entry is about (`billing.invoice` + its id, migration 0118) so a history can
//! be read back from that record rather than scanned for in a tenant-wide list.
//! One log, two ways in: an audit trail that lives in two tables is an audit
//! trail with two answers.
//!
//! **Append-only.** There is no update or delete path to `audit_log` in this
//! crate — this module is the whole surface, and it writes and reads only.
//! Entries leave only when the tenant does (`ON DELETE CASCADE`, migration
//! 0015).
//!
//! New table (migration 0015) is not in the offline query cache, so these use
//! the runtime `sqlx::query*` path.

use time::OffsetDateTime;

use crate::error::Result;
use crate::id::{self, TenantId, UserId};
use crate::model::AuditEntry;
use crate::store::{Store, TenantStore};

/// The columns every audit read returns, already resolved to an actor label.
/// One string so the tenant-wide and per-record reads can never drift into
/// answering with different shapes.
const SELECT_ENTRIES: &str = "SELECT a.id, COALESCE(u.email, a.actor_label) AS actor, \
            a.action, a.target, a.detail, a.entity_type, a.entity_id, a.created_at \
     FROM audit_log a \
     LEFT JOIN users u ON u.id = a.actor_user_id AND u.tenant_id = a.tenant_id";

/// One row of [`SELECT_ENTRIES`], before it becomes an [`AuditEntry`].
type EntryRow = (
    String,
    Option<String>,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    OffsetDateTime,
);

fn entry_of(row: EntryRow) -> AuditEntry {
    let (id, actor, action, target, detail, entity_type, entity_id, created_at) = row;
    AuditEntry {
        id,
        actor,
        action,
        target,
        detail,
        entity_type,
        entity_id,
        created_at,
    }
}

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
        self.write_audit(
            tenant,
            actor_user_id,
            actor_label,
            action,
            target,
            detail,
            None,
            None,
        )
        .await
    }

    /// Records one audit entry *about a business record* — the same log, with
    /// the subject addressable as `(entity_type, entity_id)` so
    /// [`TenantStore::list_entity_audit`] can read that record's history back.
    ///
    /// `entity_id` is `None` only for an act that belongs to a kind of record
    /// rather than to one of them (importing a batch of leads); such an entry
    /// stays in the tenant-wide log and appears on no record's tab.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure (callers treat this as best-effort — an
    /// audit write must never undo the action it describes).
    pub async fn record_entity_audit(
        &self,
        tenant: &TenantId,
        actor_user_id: Option<&UserId>,
        action: &str,
        entity_type: &str,
        entity_id: Option<&str>,
        target: Option<&str>,
    ) -> Result<()> {
        self.write_audit(
            tenant,
            actor_user_id,
            None,
            action,
            target,
            None,
            Some(entity_type),
            entity_id,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn write_audit(
        &self,
        tenant: &TenantId,
        actor_user_id: Option<&UserId>,
        actor_label: Option<&str>,
        action: &str,
        target: Option<&str>,
        detail: Option<&str>,
        entity_type: Option<&str>,
        entity_id: Option<&str>,
    ) -> Result<()> {
        let audit_id = id::generate_token();
        sqlx::query(
            "INSERT INTO audit_log \
               (id, tenant_id, actor_user_id, actor_label, action, target, detail, \
                entity_type, entity_id) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(audit_id)
        .bind(tenant.as_str())
        .bind(actor_user_id.map(UserId::as_str))
        .bind(actor_label)
        .bind(action)
        .bind(target)
        .bind(detail)
        .bind(entity_type)
        .bind(entity_id)
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
        let rows = sqlx::query_as::<_, EntryRow>(&format!(
            "{SELECT_ENTRIES} WHERE a.tenant_id = $1 ORDER BY a.created_at DESC, a.id DESC LIMIT $2"
        ))
        .bind(self.tenant().as_str())
        .bind(capped)
        .fetch_all(self.pool())
        .await?;
        Ok(rows.into_iter().map(entry_of).collect())
    }

    /// One business record's history, newest first — every entry recorded
    /// against `(entity_type, entity_id)` **in this tenant**.
    ///
    /// The tenant clause is what makes this safe to expose from a record page:
    /// another tenant's record id is not a different answer but an empty one,
    /// exactly like an id that was never issued. There is no existence check
    /// and no `404`, so the endpoint is not an oracle for "this id exists
    /// somewhere".
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn list_entity_audit(
        &self,
        entity_type: &str,
        entity_id: &str,
        limit: i64,
    ) -> Result<Vec<AuditEntry>> {
        let capped = limit.clamp(1, 500);
        let rows = sqlx::query_as::<_, EntryRow>(&format!(
            "{SELECT_ENTRIES} \
             WHERE a.tenant_id = $1 AND a.entity_type = $2 AND a.entity_id = $3 \
             ORDER BY a.created_at DESC, a.id DESC LIMIT $4"
        ))
        .bind(self.tenant().as_str())
        .bind(entity_type)
        .bind(entity_id)
        .bind(capped)
        .fetch_all(self.pool())
        .await?;
        Ok(rows.into_iter().map(entry_of).collect())
    }
}
