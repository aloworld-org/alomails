//! # alo-identity — the credential authority
//!
//! alo's single source of truth for *who* a request is: argon2id
//! passwords, TOTP 2FA, opaque revocable tokens, and an OpenID Connect /
//! OAuth 2.0 provider (alo-as-IdP). SMTP AUTH, IMAP/POP3 `LOGIN`, and
//! the JMAP bearer all authenticate through here; the store beneath owns
//! persistence and the tenancy-by-construction door (`for_account`).
//!
//! Design: `docs/design/identity.md`; token model: `docs/decisions/0008`.
//!
//! **Every secret comparison is constant-time** (see [`secret::ct_eq`] and
//! [`password::Passwords::verify_or_dummy`]); no secret enters a log, an
//! error, or a `Debug`.

use std::sync::Arc;

use alo_store::{Store, StoreError, TenantId, UserId};
use tokio::sync::Semaphore;

pub mod app_password;
pub mod config;
pub mod jwt;
pub mod keys;
pub mod oauth;
pub mod password;
pub mod provision;
pub mod ratelimit;
pub mod secret;
pub mod signup;
pub mod site_invites;
pub mod token;
pub mod totp;
mod user_invites;
pub mod xoauth2;

pub use config::{ConfigError, IdentityConfig};
pub use oauth::router;
pub use password::Passwords;
pub use ratelimit::RateLimiter;

/// The OIDC `openid` scope — required for any OIDC request; grants `sub`.
pub const SCOPE_OPENID: &str = "openid";
/// The `email` scope — grants the `email`/`email_verified` claims.
pub const SCOPE_EMAIL: &str = "email";
/// The `profile` scope — grants `name`/`preferred_username`.
pub const SCOPE_PROFILE: &str = "profile";
/// The `offline_access` scope — requests a refresh token.
pub const SCOPE_OFFLINE: &str = "offline_access";

/// Why an identity operation failed. Internal detail (SQL, crypto) never
/// reaches a caller verbatim.
#[derive(Debug, thiserror::Error)]
pub enum IdentityError {
    /// A persistence failure in the store beneath.
    #[error("identity store error")]
    Store(#[from] StoreError),
    /// A cryptographic failure (hashing, signing, RNG).
    #[error("identity crypto error")]
    Crypto,
    /// A configuration error.
    #[error("identity config error")]
    Config(#[from] ConfigError),
    /// No signing key is provisioned (bootstrap incomplete).
    #[error("no signing key configured")]
    NoSigningKey,
}

/// An identity result.
pub type Result<T> = std::result::Result<T, IdentityError>;

/// An authenticated principal: the `(tenant, user)` the tenant door
/// consumes, plus the granted OAuth scope (empty for legacy-protocol auth,
/// which grants no OAuth capability).
#[derive(Debug, Clone)]
pub struct Principal {
    /// The principal's tenant.
    pub tenant: TenantId,
    /// The principal's user.
    pub user: UserId,
    /// The granted scope (space-separated), or empty for protocol logins.
    pub scope: String,
}

impl Principal {
    /// A principal with no OAuth scope (a protocol login).
    pub fn protocol(tenant: TenantId, user: UserId) -> Self {
        Self {
            tenant,
            user,
            scope: String::new(),
        }
    }
}

/// The credential authority. Cheap to clone (an `Arc` plus config).
#[derive(Clone)]
pub struct Identity {
    store: Arc<Store>,
    cfg: IdentityConfig,
    passwords: Arc<Passwords>,
    rate: RateLimiter,
    /// Bounds concurrent argon2 hashes so a flood of auth attempts cannot
    /// exhaust memory (each argon2id pass allocates ~19 MiB). Held for the
    /// duration of one hash/verify.
    argon2_slots: Arc<Semaphore>,
}

impl Identity {
    /// Builds the authority over a store handle and configuration.
    ///
    /// # Errors
    /// [`IdentityError::Crypto`] if the argon2 parameters are invalid.
    pub fn new(store: Arc<Store>, cfg: IdentityConfig) -> Result<Self> {
        let passwords = Passwords::new(&cfg).map_err(|_| IdentityError::Crypto)?;
        // Cap concurrent argon2 work at the core count: argon2id is CPU- and
        // memory-hard, so core-count concurrency saturates the CPU while
        // bounding peak memory (permits × ~19 MiB).
        let slots = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        Ok(Self {
            store,
            cfg,
            passwords: Arc::new(passwords),
            rate: RateLimiter::new(),
            argon2_slots: Arc::new(Semaphore::new(slots)),
        })
    }

    /// The configuration this authority was built with.
    pub fn config(&self) -> &IdentityConfig {
        &self.cfg
    }

    /// The store beneath (for callers that also need mail-data access via
    /// `for_account`).
    pub fn store(&self) -> &Arc<Store> {
        &self.store
    }

    /// Verifies a username/password with **no 2FA** — the entry point for
    /// legacy protocols (SMTP AUTH, IMAP/POP3 `LOGIN`) that cannot prompt
    /// for a TOTP code. A wrong password and an unknown user are
    /// indistinguishable in time (dummy argon2 verify) and in result
    /// (`Ok(None)`). Returns a scope-less [`Principal`] on success.
    ///
    /// Note: for a 2FA-enabled user this still authenticates on the account
    /// password — the fail-closed 2FA policy (and the app-password
    /// alternative) lives one level up in
    /// [`Identity::authenticate_legacy`], the entry point the protocols
    /// actually call (see `docs/design/identity.md`).
    ///
    /// # Errors
    /// [`IdentityError::Store`] on a persistence failure.
    pub async fn authenticate_password(
        &self,
        username: &str,
        password: &str,
    ) -> Result<Option<Principal>> {
        let cred = self.store.credentials_by_username(username).await?;
        // One argon2 pass runs below (verify, or the unknown-user dummy);
        // the permit bounds concurrent memory-hard work.
        let _permit = self.argon2_slots.acquire().await;
        match cred {
            Some(c) if self.passwords.verify(password, &c.password_hash) => {
                Ok(Some(Principal::protocol(c.tenant, c.user)))
            }
            // Known user, wrong password: the argon2 verify above already
            // ran, so this path's timing matches the unknown-user path.
            Some(_) => Ok(None),
            None => {
                // Unknown user: pay one argon2 cost so timing does not leak
                // existence.
                let _ = self.passwords.verify_or_dummy(password, None);
                Ok(None)
            }
        }
    }

    /// Authenticates a legacy mail protocol (SMTP AUTH, IMAP/POP3 `LOGIN`,
    /// DAV HTTP Basic) where there is nowhere to prompt for a second
    /// factor. On top of [`Identity::authenticate_password`] it adds the
    /// protections the bare password check lacks:
    ///
    /// - **Fail closed for a 2FA account's primary password.** A basic-auth
    ///   exchange cannot carry a TOTP code, so if the account has TOTP
    ///   **enabled** the primary password is refused (returns `None`,
    ///   indistinguishable from a wrong password — no oracle). A phished
    ///   primary therefore cannot bypass 2FA over IMAP/SMTP. Such a user
    ///   connects a legacy client with an app-specific password instead.
    /// - **App-specific passwords.** A secret that is not an accepted
    ///   primary is tried against the user's app passwords
    ///   ([`Identity::verify_app_password`]) — server-generated per-client
    ///   credentials that carry no second-factor obligation, because they
    ///   are issued from inside an already-authenticated session and are
    ///   never a phishable, human-chosen secret. The 2FA-refusal path runs
    ///   the same check, so "correct primary, policy-refused" costs the
    ///   same argon2 work as "wrong password" and timing cannot say which
    ///   happened.
    /// - **Per-username backoff across connections.** The protocols already
    ///   cap failures per connection; this adds a shared exponential backoff
    ///   keyed on the username so an attacker rotating TCP connections is
    ///   still throttled. A correct-password 2FA refusal is *not* counted as
    ///   a failure (the user is not guessing).
    ///
    /// # Errors
    /// [`IdentityError::Store`] on a persistence failure.
    pub async fn authenticate_legacy(
        &self,
        username: &str,
        password: &str,
    ) -> Result<Option<Principal>> {
        let key = format!("legacy|{username}");
        if self.rate.retry_after(&key).is_some() {
            // Backed off: an indistinguishable failure (no lockout oracle).
            return Ok(None);
        }
        match self.authenticate_password(username, password).await? {
            Some(principal) => {
                if !self
                    .totp_enabled(&principal.tenant, &principal.user)
                    .await?
                {
                    self.rate.record_success(&key);
                    return Ok(Some(principal));
                }
                tracing::info!(
                    "legacy auth refused primary password: account has 2FA enabled — use an app password or the OIDC flow"
                );
                // Run the app-password check anyway: it keeps this path's
                // cost equal to the wrong-password path below (no "correct
                // primary" timing oracle), and in the astronomically
                // unlikely case the secret also IS an app password, that
                // credential is valid in its own right.
                match self.verify_app_password(username, password).await? {
                    Some(principal) => {
                        self.rate.record_success(&key);
                        Ok(Some(principal))
                    }
                    // Correct password, but policy-refused: no failure
                    // strike — the user is not guessing.
                    None => Ok(None),
                }
            }
            None => {
                // Not the primary password (or an unknown user): try the
                // app passwords. The unknown-user path pays the dummy hash
                // inside, so timing still never reveals user existence.
                match self.verify_app_password(username, password).await? {
                    Some(principal) => {
                        self.rate.record_success(&key);
                        Ok(Some(principal))
                    }
                    None => {
                        self.rate.record_failure(&key);
                        Ok(None)
                    }
                }
            }
        }
    }

    /// Sets (or replaces) a user's password, argon2id-hashing it here — the
    /// store only ever holds the PHC string.
    ///
    /// # Errors
    /// [`IdentityError::Crypto`] on a hashing failure;
    /// [`IdentityError::Store`] if the user is unknown or the username taken.
    pub async fn set_password(
        &self,
        tenant: &TenantId,
        user: &UserId,
        username: &str,
        password: &str,
    ) -> Result<()> {
        let hash = {
            let _permit = self.argon2_slots.acquire().await;
            self.passwords
                .hash(password)
                .map_err(|_| IdentityError::Crypto)?
        };
        self.store
            .for_tenant(tenant.clone())
            .set_password_hash(user, username, &hash)
            .await?;
        Ok(())
    }

    /// The rate limiter guarding credential endpoints.
    pub(crate) fn rate_limiter(&self) -> &RateLimiter {
        &self.rate
    }
}
