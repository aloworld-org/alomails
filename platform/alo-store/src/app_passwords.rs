//! App-specific password persistence (mail M1). Storage only, like the
//! rest of identity persistence: generation, argon2id hashing, and
//! verification live in `alo-identity` — this module never sees a
//! plaintext secret, only the PHC string it stores and returns verbatim.
//!
//! Two doors, mirroring `identity.rs`:
//! - **Pre-tenant lookup on [`Store`]** — resolve a login username to the
//!   user's app-password hashes before any tenant is known (the legacy
//!   IMAP/POP3/SMTP auth path). Returns rows for exactly the one user the
//!   unique username index names, never guessing.
//! - **Tenant-scoped ownership on [`TenantStore`]** — create, list and
//!   revoke, reached only through the tenant door so a caller cannot
//!   touch another tenant's credentials.

use time::OffsetDateTime;

use crate::error::{Result, StoreError};
use crate::id::{AppPasswordId, TenantId, UserId};
use crate::store::{Store, TenantStore};

/// The most app passwords one user may hold at once. Every legacy login
/// attempt for a user verifies the presented secret against each of their
/// hashes (argon2id, deliberately expensive), so this cap bounds the work
/// one authentication can cost — and twenty named devices is far beyond
/// any real desk.
pub const APP_PASSWORDS_MAX: i64 = 20;

/// The longest accepted app-password name. A name labels a device in a
/// settings list; a paragraph is not a label.
pub const APP_PASSWORD_NAME_MAX_CHARS: usize = 100;

/// One app password as its owner sees it: the record, never the secret.
#[derive(Debug)]
pub struct AppPasswordRow {
    /// The record's id (revocation handle).
    pub id: AppPasswordId,
    /// The user-chosen label ("Thunderbird on the desk machine").
    pub name: String,
    /// When it was created.
    pub created_at: OffsetDateTime,
    /// When it last authenticated a connection, if ever.
    pub last_used_at: Option<OffsetDateTime>,
}

/// One app-password hash resolved by login username, for `alo-identity`
/// to verify a presented secret against on the legacy auth path.
pub struct AppPasswordCredential {
    /// The record's id (so a successful verify can stamp `last_used_at`).
    pub id: AppPasswordId,
    /// The owning tenant.
    pub tenant: TenantId,
    /// The owning user.
    pub user: UserId,
    /// The argon2id PHC hash string. Verified by `alo-identity`.
    pub password_hash: String,
}

impl Store {
    /// Resolves a global login username to the owning user's app-password
    /// hashes, oldest first. Empty if the username is unknown or the user
    /// has none — the two are indistinguishable here by design (the caller
    /// pays a dummy hash either way, so timing never leaks existence).
    /// Cross-tenant by necessity, like [`Store::credentials_by_username`]:
    /// the username carries no tenant hint, and the unique username index
    /// pins the rows to exactly one user.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn app_password_credentials_by_username(
        &self,
        username: &str,
    ) -> Result<Vec<AppPasswordCredential>> {
        let rows = sqlx::query_as::<_, (String, String, String, String)>(
            "SELECT a.id, a.tenant_id, a.user_id, a.password_hash \
             FROM app_passwords a \
             JOIN credentials c ON c.user_id = a.user_id AND c.tenant_id = a.tenant_id \
             WHERE c.username = $1 ORDER BY a.created_at",
        )
        .bind(username)
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|(id, tenant, user, password_hash)| AppPasswordCredential {
                id: AppPasswordId::new(id),
                tenant: TenantId::new(tenant),
                user: UserId::new(user),
                password_hash,
            })
            .collect())
    }

    /// Stamps an app password's `last_used_at` after a successful verify,
    /// so the owner's list can answer "is this one still in use?". Scoped
    /// by the unguessable id the verify just resolved; silent if the row
    /// was revoked in the meantime (the connection it authenticated is
    /// already live — M1.2's seam re-checks on the *next* one).
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn touch_app_password(&self, id: &AppPasswordId) -> Result<()> {
        sqlx::query("UPDATE app_passwords SET last_used_at = now() WHERE id = $1")
            .bind(id.as_str())
            .execute(self.pool())
            .await?;
        Ok(())
    }
}

impl TenantStore {
    /// Records a new app password for a user in this tenant. The hash is
    /// produced by `alo-identity`; the store never sees the secret.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the user is not in this tenant;
    /// [`StoreError::Validation`] if the name is empty or too long;
    /// [`StoreError::Conflict`] if the user already holds
    /// [`APP_PASSWORDS_MAX`] app passwords.
    pub async fn create_app_password(
        &self,
        user: &UserId,
        name: &str,
        password_hash: &str,
    ) -> Result<AppPasswordId> {
        let name = name.trim();
        if name.is_empty() {
            return Err(StoreError::Validation(
                "an app password needs a name".into(),
            ));
        }
        if name.chars().count() > APP_PASSWORD_NAME_MAX_CHARS {
            return Err(StoreError::Validation(format!(
                "an app password name is at most {APP_PASSWORD_NAME_MAX_CHARS} characters"
            )));
        }
        self.assert_user(user).await?;
        let held: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM app_passwords WHERE tenant_id = $1 AND user_id = $2",
        )
        .bind(self.tenant().as_str())
        .bind(user.as_str())
        .fetch_one(self.pool())
        .await?;
        if held >= APP_PASSWORDS_MAX {
            return Err(StoreError::Conflict(format!(
                "at most {APP_PASSWORDS_MAX} app passwords per user — revoke one first"
            )));
        }
        let id = AppPasswordId::generate();
        sqlx::query(
            "INSERT INTO app_passwords (id, tenant_id, user_id, name, password_hash) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(id.as_str())
        .bind(self.tenant().as_str())
        .bind(user.as_str())
        .bind(name)
        .bind(password_hash)
        .execute(self.pool())
        .await?;
        Ok(id)
    }

    /// A user's app passwords, oldest first — the settings list (name,
    /// created, last used; never a hash).
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the user is not in this tenant.
    pub async fn list_app_passwords(&self, user: &UserId) -> Result<Vec<AppPasswordRow>> {
        self.assert_user(user).await?;
        let rows = sqlx::query_as::<_, (String, String, OffsetDateTime, Option<OffsetDateTime>)>(
            "SELECT id, name, created_at, last_used_at FROM app_passwords \
             WHERE tenant_id = $1 AND user_id = $2 ORDER BY created_at",
        )
        .bind(self.tenant().as_str())
        .bind(user.as_str())
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|(id, name, created_at, last_used_at)| AppPasswordRow {
                id: AppPasswordId::new(id),
                name,
                created_at,
                last_used_at,
            })
            .collect())
    }

    /// Revokes one app password by deleting its row: it stops verifying on
    /// the next connection, and the hash is gone with it.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if no such record belongs to this
    /// `(tenant, user)` — a foreign id gets the same clean denial as an
    /// absent one.
    pub async fn revoke_app_password(&self, user: &UserId, id: &AppPasswordId) -> Result<()> {
        let done = sqlx::query(
            "DELETE FROM app_passwords WHERE tenant_id = $1 AND user_id = $2 AND id = $3",
        )
        .bind(self.tenant().as_str())
        .bind(user.as_str())
        .bind(id.as_str())
        .execute(self.pool())
        .await?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }
}
