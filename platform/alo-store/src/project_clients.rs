//! The client facts of a project — who it is worked for, in what currency, at
//! what rate, against what budget (alo Projects, ADR 0035, wave B3), reached
//! through the account door like every other business record.
//!
//! alo Projects does not start with a new noun. The workspace already has
//! projects: a [`crate::tasks`] project is the board a team's tasks live on
//! (ADR 0021, ADR 0022). What this module adds is a **second lens on those
//! same rows** — a client project is a `task_projects` row with a
//! `project_clients` row beside it, and a project without one is exactly what
//! an internal project is (`docs/design/projects.md`, "One project list,
//! extended"). Three consequences, all deliberate:
//!
//! - `tasks.rs` is untouched by this wave. It knows nothing about money and
//!   should not learn (law 3), so the client facts live here end to end.
//! - The join is a `LEFT JOIN`: absence *is* the answer, with no sentinel
//!   value to misread.
//! - Client facts may only be attached to a **`team`** project. `kind`
//!   governs visibility — `personal` resolves only for its owner — and an
//!   engagement whose hours are approved by somebody else and billed to a
//!   customer is not private work. Attaching them to a personal board is a
//!   [`StoreError::Validation`] naming the rule.
//!
//! Client facts are **tenant-wide**, like `billing_customers`: everyone bills
//! the same customers and works the same engagements. The personal half of
//! this module's data — a person's hours — arrives at B3.03 and is reached
//! through a deliberately narrower door.
//!
//! Two fields are **snapshots, not references**. `currency` is copied from the
//! customer when the facts are written, and `rate_cents` is copied onto each
//! time entry as it is written (B3.03), for the reason a billing line
//! snapshots its price instead of joining to the price list: a change today
//! must never restate work that was already recorded.

use sqlx::{Postgres, Transaction};
use time::{Date, OffsetDateTime};

use crate::account::AccountStore;
use crate::billing_field::{currency as validate_currency, unit_price_cents};
use crate::error::{Result, StoreError};
use crate::id::{BillingCustomerId, ProjectId};

/// Largest budget we accept in hours, expressed in minutes: about nineteen
/// person-years. Beyond that it is a typo, not a plan.
pub const BUDGET_MINUTES_MAX: i64 = 10_000_000;

/// Largest budget we accept in money, in integer cents: a billion euro. Four
/// orders of magnitude below `i64::MAX` after the profitability report's
/// arithmetic (B3.08), so no figure it computes can wrap.
pub const BUDGET_CENTS_MAX: i64 = 100_000_000_000;

/// The `task_projects.kind` that may carry client facts.
const TEAM_KIND: &str = "team";

/// The columns every read of a project's client facts selects, in
/// `ClientRow` order.
const CLIENT_COLS: &str = "project_id, customer_id, currency, rate_cents, budget_minutes, \
     budget_cents, starts_on, created_at, updated_at";

/// The writable shape of a project's client facts. Used for the whole
/// idempotent set — there is no create/update pair, because an engagement's
/// client facts are one record that either applies or does not
/// (`PUT /projects/{id}/client`, B3.04).
#[derive(Debug, Clone)]
pub struct NewProjectClient {
    /// The customer the work is billed to. Required: a project with no
    /// customer is an internal project, expressed by having no client facts
    /// at all rather than by a blank field.
    pub customer_id: BillingCustomerId,
    /// ISO 4217 currency for this engagement, or `None` to snapshot the
    /// customer's own — which is what a caller who has not thought about it
    /// means, and what the UI sends.
    pub currency: Option<String>,
    /// Default hourly rate in integer cents, or `None` when nobody has priced
    /// the engagement yet.
    ///
    /// An unpriced engagement is legal and normal: the person logging the hour
    /// is frequently not the person who prices it, and refusing the facts
    /// would lose the engagement to protect the price. What is not legal is
    /// *billing* an unrated hour — the handoff (B3.06) demands a rate rather
    /// than guessing one.
    pub rate_cents: Option<i64>,
    /// Budget in hours, held as minutes. Advisory.
    pub budget_minutes: Option<i64>,
    /// Budget in money, in integer cents of [`Self::currency`]. Advisory.
    pub budget_cents: Option<i64>,
    /// The day the engagement starts, or `None` when nobody has said.
    pub starts_on: Option<Date>,
}

impl NewProjectClient {
    /// The minimum a caller must state: which customer the work is for.
    /// Everything else has an honest absent value.
    pub fn for_customer(customer_id: BillingCustomerId) -> Self {
        Self {
            customer_id,
            currency: None,
            rate_cents: None,
            budget_minutes: None,
            budget_cents: None,
            starts_on: None,
        }
    }
}

/// The stored client facts of one project.
#[derive(Debug, Clone)]
pub struct ProjectClient {
    /// The `task_projects` row these facts describe — also the key.
    pub project_id: ProjectId,
    /// The customer the work is billed to.
    pub customer_id: BillingCustomerId,
    /// ISO 4217 currency code, uppercase. Snapshotted from the customer when
    /// the facts were written; thereafter the engagement's own.
    pub currency: String,
    /// Default hourly rate in integer cents, or `None` when unpriced.
    pub rate_cents: Option<i64>,
    /// Budget in hours, held as minutes. Advisory — nothing refuses an hour
    /// logged past it.
    pub budget_minutes: Option<i64>,
    /// Budget in money, in integer cents. Advisory, as above.
    pub budget_cents: Option<i64>,
    /// The day the engagement starts, or `None`.
    pub starts_on: Option<Date>,
    /// When the project first became client work.
    pub created_at: OffsetDateTime,
    /// When its client facts were last replaced.
    pub updated_at: OffsetDateTime,
}

impl ProjectClient {
    /// Whether the engagement carries a default hourly rate. An engagement
    /// without one is countable but not billable: the handoff (B3.06) names
    /// unrated hours rather than pricing them at zero.
    pub fn is_priced(&self) -> bool {
        self.rate_cents.is_some()
    }
}

/// A validated, normalised set of client facts ready to be bound into a
/// statement.
///
/// Readable across the crate because [`crate::project_templates`] writes the
/// same row inside its own transaction (a copy lands whole or not at all), and
/// it must write facts validated by these rules rather than by a second set.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Normalized {
    pub(crate) currency: String,
    pub(crate) rate_cents: Option<i64>,
    pub(crate) budget_minutes: Option<i64>,
    pub(crate) budget_cents: Option<i64>,
    pub(crate) starts_on: Option<Date>,
}

/// Validates a budget in minutes: non-negative and bounded, or absent.
fn budget_minutes(value: Option<i64>) -> Result<Option<i64>> {
    match value {
        Some(minutes) if !(0..=BUDGET_MINUTES_MAX).contains(&minutes) => {
            Err(StoreError::Validation(format!(
                "budget hours must be between 0 and {BUDGET_MINUTES_MAX} minutes"
            )))
        }
        other => Ok(other),
    }
}

/// Validates a budget in money: non-negative and bounded, or absent.
fn budget_cents(value: Option<i64>) -> Result<Option<i64>> {
    match value {
        Some(cents) if !(0..=BUDGET_CENTS_MAX).contains(&cents) => Err(StoreError::Validation(
            format!("budget amount must be between 0 and {BUDGET_CENTS_MAX} cents"),
        )),
        other => Ok(other),
    }
}

/// Validates and normalises the client facts against the customer's own
/// currency, which is what an unstated currency means. Pure — no database, so
/// the rules are unit-tested directly.
///
/// The rate shares [`crate::billing_field::unit_price_cents`]'s ceiling rather
/// than growing a bound of its own: a rate becomes an invoice line's unit
/// price at the handoff (B3.06), and a rate and a price cannot be allowed to
/// disagree about what a legal amount is.
pub(crate) fn normalize(input: &NewProjectClient, customer_currency: &str) -> Result<Normalized> {
    Ok(Normalized {
        currency: match input.currency.as_deref() {
            Some(stated) => validate_currency(stated)?,
            None => validate_currency(customer_currency)?,
        },
        rate_cents: input
            .rate_cents
            .map(|cents| unit_price_cents("hourly rate", cents))
            .transpose()?,
        budget_minutes: budget_minutes(input.budget_minutes)?,
        budget_cents: budget_cents(input.budget_cents)?,
        starts_on: input.starts_on,
    })
}

impl AccountStore {
    /// Validates optional client facts inside a caller-owned transaction.
    pub(crate) async fn prepare_project_client_in(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        input: &NewProjectClient,
    ) -> Result<Normalized> {
        let customer = sqlx::query_as::<_, (String, Option<OffsetDateTime>)>(
            "SELECT currency, archived_at FROM billing_customers \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(input.customer_id.as_str())
        .fetch_optional(&mut **tx)
        .await
        .map_err(StoreError::Db)?
        .ok_or(StoreError::NotFound)?;
        if customer.1.is_some() {
            return Err(StoreError::Validation(
                "the customer is archived; restore it before billing work to it".to_owned(),
            ));
        }
        normalize(input, &customer.0)
    }

    /// Inserts a team project and optional prepared client facts in a
    /// caller-owned transaction.
    pub(crate) async fn insert_project_in(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        name: &str,
        color: Option<&str>,
        client: Option<(&NewProjectClient, Normalized)>,
    ) -> Result<ProjectId> {
        let id = ProjectId::generate();
        sqlx::query(
            "INSERT INTO task_projects (tenant_id, id, name, kind, owner_user_id, color) \
             VALUES ($1, $2, $3, 'team', $4, $5)",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(name)
        .bind(self.user.as_str())
        .bind(color)
        .execute(&mut **tx)
        .await
        .map_err(StoreError::Db)?;

        if let Some((input, facts)) = client {
            sqlx::query(
                "INSERT INTO project_clients (tenant_id, project_id, customer_id, currency, \
                     rate_cents, budget_minutes, budget_cents, starts_on) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
            )
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .bind(input.customer_id.as_str())
            .bind(facts.currency)
            .bind(facts.rate_cents)
            .bind(facts.budget_minutes)
            .bind(facts.budget_cents)
            .bind(facts.starts_on)
            .execute(&mut **tx)
            .await
            .map_err(StoreError::Db)?;
        }
        Ok(id)
    }

    /// Creates a team project and, when supplied, its client facts in one
    /// transaction. A client engagement is one user intent: a failed customer
    /// lookup or facts write must never leave an unrelated internal board
    /// behind.
    pub async fn create_project(
        &self,
        name: &str,
        color: Option<&str>,
        input: Option<&NewProjectClient>,
    ) -> Result<ProjectId> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let prepared = match input {
            Some(client) => Some((
                client,
                self.prepare_project_client_in(&mut tx, client).await?,
            )),
            None => None,
        };
        let id = self
            .insert_project_in(&mut tx, name, color, prepared)
            .await?;

        tx.commit().await.map_err(StoreError::Db)?;
        Ok(id)
    }

    /// Sets — or wholly replaces — a project's client facts, making it client
    /// work.
    ///
    /// Idempotent by design: a UI that saves one form makes one call, not a
    /// create-or-update pair, and calling twice with the same facts leaves the
    /// same row. `created_at` survives a replacement, so "when did this become
    /// client work" stays answerable.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the project or the customer is not this
    /// tenant's, or the project is a colleague's personal board — existence is
    /// never disclosed; [`StoreError::Validation`] when the project is the
    /// caller's own personal board, when either the project or the customer is
    /// archived, or when a currency, rate or budget breaks its rule;
    /// [`StoreError::Db`] on failure.
    pub async fn set_project_client(
        &self,
        project: &ProjectId,
        input: &NewProjectClient,
    ) -> Result<ProjectClient> {
        self.require_client_project(project).await?;
        let customer = self
            .billing_customer(&input.customer_id)
            .await?
            .ok_or(StoreError::NotFound)?;
        if customer.is_archived() {
            return Err(StoreError::Validation(
                "the customer is archived; restore it before billing work to it".to_owned(),
            ));
        }
        let facts = normalize(input, &customer.currency)?;
        let row = sqlx::query_as::<_, ClientRow>(&format!(
            "INSERT INTO project_clients (tenant_id, project_id, customer_id, currency, \
                 rate_cents, budget_minutes, budget_cents, starts_on) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
             ON CONFLICT (tenant_id, project_id) DO UPDATE SET \
                 customer_id = EXCLUDED.customer_id, currency = EXCLUDED.currency, \
                 rate_cents = EXCLUDED.rate_cents, \
                 budget_minutes = EXCLUDED.budget_minutes, \
                 budget_cents = EXCLUDED.budget_cents, \
                 starts_on = EXCLUDED.starts_on, updated_at = now() \
             RETURNING {CLIENT_COLS}"
        ))
        .bind(self.tenant.as_str())
        .bind(project.as_str())
        .bind(customer.id.as_str())
        .bind(&facts.currency)
        .bind(facts.rate_cents)
        .bind(facts.budget_minutes)
        .bind(facts.budget_cents)
        .bind(facts.starts_on)
        .fetch_one(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(row.into_client())
    }

    /// One project's client facts, or `None` when it is internal work —
    /// including when the id belongs to another tenant, which is
    /// indistinguishable by design.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn project_client(&self, project: &ProjectId) -> Result<Option<ProjectClient>> {
        let row = sqlx::query_as::<_, ClientRow>(&format!(
            "SELECT {CLIENT_COLS} FROM project_clients WHERE tenant_id = $1 AND project_id = $2"
        ))
        .bind(self.tenant.as_str())
        .bind(project.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(row.map(ClientRow::into_client))
    }

    /// Every engagement of the tenant, oldest first — the client half of the
    /// engagement list (B3.04), which the route layer zips onto
    /// [`AccountStore::task_projects`] so an internal project appears with no
    /// client facts rather than not at all.
    ///
    /// Only `team` projects can hold client facts, so this never exposes a
    /// colleague's personal board.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn project_clients(&self) -> Result<Vec<ProjectClient>> {
        let rows = sqlx::query_as::<_, ClientRow>(&format!(
            "SELECT {CLIENT_COLS} FROM project_clients WHERE tenant_id = $1 \
             ORDER BY created_at, project_id"
        ))
        .bind(self.tenant.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(ClientRow::into_client).collect())
    }

    /// The tenant's engagements for one customer — the read the unbilled view
    /// (B3.06) makes, and a customer's own drawer.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn project_clients_for_customer(
        &self,
        customer_id: &BillingCustomerId,
    ) -> Result<Vec<ProjectClient>> {
        let rows = sqlx::query_as::<_, ClientRow>(&format!(
            "SELECT {CLIENT_COLS} FROM project_clients \
             WHERE tenant_id = $1 AND customer_id = $2 ORDER BY created_at, project_id"
        ))
        .bind(self.tenant.as_str())
        .bind(customer_id.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(ClientRow::into_client).collect())
    }

    /// Makes a project internal work again.
    ///
    /// The hours stay. What is deleted is the *claim that they are billable to
    /// somebody* — the board, its tasks and everything logged against it are
    /// untouched, and hours already carried onto an invoice keep their link to
    /// the document that carries them.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the project is not this tenant's or was
    /// not client work; [`StoreError::Db`] on failure.
    pub async fn clear_project_client(&self, project: &ProjectId) -> Result<()> {
        let done =
            sqlx::query("DELETE FROM project_clients WHERE tenant_id = $1 AND project_id = $2")
                .bind(self.tenant.as_str())
                .bind(project.as_str())
                .execute(&self.pool)
                .await
                .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Confirms a project may carry client facts: it is **this tenant's**, it
    /// is a `team` board, and it is not archived.
    ///
    /// The two denials are deliberately different answers. A colleague's
    /// personal board is not visible to this caller at all, so it reads as
    /// absent — telling them "that is a personal project" would confirm a row
    /// they may not see. Their *own* personal board they can already see, so
    /// the honest answer there is the rule they broke.
    async fn require_client_project(&self, project: &ProjectId) -> Result<()> {
        let row = sqlx::query_as::<_, (String, String, bool)>(
            "SELECT kind, owner_user_id, archived FROM task_projects \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(project.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        let (kind, owner_user_id, archived) = row.ok_or(StoreError::NotFound)?;
        if kind != TEAM_KIND {
            if owner_user_id != self.user.as_str() {
                return Err(StoreError::NotFound);
            }
            return Err(StoreError::Validation(
                "client facts can only be attached to a team project".to_owned(),
            ));
        }
        if archived {
            return Err(StoreError::Validation(
                "the project is archived; restore it before making it client work".to_owned(),
            ));
        }
        Ok(())
    }
}

// ---- row types --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct ClientRow {
    project_id: String,
    customer_id: String,
    currency: String,
    rate_cents: Option<i64>,
    budget_minutes: Option<i64>,
    budget_cents: Option<i64>,
    starts_on: Option<Date>,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl ClientRow {
    fn into_client(self) -> ProjectClient {
        ProjectClient {
            project_id: ProjectId::new(self.project_id),
            customer_id: BillingCustomerId::new(self.customer_id),
            currency: self.currency,
            rate_cents: self.rate_cents,
            budget_minutes: self.budget_minutes,
            budget_cents: self.budget_cents,
            starts_on: self.starts_on,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Month;

    fn message<T: std::fmt::Debug>(result: Result<T>) -> String {
        match result {
            Err(StoreError::Validation(msg)) => msg,
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    fn facts() -> NewProjectClient {
        NewProjectClient::for_customer(BillingCustomerId::new("cust-1"))
    }

    #[test]
    fn an_unstated_currency_is_the_customers_own() {
        let normalized = normalize(&facts(), "eur").unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(normalized.currency, "EUR", "and it is uppercased");
    }

    #[test]
    fn a_stated_currency_wins_over_the_customers() {
        let stated = NewProjectClient {
            currency: Some(" chf ".to_owned()),
            ..facts()
        };
        let normalized = normalize(&stated, "EUR").unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(normalized.currency, "CHF");
    }

    #[test]
    fn a_currency_that_is_not_a_code_is_refused_from_either_source() {
        let stated = NewProjectClient {
            currency: Some("EURO".to_owned()),
            ..facts()
        };
        assert!(message(normalize(&stated, "EUR")).contains("ISO 4217"));
        // A customer whose currency somehow fails the rule is a bug, and a bug
        // that writes an unreadable engagement is worse than one that refuses.
        assert!(message(normalize(&facts(), "")).contains("ISO 4217"));
    }

    #[test]
    fn an_unpriced_engagement_is_legal() {
        let normalized = normalize(&facts(), "EUR").unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(normalized.rate_cents, None);
        assert_eq!(normalized.budget_minutes, None);
        assert_eq!(normalized.budget_cents, None);
        assert_eq!(normalized.starts_on, None);
    }

    #[test]
    fn a_rate_shares_the_billing_line_ceiling() {
        for ok in [0, 1, 9_500, crate::billing_field::UNIT_PRICE_MAX_CENTS] {
            let input = NewProjectClient {
                rate_cents: Some(ok),
                ..facts()
            };
            let normalized = normalize(&input, "EUR").unwrap_or_else(|e| panic!("{e:?}"));
            assert_eq!(normalized.rate_cents, Some(ok));
        }
        for bad in [
            -1,
            crate::billing_field::UNIT_PRICE_MAX_CENTS + 1,
            i64::MIN,
            i64::MAX,
        ] {
            let input = NewProjectClient {
                rate_cents: Some(bad),
                ..facts()
            };
            assert!(
                message(normalize(&input, "EUR")).contains("hourly rate"),
                "expected rejection naming the field: {bad}"
            );
        }
    }

    #[test]
    fn budgets_are_non_negative_and_bounded_at_both_ends() {
        for ok in [0, 60, BUDGET_MINUTES_MAX] {
            assert_eq!(budget_minutes(Some(ok)).unwrap_or(None), Some(ok));
        }
        for bad in [-1, BUDGET_MINUTES_MAX + 1, i64::MIN, i64::MAX] {
            assert!(message(budget_minutes(Some(bad))).contains("budget hours"));
        }
        for ok in [0, 250_000, BUDGET_CENTS_MAX] {
            assert_eq!(budget_cents(Some(ok)).unwrap_or(None), Some(ok));
        }
        for bad in [-1, BUDGET_CENTS_MAX + 1, i64::MIN, i64::MAX] {
            assert!(message(budget_cents(Some(bad))).contains("budget amount"));
        }
        // Absent is not zero, and is always allowed: a budget nobody has set
        // is not a budget of nothing.
        assert_eq!(budget_minutes(None).unwrap_or(Some(0)), None);
        assert_eq!(budget_cents(None).unwrap_or(Some(0)), None);
    }

    #[test]
    fn both_budgets_may_be_carried_at_once() {
        let input = NewProjectClient {
            budget_minutes: Some(48_000),
            budget_cents: Some(9_500_000),
            starts_on: Date::from_calendar_date(2026, Month::September, 1).ok(),
            ..facts()
        };
        let normalized = normalize(&input, "EUR").unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(normalized.budget_minutes, Some(48_000));
        assert_eq!(normalized.budget_cents, Some(9_500_000));
        assert!(normalized.starts_on.is_some(), "a start date is carried");
    }

    #[test]
    fn a_priced_engagement_says_so() {
        let unpriced = ProjectClient {
            project_id: ProjectId::new("p"),
            customer_id: BillingCustomerId::new("c"),
            currency: "EUR".to_owned(),
            rate_cents: None,
            budget_minutes: None,
            budget_cents: None,
            starts_on: None,
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
        };
        assert!(!unpriced.is_priced());
        assert!(
            ProjectClient {
                rate_cents: Some(9_500),
                ..unpriced
            }
            .is_priced()
        );
    }
}
