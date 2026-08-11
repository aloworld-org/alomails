//! The arithmetic of leave: entitlement, accrual, the cost of a request and the
//! balance that falls out of them (alo HR, ADR 0035, wave B6.03a;
//! `docs/design/hr.md`, "Leave").
//!
//! A **pure** module: no database, no clock, no tenant — the same shape
//! [`crate::billing_totals`], [`crate::time_hours`] and [`crate::fin_rules`]
//! have, and for the same reason. Arithmetic that money or a statutory
//! entitlement depends on must be testable without a fixture, and a person who
//! disagrees with their balance must be able to be shown the working.
//!
//! # Minutes, and the working pattern that makes a day mean something
//!
//! Everything here is integer minutes. Days are the unit humans speak, but a day
//! is not a fixed quantity: it is whatever that person normally works on that
//! weekday, which is why the pattern comes from the employment **in force on the
//! day in question** ([`crate::hr_employments::Employment::minutes_on`]) and why
//! a mid-year move from five days to four does not restate last spring's
//! balance. Turning minutes into the "1.5 days" a screen shows is
//! [`days_tenths`], and it is display only — no balance is ever folded from it.
//!
//! Not one quantity in this module is a fractional type, and a test asserts that
//! of the source itself.
//!
//! # Where the remainders go
//!
//! Integer division loses remainders, and twelve twelfths of 12 500 minutes is
//! not 12 500 if each twelfth is rounded on its own. Both places where a whole
//! is cut into parts therefore compute **cumulatively** — part *n* is
//! `whole × n / parts − whole × (n−1) / parts` — so the parts sum to the whole
//! exactly:
//!
//! - [`monthly_grant_minutes`], so the twelve monthly accruals of a leave year
//!   add up to the year's entitlement;
//! - [`prorated_entitlement_minutes`], so a joiner and a leaver whose
//!   employments partition a year get two entitlements that add up to one full
//!   year's — the arithmetic somebody's final payslip depends on.
//!
//! Both properties are asserted over thousands of generated cases in this
//! module's tests, not argued about here.

use time::{Date, Month};

use crate::billing_totals::{div_round_half_away, to_i64};
use crate::error::{Result, StoreError};
use crate::hr_employments::{MINUTES_PER_DAY_MAX, PATTERN_DAYS};

/// Months in a leave year. Not a configurable: a leave year is a year.
pub const MONTHS_PER_YEAR: i64 = 12;

/// The latest day-of-month a leave year may start on.
///
/// 29 February is a date three years in four cannot construct, and a balance
/// fold must never guess a date. Every national leave-year start alo has met
/// (1 January, 1 April, 1 May, 1 July, 1 October) is inside this bound.
pub const LEAVE_YEAR_START_DAY_MAX: u8 = 28;

/// The most minutes a policy may grant for one leave year: 366 days of them.
/// More than that is a typo, and it would inflate every balance folded from it.
pub const ENTITLEMENT_MAX_MINUTES: i64 = 366 * MINUTES_PER_DAY_MAX as i64;

/// The longest range one leave request may cover. A year and a day — long enough
/// for any real absence, short enough that the day-by-day fold below is bounded.
pub const REQUEST_MAX_DAYS: i64 = 366;

/// How an entitlement arrives over a leave year.
///
/// A closed vocabulary matched by the CHECK one layer down, and the thing the
/// accrual arithmetic dispatches on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accrual {
    /// The whole entitlement is granted on the leave year's first day. Common
    /// where leave is a contractual allowance rather than something earned.
    UpFront,
    /// A twelfth is granted at each month start, remainder carried so the twelve
    /// grants sum exactly to the year ([`monthly_grant_minutes`]).
    Monthly,
}

impl Accrual {
    /// The stored word.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UpFront => "up_front",
            Self::Monthly => "monthly",
        }
    }

    /// Reads an accrual — from a request body or from a stored row.
    ///
    /// # Errors
    /// [`StoreError::Validation`] naming the accepted set.
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim() {
            "up_front" => Ok(Self::UpFront),
            "monthly" => Ok(Self::Monthly),
            _ => Err(StoreError::Validation(
                "accrual must be one of: up_front, monthly".to_owned(),
            )),
        }
    }
}

impl std::fmt::Display for Accrual {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// When a policy's leave year begins — a month and a day-of-month, so it lands
/// on the same calendar day every year.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeaveYear {
    month: Month,
    day: u8,
}

impl LeaveYear {
    /// The calendar year: 1 January. The default every tenant starts on.
    #[must_use]
    pub fn calendar() -> Self {
        Self {
            month: Month::January,
            day: 1,
        }
    }

    /// A leave year starting on `month`/`day`.
    ///
    /// # Errors
    /// [`StoreError::Validation`] naming the rule when the month is not 1..=12
    /// or the day is not 1..=[`LEAVE_YEAR_START_DAY_MAX`] (see the constant for
    /// why the bound is 28 rather than 31).
    pub fn new(month: u8, day: u8) -> Result<Self> {
        let month = Month::try_from(month).map_err(|_| {
            StoreError::Validation("leave year start month must be between 1 and 12".to_owned())
        })?;
        if !(1..=LEAVE_YEAR_START_DAY_MAX).contains(&day) {
            return Err(StoreError::Validation(format!(
                "leave year start day must be between 1 and {LEAVE_YEAR_START_DAY_MAX}, so the \
                 leave year begins on a day that exists in every year"
            )));
        }
        Ok(Self { month, day })
    }

    /// The stored month, 1..=12.
    #[must_use]
    pub fn month(self) -> u8 {
        u8::from(self.month)
    }

    /// The stored day-of-month, 1..=28.
    #[must_use]
    pub fn day(self) -> u8 {
        self.day
    }

    /// The first day of the leave year that contains `day`.
    ///
    /// Total: the day-of-month bound guarantees the date exists in every year,
    /// so there is no failure case to report.
    #[must_use]
    pub fn first_day(self, day: Date) -> Date {
        let candidate = self.on(day.year());
        if candidate <= day {
            candidate
        } else {
            self.on(day.year() - 1)
        }
    }

    /// The leave year containing `day`, as its first and **inclusive** last day.
    #[must_use]
    pub fn window(self, day: Date) -> (Date, Date) {
        let first = self.first_day(day);
        let next = self.on(first.year() + 1);
        (first, next.previous_day().unwrap_or(next))
    }

    /// This leave year's start in a given calendar year.
    fn on(self, year: i32) -> Date {
        // Constructible for every year: month is a real month and day <= 28.
        Date::from_calendar_date(year, self.month, self.day).unwrap_or(Date::MIN)
    }
}

impl Default for LeaveYear {
    fn default() -> Self {
        Self::calendar()
    }
}

/// The minutes a full-time week is worth, from the pattern that defines it.
///
/// The denominator of every pro-rata entitlement: somebody on 1 440 minutes a
/// week against a 2 400-minute full week gets three-fifths of the policy's
/// figure.
#[must_use]
pub fn weekly_minutes(pattern: &[i32; PATTERN_DAYS]) -> i32 {
    pattern.iter().sum()
}

/// The minutes in this person's average working day — the divisor a screen uses
/// to say "1.5 days", and nothing else.
///
/// Days that are not worked at all are excluded, because dividing a week by
/// seven would price a four-day week's day off at four-sevenths of a day and
/// nobody would recognise the number. An empty pattern (a person who works no
/// day) averages zero, and [`days_tenths`] answers zero rather than dividing.
#[must_use]
pub fn average_working_day_minutes(pattern: &[i32; PATTERN_DAYS]) -> i32 {
    let worked: Vec<i32> = pattern.iter().copied().filter(|m| *m > 0).collect();
    let days = i128::try_from(worked.len()).unwrap_or(0);
    if days == 0 {
        return 0;
    }
    let total: i128 = worked.iter().map(|m| i128::from(*m)).sum();
    i32::try_from(div_round_half_away(total, days)).unwrap_or(MINUTES_PER_DAY_MAX)
}

/// Minutes as tenths of a day — **display only**.
///
/// A balance is minutes; this is how a screen writes it down. `4 800` minutes on
/// an eight-hour average day is `100` — ten days — and the screen renders one
/// decimal from it. Never fold a balance from this figure: the rounding here is
/// for a human's eye, and the arithmetic that decides whether a request fits
/// uses the minutes.
#[must_use]
pub fn days_tenths(minutes: i64, average_day_minutes: i32) -> i64 {
    if average_day_minutes <= 0 {
        return 0;
    }
    to_i64(div_round_half_away(
        i128::from(minutes) * 10,
        i128::from(average_day_minutes),
    ))
}

/// The policy's full-year figure scaled to one person's working pattern.
///
/// `full_year_minutes` is stated at a full-time pattern (that is what a policy
/// carries), so somebody on a three-day week gets three-fifths of it. Rounded
/// half away from zero, once — the convention the whole suite uses.
///
/// A `full_time_weekly_minutes` of zero (a tenant who has stated no full week)
/// scales nothing and returns the policy figure unchanged, which is the only
/// answer that is not a division by zero or a silent nought.
#[must_use]
pub fn scaled_entitlement_minutes(
    full_year_minutes: i64,
    weekly_minutes: i32,
    full_time_weekly_minutes: i32,
) -> i64 {
    if full_time_weekly_minutes <= 0 {
        return full_year_minutes;
    }
    if weekly_minutes <= 0 {
        return 0;
    }
    to_i64(div_round_half_away(
        i128::from(full_year_minutes) * i128::from(weekly_minutes),
        i128::from(full_time_weekly_minutes),
    ))
}

/// `entitlement` pro-rated by the days somebody was employed inside one leave
/// year.
///
/// `year` is the leave year as [`LeaveYear::window`] gives it (first day, last
/// day inclusive). `employed_from`/`employed_to` are the employment's own
/// bounds; `None` means "not bounded on that side", i.e. employed for the whole
/// of the year on that side.
///
/// Cumulative by construction, so **a joiner and a leaver whose employments
/// partition the year get two figures that sum to exactly one year's** — the
/// property a final payslip depends on, asserted over generated cases in the
/// tests. Somebody employed for none of the year gets zero, never a negative.
#[must_use]
pub fn prorated_entitlement_minutes(
    entitlement: i64,
    year: (Date, Date),
    employed_from: Option<Date>,
    employed_to: Option<Date>,
) -> i64 {
    let (first, last) = year;
    if last < first || entitlement <= 0 {
        return 0;
    }
    let total_days = (last - first).whole_days() + 1;
    let from = employed_from.map_or(first, |d| d.max(first));
    let to = employed_to.map_or(last, |d| d.min(last));
    if from > to {
        return 0;
    }
    let elapsed_before = (from - first).whole_days();
    let elapsed_through = (to - first).whole_days() + 1;
    share(entitlement, elapsed_through, total_days) - share(entitlement, elapsed_before, total_days)
}

/// `month`'s grant under [`Accrual::Monthly`]: a twelfth of the entitlement,
/// with the remainder carried into the following months.
///
/// `month` is 1..=12 counted from the leave year's first day, not a calendar
/// month; anything outside that grants nothing. The twelve grants sum to
/// `entitlement` **exactly** — that is the whole point of computing them
/// cumulatively rather than rounding a twelfth twelve times.
#[must_use]
pub fn monthly_grant_minutes(entitlement: i64, month: i64) -> i64 {
    if !(1..=MONTHS_PER_YEAR).contains(&month) || entitlement <= 0 {
        return 0;
    }
    share(entitlement, month, MONTHS_PER_YEAR) - share(entitlement, month - 1, MONTHS_PER_YEAR)
}

/// How much of `entitlement` has arrived by `as_of`.
///
/// `year_first_day` is the first day of the leave year being asked about (from
/// [`LeaveYear::window`]). `as_of` before it accrues nothing; `as_of` after the
/// year has run accrues all of it, so asking about a past year gives the whole
/// figure and no caller needs to special-case history.
///
/// [`Accrual::Monthly`] grants at each **month start**, the first of them being
/// the leave year's own first day: somebody in their first month has a twelfth,
/// not nothing.
#[must_use]
pub fn accrued_minutes(
    entitlement: i64,
    accrual: Accrual,
    year_first_day: Date,
    as_of: Date,
) -> i64 {
    if as_of < year_first_day || entitlement <= 0 {
        return 0;
    }
    match accrual {
        Accrual::UpFront => entitlement,
        Accrual::Monthly => share(
            entitlement,
            months_granted(year_first_day, as_of),
            MONTHS_PER_YEAR,
        ),
    }
}

/// The number of monthly grants made by `as_of`, 1..=12.
///
/// Counted from `year_first_day`, whose own day-of-month is the monthly
/// anniversary: a leave year starting on the 6th grants again on the 6th.
fn months_granted(year_first_day: Date, as_of: Date) -> i64 {
    let years = i64::from(as_of.year() - year_first_day.year());
    let months = i64::from(u8::from(as_of.month())) - i64::from(u8::from(year_first_day.month()));
    let mut elapsed = years * MONTHS_PER_YEAR + months;
    if as_of.day() < year_first_day.day() {
        elapsed -= 1;
    }
    // The first grant is on the leave year's own first day.
    (elapsed + 1).clamp(1, MONTHS_PER_YEAR)
}

/// The day carried-over leave lapses on, or `None` when the policy does not
/// expire it.
///
/// Counted in whole calendar months from the new leave year's first day, so a
/// 15-month rule on a 1 January year lapses on 1 April of the following year.
/// The day of the month is kept; a shorter target month clamps to its last day.
#[must_use]
pub fn carryover_expires_on(year_first_day: Date, after_months: Option<i32>) -> Option<Date> {
    after_months
        .filter(|months| *months > 0)
        .map(|months| add_months(year_first_day, i64::from(months)))
}

/// What somebody actually carries into a leave year: last year's remainder,
/// capped by the policy and dropped once it has lapsed.
///
/// A negative remainder carries nothing rather than a debt: a balance that went
/// below zero was allowed by `allow_negative` on the year it happened, and
/// pulling it into the next year would charge somebody twice for one absence.
#[must_use]
pub fn carried_in_minutes(
    previous_remaining: i64,
    cap_minutes: i64,
    expires_on: Option<Date>,
    as_of: Date,
) -> i64 {
    if cap_minutes <= 0 || expires_on.is_some_and(|expiry| as_of >= expiry) {
        return 0;
    }
    previous_remaining.clamp(0, cap_minutes)
}

/// One day inside a requested range, as the caller resolved it: what the person
/// normally works then, and whether anything already covers the day.
///
/// The caller resolves each day rather than passing one pattern for the range,
/// because the employment in force can change inside a long request and the
/// holiday calendar is per employment. That is also what makes the fold below
/// pure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestedDay {
    /// The calendar day.
    pub day: Date,
    /// Minutes normally worked on it, from the employment in force that day.
    pub pattern_minutes: i32,
    /// A public holiday on that employment's calendar. Costs nothing — the one
    /// reason the holiday tables (B6.04) are in scope at all.
    pub holiday: bool,
    /// Already covered by another approved request. Costs nothing here because
    /// it was already charged there; an overlapping request is refused, and the
    /// figure exists so the refusal can say by how much.
    pub already_covered: bool,
}

impl RequestedDay {
    /// What this day would cost if nothing covered it: the pattern's minutes,
    /// never negative.
    #[must_use]
    pub fn full_minutes(&self) -> i64 {
        i64::from(self.pattern_minutes.max(0))
    }

    /// What this day actually consumes.
    #[must_use]
    pub fn cost_minutes(&self) -> i64 {
        if self.holiday || self.already_covered {
            0
        } else {
            self.full_minutes()
        }
    }
}

/// What a request costs, and the working behind the figure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RequestCost {
    /// The minutes the request consumes from the balance.
    pub minutes: i64,
    /// How many days cost anything — the "3 days" a screen shows beside the
    /// dates, before it converts the minutes.
    pub working_days: u32,
    /// Minutes a public holiday inside the range saved.
    pub holiday_minutes: i64,
    /// Minutes already covered by an approved request. Non-zero means the
    /// request overlaps, and by this much.
    pub overlap_minutes: i64,
}

/// Folds resolved days into what the request costs.
///
/// **Additive by construction**: the cost of a range equals the sum of the costs
/// of its single-day sub-ranges, so a week booked at once and five days booked
/// separately cost the same. It is the one arithmetic surprise employees
/// actually notice, and the tests assert it over generated ranges.
#[must_use]
pub fn request_cost(days: &[RequestedDay]) -> RequestCost {
    let mut cost = RequestCost::default();
    for day in days {
        let full = day.full_minutes();
        if day.holiday {
            cost.holiday_minutes += full;
            continue;
        }
        if day.already_covered {
            cost.overlap_minutes += full;
            continue;
        }
        if full > 0 {
            cost.minutes += full;
            cost.working_days += 1;
        }
    }
    cost
}

/// Resolves the days of a request against one working pattern, a set of public
/// holidays and the days approved leave already covers.
///
/// The ordinary case — one employment, one calendar — expressed once so callers
/// do not each walk a date range. A request spanning a change of terms resolves
/// its days itself and calls [`request_cost`] directly.
///
/// # Errors
/// [`StoreError::Validation`] when the range ends before it starts or is longer
/// than [`REQUEST_MAX_DAYS`], each naming the rule.
pub fn requested_days(
    from: Date,
    to: Date,
    pattern: &[i32; PATTERN_DAYS],
    holidays: &[Date],
    already_covered: &[Date],
) -> Result<Vec<RequestedDay>> {
    if to < from {
        return Err(StoreError::Validation(
            "leave must end on or after the day it starts".to_owned(),
        ));
    }
    let length = (to - from).whole_days() + 1;
    if length > REQUEST_MAX_DAYS {
        return Err(StoreError::Validation(format!(
            "leave must not cover more than {REQUEST_MAX_DAYS} days in one request"
        )));
    }
    let mut days = Vec::with_capacity(usize::try_from(length).unwrap_or(0));
    let mut day = from;
    while day <= to {
        let index = usize::from(day.weekday().number_days_from_monday());
        days.push(RequestedDay {
            day,
            pattern_minutes: pattern.get(index).copied().unwrap_or(0),
            holiday: holidays.contains(&day),
            already_covered: already_covered.contains(&day),
        });
        // A range bounded above cannot walk off the calendar; the guard keeps
        // the loop total rather than relying on that.
        match day.next_day() {
            Some(next) => day = next,
            None => break,
        }
    }
    Ok(days)
}

/// Everything a balance is folded from, as the caller read it out of the store.
///
/// Deliberately not computed here: this module has no database. The store's
/// balance query (B6.03b) fills these in and calls [`balance`], so the figure a
/// screen shows and the figure an approval is checked against are the same fold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LeaveLedger {
    /// The person's entitlement for this leave year, already scaled and
    /// pro-rated ([`prorated_entitlement_minutes`]).
    pub entitlement_minutes: i64,
    /// What they carried in from last year ([`carried_in_minutes`]).
    pub carried_in_minutes: i64,
    /// How much of the entitlement has arrived by the date asked about
    /// ([`accrued_minutes`]).
    pub accrued_minutes: i64,
    /// Approved leave whose days have passed.
    pub taken_minutes: i64,
    /// Approved leave still ahead.
    pub booked_minutes: i64,
    /// Requests nobody has decided. Consume nothing — they are reported so a
    /// manager approving the second of two overlapping requests is told what the
    /// first already costs.
    pub pending_minutes: i64,
}

/// A leave balance, with the working that produced it.
///
/// Every field is minutes, and the identity `remaining = carried_in + accrued −
/// taken − booked` holds exactly — `pending` is beside the balance, never
/// inside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Balance {
    /// The entitlement this leave year.
    pub entitlement_minutes: i64,
    /// Carried in from last year.
    pub carried_in_minutes: i64,
    /// Accrued so far.
    pub accrued_minutes: i64,
    /// Approved and in the past.
    pub taken_minutes: i64,
    /// Approved and still ahead.
    pub booked_minutes: i64,
    /// Awaiting a decision — reported, not deducted.
    pub pending_minutes: i64,
    /// What is left: `carried_in + accrued − taken − booked`. Negative where the
    /// policy allowed it.
    pub remaining_minutes: i64,
}

/// Folds a ledger into a balance. The one place the identity is written down.
#[must_use]
pub fn balance(ledger: &LeaveLedger) -> Balance {
    let remaining = i128::from(ledger.carried_in_minutes) + i128::from(ledger.accrued_minutes)
        - i128::from(ledger.taken_minutes)
        - i128::from(ledger.booked_minutes);
    Balance {
        entitlement_minutes: ledger.entitlement_minutes,
        carried_in_minutes: ledger.carried_in_minutes,
        accrued_minutes: ledger.accrued_minutes,
        taken_minutes: ledger.taken_minutes,
        booked_minutes: ledger.booked_minutes,
        pending_minutes: ledger.pending_minutes,
        remaining_minutes: to_i64(remaining),
    }
}

/// `whole × parts_elapsed / parts`, exactly, in `i128`.
///
/// The cumulative form is what makes the parts sum to the whole: subtracting two
/// shares gives one part, and the remainder each division dropped is carried by
/// the next one rather than lost.
fn share(whole: i64, parts_elapsed: i64, parts: i64) -> i64 {
    if parts <= 0 {
        return 0;
    }
    let elapsed = parts_elapsed.clamp(0, parts);
    to_i64(i128::from(whole) * i128::from(elapsed) / i128::from(parts))
}

/// `day` plus `months` whole calendar months, clamping into a shorter month.
///
/// `time` has no month arithmetic, and the naive "add 30 days" would drift a
/// 15-month carryover rule by a fortnight.
fn add_months(day: Date, months: i64) -> Date {
    let total =
        i64::from(day.year()) * MONTHS_PER_YEAR + i64::from(u8::from(day.month())) - 1 + months;
    let year = i32::try_from(total.div_euclid(MONTHS_PER_YEAR)).unwrap_or(day.year());
    let month_index = u8::try_from(total.rem_euclid(MONTHS_PER_YEAR) + 1).unwrap_or(1);
    let month = Month::try_from(month_index).unwrap_or(Month::January);
    let last = time::util::days_in_month(month, year);
    Date::from_calendar_date(year, month, day.day().min(last)).unwrap_or(day)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::hr_employments::FULL_TIME_PATTERN;
    use time::Duration;

    /// A whole day, in minutes — the ceiling one day of a pattern may state.
    const MINUTES_IN_A_DAY: i64 = MINUTES_PER_DAY_MAX as i64;

    /// One week, in days.
    const DAYS_IN_A_WEEK: i64 = PATTERN_DAYS as i64;

    /// This module's own source, so the no-fractional-type property below is
    /// asserted about the code rather than promised in a comment.
    const SOURCE: &str = include_str!("hr_leave_math.rs");

    /// A tiny deterministic generator: thousands of cases, no dependency, and a
    /// failure that reproduces. xorshift64*, seeded per test — the same one
    /// `billing_totals` property-tests the money with.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            self.0 = x;
            x.wrapping_mul(0x2545_F491_4F6C_DD1D)
        }

        /// A value in `0..=max`.
        fn upto(&mut self, max: i64) -> i64 {
            if max <= 0 {
                0
            } else {
                i64::try_from(self.next() % u64::try_from(max + 1).unwrap_or(1)).unwrap_or(0)
            }
        }

        /// An entitlement in minutes, biased towards the awkward figures: 25
        /// days of a 7h36 day, a 30-hour contract's year, a single hour.
        fn entitlement(&mut self) -> i64 {
            self.upto(ENTITLEMENT_MAX_MINUTES)
        }

        /// A real date in a span of years wide enough to include leap years.
        fn date(&mut self) -> Date {
            let year = 2020 + i32::try_from(self.upto(12)).unwrap_or(0);
            let ordinal = u16::try_from(self.upto(364) + 1).unwrap_or(1);
            Date::from_ordinal_date(year, ordinal).expect("a real date")
        }

        /// A working pattern: sometimes full time, often not, occasionally
        /// weekends only.
        fn pattern(&mut self) -> [i32; PATTERN_DAYS] {
            let mut pattern = [0_i32; PATTERN_DAYS];
            for slot in &mut pattern {
                *slot = if self.upto(2) == 0 {
                    0
                } else {
                    i32::try_from(self.upto(MINUTES_IN_A_DAY)).unwrap_or(0)
                };
            }
            pattern
        }

        fn leave_year(&mut self) -> LeaveYear {
            let month = u8::try_from(self.upto(11) + 1).unwrap_or(1);
            let day =
                u8::try_from(self.upto(i64::from(LEAVE_YEAR_START_DAY_MAX) - 1) + 1).unwrap_or(1);
            LeaveYear::new(month, day).expect("inside the bounds by construction")
        }
    }

    fn day(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).expect("a real date")
    }

    fn invalid<T: std::fmt::Debug>(result: Result<T>) -> String {
        match result {
            Err(StoreError::Validation(message)) => message,
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    // ---- the module's own shape --------------------------------------------

    #[test]
    fn not_one_quantity_in_this_module_is_a_fractional_type() {
        // The code, not the tests: the assertion below names the types it
        // forbids, so it would find itself.
        let code = SOURCE
            .split("#[cfg(test)]")
            .next()
            .expect("the module has a body");
        for forbidden in ["f32", "f64", "as f", "f64::", "0.5", "1.0"] {
            assert!(
                !code.contains(forbidden),
                "the leave arithmetic must be integer-only, found `{forbidden}`"
            );
        }
        assert!(code.contains("i128"), "and it widens rather than rounding");
    }

    // ---- the leave year ----------------------------------------------------

    #[test]
    fn a_leave_year_starts_on_a_day_that_exists_in_every_year() {
        assert_eq!(LeaveYear::calendar().month(), 1);
        assert_eq!(LeaveYear::default().day(), 1);
        assert!(LeaveYear::new(4, 1).is_ok(), "an April year is ordinary");
        assert!(LeaveYear::new(2, 28).is_ok());
        assert!(invalid(LeaveYear::new(2, 29)).contains("every year"));
        assert!(invalid(LeaveYear::new(13, 1)).contains("between 1 and 12"));
        assert!(invalid(LeaveYear::new(1, 0)).contains("leave year start day"));
    }

    #[test]
    fn the_leave_year_containing_a_day_is_the_one_that_started_before_it() {
        let april = LeaveYear::new(4, 1).unwrap();
        // A March day belongs to the year that started last April.
        let (first, last) = april.window(day(2026, Month::March, 31));
        assert_eq!(first, day(2025, Month::April, 1));
        assert_eq!(last, day(2026, Month::March, 31));
        // The first day itself belongs to the year it opens.
        let (first, last) = april.window(day(2026, Month::April, 1));
        assert_eq!(first, day(2026, Month::April, 1));
        assert_eq!(last, day(2027, Month::March, 31));
        // A calendar leave year over a leap year is 366 days, inclusive.
        let (first, last) = LeaveYear::calendar().window(day(2028, Month::June, 1));
        assert_eq!(first, day(2028, Month::January, 1));
        assert_eq!(last, day(2028, Month::December, 31));
        assert_eq!((last - first).whole_days() + 1, 366);
    }

    #[test]
    fn every_generated_leave_year_is_a_contiguous_span_of_a_year() {
        let mut rng = Rng(0x1eaf_1234_5678_9abc);
        for _ in 0..5_000 {
            let year = rng.leave_year();
            let subject = rng.date();
            let (first, last) = year.window(subject);
            assert!(first <= subject && subject <= last, "the day is inside it");
            let length = (last - first).whole_days() + 1;
            assert!((365..=366).contains(&length), "a year is a year: {length}");
            // The next leave year starts the day after this one ends, with no
            // gap and no overlap.
            let (next_first, _) = year.window(last.next_day().unwrap());
            assert_eq!(next_first, last.next_day().unwrap());
        }
    }

    // ---- entitlement -------------------------------------------------------

    #[test]
    fn an_entitlement_scales_with_the_working_pattern() {
        let full_week = weekly_minutes(&FULL_TIME_PATTERN);
        assert_eq!(full_week, 2_400);
        // 25 days of eight hours is 12 000 minutes a year, at a full week.
        assert_eq!(
            scaled_entitlement_minutes(12_000, full_week, full_week),
            12_000
        );
        // A three-day week: three fifths.
        let three_days = weekly_minutes(&[480, 480, 480, 0, 0, 0, 0]);
        assert_eq!(
            scaled_entitlement_minutes(12_000, three_days, full_week),
            7_200
        );
        // A 30-hour contract over five days: three quarters.
        let thirty_hours = weekly_minutes(&[360, 360, 360, 360, 360, 0, 0]);
        assert_eq!(
            scaled_entitlement_minutes(12_000, thirty_hours, full_week),
            9_000
        );
        // Nobody working no day accrues nothing rather than a division fault.
        assert_eq!(scaled_entitlement_minutes(12_000, 0, full_week), 0);
        assert_eq!(scaled_entitlement_minutes(12_000, full_week, 0), 12_000);
    }

    #[test]
    fn a_joiner_gets_the_part_of_the_year_they_were_here_for() {
        let year = LeaveYear::calendar().window(day(2026, Month::June, 1));
        // Joined 1 July: 184 of 365 days. Left 30 June: the other 181.
        let joined =
            prorated_entitlement_minutes(12_000, year, Some(day(2026, Month::July, 1)), None);
        let left =
            prorated_entitlement_minutes(12_000, year, None, Some(day(2026, Month::June, 30)));
        assert_eq!(left, 12_000 * 181 / 365, "5 950 minutes");
        // The joiner's figure is the year *less* the leaver's, not the naive
        // 12 000 × 184 / 365 (which is 6 049): the minute the first division
        // dropped is carried by the second, which is the whole point of
        // computing cumulatively.
        assert_eq!(joined, 6_050);
        assert_eq!(joined + left, 12_000, "and the two are the whole year");
        // Employed for none of it, and employed for all of it.
        assert_eq!(
            prorated_entitlement_minutes(12_000, year, Some(day(2027, Month::January, 1)), None),
            0
        );
        assert_eq!(
            prorated_entitlement_minutes(12_000, year, None, None),
            12_000
        );
        // A single day of employment is a day's share, not nothing.
        let one_day = prorated_entitlement_minutes(
            12_000,
            year,
            Some(day(2026, Month::March, 2)),
            Some(day(2026, Month::March, 2)),
        );
        // A 365th of the year, give or take the minute the carry is holding.
        assert_eq!(one_day, 33);
        assert!((one_day - 12_000 / 365).abs() <= 1);
    }

    #[test]
    fn two_employments_that_partition_a_year_sum_to_exactly_one_year() {
        let mut rng = Rng(0x105e_dead_beef_0001);
        for _ in 0..20_000 {
            let policy_year = rng.leave_year();
            let (first, last) = policy_year.window(rng.date());
            let entitlement = rng.entitlement();
            let length = (last - first).whole_days() + 1;
            // A leaver who goes on `split` and a joiner who starts the day
            // after, together covering the whole leave year.
            let offset = rng.upto(length - 2);
            let split = first + Duration::days(offset);
            let leaver =
                prorated_entitlement_minutes(entitlement, (first, last), None, Some(split));
            let joiner = prorated_entitlement_minutes(
                entitlement,
                (first, last),
                Some(split.next_day().unwrap()),
                None,
            );
            assert_eq!(
                leaver + joiner,
                entitlement,
                "{entitlement} minutes split at {split} lost or gained a minute"
            );
            assert!(leaver >= 0 && joiner >= 0);
        }
    }

    #[test]
    fn a_pro_rata_share_never_exceeds_the_year_however_it_is_cut() {
        let mut rng = Rng(0x105e_dead_beef_0002);
        for _ in 0..10_000 {
            let year = rng.leave_year().window(rng.date());
            let entitlement = rng.entitlement();
            let a = rng.date();
            let b = rng.date();
            let share =
                prorated_entitlement_minutes(entitlement, year, Some(a.min(b)), Some(a.max(b)));
            assert!(
                (0..=entitlement).contains(&share),
                "{share} is not inside 0..={entitlement}"
            );
        }
    }

    // ---- accrual -----------------------------------------------------------

    #[test]
    fn up_front_grants_the_whole_year_on_its_first_day() {
        let first = day(2026, Month::January, 1);
        assert_eq!(
            accrued_minutes(12_000, Accrual::UpFront, first, first),
            12_000
        );
        assert_eq!(
            accrued_minutes(
                12_000,
                Accrual::UpFront,
                first,
                day(2025, Month::December, 31)
            ),
            0,
            "nothing accrues before the year opens"
        );
    }

    #[test]
    fn monthly_grants_a_twelfth_at_each_month_start() {
        let first = day(2026, Month::January, 1);
        let monthly = |as_of: Date| accrued_minutes(12_000, Accrual::Monthly, first, as_of);
        assert_eq!(monthly(first), 1_000, "the first month is granted at once");
        assert_eq!(monthly(day(2026, Month::January, 31)), 1_000);
        assert_eq!(monthly(day(2026, Month::February, 1)), 2_000);
        assert_eq!(monthly(day(2026, Month::June, 15)), 6_000);
        assert_eq!(monthly(day(2026, Month::December, 31)), 12_000);
        assert_eq!(
            monthly(day(2027, Month::May, 1)),
            12_000,
            "a past leave year has fully accrued"
        );
        // A leave year starting on the 6th grants again on the 6th, not the 1st.
        let sixth = day(2026, Month::April, 6);
        assert_eq!(
            accrued_minutes(12_000, Accrual::Monthly, sixth, sixth),
            1_000
        );
        assert_eq!(
            accrued_minutes(12_000, Accrual::Monthly, sixth, day(2026, Month::May, 5)),
            1_000
        );
        assert_eq!(
            accrued_minutes(12_000, Accrual::Monthly, sixth, day(2026, Month::May, 6)),
            2_000
        );
    }

    #[test]
    fn the_twelve_monthly_grants_sum_exactly_to_the_year() {
        let mut rng = Rng(0xacc4_0000_0000_0001);
        for _ in 0..20_000 {
            let entitlement = rng.entitlement();
            let grants: Vec<i64> = (1..=MONTHS_PER_YEAR)
                .map(|month| monthly_grant_minutes(entitlement, month))
                .collect();
            assert_eq!(
                grants.iter().sum::<i64>(),
                entitlement,
                "{entitlement} minutes did not divide into twelve"
            );
            // And the accrual at each month start is the running total of them.
            let first = day(2026, Month::January, 1);
            let mut running = 0;
            for (index, grant) in grants.iter().enumerate() {
                running += grant;
                let as_of = add_months(first, i64::try_from(index).unwrap_or(0));
                assert_eq!(
                    accrued_minutes(entitlement, Accrual::Monthly, first, as_of),
                    running,
                    "month {} of {entitlement}",
                    index + 1
                );
            }
        }
        assert_eq!(
            monthly_grant_minutes(12_000, 0),
            0,
            "there is no month zero"
        );
        assert_eq!(monthly_grant_minutes(12_000, 13), 0);
    }

    #[test]
    fn accrual_never_exceeds_the_entitlement_on_any_day_of_any_year() {
        let mut rng = Rng(0xacc4_0000_0000_0002);
        for _ in 0..10_000 {
            let entitlement = rng.entitlement();
            let year = rng.leave_year();
            let subject = rng.date();
            let (first, _) = year.window(subject);
            for accrual in [Accrual::UpFront, Accrual::Monthly] {
                let accrued = accrued_minutes(entitlement, accrual, first, subject);
                assert!(
                    (0..=entitlement).contains(&accrued),
                    "{accrued} is not inside 0..={entitlement}"
                );
            }
            // Monotone: a later day never accrues less.
            let later = subject.next_day().unwrap();
            assert!(
                accrued_minutes(entitlement, Accrual::Monthly, first, later)
                    >= accrued_minutes(entitlement, Accrual::Monthly, first, subject)
            );
        }
    }

    #[test]
    fn the_vocabulary_is_closed_and_round_trips() {
        for accrual in [Accrual::UpFront, Accrual::Monthly] {
            assert_eq!(Accrual::parse(accrual.as_str()).unwrap(), accrual);
            assert_eq!(accrual.to_string(), accrual.as_str());
        }
        assert!(invalid(Accrual::parse("weekly")).contains("monthly"));
    }

    // ---- carryover ---------------------------------------------------------

    #[test]
    fn carryover_is_capped_and_lapses_on_the_day_the_policy_says() {
        let first = day(2026, Month::January, 1);
        let expiry = carryover_expires_on(first, Some(15));
        assert_eq!(expiry, Some(day(2027, Month::April, 1)), "15 whole months");
        assert_eq!(carryover_expires_on(first, None), None);
        assert_eq!(carryover_expires_on(first, Some(0)), None);
        // Inside the window, capped.
        assert_eq!(carried_in_minutes(5_000, 2_400, expiry, first), 2_400);
        assert_eq!(carried_in_minutes(1_000, 2_400, expiry, first), 1_000);
        // A negative remainder is not a debt carried forward.
        assert_eq!(carried_in_minutes(-3_000, 2_400, expiry, first), 0);
        // No cap means no carryover at all.
        assert_eq!(carried_in_minutes(5_000, 0, expiry, first), 0);
        // On and after the expiry day, nothing survives.
        assert_eq!(
            carried_in_minutes(5_000, 2_400, expiry, day(2027, Month::March, 31)),
            2_400
        );
        assert_eq!(
            carried_in_minutes(5_000, 2_400, expiry, day(2027, Month::April, 1)),
            0
        );
    }

    #[test]
    fn a_month_count_lands_on_the_same_day_of_a_later_month() {
        assert_eq!(
            add_months(day(2026, Month::January, 31), 1),
            day(2026, Month::February, 28),
            "a shorter month clamps rather than rolling over"
        );
        assert_eq!(
            add_months(day(2028, Month::January, 31), 1),
            day(2028, Month::February, 29),
            "and a leap February is 29 days"
        );
        assert_eq!(
            add_months(day(2026, Month::November, 15), 3),
            day(2027, Month::February, 15)
        );
        assert_eq!(
            add_months(day(2026, Month::March, 1), 0),
            day(2026, Month::March, 1)
        );
    }

    // ---- the cost of a request --------------------------------------------

    #[test]
    fn a_request_costs_what_the_person_normally_works() {
        // Mon–Thu 8h, Fri 4h — the pattern the design note argues from.
        let pattern = [480, 480, 480, 480, 240, 0, 0];
        assert_eq!(weekly_minutes(&pattern), 2_160);
        // 2026-08-10 is a Monday. A whole week costs the week.
        let week = requested_days(
            day(2026, Month::August, 10),
            day(2026, Month::August, 16),
            &pattern,
            &[],
            &[],
        )
        .unwrap();
        let cost = request_cost(&week);
        assert_eq!(cost.minutes, 2_160);
        assert_eq!(cost.working_days, 5, "the weekend costs nothing");
        // The Friday alone is 240 minutes — half a day, because that is what
        // half a Friday is.
        let friday = requested_days(
            day(2026, Month::August, 14),
            day(2026, Month::August, 14),
            &pattern,
            &[],
            &[],
        )
        .unwrap();
        assert_eq!(request_cost(&friday).minutes, 240);
        // A Saturday is not leave at all.
        let saturday = requested_days(
            day(2026, Month::August, 15),
            day(2026, Month::August, 15),
            &pattern,
            &[],
            &[],
        )
        .unwrap();
        assert_eq!(request_cost(&saturday), RequestCost::default());
    }

    #[test]
    fn a_holiday_inside_the_range_costs_nothing_and_says_so() {
        let pattern = FULL_TIME_PATTERN;
        // Assumption Day, a Tuesday in this fixture's week.
        let holiday = day(2026, Month::August, 11);
        let days = requested_days(
            day(2026, Month::August, 10),
            day(2026, Month::August, 12),
            &pattern,
            &[holiday],
            &[],
        )
        .unwrap();
        let cost = request_cost(&days);
        assert_eq!(cost.minutes, 960, "two working days, not three");
        assert_eq!(cost.working_days, 2);
        assert_eq!(cost.holiday_minutes, 480);
        assert_eq!(cost.overlap_minutes, 0);
    }

    #[test]
    fn an_overlap_is_measured_so_the_refusal_can_name_it() {
        let already = day(2026, Month::August, 12);
        let days = requested_days(
            day(2026, Month::August, 10),
            day(2026, Month::August, 12),
            &FULL_TIME_PATTERN,
            &[],
            &[already],
        )
        .unwrap();
        let cost = request_cost(&days);
        assert_eq!(cost.minutes, 960);
        assert_eq!(cost.overlap_minutes, 480, "and by exactly how much");
        // A holiday that is also already covered is counted once, as a holiday:
        // it was never going to cost anything.
        let both = requested_days(
            day(2026, Month::August, 12),
            day(2026, Month::August, 12),
            &FULL_TIME_PATTERN,
            &[already],
            &[already],
        )
        .unwrap();
        let cost = request_cost(&both);
        assert_eq!(cost.holiday_minutes, 480);
        assert_eq!(cost.overlap_minutes, 0);
        assert_eq!(cost.minutes, 0);
    }

    #[test]
    fn a_range_costs_exactly_what_its_days_cost_one_at_a_time() {
        let mut rng = Rng(0xc057_0000_0000_0001);
        for _ in 0..5_000 {
            let pattern = rng.pattern();
            let from = rng.date();
            let length = rng.upto(40);
            let to = from + Duration::days(length);
            // A handful of holidays and covered days inside and around the
            // range, so the properties are not all trivially zero.
            let holidays: Vec<Date> = (0..3)
                .map(|_| from + Duration::days(rng.upto(40)))
                .collect();
            let covered: Vec<Date> = (0..3)
                .map(|_| from + Duration::days(rng.upto(40)))
                .collect();
            let days = requested_days(from, to, &pattern, &holidays, &covered).unwrap();
            let whole = request_cost(&days);
            // One day at a time.
            let mut piecewise = RequestCost::default();
            for single in &days {
                let one =
                    requested_days(single.day, single.day, &pattern, &holidays, &covered).unwrap();
                let cost = request_cost(&one);
                piecewise.minutes += cost.minutes;
                piecewise.working_days += cost.working_days;
                piecewise.holiday_minutes += cost.holiday_minutes;
                piecewise.overlap_minutes += cost.overlap_minutes;
            }
            assert_eq!(whole, piecewise, "{from}..={to} is not additive");
            // And split at an arbitrary point, which is how a person books a
            // fortnight as two weeks.
            let split = from + Duration::days(rng.upto(length));
            let head =
                request_cost(&requested_days(from, split, &pattern, &holidays, &covered).unwrap());
            let tail = match split.next_day() {
                Some(next) if next <= to => {
                    request_cost(&requested_days(next, to, &pattern, &holidays, &covered).unwrap())
                }
                _ => RequestCost::default(),
            };
            assert_eq!(whole.minutes, head.minutes + tail.minutes);
            assert!(whole.minutes >= 0);
            // Never more than every day of the range at the daily ceiling.
            assert!(whole.minutes <= (length + 1) * MINUTES_IN_A_DAY);
        }
    }

    #[test]
    fn a_range_that_is_backwards_or_absurd_is_refused_by_name() {
        let from = day(2026, Month::August, 10);
        assert!(
            invalid(requested_days(
                from,
                day(2026, Month::August, 9),
                &FULL_TIME_PATTERN,
                &[],
                &[]
            ))
            .contains("on or after")
        );
        // A year and a day is the most one request may cover.
        assert!(
            requested_days(
                from,
                from + Duration::days(REQUEST_MAX_DAYS - 1),
                &FULL_TIME_PATTERN,
                &[],
                &[]
            )
            .is_ok()
        );
        assert!(
            invalid(requested_days(
                from,
                from + Duration::days(REQUEST_MAX_DAYS),
                &FULL_TIME_PATTERN,
                &[],
                &[]
            ))
            .contains("366 days")
        );
        // One day is a request.
        assert_eq!(
            requested_days(from, from, &FULL_TIME_PATTERN, &[], &[])
                .unwrap()
                .len(),
            1
        );
    }

    // ---- the balance -------------------------------------------------------

    #[test]
    fn a_balance_is_carried_in_plus_accrued_less_taken_and_booked() {
        let folded = balance(&LeaveLedger {
            entitlement_minutes: 12_000,
            carried_in_minutes: 960,
            accrued_minutes: 6_000,
            taken_minutes: 2_400,
            booked_minutes: 1_440,
            pending_minutes: 480,
        });
        assert_eq!(folded.remaining_minutes, 960 + 6_000 - 2_400 - 1_440);
        assert_eq!(folded.pending_minutes, 480, "reported, never deducted");
        assert_eq!(folded.entitlement_minutes, 12_000);
    }

    #[test]
    fn the_balance_identity_holds_for_every_generated_ledger() {
        let mut rng = Rng(0xba1a_0000_0000_0001);
        for _ in 0..20_000 {
            let ledger = LeaveLedger {
                entitlement_minutes: rng.entitlement(),
                carried_in_minutes: rng.upto(20_000),
                accrued_minutes: rng.upto(20_000),
                taken_minutes: rng.upto(20_000),
                booked_minutes: rng.upto(20_000),
                pending_minutes: rng.upto(20_000),
            };
            let folded = balance(&ledger);
            assert_eq!(
                folded.remaining_minutes,
                ledger.carried_in_minutes + ledger.accrued_minutes
                    - ledger.taken_minutes
                    - ledger.booked_minutes
            );
            // Pending is beside the balance: it moves nothing.
            let without_pending = balance(&LeaveLedger {
                pending_minutes: 0,
                ..ledger
            });
            assert_eq!(folded.remaining_minutes, without_pending.remaining_minutes);
        }
    }

    #[test]
    fn a_corrupted_ledger_saturates_rather_than_wrapping() {
        let folded = balance(&LeaveLedger {
            carried_in_minutes: i64::MAX,
            accrued_minutes: i64::MAX,
            ..LeaveLedger::default()
        });
        assert_eq!(folded.remaining_minutes, i64::MAX);
    }

    // ---- days, for the screen only ----------------------------------------

    #[test]
    fn minutes_become_the_days_a_screen_shows() {
        let pattern = [480, 480, 480, 480, 240, 0, 0];
        // The average worked day of Mon–Thu 8h, Fri 4h is 7h12 = 432 minutes.
        let average = average_working_day_minutes(&pattern);
        assert_eq!(average, 432);
        assert_eq!(days_tenths(2_160, average), 50, "a week is five days");
        assert_eq!(days_tenths(432, average), 10);
        assert_eq!(days_tenths(216, average), 5, "half a day");
        assert_eq!(days_tenths(0, average), 0);
        // Full time is a plain eight-hour day.
        assert_eq!(average_working_day_minutes(&FULL_TIME_PATTERN), 480);
        assert_eq!(days_tenths(4_800, 480), 100);
        // A negative balance reads negative, not absurd.
        assert_eq!(days_tenths(-480, 480), -10);
        // Nobody working any day divides by nothing.
        assert_eq!(average_working_day_minutes(&[0; PATTERN_DAYS]), 0);
        assert_eq!(days_tenths(480, 0), 0);
    }

    #[test]
    fn an_average_day_is_inside_the_days_the_pattern_states() {
        let mut rng = Rng(0xda75_0000_0000_0001);
        for _ in 0..5_000 {
            let pattern = rng.pattern();
            let average = i64::from(average_working_day_minutes(&pattern));
            let worked: Vec<i64> = pattern
                .iter()
                .filter(|m| **m > 0)
                .map(|m| i64::from(*m))
                .collect();
            if worked.is_empty() {
                assert_eq!(average, 0);
                continue;
            }
            let smallest = worked.iter().copied().min().unwrap_or(0);
            let largest = worked.iter().copied().max().unwrap_or(0);
            assert!(
                (smallest..=largest).contains(&average),
                "{average} is outside {smallest}..={largest}"
            );
            assert!(average <= MINUTES_IN_A_DAY);
            assert!(i64::from(weekly_minutes(&pattern)) <= DAYS_IN_A_WEEK * MINUTES_IN_A_DAY);
        }
    }
}
