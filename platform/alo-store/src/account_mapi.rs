//! MAPI data-access support on [`AccountStore`], kept out of `account.rs`
//! (Law 3: its reason to change is the MAPI adapter's needs, not the JMAP
//! core's). Everything here is still account-scoped by construction — the same
//! `(tenant, user)` predicate the rest of `AccountStore` carries — so the
//! adapter inherits isolation rather than re-implementing it.
//!
//! ## Why this is not `list_mailbox` or `imap_view`
//!
//! A MAPI contents-table row needs seven facts about a message, and no
//! existing read returns all of them:
//!
//! * [`AccountStore::list_mailbox`] is bounded and ordered but carries neither
//!   the read state nor whether there is an attachment — the two things a
//!   message list makes most visible, since one decides whether a row is bold
//!   and the other whether it shows a paperclip.
//! * `imap_view` and `imap_search_rows` carry the flags but are **unbounded**:
//!   they read a whole mailbox. IMAP can afford that because a session holds
//!   one mailbox open; a MAPI `Execute` is a single HTTP request that must not
//!   grow with the size of somebody's inbox.
//!
//! So this is one bounded query returning exactly the row, rather than three
//! reads stitched together — which would also be three chances for the flags
//! and the summary to disagree about which messages are in the mailbox.

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::error::Result;
use crate::id::{MailboxId, MessageId};
use crate::model::Page;

/// One message as a MAPI contents table lists it.
///
/// Ordered newest-first by the time the message entered the mailbox, which is
/// the order a mail client shows by default and the same order
/// [`AccountStore::list_mailbox`] uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapiMessageRow {
    /// Opaque message id.
    pub id: MessageId,
    /// Unfolded subject.
    pub subject: String,
    /// Unfolded `From`.
    pub from_addr: String,
    /// When the store received it.
    pub received_at: OffsetDateTime,
    /// Size of the raw message in octets.
    pub size: i64,
    /// Whether this account's copy currently bears `$seen`.
    pub seen: bool,
    /// Whether the message carries an attachment.
    ///
    /// `false` covers both "no attachment" and "not yet computed" — the store
    /// backfills the column, and an unfilled row is reported as carrying none.
    /// A missing paperclip on a message that has one is a smaller wrong than a
    /// paperclip on a message that does not, which sends somebody looking for
    /// a file that was never there.
    pub has_attachment: bool,
}

/// One directory entry a typed name resolved to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapiDirectoryEntry {
    /// What a client displays.
    pub display_name: String,
    /// The address a message would be sent to.
    pub email: String,
}

impl AccountStore {
    /// Directory entries whose name or address matches `needle`, bounded.
    ///
    /// Two sources, and both are scoped by construction:
    ///
    /// * **The tenant's own people**, matched on their address. alo has no
    ///   display-name column on a user, so a colleague's display name *is*
    ///   their address — which is truthful, where a name invented by
    ///   prettifying the local part would not be.
    /// * **This account's contacts**, matched on display name or address.
    ///   These are where the names somebody actually types live.
    ///
    /// The match is a case-insensitive substring, which is what ambiguous name
    /// resolution means to a person: they type three letters and expect the
    /// colleague. It is deliberately not a prefix match — "müller" should find
    /// "Anna Müller".
    ///
    /// # Errors
    /// [`crate::StoreError::Db`] on failure.
    pub async fn mapi_resolve(&self, needle: &str, limit: i64) -> Result<Vec<MapiDirectoryEntry>> {
        // An empty needle would match the whole directory, which is a browse
        // rather than a resolve, and browsing has its own operations.
        if needle.trim().is_empty() {
            return Ok(Vec::new());
        }
        let pattern = format!("%{}%", needle.trim().to_lowercase());
        let rows = sqlx::query!(
            "SELECT display_name, email FROM ( \
               SELECT u.email AS display_name, u.email AS email, 0 AS rank \
                 FROM users u \
                WHERE u.tenant_id = $1 AND lower(u.email) LIKE $2 \
               UNION \
               SELECT c.display_name AS display_name, \
                      (c.emails -> 0 ->> 'email') AS email, 1 AS rank \
                 FROM contacts c \
                WHERE c.tenant_id = $1 AND c.user_id = $3 \
                  AND (lower(c.display_name) LIKE $2 \
                       OR lower(coalesce(c.emails -> 0 ->> 'email', '')) LIKE $2) \
             ) matches \
             WHERE email IS NOT NULL AND email <> '' \
             ORDER BY rank, display_name LIMIT $4",
            self.tenant().as_str(),
            pattern,
            self.user().as_str(),
            limit
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .filter_map(|row| {
                Some(MapiDirectoryEntry {
                    display_name: row.display_name?,
                    email: row.email?,
                })
            })
            .collect())
    }

    /// A bounded, newest-first page of one of this account's mailboxes, with
    /// everything a MAPI contents-table row carries. A foreign mailbox yields
    /// an empty list.
    ///
    /// # Errors
    /// [`crate::StoreError::Db`] on failure.
    pub async fn mapi_mailbox_rows(
        &self,
        mailbox: &MailboxId,
        page: Page,
    ) -> Result<Vec<MapiMessageRow>> {
        let rows = sqlx::query!(
            "SELECT m.id, m.subject, m.from_addr, m.received_at, m.size, \
                    COALESCE(m.has_attachment, false) AS \"has_attachment!: bool\", \
                    EXISTS ( \
                      SELECT 1 FROM message_keywords k \
                      WHERE k.tenant_id = m.tenant_id AND k.message_id = m.id \
                        AND k.keyword = $6 \
                    ) AS \"seen!: bool\" \
             FROM mailbox_messages mm \
             JOIN messages m ON m.id = mm.message_id AND m.tenant_id = mm.tenant_id \
             WHERE mm.tenant_id = $1 AND mm.mailbox_id = $2 AND m.user_id = $3 \
             ORDER BY mm.added_at DESC LIMIT $4 OFFSET $5",
            self.tenant().as_str(),
            mailbox.as_str(),
            self.user().as_str(),
            page.limit(),
            page.offset(),
            crate::store::SEEN
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| MapiMessageRow {
                id: MessageId::new(row.id),
                subject: row.subject,
                from_addr: row.from_addr,
                received_at: row.received_at,
                size: row.size,
                seen: row.seen,
                has_attachment: row.has_attachment,
            })
            .collect())
    }
}
