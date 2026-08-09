//! Identity persistence for `alo-identity` (Law 3: kept out of
//! `store.rs`). This module is **storage only** — all cryptography
//! (argon2id password hashing/verification, token hashing, TOTP, JWT
//! signing) lives in `alo-identity`. The store persists values already
//! hashed by that crate and never sees a plaintext password or token; the
//! one secret it holds is an argon2 PHC string it stores and returns
//! verbatim.
//!
//! Two shapes of method live here:
//! - **Pre-tenant lookups on [`Store`]** — resolve an unforgeable secret
//!   or identifier (a username, a token hash, an auth-code hash, a client
//!   id) *to* a `(tenant, user)` before any tenant is known. These are the
//!   only identity methods that may cross tenants; each returns the single
//!   owning row or nothing, never guessing.
//! - **Tenant-scoped provisioning on [`TenantStore`]** — reached only
//!   through the tenant door, so a caller cannot write another tenant's
//!   identity rows.

use time::OffsetDateTime;

use crate::error::{Result, StoreError};
use crate::id::{GroupId, TenantId, UserId};
use crate::store::{Store, TenantStore};

/// A stored credential row (argon2 PHC hash) resolved by global username.
pub struct CredentialRow {
    /// The owning tenant.
    pub tenant: TenantId,
    /// The owning user.
    pub user: UserId,
    /// The argon2id PHC hash string. Verified by `alo-identity`.
    pub password_hash: String,
}

/// A resolved access-token row (looked up by SHA-256 hash).
pub struct AccessTokenRow {
    /// The owning tenant.
    pub tenant: TenantId,
    /// The owning user.
    pub user: UserId,
    /// The granted scope (space-separated).
    pub scope: String,
    /// When the token expires.
    pub expires_at: OffsetDateTime,
    /// When the token was revoked, if it has been.
    pub revoked_at: Option<OffsetDateTime>,
}

/// A resolved refresh-token row (looked up by SHA-256 hash).
pub struct RefreshTokenRow {
    /// The owning tenant.
    pub tenant: TenantId,
    /// The owning user.
    pub user: UserId,
    /// The client the refresh token is bound to.
    pub client_id: String,
    /// The granted scope.
    pub scope: String,
    /// When the token expires.
    pub expires_at: OffsetDateTime,
    /// When the token was revoked, if it has been.
    pub revoked_at: Option<OffsetDateTime>,
    /// The hash this token was rotated into, if it has been spent.
    pub rotated_to: Option<String>,
}

/// The outcome of consuming a single-use authorization code.
pub enum AuthCodeOutcome {
    /// The code was valid and is now consumed (returned exactly once).
    Valid(AuthCodeRow),
    /// The code existed but was already used — a replay.
    Replayed {
        /// The tenant that owned the replayed code.
        tenant: TenantId,
        /// The user that owned the replayed code.
        user: UserId,
        /// The client the replayed code was issued to.
        client_id: String,
    },
    /// No such code, or it had expired.
    NotFound,
}

/// The authenticated context captured in an authorization code.
pub struct AuthCodeRow {
    /// The owning tenant.
    pub tenant: TenantId,
    /// The owning user.
    pub user: UserId,
    /// The client the code was issued to.
    pub client_id: String,
    /// The exact redirect URI the code was bound to.
    pub redirect_uri: String,
    /// The PKCE S256 code challenge.
    pub code_challenge: String,
    /// The granted scope.
    pub scope: String,
    /// The OIDC nonce, if the request carried one.
    pub nonce: Option<String>,
}

/// A registered OAuth client.
pub struct OAuthClient {
    /// The client id.
    pub client_id: String,
    /// The tenant the client belongs to, or `None` for a deployment-wide
    /// first-party client.
    pub tenant: Option<TenantId>,
    /// Human-readable name.
    pub name: String,
    /// Allowed redirect URIs (exact match).
    pub redirect_uris: Vec<String>,
    /// The argon2 hash of the client secret, or `None` for a public
    /// (PKCE-only) client.
    ///
    /// **Not yet enforced:** the token endpoint currently authenticates
    /// clients by PKCE only (`token_endpoint_auth_methods_supported:
    /// ["none"]`). This field is reserved for confidential-client support;
    /// until that lands, a non-`None` secret is stored but never checked.
    /// Recorded in `docs/design/security-audit-followups.md`.
    pub secret_hash: Option<String>,
}

/// A TOTP enrollment row.
pub struct TotpRow {
    /// The raw shared secret.
    pub secret: Vec<u8>,
    /// Whether enrollment was confirmed (a disabled secret never gates
    /// login).
    pub enabled: bool,
}

/// A signing key for ID tokens (deployment-global).
pub struct SigningKeyRow {
    /// The key id (published in the JWKS and the JWT header).
    pub kid: String,
    /// The signing algorithm (`EdDSA`).
    pub algorithm: String,
    /// The private key material.
    pub private_key: Vec<u8>,
    /// The public key material.
    pub public_key: Vec<u8>,
}

/// A signing key's **public** half only — for the JWKS, which must never
/// load private seed material into memory.
pub struct PublicKeyRow {
    /// The key id.
    pub kid: String,
    /// The signing algorithm (`EdDSA`).
    pub algorithm: String,
    /// The public key material.
    pub public_key: Vec<u8>,
}

impl Store {
    /// Resolves a global login username to its credential row. `None` if no
    /// such username. Cross-tenant by necessity (the username carries no
    /// tenant hint); returns exactly one row (the username unique index) or
    /// nothing.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn credentials_by_username(&self, username: &str) -> Result<Option<CredentialRow>> {
        let row = sqlx::query!(
            "SELECT tenant_id, user_id, password_hash FROM credentials WHERE username = $1",
            username
        )
        .fetch_optional(self.pool())
        .await?;
        Ok(row.map(|r| CredentialRow {
            tenant: TenantId::new(r.tenant_id),
            user: UserId::new(r.user_id),
            password_hash: r.password_hash,
        }))
    }

    /// Resolves an access-token hash to its row (including revocation and
    /// expiry, which the caller checks). `None` if unknown.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn access_token(&self, token_hash: &str) -> Result<Option<AccessTokenRow>> {
        let row = sqlx::query!(
            "SELECT tenant_id, user_id, scope, expires_at, revoked_at \
             FROM access_tokens WHERE token_hash = $1",
            token_hash
        )
        .fetch_optional(self.pool())
        .await?;
        Ok(row.map(|r| AccessTokenRow {
            tenant: TenantId::new(r.tenant_id),
            user: UserId::new(r.user_id),
            scope: r.scope,
            expires_at: r.expires_at,
            revoked_at: r.revoked_at,
        }))
    }

    /// Resolves a refresh-token hash to its row. `None` if unknown.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn refresh_token(&self, token_hash: &str) -> Result<Option<RefreshTokenRow>> {
        let row = sqlx::query!(
            "SELECT tenant_id, user_id, client_id, scope, expires_at, revoked_at, rotated_to \
             FROM refresh_tokens WHERE token_hash = $1",
            token_hash
        )
        .fetch_optional(self.pool())
        .await?;
        Ok(row.map(|r| RefreshTokenRow {
            tenant: TenantId::new(r.tenant_id),
            user: UserId::new(r.user_id),
            client_id: r.client_id,
            scope: r.scope,
            expires_at: r.expires_at,
            revoked_at: r.revoked_at,
            rotated_to: r.rotated_to,
        }))
    }

    /// Looks up a registered OAuth client. `None` if unknown.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn oauth_client(&self, client_id: &str) -> Result<Option<OAuthClient>> {
        let row = sqlx::query!(
            "SELECT client_id, tenant_id, name, redirect_uris, secret_hash \
             FROM oauth_clients WHERE client_id = $1",
            client_id
        )
        .fetch_optional(self.pool())
        .await?;
        Ok(row.map(|r| OAuthClient {
            client_id: r.client_id,
            tenant: r.tenant_id.map(TenantId::new),
            name: r.name,
            redirect_uris: r.redirect_uris,
            secret_hash: r.secret_hash,
        }))
    }

    /// Registers (or replaces) an OAuth client. A `None` tenant is a
    /// deployment-wide first-party client; a `None` `secret_hash` is a
    /// public (PKCE-only) client. Admin/bootstrap path.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn register_oauth_client(
        &self,
        client_id: &str,
        tenant: Option<&TenantId>,
        name: &str,
        redirect_uris: &[String],
        secret_hash: Option<&str>,
    ) -> Result<()> {
        sqlx::query!(
            "INSERT INTO oauth_clients (client_id, tenant_id, name, redirect_uris, secret_hash) \
             VALUES ($1, $2, $3, $4, $5) \
             ON CONFLICT (client_id) DO UPDATE \
               SET name = $3, redirect_uris = $4, secret_hash = $5",
            client_id,
            tenant.map(TenantId::as_str),
            name,
            redirect_uris,
            secret_hash,
        )
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Atomically consumes a single-use authorization code. The `UPDATE`
    /// that sets `used_at` is the single-use gate — exactly one caller can
    /// win it; a second sees the code already used and gets
    /// [`AuthCodeOutcome::Replayed`] so the caller can revoke the chain.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn consume_auth_code(&self, code_hash: &str) -> Result<AuthCodeOutcome> {
        let consumed = sqlx::query!(
            "UPDATE oauth_auth_codes SET used_at = now() \
             WHERE code_hash = $1 AND used_at IS NULL AND expires_at > now() \
             RETURNING tenant_id, user_id, client_id, redirect_uri, code_challenge, scope, nonce",
            code_hash
        )
        .fetch_optional(self.pool())
        .await?;
        if let Some(r) = consumed {
            return Ok(AuthCodeOutcome::Valid(AuthCodeRow {
                tenant: TenantId::new(r.tenant_id),
                user: UserId::new(r.user_id),
                client_id: r.client_id,
                redirect_uri: r.redirect_uri,
                code_challenge: r.code_challenge,
                scope: r.scope,
                nonce: r.nonce,
            }));
        }
        // Not consumable: either it never existed / expired, or it was
        // already used (a replay we must report).
        let existing = sqlx::query!(
            "SELECT tenant_id, user_id, client_id, used_at FROM oauth_auth_codes \
             WHERE code_hash = $1",
            code_hash
        )
        .fetch_optional(self.pool())
        .await?;
        match existing {
            Some(r) if r.used_at.is_some() => Ok(AuthCodeOutcome::Replayed {
                tenant: TenantId::new(r.tenant_id),
                user: UserId::new(r.user_id),
                client_id: r.client_id,
            }),
            _ => Ok(AuthCodeOutcome::NotFound),
        }
    }

    /// Marks an access token revoked (idempotent). Logout / revoke.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn revoke_access_token(&self, token_hash: &str) -> Result<()> {
        sqlx::query!(
            "UPDATE access_tokens SET revoked_at = now() \
             WHERE token_hash = $1 AND revoked_at IS NULL",
            token_hash
        )
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Marks a refresh token revoked (idempotent).
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn revoke_refresh_token(&self, token_hash: &str) -> Result<()> {
        sqlx::query!(
            "UPDATE refresh_tokens SET revoked_at = now() \
             WHERE token_hash = $1 AND revoked_at IS NULL",
            token_hash
        )
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Revokes every access and refresh token for a `(user, client)` — the
    /// response to a detected refresh-token replay (RFC 6749 §10.4).
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn revoke_user_client_tokens(
        &self,
        tenant: &TenantId,
        user: &UserId,
        client_id: &str,
    ) -> Result<()> {
        sqlx::query!(
            "UPDATE refresh_tokens SET revoked_at = now() \
             WHERE tenant_id = $1 AND user_id = $2 AND client_id = $3 AND revoked_at IS NULL",
            tenant.as_str(),
            user.as_str(),
            client_id
        )
        .execute(self.pool())
        .await?;
        sqlx::query!(
            "UPDATE access_tokens SET revoked_at = now() \
             WHERE tenant_id = $1 AND user_id = $2 AND client_id = $3 AND revoked_at IS NULL",
            tenant.as_str(),
            user.as_str(),
            client_id
        )
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Atomically rotates a refresh token: the guarded `UPDATE` is the
    /// single-use gate — it revokes `old_hash` and records the rotation
    /// **only if the token has not already been rotated or revoked**, so two
    /// concurrent redemptions of one token cannot both succeed. Returns
    /// `true` if this call won the rotation (and inserted the new token),
    /// `false` if the token was already spent (a replay the caller must
    /// respond to by revoking the chain). Runs in one transaction so a crash
    /// cannot leave a half-rotated pair.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    #[allow(clippy::too_many_arguments)]
    pub async fn rotate_refresh_token(
        &self,
        old_hash: &str,
        new_hash: &str,
        tenant: &TenantId,
        user: &UserId,
        client_id: &str,
        scope: &str,
        expires_at: OffsetDateTime,
    ) -> Result<bool> {
        let mut tx = self.pool().begin().await.map_err(StoreError::Db)?;
        let won = sqlx::query!(
            "UPDATE refresh_tokens SET revoked_at = now(), rotated_to = $2 \
             WHERE token_hash = $1 AND rotated_to IS NULL AND revoked_at IS NULL \
             RETURNING token_hash",
            old_hash,
            new_hash
        )
        .fetch_optional(&mut *tx)
        .await?;
        if won.is_none() {
            // Lost the race (or already revoked): do not mint a new token.
            // Dropping the transaction rolls back.
            return Ok(false);
        }
        sqlx::query!(
            "INSERT INTO refresh_tokens \
               (token_hash, tenant_id, user_id, client_id, scope, expires_at) \
             VALUES ($1, $2, $3, $4, $5, $6)",
            new_hash,
            tenant.as_str(),
            user.as_str(),
            client_id,
            scope,
            expires_at
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(true)
    }

    /// All non-retired signing keys, newest first (index 0 signs; all are
    /// published in the JWKS). Deployment-global.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn signing_keys(&self) -> Result<Vec<SigningKeyRow>> {
        let rows = sqlx::query!(
            "SELECT kid, algorithm, private_key, public_key FROM signing_keys \
             WHERE retired_at IS NULL ORDER BY created_at DESC"
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| SigningKeyRow {
                kid: r.kid,
                algorithm: r.algorithm,
                private_key: r.private_key,
                public_key: r.public_key,
            })
            .collect())
    }

    /// The **public** halves of all non-retired signing keys, for the JWKS.
    /// Never loads private seed material. Deployment-global.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn public_signing_keys(&self) -> Result<Vec<PublicKeyRow>> {
        let rows = sqlx::query!(
            "SELECT kid, algorithm, public_key FROM signing_keys \
             WHERE retired_at IS NULL ORDER BY created_at DESC"
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| PublicKeyRow {
                kid: r.kid,
                algorithm: r.algorithm,
                public_key: r.public_key,
            })
            .collect())
    }

    /// Inserts a signing key (bootstrap / rotation). Deployment-global.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn insert_signing_key(
        &self,
        kid: &str,
        algorithm: &str,
        private_key: &[u8],
        public_key: &[u8],
    ) -> Result<()> {
        sqlx::query!(
            "INSERT INTO signing_keys (kid, algorithm, private_key, public_key) \
             VALUES ($1, $2, $3, $4)",
            kid,
            algorithm,
            private_key,
            public_key
        )
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Retires a signing key (rotation): it stops signing and drops out of
    /// the JWKS after this is called. Idempotent.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn retire_signing_key(&self, kid: &str) -> Result<()> {
        sqlx::query!(
            "UPDATE signing_keys SET retired_at = now() WHERE kid = $1 AND retired_at IS NULL",
            kid
        )
        .execute(self.pool())
        .await?;
        Ok(())
    }
}

impl TenantStore {
    /// Stores (or replaces) a user's argon2 password hash under a global
    /// login username. The hash is produced by `alo-identity`; the store
    /// never sees the plaintext.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the user is not in this tenant;
    /// [`StoreError::Conflict`] if the username is taken.
    pub async fn set_password_hash(
        &self,
        user: &UserId,
        username: &str,
        password_hash: &str,
    ) -> Result<()> {
        self.assert_user(user).await?;
        sqlx::query!(
            "INSERT INTO credentials (user_id, tenant_id, username, password_hash) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT (user_id) DO UPDATE SET username = $3, password_hash = $4",
            user.as_str(),
            self.tenant().as_str(),
            username,
            password_hash
        )
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// The email address of a user in this tenant, for OIDC claims. `None`
    /// if the user is not in this tenant.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn email_of(&self, user: &UserId) -> Result<Option<String>> {
        let row = sqlx::query!(
            "SELECT email FROM users WHERE tenant_id = $1 AND id = $2",
            self.tenant().as_str(),
            user.as_str()
        )
        .fetch_optional(self.pool())
        .await?;
        Ok(row.map(|r| r.email))
    }

    /// The email addresses of many users at once, keyed by user id — for any
    /// surface that labels a list of people (a chat room's authors, a task's
    /// followers, a space's members).
    ///
    /// One query rather than one per id: a page of chat history can name fifty
    /// authors, and [`email_of`](Self::email_of) in a loop turns rendering a
    /// screen into fifty round trips.
    ///
    /// Ids not in this tenant are simply absent from the map — a caller cannot
    /// use this to discover whether a foreign user exists, only to label the
    /// people it was already allowed to see.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn emails_of(
        &self,
        users: &[UserId],
    ) -> Result<std::collections::HashMap<String, String>> {
        emails_of_ids(self.pool(), self.tenant().as_str(), users).await
    }

    // ---- mailbox delegation (ADR 0017) --------------------------------

    /// Grants `delegate` access to `owner`'s mailbox in this tenant (creating or
    /// updating the grant). `can_write` allows managing the mailbox (move / flag
    /// / delete), else read-only. `send_mode` is `none` / `as` (send as the
    /// owner's address) / `on_behalf` (send with a `Sender:` of the delegate);
    /// any send mode implies write. Both users must be in this tenant.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if either user is not in this tenant;
    /// [`StoreError::Conflict`] if owner and delegate are the same user, or the
    /// send mode is not one of the accepted values; [`StoreError::Db`] on failure.
    pub async fn grant_delegate(
        &self,
        owner: &UserId,
        delegate: &UserId,
        can_write: bool,
        send_mode: &str,
    ) -> Result<()> {
        if owner == delegate {
            return Err(StoreError::Conflict(
                "a user cannot delegate to themselves".into(),
            ));
        }
        if !matches!(send_mode, "none" | "as" | "on_behalf") {
            return Err(StoreError::Conflict("invalid send mode".into()));
        }
        // Sending requires the ability to create drafts — a send grant implies write.
        let can_write = can_write || send_mode != "none";
        self.assert_user(owner).await?;
        self.assert_user(delegate).await?;
        sqlx::query(
            "INSERT INTO account_delegates (tenant_id, owner_id, delegate_id, can_write, send_mode) \
             VALUES ($1, $2, $3, $4, $5) \
             ON CONFLICT (owner_id, delegate_id) DO UPDATE SET can_write = $4, send_mode = $5",
        )
        .bind(self.tenant().as_str())
        .bind(owner.as_str())
        .bind(delegate.as_str())
        .bind(can_write)
        .bind(send_mode)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Revokes `delegate`'s access to `owner`'s mailbox. Silent if absent.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn revoke_delegate(&self, owner: &UserId, delegate: &UserId) -> Result<()> {
        sqlx::query(
            "DELETE FROM account_delegates \
             WHERE tenant_id = $1 AND owner_id = $2 AND delegate_id = $3",
        )
        .bind(self.tenant().as_str())
        .bind(owner.as_str())
        .bind(delegate.as_str())
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// The delegate's grant on `owner`'s mailbox: `Some((can_write, send_mode))`
    /// if granted, `None` otherwise. The single authorization check on the
    /// request path — scoped to this tenant, so it can never authorize across
    /// tenants.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn delegation(
        &self,
        owner: &UserId,
        delegate: &UserId,
    ) -> Result<Option<(bool, String)>> {
        let row: Option<(bool, String)> = sqlx::query_as(
            "SELECT can_write, send_mode FROM account_delegates \
             WHERE tenant_id = $1 AND owner_id = $2 AND delegate_id = $3",
        )
        .bind(self.tenant().as_str())
        .bind(owner.as_str())
        .bind(delegate.as_str())
        .fetch_optional(self.pool())
        .await?;
        Ok(row)
    }

    /// The users granted access to `owner`'s mailbox — `(delegate id, email,
    /// can_write, send_mode)` — for the management UI.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn delegates_of(
        &self,
        owner: &UserId,
    ) -> Result<Vec<(String, String, bool, String)>> {
        let rows = sqlx::query_as::<_, (String, String, bool, String)>(
            "SELECT d.delegate_id, u.email, d.can_write, d.send_mode \
             FROM account_delegates d JOIN users u ON u.id = d.delegate_id \
             WHERE d.tenant_id = $1 AND d.owner_id = $2 ORDER BY u.email",
        )
        .bind(self.tenant().as_str())
        .bind(owner.as_str())
        .fetch_all(self.pool())
        .await?;
        Ok(rows)
    }

    /// The mailboxes `delegate` may access — `(owner id, owner email, can_write,
    /// send_mode)` — for the session's shared-account list.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn delegations_for(
        &self,
        delegate: &UserId,
    ) -> Result<Vec<(String, String, bool, String)>> {
        let rows = sqlx::query_as::<_, (String, String, bool, String)>(
            "SELECT d.owner_id, u.email, d.can_write, d.send_mode \
             FROM account_delegates d JOIN users u ON u.id = d.owner_id \
             WHERE d.tenant_id = $1 AND d.delegate_id = $2 ORDER BY u.email",
        )
        .bind(self.tenant().as_str())
        .bind(delegate.as_str())
        .fetch_all(self.pool())
        .await?;
        Ok(rows)
    }

    /// Restricts `delegate`'s access to `owner`'s mailbox to exactly the given
    /// folders (ADR 0017, Outlook parity). An empty list clears the restriction
    /// — the grant reverts to whole-mailbox. The grant must already exist (the
    /// rows hang off it via a cascading foreign key).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn set_delegate_folders(
        &self,
        owner: &UserId,
        delegate: &UserId,
        mailboxes: &[String],
    ) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        sqlx::query(
            "DELETE FROM delegate_folders \
             WHERE tenant_id = $1 AND owner_id = $2 AND delegate_id = $3",
        )
        .bind(self.tenant().as_str())
        .bind(owner.as_str())
        .bind(delegate.as_str())
        .execute(&mut *tx)
        .await?;
        for mb in mailboxes {
            sqlx::query(
                "INSERT INTO delegate_folders (tenant_id, owner_id, delegate_id, mailbox_id) \
                 VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING",
            )
            .bind(self.tenant().as_str())
            .bind(owner.as_str())
            .bind(delegate.as_str())
            .bind(mb)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// The folders `delegate` is restricted to on `owner`'s mailbox. An empty
    /// vec means **no restriction** — whole-mailbox access (the default).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn delegate_folders(&self, owner: &UserId, delegate: &UserId) -> Result<Vec<String>> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT mailbox_id FROM delegate_folders \
             WHERE tenant_id = $1 AND owner_id = $2 AND delegate_id = $3",
        )
        .bind(self.tenant().as_str())
        .bind(owner.as_str())
        .bind(delegate.as_str())
        .fetch_all(self.pool())
        .await?;
        Ok(rows.into_iter().map(|(m,)| m).collect())
    }

    // ---- aliases ------------------------------------------------------

    /// Adds an inbound alias address (lowercased) that routes to `user`.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the user is not in this tenant;
    /// [`StoreError::Conflict`] if the address is already in use.
    pub async fn add_alias(&self, user: &UserId, address: &str) -> Result<()> {
        self.assert_user(user).await?;
        sqlx::query!(
            "INSERT INTO aliases (address, tenant_id, user_id) VALUES ($1, $2, $3)",
            address.to_lowercase(),
            self.tenant().as_str(),
            user.as_str()
        )
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Removes an alias in this tenant. Silent if absent.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn remove_alias(&self, address: &str) -> Result<()> {
        sqlx::query!(
            "DELETE FROM aliases WHERE tenant_id = $1 AND address = $2",
            self.tenant().as_str(),
            address.to_lowercase()
        )
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Lists a user's alias addresses within this tenant.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn aliases_of(&self, user: &UserId) -> Result<Vec<String>> {
        let rows = sqlx::query!(
            "SELECT address FROM aliases WHERE tenant_id = $1 AND user_id = $2 ORDER BY address",
            self.tenant().as_str(),
            user.as_str()
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows.into_iter().map(|r| r.address).collect())
    }

    // ---- groups -------------------------------------------------------

    /// Creates a named group in this tenant.
    ///
    /// # Errors
    /// [`StoreError::Conflict`] if the name is taken in this tenant.
    pub async fn create_group(&self, name: &str) -> Result<GroupId> {
        let id = GroupId::generate();
        sqlx::query!(
            "INSERT INTO groups (id, tenant_id, name) VALUES ($1, $2, $3)",
            id.as_str(),
            self.tenant().as_str(),
            name
        )
        .execute(self.pool())
        .await?;
        Ok(id)
    }

    /// Renames a group in this tenant. Runtime query (kept off the offline
    /// cache path, like the other newer writes).
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the group isn't this tenant's;
    /// [`StoreError::Conflict`] if the name is taken in this tenant.
    pub async fn rename_group(&self, group: &GroupId, name: &str) -> Result<()> {
        let done = sqlx::query("UPDATE groups SET name = $3 WHERE tenant_id = $1 AND id = $2")
            .bind(self.tenant().as_str())
            .bind(group.as_str())
            .bind(name)
            .execute(self.pool())
            .await?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Adds a user to a group (both in this tenant). Idempotent.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the user or group is not in this tenant.
    pub async fn add_group_member(&self, group: &GroupId, user: &UserId) -> Result<()> {
        self.assert_user(user).await?;
        self.assert_group(group).await?;
        sqlx::query!(
            "INSERT INTO group_members (group_id, tenant_id, user_id) VALUES ($1, $2, $3) \
             ON CONFLICT (group_id, user_id) DO NOTHING",
            group.as_str(),
            self.tenant().as_str(),
            user.as_str()
        )
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Removes a user from a group. Silent if not a member.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn remove_group_member(&self, group: &GroupId, user: &UserId) -> Result<()> {
        sqlx::query!(
            "DELETE FROM group_members WHERE group_id = $1 AND tenant_id = $2 AND user_id = $3",
            group.as_str(),
            self.tenant().as_str(),
            user.as_str()
        )
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Lists the user ids in a group within this tenant.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn group_members(&self, group: &GroupId) -> Result<Vec<UserId>> {
        let rows = sqlx::query!(
            "SELECT user_id FROM group_members WHERE group_id = $1 AND tenant_id = $2",
            group.as_str(),
            self.tenant().as_str()
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows.into_iter().map(|r| UserId::new(r.user_id)).collect())
    }

    /// All groups in this tenant with their optional list address and member
    /// count, for the admin console. Runtime-checked query.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn list_groups(&self) -> Result<Vec<crate::model::GroupRow>> {
        let rows = sqlx::query_as::<_, (String, String, Option<String>, i64)>(
            "SELECT g.id, g.name, g.address, \
               (SELECT count(*) FROM group_members m WHERE m.group_id = g.id)::bigint AS members \
             FROM groups g WHERE g.tenant_id = $1 ORDER BY g.name",
        )
        .bind(self.tenant().as_str())
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|(id, name, address, member_count)| crate::model::GroupRow {
                id,
                name,
                address,
                member_count,
            })
            .collect())
    }

    /// Deletes a group (its memberships cascade). Runtime-checked query.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn delete_group(&self, group: &GroupId) -> Result<()> {
        sqlx::query("DELETE FROM groups WHERE tenant_id = $1 AND id = $2")
            .bind(self.tenant().as_str())
            .bind(group.as_str())
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Sets (or clears with `None`) a group's distribution-list address; stored
    /// lowercase. Runtime-checked query.
    ///
    /// # Errors
    /// [`StoreError::Conflict`] if the address is already in use;
    /// [`StoreError::Db`] on other failure.
    pub async fn set_group_address(&self, group: &GroupId, address: Option<&str>) -> Result<()> {
        let normalized = address.map(|a| a.trim().to_lowercase());
        sqlx::query("UPDATE groups SET address = $3 WHERE tenant_id = $1 AND id = $2")
            .bind(self.tenant().as_str())
            .bind(group.as_str())
            .bind(normalized)
            .execute(self.pool())
            .await
            .map_err(|e| match &e {
                sqlx::Error::Database(db) if db.is_unique_violation() => {
                    StoreError::Conflict("group address in use".to_owned())
                }
                _ => StoreError::Db(e),
            })?;
        Ok(())
    }

    /// A group's members as `(user_id, email)` pairs within this tenant, for the
    /// admin UI. Runtime-checked query.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn group_members_detailed(&self, group: &GroupId) -> Result<Vec<(String, String)>> {
        let rows = sqlx::query_as::<_, (String, String)>(
            "SELECT u.id, u.email FROM group_members m \
             JOIN users u ON u.id = m.user_id AND u.tenant_id = m.tenant_id \
             WHERE m.group_id = $1 AND m.tenant_id = $2 ORDER BY u.email",
        )
        .bind(group.as_str())
        .bind(self.tenant().as_str())
        .fetch_all(self.pool())
        .await?;
        Ok(rows)
    }

    /// Confirms a group exists in this tenant; `NotFound` otherwise.
    async fn assert_group(&self, group: &GroupId) -> Result<()> {
        sqlx::query!(
            "SELECT 1 AS one FROM groups WHERE tenant_id = $1 AND id = $2",
            self.tenant().as_str(),
            group.as_str()
        )
        .fetch_optional(self.pool())
        .await?
        .ok_or(StoreError::NotFound)
        .map(|_| ())
    }

    // ---- TOTP + recovery codes ---------------------------------------

    /// Stores (or replaces) a user's TOTP secret, initially **disabled**
    /// (an unconfirmed secret never gates login).
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the user is not in this tenant.
    pub async fn set_totp_secret(&self, user: &UserId, secret: &[u8]) -> Result<()> {
        self.assert_user(user).await?;
        sqlx::query!(
            "INSERT INTO totp_secrets (user_id, tenant_id, secret, enabled) \
             VALUES ($1, $2, $3, FALSE) \
             ON CONFLICT (user_id) DO UPDATE SET secret = $3, enabled = FALSE",
            user.as_str(),
            self.tenant().as_str(),
            secret
        )
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Marks a user's TOTP enrollment confirmed (enabled). No-op if none.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn enable_totp(&self, user: &UserId) -> Result<()> {
        sqlx::query!(
            "UPDATE totp_secrets SET enabled = TRUE WHERE tenant_id = $1 AND user_id = $2",
            self.tenant().as_str(),
            user.as_str()
        )
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Removes a user's TOTP enrollment (disable 2FA). Silent if none.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn clear_totp(&self, user: &UserId) -> Result<()> {
        sqlx::query!(
            "DELETE FROM totp_secrets WHERE tenant_id = $1 AND user_id = $2",
            self.tenant().as_str(),
            user.as_str()
        )
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// A user's TOTP row (secret + enabled flag), if enrolled.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn totp_of(&self, user: &UserId) -> Result<Option<TotpRow>> {
        let row = sqlx::query!(
            "SELECT secret, enabled FROM totp_secrets WHERE tenant_id = $1 AND user_id = $2",
            self.tenant().as_str(),
            user.as_str()
        )
        .fetch_optional(self.pool())
        .await?;
        Ok(row.map(|r| TotpRow {
            secret: r.secret,
            enabled: r.enabled,
        }))
    }

    /// Replaces a user's recovery codes with a fresh set of SHA-256 hashes
    /// (the plaintext codes are shown once by `alo-identity` and never
    /// stored).
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the user is not in this tenant.
    pub async fn set_recovery_codes(&self, user: &UserId, code_hashes: &[String]) -> Result<()> {
        self.assert_user(user).await?;
        let mut tx = self.pool().begin().await.map_err(StoreError::Db)?;
        sqlx::query!(
            "DELETE FROM recovery_codes WHERE tenant_id = $1 AND user_id = $2",
            self.tenant().as_str(),
            user.as_str()
        )
        .execute(&mut *tx)
        .await?;
        for hash in code_hashes {
            let id = crate::id::MessageId::generate();
            sqlx::query!(
                "INSERT INTO recovery_codes (id, tenant_id, user_id, code_hash) \
                 VALUES ($1, $2, $3, $4)",
                id.as_str(),
                self.tenant().as_str(),
                user.as_str(),
                hash
            )
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(())
    }

    /// A user's unused recovery codes as `(id, code_hash)` pairs, for
    /// `alo-identity` to constant-time compare against a presented code.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn unused_recovery_codes(&self, user: &UserId) -> Result<Vec<(String, String)>> {
        let rows = sqlx::query!(
            "SELECT id, code_hash FROM recovery_codes \
             WHERE tenant_id = $1 AND user_id = $2 AND used_at IS NULL",
            self.tenant().as_str(),
            user.as_str()
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows.into_iter().map(|r| (r.id, r.code_hash)).collect())
    }

    /// Atomically marks one recovery code used. Returns `true` if this call
    /// consumed it, `false` if it was already used (single-use gate).
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn consume_recovery_code(&self, id: &str) -> Result<bool> {
        let updated = sqlx::query!(
            "UPDATE recovery_codes SET used_at = now() \
             WHERE id = $1 AND tenant_id = $2 AND used_at IS NULL RETURNING id",
            id,
            self.tenant().as_str()
        )
        .fetch_optional(self.pool())
        .await?;
        Ok(updated.is_some())
    }

    // ---- OAuth issuance ----------------------------------------------

    /// Issues (persists) an authorization code for an authenticated user.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    #[allow(clippy::too_many_arguments)]
    pub async fn issue_auth_code(
        &self,
        user: &UserId,
        code_hash: &str,
        client_id: &str,
        redirect_uri: &str,
        code_challenge: &str,
        scope: &str,
        nonce: Option<&str>,
        expires_at: OffsetDateTime,
    ) -> Result<()> {
        sqlx::query!(
            "INSERT INTO oauth_auth_codes \
               (code_hash, tenant_id, user_id, client_id, redirect_uri, code_challenge, \
                scope, nonce, expires_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
            code_hash,
            self.tenant().as_str(),
            user.as_str(),
            client_id,
            redirect_uri,
            code_challenge,
            scope,
            nonce,
            expires_at
        )
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Persists an issued access token (opaque; only its hash is stored).
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn issue_access_token(
        &self,
        user: &UserId,
        token_hash: &str,
        client_id: Option<&str>,
        scope: &str,
        expires_at: OffsetDateTime,
    ) -> Result<()> {
        sqlx::query!(
            "INSERT INTO access_tokens \
               (token_hash, tenant_id, user_id, client_id, scope, expires_at) \
             VALUES ($1, $2, $3, $4, $5, $6)",
            token_hash,
            self.tenant().as_str(),
            user.as_str(),
            client_id,
            scope,
            expires_at
        )
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Persists an issued refresh token (opaque; only its hash is stored).
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn issue_refresh_token(
        &self,
        user: &UserId,
        token_hash: &str,
        client_id: &str,
        scope: &str,
        expires_at: OffsetDateTime,
    ) -> Result<()> {
        sqlx::query!(
            "INSERT INTO refresh_tokens \
               (token_hash, tenant_id, user_id, client_id, scope, expires_at) \
             VALUES ($1, $2, $3, $4, $5, $6)",
            token_hash,
            self.tenant().as_str(),
            user.as_str(),
            client_id,
            scope,
            expires_at
        )
        .execute(self.pool())
        .await?;
        Ok(())
    }
}

/// Email addresses for a set of user ids within one tenant, keyed by id.
///
/// The shared body behind [`TenantStore::emails_of`] and the account-scoped
/// callers that need the same answer (chat resolving `@handles` against a
/// room's members). It lives as a free function so that reaching it does not
/// require a [`TenantStore`] — an account door should not have to widen itself
/// to a tenant door just to put a name beside a message.
///
/// Always bounded by `tenant`: ids from elsewhere are absent from the map, not
/// errors, so this cannot be used to discover whether a foreign user exists.
///
/// # Errors
/// [`StoreError::Db`] on a database failure.
pub(crate) async fn emails_of_ids(
    pool: &sqlx::PgPool,
    tenant: &str,
    users: &[UserId],
) -> Result<std::collections::HashMap<String, String>> {
    if users.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let mut ids: Vec<String> = users.iter().map(|u| u.as_str().to_owned()).collect();
    ids.sort_unstable();
    ids.dedup();
    let rows = sqlx::query!(
        "SELECT id, email FROM users WHERE tenant_id = $1 AND id = ANY($2)",
        tenant,
        &ids[..]
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| (r.id, r.email)).collect())
}
