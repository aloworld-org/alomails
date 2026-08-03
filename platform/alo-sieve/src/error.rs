//! Compile errors. A compile error is a *user-visible validation result*
//! (surfaced by `SieveScript/set` as `invalidScript`), so its message is
//! safe to show the script author — it never carries another user's data.

use thiserror::Error;

/// A Sieve compilation failure.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum CompileError {
    /// A syntax error at parse time, with a human-readable reason.
    #[error("syntax error: {0}")]
    Syntax(String),
    /// A command/test/comparator from an extension not `require`d.
    #[error("extension not required: {0}")]
    MissingRequire(String),
    /// A `require` for an extension this engine does not implement.
    #[error("unsupported extension: {0}")]
    UnsupportedExtension(String),
    /// The script exceeds a hard limit (size, depth, test-list, literal).
    #[error("limit exceeded: {0}")]
    LimitExceeded(String),
}

/// Compile result alias.
pub type Result<T> = std::result::Result<T, CompileError>;
