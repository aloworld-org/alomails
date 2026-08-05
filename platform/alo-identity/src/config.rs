//! Identity configuration (`ALO_IDENTITY_*`), following the same
//! env-var convention as `alo-smtp`/`alo-imap`. Token lifetimes and
//! argon2id parameters are here because they are operational contracts:
//! the argon2 params are documented in `docs/design/identity.md`, and a
//! stored PHC hash is self-describing so raising them stays backward
//! compatible.

use std::env;

use time::Duration;

use crate::secret::Secret;

/// Env var for the OIDC issuer (the `iss` claim and discovery base URL).
pub const ENV_ISSUER: &str = "ALO_IDENTITY_ISSUER";
/// Env var for the resource-server secret that guards token introspection
/// (RFC 7662). Unset ⇒ the introspection endpoint is disabled entirely.
pub const ENV_INTROSPECT_SECRET: &str = "ALO_IDENTITY_INTROSPECT_SECRET";
/// Env var for the access-token lifetime, in seconds.
pub const ENV_ACCESS_TTL: &str = "ALO_IDENTITY_ACCESS_TTL_SECS";
/// Env var for the refresh-token lifetime, in seconds.
pub const ENV_REFRESH_TTL: &str = "ALO_IDENTITY_REFRESH_TTL_SECS";
/// Env var for the authorization-code lifetime, in seconds.
pub const ENV_CODE_TTL: &str = "ALO_IDENTITY_CODE_TTL_SECS";
/// Env var for the argon2id memory cost, in KiB.
pub const ENV_ARGON2_M: &str = "ALO_IDENTITY_ARGON2_M_KIB";
/// Env var for the argon2id time cost (iterations).
pub const ENV_ARGON2_T: &str = "ALO_IDENTITY_ARGON2_T";
/// Env var for the argon2id parallelism (lanes).
pub const ENV_ARGON2_P: &str = "ALO_IDENTITY_ARGON2_P";

/// OWASP-recommended argon2id baseline memory cost (KiB): 19 MiB.
pub const DEFAULT_ARGON2_M_KIB: u32 = 19_456;
/// OWASP-recommended argon2id baseline time cost.
pub const DEFAULT_ARGON2_T: u32 = 2;
/// OWASP-recommended argon2id baseline parallelism.
pub const DEFAULT_ARGON2_P: u32 = 1;

/// Why identity configuration failed to load.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// A required variable was unset.
    #[error("missing required config: {0}")]
    Missing(&'static str),
    /// A variable held an unparseable value (the variable is named; its
    /// value is not echoed).
    #[error("invalid config value for {0}")]
    Invalid(&'static str),
}

/// Identity runtime configuration.
#[derive(Debug, Clone)]
pub struct IdentityConfig {
    /// The OIDC issuer URL (no trailing slash), e.g. `https://id.example`.
    pub issuer: String,
    /// Access-token lifetime.
    pub access_ttl: Duration,
    /// Refresh-token lifetime.
    pub refresh_ttl: Duration,
    /// Authorization-code lifetime.
    pub code_ttl: Duration,
    /// argon2id memory cost (KiB).
    pub argon2_m_kib: u32,
    /// argon2id time cost.
    pub argon2_t: u32,
    /// argon2id parallelism.
    pub argon2_p: u32,
    /// The shared secret a resource server (a standalone product like Drive)
    /// presents to call the token-introspection endpoint. `None` disables
    /// introspection — the endpoint 404s — so it is off unless deliberately
    /// configured.
    pub introspect_secret: Option<Secret>,
}

impl IdentityConfig {
    /// A config for a given issuer, with the recommended defaults.
    pub fn new(issuer: impl Into<String>) -> Self {
        Self {
            issuer: issuer.into().trim_end_matches('/').to_owned(),
            access_ttl: Duration::hours(1),
            refresh_ttl: Duration::days(30),
            code_ttl: Duration::seconds(60),
            argon2_m_kib: DEFAULT_ARGON2_M_KIB,
            argon2_t: DEFAULT_ARGON2_T,
            argon2_p: DEFAULT_ARGON2_P,
            introspect_secret: None,
        }
    }

    /// Loads configuration from the environment.
    ///
    /// # Errors
    /// [`ConfigError::Missing`] if `ALO_IDENTITY_ISSUER` is unset;
    /// [`ConfigError::Invalid`] if any numeric variable is unparseable.
    pub fn from_env() -> Result<Self, ConfigError> {
        let issuer = env::var(ENV_ISSUER).map_err(|_| ConfigError::Missing(ENV_ISSUER))?;
        let mut cfg = Self::new(issuer);
        if let Some(secs) = env_u64(ENV_ACCESS_TTL)? {
            cfg.access_ttl = Duration::seconds(secs as i64);
        }
        if let Some(secs) = env_u64(ENV_REFRESH_TTL)? {
            cfg.refresh_ttl = Duration::seconds(secs as i64);
        }
        if let Some(secs) = env_u64(ENV_CODE_TTL)? {
            cfg.code_ttl = Duration::seconds(secs as i64);
        }
        if let Some(m) = env_u32(ENV_ARGON2_M)? {
            cfg.argon2_m_kib = m;
        }
        if let Some(t) = env_u32(ENV_ARGON2_T)? {
            cfg.argon2_t = t;
        }
        if let Some(p) = env_u32(ENV_ARGON2_P)? {
            cfg.argon2_p = p;
        }
        if let Ok(secret) = env::var(ENV_INTROSPECT_SECRET) {
            let secret = secret.trim();
            if !secret.is_empty() {
                cfg.introspect_secret = Some(Secret::new(secret));
            }
        }
        Ok(cfg)
    }

    /// The JWKS URI advertised in discovery and used to publish keys.
    pub fn jwks_uri(&self) -> String {
        format!("{}/oauth/jwks", self.issuer)
    }

    /// The authorization endpoint URL.
    pub fn authorization_endpoint(&self) -> String {
        format!("{}/oauth/authorize", self.issuer)
    }

    /// The token endpoint URL.
    pub fn token_endpoint(&self) -> String {
        format!("{}/oauth/token", self.issuer)
    }

    /// The userinfo endpoint URL.
    pub fn userinfo_endpoint(&self) -> String {
        format!("{}/oauth/userinfo", self.issuer)
    }

    /// The token-introspection endpoint URL (RFC 7662).
    pub fn introspection_endpoint(&self) -> String {
        format!("{}/oauth/introspect", self.issuer)
    }
}

fn env_u64(key: &'static str) -> Result<Option<u64>, ConfigError> {
    match env::var(key) {
        Ok(v) => v
            .trim()
            .parse::<u64>()
            .map(Some)
            .map_err(|_| ConfigError::Invalid(key)),
        Err(_) => Ok(None),
    }
}

fn env_u32(key: &'static str) -> Result<Option<u32>, ConfigError> {
    match env::var(key) {
        Ok(v) => v
            .trim()
            .parse::<u32>()
            .map(Some)
            .map_err(|_| ConfigError::Invalid(key)),
        Err(_) => Ok(None),
    }
}
