//! Which public-holiday calendars a tenant observes, and the days that follow
//! from the choice (alo HR, ADR 0035, wave B6.04; `docs/design/hr.md`, "Public
//! holidays").
//!
//! The holidays themselves live in [`crate::hr_holiday_seed`] — a pure table
//! with each country's instrument named beside it. What is per-tenant is only
//! the *choice*: the calendars a company observes, and the one its leave
//! arithmetic folds against.
//!
//! # Why a holiday costs nothing, and why that is the whole point
//!
//! A public holiday inside a leave range consumes no balance: somebody who books
//! the week of 25 December spends four days, not five. That single rule is the
//! only reason this file exists, and it is why the choice must be *one*
//! calendar rather than a set — two calendars would make "is this day free" a
//! question with two answers and a balance nobody can explain. The set exists
//! because a company with staff in three countries wants to *see* three
//! calendars; the default is the one that counts.
//!
//! # A tenant that observes nothing
//!
//! Answers no holidays, and leave is folded on the working pattern alone. That
//! is correct rather than degraded — it is what every balance in the product did
//! before this file existed — so nothing here ever fails because a choice has
//! not been made.

use time::{Date, OffsetDateTime};

use crate::error::{Result, StoreError};
use crate::hr_holiday_seed::{
    Holiday, HolidayCalendar, holiday_calendar, holiday_calendars, holiday_year_covered,
};
use crate::id::UserId;
use crate::store::TenantStore;

/// The most calendars one tenant may observe. Ten is more countries than any
/// tenant this product is built for has staff in, and it bounds the array the
/// schema stores.
pub const OBSERVED_CALENDARS_MAX: usize = 10;

/// A tenant's choice of holiday calendars.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HolidaySelection {
    /// The calendars observed, uppercase ISO 3166-1 alpha-2, in the order they
    /// were chosen. Empty means the tenant deliberately observes none.
    pub calendars: Vec<String>,
    /// The calendar the leave arithmetic uses. `None` only while nothing is
    /// observed.
    pub default_calendar: Option<String>,
    /// The user who last made the choice.
    pub chosen_by: String,
    /// When it was last changed.
    pub updated_at: OffsetDateTime,
}

/// The tenant's default calendar, resolved once and then asked about days.
///
/// Deliberately not a set of dates: a resolver is read from the database once
/// per store call and then answers any range without going back, which is what
/// lets a list of fifty leave requests fold its costs from one query.
#[derive(Debug, Clone, Copy, Default)]
pub struct TenantHolidays {
    calendar: Option<&'static HolidayCalendar>,
}

impl TenantHolidays {
    /// A tenant that observes no calendar: every day is an ordinary day.
    #[must_use]
    pub const fn none() -> Self {
        Self { calendar: None }
    }

    /// The resolver for one calendar, or [`TenantHolidays::none`] when the code
    /// is not one the seed carries.
    #[must_use]
    pub fn for_calendar(code: &str) -> Self {
        Self {
            calendar: holiday_calendar(code),
        }
    }

    /// The observed calendar's code, or `None`.
    #[must_use]
    pub fn calendar_code(&self) -> Option<&'static str> {
        self.calendar.map(|calendar| calendar.code)
    }

    /// The holidays between `from` and `to` inclusive, earliest first.
    ///
    /// Lenient about the seed's reviewed years: a range reaching outside them
    /// answers nothing for those days rather than inventing them. Refusing here
    /// would make an old leave request unreadable, which is a worse answer than
    /// folding it the way it was folded when it was made.
    #[must_use]
    pub fn between(&self, from: Date, to: Date) -> Vec<Holiday> {
        self.calendar
            .map(|calendar| calendar.between(from, to))
            .unwrap_or_default()
    }

    /// Just the days, for the leave fold — each date once, however many names
    /// it carries. Luxembourg's Europe Day fell on Ascension in 2024, and a day
    /// off twice is still one day off.
    #[must_use]
    pub fn days(&self, from: Date, to: Date) -> Vec<Date> {
        let mut days: Vec<Date> = self
            .between(from, to)
            .into_iter()
            .map(|holiday| holiday.day)
            .collect();
        days.dedup();
        days
    }

    /// Whether `day` is a public holiday on the observed calendar.
    #[must_use]
    pub fn is_holiday(&self, day: Date) -> bool {
        !self.between(day, day).is_empty()
    }

    /// Refuses a range whose years the seed has not been reviewed for, naming
    /// the range it does carry.
    ///
    /// A tenant that observes nothing is never refused: no holiday can be
    /// missing from a calculation that uses none.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when a year in `from..=to` is outside the
    /// seed's reviewed range.
    pub fn covers(&self, from: Date, to: Date) -> Result<()> {
        if self.calendar.is_none() {
            return Ok(());
        }
        for year in from.year()..=to.year() {
            holiday_year_covered(year)?;
        }
        Ok(())
    }
}

/// Normalises and validates a list of calendar codes.
///
/// Uppercased and trimmed, duplicates dropped keeping the first, every code
/// checked against the seed — an unknown one is named rather than silently
/// dropped, because a tenant who typed `UK` must learn that we carry no such
/// calendar rather than find their staff working on Christmas Day.
fn clean_calendars(calendars: &[String]) -> Result<Vec<String>> {
    let mut out: Vec<String> = Vec::with_capacity(calendars.len());
    for code in calendars {
        let code = code.trim().to_ascii_uppercase();
        if code.is_empty() {
            continue;
        }
        if holiday_calendar(&code).is_none() {
            let known = holiday_calendars()
                .iter()
                .map(|calendar| calendar.code)
                .collect::<Vec<_>>()
                .join(", ");
            return Err(StoreError::Validation(format!(
                "there is no public-holiday calendar for {code}; the ones we carry are {known}"
            )));
        }
        if !out.contains(&code) {
            out.push(code);
        }
    }
    if out.len() > OBSERVED_CALENDARS_MAX {
        return Err(StoreError::Validation(format!(
            "a company may observe at most {OBSERVED_CALENDARS_MAX} holiday calendars"
        )));
    }
    Ok(out)
}

impl TenantStore {
    /// The tenant's choice, or `None` while nobody has made one.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn hr_holiday_selection(&self) -> Result<Option<HolidaySelection>> {
        let row: Option<(Vec<String>, Option<String>, String, OffsetDateTime)> = sqlx::query_as(
            "SELECT calendars, default_calendar, chosen_by, updated_at \
               FROM hr_holiday_selection WHERE tenant_id = $1",
        )
        .bind(self.tenant().as_str())
        .fetch_optional(self.pool())
        .await
        .map_err(StoreError::Db)?;
        Ok(row.map(
            |(calendars, default_calendar, chosen_by, updated_at)| HolidaySelection {
                calendars,
                default_calendar,
                chosen_by,
                updated_at,
            },
        ))
    }

    /// The tenant's choice, seeding it from their country on first read.
    ///
    /// A company registered in Belgium observes the Belgian calendar without
    /// pressing anything — the same reasoning that seeds their first leave
    /// policy from the Belgian statutory minimum
    /// ([`crate::hr_statutory_leave`]): the first person to ask for time off
    /// must find an arithmetic that already knows 21 July is free. A country the
    /// seed does not carry produces an explicit empty choice, so the question is
    /// settled either way and the screen can say so.
    ///
    /// Idempotent, and two callers racing produce one row.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn ensure_hr_holiday_selection(&self, actor: &UserId) -> Result<HolidaySelection> {
        if let Some(chosen) = self.hr_holiday_selection().await? {
            return Ok(chosen);
        }
        let country: Option<String> =
            sqlx::query_scalar("SELECT country FROM billing_settings WHERE tenant_id = $1")
                .bind(self.tenant().as_str())
                .fetch_optional(self.pool())
                .await
                .map_err(StoreError::Db)?;
        let seeded: Vec<String> = country
            .as_deref()
            .and_then(holiday_calendar)
            .map(|calendar| vec![calendar.code.to_owned()])
            .unwrap_or_default();
        sqlx::query(
            "INSERT INTO hr_holiday_selection (tenant_id, calendars, default_calendar, chosen_by) \
             VALUES ($1, $2, $3, $4) ON CONFLICT (tenant_id) DO NOTHING",
        )
        .bind(self.tenant().as_str())
        .bind(&seeded)
        .bind(seeded.first())
        .bind(actor.as_str())
        .execute(self.pool())
        .await
        .map_err(StoreError::Db)?;
        // The row exists now, whether this call or a racing one wrote it.
        self.hr_holiday_selection().await?.ok_or_else(|| {
            StoreError::Validation("the holiday choice could not be read back".into())
        })
    }

    /// Records which calendars the tenant observes, and which one counts.
    ///
    /// `default_calendar` may be omitted, in which case the first observed
    /// calendar is used — a company that names one country is not made to say
    /// twice which one it means. Naming a default that is not observed is a
    /// refusal rather than a silent addition: the two halves of the choice
    /// disagreeing is exactly the mistake worth catching.
    ///
    /// # Errors
    /// [`StoreError::Validation`] on an unknown calendar code, too many
    /// calendars, or a default that is not among them; [`StoreError::Db`] on
    /// failure.
    pub async fn set_hr_holiday_selection(
        &self,
        calendars: &[String],
        default_calendar: Option<&str>,
        actor: &UserId,
    ) -> Result<HolidaySelection> {
        let calendars = clean_calendars(calendars)?;
        let default = match default_calendar
            .map(str::trim)
            .filter(|code| !code.is_empty())
        {
            None => calendars.first().cloned(),
            Some(code) => {
                let code = code.to_ascii_uppercase();
                if !calendars.contains(&code) {
                    return Err(StoreError::Validation(format!(
                        "{code} cannot be the default holiday calendar because this company does \
                         not observe it"
                    )));
                }
                Some(code)
            }
        };
        sqlx::query(
            "INSERT INTO hr_holiday_selection (tenant_id, calendars, default_calendar, chosen_by) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT (tenant_id) DO UPDATE \
                SET calendars = EXCLUDED.calendars, \
                    default_calendar = EXCLUDED.default_calendar, \
                    chosen_by = EXCLUDED.chosen_by, \
                    updated_at = now()",
        )
        .bind(self.tenant().as_str())
        .bind(&calendars)
        .bind(default.as_deref())
        .bind(actor.as_str())
        .execute(self.pool())
        .await
        .map_err(StoreError::Db)?;
        self.hr_holiday_selection().await?.ok_or_else(|| {
            StoreError::Validation("the holiday choice could not be read back".into())
        })
    }

    /// The resolver every leave computation folds against — one query, then pure
    /// arithmetic.
    ///
    /// **Does not seed.** A read that has to write is a read that fails on a
    /// replica and surprises a reader; seeding is
    /// [`TenantStore::ensure_hr_holiday_selection`], called by the screen that
    /// shows the choice. Until then a tenant folds leave on the working pattern
    /// alone, which is what every balance did before this wave.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn hr_holidays(&self) -> Result<TenantHolidays> {
        let code: Option<String> = sqlx::query_scalar(
            "SELECT default_calendar FROM hr_holiday_selection WHERE tenant_id = $1",
        )
        .bind(self.tenant().as_str())
        .fetch_optional(self.pool())
        .await
        .map_err(StoreError::Db)?
        .flatten();
        Ok(code
            .as_deref()
            .map_or_else(TenantHolidays::none, TenantHolidays::for_calendar))
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

    #[test]
    fn a_tenant_observing_nothing_has_no_holidays_and_is_never_refused() {
        let none = TenantHolidays::none();
        assert!(none.calendar_code().is_none());
        assert!(
            none.days(day(2026, Month::January, 1), day(2026, Month::December, 31))
                .is_empty()
        );
        assert!(!none.is_holiday(day(2026, Month::December, 25)));
        // Even a year the seed has never heard of: nothing can be missing from a
        // calculation that uses no calendar.
        assert!(
            none.covers(day(2099, Month::January, 1), day(2099, Month::March, 1))
                .is_ok()
        );
    }

    #[test]
    fn an_observed_calendar_answers_its_own_days() {
        let be = TenantHolidays::for_calendar("be");
        assert_eq!(be.calendar_code(), Some("BE"));
        assert!(be.is_holiday(day(2026, Month::July, 21)), "national day");
        assert!(!be.is_holiday(day(2026, Month::July, 20)));
        let week = be.days(
            day(2026, Month::December, 21),
            day(2026, Month::December, 27),
        );
        assert_eq!(week, vec![day(2026, Month::December, 25)]);
        // An unknown code observes nothing rather than guessing a neighbour.
        assert!(TenantHolidays::for_calendar("UK").calendar_code().is_none());
    }

    /// Two names on one date is one day off: Luxembourg's Europe Day fell on
    /// Ascension in 2024, and the fold must not see the day twice.
    #[test]
    fn a_date_two_holidays_share_is_one_day() {
        let lu = TenantHolidays::for_calendar("LU");
        let ascension = day(2024, Month::May, 9);
        assert_eq!(lu.between(ascension, ascension).len(), 2, "two names");
        assert_eq!(lu.days(ascension, ascension), vec![ascension], "one day");
    }

    #[test]
    fn a_year_the_seed_has_not_been_reviewed_for_is_refused_for_an_observer() {
        let fr = TenantHolidays::for_calendar("FR");
        assert!(
            fr.covers(day(2026, Month::March, 1), day(2026, Month::March, 5))
                .is_ok()
        );
        let refusal = format!(
            "{:?}",
            fr.covers(day(2036, Month::March, 1), day(2036, Month::March, 5))
                .unwrap_err()
        );
        assert!(refusal.contains("2036"), "{refusal}");
        // The lenient reader still answers, with nothing.
        assert!(
            fr.days(day(2036, Month::March, 1), day(2036, Month::March, 5))
                .is_empty()
        );
    }

    #[test]
    fn a_choice_is_cleaned_and_an_unknown_code_is_named() {
        let cleaned = clean_calendars(&[
            " be ".to_owned(),
            "nl".to_owned(),
            "BE".to_owned(),
            String::new(),
        ])
        .expect("a clean choice");
        assert_eq!(cleaned, vec!["BE".to_owned(), "NL".to_owned()]);
        let refusal = format!("{:?}", clean_calendars(&["UK".to_owned()]).unwrap_err());
        assert!(refusal.contains("UK"), "{refusal}");
        assert!(
            refusal.contains("BE"),
            "the known list is offered: {refusal}"
        );
        let too_many: Vec<String> = [
            "AT", "BE", "DE", "DK", "ES", "FI", "FR", "IE", "IT", "LU", "MT",
        ]
        .iter()
        .map(|code| (*code).to_owned())
        .collect();
        assert!(clean_calendars(&too_many).is_err());
        assert!(clean_calendars(&[]).expect("an empty choice").is_empty());
    }
}
