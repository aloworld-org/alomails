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
    /// The caller can see the resource but lacks the role to perform the
    /// action (e.g. a Space viewer trying to write, or a non-manager trying
    /// to change membership — ADR 0026). Distinct from [`Self::NotFound`],
    /// which hides existence from non-members; a `Forbidden` is only ever
    /// returned to someone who already knows the resource exists.
    #[error("forbidden")]
    Forbidden,
    /// A uniqueness/precondition conflict (e.g. duplicate mailbox name).
    #[error("conflict: {0}")]
    Conflict(String),
    /// The input is malformed — a field the caller can fix before retrying
    /// (a malformed VAT id, a negative amount, an unknown currency code).
    /// Distinct from [`Self::Conflict`], which is a well-formed request that
    /// disagrees with the current state: routes map this to `422`, a conflict
    /// to `409`. The message names the violated rule and never echoes stored
    /// data from another tenant.
    #[error("invalid input: {0}")]
    Validation(String),
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

impl StoreError {
    /// A log-safe line describing an internal failure, for the moment a
    /// service turns one into a 500 or a `serverFail`.
    ///
    /// Every variant's `Display` is safe by construction — the detail lives in
    /// `#[source]` precisely so it stays out of the message. This summarises
    /// that source rather than printing it: a database's own error text can
    /// quote a value straight out of a row (`Key (email)=(…)`), and a log is
    /// not a place a customer's address may appear. What survives is the
    /// SQLSTATE, which names the fault without naming the data.
    ///
    /// The blob source is kept whole. It carries a tenant id and a content
    /// hash and no message content, and it is the one thing that distinguishes
    /// "this body is missing" from "the database is down" — the difference
    /// between a five-minute diagnosis and an afternoon.
    #[must_use]
    pub fn log_cause(&self) -> String {
        match self {
            Self::Db(sqlx::Error::Database(db)) => format!(
                "store database error (sqlstate {})",
                db.code().as_deref().unwrap_or("none")
            ),
            Self::Db(other) => format!("store database error ({})", sqlx_kind(other)),
            Self::Blob(source) => format!("blob store error: {source}"),
            other => other.to_string(),
        }
    }
}

/// The name of a non-database `sqlx::Error`, without its text.
///
/// A kind is enough to tell a closed pool from a broken connection, and unlike
/// the error's own `Display` it cannot grow a value in some future release.
fn sqlx_kind(error: &sqlx::Error) -> &'static str {
    match error {
        sqlx::Error::Configuration(_) => "configuration",
        sqlx::Error::Io(_) => "io",
        sqlx::Error::Tls(_) => "tls",
        sqlx::Error::Protocol(_) => "protocol",
        sqlx::Error::TypeNotFound { .. } => "type not found",
        sqlx::Error::ColumnIndexOutOfBounds { .. } => "column index out of bounds",
        sqlx::Error::ColumnNotFound(_) => "column not found",
        sqlx::Error::ColumnDecode { .. } => "column decode",
        sqlx::Error::Decode(_) => "decode",
        sqlx::Error::PoolTimedOut => "pool timed out",
        sqlx::Error::PoolClosed => "pool closed",
        sqlx::Error::WorkerCrashed => "worker crashed",
        // `sqlx::Error` is `#[non_exhaustive]`. A variant added upstream is
        // reported as unknown rather than printed — the safe default for text
        // nobody here has read.
        _ => "other",
    }
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

#[cfg(test)]
mod tests {
    use super::StoreError;

    #[test]
    fn a_database_fault_is_named_without_its_text() {
        // The text is the risk. A database's own error quotes the value that
        // broke the constraint, which for us is somebody's address; the log
        // needs to know a query failed, not who it failed on.
        let leaky = "Key (email)=(alice@example.test) already exists";
        let cause = StoreError::Db(sqlx::Error::Protocol(leaky.to_owned())).log_cause();
        assert!(
            !cause.contains("alice@example.test"),
            "no value out of a row reaches the log: {cause}",
        );
        assert!(cause.contains("protocol"), "but the kind does: {cause}");
    }

    #[test]
    fn a_missing_body_says_it_is_the_blob_store() {
        // The failure that started this: a message row whose bytes were gone
        // read as a bare 500 with nothing logged, indistinguishable from the
        // database being down.
        let cause = StoreError::Blob(object_store::Error::NotImplemented).log_cause();
        assert!(cause.starts_with("blob store error:"), "{cause}");
    }

    #[test]
    fn the_refusals_read_as_themselves() {
        // These four never reach a log line — they are the caller's to fix and
        // are answered on the wire — but the cause should still be truthful if
        // one ever does.
        assert_eq!(StoreError::NotFound.log_cause(), "not found");
        assert_eq!(StoreError::Forbidden.log_cause(), "forbidden");
        assert_eq!(StoreError::OverQuota.log_cause(), "storage quota exceeded");
    }
}
