//! Which apps a person may open — the admin console's per-user app switches
//! (migration 0208).
//!
//! A tenant admin decides, per user, which of the rail's modules that person
//! gets. Everybody starts with all of them; a switch turned off writes a
//! denial row, and turning it back on deletes that row.
//!
//! # Denials, not grants
//!
//! Stored the other way round from how it is shown. The console renders
//! checkboxes that read "has access", because that is the sentence an
//! administrator thinks in; the table holds the complement, because the empty
//! table then means what is actually true today — everybody can open
//! everything — and a module added next month needs no backfill for every
//! existing account. Migration 0208 argues this at length, including what it
//! costs.
//!
//! # This narrows and never widens
//!
//! Not being denied a module is not permission to use it. Finance still wants
//! an admin or an accountant, People still wants the HR role, a Space still
//! wants membership. Every existing gate keeps its own answer and this one is
//! asked as well — so the effect of a row is only ever to take something away.
//!
//! # Why not another role
//!
//! [`crate::tenant_roles`] answers "what may this person do" for cross-cutting
//! jobs, as a closed set of roles the gates name in words. This answers a
//! flatter question — which apps was this person given — and in a tenant of
//! fifty people there are fifty answers, none of which is a job title. Roles
//! would need one minted per combination.

use crate::error::{Result, StoreError};
use crate::id::UserId;
use crate::store::TenantStore;

/// A rail module whose access can be switched off for one person.
///
/// Closed, and deliberately the same set as the CHECK in migration 0208: a
/// denial naming a module no gate reads would show as switched off in the
/// console while every route still answered it, which is worse than a refusal
/// because it looks like it worked.
///
/// Mail and Home are absent and cannot be denied. `/jmap` carries the session,
/// blob upload and the event stream every other surface depends on, so a
/// denial there would not read as "no mail app" — it would be a broken
/// account. Withholding mail needs its own answer, not a value here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AppModule {
    Agenda,
    Billing,
    Chat,
    Crm,
    Drive,
    Finance,
    Hr,
    Insights,
    Inventory,
    Meet,
    Projects,
    Sites,
    Tasks,
}

/// Every module an admin can switch, in the order the console shows them.
pub const ALL_MODULES: [AppModule; 13] = [
    AppModule::Agenda,
    AppModule::Billing,
    AppModule::Chat,
    AppModule::Crm,
    AppModule::Drive,
    AppModule::Finance,
    AppModule::Hr,
    AppModule::Insights,
    AppModule::Inventory,
    AppModule::Meet,
    AppModule::Projects,
    AppModule::Sites,
    AppModule::Tasks,
];

impl AppModule {
    /// The stored word, which is also the rail's id for the module.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Agenda => "agenda",
            Self::Billing => "billing",
            Self::Chat => "chat",
            Self::Crm => "crm",
            Self::Drive => "drive",
            Self::Finance => "finance",
            Self::Hr => "hr",
            Self::Insights => "insights",
            Self::Inventory => "inventory",
            Self::Meet => "meet",
            Self::Projects => "projects",
            Self::Sites => "sites",
            Self::Tasks => "tasks",
        }
    }

    /// Reads a module id — from a request body or from a stored row.
    ///
    /// # Errors
    /// [`StoreError::Validation`] naming the accepted set. An unknown word is
    /// refused rather than ignored, so an admin never gets a confirmation for
    /// a switch that was thrown away.
    pub fn parse(value: &str) -> Result<Self> {
        ALL_MODULES
            .into_iter()
            .find(|module| module.as_str() == value.trim())
            .ok_or_else(|| {
                StoreError::Validation(format!(
                    "module must be one of: {}",
                    ALL_MODULES
                        .iter()
                        .map(|module| module.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            })
    }
}

impl std::fmt::Display for AppModule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Turns stored module words into modules, dropping any this build does not
/// know.
///
/// A row can only be a word the CHECK allowed, so an unknown one means the
/// database is ahead of this binary (a rolling deploy). Dropping it fails
/// **open** for that one module — the person keeps an app an admin meant to
/// take away, until the new binary lands.
///
/// That is the opposite direction from [`crate::tenant_roles`], which drops
/// unknown roles and so fails closed, and the difference is deliberate: there,
/// an unknown word would grant access nothing can enforce; here, it would
/// withhold an app while the surfaces that own it still answered. Neither is
/// good, and each errs toward the state the rest of the system agrees with.
pub(crate) fn modules_from_words(words: &[String]) -> Vec<AppModule> {
    let mut modules: Vec<AppModule> = words
        .iter()
        .filter_map(|word| AppModule::parse(word).ok())
        .collect();
    modules.sort_unstable();
    modules.dedup();
    modules
}

impl TenantStore {
    /// Switches one app on or off for one user of this tenant.
    ///
    /// `allowed` is the checkbox as the admin sees it: `true` deletes any
    /// denial, `false` writes one. Idempotent in both directions — switching
    /// off an app already off keeps the original denial and its provenance, so
    /// "since when has this person not had Billing?" stays answerable.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the user is not a member of this tenant,
    /// including when they exist in another one — the same answer an id that
    /// was never issued gets, so the refusal is never an oracle either.
    /// [`StoreError::Db`] on failure.
    pub async fn set_module_access(
        &self,
        user: &UserId,
        module: AppModule,
        allowed: bool,
        decided_by: &UserId,
    ) -> Result<()> {
        self.assert_user(user).await?;
        if allowed {
            sqlx::query(
                "DELETE FROM tenant_user_module_denials \
                  WHERE tenant_id = $1 AND user_id = $2 AND module = $3",
            )
            .bind(self.tenant().as_str())
            .bind(user.as_str())
            .bind(module.as_str())
            .execute(self.pool())
            .await?;
        } else {
            sqlx::query(
                "INSERT INTO tenant_user_module_denials \
                     (tenant_id, user_id, module, denied_by) \
                 VALUES ($1, $2, $3, $4) \
                 ON CONFLICT (tenant_id, user_id, module) DO NOTHING",
            )
            .bind(self.tenant().as_str())
            .bind(user.as_str())
            .bind(module.as_str())
            .bind(decided_by.as_str())
            .execute(self.pool())
            .await?;
        }
        Ok(())
    }

    /// The modules this user of the tenant may not open, sorted.
    ///
    /// For the admin console, which shows one person's switches. The session
    /// path does not use this — it reads the same fact alongside the admin
    /// flag and the roles in a single query, because that one runs on every
    /// request in the product.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the user is not a member of this tenant.
    /// [`StoreError::Db`] on failure.
    pub async fn denied_modules(&self, user: &UserId) -> Result<Vec<AppModule>> {
        self.assert_user(user).await?;
        let words: Vec<String> = sqlx::query_scalar(
            "SELECT module FROM tenant_user_module_denials \
              WHERE tenant_id = $1 AND user_id = $2",
        )
        .bind(self.tenant().as_str())
        .bind(user.as_str())
        .fetch_all(self.pool())
        .await?;
        Ok(modules_from_words(&words))
    }
}
