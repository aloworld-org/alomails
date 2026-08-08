//! The profitability report (alo Projects, ADR 0035, wave B3.08) — what an
//! engagement's hours are worth, against what somebody budgeted for it.
//!
//! `docs/features.md` calls this "hours × rates vs budget", and that is exactly
//! what it is: **the revenue side**. Salary and cost rates — the other half of
//! the word profitability — need B4's ledger and B6's employees, so every label
//! here says *value* and *budget* and none of them says *margin*
//! (`docs/design/projects.md` § Budgets and the profitability report).
//!
//! # Two answers, and why they are dated differently
//!
//! A period bounds work; a budget does not. So each engagement carries both:
//!
//! - **The period's figures** — minutes logged between `from` and `to`,
//!   billable and not, what the billable ones are worth, and how much of that is
//!   already on a document. This is "what did this quarter produce".
//! - **The to-date figures** — everything up to and including `to`, which is
//!   what a budget is consumed by. Comparing one quarter's hours against a
//!   budget for a whole engagement would report a project as 20% used forever.
//!
//! *Rejected: bounding the budget consumption by the period too.* It reads
//! tidier and answers nothing — a budget is a total, and a fraction of it is
//! only meaningful against everything spent so far. *Also rejected: consumption
//! to "now"* rather than to the period's last day: a report of Q1, re-read in
//! Q4, would then move under the reader, and two reads of a closed quarter must
//! agree.
//!
//! # Currencies are grouped, never converted
//!
//! [`crate::crm_report`]'s rule, for the same reason: adding euros to dollars at
//! a rate somebody chose today invents a figure that reconciles against nothing.
//! An engagement whose hours were priced in two currencies answers one row per
//! currency, and the **money** budget is measured only against the value in the
//! engagement's own currency — the currency that budget is stated in.
//!
//! # Rated and unrated hours
//!
//! An unpriced engagement is legal and normal ([`crate::project_clients`]), so a
//! billable hour carrying no rate is counted and **never priced**: valuing it at
//! zero would report an engagement as producing nothing when what is missing is
//! a price nobody has set. Those minutes are their own figure, so a reader can
//! see the gap and close it.
//!
//! Money is folded through [`crate::time_hours::hours_net_cents`] over the very
//! figures a billing line carries, per (project, rate, currency) group and with
//! the minutes summed **before** the conversion — which is what makes a figure
//! on this report and a figure on the printed invoice the same figure.
//!
//! # Who may read it
//!
//! A project aggregate, so anyone who can see the project can see it, and
//! **nothing here names a person**: the type has no per-user field, the SQL
//! groups by project and never by user, and the visibility predicate is the one
//! [`crate::project_hours`] uses. Proposals are excluded — an entry the agent
//! drafted is not an hour until a human accepts it (ADR 0023).

use time::Date;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::{BillingCustomerId, ProjectId};
use crate::time_hours::hours_net_cents;

/// What one currency's rated hours are worth over a period.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfitabilityCurrency {
    /// ISO 4217 code, uppercase — the currency the hours were **priced in**,
    /// snapshotted onto each entry when it was written.
    pub currency: String,
    /// Billable minutes carrying a rate in this currency.
    pub billable_minutes: i64,
    /// What they are worth, net of VAT, in integer cents. VAT is not here: a
    /// line's tax is rounded at the rate subtotal, not per line, so a per-group
    /// VAT figure would be a number that appears on no document.
    pub net_cents: i64,
    /// The subset already carried onto a billing document.
    pub billed_minutes: i64,
    /// What that subset is worth, computed the same way.
    pub billed_net_cents: i64,
}

impl ProfitabilityCurrency {
    /// What has been earned but not yet put on a document, in integer cents.
    /// The figure a partner actually chases at the end of a month.
    #[must_use]
    pub fn unbilled_net_cents(&self) -> i64 {
        self.net_cents.saturating_sub(self.billed_net_cents)
    }

    /// Folds another tally of the same currency into this one.
    fn add(&mut self, other: &Self) {
        self.billable_minutes = self.billable_minutes.saturating_add(other.billable_minutes);
        self.net_cents = self.net_cents.saturating_add(other.net_cents);
        self.billed_minutes = self.billed_minutes.saturating_add(other.billed_minutes);
        self.billed_net_cents = self.billed_net_cents.saturating_add(other.billed_net_cents);
    }
}

/// One engagement's whole answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectProfitability {
    /// The board the hours were logged against.
    pub project_id: ProjectId,
    /// Its name, as it reads today — so a saved file says which engagement it
    /// is about.
    pub project_name: String,
    /// The customer the work is billed to. Always present: the report is over
    /// engagements, and a project with no client facts is internal work.
    pub customer_id: BillingCustomerId,
    /// The engagement's own currency, from its client facts — the currency
    /// [`Self::budget_cents`] is stated in.
    pub currency: String,
    /// The hours budget, held as minutes, or `None` when nobody set one.
    /// Advisory: nothing refuses an hour logged past it.
    pub budget_minutes: Option<i64>,
    /// The money budget in integer cents of [`Self::currency`], or `None`.
    pub budget_cents: Option<i64>,
    /// Every accepted minute inside the period, billable or not.
    pub minutes: i64,
    /// The subset somebody marked chargeable.
    pub billable_minutes: i64,
    /// Billable minutes inside the period carrying no rate: counted here and
    /// priced nowhere, because an hour is never valued at a price nobody set.
    pub unrated_minutes: i64,
    /// One row per currency the period's rated hours were priced in, in code
    /// order. Never added together.
    pub by_currency: Vec<ProfitabilityCurrency>,
    /// Every accepted minute up to and including the period's last day — what
    /// the hours budget is consumed by.
    pub to_date_minutes: i64,
    /// What the rated billable hours to date are worth **in the engagement's
    /// own currency** — what the money budget is consumed by. Hours priced in
    /// another currency are not converted and are not in this figure.
    pub to_date_net_cents: i64,
}

impl ProjectProfitability {
    /// How much of the hours budget has been consumed to date, in basis points
    /// (10 000 = the whole budget), or `None` when there is no hours budget —
    /// or one of zero, which no proportion is defined against.
    ///
    /// Basis points rather than a percentage float, for the reason money is
    /// cents. Consumption past the budget is reported as it is, over 10 000:
    /// the budget is advisory and a figure clamped at "100%" would hide the one
    /// case a reader opens this report to find.
    #[must_use]
    pub fn hours_consumption_bp(&self) -> Option<i64> {
        consumption_bp(self.to_date_minutes, self.budget_minutes)
    }

    /// The same proportion for the money budget, over the value of the rated
    /// hours to date in the engagement's own currency.
    #[must_use]
    pub fn budget_consumption_bp(&self) -> Option<i64> {
        consumption_bp(self.to_date_net_cents, self.budget_cents)
    }

    /// What is left of the money budget, in integer cents, or `None` when there
    /// is no money budget. Negative when the engagement is over it — an overrun
    /// is a fact, not a floor at zero.
    #[must_use]
    pub fn budget_remaining_cents(&self) -> Option<i64> {
        self.budget_cents
            .map(|budget| budget.saturating_sub(self.to_date_net_cents))
    }
}

/// A proportion in basis points, or `None` when there is nothing to be a
/// proportion of.
fn consumption_bp(spent: i64, budget: Option<i64>) -> Option<i64> {
    match budget {
        Some(budget) if budget > 0 => Some(spent.saturating_mul(10_000) / budget),
        _ => None,
    }
}

/// The whole report: one period, one row per engagement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfitabilityReport {
    /// First day counted, inclusive.
    pub from: Date,
    /// Last day counted, inclusive — and the day the to-date figures are taken
    /// at.
    pub to: Date,
    /// The engagements, by name. An engagement nobody worked in the period is
    /// present with zeroes rather than absent: "we did not touch it" is an
    /// answer this report exists to give.
    pub projects: Vec<ProjectProfitability>,
}

/// What a whole report adds up to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfitabilityTotals {
    /// Every accepted minute in the period, across every engagement.
    pub minutes: i64,
    /// The chargeable subset.
    pub billable_minutes: i64,
    /// The chargeable minutes carrying no rate.
    pub unrated_minutes: i64,
    /// The value, one row per currency, in code order. Never a grand total:
    /// this report does not convert.
    pub by_currency: Vec<ProfitabilityCurrency>,
}

/// Folds a report's engagements into its totals.
///
/// Pure, and in the store rather than at the edge because it is money: a figure
/// on a screen must be one the server computed from the rows a document would
/// carry.
#[must_use]
pub fn profitability_totals(projects: &[ProjectProfitability]) -> ProfitabilityTotals {
    let mut totals = ProfitabilityTotals::default();
    for project in projects {
        totals.minutes = totals.minutes.saturating_add(project.minutes);
        totals.billable_minutes = totals
            .billable_minutes
            .saturating_add(project.billable_minutes);
        totals.unrated_minutes = totals
            .unrated_minutes
            .saturating_add(project.unrated_minutes);
        for row in &project.by_currency {
            merge(&mut totals.by_currency, row);
        }
    }
    totals
}

/// Adds a currency tally into a list kept sorted by code, so two reads of the
/// same rows agree byte for byte.
fn merge(rows: &mut Vec<ProfitabilityCurrency>, row: &ProfitabilityCurrency) {
    match rows.binary_search_by(|seen| seen.currency.cmp(&row.currency)) {
        Ok(at) => rows[at].add(row),
        Err(at) => rows.insert(at, row.clone()),
    }
}

/// The `task_projects` visibility predicate, the same one
/// [`crate::project_hours`] uses: a shared board, or the caller's own private
/// one. Client facts can only be attached to a `team` board today, so the
/// second branch is unreachable — and it is spelled out anyway, because a
/// project's report must be visible exactly when the project is, not when a
/// second rule somewhere else happens to agree.
const VISIBLE_PROJECT: &str = "(p.kind = 'team' OR (p.kind = 'personal' AND p.owner_user_id = $2))";

/// The grouped read behind the report: one row per (engagement, rate,
/// currency), with the period's minutes and the to-date minutes side by side.
///
/// The join to `time_entries` is a LEFT JOIN with the date ceiling **in the ON
/// clause**, so an engagement nobody has worked still answers a row — moving
/// that predicate to the WHERE clause would silently drop every untouched
/// engagement from the report.
const REPORT_SQL: &str = "\
    SELECT p.id AS project_id, p.name AS project_name, c.customer_id, c.currency, \
        c.budget_minutes, c.budget_cents, e.rate_cents, e.currency AS entry_currency, \
        COALESCE(SUM(e.minutes) FILTER (WHERE e.work_date >= $3), 0)::bigint AS minutes, \
        COALESCE(SUM(e.minutes) FILTER (WHERE e.work_date >= $3 AND e.billable), 0)::bigint \
            AS billable_minutes, \
        COALESCE(SUM(e.minutes) FILTER (WHERE e.work_date >= $3 AND e.billable \
            AND e.invoice_id IS NOT NULL), 0)::bigint AS billed_minutes, \
        COALESCE(SUM(e.minutes), 0)::bigint AS to_date_minutes, \
        COALESCE(SUM(e.minutes) FILTER (WHERE e.billable), 0)::bigint AS to_date_billable_minutes \
    FROM project_clients c \
    JOIN task_projects p ON p.tenant_id = c.tenant_id AND p.id = c.project_id \
    LEFT JOIN time_entries e ON e.tenant_id = c.tenant_id AND e.project_id = c.project_id \
        AND e.state = 'active' AND e.work_date <= $4 \
    WHERE c.tenant_id = $1 AND {visible} AND ($5::text IS NULL OR c.project_id = $5) \
    GROUP BY p.id, p.name, c.customer_id, c.currency, c.budget_minutes, c.budget_cents, \
        e.rate_cents, e.currency \
    ORDER BY lower(p.name), p.id, e.currency, e.rate_cents";

impl AccountStore {
    /// The profitability of every engagement this caller can see, over a stated
    /// period — or of one of them.
    ///
    /// One statement, tenant-bound by construction, grouped by engagement and
    /// rate; the money is folded in Rust through the same code a billing line
    /// uses, so this report and an invoice cannot disagree by a cent. Nothing
    /// is converted between currencies and nothing is summed across them.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the period ends before it starts;
    /// [`StoreError::NotFound`] when a named project is not one this caller can
    /// see, or is not client work — internal work has neither a rate nor a
    /// budget, and the denial is the same one an id that never existed gets, so
    /// this is not an existence oracle; [`StoreError::Db`] on failure.
    pub async fn project_profitability(
        &self,
        from: Date,
        to: Date,
        project: Option<&ProjectId>,
    ) -> Result<ProfitabilityReport> {
        if to < from {
            return Err(StoreError::Validation(
                "the period ends before it starts".to_owned(),
            ));
        }
        let rows =
            sqlx::query_as::<_, ReportRow>(&REPORT_SQL.replace("{visible}", VISIBLE_PROJECT))
                .bind(self.tenant.as_str())
                .bind(self.user.as_str())
                .bind(from)
                .bind(to)
                .bind(project.map(ProjectId::as_str))
                .fetch_all(&self.pool)
                .await
                .map_err(StoreError::Db)?;
        if project.is_some() && rows.is_empty() {
            return Err(StoreError::NotFound);
        }
        Ok(ProfitabilityReport {
            from,
            to,
            projects: assemble(&rows),
        })
    }
}

// ---- row types --------------------------------------------------------------

/// One grouped row of [`REPORT_SQL`]: an engagement at one rate in one
/// currency. No user, no day and no note — a project aggregate discloses
/// projects, minutes and money, never who worked when.
#[derive(Debug, sqlx::FromRow)]
struct ReportRow {
    project_id: String,
    project_name: String,
    customer_id: String,
    /// The engagement's currency, from its client facts.
    currency: String,
    budget_minutes: Option<i64>,
    budget_cents: Option<i64>,
    /// The rate these hours were snapshotted at, `None` for unrated hours — and
    /// `None` on the single empty row an unworked engagement answers with.
    rate_cents: Option<i64>,
    /// The currency that rate is expressed in, snapshotted with it.
    entry_currency: Option<String>,
    minutes: i64,
    billable_minutes: i64,
    billed_minutes: i64,
    to_date_minutes: i64,
    to_date_billable_minutes: i64,
}

impl ReportRow {
    /// The rate and its currency, but only as the pair they were stored as: an
    /// hour is priced when it carries both, and half a snapshot prices nothing.
    fn priced(&self) -> Option<(i64, &str)> {
        match (self.rate_cents, self.entry_currency.as_deref()) {
            (Some(rate), Some(currency)) => Some((rate, currency)),
            _ => None,
        }
    }
}

/// Folds the grouped rows into one entry per engagement.
///
/// Split out from the query so the shape of the answer — what a row means, what
/// an absent one means, and where the money is folded — is decided in code that
/// is tested without a database.
///
/// The rows arrive ordered by engagement, so a change of project id closes the
/// previous one; within an engagement each row is one (rate, currency) group
/// whose minutes the database has already summed, which is what lets the
/// conversion to money happen **once** per group rather than once per entry
/// ([`crate::time_hours`]).
fn assemble(rows: &[ReportRow]) -> Vec<ProjectProfitability> {
    let mut projects: Vec<ProjectProfitability> = Vec::new();
    for row in rows {
        let open = projects
            .last()
            .is_some_and(|open| open.project_id.as_str() == row.project_id);
        if !open {
            projects.push(ProjectProfitability {
                project_id: ProjectId::new(row.project_id.clone()),
                project_name: row.project_name.clone(),
                customer_id: BillingCustomerId::new(row.customer_id.clone()),
                currency: row.currency.clone(),
                budget_minutes: row.budget_minutes,
                budget_cents: row.budget_cents,
                minutes: 0,
                billable_minutes: 0,
                unrated_minutes: 0,
                by_currency: Vec::new(),
                to_date_minutes: 0,
                to_date_net_cents: 0,
            });
        }
        let at = projects.len().saturating_sub(1);
        let project = &mut projects[at];
        project.minutes = project.minutes.saturating_add(row.minutes);
        project.billable_minutes = project
            .billable_minutes
            .saturating_add(row.billable_minutes);
        project.to_date_minutes = project.to_date_minutes.saturating_add(row.to_date_minutes);

        let Some((rate, currency)) = row.priced() else {
            // Unrated billable hours are counted and never priced. Unrated
            // *non*-billable hours are already inside `minutes` and are nobody's
            // gap to close.
            project.unrated_minutes = project.unrated_minutes.saturating_add(row.billable_minutes);
            continue;
        };
        merge(
            &mut project.by_currency,
            &ProfitabilityCurrency {
                currency: currency.to_owned(),
                billable_minutes: row.billable_minutes,
                net_cents: hours_net_cents(row.billable_minutes, rate),
                billed_minutes: row.billed_minutes,
                billed_net_cents: hours_net_cents(row.billed_minutes, rate),
            },
        );
        // The money budget is stated in the engagement's own currency, so only
        // hours priced in that currency consume it. Hours priced in another are
        // in `by_currency` and nowhere near the budget — converting them would
        // be an invented figure.
        if currency == project.currency {
            project.to_date_net_cents = project
                .to_date_net_cents
                .saturating_add(hours_net_cents(row.to_date_billable_minutes, rate));
        }
    }
    projects
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    /// One grouped row of an engagement: `Sunrise portal`, worked for
    /// `cust-1`, budgeted in EUR.
    fn row(rate: Option<i64>, currency: Option<&str>, minutes: i64) -> ReportRow {
        ReportRow {
            project_id: "prj-1".to_owned(),
            project_name: "Sunrise portal".to_owned(),
            customer_id: "cust-1".to_owned(),
            currency: "EUR".to_owned(),
            budget_minutes: Some(6_000),
            budget_cents: Some(1_000_000),
            rate_cents: rate,
            entry_currency: currency.map(str::to_owned),
            minutes,
            billable_minutes: minutes,
            billed_minutes: 0,
            to_date_minutes: minutes,
            to_date_billable_minutes: minutes,
        }
    }

    #[test]
    fn an_engagement_nobody_worked_is_a_row_of_zeroes_and_not_an_absence() {
        // The single row a LEFT JOIN answers with: no rate, no currency, no
        // minutes. "We did not touch it this quarter" is an answer.
        let empty = ReportRow {
            minutes: 0,
            billable_minutes: 0,
            to_date_minutes: 0,
            to_date_billable_minutes: 0,
            ..row(None, None, 0)
        };
        let projects = assemble(&[empty]);
        assert_eq!(projects.len(), 1);
        let project = &projects[0];
        assert_eq!(project.project_name, "Sunrise portal");
        assert_eq!(project.customer_id.as_str(), "cust-1");
        assert_eq!(project.minutes, 0);
        assert_eq!(project.unrated_minutes, 0);
        assert!(project.by_currency.is_empty(), "no money at all, not zero");
        assert_eq!(project.hours_consumption_bp(), Some(0));
        assert_eq!(project.budget_consumption_bp(), Some(0));
        assert_eq!(project.budget_remaining_cents(), Some(1_000_000));
    }

    #[test]
    fn rated_hours_are_worth_what_the_invoice_line_would_carry() {
        // 90 minutes at €95.00 an hour is €142.50 — the figure
        // `time_hours::hours_net_cents` produces, because it is that code.
        let projects = assemble(&[row(Some(9_500), Some("EUR"), 90)]);
        let money = &projects[0].by_currency[0];
        assert_eq!(money.currency, "EUR");
        assert_eq!(money.billable_minutes, 90);
        assert_eq!(money.net_cents, 14_250);
        assert_eq!(money.billed_net_cents, 0);
        assert_eq!(money.unbilled_net_cents(), 14_250);
        assert_eq!(projects[0].to_date_net_cents, 14_250);
    }

    #[test]
    fn a_group_is_summed_before_it_is_converted() {
        // The database sums the minutes of a (rate, currency) group; the fold
        // converts once. Ten one-minute stints are ten minutes, not ten
        // roundings — the property that keeps this report and the document
        // agreeing.
        let projects = assemble(&[row(Some(6_000), Some("EUR"), 10)]);
        assert_eq!(
            projects[0].by_currency[0].net_cents,
            hours_net_cents(10, 6_000)
        );
        assert!(projects[0].by_currency[0].net_cents < 10 * hours_net_cents(1, 6_000));
    }

    #[test]
    fn unrated_billable_hours_are_counted_and_never_priced() {
        let projects = assemble(&[row(Some(9_500), Some("EUR"), 60), row(None, None, 45)]);
        let project = &projects[0];
        assert_eq!(project.minutes, 105);
        assert_eq!(project.billable_minutes, 105);
        assert_eq!(project.unrated_minutes, 45, "never valued at zero");
        assert_eq!(project.by_currency.len(), 1);
        assert_eq!(project.by_currency[0].billable_minutes, 60);
        // …and they do not consume the money budget either, because nobody has
        // said what they are worth.
        assert_eq!(project.to_date_net_cents, 9_500);
    }

    #[test]
    fn half_a_snapshot_prices_nothing() {
        // A rate without its currency (or the other way round) cannot be
        // trusted to be money in any particular currency, so it is unrated.
        for broken in [row(Some(9_500), None, 60), row(None, Some("EUR"), 60)] {
            let projects = assemble(&[broken]);
            assert_eq!(projects[0].unrated_minutes, 60);
            assert!(projects[0].by_currency.is_empty());
        }
    }

    #[test]
    fn non_billable_hours_are_in_the_minutes_and_in_no_money_figure() {
        let internal = ReportRow {
            billable_minutes: 0,
            to_date_billable_minutes: 0,
            ..row(None, None, 120)
        };
        let projects = assemble(&[internal]);
        assert_eq!(projects[0].minutes, 120);
        assert_eq!(projects[0].billable_minutes, 0);
        assert_eq!(
            projects[0].unrated_minutes, 0,
            "an hour nobody meant to charge for is not a missing price"
        );
    }

    #[test]
    fn two_currencies_are_two_rows_and_are_never_added_together() {
        let projects = assemble(&[
            row(Some(10_000), Some("USD"), 60),
            row(Some(9_500), Some("EUR"), 60),
        ]);
        let money = &projects[0].by_currency;
        assert_eq!(
            money
                .iter()
                .map(|m| m.currency.as_str())
                .collect::<Vec<_>>(),
            ["EUR", "USD"],
            "sorted by code, so two reads of one engagement agree"
        );
        assert_eq!(money[0].net_cents, 9_500);
        assert_eq!(money[1].net_cents, 10_000);
        // The budget is stated in the engagement's currency (EUR), so only the
        // euro hours consume it: the dollars are reported and never converted.
        assert_eq!(projects[0].to_date_net_cents, 9_500);
        assert_eq!(projects[0].budget_remaining_cents(), Some(990_500));
    }

    #[test]
    fn two_rates_on_one_engagement_fold_into_one_currency_row() {
        // A rate rise mid-engagement is two groups of hours and one currency.
        let projects = assemble(&[
            row(Some(9_500), Some("EUR"), 60),
            row(Some(11_000), Some("EUR"), 60),
        ]);
        assert_eq!(projects[0].by_currency.len(), 1);
        assert_eq!(projects[0].by_currency[0].billable_minutes, 120);
        assert_eq!(projects[0].by_currency[0].net_cents, 20_500);
    }

    #[test]
    fn the_billed_subset_is_carried_beside_the_value_not_inside_it() {
        let partly = ReportRow {
            billed_minutes: 60,
            ..row(Some(9_500), Some("EUR"), 120)
        };
        let money = &assemble(&[partly])[0].by_currency[0];
        assert_eq!(money.net_cents, 19_000);
        assert_eq!(money.billed_net_cents, 9_500);
        assert_eq!(
            money.unbilled_net_cents(),
            9_500,
            "what is still to invoice"
        );
    }

    #[test]
    fn the_period_bounds_the_work_and_the_budget_is_consumed_to_date() {
        // 120 minutes this quarter, 600 minutes since the engagement began.
        let older = ReportRow {
            minutes: 120,
            billable_minutes: 120,
            to_date_minutes: 600,
            to_date_billable_minutes: 600,
            ..row(Some(9_500), Some("EUR"), 0)
        };
        let project = &assemble(&[older])[0];
        assert_eq!(project.minutes, 120, "the period's work");
        assert_eq!(project.by_currency[0].net_cents, 19_000);
        assert_eq!(project.to_date_minutes, 600);
        assert_eq!(project.to_date_net_cents, 95_000, "everything up to `to`");
        // A quarter's hours against a whole engagement's budget would report
        // this as 2% used; it is 10%.
        assert_eq!(project.hours_consumption_bp(), Some(1_000));
        assert_eq!(project.budget_consumption_bp(), Some(950));
    }

    #[test]
    fn an_overrun_is_reported_rather_than_clamped() {
        let over = ReportRow {
            to_date_minutes: 9_000,
            to_date_billable_minutes: 12_000,
            ..row(Some(9_500), Some("EUR"), 60)
        };
        let project = &assemble(&[over])[0];
        assert_eq!(project.hours_consumption_bp(), Some(15_000));
        assert_eq!(project.budget_consumption_bp(), Some(19_000));
        assert!(
            project.budget_remaining_cents().unwrap_or_default() < 0,
            "an engagement past its budget says so"
        );
    }

    #[test]
    fn an_absent_budget_is_no_proportion_and_a_zero_budget_is_not_one_either() {
        let unbudgeted = ReportRow {
            budget_minutes: None,
            budget_cents: None,
            ..row(Some(9_500), Some("EUR"), 60)
        };
        let project = &assemble(&[unbudgeted])[0];
        assert_eq!(project.hours_consumption_bp(), None);
        assert_eq!(project.budget_consumption_bp(), None);
        assert_eq!(project.budget_remaining_cents(), None);

        let zero = ReportRow {
            budget_minutes: Some(0),
            budget_cents: Some(0),
            ..row(Some(9_500), Some("EUR"), 60)
        };
        let project = &assemble(&[zero])[0];
        assert_eq!(project.hours_consumption_bp(), None);
        assert_eq!(project.budget_consumption_bp(), None);
    }

    #[test]
    fn a_change_of_engagement_closes_the_previous_one() {
        let second = ReportRow {
            project_id: "prj-2".to_owned(),
            project_name: "Warehouse".to_owned(),
            customer_id: "cust-2".to_owned(),
            ..row(Some(8_000), Some("EUR"), 30)
        };
        let projects = assemble(&[row(Some(9_500), Some("EUR"), 60), second]);
        assert_eq!(projects.len(), 2);
        assert_eq!(projects[0].project_id.as_str(), "prj-1");
        assert_eq!(projects[1].project_id.as_str(), "prj-2");
        assert_eq!(projects[1].by_currency[0].net_cents, 4_000);
    }

    #[test]
    fn the_totals_are_per_currency_and_never_a_grand_total() {
        let mut usd = assemble(&[row(Some(10_000), Some("USD"), 120)]);
        usd[0].project_id = ProjectId::new("prj-2");
        let eur = assemble(&[row(Some(9_500), Some("EUR"), 60), row(None, None, 30)]);
        let all: Vec<ProjectProfitability> = eur.into_iter().chain(usd).collect();

        let totals = profitability_totals(&all);
        assert_eq!(totals.minutes, 210);
        assert_eq!(totals.billable_minutes, 210);
        assert_eq!(totals.unrated_minutes, 30);
        assert_eq!(
            totals.by_currency.len(),
            2,
            "euros are not added to dollars"
        );
        assert_eq!(totals.by_currency[0].currency, "EUR");
        assert_eq!(totals.by_currency[0].net_cents, 9_500);
        assert_eq!(totals.by_currency[1].currency, "USD");
        assert_eq!(totals.by_currency[1].net_cents, 20_000);
    }

    #[test]
    fn an_empty_report_totals_to_nothing_rather_than_to_a_currency_it_invented() {
        let totals = profitability_totals(&[]);
        assert_eq!(totals.minutes, 0);
        assert!(totals.by_currency.is_empty());
    }
}
