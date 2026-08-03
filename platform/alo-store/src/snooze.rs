//! Snooze (Gmail-style): hide a message until a chosen time. Snoozing moves the
//! message into the account's **Snoozed** mailbox and records a wake time
//! (`messages.snooze_until`, migration 0021); a background sweeper on `Store`
//! returns due messages to the Inbox, marked unread. Membership changes reuse
//! the tested `add_to_mailbox` / `remove_from_mailbox` helpers; the new column
//! is read/written with runtime queries (not in the offline cache).

use crate::account::AccountStore;
use crate::error::Result;
use crate::id::{MailboxId, MessageId, TenantId, UserId};
use crate::store::{SEEN, Store};

impl AccountStore {
    /// The account's Snoozed mailbox id, or `None` if it has none yet.
    async fn snoozed_mailbox(&self) -> Result<Option<MailboxId>> {
        let id: Option<String> = sqlx::query_scalar(
            "SELECT id FROM mailboxes WHERE tenant_id = $1 AND user_id = $2 AND role = 'snoozed'",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .fetch_optional(&self.pool)
        .await?;
        Ok(id.map(MailboxId::new))
    }

    /// Gets-or-creates the account's Snoozed mailbox.
    async fn ensure_snoozed_mailbox(&self) -> Result<MailboxId> {
        if let Some(id) = self.snoozed_mailbox().await? {
            return Ok(id);
        }
        self.create_mailbox(None, "Snoozed", Some("snoozed")).await
    }

    /// Snoozes messages until `until_epoch` (Unix seconds): move each from
    /// `from` to the Snoozed mailbox and record the wake time. Only this
    /// account's messages are affected (the membership helpers enforce it).
    ///
    /// # Errors
    /// [`StoreError::Db`](crate::error::StoreError::Db) on failure.
    pub async fn snooze(
        &self,
        ids: &[MessageId],
        from: &MailboxId,
        until_epoch: i64,
    ) -> Result<()> {
        let snoozed = self.ensure_snoozed_mailbox().await?;
        let id_strs: Vec<String> = ids.iter().map(|m| m.as_str().to_owned()).collect();
        sqlx::query(
            "UPDATE messages SET snooze_until = to_timestamp($4) \
             WHERE tenant_id = $1 AND user_id = $2 AND id = ANY($3)",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(&id_strs)
        .bind(until_epoch)
        .execute(&self.pool)
        .await?;
        for id in ids {
            // Add to Snoozed first so the message is never in neither mailbox.
            self.add_to_mailbox(id, &snoozed).await?;
            if from.as_str() != snoozed.as_str() {
                self.remove_from_mailbox(id, from).await?;
            }
        }
        Ok(())
    }
}

impl Store {
    /// Returns every message whose snooze time has passed, back to its owner's
    /// Inbox (marked unread), and clears its wake time. Cross-tenant maintenance
    /// (like the vacation machinery), safe to call on an interval. Returns how
    /// many messages were woken.
    ///
    /// # Errors
    /// [`StoreError::Db`](crate::error::StoreError::Db) on failure.
    pub async fn sweep_snoozes(&self) -> Result<usize> {
        let due: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT id, tenant_id, user_id FROM messages \
             WHERE snooze_until IS NOT NULL AND snooze_until <= now() LIMIT 500",
        )
        .fetch_all(self.pool())
        .await?;

        let mut woken = 0;
        for (id, tenant, user) in due {
            let acc = self.for_account(TenantId::new(tenant), UserId::new(user));
            let mid = MessageId::new(id.as_str());
            let inbox = acc.inbox().await?;
            if let Some(snoozed) = acc.snoozed_mailbox().await? {
                // Add to Inbox first, then leave Snoozed — never in neither.
                acc.add_to_mailbox(&mid, &inbox).await?;
                acc.remove_from_mailbox(&mid, &snoozed).await?;
            } else {
                acc.add_to_mailbox(&mid, &inbox).await?;
            }
            // A woken conversation should draw the eye — return it unread.
            acc.set_keyword(&mid, SEEN, false).await?;
            sqlx::query(
                "UPDATE messages SET snooze_until = NULL \
                 WHERE tenant_id = $1 AND user_id = $2 AND id = $3",
            )
            .bind(acc.tenant.as_str())
            .bind(acc.user.as_str())
            .bind(&id)
            .execute(self.pool())
            .await?;
            woken += 1;
        }
        Ok(woken)
    }
}
