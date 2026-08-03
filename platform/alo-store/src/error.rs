//! Store errors. Internal SQL/S3 detail never reaches a caller
//! verbatim; a wrong-tenant lookup returns the same `NotFound` as a
//! truly absent row (no existence oracle across tenants).

/// Why a store operation failed.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// The row does not exist — or exists under a different tenant, which
    /// is indistinguishable by design.
    #[error("not found")]
    NotFound,
    /// A uniqueness/precondition conflict (e.g. duplicate mailbox name).
    #[error("conflict: {0}")]
    Conflict(String),
    /// An object exceeded the configured byte ceiling.
    #[error("object too large: {size} bytes exceeds limit of {limit}")]
    TooLarge {
        /// The offending size.
        size: usize,
        /// The configured limit.
        limit: usize,
    },
    /// The write would exceed the tenant's storage quota (ADR 0012). No size
    /// detail — the message never carries how full another tenant is.
    #[error("storage quota exceeded")]
    OverQuota,
    /// A database failure (detail in the source, never in the message).
    #[error("store database error")]
    Db(#[source] sqlx::Error),
    /// A schema-migration failure.
    #[error("store migration error")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    /// A credential-hashing or secure-random failure (auth path).
    #[error("credential crypto error")]
    Crypto,
    /// A blob-store failure.
    #[error("blob store error")]
    Blob(#[source] object_store::Error),
}

impl From<sqlx::Error> for StoreError {
    fn from(error: sqlx::Error) -> Self {
        match error {
            // A `fetch_one` with no row is a not-found, not a DB fault.
            sqlx::Error::RowNotFound => Self::NotFound,
            // Unique-violation → conflict (the SQLSTATE, not the detail).
            sqlx::Error::Database(ref db) if db.code().as_deref() == Some("23505") => {
                Self::Conflict("unique constraint".to_owned())
            }
            other => Self::Db(other),
        }
    }
}

impl From<object_store::Error> for StoreError {
    fn from(error: object_store::Error) -> Self {
        match error {
            object_store::Error::NotFound { .. } => Self::NotFound,
            other => Self::Blob(other),
        }
    }
}

/// Store result alias.
pub type Result<T> = std::result::Result<T, StoreError>;
