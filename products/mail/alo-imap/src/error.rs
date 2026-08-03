//! Crate error type. Wire-facing failures are turned into tagged `NO`/
//! `BAD` responses at the session edge; only genuinely fatal transport/
//! setup errors surface as `ImapError`.

use thiserror::Error;

/// A fatal error in the IMAP/POP3 server (transport, TLS, store wiring).
/// Per-command failures are **not** these — they become tagged responses.
#[derive(Debug, Error)]
pub enum ImapError {
    /// TLS setup/handshake failure.
    #[error("tls error: {message}")]
    Tls {
        /// What failed (never carries key material).
        message: String,
    },
    /// A listener could not bind or accept.
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    /// A store operation failed fatally during setup.
    #[error("store error: {0}")]
    Store(#[from] alo_store::StoreError),
    /// Invalid configuration (e.g. an unparseable address).
    #[error("configuration error: {message}")]
    Config {
        /// What was wrong.
        message: String,
    },
}

/// Crate result alias.
pub type Result<T> = std::result::Result<T, ImapError>;
