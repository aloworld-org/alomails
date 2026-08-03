//! Server configuration from the environment, mirroring `alo-smtp`'s
//! `ALO_*` conventions. Listeners are off unless their address is set.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

/// IMAP/POP3 server configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// STARTTLS IMAP listener (143). Cleartext until STARTTLS; auth
    /// refused before TLS.
    pub imap_addr: Option<SocketAddr>,
    /// Implicit-TLS IMAP listener (993).
    pub imaps_addr: Option<SocketAddr>,
    /// Implicit-TLS POP3 listener (995).
    pub pop3s_addr: Option<SocketAddr>,
    /// Hostname used in greetings and for the self-signed dev cert.
    pub hostname: String,
    /// PEM certificate path (with `tls_key`), else self-signed in dev.
    pub tls_cert: Option<PathBuf>,
    /// PEM private-key path.
    pub tls_key: Option<PathBuf>,
    /// Permit generating a self-signed certificate when no PEM is set
    /// (development only).
    pub allow_self_signed: bool,
    /// Largest literal ({n}) the parser will accept — bounds APPEND and
    /// any literal argument before allocation.
    pub max_literal: usize,
    /// Longest non-literal command line accepted before `BAD`.
    pub max_line: usize,
    /// Idle-connection timeout (also the ceiling for an IDLE without a
    /// heartbeat from us).
    pub idle_timeout: Duration,
    /// Failed authentications tolerated before the connection is dropped.
    pub max_auth_failures: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            imap_addr: None,
            imaps_addr: None,
            pop3s_addr: None,
            hostname: "localhost".to_owned(),
            tls_cert: None,
            tls_key: None,
            allow_self_signed: false,
            max_literal: 25 * 1024 * 1024,
            max_line: 8192,
            idle_timeout: Duration::from_secs(30 * 60),
            max_auth_failures: 3,
        }
    }
}

impl Config {
    /// Builds a configuration from `ALO_IMAP_*`/`ALO_POP3_*`
    /// environment variables, falling back to defaults.
    ///
    /// # Errors
    /// [`crate::ImapError::Config`] if an address fails to parse.
    pub fn from_env() -> crate::Result<Self> {
        let d = Self::default();
        Ok(Self {
            imap_addr: parse_addr("ALO_IMAP_ADDR")?,
            imaps_addr: parse_addr("ALO_IMAPS_ADDR")?,
            pop3s_addr: parse_addr("ALO_POP3S_ADDR")?,
            hostname: std::env::var("ALO_IMAP_HOSTNAME").unwrap_or(d.hostname),
            tls_cert: std::env::var("ALO_IMAP_TLS_CERT").ok().map(PathBuf::from),
            tls_key: std::env::var("ALO_IMAP_TLS_KEY").ok().map(PathBuf::from),
            allow_self_signed: std::env::var("ALO_IMAP_ALLOW_SELF_SIGNED")
                .map(|v| v == "true" || v == "1")
                .unwrap_or(false),
            max_literal: d.max_literal,
            max_line: d.max_line,
            idle_timeout: d.idle_timeout,
            max_auth_failures: d.max_auth_failures,
        })
    }
}

fn parse_addr(key: &str) -> crate::Result<Option<SocketAddr>> {
    match std::env::var(key) {
        Ok(v) => v
            .parse::<SocketAddr>()
            .map(Some)
            .map_err(|e| crate::ImapError::Config {
                message: format!("{key}: invalid socket address {v:?}: {e}"),
            }),
        Err(_) => Ok(None),
    }
}
