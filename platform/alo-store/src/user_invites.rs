//! Inviting somebody into the workspace instead of choosing a password for
//! them (migration 0209).
//!
//! An admin enters an address; alo creates the account with **no credential**
//! and mails a one-time link. The person opens it, sets their own password and
//! their own recovery address, and only then can they sign in. The admin never
//! learns either.
//!
//! # The two writes that must not come apart
//!
//! Spending the token and installing the credential happen in one transaction,
//! for the reason [`crate::site_editors`] gives for the same shape: a crash
//! between them would leave either a reusable link (a permanent key to an
//! account) or an account that can never be signed into and whose invitation
//! is spent. The recovery address is written in the same transaction, because
//! an account that can be signed into but not recovered is exactly the state
//! this feature exists to end.
//!
//! # Why the recovery address is captured here and not later
//!
//! `/reset/*` proves control of the recovery mailbox captured at signup. An
//! admin-created account never had one, so "forgot password" could not help
//! anybody an admin had added — their only route back was to ask the admin for
//! another password over the same unprotected channel. Acceptance is the one
//! moment the person is present, authenticated by the token, and able to name
//! an address that is not the mailbox they are about to be locked out of.

use crate::error::Result;
use crate::id::{TenantId, UserId};
use crate::store::TenantStore;

/// How long an invitation stays usable. An invitation is a credential-shaped
/// thing sitting in a mailbox; one that never expires is a permanent key.
pub const INVITE_TTL_DAYS: i64 = 7;

/// Who an unspent invitation is for — enough to greet them by address on the
/// acceptance page, and nothing else.
#[derive(Debug, Clone)]
pub struct UserInviteTarget {
    pub tenant: TenantId,
    pub user: UserId,
    /// The invited address, which is also the username the credential is
    /// installed under.
    pub email: String,
}

impl TenantStore {
    /// Records an invitation for a user of this tenant.
    ///
    /// The caller hashes the token; this never sees the token itself, so a
    /// database backup does not carry every outstanding invitation.
    ///
    /// Re-inviting is deliberately additive rather than a replacement: each
    /// send gets its own row, older unspent rows stay valid until they expire,
    /// and the person can use whichever mail they happened to open. Spending
    /// any one of them installs the credential, after which all the others
    /// fail the `accepted_at IS NULL` test on the account they point at.
    ///
    /// # Errors
    /// [`crate::StoreError::Db`] on failure.
    pub async fn invite_user(
        &self,
        user: &UserId,
        email: &str,
        token_hash: &str,
        invited_by: &UserId,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO user_invites \
                 (token_hash, tenant_id, user_id, email, invited_by, expires_at) \
             VALUES ($1, $2, $3, lower($4), $5, now() + ($6 || ' days')::interval)",
        )
        .bind(token_hash)
        .bind(self.tenant().as_str())
        .bind(user.as_str())
        .bind(email)
        .bind(invited_by.as_str())
        .bind(INVITE_TTL_DAYS.to_string())
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Whether this user has an invitation still outstanding, for the admin
    /// console's per-row badge.
    ///
    /// # Errors
    /// [`crate::StoreError::Db`] on failure.
    pub async fn has_open_invite(&self, user: &UserId) -> Result<bool> {
        let open: Option<i32> = sqlx::query_scalar(
            "SELECT 1 FROM user_invites \
              WHERE tenant_id = $1 AND user_id = $2 \
                AND accepted_at IS NULL AND expires_at > now() LIMIT 1",
        )
        .bind(self.tenant().as_str())
        .bind(user.as_str())
        .fetch_optional(self.pool())
        .await?;
        Ok(open.is_some())
    }
}

/// Reads and spends invitations. Not on [`TenantStore`]: the person opening the
/// link is not signed in and there is no tenant to scope by — the token *is*
/// the claim, and the row it matches is what says which tenant they belong to.
pub struct InviteStore {
    pool: sqlx::PgPool,
}

impl InviteStore {
    pub(crate) fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }

    /// Who an unspent, unexpired invitation is for, or `None`.
    ///
    /// `None` covers unknown, spent and expired alike, so the acceptance page
    /// cannot be used to discover which tokens once existed.
    ///
    /// # Errors
    /// [`crate::StoreError::Db`] on failure.
    pub async fn invite(&self, token_hash: &str) -> Result<Option<UserInviteTarget>> {
        let row: Option<(String, String, String)> = sqlx::query_as(
            "SELECT tenant_id, user_id, email FROM user_invites \
              WHERE token_hash = $1 AND accepted_at IS NULL AND expires_at > now()",
        )
        .bind(token_hash)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(tenant, user, email)| UserInviteTarget {
            tenant: TenantId::new(tenant),
            user: UserId::new(user),
            email,
        }))
    }

    /// Installs the credential, records the recovery address and spends the
    /// token — atomically.
    ///
    /// `FOR UPDATE` on the invite row makes two simultaneous acceptances of the
    /// same link resolve to one: the second waits, then finds `accepted_at`
    /// set and answers `None`. Without it both would insert a credential and
    /// the second would fail on the unique index, which is the same outcome
    /// reported as a server error instead of as a spent link.
    ///
    /// # Errors
    /// [`crate::StoreError::Db`] on failure.
    pub async fn accept(
        &self,
        token_hash: &str,
        password_hash: &str,
        recovery_email: &str,
    ) -> Result<Option<UserInviteTarget>> {
        let mut tx = self.pool.begin().await?;
        let row: Option<(String, String, String)> = sqlx::query_as(
            "SELECT tenant_id, user_id, email FROM user_invites \
              WHERE token_hash = $1 AND accepted_at IS NULL AND expires_at > now() \
              FOR UPDATE",
        )
        .bind(token_hash)
        .fetch_optional(&mut *tx)
        .await?;
        let Some((tenant, user, email)) = row else {
            return Ok(None);
        };
        // A resent invitation leaves more than one live link, and the account
        // can still only be claimed once. Without this, spending the second
        // one tried to install a second credential and failed on the unique
        // index — a 500 where the honest answer is "that link has been used".
        // So a claimed account spends every link it has left.
        let claimed: Option<i32> =
            sqlx::query_scalar("SELECT 1 FROM credentials WHERE user_id = $1 LIMIT 1")
                .bind(&user)
                .fetch_optional(&mut *tx)
                .await?;
        if claimed.is_some() {
            sqlx::query(
                "UPDATE user_invites SET accepted_at = now()                   WHERE tenant_id = $1 AND user_id = $2 AND accepted_at IS NULL",
            )
            .bind(&tenant)
            .bind(&user)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            return Ok(None);
        }
        sqlx::query(
            "INSERT INTO credentials (user_id, tenant_id, username, password_hash) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(&user)
        .bind(&tenant)
        .bind(&email)
        .bind(password_hash)
        .execute(&mut *tx)
        .await?;
        // The whole point of the feature: an account that can be signed into
        // and never recovered is the state this replaces.
        sqlx::query(
            "INSERT INTO account_recovery (address, tenant_id, user_id, recovery_email) \
             VALUES (lower($1), $2, $3, lower($4)) \
             ON CONFLICT (address) DO UPDATE SET \
                 tenant_id = EXCLUDED.tenant_id, \
                 user_id = EXCLUDED.user_id, \
                 recovery_email = EXCLUDED.recovery_email",
        )
        .bind(&email)
        .bind(&tenant)
        .bind(&user)
        .bind(recovery_email)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE user_invites SET accepted_at = now()               WHERE tenant_id = $1 AND user_id = $2 AND accepted_at IS NULL",
        )
        .bind(&tenant)
        .bind(&user)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(Some(UserInviteTarget {
            tenant: TenantId::new(tenant),
            user: UserId::new(user),
            email,
        }))
    }
}
