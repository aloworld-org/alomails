//! IMAP/POP3 data-access support on [`AccountStore`], kept out of
//! `account.rs` (Law 3: its reason to change is the shims' needs, not the
//! JMAP core's). Everything here is still account-scoped by construction
//! — the same `(tenant, user)` predicate the rest of `AccountStore`
//! carries — so the shims inherit isolation rather than re-implementing
//! it. See `docs/design/imap-pop3-shims.md`.

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::changes::{Change, TYPE_EMAIL, TYPE_MAILBOX};
use crate::error::{Result, StoreError};
use crate::id::{MailboxId, MessageId};

/// A mailbox as the IMAP layer needs it: identity, hierarchy, role, the
/// UID epoch (`uid_validity`/`uid_next`), and the live counters.
#[derive(Debug, Clone)]
pub struct ImapMailbox {
    /// Opaque mailbox id.
    pub id: MailboxId,
    /// Parent mailbox id, or `None` at the root.
    pub parent_id: Option<MailboxId>,
    /// Display name (one path segment).
    pub name: String,
    /// JMAP role (`inbox`/`sent`/…), or `None`.
    pub role: Option<String>,
    /// Stable per-mailbox UIDVALIDITY.
    pub uid_validity: i64,
    /// Next UID to assign (IMAP `UIDNEXT`).
    pub uid_next: i64,
    /// Total messages (IMAP `EXISTS`).
    pub total_messages: i64,
    /// Messages without `$seen` (IMAP `UNSEEN` count / STATUS).
    pub unread_messages: i64,
}

/// One message's place in a mailbox view: its UID, its id, and its full
/// keyword/flag set. Ordered by UID ascending, the position (1-based) is
/// the IMAP message sequence number.
#[derive(Debug, Clone)]
pub struct ImapEntry {
    /// Per-mailbox UID.
    pub uid: i64,
    /// The message id.
    pub message: MessageId,
    /// The message's keywords (store form, e.g. `$seen`); the IMAP layer
    /// maps these to system/user flags.
    pub flags: Vec<String>,
}

impl ImapEntry {
    /// Whether the message currently bears `$seen`.
    pub fn seen(&self) -> bool {
        self.flags.iter().any(|f| f == crate::store::SEEN)
    }
}

/// A message row for IMAP SEARCH: its UID plus the fields cheap searches
/// test without fetching the body.
#[derive(Debug, Clone)]
pub struct ImapSearchRow {
    /// Per-mailbox UID.
    pub uid: i64,
    /// The message id (for body fetches when a BODY/TEXT/header key needs
    /// the raw bytes).
    pub message: MessageId,
    /// Unfolded subject.
    pub subject: String,
    /// Unfolded `From`.
    pub from_addr: String,
    /// Unfolded `To`.
    pub to_addrs: String,
    /// Raw size in octets.
    pub size: i64,
    /// When the store received it (INTERNALDATE).
    pub received_at: OffsetDateTime,
    /// `Date` header, if present.
    pub sent_at: Option<OffsetDateTime>,
    /// The message's keywords.
    pub flags: Vec<String>,
}

impl AccountStore {
    /// All of this account's mailboxes (bounded at 500 — a shim limit,
    /// documented), for LIST/STATUS and name resolution.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn imap_mailboxes(&self) -> Result<Vec<ImapMailbox>> {
        let rows = sqlx::query!(
            "SELECT id, parent_id, name, role, uid_validity, uid_next, \
             total_messages, unread_messages \
             FROM mailboxes WHERE tenant_id = $1 AND user_id = $2 \
             ORDER BY name LIMIT 500",
            self.tenant().as_str(),
            self.user().as_str()
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| ImapMailbox {
                id: MailboxId::new(r.id),
                parent_id: r.parent_id.map(MailboxId::new),
                name: r.name,
                role: r.role,
                uid_validity: r.uid_validity,
                uid_next: r.uid_next,
                total_messages: r.total_messages,
                unread_messages: r.unread_messages,
            })
            .collect())
    }

    /// The ordered view of a mailbox: `(uid, message, seen)` ascending by
    /// UID. A foreign/other-account mailbox yields an empty view (its rows
    /// are filtered out by the account predicate — never another account's
    /// messages).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn imap_view(&self, mailbox: &MailboxId) -> Result<Vec<ImapEntry>> {
        let rows = sqlx::query!(
            "SELECT mm.uid, mm.message_id, \
             COALESCE(array_agg(k.keyword) FILTER (WHERE k.keyword IS NOT NULL), '{}') \
               AS \"flags!: Vec<String>\" \
             FROM mailbox_messages mm \
             JOIN messages m ON m.id = mm.message_id AND m.tenant_id = mm.tenant_id \
             LEFT JOIN message_keywords k \
               ON k.message_id = mm.message_id AND k.tenant_id = mm.tenant_id \
             WHERE mm.tenant_id = $1 AND mm.mailbox_id = $2 AND m.user_id = $3 \
             GROUP BY mm.uid, mm.message_id \
             ORDER BY mm.uid ASC",
            self.tenant().as_str(),
            mailbox.as_str(),
            self.user().as_str()
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| ImapEntry {
                uid: r.uid,
                message: MessageId::new(r.message_id),
                flags: r.flags,
            })
            .collect())
    }

    /// Per-message rows for IMAP SEARCH over `mailbox`: uid, id, the
    /// fields cheap searches use (subject/from/to/size/dates), and the full
    /// flag set — one query, account-scoped. Body/header substring searches
    /// fetch bytes separately, only for candidates.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn imap_search_rows(&self, mailbox: &MailboxId) -> Result<Vec<ImapSearchRow>> {
        let rows = sqlx::query!(
            "SELECT mm.uid, m.id, m.subject, m.from_addr, m.to_addrs, m.size, \
             m.received_at, m.sent_at, \
             COALESCE(array_agg(k.keyword) FILTER (WHERE k.keyword IS NOT NULL), '{}') \
               AS \"flags!: Vec<String>\" \
             FROM mailbox_messages mm \
             JOIN messages m ON m.id = mm.message_id AND m.tenant_id = mm.tenant_id \
             LEFT JOIN message_keywords k \
               ON k.message_id = mm.message_id AND k.tenant_id = mm.tenant_id \
             WHERE mm.tenant_id = $1 AND mm.mailbox_id = $2 AND m.user_id = $3 \
             GROUP BY mm.uid, m.id \
             ORDER BY mm.uid ASC",
            self.tenant().as_str(),
            mailbox.as_str(),
            self.user().as_str()
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| ImapSearchRow {
                uid: r.uid,
                message: MessageId::new(r.id),
                subject: r.subject,
                from_addr: r.from_addr,
                to_addrs: r.to_addrs,
                size: r.size,
                received_at: r.received_at,
                sent_at: r.sent_at,
                flags: r.flags,
            })
            .collect())
    }

    /// This account's UIDs in `mailbox` that bear `keyword`, ascending —
    /// e.g. the `$deleted` set for EXPUNGE, or a flag-filtered UID SEARCH.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn imap_flagged_uids(
        &self,
        mailbox: &MailboxId,
        keyword: &str,
    ) -> Result<Vec<(i64, MessageId)>> {
        let rows = sqlx::query!(
            "SELECT mm.uid, mm.message_id \
             FROM mailbox_messages mm \
             JOIN messages m ON m.id = mm.message_id AND m.tenant_id = mm.tenant_id \
             JOIN message_keywords k ON k.message_id = mm.message_id AND k.tenant_id = mm.tenant_id \
             WHERE mm.tenant_id = $1 AND mm.mailbox_id = $2 AND m.user_id = $3 AND k.keyword = $4 \
             ORDER BY mm.uid ASC",
            self.tenant().as_str(),
            mailbox.as_str(),
            self.user().as_str(),
            keyword
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| (r.uid, MessageId::new(r.message_id)))
            .collect())
    }

    /// The UID of `message` in `mailbox`, if it is a member. Account-scoped
    /// via the message join.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn imap_uid_of(
        &self,
        mailbox: &MailboxId,
        message: &MessageId,
    ) -> Result<Option<i64>> {
        let row = sqlx::query!(
            "SELECT mm.uid FROM mailbox_messages mm \
             JOIN messages m ON m.id = mm.message_id AND m.tenant_id = mm.tenant_id \
             WHERE mm.tenant_id = $1 AND mm.mailbox_id = $2 AND m.user_id = $3 \
               AND mm.message_id = $4",
            self.tenant().as_str(),
            mailbox.as_str(),
            self.user().as_str(),
            message.as_str()
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| r.uid))
    }

    /// IMAP `APPEND`: ingest `raw` into `mailbox` with an optional
    /// INTERNALDATE, through the **same** ingestion path as delivery (no
    /// second parser). Returns the new message id and its assigned UID.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if `mailbox` is not this account's;
    /// [`StoreError::TooLarge`] over the ceiling; [`StoreError::Db`]/
    /// [`StoreError::Blob`] on failure.
    pub async fn imap_append(
        &self,
        mailbox: &MailboxId,
        raw: &[u8],
        internaldate: Option<OffsetDateTime>,
    ) -> Result<(MessageId, i64)> {
        let message = self.ingest_at(mailbox, raw, internaldate).await?;
        let uid = self
            .imap_uid_of(mailbox, &message)
            .await?
            .ok_or(StoreError::NotFound)?;
        Ok((message, uid))
    }

    /// IMAP `EXPUNGE` of one message from `mailbox`: drop its membership
    /// there; if that leaves it in no mailbox, destroy it outright (so an
    /// expunged message never lingers as an unfiled orphan). A message
    /// still in another mailbox (e.g. after COPY) survives there.
    /// Atomic; records the change for IDLE/`/changes`.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if `message` is not this account's.
    pub async fn imap_expunge(&self, mailbox: &MailboxId, message: &MessageId) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        // Ownership + serialize against concurrent flag/membership writes.
        self.lock_message(&mut tx, message).await?;
        let seen = self.message_is_seen(&mut tx, message).await?;

        let removed = sqlx::query!(
            "DELETE FROM mailbox_messages \
             WHERE tenant_id = $1 AND mailbox_id = $2 AND message_id = $3",
            self.tenant().as_str(),
            mailbox.as_str(),
            message.as_str()
        )
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if removed == 0 {
            // Not a member of this mailbox — nothing to expunge.
            tx.commit().await.map_err(StoreError::Db)?;
            return Ok(());
        }
        let unread_delta: i64 = if seen { 0 } else { -1 };
        sqlx::query!(
            "UPDATE mailboxes SET total_messages = total_messages - 1, \
             unread_messages = unread_messages + $1 WHERE tenant_id = $2 AND id = $3",
            unread_delta,
            self.tenant().as_str(),
            mailbox.as_str()
        )
        .execute(&mut *tx)
        .await?;

        let remaining = sqlx::query!(
            "SELECT count(*) AS \"n!\" FROM mailbox_messages \
             WHERE tenant_id = $1 AND message_id = $2",
            self.tenant().as_str(),
            message.as_str()
        )
        .fetch_one(&mut *tx)
        .await?
        .n;

        if remaining == 0 {
            sqlx::query!(
                "DELETE FROM messages WHERE tenant_id = $1 AND user_id = $2 AND id = $3",
                self.tenant().as_str(),
                self.user().as_str(),
                message.as_str()
            )
            .execute(&mut *tx)
            .await?;
            self.record(
                &mut tx,
                &[
                    Change::destroyed(TYPE_EMAIL, message.as_str()),
                    Change::updated(TYPE_MAILBOX, mailbox.as_str()),
                ],
            )
            .await?;
        } else {
            self.record(
                &mut tx,
                &[
                    Change::updated(TYPE_EMAIL, message.as_str()),
                    Change::updated(TYPE_MAILBOX, mailbox.as_str()),
                ],
            )
            .await?;
        }
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(())
    }
}
