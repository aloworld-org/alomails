//! Employments — the terms somebody is employed on, and the fact that they
//! change while the person does not (alo HR, ADR 0035, wave B6.02a;
//! `docs/design/hr.md`, "Employees, and the employments under them").
//!
//! # Appended, never edited in place
//!
//! A promotion, a move to four days a week, a pay rise and a fixed-term renewal
//! are all changes to the *terms*. A leave balance computed last March must
//! still be explicable next March, which requires knowing the working pattern
//! that was in force **then** — so a change ends the current row and starts the
//! next one, and any date-bound computation asks
//! [`TenantStore::hr_employment_on`] which employment covered that day.
//!
//! It is the shape B1 used for the FX rate snapshot and B3 for the rate on a
//! time entry: *the figure that was true when the fact happened is stored with
//! the fact*, never re-derived from today's settings. Editing the current row
//! instead would make every historical balance unreproducible the moment
//! somebody went part-time.
//!
//! # At most one open period
//!
//! Two open employments would be two working patterns on the same day, and the
//! balance fold would have to pick one. A partial unique index says so in the
//! schema; [`TenantStore::append_hr_employment`] closes the outgoing period in
//! the **same transaction** that opens the incoming one, so a reader never sees
//! a person with none.
//!
//! # Pay is money, and money is integer cents
//!
//! Never a float, anywhere in this suite. Pay is also HR-door-only: it is on
//! [`Employment`], which no directory read returns.

use time::{Date, Duration, OffsetDateTime};

use crate::account::AccountStore;
use crate::billing_field::{bounded, currency as validate_currency, unit_price_cents};
use crate::error::{Result, StoreError};
use crate::id::{HrEmployeeId, HrEmploymentId, UserId};
use crate::store::TenantStore;

/// A job title or a team name — a label on a chart, not a paragraph.
pub const JOB_TITLE_MAX_CHARS: usize = 120;
/// The number of entries in a working pattern: Monday..Sunday, always seven,
/// because a week is seven days in every tenant.
pub const PATTERN_DAYS: usize = 7;
/// The most minutes a working pattern may state for one day. A day is 1 440
/// minutes; a pattern claiming more is a typo, and it would inflate every leave
/// balance folded from it.
pub const MINUTES_PER_DAY_MAX: i32 = 1_440;

/// The default pattern: eight hours Monday to Friday, nothing at the weekend.
/// Stated once, here, so "full time" means the same thing to the balance
/// arithmetic as it does to a form's placeholder.
pub const FULL_TIME_PATTERN: [i32; PATTERN_DAYS] = [480, 480, 480, 480, 480, 0, 0];

/// What kind of contract somebody is on.
///
/// A closed vocabulary, matched by the CHECK one layer down: a word no code
/// knows is a term nothing can compute with, and this one decides how a
/// fixed-term renewal, an apprentice's statutory entitlement and a contractor's
/// absence from the payroll export are each treated later in the wave.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContractKind {
    /// No end date by intention — the ordinary European employment contract.
    Permanent,
    /// A contract with an agreed end date; renewal appends a new employment.
    FixedTerm,
    /// Permanent, but on a pattern that is less than the tenant's full week.
    /// The *pattern* is what the arithmetic uses; this word is what people call
    /// it.
    PartTime,
    /// A training contract — apprenticeship, `Ausbildung`, `alternance`.
    Apprentice,
    /// Somebody who invoices rather than being paid: on the org chart, out of
    /// the payroll export.
    Contractor,
    /// A placement or internship, paid or not.
    Intern,
}

impl ContractKind {
    /// The stored word.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Permanent => "permanent",
            Self::FixedTerm => "fixed_term",
            Self::PartTime => "part_time",
            Self::Apprentice => "apprentice",
            Self::Contractor => "contractor",
            Self::Intern => "intern",
        }
    }

    /// Reads a contract kind — from a request body or from a stored row.
    ///
    /// # Errors
    /// [`StoreError::Validation`] naming the accepted set.
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim() {
            "permanent" => Ok(Self::Permanent),
            "fixed_term" => Ok(Self::FixedTerm),
            "part_time" => Ok(Self::PartTime),
            "apprentice" => Ok(Self::Apprentice),
            "contractor" => Ok(Self::Contractor),
            "intern" => Ok(Self::Intern),
            _ => Err(StoreError::Validation(
                "contract kind must be one of: permanent, fixed_term, part_time, apprentice, \
                 contractor, intern"
                    .to_owned(),
            )),
        }
    }
}

impl std::fmt::Display for ContractKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What the pay figure is *per*.
///
/// Stored rather than derived, because "3 200" means nothing without it and a
/// payroll export that guessed would guess wrong for every hourly employee.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayPeriod {
    /// Per hour worked.
    Hour,
    /// Per calendar month — the ordinary European salary.
    Month,
    /// Per year.
    Year,
}

impl PayPeriod {
    /// The stored word.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hour => "hour",
            Self::Month => "month",
            Self::Year => "year",
        }
    }

    /// Reads a pay period.
    ///
    /// # Errors
    /// [`StoreError::Validation`] naming the accepted set.
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim() {
            "hour" => Ok(Self::Hour),
            "month" => Ok(Self::Month),
            "year" => Ok(Self::Year),
            _ => Err(StoreError::Validation(
                "pay period must be one of: hour, month, year".to_owned(),
            )),
        }
    }
}

impl std::fmt::Display for PayPeriod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The columns every read of an employment selects, in `EmploymentRow` order.
const EMPLOYMENT_COLS: &str = "id, employee_id, job_title, team, contract_kind, started_on, \
     ended_on, pattern_minutes, pay_amount_cents, pay_period, pay_currency, created_by, \
     created_at, updated_at";

/// The writable shape of one period of employment.
#[derive(Debug, Clone)]
pub struct NewEmployment {
    /// What the job is called.
    pub job_title: String,
    /// The team or department it sits in.
    pub team: String,
    /// The kind of contract.
    pub contract_kind: ContractKind,
    /// The first day these terms apply.
    pub started_on: Date,
    /// The last day they apply, when it is already agreed (a fixed term). Left
    /// `None` for an open period, which is the ordinary case; appending the
    /// next period closes this one automatically.
    pub ended_on: Option<Date>,
    /// Minutes normally worked, Monday..Sunday. This — not the contract word —
    /// is what makes "a day off" a number of minutes.
    pub pattern_minutes: [i32; PATTERN_DAYS],
    /// Gross pay in integer cents, or `None` when the tenant does not record it
    /// here (an unpaid intern, or a contractor who invoices).
    pub pay_amount_cents: Option<i64>,
    /// What that figure is per.
    pub pay_period: PayPeriod,
    /// ISO 4217 currency the pay is in.
    pub pay_currency: String,
}

impl Default for NewEmployment {
    /// Full-time, permanent, in euro, starting on the Unix epoch — a caller
    /// always states `started_on`, and a default date that is obviously not
    /// today is better than one that silently looks right.
    fn default() -> Self {
        Self {
            job_title: String::new(),
            team: String::new(),
            contract_kind: ContractKind::Permanent,
            started_on: Date::from_ordinal_date(1970, 1).unwrap_or(Date::MIN),
            ended_on: None,
            pattern_minutes: FULL_TIME_PATTERN,
            pay_amount_cents: None,
            pay_period: PayPeriod::Month,
            pay_currency: crate::billing_field::DEFAULT_CURRENCY.to_owned(),
        }
    }
}

/// One stored period of employment.
#[derive(Debug, Clone)]
pub struct Employment {
    /// Opaque id, unique within the tenant.
    pub id: HrEmploymentId,
    /// The person these terms are for.
    pub employee_id: HrEmployeeId,
    /// What the job is called.
    pub job_title: String,
    /// The team or department.
    pub team: String,
    /// The kind of contract.
    pub contract_kind: ContractKind,
    /// The first day these terms applied.
    pub started_on: Date,
    /// The last day they applied; `None` while they are the current terms.
    pub ended_on: Option<Date>,
    /// Minutes normally worked, Monday..Sunday.
    pub pattern_minutes: [i32; PATTERN_DAYS],
    /// Private: gross pay in integer cents.
    pub pay_amount_cents: Option<i64>,
    /// What that figure is per.
    pub pay_period: PayPeriod,
    /// ISO 4217 currency, uppercase.
    pub pay_currency: String,
    /// The user who recorded these terms.
    pub created_by: String,
    /// Creation time.
    pub created_at: OffsetDateTime,
    /// Last modification time (a period is closed, never otherwise rewritten).
    pub updated_at: OffsetDateTime,
}

impl Employment {
    /// Whether these are the current terms.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.ended_on.is_none()
    }

    /// Whether these terms covered `day`.
    #[must_use]
    pub fn covers(&self, day: Date) -> bool {
        day >= self.started_on && self.ended_on.is_none_or(|end| day <= end)
    }

    /// The minutes normally worked on `day`'s weekday — the figure a leave
    /// request's cost is folded from, so it lives with the terms rather than in
    /// every caller.
    #[must_use]
    pub fn minutes_on(&self, day: Date) -> i32 {
        // `time`'s Monday-based index is the same order the array is stored in.
        let index = usize::from(day.weekday().number_days_from_monday());
        self.pattern_minutes.get(index).copied().unwrap_or(0)
    }

    /// Minutes in a normal week — what "full time" means for this person, and
    /// the denominator every pro-rata entitlement uses.
    #[must_use]
    pub fn weekly_minutes(&self) -> i32 {
        self.pattern_minutes.iter().sum()
    }
}

/// A validated, normalised employment ready to be bound into a statement.
#[derive(Debug)]
struct Normalized {
    job_title: String,
    team: String,
    started_on: Date,
    ended_on: Option<Date>,
    pattern_minutes: Vec<i32>,
    pay_amount_cents: Option<i64>,
    pay_currency: String,
}

/// Validates a working pattern: seven days, each a plausible number of minutes.
///
/// # Errors
/// [`StoreError::Validation`] naming the day that broke the rule.
pub fn working_pattern(pattern: &[i32; PATTERN_DAYS]) -> Result<Vec<i32>> {
    const DAY_NAMES: [&str; PATTERN_DAYS] = [
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
        "Sunday",
    ];
    for (index, minutes) in pattern.iter().enumerate() {
        if !(0..=MINUTES_PER_DAY_MAX).contains(minutes) {
            let day = DAY_NAMES.get(index).copied().unwrap_or("a day");
            return Err(StoreError::Validation(format!(
                "working pattern for {day} must be between 0 and {MINUTES_PER_DAY_MAX} minutes"
            )));
        }
    }
    Ok(pattern.to_vec())
}

/// Validates and normalises a whole employment. Pure — no database.
fn normalize(input: &NewEmployment) -> Result<Normalized> {
    if input.ended_on.is_some_and(|end| end < input.started_on) {
        return Err(StoreError::Validation(
            "employment end date must not be before its start date".to_owned(),
        ));
    }
    Ok(Normalized {
        job_title: bounded("job title", &input.job_title, JOB_TITLE_MAX_CHARS)?,
        team: bounded("team", &input.team, JOB_TITLE_MAX_CHARS)?,
        started_on: input.started_on,
        ended_on: input.ended_on,
        pattern_minutes: working_pattern(&input.pattern_minutes)?,
        pay_amount_cents: match input.pay_amount_cents {
            Some(cents) => Some(unit_price_cents("pay", cents)?),
            None => None,
        },
        pay_currency: validate_currency(&input.pay_currency)?,
    })
}

impl TenantStore {
    /// Starts a new period of employment, closing the current one the day
    /// before it begins — both in one transaction, so nobody is ever briefly
    /// employed on no terms or on two.
    ///
    /// The new period must start **after** the latest one began: employment
    /// history is appended in order, and back-dating a change would silently
    /// restate balances already folded from the terms it replaced. Correcting a
    /// mistyped period is a repair, not an ordinary write, and it is not
    /// something this door does.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the employee is not this tenant's;
    /// [`StoreError::Validation`] on a bad field, a pattern outside 0..=1 440
    /// minutes, or a start that does not follow the previous period;
    /// [`StoreError::Db`] on failure.
    pub async fn append_hr_employment(
        &self,
        employee: &HrEmployeeId,
        input: &NewEmployment,
        actor: &UserId,
    ) -> Result<HrEmploymentId> {
        let e = normalize(input)?;
        let mut tx = self.pool().begin().await.map_err(StoreError::Db)?;
        // Lock the person's row: two concurrent appends must not both read
        // "there is one open period" and both leave one open.
        let exists: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM hr_employees WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
        )
        .bind(self.tenant().as_str())
        .bind(employee.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        if exists.is_none() {
            return Err(StoreError::NotFound);
        }
        let latest: Option<(String, Date, Option<Date>)> = sqlx::query_as(
            "SELECT id, started_on, ended_on FROM hr_employments \
              WHERE tenant_id = $1 AND employee_id = $2 \
              ORDER BY (ended_on IS NULL) DESC, started_on DESC, id \
              LIMIT 1",
        )
        .bind(self.tenant().as_str())
        .bind(employee.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        if let Some((latest_id, latest_start, latest_end)) = latest {
            if e.started_on <= latest_start {
                return Err(StoreError::Validation(
                    "new terms must start after the period they replace".to_owned(),
                ));
            }
            match latest_end {
                // The ordinary case: close the running period the day before
                // the new one begins, so the two are contiguous and neither
                // overlaps.
                None => {
                    let closes_on = e.started_on - Duration::days(1);
                    sqlx::query(
                        "UPDATE hr_employments SET ended_on = $3, updated_at = now() \
                          WHERE tenant_id = $1 AND id = $2",
                    )
                    .bind(self.tenant().as_str())
                    .bind(&latest_id)
                    .bind(closes_on)
                    .execute(&mut *tx)
                    .await
                    .map_err(StoreError::Db)?;
                }
                Some(end) if end >= e.started_on => {
                    return Err(StoreError::Validation(
                        "new terms must start after the previous period ended".to_owned(),
                    ));
                }
                Some(_) => {}
            }
        }
        let id = HrEmploymentId::generate();
        sqlx::query(
            "INSERT INTO hr_employments (tenant_id, id, employee_id, job_title, team, \
                 contract_kind, started_on, ended_on, pattern_minutes, pay_amount_cents, \
                 pay_period, pay_currency, created_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
        )
        .bind(self.tenant().as_str())
        .bind(id.as_str())
        .bind(employee.as_str())
        .bind(&e.job_title)
        .bind(&e.team)
        .bind(input.contract_kind.as_str())
        .bind(e.started_on)
        .bind(e.ended_on)
        .bind(&e.pattern_minutes)
        .bind(e.pay_amount_cents)
        .bind(input.pay_period.as_str())
        .bind(&e.pay_currency)
        .bind(actor.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(id)
    }

    /// Ends the running period of employment — the leaver's act, and the one
    /// archiving a record performs.
    ///
    /// Idempotent in intent rather than in effect: there is either a running
    /// period to end or there is not, and ending one that already ended is a
    /// [`StoreError::NotFound`] rather than a silent re-stamp of a date payroll
    /// may already have used.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the employee is not this tenant's or has
    /// no running period; [`StoreError::Validation`] when the day is before the
    /// period began; [`StoreError::Db`] on failure.
    pub async fn end_hr_employment(&self, employee: &HrEmployeeId, on: Date) -> Result<()> {
        let open: Option<(String, Date)> = sqlx::query_as(
            "SELECT id, started_on FROM hr_employments \
              WHERE tenant_id = $1 AND employee_id = $2 AND ended_on IS NULL",
        )
        .bind(self.tenant().as_str())
        .bind(employee.as_str())
        .fetch_optional(self.pool())
        .await
        .map_err(StoreError::Db)?;
        let Some((id, started_on)) = open else {
            return Err(StoreError::NotFound);
        };
        if on < started_on {
            return Err(StoreError::Validation(
                "employment end date must not be before its start date".to_owned(),
            ));
        }
        sqlx::query(
            "UPDATE hr_employments SET ended_on = $3, updated_at = now() \
              WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant().as_str())
        .bind(&id)
        .bind(on)
        .execute(self.pool())
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }

    /// Every period of employment for one person, newest first — the HR door's
    /// history, including pay.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the employee is not this tenant's;
    /// [`StoreError::Db`] on failure.
    pub async fn hr_employments(&self, employee: &HrEmployeeId) -> Result<Vec<Employment>> {
        self.assert_tenant_employee(employee).await?;
        employments_of(self.pool(), self.tenant().as_str(), employee.as_str()).await
    }

    /// The period of employment that covered `day`, or `None` when the person
    /// was not employed then.
    ///
    /// This is the function every date-bound computation in the wave asks
    /// before it uses a working pattern or a pay figure: the terms that were in
    /// force **then**, never the ones in force now.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the employee is not this tenant's;
    /// [`StoreError::Db`] on failure.
    pub async fn hr_employment_on(
        &self,
        employee: &HrEmployeeId,
        day: Date,
    ) -> Result<Option<Employment>> {
        self.assert_tenant_employee(employee).await?;
        let row = sqlx::query_as::<_, EmploymentRow>(&format!(
            "SELECT {EMPLOYMENT_COLS} FROM hr_employments \
              WHERE tenant_id = $1 AND employee_id = $2 \
                AND started_on <= $3 AND (ended_on IS NULL OR ended_on >= $3) \
              ORDER BY started_on DESC LIMIT 1"
        ))
        .bind(self.tenant().as_str())
        .bind(employee.as_str())
        .bind(day)
        .fetch_optional(self.pool())
        .await
        .map_err(StoreError::Db)?;
        row.map(EmploymentRow::into_employment).transpose()
    }

    /// Proves an employee id is this tenant's, so a guessed id from another
    /// tenant is a [`StoreError::NotFound`] rather than a read.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when it is not; [`StoreError::Db`] on failure.
    pub(crate) async fn assert_tenant_employee(&self, employee: &HrEmployeeId) -> Result<()> {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM hr_employees WHERE tenant_id = $1 AND id = $2)",
        )
        .bind(self.tenant().as_str())
        .bind(employee.as_str())
        .fetch_one(self.pool())
        .await
        .map_err(StoreError::Db)?;
        if !exists {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }
}

impl AccountStore {
    /// **The own door.** The caller's own employment history, newest first —
    /// their contract, their pattern and their pay, which they are entitled to
    /// see about themselves and nobody else is entitled to see through here.
    ///
    /// Empty when the signed-in user has no employee record. There is no
    /// argument that could ask for a colleague's.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn my_hr_employments(&self) -> Result<Vec<Employment>> {
        let rows = sqlx::query_as::<_, EmploymentRow>(&format!(
            "SELECT {EMPLOYMENT_COLS} FROM hr_employments p \
              WHERE p.tenant_id = $1 AND p.employee_id IN ( \
                    SELECT e.id FROM hr_employees e \
                     WHERE e.tenant_id = $1 AND e.user_id = $2 \
              ) \
              ORDER BY p.started_on DESC, p.id"
        ))
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter()
            .map(EmploymentRow::into_employment)
            .collect()
    }
}

/// The history read, shared so the HR door and any later caller agree on the
/// order (newest first, ties broken by id so the order is total).
async fn employments_of(
    pool: &sqlx::PgPool,
    tenant: &str,
    employee: &str,
) -> Result<Vec<Employment>> {
    let rows = sqlx::query_as::<_, EmploymentRow>(&format!(
        "SELECT {EMPLOYMENT_COLS} FROM hr_employments \
          WHERE tenant_id = $1 AND employee_id = $2 \
          ORDER BY started_on DESC, id"
    ))
    .bind(tenant)
    .bind(employee)
    .fetch_all(pool)
    .await
    .map_err(StoreError::Db)?;
    rows.into_iter()
        .map(EmploymentRow::into_employment)
        .collect()
}

// ---- row types --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct EmploymentRow {
    id: String,
    employee_id: String,
    job_title: String,
    team: String,
    contract_kind: String,
    started_on: Date,
    ended_on: Option<Date>,
    pattern_minutes: Vec<i32>,
    pay_amount_cents: Option<i64>,
    pay_period: String,
    pay_currency: String,
    created_by: String,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl EmploymentRow {
    /// Fallible on purpose: a stored word this build does not know is a schema
    /// disagreement, and answering with a guessed contract kind would be worse
    /// than answering with an error.
    fn into_employment(self) -> Result<Employment> {
        let mut pattern = [0_i32; PATTERN_DAYS];
        if self.pattern_minutes.len() != PATTERN_DAYS {
            return Err(StoreError::Validation(format!(
                "stored working pattern must have {PATTERN_DAYS} entries"
            )));
        }
        for (slot, minutes) in pattern.iter_mut().zip(self.pattern_minutes) {
            *slot = minutes;
        }
        Ok(Employment {
            id: HrEmploymentId::new(self.id),
            employee_id: HrEmployeeId::new(self.employee_id),
            job_title: self.job_title,
            team: self.team,
            contract_kind: ContractKind::parse(&self.contract_kind)?,
            started_on: self.started_on,
            ended_on: self.ended_on,
            pattern_minutes: pattern,
            pay_amount_cents: self.pay_amount_cents,
            pay_period: PayPeriod::parse(&self.pay_period)?,
            pay_currency: self.pay_currency,
            created_by: self.created_by,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use time::Month;

    fn day(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).expect("a real date")
    }

    fn terms() -> NewEmployment {
        NewEmployment {
            job_title: "Vertrieb".to_owned(),
            started_on: day(2026, Month::January, 1),
            ..Default::default()
        }
    }

    fn invalid<T: std::fmt::Debug>(result: Result<T>) -> String {
        match result {
            Err(StoreError::Validation(msg)) => msg,
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn the_vocabularies_are_closed_and_round_trip() {
        for kind in [
            ContractKind::Permanent,
            ContractKind::FixedTerm,
            ContractKind::PartTime,
            ContractKind::Apprentice,
            ContractKind::Contractor,
            ContractKind::Intern,
        ] {
            assert_eq!(ContractKind::parse(kind.as_str()).unwrap_or(kind), kind);
        }
        for period in [PayPeriod::Hour, PayPeriod::Month, PayPeriod::Year] {
            assert_eq!(PayPeriod::parse(period.as_str()).unwrap_or(period), period);
        }
        assert!(invalid(ContractKind::parse("zero_hours")).contains("permanent"));
        assert!(invalid(PayPeriod::parse("week")).contains("month"));
    }

    #[test]
    fn a_working_pattern_is_seven_plausible_days() {
        assert_eq!(
            working_pattern(&FULL_TIME_PATTERN)
                .unwrap_or_default()
                .len(),
            PATTERN_DAYS
        );
        // A four-day week and a weekend shift are both ordinary.
        assert!(working_pattern(&[480, 480, 480, 480, 0, 0, 0]).is_ok());
        assert!(working_pattern(&[0, 0, 0, 0, 0, 300, 300]).is_ok());
        let message = invalid(working_pattern(&[480, 480, 480, 480, 480, 0, 1_441]));
        assert!(message.contains("Sunday"), "names the day: {message}");
        assert!(invalid(working_pattern(&[-1, 0, 0, 0, 0, 0, 0])).contains("Monday"));
    }

    #[test]
    fn an_end_before_a_start_is_refused() {
        let backwards = NewEmployment {
            ended_on: Some(day(2025, Month::December, 31)),
            ..terms()
        };
        assert!(invalid(normalize(&backwards)).contains("end date"));
        let same_day = NewEmployment {
            ended_on: Some(day(2026, Month::January, 1)),
            ..terms()
        };
        assert!(normalize(&same_day).is_ok(), "one day of work is a period");
    }

    #[test]
    fn pay_is_integer_cents_and_bounded() {
        let negative = NewEmployment {
            pay_amount_cents: Some(-1),
            ..terms()
        };
        assert!(invalid(normalize(&negative)).contains("pay"));
        let absurd = NewEmployment {
            pay_amount_cents: Some(i64::MAX),
            ..terms()
        };
        assert!(invalid(normalize(&absurd)).contains("pay"));
        let ordinary = NewEmployment {
            pay_amount_cents: Some(320_000),
            ..terms()
        };
        assert_eq!(
            normalize(&ordinary)
                .unwrap_or_else(|error| panic!("rejected: {error}"))
                .pay_amount_cents,
            Some(320_000)
        );
        let unpaid = NewEmployment {
            pay_amount_cents: None,
            ..terms()
        };
        assert!(normalize(&unpaid).is_ok(), "an unpaid intern is employed");
    }

    #[test]
    fn a_currency_is_a_currency() {
        let lower = NewEmployment {
            pay_currency: "pln".to_owned(),
            ..terms()
        };
        assert_eq!(
            normalize(&lower)
                .unwrap_or_else(|error| panic!("rejected: {error}"))
                .pay_currency,
            "PLN"
        );
        let bad = NewEmployment {
            pay_currency: "euro".to_owned(),
            ..terms()
        };
        assert!(invalid(normalize(&bad)).contains("currency"));
    }

    #[test]
    fn a_period_knows_which_days_it_covered_and_what_they_were_worth() {
        let mut employment = Employment {
            id: HrEmploymentId::new("p".to_owned()),
            employee_id: HrEmployeeId::new("e".to_owned()),
            job_title: "Vertrieb".to_owned(),
            team: String::new(),
            contract_kind: ContractKind::PartTime,
            started_on: day(2026, Month::January, 1),
            ended_on: None,
            // Monday, Tuesday, Wednesday — a three-day week.
            pattern_minutes: [480, 480, 480, 0, 0, 0, 0],
            pay_amount_cents: Some(200_000),
            pay_period: PayPeriod::Month,
            pay_currency: "EUR".to_owned(),
            created_by: "u".to_owned(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
        };
        assert!(employment.is_open());
        assert!(
            employment.covers(day(2030, Month::June, 1)),
            "still running"
        );
        assert!(!employment.covers(day(2025, Month::December, 31)));
        assert_eq!(employment.weekly_minutes(), 1_440);
        // 2026-08-10 is a Monday; the Thursday after it is not worked.
        assert_eq!(employment.minutes_on(day(2026, Month::August, 10)), 480);
        assert_eq!(employment.minutes_on(day(2026, Month::August, 13)), 0);
        employment.ended_on = Some(day(2026, Month::June, 30));
        assert!(!employment.is_open());
        assert!(employment.covers(day(2026, Month::June, 30)), "inclusive");
        assert!(!employment.covers(day(2026, Month::July, 1)));
    }
}
