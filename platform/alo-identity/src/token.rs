//! Opaque access & refresh tokens: issued here, stored only as a SHA-256
//! hash, resolved and revoked by hash. Access tokens are short-lived and
//! **revocable** (a `revoked_at` checked on every use); refresh tokens are
//! longer-lived and rotated on use (`oauth.rs` orchestrates the grant).
//! See ADR 0008 for why access tokens are opaque rather than JWTs.

use time::OffsetDateTime;

use alo_store::{TenantId, UserId};

use crate::secret::{self, Secret};
use crate::totp::TotpOutcome;
use crate::{Identity, IdentityError, Principal, Result, SCOPE_EMAIL, SCOPE_OPENID, SCOPE_PROFILE};

impl Identity {
    /// Issues an opaque access token for `(tenant, user)` with `scope`,
    /// optionally bound to an OAuth `client_id`. Only the token's hash is
    /// stored; the token itself is returned once.
    ///
    /// # Errors
    /// [`IdentityError::Crypto`] on RNG failure; [`IdentityError::Store`] on
    /// a persistence failure.
    pub async fn issue_access_token(
        &self,
        tenant: &TenantId,
        user: &UserId,
        client_id: Option<&str>,
        scope: &str,
    ) -> Result<Secret> {
        let token = secret::random_token().map_err(|_| IdentityError::Crypto)?;
        let hash = secret::hash_at_rest(token.reveal());
        let expires_at = OffsetDateTime::now_utc() + self.config().access_ttl;
        self.store()
            .for_tenant(tenant.clone())
            .issue_access_token(user, &hash, client_id, scope, expires_at)
            .await?;
        Ok(token)
    }

    /// Issues an opaque refresh token bound to `(user, client, scope)`.
    ///
    /// # Errors
    /// [`IdentityError::Crypto`] on RNG failure; [`IdentityError::Store`] on
    /// a persistence failure.
    pub async fn issue_refresh_token(
        &self,
        tenant: &TenantId,
        user: &UserId,
        client_id: &str,
        scope: &str,
    ) -> Result<Secret> {
        let token = secret::random_token().map_err(|_| IdentityError::Crypto)?;
        let hash = secret::hash_at_rest(token.reveal());
        let expires_at = OffsetDateTime::now_utc() + self.config().refresh_ttl;
        self.store()
            .for_tenant(tenant.clone())
            .issue_refresh_token(user, &hash, client_id, scope, expires_at)
            .await?;
        Ok(token)
    }

    /// Resolves a presented access token to its principal, or `None` if it
    /// is unknown, expired, or revoked. The token is never compared
    /// directly — only its irreversible SHA-256 hash, looked up by index.
    ///
    /// # Errors
    /// [`IdentityError::Store`] on a persistence failure.
    pub async fn resolve_access_token(&self, presented: &str) -> Result<Option<Principal>> {
        let hash = secret::hash_at_rest(presented);
        let Some(row) = self.store().access_token(&hash).await? else {
            return Ok(None);
        };
        if row.revoked_at.is_some() || row.expires_at <= OffsetDateTime::now_utc() {
            return Ok(None);
        }
        Ok(Some(Principal {
            tenant: row.tenant,
            user: row.user,
            scope: row.scope,
        }))
    }

    /// Revokes a presented access token (logout). Idempotent; unknown
    /// tokens are a silent no-op (no existence oracle).
    ///
    /// # Errors
    /// [`IdentityError::Store`] on a persistence failure.
    pub async fn revoke_access_token(&self, presented: &str) -> Result<()> {
        let hash = secret::hash_at_rest(presented);
        self.store().revoke_access_token(&hash).await?;
        Ok(())
    }

    /// Revokes a presented refresh token. Idempotent.
    ///
    /// # Errors
    /// [`IdentityError::Store`] on a persistence failure.
    pub async fn revoke_refresh_token(&self, presented: &str) -> Result<()> {
        let hash = secret::hash_at_rest(presented);
        self.store().revoke_refresh_token(&hash).await?;
        Ok(())
    }

    /// The first-party programmatic login: verify password **and** (if
    /// enrolled) the second factor, then issue a full-scope access token.
    /// This backs the non-public JMAP `/auth/token` used by the raw JMAP
    /// client — a password POST that stays on alo, unlike the public
    /// OAuth flow. Unlike [`Identity::authenticate_password`] it **enforces
    /// 2FA**, so a TOTP user must supply `otp`. `None` on any failure
    /// (indistinguishable — no oracle).
    ///
    /// # Errors
    /// [`IdentityError::Crypto`]/[`IdentityError::Store`] on failure.
    pub async fn password_login(
        &self,
        username: &str,
        password: &str,
        otp: Option<&str>,
    ) -> Result<Option<(Secret, Principal)>> {
        // Per-username backoff so this internet-reachable password endpoint
        // is not an unthrottled online brute-force target (the SMTP/IMAP
        // AUTH paths have their own per-connection caps). When backed off we
        // refuse *before* argon2, which also sheds the hashing load.
        let key = format!("auth-token|{username}");
        if self.rate_limiter().retry_after(&key).is_some() {
            return Ok(None);
        }
        let Some(principal) = self.authenticate_password(username, password).await? else {
            self.rate_limiter().record_failure(&key);
            return Ok(None);
        };
        if self
            .check_second_factor(&principal.tenant, &principal.user, otp)
            .await?
            == TotpOutcome::Failed
        {
            self.rate_limiter().record_failure(&key);
            return Ok(None);
        }
        self.rate_limiter().record_success(&key);
        let scope = format!("{SCOPE_OPENID} {SCOPE_EMAIL} {SCOPE_PROFILE}");
        let token = self
            .issue_access_token(&principal.tenant, &principal.user, None, &scope)
            .await?;
        Ok(Some((token, principal)))
    }
}
