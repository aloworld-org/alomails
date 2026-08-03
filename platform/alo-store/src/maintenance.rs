//! One-time maintenance sweeps. Currently: backfill `messages.has_attachment`
//! for rows ingested before the column existed (migration 0022). The value is
//! computed by re-reading each message's raw bytes and applying the same cheap
//! heuristic ingest uses. Cross-tenant, batched, idempotent — a background task
//! calls it until it returns 0.

use crate::error::Result;
use crate::id::{MessageId, TenantId, UserId};
use crate::message::detect_attachment;
use crate::store::Store;

impl Store {
    /// Computes `has_attachment` for up to `limit` messages that don't have it
    /// yet (`NULL`). Returns how many were processed; 0 means the backfill is
    /// complete.
    ///
    /// # Errors
    /// [`StoreError::Db`](crate::error::StoreError::Db) on failure.
    pub async fn backfill_has_attachment(&self, limit: i64) -> Result<usize> {
        let rows: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT id, tenant_id, user_id FROM messages WHERE has_attachment IS NULL LIMIT $1",
        )
        .bind(limit)
        .fetch_all(self.pool())
        .await?;

        let mut done = 0;
        for (id, tenant, user) in rows {
            let acc = self.for_account(TenantId::new(tenant), UserId::new(user));
            let mid = MessageId::new(id.as_str());
            // A missing blob (shouldn't happen) still gets marked computed, as
            // `false`, so the backfill terminates.
            let has = match acc.message_bytes(&mid).await {
                Ok(bytes) => detect_attachment(&bytes),
                Err(_) => false,
            };
            sqlx::query(
                "UPDATE messages SET has_attachment = $1 \
                 WHERE tenant_id = $2 AND user_id = $3 AND id = $4",
            )
            .bind(has)
            .bind(acc.tenant.as_str())
            .bind(acc.user.as_str())
            .bind(&id)
            .execute(self.pool())
            .await?;
            done += 1;
        }
        Ok(done)
    }
}
