//! Tenant-wide scoped roles — alo's first one is the accountant (ADR 0035,
//! wave B4.12; `docs/design/finance.md`, "The accountant role").
//!
//! Until this module a tenant had two access facts: `users.is_admin`, and
//! Spaces membership with a `SpaceRole` (ADR 0026/0028, which governs Drive and
//! what attaches to a Space). Neither answers the question an external
//! accountant asks — *give me the books and none of the mail* — because a
//! tenant admin has everything and a Space is a container the ledger does not
//! live in.
//!
//! So a role is a row: `(tenant, user, role)`, with who granted it and when.
//!
//! # What a role means is decided by its gates, not here
//!
//! This module stores and reads role rows and nothing else. Which surfaces a
//! role opens is a property of the surfaces — `alo-jmap`'s `require_finance`
//! accepts an admin *or* an accountant, and the billing/CRM write gate refuses
//! an accountant — and putting that map in the store would put the API's
//! shape in a layer that cannot see the API.
//!
//! # Membership is proved before a grant is written
//!
//! `users.id` is globally unique, so an `INSERT` that took a user id on trust
//! would happily make another tenant's user an accountant of this one. Every
//! write here proves the user belongs to the granting tenant first, and answers
//! [`StoreError::NotFound`] when they do not — the same answer an id that was
//! never issued gets, so the refusal is never an oracle either.
//!
//! *Rejected: a general RBAC engine* (roles × permissions × resources). One
//! role ships today; a permission matrix built for a single caller encodes that
//! caller's accidents. The second role — B6's HR role is the likely one — adds
//! a value to [`TenantRole`] and the CHECK in migration 0149, and widens the
//! gates by a word.

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::UserId;
use crate::store::TenantStore;
use crate::user_modules::AppModule;

/// A tenant-wide role a user may hold in addition to being (or not being) a
/// tenant admin.
///
/// Deliberately a closed enum rather than a string: a role that no gate knows
/// is an access fact that silently does nothing, and the database's own CHECK
/// says the same thing one layer down.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TenantRole {
    /// The books, and only the books: every `/finance/*` read, the finance
    /// writes an accountant does (manual entries, matches, expense decisions,
    /// the period lock), billing and CRM read-only, and nothing else — no mail
    /// of anyone else's, no files, no admin console.
    Accountant,
    /// The workforce: the whole employee directory including the private
    /// fields, employments and pay, HR documents, leave policies and every
    /// leave decision, hiring, and the payroll export (alo HR, ADR 0035, wave
    /// B6; `docs/design/hr.md`, "The HR role").
    ///
    /// This role only ever **adds**. Holding it does not narrow anything an
    /// ordinary member could already do, and it is not implied by
    /// [`Self::Accountant`] — an external bookkeeper reading everybody's
    /// contract and home address is exactly the failure that role exists to
    /// prevent. Somebody who genuinely runs both is granted both, deliberately,
    /// with each grant's provenance recorded.
    Hr,
    /// A deliberately restricted alo Sites collaborator. A non-admin holder
    /// can use only the Sites API, and only for resources named by their
    /// per-site grants. Unlike additive business roles, this role narrows the
    /// account so sharing a website never shares the surrounding workspace.
    SiteEditor,
}

impl TenantRole {
    /// The stored word.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accountant => "accountant",
            Self::Hr => "hr",
            Self::SiteEditor => "site_editor",
        }
    }

    /// Reads a role name — from a request body or from a stored row.
    ///
    /// # Errors
    /// [`StoreError::Validation`] naming the accepted set. A word this build
    /// does not know is refused rather than ignored: a granted role nothing
    /// enforces would read as access that was given when it was not.
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim() {
            "accountant" => Ok(Self::Accountant),
            "hr" => Ok(Self::Hr),
            "site_editor" => Ok(Self::SiteEditor),
            _ => Err(StoreError::Validation(
                "role must be one of: accountant, hr, site_editor".to_owned(),
            )),
        }
    }
}

impl std::fmt::Display for TenantRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The access facts a request needs about its caller, read together.
///
/// One query rather than three: `authenticate` runs on every single request in
/// the product, and a round trip per request to learn a fact almost nobody has
/// would be paid by the mail hot path forever.
#[derive(Debug, Clone, Default)]
pub struct AccessFacts {
    /// Whether the user is a tenant admin.
    pub is_admin: bool,
    /// The tenant-wide roles they hold, sorted and without duplicates.
    pub roles: Vec<TenantRole>,
    /// The rail modules an admin has switched off for this person, sorted
    /// (migration 0208). Ordinarily empty — the common case is somebody who
    /// has been denied nothing.
    pub denied_modules: Vec<AppModule>,
}

impl AccessFacts {
    /// Whether this caller holds `role`.
    #[must_use]
    pub fn has(&self, role: TenantRole) -> bool {
        self.roles.contains(&role)
    }

    /// Whether this caller may open `module`.
    ///
    /// Answers only the admin's per-user switch. A `true` here is not
    /// permission to use the module — Finance still wants an admin or an
    /// accountant, a Space still wants membership — it only says this
    /// particular door was not shut.
    ///
    /// A tenant admin is never denied. An administrator who switched an app
    /// off for themselves and then could not reach the console to switch it
    /// back on would be a support call, and the console is where the switch
    /// lives.
    #[must_use]
    pub fn may_open(&self, module: AppModule) -> bool {
        self.is_admin || !self.denied_modules.contains(&module)
    }
}

/// Turns stored role words into roles, dropping any this build does not know.
///
/// A row can only be a word the CHECK allowed, so an unknown one means the
/// database is ahead of this binary (a rolling deploy). Dropping it fails
/// **closed** — the caller is treated as not holding a role nothing here can
/// enforce — which is the safe direction for an access fact.
fn roles_from_words(words: Vec<String>) -> Vec<TenantRole> {
    let mut roles: Vec<TenantRole> = words
        .iter()
        .filter_map(|word| TenantRole::parse(word).ok())
        .collect();
    roles.sort_unstable();
    roles.dedup();
    roles
}

impl AccountStore {
    /// The signed-in user's access facts: the admin flag and their roles, in
    /// one read. Used by the API's `authenticate`.
    ///
    /// A user row that is not this tenant's (or is gone) answers with no admin
    /// and no roles, exactly as [`AccountStore::is_admin`] answers `false`.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure — never a partial answer, because a
    /// swallowed error here would silently downgrade a caller's access.
    pub async fn access_facts(&self) -> Result<AccessFacts> {
        // Scalar subqueries rather than a second LEFT JOIN: joining two
        // one-to-many tables multiplies the rows, and each `array_agg` would
        // then count the other table's rows — three roles would report every
        // denial three times. Correct with DISTINCT, but only accidentally.
        let row: Option<(bool, Vec<String>, Vec<String>)> = sqlx::query_as(
            "SELECT u.is_admin, \
                    coalesce((SELECT array_agg(r.role) FROM tenant_user_roles r \
                               WHERE r.tenant_id = u.tenant_id AND r.user_id = u.id), \
                             '{}') AS roles, \
                    coalesce((SELECT array_agg(d.module) \
                                FROM tenant_user_module_denials d \
                               WHERE d.tenant_id = u.tenant_id AND d.user_id = u.id), \
                             '{}') AS denied \
               FROM users u \
              WHERE u.tenant_id = $1 AND u.id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .fetch_optional(&self.pool)
        .await?;
        Ok(match row {
            Some((is_admin, roles, denied)) => AccessFacts {
                is_admin,
                roles: roles_from_words(roles),
                denied_modules: crate::user_modules::modules_from_words(&denied),
            },
            None => AccessFacts::default(),
        })
    }
}

impl TenantStore {
    /// Grants `role` to a user of this tenant. Idempotent: granting a role the
    /// user already holds keeps the original grant (and its provenance) rather
    /// than restamping it, so "since when has this person had the books?" stays
    /// answerable.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the user is not a member of this tenant —
    /// including when they exist in another one, which is the same answer an
    /// id that was never issued gets. [`StoreError::Db`] on failure.
    pub async fn grant_role(
        &self,
        user: &UserId,
        role: TenantRole,
        granted_by: &UserId,
    ) -> Result<()> {
        self.assert_user(user).await?;
        sqlx::query(
            "INSERT INTO tenant_user_roles (tenant_id, user_id, role, granted_by) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT (tenant_id, user_id, role) DO NOTHING",
        )
        .bind(self.tenant().as_str())
        .bind(user.as_str())
        .bind(role.as_str())
        .bind(granted_by.as_str())
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Revokes `role` from a user of this tenant. Idempotent: revoking a role
    /// they do not hold is a no-op, because the caller's intent — *this person
    /// must not have it* — is satisfied either way.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the user is not a member of this tenant;
    /// [`StoreError::Db`] on failure.
    pub async fn revoke_role(&self, user: &UserId, role: TenantRole) -> Result<()> {
        self.assert_user(user).await?;
        sqlx::query(
            "DELETE FROM tenant_user_roles \
              WHERE tenant_id = $1 AND user_id = $2 AND role = $3",
        )
        .bind(self.tenant().as_str())
        .bind(user.as_str())
        .bind(role.as_str())
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// The roles one user of this tenant holds, sorted. A user of another
    /// tenant holds none here, whatever they hold there.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn user_roles(&self, user: &UserId) -> Result<Vec<TenantRole>> {
        let words: Vec<String> = sqlx::query_scalar(
            "SELECT role FROM tenant_user_roles \
              WHERE tenant_id = $1 AND user_id = $2 ORDER BY role",
        )
        .bind(self.tenant().as_str())
        .bind(user.as_str())
        .fetch_all(self.pool())
        .await?;
        Ok(roles_from_words(words))
    }

    /// Every role grant in this tenant as `(user, role)` pairs, ordered by
    /// user — one read for a whole user list, so the admin console does not ask
    /// per row.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn role_grants(&self) -> Result<Vec<(UserId, TenantRole)>> {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT user_id, role FROM tenant_user_roles \
              WHERE tenant_id = $1 ORDER BY user_id, role",
        )
        .bind(self.tenant().as_str())
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .filter_map(|(user, word)| {
                TenantRole::parse(&word)
                    .ok()
                    .map(|role| (UserId::new(user), role))
            })
            .collect())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::{AccessFacts, TenantRole, roles_from_words};

    #[test]
    fn a_role_is_the_word_it_is_stored_as() {
        assert_eq!(TenantRole::Accountant.as_str(), "accountant");
        assert_eq!(TenantRole::Hr.as_str(), "hr");
        assert_eq!(
            TenantRole::parse("accountant").unwrap(),
            TenantRole::Accountant
        );
        assert_eq!(
            TenantRole::parse("  accountant ").unwrap(),
            TenantRole::Accountant
        );
        assert_eq!(TenantRole::parse(" hr\n").unwrap(), TenantRole::Hr);
    }

    #[test]
    fn a_word_no_gate_knows_is_refused_and_named() {
        let err = TenantRole::parse("admin").unwrap_err();
        let message = format!("{err}");
        assert!(
            message.contains("accountant") && message.contains("hr"),
            "the refusal names the whole accepted set: {message}"
        );
        assert!(TenantRole::parse("").is_err());
        assert!(TenantRole::parse("Accountant").is_err(), "stored lowercase");
        assert!(TenantRole::parse("HR").is_err(), "stored lowercase");
        assert!(
            TenantRole::parse("payroll").is_err(),
            "a role no gate knows is refused, not stored"
        );
    }

    #[test]
    fn unknown_stored_words_fail_closed_rather_than_widen_access() {
        let roles = roles_from_words(vec![
            "accountant".to_owned(),
            "payroll".to_owned(),
            "accountant".to_owned(),
        ]);
        assert_eq!(
            roles,
            vec![TenantRole::Accountant],
            "deduped, `payroll` dropped"
        );
        let facts = AccessFacts {
            is_admin: false,
            roles,
            ..Default::default()
        };
        assert!(facts.has(TenantRole::Accountant));
        assert!(!facts.is_admin, "a role is never an admin flag");
    }

    #[test]
    fn the_two_roles_are_independent_and_never_imply_each_other() {
        // The books and the workforce are separate grants: the accountant's
        // role exists to keep an external bookkeeper out of everything that is
        // not the ledger, and HR's exists to let somebody see the workforce.
        let books = AccessFacts {
            is_admin: false,
            roles: roles_from_words(vec!["accountant".to_owned()]),
            ..Default::default()
        };
        assert!(books.has(TenantRole::Accountant));
        assert!(!books.has(TenantRole::Hr), "the books are not the people");
        let people = AccessFacts {
            is_admin: false,
            roles: roles_from_words(vec!["hr".to_owned()]),
            ..Default::default()
        };
        assert!(people.has(TenantRole::Hr));
        assert!(!people.has(TenantRole::Accountant));
        // Somebody may hold both, and then holds the union — never a product.
        let both = roles_from_words(vec!["hr".to_owned(), "accountant".to_owned()]);
        assert_eq!(both, vec![TenantRole::Accountant, TenantRole::Hr], "sorted");
    }

    #[test]
    fn holding_no_role_is_the_default() {
        let facts = AccessFacts::default();
        assert!(!facts.is_admin);
        assert!(!facts.has(TenantRole::Accountant));
        assert!(!facts.has(TenantRole::Hr));
    }
}
