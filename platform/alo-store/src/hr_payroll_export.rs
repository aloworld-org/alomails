//! The payroll period, folded (alo HR, ADR 0035, wave B6.10;
//! `docs/design/hr.md`, "Payroll export").
//!
//! # What this is, and the one thing it must never become
//!
//! A per-period file of **the facts we hold** about the people a company
//! employs: staff number, name, national identifier, IBAN, contract, working
//! pattern, the pay figure on their terms, the leave they took inside the
//! period, and the claims they are owed back. A payroll bureau turns that into
//! wages.
//!
//! **There is no calculation here.** No gross-to-net, no tax, no social
//! contribution, no accrual, no proration of a monthly salary. That is not a
//! scope cut for this wave: payroll calculation is a **permanent non-goal**
//! (`ROADMAP.md`, ADR 0035), because it is per-country statutory software with a
//! compliance obligation per member state and an update cycle we would be
//! signing up to forever. Every figure below is either a stored fact or a sum of
//! stored facts, and the one place a number is *derived* — leave minutes — is
//! derived by exactly the fold a balance is
//! ([`crate::hr_leave_balances`]), so the file and the screen cannot disagree.
//!
//! # Six decisions this fold makes
//!
//! **The period selects employments, not people.** Somebody is on the file when
//! a period of employment overlapped the period asked for — so a leaver who
//! worked the first week is on March's file, and somebody who starts in April is
//! not. Archived records are read for the same reason: a leaver is paid.
//!
//! **The terms are the latest ones the period touched.** A pay rise mid-period
//! puts the new terms on the file, because that is what the bureau is being told
//! to pay from; the previous terms are still on the record, and the period's
//! *leave* is charged at the pattern in force on each day rather than at these
//! terms (see below). A tenant who needs the file split at a change of terms
//! draws two periods.
//!
//! **A contractor is not on the payroll.** [`ContractKind::Contractor`] is
//! somebody who invoices: on the org chart, out of this file
//! ([`crate::hr_employments`] says so where the vocabulary is defined). Leaving
//! them in would put a supplier into a wage run.
//!
//! **Leave is separated by what it does to pay, not by policy name.** An unpaid
//! absence is the one thing on this file that changes what somebody is paid, so
//! it gets its own column; sick leave gets its own because most member states
//! treat it apart; everything else paid is one figure. The rule is the policy's
//! own two facts — [`LeavePolicy::paid`] and [`LeaveKind`] — never a name
//! somebody typed. **Public holidays inside an absence cost nothing**, exactly
//! as they cost nothing against a balance.
//!
//! **Claims are the employee's own money, already decided.** Personal-method
//! expenses (a train ticket) and mileage (an ordinary expense with a journey
//! behind it, [`crate::fin_mileage`]) that a manager has approved, dated by the
//! day the money was spent. A claim nobody has decided is not owed yet and is
//! not on the file; if it is approved after the file was drawn it appears in the
//! next draw of the same period, which is why the draw is recorded with the day
//! it happened.
//!
//! **Money never crosses currencies.** A claim in a currency other than the
//! person's pay currency is *not* silently added: it is counted, and the count
//! is a column ([`PayrollLine::claims_other_currency`]). A file that had quietly
//! added JPY to EUR would be wrong in the one direction nobody checks.
//!
//! # Nothing is stored twice
//!
//! The fold reads; [`TenantStore::record_hr_payroll_export`] writes the receipt
//! — a period, a mapping, a count, a person and a time. The figures are not kept
//! (`migrations/0207_hr_payroll_exports.sql`, decision 3): we do not hold a copy
//! of a document carrying every employee's pay and IBAN in a table we would then
//! have to defend.
//!
//! The rendering — which columns, in which order, under which heading, with
//! which date and decimal format — is [`crate::hr_payroll_mapping`]. This module
//! decides *what is true*; that one decides *what a Belgian bureau's sheet looks
//! like*.

use std::collections::BTreeMap;

use time::{Date, OffsetDateTime};

use crate::error::{Result, StoreError};
use crate::hr_employees::display_name;
use crate::hr_employments::{ContractKind, Employment, PayPeriod};
use crate::hr_leave_policies::{LeaveKind, LeavePolicy};
use crate::hr_leave_requests::{LeaveRequestQuery, LeaveStatus};
use crate::id::{HrEmployeeId, HrPayrollExportId, UserId};
use crate::store::TenantStore;

/// The longest period one file may cover. A payroll period is a month or a
/// fortnight; a year is already generous, and an unbounded range is a read whose
/// cost the caller chooses — the same ceiling
/// [`crate::hr_absences::ABSENCE_WINDOW_MAX_DAYS`] puts on the absence layer.
pub const PAYROLL_PERIOD_MAX_DAYS: i64 = 366;

/// Minutes in an hour — the one conversion the file performs, and it is a unit
/// change rather than a calculation.
pub const MINUTES_PER_HOUR: i64 = 60;

/// One person on one period's file: everything the export knows about them, in
/// the units they are stored in (cents, minutes, whole dates). Nothing here is
/// formatted — [`crate::hr_payroll_mapping`] does that, per country.
#[derive(Debug, Clone)]
pub struct PayrollLine {
    /// The record this line is about, so a bureau's query can be answered.
    pub employee_id: HrEmployeeId,
    /// The tenant's own staff number, blank when they have not given one.
    pub staff_number: String,
    /// Given (first) name.
    pub given_name: String,
    /// Family (last) name.
    pub family_name: String,
    /// What they are called — the directory's own rule, not a second one.
    pub full_name: String,
    /// National identifier / social-security number, blank when not recorded.
    pub national_id: String,
    /// The account wages are paid into, blank when not recorded.
    pub iban: String,
    /// ISO 3166-1 alpha-2 country of their home address, blank when not
    /// recorded.
    pub country: String,
    /// Date of birth, when recorded: most member states' filings need it.
    pub date_of_birth: Option<Date>,
    /// The job title on the terms this file is drawn from.
    pub job_title: String,
    /// The team on those terms.
    pub team: String,
    /// The kind of contract.
    pub contract_kind: ContractKind,
    /// The day those terms began — which may be inside the period (a joiner).
    pub started_on: Date,
    /// The day they ended, when they have (a leaver, or a change of terms after
    /// the period).
    pub ended_on: Option<Date>,
    /// Minutes in their normal week, from the working pattern.
    pub weekly_minutes: i32,
    /// Gross pay in integer cents, or `None` for an unpaid intern.
    pub pay_amount_cents: Option<i64>,
    /// What that figure is per.
    pub pay_period: PayPeriod,
    /// ISO 4217 currency the pay is in.
    pub pay_currency: String,
    /// Approved paid leave taken inside the period, in minutes.
    pub paid_leave_minutes: i64,
    /// Approved sick leave taken inside the period, in minutes.
    pub sick_leave_minutes: i64,
    /// Approved **unpaid** leave taken inside the period — the one absence that
    /// changes what somebody is paid.
    pub unpaid_leave_minutes: i64,
    /// Approved personal expense claims spent inside the period, in cents of
    /// [`Self::pay_currency`], excluding mileage.
    pub expense_cents: i64,
    /// Approved mileage allowance spent inside the period, in cents of
    /// [`Self::pay_currency`].
    pub mileage_cents: i64,
    /// How many approved claims were left out because they are in another
    /// currency. A count, never an amount: the file states that something was
    /// not added rather than adding it wrongly.
    pub claims_other_currency: i64,
}

impl PayrollLine {
    /// All leave taken in the period, paid and unpaid — the figure a file
    /// carrying one leave column shows.
    #[must_use]
    pub fn total_leave_minutes(&self) -> i64 {
        self.paid_leave_minutes
            .saturating_add(self.sick_leave_minutes)
            .saturating_add(self.unpaid_leave_minutes)
    }

    /// What the company owes this person back, in cents — expenses and mileage
    /// together, both already approved.
    #[must_use]
    pub fn claims_cents(&self) -> i64 {
        self.expense_cents.saturating_add(self.mileage_cents)
    }
}

/// The receipt for one draw of the file: no figures, and nobody's name.
#[derive(Debug, Clone)]
pub struct PayrollExport {
    /// Opaque id, unique within the tenant.
    pub id: HrPayrollExportId,
    /// First day of the period, inclusive.
    pub from_day: Date,
    /// Last day of the period, inclusive.
    pub to_day: Date,
    /// The column mapping it was rendered in.
    pub mapping_key: String,
    /// How many people were on it.
    pub line_count: i32,
    /// The user who drew it.
    pub drawn_by: String,
    /// When they drew it.
    pub created_at: OffsetDateTime,
}

/// Which column of the file a policy's minutes belong in — the fold's one
/// classification, made from the policy's own two facts.
fn leave_bucket(policy: &LeavePolicy) -> LeaveBucket {
    if !policy.paid {
        LeaveBucket::Unpaid
    } else if policy.kind == LeaveKind::Sick {
        LeaveBucket::Sick
    } else {
        LeaveBucket::Paid
    }
}

/// The three leave columns. Deliberately not the policy vocabulary: a tenant
/// may run six policies, and a payroll file has the columns a bureau reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LeaveBucket {
    /// Paid leave of any kind but sickness.
    Paid,
    /// Paid sick leave, which most member states treat apart.
    Sick,
    /// Unpaid absence — the one that changes what somebody is paid.
    Unpaid,
}

/// The terms this file is drawn from: the latest period of employment that
/// overlapped `from..=to`.
///
/// Pure, so the rule is testable without a database. `None` when no period
/// touched the window, which is what keeps somebody who joins next month off
/// this month's file.
#[must_use]
pub fn terms_for_period(employments: &[Employment], from: Date, to: Date) -> Option<&Employment> {
    employments
        .iter()
        .filter(|employment| {
            employment.started_on <= to && employment.ended_on.is_none_or(|end| end >= from)
        })
        .max_by_key(|employment| employment.started_on)
}

/// The employee columns as they are read. Kept apart from [`PayrollLine`]
/// because this is the row shape, and the line is what a period makes of it.
#[derive(sqlx::FromRow)]
struct PersonRow {
    id: String,
    user_id: Option<String>,
    staff_number: Option<String>,
    preferred_name: String,
    given_name: String,
    family_name: String,
    national_id: Option<String>,
    iban: Option<String>,
    country: String,
    date_of_birth: Option<Date>,
}

/// One person's approved claims inside the period, split by whether a journey
/// is behind them, in one currency.
#[derive(sqlx::FromRow)]
struct ClaimsRow {
    user_id: String,
    currency: String,
    expense_cents: i64,
    mileage_cents: i64,
    claim_count: i64,
}

impl TenantStore {
    /// The period's file, one line per person the period employed, ordered by
    /// family then given name — an alphabetical file is what a human checks
    /// against a list.
    ///
    /// **HR's read**: it returns pay, national identifiers and bank accounts,
    /// and the route in front of it is behind the HR door.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the period ends before it starts, is
    /// longer than [`PAYROLL_PERIOD_MAX_DAYS`], or covers no employment at all —
    /// never an empty file, which would read as "nobody is paid";
    /// [`StoreError::Db`] on failure.
    pub async fn hr_payroll_lines(&self, from: Date, to: Date) -> Result<Vec<PayrollLine>> {
        if to < from {
            return Err(StoreError::Validation(
                "the period must end on or after the day it starts".to_owned(),
            ));
        }
        if (to - from).whole_days() + 1 > PAYROLL_PERIOD_MAX_DAYS {
            return Err(StoreError::Validation(format!(
                "the period must not be longer than {PAYROLL_PERIOD_MAX_DAYS} days"
            )));
        }
        let people = sqlx::query_as::<_, PersonRow>(
            "SELECT e.id, e.user_id, e.staff_number, e.preferred_name, e.given_name, \
                 e.family_name, e.national_id, e.iban, e.country, e.date_of_birth \
               FROM hr_employees e \
              WHERE e.tenant_id = $1 \
                AND EXISTS ( \
                    SELECT 1 FROM hr_employments p \
                     WHERE p.tenant_id = e.tenant_id AND p.employee_id = e.id \
                       AND p.started_on <= $3 \
                       AND (p.ended_on IS NULL OR p.ended_on >= $2) \
                ) \
              ORDER BY e.family_name, e.given_name, e.id",
        )
        .bind(self.tenant().as_str())
        .bind(from)
        .bind(to)
        .fetch_all(self.pool())
        .await
        .map_err(StoreError::Db)?;
        if people.is_empty() {
            return Err(StoreError::Validation(
                "no period of employment covers these days — a payroll file over nobody is not a \
                 file"
                    .to_owned(),
            ));
        }

        // The three tenant-wide reads the fold needs, each once: what the
        // policies mean, which days are public holidays, and what everybody is
        // owed back.
        let policies: BTreeMap<String, LeaveBucket> = self
            .hr_leave_policies(true)
            .await?
            .iter()
            .map(|policy| (policy.id.as_str().to_owned(), leave_bucket(policy)))
            .collect();
        let holidays = self.hr_holidays().await?;
        let claims = self.payroll_claims(from, to).await?;
        let approved = self
            .hr_leave_requests(
                &LeaveRequestQuery::default()
                    .with_statuses(&[LeaveStatus::Approved])
                    .within(from, to),
            )
            .await?;

        let mut lines = Vec::with_capacity(people.len());
        for person in people {
            let employee = HrEmployeeId::new(person.id.clone());
            let employments = self.hr_employments(&employee).await?;
            let Some(terms) = terms_for_period(&employments, from, to) else {
                continue;
            };
            // A contractor invoices; they are on the chart and off the payroll.
            if terms.contract_kind == ContractKind::Contractor {
                continue;
            }
            let mut paid_leave_minutes = 0_i64;
            let mut sick_leave_minutes = 0_i64;
            let mut unpaid_leave_minutes = 0_i64;
            for request in approved
                .iter()
                .filter(|request| request.employee_id.as_str() == person.id)
            {
                let bucket = policies
                    .get(request.policy_id.as_str())
                    .copied()
                    .unwrap_or(LeaveBucket::Paid);
                let minutes = leave_minutes_in(
                    &employments,
                    request.from_day.max(from),
                    request.to_day.min(to),
                    &holidays,
                );
                match bucket {
                    LeaveBucket::Paid => paid_leave_minutes += minutes,
                    LeaveBucket::Sick => sick_leave_minutes += minutes,
                    LeaveBucket::Unpaid => unpaid_leave_minutes += minutes,
                }
            }
            let owed = person
                .user_id
                .as_ref()
                .and_then(|user| claims.get(user))
                .map_or_else(Owed::default, |owed| {
                    Owed::in_currency(owed, &terms.pay_currency)
                });
            lines.push(PayrollLine {
                employee_id: employee,
                staff_number: person.staff_number.unwrap_or_default(),
                full_name: display_name(
                    &person.preferred_name,
                    &person.given_name,
                    &person.family_name,
                ),
                given_name: person.given_name,
                family_name: person.family_name,
                national_id: person.national_id.unwrap_or_default(),
                iban: person.iban.unwrap_or_default(),
                country: person.country,
                date_of_birth: person.date_of_birth,
                job_title: terms.job_title.clone(),
                team: terms.team.clone(),
                contract_kind: terms.contract_kind,
                started_on: terms.started_on,
                ended_on: terms.ended_on,
                weekly_minutes: terms.weekly_minutes(),
                pay_amount_cents: terms.pay_amount_cents,
                pay_period: terms.pay_period,
                pay_currency: terms.pay_currency.clone(),
                paid_leave_minutes,
                sick_leave_minutes,
                unpaid_leave_minutes,
                expense_cents: owed.expense_cents,
                mileage_cents: owed.mileage_cents,
                claims_other_currency: owed.other_currency_claims,
            });
        }
        if lines.is_empty() {
            return Err(StoreError::Validation(
                "every person employed in these days invoices rather than being paid — there is \
                 no payroll file to draw"
                    .to_owned(),
            ));
        }
        Ok(lines)
    }

    /// What every user is owed back for the period, by currency: personal claims
    /// a manager has already approved, with mileage told apart by the journey
    /// behind it.
    ///
    /// A company card is not on this file — the company paid, and nobody is owed
    /// anything.
    async fn payroll_claims(&self, from: Date, to: Date) -> Result<BTreeMap<String, Vec<Owed>>> {
        let rows = sqlx::query_as::<_, ClaimsRow>(
            // `SUM` over `BIGINT` answers in `NUMERIC`; the cast keeps every
            // amount in this suite an `i64` of cents from the column to the
            // file, which is the rule money is never allowed to leave.
            "SELECT e.user_id, e.currency, \
                 COALESCE(SUM(CASE WHEN m.expense_id IS NULL THEN e.gross_cents ELSE 0 END), 0) \
                     ::BIGINT AS expense_cents, \
                 COALESCE(SUM(CASE WHEN m.expense_id IS NULL THEN 0 ELSE e.gross_cents END), 0) \
                     ::BIGINT AS mileage_cents, \
                 COUNT(*)::BIGINT AS claim_count \
               FROM fin_expenses e \
               LEFT JOIN fin_mileage m \
                      ON m.tenant_id = e.tenant_id AND m.expense_id = e.id \
              WHERE e.tenant_id = $1 \
                AND e.method = 'personal' \
                AND e.status IN ('approved', 'reimbursed') \
                AND e.spent_on BETWEEN $2 AND $3 \
              GROUP BY e.user_id, e.currency",
        )
        .bind(self.tenant().as_str())
        .bind(from)
        .bind(to)
        .fetch_all(self.pool())
        .await
        .map_err(StoreError::Db)?;
        let mut owed: BTreeMap<String, Vec<Owed>> = BTreeMap::new();
        for row in rows {
            owed.entry(row.user_id).or_default().push(Owed {
                currency: row.currency,
                expense_cents: row.expense_cents,
                mileage_cents: row.mileage_cents,
                other_currency_claims: row.claim_count,
            });
        }
        Ok(owed)
    }

    /// Files the fact that somebody drew the file — the receipt, not the file
    /// (`migrations/0207_hr_payroll_exports.sql`).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn record_hr_payroll_export(
        &self,
        from: Date,
        to: Date,
        mapping_key: &str,
        line_count: usize,
        actor: &UserId,
    ) -> Result<HrPayrollExportId> {
        let id = HrPayrollExportId::generate();
        sqlx::query(
            "INSERT INTO hr_payroll_exports \
                 (tenant_id, id, from_day, to_day, mapping_key, line_count, drawn_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(self.tenant().as_str())
        .bind(id.as_str())
        .bind(from)
        .bind(to)
        .bind(mapping_key)
        .bind(i32::try_from(line_count).unwrap_or(i32::MAX))
        .bind(actor.as_str())
        .execute(self.pool())
        .await
        .map_err(StoreError::Db)?;
        Ok(id)
    }

    /// The receipts, newest first — "when was this quarter last drawn, and by
    /// whom".
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn hr_payroll_exports(&self) -> Result<Vec<PayrollExport>> {
        let rows = sqlx::query_as::<_, ExportRow>(
            "SELECT id, from_day, to_day, mapping_key, line_count, drawn_by, created_at \
               FROM hr_payroll_exports \
              WHERE tenant_id = $1 \
              ORDER BY created_at DESC, id \
              LIMIT 200",
        )
        .bind(self.tenant().as_str())
        .fetch_all(self.pool())
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(ExportRow::into_export).collect())
    }
}

/// What one person is owed in one currency, and how many claims are behind it.
#[derive(Debug, Clone, Default)]
struct Owed {
    currency: String,
    expense_cents: i64,
    mileage_cents: i64,
    other_currency_claims: i64,
}

impl Owed {
    /// The totals in `currency`, with every claim in another currency counted
    /// rather than added.
    fn in_currency(this: &[Self], currency: &str) -> Self {
        let mut out = Self {
            currency: currency.to_owned(),
            ..Self::default()
        };
        for owed in this {
            if owed.currency == currency {
                out.expense_cents = out.expense_cents.saturating_add(owed.expense_cents);
                out.mileage_cents = out.mileage_cents.saturating_add(owed.mileage_cents);
            } else {
                out.other_currency_claims = out
                    .other_currency_claims
                    .saturating_add(owed.other_currency_claims);
            }
        }
        out
    }
}

/// What an absence clipped to the period costs, at the pattern in force on each
/// day and with public holidays free.
///
/// The same rule [`crate::hr_leave_balances::fold_leave_year`] charges a balance
/// by, day by day for the same reason: the pattern can change inside a request,
/// and a holiday inside approved leave costs nothing.
fn leave_minutes_in(
    employments: &[Employment],
    from: Date,
    to: Date,
    holidays: &crate::hr_holidays::TenantHolidays,
) -> i64 {
    if to < from {
        return 0;
    }
    let public = holidays.days(from, to);
    let mut minutes = 0_i64;
    let mut day = from;
    while day <= to {
        if !public.contains(&day) {
            minutes += i64::from(
                employments
                    .iter()
                    .find(|employment| employment.covers(day))
                    .map_or(0, |employment| employment.minutes_on(day))
                    .max(0),
            );
        }
        match day.next_day() {
            Some(next) => day = next,
            None => break,
        }
    }
    minutes
}

#[derive(sqlx::FromRow)]
struct ExportRow {
    id: String,
    from_day: Date,
    to_day: Date,
    mapping_key: String,
    line_count: i32,
    drawn_by: String,
    created_at: OffsetDateTime,
}

impl ExportRow {
    fn into_export(self) -> PayrollExport {
        PayrollExport {
            id: HrPayrollExportId::new(self.id),
            from_day: self.from_day,
            to_day: self.to_day,
            mapping_key: self.mapping_key,
            line_count: self.line_count,
            drawn_by: self.drawn_by,
            created_at: self.created_at,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::hr_employments::{FULL_TIME_PATTERN, PATTERN_DAYS};
    use crate::hr_holidays::TenantHolidays;
    use crate::hr_leave_math::LeaveYear;
    use crate::hr_leave_policies::NewLeavePolicy;
    use crate::id::{HrEmploymentId, HrLeavePolicyId};
    use time::Month;

    fn day(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).expect("a real date")
    }

    fn terms(started: Date, ended: Option<Date>, pattern: [i32; PATTERN_DAYS]) -> Employment {
        Employment {
            id: HrEmploymentId::new(format!("p-{started}")),
            employee_id: HrEmployeeId::new("e".to_owned()),
            job_title: "Vertrieb".to_owned(),
            team: "Sales".to_owned(),
            contract_kind: ContractKind::Permanent,
            started_on: started,
            ended_on: ended,
            pattern_minutes: pattern,
            pay_amount_cents: Some(320_000),
            pay_period: PayPeriod::Month,
            pay_currency: "EUR".to_owned(),
            created_by: "u".to_owned(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    fn policy(kind: LeaveKind, paid: bool) -> LeavePolicy {
        LeavePolicy {
            id: HrLeavePolicyId::new("pol".to_owned()),
            name: "Whatever the tenant called it".to_owned(),
            kind,
            entitlement_minutes: 0,
            accrual: NewLeavePolicy::default().accrual,
            leave_year: LeaveYear::calendar(),
            carryover_cap_minutes: 0,
            carryover_expires_after_months: None,
            allow_negative: false,
            requires_approval: true,
            paid,
            archived_at: None,
            created_by: "u".to_owned(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn the_terms_are_the_latest_ones_the_period_touched() {
        let march = (day(2026, Month::March, 1), day(2026, Month::March, 31));
        let history = vec![
            terms(
                day(2026, Month::March, 16),
                None,
                [480, 480, 480, 480, 480, 0, 0],
            ),
            terms(
                day(2025, Month::January, 1),
                Some(day(2026, Month::March, 15)),
                FULL_TIME_PATTERN,
            ),
        ];
        let picked = terms_for_period(&history, march.0, march.1).expect("a period was worked");
        assert_eq!(picked.started_on, day(2026, Month::March, 16), "the rise");
        // Somebody who starts in April is not on March's file…
        let joiner = vec![terms(day(2026, Month::April, 1), None, FULL_TIME_PATTERN)];
        assert!(terms_for_period(&joiner, march.0, march.1).is_none());
        // …and a leaver who worked one day of it is.
        let leaver = vec![terms(
            day(2025, Month::June, 1),
            Some(day(2026, Month::March, 1)),
            FULL_TIME_PATTERN,
        )];
        assert!(terms_for_period(&leaver, march.0, march.1).is_some());
        // Somebody who left before the period is not.
        let gone = vec![terms(
            day(2024, Month::June, 1),
            Some(day(2026, Month::February, 28)),
            FULL_TIME_PATTERN,
        )];
        assert!(terms_for_period(&gone, march.0, march.1).is_none());
        assert!(terms_for_period(&[], march.0, march.1).is_none());
    }

    #[test]
    fn a_policy_lands_in_a_column_by_what_it_does_to_pay_never_by_its_name() {
        assert_eq!(
            leave_bucket(&policy(LeaveKind::Annual, true)),
            LeaveBucket::Paid
        );
        assert_eq!(
            leave_bucket(&policy(LeaveKind::OtherPaid, true)),
            LeaveBucket::Paid
        );
        assert_eq!(
            leave_bucket(&policy(LeaveKind::Sick, true)),
            LeaveBucket::Sick
        );
        assert_eq!(
            leave_bucket(&policy(LeaveKind::Unpaid, false)),
            LeaveBucket::Unpaid
        );
        // Sick leave a tenant does not pay is unpaid absence on the file: the
        // column is about the money, and `paid` is the fact that decides it.
        assert_eq!(
            leave_bucket(&policy(LeaveKind::Sick, false)),
            LeaveBucket::Unpaid
        );
    }

    #[test]
    fn leave_is_charged_at_the_pattern_in_force_and_a_holiday_costs_nothing() {
        // 2026-03-02 is a Monday.
        let week = terms(day(2026, Month::January, 1), None, FULL_TIME_PATTERN);
        let history = vec![week];
        let none = TenantHolidays::none();
        // Monday to Friday, five eight-hour days.
        assert_eq!(
            leave_minutes_in(
                &history,
                day(2026, Month::March, 2),
                day(2026, Month::March, 6),
                &none
            ),
            5 * 480
        );
        // The weekend inside it costs nothing, because nobody works it.
        assert_eq!(
            leave_minutes_in(
                &history,
                day(2026, Month::March, 2),
                day(2026, Month::March, 8),
                &none
            ),
            5 * 480
        );
        // A clip that ends before it starts (an absence entirely outside the
        // period) is nothing at all, never a negative.
        assert_eq!(
            leave_minutes_in(
                &history,
                day(2026, Month::March, 6),
                day(2026, Month::March, 2),
                &none
            ),
            0
        );
        // German unity day is a Friday in 2025; on the German calendar that
        // Friday is free, so the same week costs one day less.
        let german = TenantHolidays::for_calendar("DE");
        assert_eq!(
            leave_minutes_in(
                &[terms(day(2024, Month::January, 1), None, FULL_TIME_PATTERN)],
                day(2025, Month::September, 29),
                day(2025, Month::October, 3),
                &german
            ),
            4 * 480,
            "a public holiday inside approved leave costs nothing"
        );
        // Days a change of terms does not cover cost nothing: the fold asks the
        // employment in force on each day, and there is none before the start.
        assert_eq!(
            leave_minutes_in(
                &[terms(day(2026, Month::March, 4), None, FULL_TIME_PATTERN)],
                day(2026, Month::March, 2),
                day(2026, Month::March, 6),
                &none
            ),
            3 * 480
        );
    }

    #[test]
    fn a_claim_in_another_currency_is_counted_never_added() {
        let owed = [
            Owed {
                currency: "EUR".to_owned(),
                expense_cents: 4_500,
                mileage_cents: 3_750,
                other_currency_claims: 3,
            },
            Owed {
                currency: "GBP".to_owned(),
                expense_cents: 9_900,
                mileage_cents: 0,
                other_currency_claims: 2,
            },
        ];
        let folded = Owed::in_currency(&owed, "EUR");
        assert_eq!(folded.expense_cents, 4_500);
        assert_eq!(folded.mileage_cents, 3_750);
        assert_eq!(folded.other_currency_claims, 2, "the two sterling claims");
        // Somebody paid in a currency they have claimed nothing in is owed
        // nothing, and every claim they have made is reported as left out.
        let elsewhere = Owed::in_currency(&owed, "PLN");
        assert_eq!(elsewhere.expense_cents, 0);
        assert_eq!(elsewhere.other_currency_claims, 5);
        assert_eq!(Owed::in_currency(&[], "EUR").expense_cents, 0);
    }

    #[test]
    fn a_line_adds_its_own_totals_and_never_overflows() {
        let mut line = line_of(320_000);
        line.paid_leave_minutes = 960;
        line.sick_leave_minutes = 480;
        line.unpaid_leave_minutes = 240;
        line.expense_cents = 4_500;
        line.mileage_cents = 3_750;
        assert_eq!(line.total_leave_minutes(), 1_680);
        assert_eq!(line.claims_cents(), 8_250);
        line.expense_cents = i64::MAX;
        assert_eq!(line.claims_cents(), i64::MAX, "saturating, never a panic");
    }

    /// One line, in the shape the fold produces.
    fn line_of(pay_amount_cents: i64) -> PayrollLine {
        PayrollLine {
            employee_id: HrEmployeeId::new("e".to_owned()),
            staff_number: "0042".to_owned(),
            given_name: "Ada".to_owned(),
            family_name: "Byron".to_owned(),
            full_name: "Ada Byron".to_owned(),
            national_id: "123456789".to_owned(),
            iban: "NL91ABNA0417164300".to_owned(),
            country: "NL".to_owned(),
            date_of_birth: Some(day(1990, Month::December, 10)),
            job_title: "Systeembeheerder".to_owned(),
            team: "Techniek".to_owned(),
            contract_kind: ContractKind::Permanent,
            started_on: day(2024, Month::March, 4),
            ended_on: None,
            weekly_minutes: 2_400,
            pay_amount_cents: Some(pay_amount_cents),
            pay_period: PayPeriod::Month,
            pay_currency: "EUR".to_owned(),
            paid_leave_minutes: 0,
            sick_leave_minutes: 0,
            unpaid_leave_minutes: 0,
            expense_cents: 0,
            mileage_cents: 0,
            claims_other_currency: 0,
        }
    }
}
