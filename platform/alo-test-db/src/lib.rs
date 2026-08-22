//! Where an integration suite gets its database, and the one place that says
//! which databases a suite may have.
//!
//! # Why this is a crate and not four lines in each harness
//!
//! It used to be four lines in each harness — thirty-six copies of
//!
//! ```ignore
//! fn database_url() -> String {
//!     std::env::var("DATABASE_URL")
//!         .unwrap_or_else(|_| "postgres://alo:alo-dev-only@127.0.0.1:5433/alo".to_owned())
//! }
//! ```
//!
//! and the copies had drifted: three ports, two passwords, and every one of
//! them naming `alo`, the database the product itself runs on. A developer
//! with no `DATABASE_URL` set ran the suites straight into it, and the suites
//! do what suites do — create tenants, thousands of them. The symptom shows up
//! much later and looks nothing like the cause: mail with no folders in it,
//! blamed on the mail code, when the real fault was a connection string.
//!
//! The constitution's rule is that a machine has exactly one alo database,
//! named `alo`, and that suites do not write into it. A rule enforced in
//! thirty-six places is enforced in none of them, because the thirty-seventh
//! suite is written by copying a neighbour. So it is enforced here.

/// The connection string for a suite, refusing the database the product runs on.
///
/// Reads `DATABASE_URL`, falling back to a local scratch database. Panics if
/// the URL names `alo`, whether it came from the environment or not.
///
/// # Panics
///
/// If the database named is `alo`. That is the point: see [`deny_shared`].
///
/// ```ignore
/// let pool = PgPool::connect(&alo_test_db::url()).await?;
/// ```
#[must_use]
pub fn url() -> String {
    let url = std::env::var("DATABASE_URL").unwrap_or_else(|_| FALLBACK.to_owned());
    deny_shared(&url);
    url
}

/// Where suites connect when nothing says otherwise.
///
/// It names a scratch database on purpose. An unset `DATABASE_URL` should mean
/// "no database yet" — a connection error the developer reads and fixes — and
/// never "quietly use the real one".
const FALLBACK: &str = "postgres://alo:alo-dev-only@127.0.0.1:5432/alo_scratch";

/// Stops a suite pointed at the database the product runs on.
///
/// A panic rather than a silent redirect, on purpose: a suite quietly moved to
/// some other database is a suite whose data nobody can find afterwards. The
/// developer should decide which database this is, not discover months later
/// that something did.
///
/// # Panics
///
/// If `url`'s database name is `alo`.
pub fn deny_shared(url: &str) {
    assert!(
        database_name(url) != "alo",
        "tests must not run against the shared `alo` database (CLAUDE.md, \
         one-database rule). Point DATABASE_URL at a scratch database — one \
         the suite may create, fill and drop, such as `alo_scratch`."
    );
}

/// The database name in a postgres URL: the last path segment, without any
/// query string.
///
/// Deliberately not a URL parser. It answers one question — "is this `alo`?" —
/// and a wrong answer errs toward refusing, which costs a developer one
/// environment variable, while the opposite error costs somebody their mail.
fn database_name(url: &str) -> &str {
    let tail = url.rsplit('/').next().unwrap_or_default();
    tail.split('?').next().unwrap_or(tail)
}

#[cfg(test)]
mod tests {
    use super::{database_name, deny_shared};

    #[test]
    fn the_name_is_the_last_segment_without_its_query() {
        assert_eq!(
            database_name("postgres://alo:pw@127.0.0.1:5432/alo_scratch"),
            "alo_scratch"
        );
        assert_eq!(
            database_name("postgres://alo:pw@127.0.0.1:5432/alo?sslmode=require"),
            "alo"
        );
        // A trailing slash names no database at all, which is not `alo`, and
        // postgres will reject it on connect with a clearer message than we
        // could give here.
        assert_eq!(database_name("postgres://alo:pw@127.0.0.1:5432/"), "");
    }

    #[test]
    fn a_scratch_database_is_allowed() {
        deny_shared("postgres://alo:pw@127.0.0.1:5432/alo_scratch");
        deny_shared("postgres://alo:pw@127.0.0.1:5432/alo_audit");
    }

    #[test]
    #[should_panic(expected = "one-database rule")]
    fn the_shared_database_is_refused() {
        deny_shared("postgres://alo:pw@127.0.0.1:5432/alo");
    }

    #[test]
    #[should_panic(expected = "one-database rule")]
    fn the_shared_database_is_refused_behind_query_parameters() {
        deny_shared("postgres://alo:pw@127.0.0.1:5432/alo?sslmode=require");
    }

    #[test]
    fn the_fallback_is_not_the_shared_database() {
        // The failure this whole crate exists to prevent: a fallback that,
        // through an innocent-looking edit, comes to name `alo` again.
        deny_shared(super::FALLBACK);
    }
}
