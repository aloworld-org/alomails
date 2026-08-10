//! `AccountStore` — account isolation by construction.
//!
//! Where a [`TenantStore`](crate::TenantStore) narrows access to one
//! tenant, `AccountStore` narrows it further to one **user** (a JMAP
//! account) within that tenant. It is the **only** door to user-owned
//! rows — mailboxes, messages, threads, keywords, the per-account change
//! log, and the blobs a user's mail references — obtained solely via
//! [`Store::for_account`](crate::Store::for_account).
//!
//! Every statement it issues carries `tenant_id = $tenant` and, for
//! user-owned tables, `user_id = $user` — or, for the tenant-level
//! `threads`/`blobs` tables (which have no `user_id`), an ownership join
//! through the user's own messages. A cross-account access is therefore
//! unrepresentable in the API and returns `NotFound`/empty in the data:
//! the same promise `for_tenant` makes for tenancy, now for accounts,
//! enforced by the type you hold rather than by a caller remembering to
//! call an ownership guard first. See
//! `docs/design/account-scoped-access-door.md`.

use bytes::Bytes;
use sqlx::PgPool;
use time::OffsetDateTime;

use crate::blob::{BlobStore, hash_hex};
use crate::error::{Result, StoreError};
use crate::id::{BlobId, CategoryId, MailboxId, MessageId, TenantId, ThreadId, UserId};
use crate::message;
use crate::model::{
    AiConfigRow, AiProviderRow, Blob, Category, EmailQuery, Mailbox, Message, MessageSummary, Page,
    SortDirection,
};
use crate::store::{MAX_KEYWORD_LEN, MAX_KEYWORDS, SEEN, category_keyword};
use crate::thread;

/// A handle scoped to one `(tenant, user)`. Holds both ids privately and
/// bakes them into every statement; no method accepts a tenant or user
/// argument. Cheap to clone. Construct only via
/// [`Store::for_account`](crate::Store::for_account).
#[derive(Clone)]
pub struct AccountStore {
    pub(crate) pool: PgPool,
    pub(crate) blobs: BlobStore,
    pub(crate) tenant: TenantId,
    pub(crate) user: UserId,
}

impl AccountStore {
    /// The tenant this handle is scoped to.
    pub fn tenant(&self) -> &TenantId {
        &self.tenant
    }

    /// The user (JMAP account) this handle is scoped to.
    pub fn user(&self) -> &UserId {
        &self.user
    }

    // ---- change log ----------------------------------------------------

    /// The account's JMAP/IMAP state token: this account's own monotonic
    /// modseq, which advances only on this account's mutations. A co-tenant
    /// user's activity never moves it, so the token leaks nothing about
    /// another account (see migration `0005`).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn state(&self) -> Result<String> {
        Ok(
            crate::changes::current_state(&self.pool, self.tenant.as_str(), self.user.as_str())
                .await?
                .to_string(),
        )
    }

    /// Computes `/changes` for one object type since `since` (a raw
    /// modseq), bounded by `max`, scoped to this account.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn changes(&self, obj_type: &str, since: i64, max: i64) -> Result<crate::Changes> {
        crate::changes::changes_since(
            &self.pool,
            self.tenant.as_str(),
            self.user.as_str(),
            obj_type,
            since,
            max,
        )
        .await
    }

    /// Records this account's object changes and bumps the tenant modseq
    /// within `tx`.
    pub(crate) async fn record(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        changes: &[crate::changes::Change<'_>],
    ) -> Result<i64> {
        crate::changes::bump_and_record(tx, self.tenant.as_str(), self.user.as_str(), changes).await
    }

    // ---- scoping helpers ----------------------------------------------
    // These fold the old `owns_*`/`assert_*` guards into WHERE clauses so
    // a foreign id is `NotFound` with no separate check to forget.

    /// Confirms a mailbox is this account's; `NotFound` otherwise.
    async fn assert_own_mailbox(&self, mailbox: &MailboxId) -> Result<()> {
        sqlx::query!(
            "SELECT 1 AS one FROM mailboxes WHERE tenant_id = $1 AND user_id = $2 AND id = $3",
            self.tenant.as_str(),
            self.user.as_str(),
            mailbox.as_str()
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or(StoreError::NotFound)
        .map(|_| ())
    }

    /// Locks this account's message row `FOR UPDATE` (also a scoped
    /// existence check). A foreign/other-user message is `NotFound`.
    pub(crate) async fn lock_message(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        message: &MessageId,
    ) -> Result<()> {
        sqlx::query!(
            "SELECT 1 AS one FROM messages \
             WHERE tenant_id = $1 AND user_id = $2 AND id = $3 FOR UPDATE",
            self.tenant.as_str(),
            self.user.as_str(),
            message.as_str()
        )
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(StoreError::NotFound)
        .map(|_| ())
    }

    /// Whether this account's message currently bears `$seen`.
    pub(crate) async fn message_is_seen(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        message: &MessageId,
    ) -> Result<bool> {
        let row = sqlx::query!(
            "SELECT 1 AS one FROM message_keywords \
             WHERE tenant_id = $1 AND message_id = $2 AND keyword = $3",
            self.tenant.as_str(),
            message.as_str(),
            SEEN
        )
        .fetch_optional(&mut **tx)
        .await?;
        Ok(row.is_some())
    }

    /// The mailbox ids this account's message is a member of.
    async fn message_mailbox_ids(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        message: &MessageId,
    ) -> Result<Vec<String>> {
        let rows = sqlx::query!(
            "SELECT mailbox_id FROM mailbox_messages WHERE tenant_id = $1 AND message_id = $2",
            self.tenant.as_str(),
            message.as_str()
        )
        .fetch_all(&mut **tx)
        .await?;
        Ok(rows.into_iter().map(|r| r.mailbox_id).collect())
    }

    // ---- mailboxes -----------------------------------------------------

    /// Creates a mailbox for this account, optionally under `parent` and
    /// with a JMAP `role`.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if `parent` is not this account's;
    /// [`StoreError::Conflict`] on a duplicate sibling name or role.
    pub async fn create_mailbox(
        &self,
        parent: Option<&MailboxId>,
        name: &str,
        role: Option<&str>,
    ) -> Result<MailboxId> {
        if let Some(parent) = parent {
            self.assert_own_mailbox(parent).await?;
        }
        let id = MailboxId::generate();
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        sqlx::query!(
            "INSERT INTO mailboxes (id, tenant_id, user_id, parent_id, name, role) \
             VALUES ($1, $2, $3, $4, $5, $6)",
            id.as_str(),
            self.tenant.as_str(),
            self.user.as_str(),
            parent.map(MailboxId::as_str),
            name,
            role
        )
        .execute(&mut *tx)
        .await?;
        self.record(
            &mut tx,
            &[crate::changes::Change::created(
                crate::changes::TYPE_MAILBOX,
                id.as_str(),
            )],
        )
        .await?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(id)
    }

    /// Gets-or-creates this account's `inbox` role mailbox.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn inbox(&self) -> Result<MailboxId> {
        if let Some(row) = sqlx::query!(
            "SELECT id FROM mailboxes WHERE tenant_id = $1 AND user_id = $2 AND role = 'inbox'",
            self.tenant.as_str(),
            self.user.as_str()
        )
        .fetch_optional(&self.pool)
        .await?
        {
            return Ok(MailboxId::new(row.id));
        }
        self.create_mailbox(None, "Inbox", Some("inbox")).await
    }

    /// Fetches one of this account's mailboxes. Foreign/other-user →
    /// `NotFound`.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if absent or not this account's.
    pub async fn mailbox(&self, id: &MailboxId) -> Result<Mailbox> {
        let row = sqlx::query!(
            "SELECT id, parent_id, name, role, color, total_messages, unread_messages \
             FROM mailboxes WHERE tenant_id = $1 AND user_id = $2 AND id = $3",
            self.tenant.as_str(),
            self.user.as_str(),
            id.as_str()
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or(StoreError::NotFound)?;
        Ok(Mailbox {
            id: MailboxId::new(row.id),
            parent_id: row.parent_id.map(MailboxId::new),
            name: row.name,
            role: row.role,
            color: row.color,
            total_messages: row.total_messages,
            unread_messages: row.unread_messages,
        })
    }

    /// Lists this account's mailboxes (paginated).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn mailboxes(&self, page: Page) -> Result<Vec<Mailbox>> {
        let rows = sqlx::query!(
            "SELECT id, parent_id, name, role, color, total_messages, unread_messages \
             FROM mailboxes WHERE tenant_id = $1 AND user_id = $2 \
             ORDER BY name LIMIT $3 OFFSET $4",
            self.tenant.as_str(),
            self.user.as_str(),
            page.limit(),
            page.offset()
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| Mailbox {
                id: MailboxId::new(row.id),
                parent_id: row.parent_id.map(MailboxId::new),
                name: row.name,
                role: row.role,
                color: row.color,
                total_messages: row.total_messages,
                unread_messages: row.unread_messages,
            })
            .collect())
    }

    /// Sets (or clears, with `None`) the display color of one of this account's
    /// mailboxes. The color string is validated by the caller (a "#rrggbb" hex);
    /// the store only persists it. Runtime query (the new column is not in the
    /// offline cache path for this write).
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the mailbox isn't this account's;
    /// [`StoreError::Db`] on failure.
    pub async fn set_mailbox_color(&self, id: &MailboxId, color: Option<&str>) -> Result<()> {
        let done = sqlx::query(
            "UPDATE mailboxes SET color = $4 \
             WHERE tenant_id = $1 AND user_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(id.as_str())
        .bind(color)
        .execute(&self.pool)
        .await?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Sets (or clears, with `None`) a flagged message's follow-up due-date.
    /// `due` is a Unix epoch (seconds). Runtime query (the column is newer than
    /// the offline cache path). Setting a due-date does not itself flag the
    /// message — the caller sets `$flagged` alongside.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the message isn't this account's;
    /// [`StoreError::Db`] on failure.
    pub async fn set_flag_due(&self, id: &MessageId, due: Option<i64>) -> Result<()> {
        let done = sqlx::query(
            "UPDATE messages \
             SET flag_due = CASE WHEN $4::bigint IS NULL THEN NULL ELSE to_timestamp($4) END \
             WHERE tenant_id = $1 AND user_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(id.as_str())
        .bind(due)
        .execute(&self.pool)
        .await?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// A message's flag due-date, or `None` if unset/absent. Runtime query.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn flag_due(&self, id: &MessageId) -> Result<Option<OffsetDateTime>> {
        let due: Option<Option<OffsetDateTime>> = sqlx::query_scalar(
            "SELECT flag_due FROM messages WHERE tenant_id = $1 AND user_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        Ok(due.flatten())
    }

    // ---- categories (colored message labels) --------------------------

    /// Lists this account's categories, in the user's chosen order (then name).
    /// Runtime query: the `categories` table is newer than the offline cache
    /// path some builds compile against.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn categories(&self) -> Result<Vec<Category>> {
        let rows = sqlx::query_as::<_, (String, String, Option<String>, i32)>(
            "SELECT id, name, color, sort_order FROM categories \
             WHERE tenant_id = $1 AND user_id = $2 ORDER BY sort_order, name",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(id, name, color, sort_order)| Category {
                id: CategoryId::new(id),
                name,
                color,
                sort_order,
            })
            .collect())
    }

    /// Creates a category (colored label) at the end of the user's list.
    /// Returns its id, which is embedded in the `$category_<id>` keyword. The
    /// caller validates `color` (a "#rrggbb" hex); the store only persists it.
    ///
    /// # Errors
    /// [`StoreError::Conflict`] if the user already has a category of that name;
    /// [`StoreError::Db`] on failure.
    pub async fn create_category(&self, name: &str, color: Option<&str>) -> Result<CategoryId> {
        let id = CategoryId::generate();
        sqlx::query(
            "INSERT INTO categories (tenant_id, user_id, id, name, color, sort_order) \
             VALUES ($1, $2, $3, $4, $5, \
                COALESCE((SELECT MAX(sort_order) + 1 FROM categories \
                          WHERE tenant_id = $1 AND user_id = $2), 0))",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(id.as_str())
        .bind(name)
        .bind(color)
        .execute(&self.pool)
        .await?;
        Ok(id)
    }

    /// Renames and/or recolors one of this account's categories. `color` of
    /// `None` clears the color; `Some(c)` sets it (validated by the caller).
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the category isn't this account's;
    /// [`StoreError::Conflict`] on a duplicate name; [`StoreError::Db`] on
    /// failure.
    pub async fn update_category(
        &self,
        id: &CategoryId,
        name: &str,
        color: Option<&str>,
    ) -> Result<()> {
        let done = sqlx::query(
            "UPDATE categories SET name = $4, color = $5 \
             WHERE tenant_id = $1 AND user_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(id.as_str())
        .bind(name)
        .bind(color)
        .execute(&self.pool)
        .await?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Deletes one of this account's categories and strips its `$category_<id>`
    /// keyword from every message that carried it, so no dangling tags remain.
    /// Idempotent per membership: touched messages emit an `Email` change so
    /// clients refresh their chips.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the category isn't this account's;
    /// [`StoreError::Db`] on failure.
    pub async fn delete_category(&self, id: &CategoryId) -> Result<()> {
        let keyword = category_keyword(id);
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;

        let done =
            sqlx::query("DELETE FROM categories WHERE tenant_id = $1 AND user_id = $2 AND id = $3")
                .bind(self.tenant.as_str())
                .bind(self.user.as_str())
                .bind(id.as_str())
                .execute(&mut *tx)
                .await
                .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }

        // Which of this user's messages still carry the tag → change records.
        let tagged: Vec<String> = sqlx::query_scalar(
            "SELECT k.message_id FROM message_keywords k \
             JOIN messages m ON m.id = k.message_id \
             WHERE k.tenant_id = $1 AND m.user_id = $2 AND k.keyword = $3",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(&keyword)
        .fetch_all(&mut *tx)
        .await
        .map_err(StoreError::Db)?;

        sqlx::query(
            "DELETE FROM message_keywords k \
             USING messages m \
             WHERE k.message_id = m.id AND k.tenant_id = $1 AND m.user_id = $2 \
                   AND k.keyword = $3",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(&keyword)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;

        if !tagged.is_empty() {
            let changes: Vec<crate::changes::Change<'_>> = tagged
                .iter()
                .map(|mid| crate::changes::Change::updated(crate::changes::TYPE_EMAIL, mid))
                .collect();
            self.record(&mut tx, &changes).await?;
        }
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(())
    }

    /// Renames one of this account's mailboxes.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if not this account's;
    /// [`StoreError::Conflict`] on a duplicate sibling name.
    pub async fn rename_mailbox(&self, id: &MailboxId, name: &str) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        self.lock_own_mailbox(&mut tx, id).await?;
        sqlx::query!(
            "UPDATE mailboxes SET name = $3 WHERE tenant_id = $1 AND user_id = $2 AND id = $4",
            self.tenant.as_str(),
            self.user.as_str(),
            name,
            id.as_str()
        )
        .execute(&mut *tx)
        .await?;
        self.record(
            &mut tx,
            &[crate::changes::Change::updated(
                crate::changes::TYPE_MAILBOX,
                id.as_str(),
            )],
        )
        .await?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(())
    }

    /// Moves one of this account's mailboxes under a new parent (`None` =
    /// root).
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the mailbox or parent is not this
    /// account's; [`StoreError::Conflict`] if the move would create a
    /// cycle or clash.
    pub async fn move_mailbox(&self, id: &MailboxId, parent: Option<&MailboxId>) -> Result<()> {
        if let Some(parent) = parent {
            self.assert_own_mailbox(parent).await?;
            // Reject any cycle, not just a direct self-parent: walking up
            // from the proposed parent must never reach `id` (which would
            // make `id` its own ancestor — an orphaned, unreachable subtree).
            let creates_cycle = sqlx::query!(
                "WITH RECURSIVE ancestors AS ( \
                   SELECT id, parent_id FROM mailboxes \
                     WHERE tenant_id = $1 AND user_id = $2 AND id = $3 \
                   UNION ALL \
                   SELECT m.id, m.parent_id FROM mailboxes m \
                     JOIN ancestors a ON m.id = a.parent_id \
                     WHERE m.tenant_id = $1 AND m.user_id = $2 \
                 ) \
                 SELECT 1 AS one FROM ancestors WHERE id = $4 LIMIT 1",
                self.tenant.as_str(),
                self.user.as_str(),
                parent.as_str(),
                id.as_str()
            )
            .fetch_optional(&self.pool)
            .await?
            .is_some();
            if creates_cycle {
                return Err(StoreError::Conflict(
                    "mailbox cannot be moved under itself or a descendant".to_owned(),
                ));
            }
        }
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        self.lock_own_mailbox(&mut tx, id).await?;
        sqlx::query!(
            "UPDATE mailboxes SET parent_id = $3 WHERE tenant_id = $1 AND user_id = $2 AND id = $4",
            self.tenant.as_str(),
            self.user.as_str(),
            parent.map(MailboxId::as_str),
            id.as_str()
        )
        .execute(&mut *tx)
        .await?;
        self.record(
            &mut tx,
            &[crate::changes::Change::updated(
                crate::changes::TYPE_MAILBOX,
                id.as_str(),
            )],
        )
        .await?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(())
    }

    /// Destroys one of this account's empty, childless mailboxes.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if not this account's;
    /// [`StoreError::Conflict`] if it still holds messages or children.
    pub async fn destroy_mailbox(&self, id: &MailboxId) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        self.lock_own_mailbox(&mut tx, id).await?;
        let has_child = sqlx::query!(
            "SELECT 1 AS one FROM mailboxes WHERE tenant_id = $1 AND parent_id = $2 LIMIT 1",
            self.tenant.as_str(),
            id.as_str()
        )
        .fetch_optional(&mut *tx)
        .await?
        .is_some();
        if has_child {
            return Err(StoreError::Conflict("mailbox has children".to_owned()));
        }
        let has_email = sqlx::query!(
            "SELECT 1 AS one FROM mailbox_messages WHERE tenant_id = $1 AND mailbox_id = $2 LIMIT 1",
            self.tenant.as_str(),
            id.as_str()
        )
        .fetch_optional(&mut *tx)
        .await?
        .is_some();
        if has_email {
            return Err(StoreError::Conflict("mailbox has emails".to_owned()));
        }
        sqlx::query!(
            "DELETE FROM mailboxes WHERE tenant_id = $1 AND user_id = $2 AND id = $3",
            self.tenant.as_str(),
            self.user.as_str(),
            id.as_str()
        )
        .execute(&mut *tx)
        .await?;
        self.record(
            &mut tx,
            &[crate::changes::Change::destroyed(
                crate::changes::TYPE_MAILBOX,
                id.as_str(),
            )],
        )
        .await?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(())
    }

    /// Locks this account's mailbox row `FOR UPDATE` (scoped existence
    /// check for the mutation paths).
    async fn lock_own_mailbox(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        mailbox: &MailboxId,
    ) -> Result<()> {
        sqlx::query!(
            "SELECT 1 AS one FROM mailboxes \
             WHERE tenant_id = $1 AND user_id = $2 AND id = $3 FOR UPDATE",
            self.tenant.as_str(),
            self.user.as_str(),
            mailbox.as_str()
        )
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(StoreError::NotFound)
        .map(|_| ())
    }

    /// Locks this account's mailbox row `FOR UPDATE` and returns its
    /// `uid_next` — the UID to assign to the next message added. Foreign/
    /// other-user mailbox → `NotFound`.
    async fn lock_own_mailbox_uid(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        mailbox: &MailboxId,
    ) -> Result<i64> {
        let row = sqlx::query!(
            "SELECT uid_next FROM mailboxes \
             WHERE tenant_id = $1 AND user_id = $2 AND id = $3 FOR UPDATE",
            self.tenant.as_str(),
            self.user.as_str(),
            mailbox.as_str()
        )
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(StoreError::NotFound)?;
        Ok(row.uid_next)
    }

    // ---- ingestion -----------------------------------------------------

    /// Delivers a raw message into this account's inbox (the SMTP/
    /// migration/IMAP-APPEND path). Convenience over [`Self::ingest`].
    ///
    /// # Errors
    /// See [`Self::ingest`].
    pub async fn deliver(&self, raw: &[u8]) -> Result<MessageId> {
        let inbox = self.inbox().await?;
        self.ingest(&inbox, raw).await
    }

    /// Ingests a raw message into one of this account's mailboxes:
    /// content-address the bytes to the blob store (first — crash-safety),
    /// then in one transaction thread it, insert the row, add membership,
    /// bump counters, and build the search vector.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if `mailbox` is not this account's;
    /// [`StoreError::TooLarge`] over the blob ceiling; [`StoreError::Db`]/
    /// [`StoreError::Blob`] on failure.
    pub async fn ingest(&self, mailbox: &MailboxId, raw: &[u8]) -> Result<MessageId> {
        self.ingest_at(mailbox, raw, None).await
    }

    /// The subset of `message_ids` (RFC 5322 `Message-ID` header values)
    /// that this account already stores — the IMAP-import dedup check, so
    /// re-running an import does not create duplicates. Empty input →
    /// empty set (no query).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn existing_message_ids(
        &self,
        message_ids: &[String],
    ) -> Result<std::collections::HashSet<String>> {
        if message_ids.is_empty() {
            return Ok(std::collections::HashSet::new());
        }
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT DISTINCT message_id_hdr FROM messages \
             WHERE tenant_id = $1 AND user_id = $2 \
               AND message_id_hdr = ANY($3::text[])",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(message_ids)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(id,)| id).collect())
    }

    /// Like [`Self::ingest`] but with an explicit `received_at` (IMAP
    /// `APPEND`'s optional INTERNALDATE); `None` means now.
    ///
    /// # Errors
    /// See [`Self::ingest`].
    pub(crate) async fn ingest_at(
        &self,
        mailbox: &MailboxId,
        raw: &[u8],
        received_at: Option<OffsetDateTime>,
    ) -> Result<MessageId> {
        // Bound the size before any parse/copy/blob work.
        if raw.len() > self.blobs.max_size() {
            return Err(StoreError::TooLarge {
                size: raw.len(),
                limit: self.blobs.max_size(),
            });
        }
        // The target mailbox must be this account's, before writing a blob.
        self.assert_own_mailbox(mailbox).await?;

        let parsed = message::parse(raw);
        let hash = hash_hex(raw);
        let size = raw.len() as i64;

        // Storage quota (ADR 0012): only genuinely new bytes count — a dedup
        // hit (this tenant already holds these exact bytes) adds nothing.
        let already = self
            .blobs
            .exists(self.tenant.as_str(), &hash)
            .await
            .unwrap_or(false);
        self.check_quota(if already { 0 } else { size }).await?;

        // Crash-safety: the blob is written BEFORE the DB commit. A crash
        // in between leaves an orphan blob no row references — invisible to
        // every tenant, reclaimed by GC — never a visible message with a
        // missing body.
        self.blobs
            .put(self.tenant.as_str(), &hash, Bytes::copy_from_slice(raw))
            .await?;

        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;

        // Upsert the blob row; refcount tracks referencing messages.
        let new_blob_id = BlobId::generate();
        let blob_row = sqlx::query!(
            "INSERT INTO blobs (id, tenant_id, hash, size, refcount, content_type) \
             VALUES ($1, $2, $3, $4, 1, 'message/rfc822') \
             ON CONFLICT (tenant_id, hash) DO UPDATE SET refcount = blobs.refcount + 1 \
             RETURNING id",
            new_blob_id.as_str(),
            self.tenant.as_str(),
            &hash,
            size
        )
        .fetch_one(&mut *tx)
        .await?;
        let blob_id = blob_row.id;

        // Thread: join the thread of any earlier message THIS ACCOUNT holds
        // that we reference, OR an existing copy of this same message (same
        // Message-ID). Threads resolve per-account.
        let (thread_id, thread_created) = self
            .resolve_thread(
                &mut tx,
                &parsed.referenced_ids,
                parsed.message_id.as_deref(),
                &parsed.subject,
            )
            .await?;

        let message_id = MessageId::generate();
        // Cc joins the full-text search corpus (Bcc does not — it is private to
        // the sender's own copy and searching it is not expected).
        let search_text = format!(
            "{} {} {} {} {}",
            parsed.subject, parsed.from_addr, parsed.to_addrs, parsed.cc_addrs, parsed.body_text
        );
        sqlx::query!(
            "INSERT INTO messages \
             (id, tenant_id, user_id, thread_id, blob_id, message_id_hdr, subject, from_addr, \
              to_addrs, cc_addrs, bcc_addrs, has_attachment, sent_at, received_at, size, \
              auth_spf, auth_dkim, auth_dmarc, auth_raw, search) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13, COALESCE($14, now()), \
                     $15,$16,$17,$18,$19, to_tsvector('simple',$20))",
            message_id.as_str(),
            self.tenant.as_str(),
            self.user.as_str(),
            thread_id.as_str(),
            blob_id,
            parsed.message_id.as_deref(),
            parsed.subject,
            parsed.from_addr,
            parsed.to_addrs,
            parsed.cc_addrs,
            parsed.bcc_addrs,
            parsed.has_attachment,
            parsed.sent_at,
            received_at,
            size,
            parsed.auth_spf.as_deref(),
            parsed.auth_dkim.as_deref(),
            parsed.auth_dmarc.as_deref(),
            parsed.auth_raw.as_deref(),
            search_text
        )
        .execute(&mut *tx)
        .await?;

        // Assign this message's per-mailbox UID and bump counters in one
        // locked step (the UPDATE takes a row lock, serializing concurrent
        // deliveries so UIDs never collide or gap-reuse). A fresh message
        // is unread. `uid_next - 1` is the value we just consumed.
        let uid = sqlx::query!(
            "UPDATE mailboxes SET uid_next = uid_next + 1, \
             total_messages = total_messages + 1, unread_messages = unread_messages + 1 \
             WHERE tenant_id = $1 AND id = $2 RETURNING uid_next - 1 AS \"uid!\"",
            self.tenant.as_str(),
            mailbox.as_str()
        )
        .fetch_one(&mut *tx)
        .await?
        .uid;
        sqlx::query!(
            "INSERT INTO mailbox_messages (tenant_id, mailbox_id, message_id, uid) \
             VALUES ($1,$2,$3,$4)",
            self.tenant.as_str(),
            mailbox.as_str(),
            message_id.as_str(),
            uid
        )
        .execute(&mut *tx)
        .await?;

        use crate::changes::{Change, TYPE_EMAIL, TYPE_MAILBOX, TYPE_THREAD};
        let thread_change = if thread_created {
            Change::created(TYPE_THREAD, thread_id.as_str())
        } else {
            Change::updated(TYPE_THREAD, thread_id.as_str())
        };
        self.record(
            &mut tx,
            &[
                Change::created(TYPE_EMAIL, message_id.as_str()),
                thread_change,
                Change::updated(TYPE_MAILBOX, mailbox.as_str()),
            ],
        )
        .await?;

        tx.commit().await.map_err(StoreError::Db)?;
        // Boundary instrumentation — ids and size only, never body/PII.
        tracing::debug!(
            tenant = %self.tenant,
            user = %self.user,
            message = %message_id,
            size,
            "ingested message"
        );
        Ok(message_id)
    }

    /// Resolves the thread for a new message: the thread of the earliest
    /// message THIS ACCOUNT already has that the new message references,
    /// else a fresh thread keyed by base subject. Returns `(thread,
    /// created)`.
    async fn resolve_thread(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        referenced_ids: &[String],
        own_message_id: Option<&str>,
        subject: &str,
    ) -> Result<(ThreadId, bool)> {
        // Match against the ids this message references AND its own Message-ID,
        // so a reply joins its parent's thread and a second copy of the same
        // message (Sent + Inbox copies, a Cc/Bcc-to-self, or a mailing-list
        // echo) joins the existing thread rather than starting a new one.
        let mut keys: Vec<String> = referenced_ids.to_vec();
        if let Some(mid) = own_message_id
            && !mid.is_empty()
            && !keys.iter().any(|k| k == mid)
        {
            keys.push(mid.to_owned());
        }
        if !keys.is_empty() {
            let existing = sqlx::query!(
                "SELECT thread_id FROM messages \
                 WHERE tenant_id = $1 AND user_id = $2 AND message_id_hdr = ANY($3::text[]) \
                 ORDER BY created_at LIMIT 1",
                self.tenant.as_str(),
                self.user.as_str(),
                &keys
            )
            .fetch_optional(&mut **tx)
            .await?;
            if let Some(row) = existing {
                return Ok((ThreadId::new(row.thread_id), false));
            }
        }
        let thread_id = ThreadId::generate();
        sqlx::query!(
            "INSERT INTO threads (id, tenant_id, subject_base) VALUES ($1, $2, $3)",
            thread_id.as_str(),
            self.tenant.as_str(),
            thread::base_subject(subject)
        )
        .execute(&mut **tx)
        .await?;
        Ok((thread_id, true))
    }

    // ---- reading -------------------------------------------------------

    /// Lists one of this account's mailboxes newest-first (paginated). A
    /// foreign mailbox yields an empty list.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn list_mailbox(
        &self,
        mailbox: &MailboxId,
        page: Page,
    ) -> Result<Vec<MessageSummary>> {
        let rows = sqlx::query!(
            "SELECT m.id, m.thread_id, m.subject, m.from_addr, m.sent_at, m.received_at, m.size \
             FROM mailbox_messages mm \
             JOIN messages m ON m.id = mm.message_id AND m.tenant_id = mm.tenant_id \
             WHERE mm.tenant_id = $1 AND mm.mailbox_id = $2 AND m.user_id = $3 \
             ORDER BY mm.added_at DESC LIMIT $4 OFFSET $5",
            self.tenant.as_str(),
            mailbox.as_str(),
            self.user.as_str(),
            page.limit(),
            page.offset()
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| MessageSummary {
                id: MessageId::new(row.id),
                thread_id: ThreadId::new(row.thread_id),
                subject: row.subject,
                from_addr: row.from_addr,
                sent_at: row.sent_at,
                received_at: row.received_at,
                size: row.size,
            })
            .collect())
    }

    /// Fetches one of this account's messages. Foreign/other-user →
    /// `NotFound`.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if absent or not this account's.
    pub async fn message(&self, id: &MessageId) -> Result<Message> {
        let row = sqlx::query!(
            "SELECT id, thread_id, blob_id, message_id_hdr, subject, from_addr, to_addrs, \
             cc_addrs, bcc_addrs, has_attachment, sent_at, received_at, size, auth_spf, \
             auth_dkim, auth_dmarc \
             FROM messages WHERE tenant_id = $1 AND user_id = $2 AND id = $3",
            self.tenant.as_str(),
            self.user.as_str(),
            id.as_str()
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or(StoreError::NotFound)?;
        Ok(Message {
            id: MessageId::new(row.id),
            thread_id: ThreadId::new(row.thread_id),
            blob_id: BlobId::new(row.blob_id),
            message_id_hdr: row.message_id_hdr,
            subject: row.subject,
            from_addr: row.from_addr,
            to_addrs: row.to_addrs,
            cc_addrs: row.cc_addrs,
            bcc_addrs: row.bcc_addrs,
            has_attachment: row.has_attachment,
            sent_at: row.sent_at,
            received_at: row.received_at,
            size: row.size,
            auth_spf: row.auth_spf,
            auth_dkim: row.auth_dkim,
            auth_dmarc: row.auth_dmarc,
        })
    }

    /// Fetches one of this account's messages' raw bytes.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if absent/foreign; [`StoreError::Blob`] on
    /// a blob failure.
    pub async fn message_bytes(&self, id: &MessageId) -> Result<Bytes> {
        let row = sqlx::query!(
            "SELECT b.hash FROM messages m JOIN blobs b ON b.id = m.blob_id \
             WHERE m.tenant_id = $1 AND m.user_id = $2 AND m.id = $3",
            self.tenant.as_str(),
            self.user.as_str(),
            id.as_str()
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or(StoreError::NotFound)?;
        self.blobs.get(self.tenant.as_str(), &row.hash).await
    }

    /// The keywords on one of this account's messages.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn keywords(&self, message: &MessageId) -> Result<Vec<String>> {
        let rows = sqlx::query!(
            "SELECT k.keyword FROM message_keywords k \
             JOIN messages m ON m.id = k.message_id AND m.tenant_id = k.tenant_id \
             WHERE k.tenant_id = $1 AND m.user_id = $2 AND k.message_id = $3 \
             ORDER BY k.keyword",
            self.tenant.as_str(),
            self.user.as_str(),
            message.as_str()
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|r| r.keyword).collect())
    }

    /// The mailbox ids one of this account's messages belongs to.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the message is not this account's.
    pub async fn mailboxes_of_message(&self, message: &MessageId) -> Result<Vec<MailboxId>> {
        // Scoped existence check: the message must be this account's.
        sqlx::query!(
            "SELECT 1 AS one FROM messages WHERE tenant_id = $1 AND user_id = $2 AND id = $3",
            self.tenant.as_str(),
            self.user.as_str(),
            message.as_str()
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or(StoreError::NotFound)?;
        let rows = sqlx::query!(
            "SELECT mailbox_id FROM mailbox_messages WHERE tenant_id = $1 AND message_id = $2",
            self.tenant.as_str(),
            message.as_str()
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| MailboxId::new(r.mailbox_id))
            .collect())
    }

    /// Full-text search over this account's messages, paginated.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn search(&self, query: &str, page: Page) -> Result<Vec<MessageSummary>> {
        let rows = sqlx::query!(
            "SELECT id, thread_id, subject, from_addr, sent_at, received_at, size \
             FROM messages \
             WHERE tenant_id = $1 AND user_id = $2 AND search @@ plainto_tsquery('simple', $3) \
             ORDER BY received_at DESC LIMIT $4 OFFSET $5",
            self.tenant.as_str(),
            self.user.as_str(),
            query,
            page.limit(),
            page.offset()
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| MessageSummary {
                id: MessageId::new(row.id),
                thread_id: ThreadId::new(row.thread_id),
                subject: row.subject,
                from_addr: row.from_addr,
                sent_at: row.sent_at,
                received_at: row.received_at,
                size: row.size,
            })
            .collect())
    }

    /// The message ids in a thread that belong to this account, oldest
    /// first. A thread the account has no message in yields an empty list.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn thread_messages(&self, thread: &ThreadId, page: Page) -> Result<Vec<MessageId>> {
        let rows = sqlx::query!(
            "SELECT id FROM messages WHERE tenant_id = $1 AND user_id = $2 AND thread_id = $3 \
             ORDER BY created_at LIMIT $4 OFFSET $5",
            self.tenant.as_str(),
            self.user.as_str(),
            thread.as_str(),
            page.limit(),
            page.offset()
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|r| MessageId::new(r.id)).collect())
    }

    // ---- flags ---------------------------------------------------------

    /// Sets or clears a keyword on one of this account's messages,
    /// maintaining every containing mailbox's unread counter
    /// transactionally.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the message is not this account's;
    /// [`StoreError::Conflict`] on the keyword length/count caps.
    pub async fn set_keyword(&self, message: &MessageId, keyword: &str, on: bool) -> Result<()> {
        if on && keyword.len() > MAX_KEYWORD_LEN {
            return Err(StoreError::Conflict("keyword too long".to_owned()));
        }
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        // Scoped lock: existence + account check + serialization against
        // add/remove_from_mailbox so the unread delta is never stale.
        self.lock_message(&mut tx, message).await?;

        let changed = if on {
            let affected = sqlx::query!(
                "INSERT INTO message_keywords (tenant_id, message_id, keyword) VALUES ($1,$2,$3) \
                 ON CONFLICT DO NOTHING",
                self.tenant.as_str(),
                message.as_str(),
                keyword
            )
            .execute(&mut *tx)
            .await?
            .rows_affected();
            if affected == 1 {
                let count = sqlx::query!(
                    "SELECT count(*) AS n FROM message_keywords \
                     WHERE tenant_id = $1 AND message_id = $2",
                    self.tenant.as_str(),
                    message.as_str()
                )
                .fetch_one(&mut *tx)
                .await?
                .n
                .unwrap_or(0);
                if count > MAX_KEYWORDS {
                    return Err(StoreError::Conflict("too many keywords".to_owned()));
                }
            }
            affected
        } else {
            sqlx::query!(
                "DELETE FROM message_keywords WHERE tenant_id = $1 AND message_id = $2 AND keyword = $3",
                self.tenant.as_str(),
                message.as_str(),
                keyword
            )
            .execute(&mut *tx)
            .await?
            .rows_affected()
        };

        // Only $seen moves the unread counter, and only when the keyword
        // actually changed (rows_affected == 1).
        if keyword == SEEN && changed == 1 {
            let delta: i64 = if on { -1 } else { 1 };
            sqlx::query!(
                "UPDATE mailboxes SET unread_messages = unread_messages + $1 \
                 WHERE tenant_id = $2 AND id IN \
                 (SELECT mailbox_id FROM mailbox_messages WHERE tenant_id = $2 AND message_id = $3)",
                delta,
                self.tenant.as_str(),
                message.as_str()
            )
            .execute(&mut *tx)
            .await?;
        }

        if changed == 1 {
            use crate::changes::{Change, TYPE_EMAIL, TYPE_MAILBOX};
            let mut records = vec![Change::updated(TYPE_EMAIL, message.as_str())];
            let mailbox_ids = if keyword == SEEN {
                self.message_mailbox_ids(&mut tx, message).await?
            } else {
                Vec::new()
            };
            for mb in &mailbox_ids {
                records.push(Change::updated(TYPE_MAILBOX, mb));
            }
            self.record(&mut tx, &records).await?;
        }

        tx.commit().await.map_err(StoreError::Db)?;
        Ok(())
    }

    /// Adds one of this account's messages to one of its mailboxes
    /// (idempotent), bumping counters when it was not already a member.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if either is not this account's.
    pub async fn add_to_mailbox(&self, message: &MessageId, mailbox: &MailboxId) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        // Lock the destination mailbox (also the ownership check → NotFound)
        // so the UID we read is the one we assign, serialized against other
        // adders/deliveries. Then lock the message (ownership + serialize).
        let candidate_uid = self.lock_own_mailbox_uid(&mut tx, mailbox).await?;
        self.lock_message(&mut tx, message).await?;
        let seen = self.message_is_seen(&mut tx, message).await?;
        let added = sqlx::query!(
            "INSERT INTO mailbox_messages (tenant_id, mailbox_id, message_id, uid) \
             SELECT $1, $2, id, $5 FROM messages WHERE tenant_id = $1 AND user_id = $4 AND id = $3 \
             ON CONFLICT DO NOTHING",
            self.tenant.as_str(),
            mailbox.as_str(),
            message.as_str(),
            self.user.as_str(),
            candidate_uid
        )
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if added == 1 {
            let unread_delta: i64 = if seen { 0 } else { 1 };
            sqlx::query!(
                "UPDATE mailboxes SET uid_next = uid_next + 1, \
                 total_messages = total_messages + 1, \
                 unread_messages = unread_messages + $1 WHERE tenant_id = $2 AND id = $3",
                unread_delta,
                self.tenant.as_str(),
                mailbox.as_str()
            )
            .execute(&mut *tx)
            .await?;
            use crate::changes::{Change, TYPE_EMAIL, TYPE_MAILBOX};
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

    /// Removes one of this account's messages from one of its mailboxes,
    /// adjusting counters when it was a member.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if either is not this account's.
    pub async fn remove_from_mailbox(
        &self,
        message: &MessageId,
        mailbox: &MailboxId,
    ) -> Result<()> {
        self.assert_own_mailbox(mailbox).await?;
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        self.lock_message(&mut tx, message).await?;
        let seen = self.message_is_seen(&mut tx, message).await?;
        let removed = sqlx::query!(
            "DELETE FROM mailbox_messages WHERE tenant_id = $1 AND mailbox_id = $2 AND message_id = $3",
            self.tenant.as_str(),
            mailbox.as_str(),
            message.as_str()
        )
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if removed == 1 {
            let unread_delta: i64 = if seen { 0 } else { -1 };
            sqlx::query!(
                "UPDATE mailboxes SET total_messages = total_messages - 1, \
                 unread_messages = unread_messages + $1 WHERE tenant_id = $2 AND id = $3",
                unread_delta,
                self.tenant.as_str(),
                mailbox.as_str()
            )
            .execute(&mut *tx)
            .await?;
            use crate::changes::{Change, TYPE_EMAIL, TYPE_MAILBOX};
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

    // ---- Email/query ---------------------------------------------------

    /// `Email/query`: filters + `receivedAt` sort + bounded page, scoped
    /// to this account.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn query_emails(&self, q: &EmailQuery) -> Result<Vec<MessageSummary>> {
        let f = &q.filter;
        let in_mailbox = f.in_mailbox.as_ref().map(MailboxId::as_str);
        let rows = match q.sort {
            SortDirection::Desc => sqlx::query!(
                r#"SELECT DISTINCT m.id, m.thread_id, m.subject, m.from_addr, m.sent_at,
                              m.received_at, m.size
                       FROM messages m
                       LEFT JOIN mailbox_messages mm
                         ON mm.message_id = m.id AND mm.tenant_id = m.tenant_id
                       WHERE m.tenant_id = $1 AND m.user_id = $2
                         AND ($3::text IS NULL OR mm.mailbox_id = $3)
                         AND ($4::text IS NULL OR m.from_addr ILIKE '%' || $4 || '%')
                         AND ($5::text IS NULL OR m.to_addrs ILIKE '%' || $5 || '%')
                         AND ($6::text IS NULL OR m.subject ILIKE '%' || $6 || '%')
                         AND ($7::text IS NULL OR m.search @@ plainto_tsquery('simple', $7))
                         AND ($8::timestamptz IS NULL OR m.received_at < $8)
                         AND ($9::timestamptz IS NULL OR m.received_at >= $9)
                         AND ($10::text IS NULL OR EXISTS
                              (SELECT 1 FROM message_keywords k
                               WHERE k.message_id = m.id AND k.keyword = $10))
                         AND ($11::text IS NULL OR NOT EXISTS
                              (SELECT 1 FROM message_keywords k
                               WHERE k.message_id = m.id AND k.keyword = $11))
                       ORDER BY m.received_at DESC, m.id DESC
                       LIMIT $12 OFFSET $13"#,
                self.tenant.as_str(),
                self.user.as_str(),
                in_mailbox,
                f.from,
                f.to,
                f.subject,
                f.text,
                f.before,
                f.after,
                f.has_keyword,
                f.not_keyword,
                q.page.limit(),
                q.page.offset()
            )
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(|r| MessageSummary {
                id: MessageId::new(r.id),
                thread_id: ThreadId::new(r.thread_id),
                subject: r.subject,
                from_addr: r.from_addr,
                sent_at: r.sent_at,
                received_at: r.received_at,
                size: r.size,
            })
            .collect(),
            SortDirection::Asc => sqlx::query!(
                r#"SELECT DISTINCT m.id, m.thread_id, m.subject, m.from_addr, m.sent_at,
                              m.received_at, m.size
                       FROM messages m
                       LEFT JOIN mailbox_messages mm
                         ON mm.message_id = m.id AND mm.tenant_id = m.tenant_id
                       WHERE m.tenant_id = $1 AND m.user_id = $2
                         AND ($3::text IS NULL OR mm.mailbox_id = $3)
                         AND ($4::text IS NULL OR m.from_addr ILIKE '%' || $4 || '%')
                         AND ($5::text IS NULL OR m.to_addrs ILIKE '%' || $5 || '%')
                         AND ($6::text IS NULL OR m.subject ILIKE '%' || $6 || '%')
                         AND ($7::text IS NULL OR m.search @@ plainto_tsquery('simple', $7))
                         AND ($8::timestamptz IS NULL OR m.received_at < $8)
                         AND ($9::timestamptz IS NULL OR m.received_at >= $9)
                         AND ($10::text IS NULL OR EXISTS
                              (SELECT 1 FROM message_keywords k
                               WHERE k.message_id = m.id AND k.keyword = $10))
                         AND ($11::text IS NULL OR NOT EXISTS
                              (SELECT 1 FROM message_keywords k
                               WHERE k.message_id = m.id AND k.keyword = $11))
                       ORDER BY m.received_at ASC, m.id ASC
                       LIMIT $12 OFFSET $13"#,
                self.tenant.as_str(),
                self.user.as_str(),
                in_mailbox,
                f.from,
                f.to,
                f.subject,
                f.text,
                f.before,
                f.after,
                f.has_keyword,
                f.not_keyword,
                q.page.limit(),
                q.page.offset()
            )
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(|r| MessageSummary {
                id: MessageId::new(r.id),
                thread_id: ThreadId::new(r.thread_id),
                subject: r.subject,
                from_addr: r.from_addr,
                sent_at: r.sent_at,
                received_at: r.received_at,
                size: r.size,
            })
            .collect(),
        };
        Ok(rows)
    }

    /// Destroys one of this account's messages everywhere: adjusts every
    /// containing mailbox's counters, deletes the row (membership/keywords
    /// cascade), and records the Email tombstone plus Mailbox updates.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the message is not this account's.
    pub async fn destroy_message(&self, message: &MessageId) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        self.lock_message(&mut tx, message).await?;
        let seen = self.message_is_seen(&mut tx, message).await?;
        let mailbox_ids = self.message_mailbox_ids(&mut tx, message).await?;
        for mb in &mailbox_ids {
            let unread_delta: i64 = if seen { 0 } else { -1 };
            sqlx::query!(
                "UPDATE mailboxes SET total_messages = total_messages - 1, \
                 unread_messages = unread_messages + $1 WHERE tenant_id = $2 AND id = $3",
                unread_delta,
                self.tenant.as_str(),
                mb
            )
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query!(
            "DELETE FROM messages WHERE tenant_id = $1 AND user_id = $2 AND id = $3",
            self.tenant.as_str(),
            self.user.as_str(),
            message.as_str()
        )
        .execute(&mut *tx)
        .await?;
        use crate::changes::{Change, TYPE_EMAIL, TYPE_MAILBOX};
        let mut records = vec![Change::destroyed(TYPE_EMAIL, message.as_str())];
        for mb in &mailbox_ids {
            records.push(Change::updated(TYPE_MAILBOX, mb));
        }
        self.record(&mut tx, &records).await?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(())
    }

    // ---- blobs (JMAP upload/download) ---------------------------------

    /// Stores an uploaded blob (content-addressed) and returns its id.
    /// Blobs are deduplicated per tenant; the account gains access to
    /// download it once one of its messages references it (see
    /// [`Self::blob_bytes`]).
    ///
    /// # Errors
    /// [`StoreError::TooLarge`] over the ceiling; [`StoreError::Db`]/
    /// [`StoreError::Blob`] on failure.
    pub async fn put_blob(&self, bytes: Bytes, content_type: Option<&str>) -> Result<BlobId> {
        if bytes.len() > self.blobs.max_size() {
            return Err(StoreError::TooLarge {
                size: bytes.len(),
                limit: self.blobs.max_size(),
            });
        }
        let hash = hash_hex(&bytes);
        let size = bytes.len() as i64;
        let already = self
            .blobs
            .exists(self.tenant.as_str(), &hash)
            .await
            .unwrap_or(false);
        self.check_quota(if already { 0 } else { size }).await?;
        self.blobs.put(self.tenant.as_str(), &hash, bytes).await?;
        let new_id = BlobId::generate();
        let row = sqlx::query!(
            "INSERT INTO blobs (id, tenant_id, hash, size, refcount, content_type) \
             VALUES ($1, $2, $3, $4, 0, $5) \
             ON CONFLICT (tenant_id, hash) DO UPDATE SET content_type = COALESCE($5, blobs.content_type) \
             RETURNING id",
            new_id.as_str(),
            self.tenant.as_str(),
            &hash,
            size,
            content_type
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(BlobId::new(row.id))
    }

    /// Reject a write of `incoming` new bytes if it would push the tenant over
    /// its storage quota (ADR 0012). A `NULL` quota means unlimited — the
    /// default — in which case this returns immediately after one cheap indexed
    /// lookup, so tenants without a cap pay effectively nothing. Runtime query
    /// (the quota column post-dates the offline cache).
    ///
    /// # Errors
    /// [`StoreError::OverQuota`] if the write would exceed the cap;
    /// [`StoreError::Db`] on failure.
    /// This tenant's storage usage and cap in octets — `(used, limit)`, where a
    /// `None` limit means unlimited (ADR 0012). Backs the JMAP `Quota/get`.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn storage_usage(&self) -> Result<(i64, Option<i64>)> {
        let limit: Option<i64> =
            sqlx::query_scalar("SELECT storage_quota_bytes FROM tenants WHERE id = $1")
                .bind(self.tenant.as_str())
                .fetch_optional(&self.pool)
                .await?
                .flatten();
        let used: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(size), 0)::bigint FROM blobs WHERE tenant_id = $1",
        )
        .bind(self.tenant.as_str())
        .fetch_one(&self.pool)
        .await?;
        Ok((used, limit))
    }

    async fn check_quota(&self, incoming: i64) -> Result<()> {
        if incoming <= 0 {
            return Ok(());
        }
        let quota: Option<i64> =
            sqlx::query_scalar("SELECT storage_quota_bytes FROM tenants WHERE id = $1")
                .bind(self.tenant.as_str())
                .fetch_optional(&self.pool)
                .await?
                .flatten();
        let Some(limit) = quota else {
            return Ok(()); // unlimited
        };
        let used: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(size), 0)::bigint FROM blobs WHERE tenant_id = $1",
        )
        .bind(self.tenant.as_str())
        .fetch_one(&self.pool)
        .await?;
        if used + incoming > limit {
            return Err(StoreError::OverQuota);
        }
        Ok(())
    }

    /// A blob's metadata, accessible to this account only if one of its
    /// messages references the blob. Foreign/unreferenced → `NotFound`.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if absent or not referenced by this account.
    pub async fn blob(&self, id: &BlobId) -> Result<Blob> {
        let row = sqlx::query!(
            "SELECT b.id, b.size, b.content_type FROM blobs b \
             WHERE b.tenant_id = $1 AND b.id = $2 AND EXISTS \
             (SELECT 1 FROM messages m \
              WHERE m.tenant_id = $1 AND m.user_id = $3 AND m.blob_id = b.id)",
            self.tenant.as_str(),
            id.as_str(),
            self.user.as_str()
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or(StoreError::NotFound)?;
        Ok(Blob {
            id: BlobId::new(row.id),
            size: row.size,
            content_type: row.content_type,
        })
    }

    /// A blob's bytes, accessible to this account only if one of its
    /// messages references the blob. Foreign/unreferenced → `NotFound`.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if absent/not-referenced; [`StoreError::Blob`]
    /// on a blob-store failure.
    pub async fn blob_bytes(&self, id: &BlobId) -> Result<Bytes> {
        let row = sqlx::query!(
            "SELECT b.hash FROM blobs b \
             WHERE b.tenant_id = $1 AND b.id = $2 AND EXISTS \
             (SELECT 1 FROM messages m \
              WHERE m.tenant_id = $1 AND m.user_id = $3 AND m.blob_id = b.id)",
            self.tenant.as_str(),
            id.as_str(),
            self.user.as_str()
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or(StoreError::NotFound)?;
        self.blobs.get(self.tenant.as_str(), &row.hash).await
    }

    /// A blob's bytes by id, scoped to this account's tenant but WITHOUT
    /// requiring a referencing message — for resolving a just-uploaded
    /// attachment while assembling an outgoing message, since an upload has
    /// refcount 0 (no message references it yet) until the draft that embeds it
    /// is created. Tenant isolation still holds: the lookup is keyed by tenant
    /// and blob ids are unguessable. (Runtime-checked query: this lookup is not
    /// in the offline `.sqlx` cache, deliberately kept simple.)
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if absent in this tenant; [`StoreError::Blob`]
    /// on a blob-store failure.
    pub async fn blob_bytes_for_send(&self, id: &BlobId) -> Result<Bytes> {
        let hash: Option<String> =
            sqlx::query_scalar("SELECT hash FROM blobs WHERE tenant_id = $1 AND id = $2")
                .bind(self.tenant.as_str())
                .bind(id.as_str())
                .fetch_optional(&self.pool)
                .await?;
        let hash = hash.ok_or(StoreError::NotFound)?;
        self.blobs.get(self.tenant.as_str(), &hash).await
    }

    /// Whether the signed-in user is a tenant admin (ADR: admin console). Gates
    /// admin-only surfaces. Runtime-checked query.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn is_admin(&self) -> Result<bool> {
        let v: Option<bool> =
            sqlx::query_scalar("SELECT is_admin FROM users WHERE tenant_id = $1 AND id = $2")
                .bind(self.tenant.as_str())
                .bind(self.user.as_str())
                .fetch_optional(&self.pool)
                .await?;
        Ok(v.unwrap_or(false))
    }

    /// The enabled default AI provider mapped for the inference client, or
    /// `None` if the tenant has no enabled default (AI is then off). Runtime
    /// query. (ADR 0011)
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn default_ai_config(&self) -> Result<Option<AiConfigRow>> {
        let row = sqlx::query_as::<_, (String, String, Option<String>, bool)>(
            "SELECT base_url, model, api_key, enabled FROM ai_providers \
             WHERE tenant_id = $1 AND is_default AND enabled LIMIT 1",
        )
        .bind(self.tenant.as_str())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(base_url, model, api_key, enabled)| AiConfigRow {
            base_url,
            // A provider may enable several models (stored comma-separated); the
            // first is the active model the AI features request.
            model: model
                .split(',')
                .next()
                .map(str::trim)
                .unwrap_or("")
                .to_owned(),
            api_key,
            enabled,
        }))
    }

    /// All AI providers configured for this tenant (admin console). Runtime
    /// query. The `api_key` is included for the caller to redact — it is never
    /// serialized to clients.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn list_ai_providers(&self) -> Result<Vec<AiProviderRow>> {
        let rows = sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                String,
                String,
                Option<String>,
                bool,
                bool,
            ),
        >(
            "SELECT id, kind, label, base_url, model, api_key, enabled, is_default \
             FROM ai_providers WHERE tenant_id = $1 ORDER BY updated_at",
        )
        .bind(self.tenant.as_str())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(
                |(id, kind, label, base_url, model, api_key, enabled, is_default)| AiProviderRow {
                    id,
                    kind,
                    label,
                    base_url,
                    model,
                    api_key,
                    enabled,
                    is_default,
                },
            )
            .collect())
    }

    /// Insert or update an AI provider (admin write). On update, a `None`
    /// `api_key` keeps the stored key. The `WHERE tenant_id` guard on the
    /// conflict update makes a foreign id a no-op, never a cross-tenant write.
    /// Runtime query.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_ai_provider(
        &self,
        id: &str,
        kind: &str,
        label: &str,
        base_url: &str,
        model: &str,
        api_key: Option<&str>,
        enabled: bool,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO ai_providers (id, tenant_id, kind, label, base_url, model, api_key, enabled) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
             ON CONFLICT (id) DO UPDATE SET \
               kind = $3, label = $4, base_url = $5, model = $6, \
               api_key = COALESCE($7, ai_providers.api_key), enabled = $8, updated_at = now() \
             WHERE ai_providers.tenant_id = $2",
        )
        .bind(id)
        .bind(self.tenant.as_str())
        .bind(kind)
        .bind(label)
        .bind(base_url)
        .bind(model)
        .bind(api_key)
        .bind(enabled)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Delete an AI provider (admin write), tenant-scoped.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn delete_ai_provider(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM ai_providers WHERE tenant_id = $1 AND id = $2")
            .bind(self.tenant.as_str())
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Make one provider the tenant's single default (admin write). Clears any
    /// existing default first, in a transaction, then sets this one. A stale or
    /// foreign `id` matches no row: rather than silently leaving the tenant with
    /// no default (which switches AI off), the whole operation rolls back and
    /// returns [`StoreError::NotFound`], preserving the prior default.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if no provider with `id` exists for this tenant;
    /// [`StoreError::Db`] on failure.
    pub async fn set_default_ai_provider(&self, id: &str) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        sqlx::query("UPDATE ai_providers SET is_default = FALSE WHERE tenant_id = $1")
            .bind(self.tenant.as_str())
            .execute(&mut *tx)
            .await?;
        let set = sqlx::query(
            "UPDATE ai_providers SET is_default = TRUE WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id)
        .execute(&mut *tx)
        .await?;
        if set.rows_affected() == 0 {
            // tx drops without commit → the clear above is rolled back too.
            return Err(StoreError::NotFound);
        }
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(())
    }
}

impl AccountStore {
    /// The IANA timezone this person reads clocks in, if it is known.
    ///
    /// `None` means nobody has told us. That is deliberately different from a
    /// default: an agent that assumes a zone puts meetings an hour out twice a
    /// year, and silently.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn user_timezone(&self) -> Result<Option<String>> {
        let row: Option<(Option<String>,)> =
            sqlx::query_as("SELECT timezone FROM users WHERE tenant_id = $1 AND id = $2")
                .bind(self.tenant.as_str())
                .bind(self.user.as_str())
                .fetch_optional(&self.pool)
                .await
                .map_err(StoreError::Db)?;
        Ok(row.and_then(|(tz,)| tz).filter(|tz| !tz.trim().is_empty()))
    }

    /// Remember the zone this person's browser reports.
    ///
    /// Written on sight rather than asked for in a settings page: the browser
    /// already knows, and a preference nobody is prompted for is one that is
    /// right far more often than one they have to find.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn set_user_timezone(&self, tz: &str) -> Result<()> {
        let tz = tz.trim();
        // A zone is an IANA name; anything else is a caller's bug and must not
        // be stored, because a stored wrong zone is worse than no zone.
        if tz.is_empty() || tz.len() > 64 || !tz.contains('/') {
            return Ok(());
        }
        sqlx::query("UPDATE users SET timezone = $3 WHERE tenant_id = $1 AND id = $2")
            .bind(self.tenant.as_str())
            .bind(self.user.as_str())
            .bind(tz)
            .execute(&self.pool)
            .await
            .map_err(StoreError::Db)?;
        Ok(())
    }
}
