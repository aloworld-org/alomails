//! How often a standing arrangement bills (alo Billing, ADR 0035, wave B2) —
//! the pure calendar arithmetic behind [`crate::billing_schedules`].
//!
//! It has no database and no clock of its own: a cadence, a date and an anchor
//! day in, the next date out. That is what lets the schedule runner be tested
//! against any date at all — the run takes `today` as an argument, and every
//! date it derives comes from here (`docs/design/billing.md`, B2.11).
//!
//! **The anchor is the day of the month the arrangement started on, not the day
//! the last invoice happened to land on.** A monthly schedule started on the
//! 31st bills on the 31st, and in February on the 28th (29th in a leap year) —
//! and then on the 31st again in March. Advancing from the *landed* date
//! instead would walk a 31st down to a 28th and leave it there forever, which
//! is how a monthly subscription silently becomes a "28th of the month" one
//! after its first February.
//!
//! Weekly is the exception, and deliberately so: a week has no month-end to
//! clamp against, so it is plain seven-day arithmetic and the anchor day is
//! never consulted. A weekly schedule keeps its weekday for ever.

use time::{Date, Duration, Month};

/// How often a schedule raises its invoice.
///
/// Four values, not a general `every N units` rule: these are the four rhythms
/// businesses actually bill on, and each one is a word a tenant can read on a
/// screen. A recurrence rule as expressive as iCalendar's belongs to a calendar,
/// where the user is choosing when to meet; here it would only be a way to
/// express arrangements no accountant would recognise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Cadence {
    /// Every seven days, keeping the weekday.
    Weekly,
    /// Every month on the anchor day, clamped to the month's length.
    Monthly,
    /// Every three months on the anchor day.
    Quarterly,
    /// Every twelve months on the anchor day.
    Yearly,
}

impl Cadence {
    /// The value stored in the `cadence` column.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
            Self::Quarterly => "quarterly",
            Self::Yearly => "yearly",
        }
    }

    /// Parses a stored or requested cadence, or `None` if it is not one of the
    /// four. Case-insensitive and blank-tolerant, so a value typed into a form
    /// and a value read back from the column go through the same door.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "weekly" => Some(Self::Weekly),
            "monthly" => Some(Self::Monthly),
            "quarterly" => Some(Self::Quarterly),
            "yearly" => Some(Self::Yearly),
            _ => None,
        }
    }

    /// How many months this cadence steps, or `None` for the weekly one, which
    /// does not count in months at all.
    fn months(self) -> Option<u8> {
        match self {
            Self::Weekly => None,
            Self::Monthly => Some(1),
            Self::Quarterly => Some(3),
            Self::Yearly => Some(12),
        }
    }
}

/// The date **after** `from` that this cadence lands on, given the day of the
/// month the arrangement is anchored to.
///
/// `anchor_day` is the day the schedule started on (1–31); it is clamped to the
/// length of the month it lands in, so 31 January + one month is 28 February
/// and + two months is 31 March. Out-of-range anchors are clamped into 1–31
/// rather than refused — the store only ever passes a stored day, and a
/// defensive clamp here is cheaper than a second error path in the runner.
///
/// `None` only when the answer would fall outside the calendar the `time` crate
/// supports, which is roughly ±9999 years — a schedule cannot walk there in any
/// number of steps a run performs, but the arithmetic is fallible so the answer
/// is too, rather than silently wrapping.
#[must_use]
pub fn next_occurrence(from: Date, cadence: Cadence, anchor_day: u8) -> Option<Date> {
    let Some(months) = cadence.months() else {
        return from.checked_add(Duration::weeks(1));
    };
    let (year, month) = add_months(from.year(), from.month(), months);
    let day = anchor_day
        .clamp(1, 31)
        .min(time::util::days_in_month(month, year));
    Date::from_calendar_date(year, month, day).ok()
}

/// `months` calendar months after `(year, month)`, rolling the year over.
///
/// `Month::nth_next` walks the twelve names; the year moves when the walk
/// passes December, which is what the ordinal comparison detects. `months` is
/// one of ours (1, 3 or 12), never a caller's number, so the walk is short.
fn add_months(year: i32, month: Month, months: u8) -> (i32, Month) {
    let landed = month.nth_next(months);
    let rolled = i32::from(u8::from(month) - 1 + months) / 12;
    (year + rolled, landed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Month::{August, December, February, January, March, May, November};

    fn day(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).unwrap_or_else(|e| panic!("{e}"))
    }

    #[test]
    fn every_cadence_round_trips_through_its_stored_form() {
        for cadence in [
            Cadence::Weekly,
            Cadence::Monthly,
            Cadence::Quarterly,
            Cadence::Yearly,
        ] {
            assert_eq!(Cadence::parse(cadence.as_str()), Some(cadence));
        }
        // What a form sends is read by the same door as what the column holds.
        assert_eq!(Cadence::parse(" Monthly "), Some(Cadence::Monthly));
        for bad in ["", "daily", "month", "every month", "biweekly"] {
            assert_eq!(Cadence::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn a_weekly_schedule_keeps_its_weekday_for_ever() {
        let mut date = day(2026, January, 1);
        let weekday = date.weekday();
        for _ in 0..60 {
            date = next_occurrence(date, Cadence::Weekly, 1)
                .unwrap_or_else(|| panic!("weekly ran off the calendar"));
            assert_eq!(date.weekday(), weekday, "{date} moved off its weekday");
        }
        // Sixty weeks later — through a year boundary and a February.
        assert_eq!(date, day(2027, February, 25));
    }

    #[test]
    fn a_month_end_anchor_is_clamped_and_then_recovers() {
        // The whole reason the anchor is the *start* day rather than the last
        // landed one: 31 → 28 → 31, not 31 → 28 → 28 for ever.
        let mut date = day(2026, January, 31);
        let mut landed = Vec::new();
        for _ in 0..4 {
            date = next_occurrence(date, Cadence::Monthly, 31)
                .unwrap_or_else(|| panic!("monthly ran off the calendar"));
            landed.push(date);
        }
        assert_eq!(
            landed,
            vec![
                day(2026, February, 28),
                day(2026, March, 31),
                day(2026, time::Month::April, 30),
                day(2026, May, 31),
            ]
        );
    }

    #[test]
    fn february_is_a_day_longer_in_a_leap_year() {
        assert_eq!(
            next_occurrence(day(2028, January, 31), Cadence::Monthly, 31),
            Some(day(2028, February, 29)),
            "2028 is a leap year"
        );
        assert_eq!(
            next_occurrence(day(2100, January, 31), Cadence::Monthly, 31),
            Some(day(2100, February, 28)),
            "2100 is divisible by 100 and not by 400"
        );
    }

    #[test]
    fn the_quarterly_and_yearly_steps_roll_the_year_over() {
        assert_eq!(
            next_occurrence(day(2026, November, 15), Cadence::Quarterly, 15),
            Some(day(2027, February, 15))
        );
        assert_eq!(
            next_occurrence(day(2026, December, 1), Cadence::Quarterly, 1),
            Some(day(2027, March, 1))
        );
        assert_eq!(
            next_occurrence(day(2026, August, 29), Cadence::Yearly, 29),
            Some(day(2027, August, 29))
        );
        // A 29 February arrangement bills on the 28th in the three years
        // between leap years, and on the 29th again when one comes round.
        assert_eq!(
            next_occurrence(day(2028, February, 29), Cadence::Yearly, 29),
            Some(day(2029, February, 28))
        );
        assert_eq!(
            next_occurrence(day(2031, February, 28), Cadence::Yearly, 29),
            Some(day(2032, February, 29))
        );
    }

    #[test]
    fn the_month_walk_agrees_with_the_calendar_from_every_month() {
        // Every starting month, every step we support: the year rolls exactly
        // when the walk passes December, and never otherwise.
        for start in 1..=12u8 {
            let month = Month::try_from(start).unwrap_or_else(|e| panic!("{e}"));
            for months in [1u8, 3, 12] {
                let (year, landed) = add_months(2026, month, months);
                let expected_month = (start - 1 + months) % 12 + 1;
                assert_eq!(u8::from(landed), expected_month, "{month:?} + {months}");
                let expected_year = 2026 + (i32::from(start) - 1 + i32::from(months)) / 12;
                assert_eq!(year, expected_year, "{month:?} + {months}");
            }
        }
    }

    #[test]
    fn an_out_of_range_anchor_is_clamped_rather_than_refused() {
        // Only reachable through corrupt data — the column is constrained to
        // 1–31 — and even then it must land on a real day.
        assert_eq!(
            next_occurrence(day(2026, January, 15), Cadence::Monthly, 0),
            Some(day(2026, February, 1))
        );
        assert_eq!(
            next_occurrence(day(2026, January, 15), Cadence::Monthly, 99),
            Some(day(2026, February, 28))
        );
    }

    #[test]
    fn the_answer_is_always_after_the_date_it_came_from() {
        // The runner's catch-up loop terminates on exactly this property: every
        // step moves forward, so a loop bounded by `today` always reaches it.
        let mut date = day(2026, January, 31);
        for cadence in [
            Cadence::Weekly,
            Cadence::Monthly,
            Cadence::Quarterly,
            Cadence::Yearly,
        ] {
            for _ in 0..10 {
                let next = next_occurrence(date, cadence, 31)
                    .unwrap_or_else(|| panic!("{cadence:?} ran off the calendar"));
                assert!(next > date, "{cadence:?}: {next} is not after {date}");
                date = next;
            }
        }
    }
}
