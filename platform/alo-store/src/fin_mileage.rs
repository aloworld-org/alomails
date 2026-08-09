//! Mileage — a journey somebody drove, and the published rate that says what it
//! is worth (alo Finance, ADR 0035, wave B4.07; `docs/design/finance.md`,
//! "Expenses, receipts and mileage").
//!
//! # A claim at a rate table, not an expense with a made-up amount
//!
//! Nobody paid €37.50 for driving 125 km. They drove 125 km, and a rate the
//! company published turns that into €37.50. So this module stores the two facts
//! apart and joins them at the moment of the claim:
//!
//! - [`MileageRate`] rows are tenant-wide configuration, **effective-dated**: a
//!   per-km rate is a number a member state changes on a New Year's Day, and one
//!   row per period is what lets December's journey book at last year's rate
//!   while January's books at this year's.
//! - [`Mileage`] is the journey — the day, the distance, where from, where to
//!   and what for — and it points at an ordinary [`crate::fin_expenses`] claim
//!   that carries the money. From there the claim is **an ordinary expense**: it
//!   is submitted, approved and reimbursed by exactly the verbs a train ticket
//!   uses, and this module has no state machine of its own.
//!
//! # The three things that are decided here and nowhere else
//!
//! **The rate is snapshotted onto the journey.** [`Mileage::rate_cents_per_km`]
//! is a copy, not a reference. Correcting the table next spring must not
//! silently restate what was approved and paid out last autumn — the rule
//! [`crate::billing_fx_rates`] follows for an issued invoice's exchange rate,
//! for the same reason: a figure somebody has already been paid is history.
//!
//! **The amount is integer arithmetic, rounded once.**
//! [`allowance_cents`] is `km_milli × cents_per_km ÷ 1000`, half-up, in `i64`
//! throughout. No float touches a number a person is paid.
//!
//! **A journey with no published rate is refused, never guessed.** The table
//! ships **empty**: whether €0.30/km is the tax-free ceiling in a given member
//! state is a statement about that state's law on that date, and it is the
//! tenant's accountant who makes it, not us. A journey before the earliest row
//! is a typed [`StoreError::Validation`] naming the day — paying an allowance at
//! a rate nobody published would be money out of the door on our authority.
//!
//! # Two facts about the claim a journey creates
//!
//! It is **personal** ([`ExpenseMethod::Personal`]) and it carries **no VAT**. A
//! per-km allowance is money the employee is owed for using their own car, which
//! is what the posting rule expects (`docs/design/finance.md` § "Posting rules":
//! mileage approved credits `employee_payable`), and an allowance is not a
//! purchase, so there is no input tax on it to reclaim. Neither is a caller's
//! choice, because neither is a fact about this journey — they are what mileage
//! *is*.
//!
//! # Doors
//!
//! The **rates** are tenant-wide configuration on the account door, like
//! [`crate::fin_categories`]: everybody reads the same table. Who may *write* it
//! is an edge decision — the HTTP layer gates the replace behind
//! `Account::require_admin`, because a rate table anybody could raise is a
//! self-service pay rise.
//!
//! The **journeys** are personal data about one employee, exactly as the claims
//! they become are: every statement binds `user_id = self.user`, so a
//! colleague's journey is unrepresentable on this door rather than merely
//! refused. Places and reasons place a named person at an address on a date and
//! never reach a log.

use time::{Date, OffsetDateTime};

use crate::account::AccountStore;
use crate::billing_field::bounded;
use crate::billing_settings::base_currency_in;
use crate::error::{Result, StoreError};
use crate::fin_expenses::{
    Expense, ExpenseMethod, ExpenseRow, NewExpense, expense_cols_prefixed, insert_expense_in,
};
use crate::id::{FinCategoryId, FinExpenseId, FinMileageId, FinMileageRateId, ProjectId, UserId};

/// The smallest per-km rate that is a rate. Zero is not a cheaper allowance, it
/// is the absence of one, and a claim worth nothing is not a claim
/// ([`crate::fin_expenses::GROSS_MIN_CENTS`]).
pub const RATE_MIN_CENTS_PER_KM: i64 = 1;

/// The typo guard on a per-km rate, scaled to a per-kilometre figure: €100/km is
/// already absurd, and the ceiling is what stops a slipped decimal point from
/// becoming a five-figure allowance.
pub const RATE_MAX_CENTS_PER_KM: i64 = 10_000;

/// The shortest journey that is a journey: one thousandth of a kilometre.
pub const KM_MIN_MILLI: i64 = 1;

/// The longest journey one claim may state, in thousandths of a kilometre —
/// 100 000 km, more than twice around the planet. The same typo guard the amount
/// carries, one step earlier.
pub const KM_MAX_MILLI: i64 = 100_000_000;

/// Thousandths of a kilometre in a kilometre — the `qty_milli` convention every
/// other alo quantity uses.
const MILLI: i64 = 1_000;

/// Longest place name we keep — "München Hauptbahnhof", not an address block.
pub const PLACE_MAX: usize = 120;

/// Longest reason we keep — a sentence about why the journey happened.
pub const MILEAGE_REASON_MAX: usize = 500;

/// Most rows the rate table may hold. One rate per year for half a century is
/// more history than any tenant has; the bound exists so a replace cannot be
/// used to write an unbounded table in one request.
pub const RATES_MAX: usize = 50;

/// Most journeys one read of a person's mileage returns. A year of driving is
/// what somebody looks at; past it the caller wants paging, which this read does
/// not offer — and it is refused at the edge rather than truncated here.
const MILEAGE_LIMIT: i64 = 2_000;

/// The columns every read of a rate selects, in [`RateRow`] order.
const RATE_COLS: &str = "id, effective_from, cents_per_km, note, created_at, updated_at";

/// The journey's columns as the joined reads select them, in [`MileageRow`]
/// order.
///
/// Every read of a journey joins it to its claim, and the two tables share three
/// column names (`id`, `user_id`, `created_at`) — a result set holding two
/// columns called `id` would be read by name and quietly answer with whichever
/// came first. So the three that collide are aliased, once, here, and
/// [`MileageRow`] renames exactly those three back. The suite below holds the
/// list of colliding names and asserts it is still the whole of the overlap.
const MILEAGE_JOIN_COLS: &str = "m.id AS m_id, m.user_id AS m_user_id, m.travelled_on, \
     m.km_milli, m.rate_cents_per_km, m.from_place, m.to_place, m.reason, m.expense_id, \
     m.created_at AS m_created_at";

/// One row of the rate table, as it is written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewMileageRate {
    /// The first day this rate applies. Unique within the tenant: two rates from
    /// the same day are a coin toss over what a person is paid.
    pub effective_from: Date,
    /// What one kilometre is worth, in integer cents of the tenant's accounting
    /// currency. Between [`RATE_MIN_CENTS_PER_KM`] and [`RATE_MAX_CENTS_PER_KM`].
    pub cents_per_km: i64,
    /// Why this rate, in the tenant's own words — "BMF-Schreiben 2026", "board
    /// decision of 12 Jan". May be empty.
    pub note: String,
}

/// One stored row of the rate table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MileageRate {
    /// Opaque id, unique within the tenant. A journey copies the *value* of the
    /// rate and never this id (see the module header).
    pub id: FinMileageRateId,
    /// The first day this rate applies.
    pub effective_from: Date,
    /// What one kilometre is worth, in integer cents.
    pub cents_per_km: i64,
    /// Why this rate.
    pub note: String,
    /// When the row was written.
    pub created_at: OffsetDateTime,
    /// When it was last written.
    pub updated_at: OffsetDateTime,
}

/// A journey somebody wants to claim.
#[derive(Debug, Clone)]
pub struct NewMileage {
    /// The day it was driven, in the traveller's own zone. It picks the rate and
    /// it becomes the claim's `spent_on`.
    pub travelled_on: Date,
    /// The distance, in thousandths of a kilometre — 12.5 km is `12500`.
    pub km_milli: i64,
    /// Where from. May be empty.
    pub from_place: String,
    /// Where to. May be empty.
    pub to_place: String,
    /// What the journey was for. May be empty; it becomes the claim's
    /// description, in the traveller's own words rather than a sentence we
    /// composed in English.
    pub reason: String,
    /// The word that decides the account the allowance books to, or `None` for
    /// the chart's `expense_default`.
    pub category_id: Option<FinCategoryId>,
    /// The engagement this journey belongs to (the B3 bridge), if any.
    pub project_id: Option<ProjectId>,
}

impl NewMileage {
    /// The minimum a journey states: a day and a distance. Everything else is
    /// detail the traveller adds.
    pub fn driven(travelled_on: Date, km_milli: i64) -> Self {
        Self {
            travelled_on,
            km_milli,
            from_place: String::new(),
            to_place: String::new(),
            reason: String::new(),
            category_id: None,
            project_id: None,
        }
    }
}

/// One stored journey.
#[derive(Debug, Clone)]
pub struct Mileage {
    /// Opaque id, unique within the tenant.
    pub id: FinMileageId,
    /// Whose journey. Always the caller, on this door.
    pub user_id: UserId,
    /// The day it was driven.
    pub travelled_on: Date,
    /// The distance, in thousandths of a kilometre.
    pub km_milli: i64,
    /// The rate in force on [`Self::travelled_on`], copied when the claim was
    /// made. Never re-read from the table (see the module header).
    pub rate_cents_per_km: i64,
    /// Where from. Personal data: never logged.
    pub from_place: String,
    /// Where to. Personal data: never logged.
    pub to_place: String,
    /// What it was for. Personal data: never logged.
    pub reason: String,
    /// The claim this journey became, which carries the money and the status.
    pub expense_id: FinExpenseId,
    /// When the journey was claimed.
    pub created_at: OffsetDateTime,
}

/// A journey and the claim it became — what any read of somebody's mileage
/// answers with, because the distance without the amount and the status is half
/// a screen.
#[derive(Debug, Clone)]
pub struct MileageClaim {
    /// The journey.
    pub journey: Mileage,
    /// Its claim: the allowance, where it books, and where it is in the flow.
    pub expense: Expense,
}

/// The allowance for a journey, in integer cents: `km_milli × cents_per_km ÷
/// 1000`, rounded **half-up**.
///
/// Half-up rather than banker's rounding, and rounded once at the end rather
/// than per kilometre, because this figure is paid to a person and has to match
/// what they computed on the same two numbers. Both inputs are bounded by their
/// validators before they reach here ([`KM_MAX_MILLI`] × [`RATE_MAX_CENTS_PER_KM`]
/// is 10¹², nine orders of magnitude inside `i64`), so the multiplication is
/// checked only to make the impossible case a `None` rather than a panic.
#[must_use]
pub fn allowance_cents(km_milli: i64, cents_per_km: i64) -> Option<i64> {
    let scaled = km_milli.checked_mul(cents_per_km)?;
    // Half-up on a non-negative product; both inputs are validated positive, and
    // the `abs` reading is deliberately not written because a negative distance
    // is not a case this module has.
    scaled.checked_add(MILLI / 2).map(|rounded| rounded / MILLI)
}

/// The rate in force on `day`: the latest row whose `effective_from` is on or
/// before it, or `None` when the table starts after that day.
///
/// Pure, over the whole (small, bounded by [`RATES_MAX`]) table, so the
/// effective-dating rule is one readable function with its own tests rather than
/// an `ORDER BY … LIMIT 1` nobody can exercise without a database.
#[must_use]
pub fn rate_effective_on(rates: &[MileageRate], day: Date) -> Option<&MileageRate> {
    rates
        .iter()
        .filter(|rate| rate.effective_from <= day)
        .max_by_key(|rate| rate.effective_from)
}

/// Validates and normalises the whole rate table before any of it is written.
///
/// All or nothing, and in one pass: a replace that wrote three good rows and
/// refused the fourth would leave the tenant with a table they did not ask for
/// and did not see. The message names the row **1-based as the screen shows it**
/// and never echoes the value (law 1).
fn normalize_rates(rates: &[NewMileageRate]) -> Result<Vec<NewMileageRate>> {
    if rates.len() > RATES_MAX {
        return Err(StoreError::Validation(format!(
            "the rate table may hold at most {RATES_MAX} rates"
        )));
    }
    let mut normalized: Vec<NewMileageRate> = Vec::with_capacity(rates.len());
    for (index, rate) in rates.iter().enumerate() {
        let at = index + 1;
        if !(RATE_MIN_CENTS_PER_KM..=RATE_MAX_CENTS_PER_KM).contains(&rate.cents_per_km) {
            return Err(StoreError::Validation(format!(
                "rate {at}: the rate per kilometre must be between {RATE_MIN_CENTS_PER_KM} and \
                 {RATE_MAX_CENTS_PER_KM} cents"
            )));
        }
        if normalized
            .iter()
            .any(|seen| seen.effective_from == rate.effective_from)
        {
            return Err(StoreError::Validation(format!(
                "rate {at}: two rates cannot start on the same day"
            )));
        }
        normalized.push(NewMileageRate {
            effective_from: rate.effective_from,
            cents_per_km: rate.cents_per_km,
            note: bounded(&format!("rate {at} note"), &rate.note, RATE_NOTE_MAX)?,
        });
    }
    Ok(normalized)
}

/// Longest note a rate row carries.
pub const RATE_NOTE_MAX: usize = 200;

/// A validated, normalised journey ready to be bound into a statement. The
/// distance is checked here; the rate and therefore the amount need the table
/// and are resolved inside the transaction.
#[derive(Debug, PartialEq, Eq)]
struct NormalizedJourney {
    km_milli: i64,
    from_place: String,
    to_place: String,
    reason: String,
}

/// Validates and normalises the fields of a journey that need no database.
fn normalize_journey(input: &NewMileage) -> Result<NormalizedJourney> {
    if !(KM_MIN_MILLI..=KM_MAX_MILLI).contains(&input.km_milli) {
        return Err(StoreError::Validation(format!(
            "the distance must be between {KM_MIN_MILLI} and {KM_MAX_MILLI} thousandths of a \
             kilometre"
        )));
    }
    Ok(NormalizedJourney {
        km_milli: input.km_milli,
        from_place: bounded("from", &input.from_place, PLACE_MAX)?,
        to_place: bounded("to", &input.to_place, PLACE_MAX)?,
        reason: bounded("reason", &input.reason, MILEAGE_REASON_MAX)?,
    })
}

impl AccountStore {
    /// The tenant's per-km rate table, newest period first.
    ///
    /// Empty is the ordinary answer for a tenant who has not published a rate:
    /// this table ships empty on purpose (module header), and an empty list is
    /// what the screen turns into "publish your first rate".
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn fin_mileage_rates(&self) -> Result<Vec<MileageRate>> {
        let rows = sqlx::query_as::<_, RateRow>(&format!(
            "SELECT {RATE_COLS} FROM fin_mileage_rates WHERE tenant_id = $1 \
             ORDER BY effective_from DESC"
        ))
        .bind(self.tenant.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(RateRow::into_rate).collect())
    }

    /// Replaces the whole rate table with `rates`, in one transaction.
    ///
    /// A **replace** rather than per-row CRUD, because the table is read as a
    /// whole — "what has this company paid per kilometre, and since when" is one
    /// document, and editing it a row at a time makes an intermediate state in
    /// which a period is missing and a journey in it is refused. Replacing is
    /// safe precisely because a journey snapshots its rate: nothing already
    /// claimed changes when the table does.
    ///
    /// Validated in full before a single row is written (see
    /// [`normalize_rates`]), so a bad row leaves the table exactly as it was.
    ///
    /// # Errors
    /// [`StoreError::Validation`] naming the offending row when a rate is out of
    /// range, two rates start on the same day, a note is too long, or there are
    /// more than [`RATES_MAX`] of them; [`StoreError::Db`] on failure.
    pub async fn replace_fin_mileage_rates(
        &self,
        rates: &[NewMileageRate],
    ) -> Result<Vec<MileageRate>> {
        let rates = normalize_rates(rates)?;
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        sqlx::query("DELETE FROM fin_mileage_rates WHERE tenant_id = $1")
            .bind(self.tenant.as_str())
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        for rate in &rates {
            sqlx::query(
                "INSERT INTO fin_mileage_rates \
                     (tenant_id, id, effective_from, cents_per_km, note) \
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(self.tenant.as_str())
            .bind(FinMileageRateId::generate().as_str())
            .bind(rate.effective_from)
            .bind(rate.cents_per_km)
            .bind(&rate.note)
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        }
        tx.commit().await.map_err(StoreError::Db)?;
        self.fin_mileage_rates().await
    }

    /// Claims a journey: writes it **and** the draft expense it is worth, in one
    /// transaction.
    ///
    /// The steps, in the order they matter:
    ///
    /// 1. The distance and the strings are validated.
    /// 2. The category and the project are confirmed to be the caller's
    ///    ([`AccountStore::require_links`]) — the same rules an ordinary claim
    ///    obeys, applied here so a journey is not a way around them.
    /// 3. Inside the transaction: the rate table is read, the rate in force on
    ///    the travel day is picked ([`rate_effective_on`]) and **copied**, and
    ///    the allowance is computed ([`allowance_cents`]).
    /// 4. The claim is written as a draft in the tenant's accounting currency,
    ///    personal and VAT-free, and the journey is written pointing at it.
    ///
    /// Either both rows land or neither does: a journey whose claim did not
    /// arrive is a distance nobody can be paid for, and a claim with no journey
    /// is an amount nobody can explain.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the distance or a string breaks its rule,
    /// when no rate was published on or before the travel day, or when the
    /// allowance rounds to nothing; [`StoreError::NotFound`] when the category
    /// or the project is not one the caller can reach; [`StoreError::Db`] on
    /// failure.
    pub async fn log_mileage(&self, new: &NewMileage) -> Result<MileageClaim> {
        let j = normalize_journey(new)?;
        // The claim's own link rules, applied to the claim this is about to
        // write. A receipt is not one of them: the receipt for a journey is the
        // rate table.
        let links = NewExpense {
            category_id: new.category_id.clone(),
            project_id: new.project_id.clone(),
            ..NewExpense::spent(new.travelled_on, 1, ExpenseMethod::Personal)
        };
        self.require_links(&links).await?;

        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let rates: Vec<MileageRate> = sqlx::query_as::<_, RateRow>(&format!(
            "SELECT {RATE_COLS} FROM fin_mileage_rates WHERE tenant_id = $1"
        ))
        .bind(self.tenant.as_str())
        .fetch_all(&mut *tx)
        .await
        .map_err(StoreError::Db)?
        .into_iter()
        .map(RateRow::into_rate)
        .collect();
        let rate = rate_effective_on(&rates, new.travelled_on).ok_or_else(|| {
            StoreError::Validation(format!(
                "no mileage rate was published for {}; add one to the rate table before \
                 claiming a journey on that day",
                new.travelled_on
            ))
        })?;
        let cents_per_km = rate.cents_per_km;
        let gross_cents = allowance_cents(j.km_milli, cents_per_km).ok_or_else(|| {
            StoreError::Validation("the allowance for this journey does not fit".to_owned())
        })?;
        // A journey so short it is worth less than half a cent. Refused rather
        // than rounded up to one, because a claim of a cent is a row in every
        // report that says nothing.
        if gross_cents < crate::fin_expenses::GROSS_MIN_CENTS {
            return Err(StoreError::Validation(
                "at this rate the journey is worth less than a cent".to_owned(),
            ));
        }
        let currency = base_currency_in(&mut tx, self.tenant.as_str()).await?;

        let claim = insert_expense_in(
            &mut tx,
            self.tenant.as_str(),
            self.user.as_str(),
            &NewExpense {
                category_id: new.category_id.clone(),
                // The traveller's own words, never a sentence we composed: a
                // hardcoded "Journey from X to Y" would be English in a European
                // product, and the places are on the journey row already.
                description: j.reason.clone(),
                currency: Some(currency),
                project_id: new.project_id.clone(),
                // Mileage is what the two facts below say it is, not a choice
                // (module header): the employee's own car, and no input tax.
                ..NewExpense::spent(new.travelled_on, gross_cents, ExpenseMethod::Personal)
            },
        )
        .await?;

        let id = FinMileageId::generate();
        // Only the timestamp comes back: every other field of the journey is
        // what was just bound, and re-reading them would be asking the database
        // to repeat this function's own arguments.
        let created_at: OffsetDateTime = sqlx::query_scalar(
            "INSERT INTO fin_mileage (tenant_id, id, user_id, travelled_on, km_milli, \
                 rate_cents_per_km, from_place, to_place, reason, expense_id) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) \
             RETURNING created_at",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(self.user.as_str())
        .bind(new.travelled_on)
        .bind(j.km_milli)
        .bind(cents_per_km)
        .bind(&j.from_place)
        .bind(&j.to_place)
        .bind(&j.reason)
        .bind(claim.id.as_str())
        .fetch_one(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(MileageClaim {
            journey: Mileage {
                id,
                user_id: self.user.clone(),
                travelled_on: new.travelled_on,
                km_milli: j.km_milli,
                rate_cents_per_km: cents_per_km,
                from_place: j.from_place,
                to_place: j.to_place,
                reason: j.reason,
                expense_id: claim.id.clone(),
                created_at,
            },
            expense: claim,
        })
    }

    /// One of the caller's **own** journeys with its claim, or `None`.
    ///
    /// A colleague's journey inside the same tenant reads exactly like another
    /// tenant's and like one that never existed: absent.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure; [`StoreError::Validation`] if the claim's
    /// stored method or status is a word this build does not know.
    pub async fn fin_mileage(&self, id: &FinMileageId) -> Result<Option<MileageClaim>> {
        let row = sqlx::query_as::<_, MileageClaimRow>(&format!(
            "SELECT {journey}, {expense} FROM fin_mileage m \
             JOIN fin_expenses e ON e.tenant_id = m.tenant_id AND e.id = m.expense_id \
             WHERE m.tenant_id = $1 AND m.user_id = $2 AND m.id = $3",
            journey = MILEAGE_JOIN_COLS,
            expense = expense_cols_prefixed("e"),
        ))
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        row.map(MileageClaimRow::into_claim).transpose()
    }

    /// The caller's own journeys between `from` and `to`, both days included,
    /// newest first, each with the claim it became.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the period ends before it starts;
    /// [`StoreError::Db`] on failure.
    pub async fn fin_mileages(&self, from: Date, to: Date) -> Result<Vec<MileageClaim>> {
        if to < from {
            return Err(StoreError::Validation(
                "the end of the period must not be before its start".to_owned(),
            ));
        }
        let rows = sqlx::query_as::<_, MileageClaimRow>(&format!(
            "SELECT {journey}, {expense} FROM fin_mileage m \
             JOIN fin_expenses e ON e.tenant_id = m.tenant_id AND e.id = m.expense_id \
             WHERE m.tenant_id = $1 AND m.user_id = $2 \
               AND m.travelled_on >= $3 AND m.travelled_on <= $4 \
             ORDER BY m.travelled_on DESC, m.created_at DESC, m.id LIMIT $5",
            journey = MILEAGE_JOIN_COLS,
            expense = expense_cols_prefixed("e"),
        ))
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(from)
        .bind(to)
        .bind(MILEAGE_LIMIT)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter().map(MileageClaimRow::into_claim).collect()
    }

    /// Withdraws one of the caller's own journeys, taking its claim with it.
    ///
    /// There is no edit: a journey is a day and a distance, and correcting one
    /// is deleting it and stating the right one — which also re-reads the rate
    /// table, so a corrected journey cannot keep a rate that was picked for a
    /// day it no longer claims.
    ///
    /// Only while the claim is still the claimant's own ([`Expense::is_editable`]
    /// — draft or rejected). Once it has been handed in, an approver is looking
    /// at it, and once it is approved the company owes money on it: withdrawing
    /// the claim is the way back, and it is the *claim's* verb.
    ///
    /// The journey row is deleted by the claim's own cascade rather than by a
    /// second statement, which is also what makes `DELETE
    /// /finance/expenses/{id}` on a mileage claim leave nothing behind.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the journey is not the caller's own;
    /// [`StoreError::Conflict`] when its claim has been handed in;
    /// [`StoreError::Db`] on failure.
    pub async fn delete_fin_mileage(&self, id: &FinMileageId) -> Result<()> {
        let found = self.fin_mileage(id).await?.ok_or(StoreError::NotFound)?;
        // The claim's own rule, and its own refusal wording: "withdraw it
        // first" is the same sentence whichever door the person came through.
        self.delete_expense(&found.journey.expense_id).await
    }
}

// ---- row types --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct RateRow {
    id: String,
    effective_from: Date,
    cents_per_km: i64,
    note: String,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl RateRow {
    fn into_rate(self) -> MileageRate {
        MileageRate {
            id: FinMileageRateId::new(self.id),
            effective_from: self.effective_from,
            cents_per_km: self.cents_per_km,
            note: self.note,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

/// A journey as the joined read selects it — the three names it shares with the
/// claim renamed back off their aliases ([`MILEAGE_JOIN_COLS`]).
#[derive(sqlx::FromRow)]
struct MileageRow {
    #[sqlx(rename = "m_id")]
    id: String,
    #[sqlx(rename = "m_user_id")]
    user_id: String,
    travelled_on: Date,
    km_milli: i64,
    rate_cents_per_km: i64,
    from_place: String,
    to_place: String,
    reason: String,
    expense_id: String,
    #[sqlx(rename = "m_created_at")]
    created_at: OffsetDateTime,
}

impl MileageRow {
    fn into_mileage(self) -> Mileage {
        Mileage {
            id: FinMileageId::new(self.id),
            user_id: UserId::new(self.user_id),
            travelled_on: self.travelled_on,
            km_milli: self.km_milli,
            rate_cents_per_km: self.rate_cents_per_km,
            from_place: self.from_place,
            to_place: self.to_place,
            reason: self.reason,
            expense_id: FinExpenseId::new(self.expense_id),
            created_at: self.created_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct MileageClaimRow {
    #[sqlx(flatten)]
    journey: MileageRow,
    #[sqlx(flatten)]
    expense: ExpenseRow,
}

impl MileageClaimRow {
    fn into_claim(self) -> Result<MileageClaim> {
        Ok(MileageClaim {
            journey: self.journey.into_mileage(),
            expense: self.expense.into_expense()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fin_expenses::EXPENSE_COLS;
    use time::Month;

    /// The column names `fin_mileage` and `fin_expenses` both have, and which
    /// [`MILEAGE_JOIN_COLS`] therefore aliases.
    const COLLIDING_COLS: [&str; 3] = ["id", "user_id", "created_at"];

    fn day(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).unwrap_or(Date::MIN)
    }

    fn rate(effective_from: Date, cents_per_km: i64) -> MileageRate {
        MileageRate {
            id: FinMileageRateId::new(format!("rate-{effective_from}")),
            effective_from,
            cents_per_km,
            note: String::new(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    fn new_rate(effective_from: Date, cents_per_km: i64) -> NewMileageRate {
        NewMileageRate {
            effective_from,
            cents_per_km,
            note: String::new(),
        }
    }

    fn invalid<T: std::fmt::Debug>(result: Result<T>) -> String {
        match result {
            Err(StoreError::Validation(msg)) => msg,
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn the_allowance_is_distance_times_rate_rounded_half_up() {
        // 125 km at €0.30 — the worked example in the module header.
        assert_eq!(allowance_cents(125_000, 30), Some(3750));
        // One kilometre at thirty cents is thirty cents.
        assert_eq!(allowance_cents(1_000, 30), Some(30));
        // 12.5 km at 30 c = 375 c exactly: the milli scale exists so this is not
        // a float.
        assert_eq!(allowance_cents(12_500, 30), Some(375));
        // Half a cent rounds up, just under it rounds down — one rounding, at
        // the end.
        assert_eq!(allowance_cents(500, 1), Some(1), "0.5 c → 1 c");
        assert_eq!(allowance_cents(499, 1), Some(0), "0.499 c → 0 c");
        assert_eq!(allowance_cents(1_500, 1), Some(2), "1.5 c → 2 c");
        // The bounds multiply to a tenth of a percent of i64's range.
        assert_eq!(
            allowance_cents(KM_MAX_MILLI, RATE_MAX_CENTS_PER_KM),
            Some(1_000_000_000),
            "the ceiling of a journey is exactly the ceiling of an amount"
        );
        // The impossible case is a None, never a panic.
        assert_eq!(allowance_cents(i64::MAX, 2), None);
    }

    #[test]
    fn the_rate_in_force_is_the_latest_one_that_had_started() {
        let table = vec![
            rate(day(2025, Month::January, 1), 30),
            rate(day(2026, Month::January, 1), 38),
            rate(day(2024, Month::July, 1), 25),
        ];
        // Order in the slice is not the answer; the dates are.
        assert_eq!(
            rate_effective_on(&table, day(2026, Month::March, 14)).map(|r| r.cents_per_km),
            Some(38)
        );
        assert_eq!(
            rate_effective_on(&table, day(2025, Month::December, 31)).map(|r| r.cents_per_km),
            Some(30),
            "December's journey books at last year's rate"
        );
        assert_eq!(
            rate_effective_on(&table, day(2026, Month::January, 1)).map(|r| r.cents_per_km),
            Some(38),
            "the effective day itself is inside the period"
        );
        // Before the table starts there is no rate, and no rate means refused
        // rather than the oldest one reached back for.
        assert_eq!(rate_effective_on(&table, day(2024, Month::June, 30)), None);
        assert_eq!(rate_effective_on(&[], day(2026, Month::March, 14)), None);
    }

    #[test]
    fn a_rate_table_is_validated_whole_and_names_the_row_that_failed() {
        let january = day(2026, Month::January, 1);
        let july = day(2026, Month::July, 1);
        let good = normalize_rates(&[new_rate(january, 30), new_rate(july, 38)])
            .unwrap_or_else(|e| panic!("rejected: {e}"));
        assert_eq!(good.len(), 2);
        assert_eq!(good[1].cents_per_km, 38);
        // An empty table is legal: it is what "we do not pay mileage" looks
        // like, and it is what every tenant starts with.
        assert!(
            normalize_rates(&[])
                .unwrap_or_else(|e| panic!("rejected: {e}"))
                .is_empty()
        );

        for bad in [0, -1, RATE_MAX_CENTS_PER_KM + 1] {
            let msg = invalid(normalize_rates(&[
                new_rate(january, 30),
                new_rate(july, bad),
            ]));
            assert!(msg.contains("rate 2"), "{msg}");
            assert!(msg.contains("per kilometre"), "{msg}");
        }
        let msg = invalid(normalize_rates(&[
            new_rate(january, 30),
            new_rate(january, 38),
        ]));
        assert!(msg.contains("rate 2"), "{msg}");
        assert!(msg.contains("same day"), "{msg}");

        let msg = invalid(normalize_rates(&[NewMileageRate {
            note: "x".repeat(RATE_NOTE_MAX + 1),
            ..new_rate(january, 30)
        }]));
        assert!(msg.contains("rate 1 note"), "{msg}");

        let table: Vec<NewMileageRate> = (0..=RATES_MAX)
            .map(|index| new_rate(january.next_day().unwrap_or(january), 30 + index as i64))
            .collect();
        let msg = invalid(normalize_rates(&table));
        assert!(msg.contains("at most"), "{msg}");
    }

    #[test]
    fn a_journey_is_a_real_distance_and_bounded_strings() {
        let j = normalize_journey(&NewMileage {
            from_place: "  Berlin  ".to_owned(),
            to_place: "München".to_owned(),
            reason: "Kundentermin".to_owned(),
            ..NewMileage::driven(day(2026, Month::March, 14), 125_000)
        })
        .unwrap_or_else(|e| panic!("rejected: {e}"));
        assert_eq!(j.km_milli, 125_000);
        assert_eq!(j.from_place, "Berlin", "trimmed");
        assert_eq!(j.reason, "Kundentermin");

        for bad in [0, -1, KM_MAX_MILLI + 1] {
            let msg = invalid(normalize_journey(&NewMileage::driven(
                day(2026, Month::March, 14),
                bad,
            )));
            assert!(msg.contains("distance must be between"), "{msg}");
        }
        assert!(
            normalize_journey(&NewMileage::driven(
                day(2026, Month::March, 14),
                KM_MAX_MILLI
            ))
            .is_ok(),
            "exactly at the bound is fine"
        );
        let msg = invalid(normalize_journey(&NewMileage {
            to_place: "x".repeat(PLACE_MAX + 1),
            ..NewMileage::driven(day(2026, Month::March, 14), 1_000)
        }));
        assert!(msg.contains("to"), "{msg}");
        let msg = invalid(normalize_journey(&NewMileage {
            reason: "x".repeat(MILEAGE_REASON_MAX + 1),
            ..NewMileage::driven(day(2026, Month::March, 14), 1_000)
        }));
        assert!(msg.contains("reason"), "{msg}");
        // Every string may be empty: a journey with no reason typed is still a
        // journey somebody drove.
        assert!(normalize_journey(&NewMileage::driven(day(2026, Month::March, 14), 1_000)).is_ok());
    }

    /// The bare column names a select list yields, in order — `m.x AS y` reads
    /// back as `y`, `m.x` as `x`, `e.x` as `x`.
    fn selected_names(select: &str) -> Vec<String> {
        select
            .split(',')
            .map(|column| {
                let column = column.trim();
                let last = column
                    .rsplit_once(" AS ")
                    .map_or(column, |(_, alias)| alias.trim());
                last.rsplit_once('.')
                    .map_or(last, |(_, name)| name)
                    .to_owned()
            })
            .collect()
    }

    #[test]
    fn the_joined_read_gives_every_column_a_name_of_its_own() {
        let journey = selected_names(MILEAGE_JOIN_COLS);
        let claim = selected_names(&expense_cols_prefixed("e"));
        assert_eq!(
            journey.len(),
            MILEAGE_JOIN_COLS.split(',').count(),
            "no column is dropped by the aliasing"
        );
        // The defect this guards: two result columns called `id`, read by name,
        // answering with whichever the driver saw first.
        for name in &journey {
            assert!(
                !claim.contains(name),
                "{name} is selected twice by the joined read; alias it in MILEAGE_JOIN_COLS \
                 and rename it on MileageRow"
            );
        }
        // …and the three that would have collided are exactly the ones the
        // constant says it aliases, both tables really having them.
        for shared in COLLIDING_COLS {
            assert!(
                claim.iter().any(|name| name == shared),
                "{shared} should be a column the claim has"
            );
            assert!(
                MILEAGE_JOIN_COLS.contains(&format!("m.{shared} AS m_{shared}")),
                "{shared} should be aliased by the journey's select list"
            );
        }
        assert!(
            EXPENSE_COLS.contains("id"),
            "the claim's column list is the one being joined against"
        );
    }
}
