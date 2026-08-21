//! What a project has cost in hours so far — the **project-grain** aggregate
//! over `time_entries` (alo Projects, ADR 0035, wave B3.07), and the one read
//! in this module that deliberately spans every person's entries.
//!
//! `docs/design/projects.md` § "The hours of a person are personal data" draws
//! the line this file sits exactly on:
//!
//! > Per-user hours are visible to their owner and to a tenant admin, and to
//! > nobody else. **Project aggregates are visible to anyone who can see the
//! > project** — hours to date, budget consumption, the value of the work —
//! > **without a per-person breakdown.**
//!
//! So this is not a hole in the account door's "every statement carries
//! `user_id = self.user`" rule; it is the one shape the rule was written to
//! allow. Two structural guarantees keep it honest, and both are in the SQL
//! rather than in a caller's memory:
//!
//! - **There is no `user_id` in the output type at all.** A breakdown cannot be
//!   asked for through this function because the function cannot express one —
//!   the admin column the design note mentions belongs to B3.08's report, on
//!   the tenant door, behind `require_admin`.
//! - **The visibility predicate is the same one [`AccountStore::task_projects`]
//!   uses**: a `team` board, or a `personal` board the caller owns. A
//!   colleague's private board contributes nothing, so "how long did that take"
//!   can never become "what has my colleague been doing".
//!
//! Proposals are excluded. An entry the agent drafted is not an hour until a
//! human accepts it (ADR 0023, and `docs/design/projects.md` § "Proposed
//! entries are not hours"), and a budget bar that filled up with suggestions
//! would be reporting on work nobody has done.
//!
//! Minutes only. Every figure here is an integer count of minutes or a day;
//! the money side of an engagement — hours × rates against `budget_cents` — is
//! the profitability report's (B3.08), which folds through
//! [`crate::time_hours`] so that it and an invoice line agree.

use time::Date;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::ProjectId;

/// The hours logged against one project, by everybody, with no per-person
/// breakdown — see the module note for why that absence is the design.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectHours {
    /// The board these hours were logged against.
    pub project_id: ProjectId,
    /// Every accepted minute on the project, billable or not.
    pub minutes: i64,
    /// The subset somebody marked chargeable. Never larger than
    /// [`Self::minutes`].
    pub billable_minutes: i64,
    /// Billable minutes in an approved week that have not yet been carried
    /// onto an invoice. This is the only total that may be presented as ready
    /// to invoice.
    pub approved_unbilled_minutes: i64,
    /// Billable minutes in a submitted week that are waiting for approval and
    /// have not yet been carried onto an invoice.
    pub submitted_unbilled_minutes: i64,
    /// The subset already carried onto a billing document. Never larger than
    /// [`Self::billable_minutes`] — only a billable hour can be billed.
    pub billed_minutes: i64,
    /// The most recent day anybody worked on it, or `None` when nobody has.
    /// A *work date*, not a timestamp: it is the day the person said they
    /// worked, which is the only date a timesheet is read by.
    pub last_worked_on: Option<Date>,
}

impl ProjectHours {
    /// An engagement nobody has logged an hour against yet. The honest zero for
    /// a project the aggregate returned no row for — absence of entries is
    /// absence of hours, not absence of the project.
    #[must_use]
    pub fn none_yet(project_id: ProjectId) -> Self {
        Self {
            project_id,
            minutes: 0,
            billable_minutes: 0,
            approved_unbilled_minutes: 0,
            submitted_unbilled_minutes: 0,
            billed_minutes: 0,
            last_worked_on: None,
        }
    }

    /// How much of `budget_minutes` these hours have consumed, in basis points
    /// (10 000 = the whole budget), or `None` when the engagement carries no
    /// hours budget — or carries one of zero, which no proportion is defined
    /// against.
    ///
    /// Basis points rather than a percentage float, for the reason money is
    /// cents: a bar drawn from an integer is a bar two clients agree about.
    /// Consumption **past** the budget is reported as it is, over 10 000 — the
    /// budget is advisory (`docs/design/projects.md` § Budgets) and a figure
    /// clamped at "100%" would hide the one case the bar exists to show.
    #[must_use]
    pub fn budget_consumption_bp(&self, budget_minutes: Option<i64>) -> Option<i64> {
        match budget_minutes {
            Some(budget) if budget > 0 => Some(self.minutes.saturating_mul(10_000) / budget),
            _ => None,
        }
    }
}

/// The `task_projects` visibility predicate, spelled once: a shared board, or
/// the caller's own private one. Identical to [`AccountStore::task_projects`]'s
/// — a project's hours must be visible exactly when the project is.
const VISIBLE_PROJECT: &str = "(p.kind = 'team' OR (p.kind = 'personal' AND p.owner_user_id = $2))";

/// The aggregate columns, in [`HoursRow`] order.
const HOURS_COLS: &str = "e.project_id, \
     COALESCE(SUM(e.minutes), 0)::bigint AS minutes, \
     COALESCE(SUM(CASE WHEN e.billable THEN e.minutes ELSE 0 END), 0)::bigint AS billable_minutes, \
     COALESCE(SUM(CASE WHEN e.billable AND e.invoice_id IS NULL AND w.status = 'approved' \
         THEN e.minutes ELSE 0 END), 0)::bigint AS approved_unbilled_minutes, \
     COALESCE(SUM(CASE WHEN e.billable AND e.invoice_id IS NULL AND w.status = 'submitted' \
         THEN e.minutes ELSE 0 END), 0)::bigint AS submitted_unbilled_minutes, \
     COALESCE(SUM(CASE WHEN e.invoice_id IS NOT NULL THEN e.minutes ELSE 0 END), 0)::bigint \
         AS billed_minutes, \
     MAX(e.work_date) AS last_worked_on";

impl AccountStore {
    /// Hours to date for every project this caller can see that anybody has
    /// worked on, newest activity first.
    ///
    /// A project with no entries is **absent** rather than present with zeroes:
    /// the engagement list joins this onto
    /// [`AccountStore::task_projects`] and fills the gap with
    /// [`ProjectHours::none_yet`], so the shape of this answer stays "what has
    /// been worked" rather than "every board, restated".
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn project_hours(&self) -> Result<Vec<ProjectHours>> {
        let rows = sqlx::query_as::<_, HoursRow>(&format!(
            "SELECT {HOURS_COLS} FROM time_entries e \
             JOIN task_projects p ON p.tenant_id = e.tenant_id AND p.id = e.project_id \
             LEFT JOIN time_weeks w ON w.tenant_id = e.tenant_id AND w.user_id = e.user_id \
               AND w.week_start = date_trunc('week', e.work_date)::date \
             WHERE e.tenant_id = $1 AND e.state = 'active' AND {VISIBLE_PROJECT} \
             GROUP BY e.project_id \
             ORDER BY MAX(e.work_date) DESC, e.project_id"
        ))
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(HoursRow::into_hours).collect())
    }

    /// Hours to date for one project the caller can see.
    ///
    /// A project nobody has worked on answers [`ProjectHours::none_yet`]; a
    /// project this caller cannot see — another tenant's, or a colleague's
    /// private board — answers [`StoreError::NotFound`], the same denial an id
    /// that never existed gets.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the project is not one this caller can
    /// see; [`StoreError::Db`] on failure.
    pub async fn project_hours_for(&self, project: &ProjectId) -> Result<ProjectHours> {
        // Existence is settled against the board, not against the entries:
        // "nobody has logged an hour" and "there is no such project" are
        // different answers and a caller acts differently on each.
        let visible: Option<(String,)> = sqlx::query_as(&format!(
            "SELECT p.id FROM task_projects p \
             WHERE p.tenant_id = $1 AND p.id = $3 AND {VISIBLE_PROJECT}"
        ))
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(project.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if visible.is_none() {
            return Err(StoreError::NotFound);
        }
        let row = sqlx::query_as::<_, HoursRow>(&format!(
            "SELECT {HOURS_COLS} FROM time_entries e \
             JOIN task_projects p ON p.tenant_id = e.tenant_id AND p.id = e.project_id \
             LEFT JOIN time_weeks w ON w.tenant_id = e.tenant_id AND w.user_id = e.user_id \
               AND w.week_start = date_trunc('week', e.work_date)::date \
             WHERE e.tenant_id = $1 AND e.state = 'active' AND e.project_id = $3 \
               AND {VISIBLE_PROJECT} \
             GROUP BY e.project_id"
        ))
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(project.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(row.map_or_else(
            || ProjectHours::none_yet(project.clone()),
            HoursRow::into_hours,
        ))
    }
}

// ---- row types --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct HoursRow {
    project_id: String,
    minutes: i64,
    billable_minutes: i64,
    approved_unbilled_minutes: i64,
    submitted_unbilled_minutes: i64,
    billed_minutes: i64,
    last_worked_on: Option<Date>,
}

impl HoursRow {
    fn into_hours(self) -> ProjectHours {
        ProjectHours {
            project_id: ProjectId::new(self.project_id),
            minutes: self.minutes,
            billable_minutes: self.billable_minutes,
            approved_unbilled_minutes: self.approved_unbilled_minutes,
            submitted_unbilled_minutes: self.submitted_unbilled_minutes,
            billed_minutes: self.billed_minutes,
            last_worked_on: self.last_worked_on,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hours(minutes: i64) -> ProjectHours {
        ProjectHours {
            minutes,
            ..ProjectHours::none_yet(ProjectId::new("p"))
        }
    }

    #[test]
    fn a_project_nobody_worked_is_zero_and_not_absent() {
        let empty = ProjectHours::none_yet(ProjectId::new("p"));
        assert_eq!(empty.minutes, 0);
        assert_eq!(empty.billable_minutes, 0);
        assert_eq!(empty.approved_unbilled_minutes, 0);
        assert_eq!(empty.submitted_unbilled_minutes, 0);
        assert_eq!(empty.billed_minutes, 0);
        assert_eq!(empty.last_worked_on, None);
    }

    #[test]
    fn consumption_is_basis_points_of_the_hours_budget() {
        assert_eq!(hours(0).budget_consumption_bp(Some(6_000)), Some(0));
        assert_eq!(hours(3_000).budget_consumption_bp(Some(6_000)), Some(5_000));
        assert_eq!(
            hours(6_000).budget_consumption_bp(Some(6_000)),
            Some(10_000)
        );
    }

    #[test]
    fn overrun_is_reported_rather_than_clamped() {
        // The budget is advisory; a bar pinned at "100%" would hide the one
        // case somebody opens this screen to find.
        assert_eq!(
            hours(9_000).budget_consumption_bp(Some(6_000)),
            Some(15_000)
        );
    }

    #[test]
    fn no_budget_is_no_proportion() {
        assert_eq!(hours(3_000).budget_consumption_bp(None), None);
        // A budget of zero is not a budget you can be 0% or 100% through.
        assert_eq!(hours(3_000).budget_consumption_bp(Some(0)), None);
        assert_eq!(hours(0).budget_consumption_bp(Some(0)), None);
    }

    #[test]
    fn an_implausible_number_of_minutes_cannot_wrap_the_proportion() {
        // `minutes` is bounded by 1440 per entry, so this is unreachable in
        // practice — and saturating there rather than panicking is what makes
        // that a claim about arithmetic instead of about the data.
        assert_eq!(
            hours(i64::MAX).budget_consumption_bp(Some(1)),
            Some(i64::MAX)
        );
    }
}
