//! Send later (Gmail-style scheduled send): hold a composed draft until a chosen
//! time. Scheduling moves the draft into the account's **Scheduled** mailbox and
//! records the validated envelope + send time in `scheduled_sends` (migration
//! 0023); a background sweeper on [`Store`] submits due messages through the
//! normal outbound path and files them to Sent. Cancelling deletes the row and
//! returns the draft to Drafts. Membership changes reuse the tested
//! `add_to_mailbox` / `remove_from_mailbox` helpers; the new table is
//! read/written with runtime queries (not in the offline cache).

use crate::account::AccountStore;
use crate::error::Result;
use crate::id::{MailboxId, MessageId, TenantId, UserId};
use crate::store::Store;

/// A due scheduled send, as the sweeper needs it: which message, whose account,
/// and the validated envelope to put back on the wire.
#[derive(Debug, Clone)]
pub struct DueSend {
    pub tenant: TenantId,
    pub user: UserId,
    pub message_id: MessageId,
    pub mail_from: String,
    pub rcpts: Vec<String>,
    /// The acting delegate's address for an on-behalf send scheduled from a
    /// shared mailbox (ADR 0017) — the sweeper prepends it as `Sender:` so the
    /// disclosure header reaches the wire exactly as an immediate send's would.
    pub on_behalf_sender: Option<String>,
}

impl AccountStore {
    /// This account's mailbox id for `role`, or `None` if it has none yet.
    /// Public: callers beyond scheduling resolve special-use folders too
    /// (e.g. junk-training detects moves into/out of the `junk` role).
    ///
    /// # Errors
    /// [`StoreError::Db`](crate::error::StoreError::Db) on failure.
    pub async fn mailbox_by_role(&self, role: &str) -> Result<Option<MailboxId>> {
        let id: Option<String> = sqlx::query_scalar(
            "SELECT id FROM mailboxes WHERE tenant_id = $1 AND user_id = $2 AND role = $3",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(role)
        .fetch_optional(&self.pool)
        .await?;
        Ok(id.map(MailboxId::new))
    }

    /// Gets-or-creates this account's Scheduled mailbox.
    async fn ensure_scheduled_mailbox(&self) -> Result<MailboxId> {
        if let Some(id) = self.mailbox_by_role("scheduled").await? {
            return Ok(id);
        }
        self.create_mailbox(None, "Scheduled", Some("scheduled"))
            .await
    }

    /// Schedule a draft to be sent at `send_at_epoch` (Unix seconds). Records the
    /// pre-validated envelope, moves the draft out of Drafts into the Scheduled
    /// mailbox, and (re)sets the row so re-scheduling the same draft is
    /// idempotent. The caller is responsible for having validated `mail_from` /
    /// `rcpts` (send-from rights, recipient sanity) exactly as an immediate
    /// submission would — the sweeper trusts what is stored here.
    ///
    /// # Errors
    /// [`StoreError::Db`](crate::error::StoreError::Db) on failure.
    pub async fn schedule_send(
        &self,
        mid: &MessageId,
        mail_from: &str,
        rcpts: &[String],
        send_at_epoch: i64,
        on_behalf_sender: Option<&str>,
    ) -> Result<()> {
        let scheduled = self.ensure_scheduled_mailbox().await?;
        sqlx::query(
            "INSERT INTO scheduled_sends \
                 (tenant_id, user_id, message_id, send_at, mail_from, rcpts, on_behalf_sender) \
             VALUES ($1, $2, $3, to_timestamp($4), $5, $6, $7) \
             ON CONFLICT (tenant_id, user_id, message_id) DO UPDATE \
                 SET send_at = EXCLUDED.send_at, mail_from = EXCLUDED.mail_from, \
                     rcpts = EXCLUDED.rcpts, on_behalf_sender = EXCLUDED.on_behalf_sender",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(mid.as_str())
        .bind(send_at_epoch)
        .bind(mail_from)
        .bind(rcpts)
        .bind(on_behalf_sender)
        .execute(&self.pool)
        .await?;
        // Add to Scheduled first so the draft is never in neither mailbox.
        self.add_to_mailbox(mid, &scheduled).await?;
        if let Some(drafts) = self.mailbox_by_role("drafts").await?
            && drafts.as_str() != scheduled.as_str()
        {
            self.remove_from_mailbox(mid, &drafts).await?;
        }
        Ok(())
    }

    /// Cancel a scheduled send: delete the row and return the draft to Drafts
    /// (removing it from Scheduled). Returns `true` if a scheduled send existed.
    /// A no-op (returns `false`) if the message was not scheduled.
    ///
    /// # Errors
    /// [`StoreError::Db`](crate::error::StoreError::Db) on failure.
    pub async fn cancel_send(&self, mid: &MessageId) -> Result<bool> {
        let deleted = sqlx::query(
            "DELETE FROM scheduled_sends \
             WHERE tenant_id = $1 AND user_id = $2 AND message_id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(mid.as_str())
        .execute(&self.pool)
        .await?
        .rows_affected();
        if deleted == 0 {
            return Ok(false);
        }
        self.return_to_drafts(mid).await?;
        Ok(true)
    }

    /// Move a message out of the Scheduled mailbox and back into Drafts. Used
    /// both when the user cancels and when the sweeper gives up after repeated
    /// failures — either way the draft should be editable again. Best-effort on
    /// the mailbox side: the message keeps its `$draft` keyword regardless.
    ///
    /// # Errors
    /// [`StoreError::Db`](crate::error::StoreError::Db) on failure.
    pub async fn return_to_drafts(&self, mid: &MessageId) -> Result<()> {
        let drafts = match self.mailbox_by_role("drafts").await? {
            Some(id) => id,
            None => self.create_mailbox(None, "Drafts", Some("drafts")).await?,
        };
        self.add_to_mailbox(mid, &drafts).await?;
        if let Some(scheduled) = self.mailbox_by_role("scheduled").await? {
            self.remove_from_mailbox(mid, &scheduled).await?;
        }
        Ok(())
    }
}

impl Store {
    /// Atomically **claim** every scheduled send whose time has passed (oldest
    /// first, bounded): the rows are deleted and returned in one statement, so a
    /// claimed send is never handed out twice. This is what makes the sweeper
    /// *at-most-once* — the row is gone before the message hits the wire, so a
    /// crash or DB hiccup after submission can never cause a double-send. A send
    /// that then fails is returned to Drafts by the caller (nothing is lost, but
    /// it is never silently re-sent). `FOR UPDATE SKIP LOCKED` keeps it correct
    /// even if more than one sweeper ever runs. Cross-tenant maintenance.
    ///
    /// # Errors
    /// [`StoreError::Db`](crate::error::StoreError::Db) on failure.
    pub async fn claim_due_sends(&self, limit: i64) -> Result<Vec<DueSend>> {
        type DueRow = (String, String, String, String, Vec<String>, Option<String>);
        let rows: Vec<DueRow> = sqlx::query_as(
            "DELETE FROM scheduled_sends \
                 WHERE (tenant_id, user_id, message_id) IN ( \
                     SELECT tenant_id, user_id, message_id FROM scheduled_sends \
                     WHERE send_at <= now() ORDER BY send_at LIMIT $1 \
                     FOR UPDATE SKIP LOCKED \
                 ) \
                 RETURNING tenant_id, user_id, message_id, mail_from, rcpts, on_behalf_sender",
        )
        .bind(limit)
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(
                |(tenant, user, id, mail_from, rcpts, on_behalf_sender)| DueSend {
                    tenant: TenantId::new(tenant),
                    user: UserId::new(user),
                    message_id: MessageId::new(id),
                    mail_from,
                    rcpts,
                    on_behalf_sender,
                },
            )
            .collect())
    }
}
