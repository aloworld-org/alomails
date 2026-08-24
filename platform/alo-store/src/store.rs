//! `Store`, `TenantStore` — tenancy by construction; the account door.
//!
//! `Store` does system operations only (tenants, migrations, auth).
//! **Tenant-level** operations — user provisioning and lookup — go
//! through a [`TenantStore`] ([`Store::for_tenant`]). **User-owned mail
//! data** (mailboxes, messages, threads, keywords, blobs, the change
//! log) is reachable only through an [`AccountStore`](crate::AccountStore)
//! ([`Store::for_account`]), which bakes `(tenant, user)` into every
//! statement so cross-account access is unrepresentable (see
//! `docs/design/account-scoped-access-door.md`).

use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

use crate::blob::BlobStore;
use crate::error::{Result, StoreError};
use crate::id::{TenantId, UserId};

/// The JMAP `$seen` keyword — the one that drives the unread counter.
pub const SEEN: &str = "$seen";

/// Keyword prefix that records category membership: a message tagged with
/// category `<id>` carries the keyword `$category_<id>`. The store owns this
/// convention (used to strip tags when a category is deleted); clients derive
/// the same keyword to set, clear, and filter by a category.
pub const CATEGORY_KEYWORD_PREFIX: &str = "$category_";

/// The keyword recording membership in category `id`.
#[must_use]
pub fn category_keyword(id: &crate::id::CategoryId) -> String {
    format!("{CATEGORY_KEYWORD_PREFIX}{}", id.as_str())
}

/// Maximum distinct keywords per message — bounds `message_keywords`
/// growth so one message cannot force an unbounded keyword set.
pub(crate) const MAX_KEYWORDS: i64 = 64;
/// Maximum length of a single keyword.
pub(crate) const MAX_KEYWORD_LEN: usize = 128;

/// The process-wide store handle: a Postgres pool plus a blob backend.
/// Its public API exposes system operations only — nothing about
/// tenant-owned rows.
#[derive(Clone)]
pub struct Store {
    pool: PgPool,
    blobs: BlobStore,
}

impl Store {
    /// Connects a pool to `database_url` and attaches `blobs`.
    ///
    /// # Errors
    /// [`StoreError::Db`] if the pool cannot connect.
    pub async fn connect(database_url: &str, blobs: BlobStore) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(16)
            .connect(database_url)
            .await
            .map_err(StoreError::Db)?;
        Ok(Self { pool, blobs })
    }

    /// Wraps an existing pool (used by tests that share one).
    pub fn new(pool: PgPool, blobs: BlobStore) -> Self {
        Self { pool, blobs }
    }

    /// Applies pending schema migrations.
    ///
    /// # Errors
    /// [`StoreError::Migrate`] on a failed migration.
    pub async fn migrate(&self) -> Result<()> {
        sqlx::migrate!("./migrations").run(&self.pool).await?;
        Ok(())
    }

    /// The newest successfully applied schema migration.
    ///
    /// This is process-level operational metadata, not tenant data. Readiness
    /// uses it to prove that the binary and database agree before traffic is
    /// sent to the service.
    ///
    /// # Errors
    /// [`StoreError::Db`] if the migration ledger cannot be read.
    pub async fn migration_version(&self) -> Result<i64> {
        let version = sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(MAX(version), 0)::BIGINT \
             FROM _sqlx_migrations WHERE success = TRUE",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(version)
    }

    /// Creates a tenant, returning its opaque id.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn create_tenant(&self, name: &str) -> Result<TenantId> {
        let id = TenantId::generate();
        sqlx::query!(
            "INSERT INTO tenants (id, name) VALUES ($1, $2)",
            id.as_str(),
            name
        )
        .execute(&self.pool)
        .await?;
        Ok(id)
    }

    /// Whether a tenant exists (a system lookup, not tenant data).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn tenant_exists(&self, tenant: &TenantId) -> Result<bool> {
        let row = sqlx::query!(
            "SELECT 1 AS one FROM tenants WHERE id = $1",
            tenant.as_str()
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.is_some())
    }

    /// A tenant-scoped handle for genuinely tenant-level operations
    /// (user provisioning and lookup). Pure — no I/O.
    pub fn for_tenant(&self, tenant: TenantId) -> TenantStore {
        TenantStore {
            pool: self.pool.clone(),
            tenant,
        }
    }

    /// A [`TenantStore`] for the tenant an account already belongs to.
    ///
    /// `pub(crate)` and narrow on purpose, and it widens nothing: the handle is
    /// scoped to the account's **own** tenant, read from the account rather
    /// than passed in, so it cannot be used to reach sideways. What it avoids
    /// is a caller inside this crate holding a whole [`Store`] just to reach a
    /// tenant-level record it is already entitled to — the campaign dispatcher
    /// mints an unsubscribe token per recipient, and tokens are tenant-level
    /// because the endpoint that redeems them has no logged-in user at all.
    pub(crate) fn tenant_scope(account: &crate::AccountStore) -> TenantStore {
        TenantStore {
            pool: account.pool.clone(),
            tenant: account.tenant.clone(),
        }
    }

    /// The **only** door to user-owned mail data: a handle scoped to one
    /// `(tenant, user)`. Pure — no I/O; every operation it exposes bakes
    /// both ids, so cross-account access is unrepresentable (see
    /// [`crate::account::AccountStore`]).
    /// Reads and spends workspace invitations (migration 0209).
    ///
    /// Not tenant-scoped, unlike every other handle here, and deliberately:
    /// the person opening an invitation link is not signed in, so there is no
    /// tenant to scope by. The token is the claim, and the row it matches is
    /// what says which tenant they are joining.
    #[must_use]
    pub fn invites(&self) -> crate::user_invites::InviteStore {
        crate::user_invites::InviteStore::new(self.pool.clone())
    }

    pub fn for_account(&self, tenant: TenantId, user: UserId) -> crate::account::AccountStore {
        crate::account::AccountStore {
            pool: self.pool.clone(),
            blobs: self.blobs.clone(),
            tenant,
            user,
        }
    }

    /// The connection pool, for the identity-persistence module
    /// ([`crate::identity`]) which lives in a sibling file (Law 3).
    pub(crate) fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// The blob backend, for sibling modules that serve bytes outside an account
    /// scope (e.g. the public share-download path in [`crate::share`]).
    pub(crate) fn blobs(&self) -> &BlobStore {
        &self.blobs
    }

    /// Resolves a recipient email address to its `(tenant, user)` for local
    /// delivery, checking canonical user addresses **and** aliases
    /// (`alo-identity`). `None` if no account has that address, or if it
    /// is ambiguous. Email/alias addresses are globally unique in a
    /// deployment; on the impossible event that an address maps to more than
    /// one account, this returns no account rather than guessing — inbound
    /// routing never picks a mailbox by chance.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn account_by_email(&self, email: &str) -> Result<Option<(TenantId, UserId)>> {
        // Canonical user addresses take precedence over aliases. `LIMIT 2`
        // detects ambiguity (a cross-tenant email collision → refuse rather
        // than guess) without scanning the whole set.
        let users = sqlx::query!(
            "SELECT tenant_id, id FROM users WHERE lower(email) = lower($1) LIMIT 2",
            email
        )
        .fetch_all(&self.pool)
        .await?;
        if users.len() == 1 {
            return Ok(Some((
                TenantId::new(users[0].tenant_id.clone()),
                UserId::new(users[0].id.clone()),
            )));
        }
        if !users.is_empty() {
            return Ok(None); // ambiguous canonical match — refuse
        }
        // No canonical user; try an alias (its address is globally unique).
        let aliases = sqlx::query!(
            "SELECT tenant_id, user_id FROM aliases WHERE address = lower($1) LIMIT 2",
            email
        )
        .fetch_all(&self.pool)
        .await?;
        if aliases.len() == 1 {
            return Ok(Some((
                TenantId::new(aliases[0].tenant_id.clone()),
                UserId::new(aliases[0].user_id.clone()),
            )));
        }
        Ok(None)
    }

    /// The member accounts of a distribution list whose address is `email`, for
    /// inbound fan-out. Empty when `email` is not a list address. Members are
    /// users (never nested lists), so expansion is single-level and loop-free.
    /// Runtime-checked query (kept out of the offline `.sqlx` cache).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn list_members_by_address(&self, email: &str) -> Result<Vec<(TenantId, UserId)>> {
        let rows = sqlx::query_as::<_, (String, String)>(
            "SELECT gm.tenant_id, gm.user_id FROM groups g \
             JOIN group_members gm ON gm.group_id = g.id \
             WHERE g.address = lower($1)",
        )
        .bind(email)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows
            .into_iter()
            .map(|(t, u)| (TenantId::new(t), UserId::new(u)))
            .collect())
    }
}

/// A tenant-scoped handle for tenant-level provisioning. Holds its
/// [`TenantId`] privately and bakes it into every statement. No method
/// accepts a tenant argument. User-owned mail data is **not** reachable
/// here — that is [`AccountStore`](crate::AccountStore)'s job.
#[derive(Clone)]
pub struct TenantStore {
    pool: PgPool,
    tenant: TenantId,
}

impl TenantStore {
    /// The tenant this handle is scoped to.
    pub fn tenant(&self) -> &TenantId {
        &self.tenant
    }

    /// The connection pool, for the identity-persistence module
    /// ([`crate::identity`]) which lives in a sibling file (Law 3).
    pub(crate) fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Confirms a user exists in this tenant; `NotFound` otherwise. Guards
    /// the provisioning paths that take a user id.
    pub(crate) async fn assert_user(&self, user: &UserId) -> Result<()> {
        sqlx::query!(
            "SELECT 1 AS one FROM users WHERE tenant_id = $1 AND id = $2",
            self.tenant.as_str(),
            user.as_str()
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or(StoreError::NotFound)
        .map(|_| ())
    }

    // ---- users (tenant-level provisioning) ----------------------------

    /// Creates a user (JMAP account) in this tenant.
    ///
    /// # Errors
    /// [`StoreError::Conflict`] if the email already exists in the tenant.
    pub async fn create_user(&self, email: &str) -> Result<UserId> {
        let id = UserId::generate();
        sqlx::query!(
            "INSERT INTO users (id, tenant_id, email) VALUES ($1, $2, $3)",
            id.as_str(),
            self.tenant.as_str(),
            email
        )
        .execute(&self.pool)
        .await?;
        Ok(id)
    }

    /// Marks a user as a tenant admin (or not). Admin-only surfaces gate on
    /// this. Runtime-checked query (kept out of the offline `.sqlx` cache).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn set_admin(&self, user: &UserId, is_admin: bool) -> Result<()> {
        sqlx::query("UPDATE users SET is_admin = $3 WHERE tenant_id = $1 AND id = $2")
            .bind(self.tenant.as_str())
            .bind(user.as_str())
            .bind(is_admin)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// All users in this tenant with read-only usage (message count + storage
    /// bytes), for the admin console. Runtime-checked query (kept out of the
    /// offline `.sqlx` cache); usage is aggregated per user.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn list_users(&self) -> Result<Vec<crate::model::UserRow>> {
        let rows = sqlx::query_as::<_, (String, String, bool, time::OffsetDateTime, i64, i64)>(
            "SELECT u.id, u.email, u.is_admin, u.created_at, \
               (SELECT count(*) FROM messages m \
                WHERE m.tenant_id = u.tenant_id AND m.user_id = u.id)::bigint AS msgs, \
               (SELECT coalesce(sum(b.size), 0) FROM messages m \
                JOIN blobs b ON b.tenant_id = m.tenant_id AND b.id = m.blob_id \
                WHERE m.tenant_id = u.tenant_id AND m.user_id = u.id)::bigint AS bytes \
             FROM users u WHERE u.tenant_id = $1 ORDER BY u.created_at",
        )
        .bind(self.tenant.as_str())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(
                |(id, email, is_admin, created_at, message_count, storage_bytes)| {
                    crate::model::UserRow {
                        id,
                        email,
                        is_admin,
                        created_at,
                        message_count,
                        storage_bytes,
                    }
                },
            )
            .collect())
    }

    /// Deletes a user in this tenant (mailboxes, messages, memberships, aliases
    /// cascade via FK). Runtime-checked query.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn delete_user(&self, user: &UserId) -> Result<()> {
        sqlx::query("DELETE FROM users WHERE tenant_id = $1 AND id = $2")
            .bind(self.tenant.as_str())
            .bind(user.as_str())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Looks up a user id by email within this tenant.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if no such user in this tenant.
    pub async fn user_by_email(&self, email: &str) -> Result<UserId> {
        let row = sqlx::query!(
            "SELECT id FROM users WHERE tenant_id = $1 AND email = $2",
            self.tenant.as_str(),
            email
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or(StoreError::NotFound)?;
        Ok(UserId::new(row.id))
    }
}
