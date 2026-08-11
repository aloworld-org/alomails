//! The public holidays of the member states alo sells into — **pure**: a seed
//! table, the rules that turn it into dates, and the computus behind the movable
//! feasts (alo HR, ADR 0035, wave B6.04; `docs/design/hr.md`, "Public
//! holidays").
//!
//! # Seed data, in the repo, with its source named
//!
//! No external calendar service and no network call: the sovereignty promise
//! forbids the dependency, and a national holiday act changes about as often as
//! a statutory leave minimum does. Each calendar names the instrument it comes
//! from, so a tenant asking "why is 21 July free and 11 July not?" gets an
//! answer rather than a shrug — the same rule
//! [`crate::hr_statutory_leave`] follows.
//!
//! # The three decisions this file records
//!
//! - **Movable feasts are computed, not listed.** Easter and the seven days that
//!   hang off it come from the anonymous Gregorian computus in
//!   [`easter_sunday`], unit-tested against a published table. Listing them per
//!   year per country is how a seed table silently runs out.
//! - **A year outside [`HOLIDAY_FIRST_YEAR`]`..=`[`HOLIDAY_LAST_YEAR`] is an
//!   error, never an empty answer.** "No holidays in 2036" and "we have not
//!   reviewed 2036 yet" must not look the same to a balance computation, so
//!   [`holiday_year_covered`] refuses the year and names the range it does
//!   carry. The rules themselves would happily compute 2036; the refusal is
//!   about whether anybody has *checked* that the law still says what this table
//!   says.
//! - **Names are the country's own words, beside a stable key.** "Koningsdag"
//!   and "Ferragosto" are proper nouns, not UI strings to be translated into
//!   English and back; the `key` beside each is what a future catalogue would
//!   translate and what a client may safely branch on. This is deliberately not
//!   an i18n hole: no sentence here is authored by us.
//!
//! # What this table is not
//!
//! **National days only.** German *Länder*, Spanish *comunidades* and Italian
//! *patron saints' days* add regional holidays that this table does not carry,
//! and a tenant whose staff sit in one region will find days missing. Regional
//! sub-calendars are a per-employment question (a cross-border employee observes
//! their own calendar, not their employer's) and are recorded as the next step
//! in `docs/design/hr.md` rather than half-built here. A missing holiday costs
//! an employee leave they should have kept, so the omission is stated on the
//! wire: every calendar carries its `note`.

use time::{Date, Duration, Month, Weekday};

use crate::error::{Result, StoreError};

/// The first year the seed has been reviewed for.
pub const HOLIDAY_FIRST_YEAR: i32 = 2020;

/// The last year the seed has been reviewed for. A request past it is refused
/// rather than answered from rules nobody has checked against the law.
pub const HOLIDAY_LAST_YEAR: i32 = 2035;

// ---- the movable feasts -----------------------------------------------------

/// Maundy Thursday, three days before Easter Sunday.
const MAUNDY_THURSDAY: i16 = -3;
/// Good Friday, two days before Easter Sunday.
const GOOD_FRIDAY: i16 = -2;
/// Easter Sunday itself.
const EASTER_SUNDAY: i16 = 0;
/// Easter Monday, the day after.
const EASTER_MONDAY: i16 = 1;
/// Danish *store bededag*, the fourth Friday after Easter (abolished from 2024).
const GREAT_PRAYER_DAY: i16 = 26;
/// Ascension, the fortieth day of Eastertide — 39 days after Easter Sunday.
const ASCENSION: i16 = 39;
/// Whit Sunday (Pentecost), 49 days after Easter Sunday.
const WHIT_SUNDAY: i16 = 49;
/// Whit Monday, the day after Pentecost.
const WHIT_MONDAY: i16 = 50;
/// Corpus Christi, the Thursday after Trinity Sunday.
const CORPUS_CHRISTI: i16 = 60;

/// Easter Sunday in the Gregorian calendar, by the anonymous computus.
///
/// The algorithm is arithmetic on the year alone — no table, no clock — and is
/// pinned in the tests against a published list of Easter dates for every year
/// this table covers.
///
/// `None` only for a year whose Easter is not a constructible date, which cannot
/// happen inside the covered range; it is spelled fallibly rather than with a
/// panic because a store never panics on data.
#[must_use]
pub fn easter_sunday(year: i32) -> Option<Date> {
    let a = year % 19;
    let b = year / 100;
    let c = year % 100;
    let d = b / 4;
    let e = b % 4;
    let f = (b + 8) / 25;
    let g = (b - f + 1) / 3;
    let h = (19 * a + b - d - g + 15) % 30;
    let i = c / 4;
    let k = c % 4;
    let l = (32 + 2 * e + 2 * i - h - k) % 7;
    let m = (a + 11 * h + 22 * l) / 451;
    let month = (h + l - 7 * m + 114) / 31;
    let day = ((h + l - 7 * m + 114) % 31) + 1;
    let month = Month::try_from(u8::try_from(month).ok()?).ok()?;
    Date::from_calendar_date(year, month, u8::try_from(day).ok()?).ok()
}

// ---- the rules --------------------------------------------------------------

/// How one holiday's date is found for a given year.
///
/// Five shapes cover every national holiday in the table: the rest of the
/// variation between member states is *which* days they observe, not how those
/// days are worked out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HolidayRule {
    /// The same calendar date every year (Christmas Day, 14 July).
    Fixed {
        /// The month it falls in.
        month: Month,
        /// The day of that month.
        day: u8,
    },
    /// A fixed date that moves when it falls on a Sunday — the Dutch
    /// *Koningsdag*, celebrated on the Saturday before.
    FixedOffSunday {
        /// The month it normally falls in.
        month: Month,
        /// The day it normally falls on.
        day: u8,
        /// Days to move it by when that date is a Sunday (negative moves it
        /// earlier).
        shift_days: i8,
    },
    /// A fixed number of days from Easter Sunday (negative before it).
    Easter {
        /// The offset in days.
        offset: i16,
    },
    /// The first `weekday` on or after a date — "the first Monday in May", "the
    /// last Monday in October" (from the 25th), "the Saturday between 31 October
    /// and 6 November".
    WeekdayOnOrAfter {
        /// The month the window opens in.
        month: Month,
        /// The day the window opens on.
        day: u8,
        /// The weekday looked for.
        weekday: Weekday,
    },
    /// As [`HolidayRule::WeekdayOnOrAfter`], except that when the opening date
    /// itself falls on `unless`, that date is the holiday. Ireland's St Brigid's
    /// Day: the first Monday in February, unless 1 February is a Friday.
    WeekdayOnOrAfterUnless {
        /// The month the window opens in.
        month: Month,
        /// The day the window opens on.
        day: u8,
        /// The weekday looked for.
        weekday: Weekday,
        /// The weekday of the opening date that makes the opening date itself
        /// the holiday.
        unless: Weekday,
    },
}

impl HolidayRule {
    /// The date this rule falls on in `year`, or `None` when the year cannot
    /// produce one (an unconstructible date; not reachable inside the covered
    /// range).
    #[must_use]
    pub fn day_in(&self, year: i32) -> Option<Date> {
        match *self {
            Self::Fixed { month, day } => Date::from_calendar_date(year, month, day).ok(),
            Self::FixedOffSunday {
                month,
                day,
                shift_days,
            } => {
                let date = Date::from_calendar_date(year, month, day).ok()?;
                if date.weekday() == Weekday::Sunday {
                    date.checked_add(Duration::days(i64::from(shift_days)))
                } else {
                    Some(date)
                }
            }
            Self::Easter { offset } => {
                easter_sunday(year)?.checked_add(Duration::days(i64::from(offset)))
            }
            Self::WeekdayOnOrAfter {
                month,
                day,
                weekday,
            } => first_weekday_on_or_after(year, month, day, weekday),
            Self::WeekdayOnOrAfterUnless {
                month,
                day,
                weekday,
                unless,
            } => {
                let opens = Date::from_calendar_date(year, month, day).ok()?;
                if opens.weekday() == unless {
                    Some(opens)
                } else {
                    first_weekday_on_or_after(year, month, day, weekday)
                }
            }
        }
    }
}

/// The first `weekday` on or after `year-month-day`, within the week that
/// follows it.
fn first_weekday_on_or_after(year: i32, month: Month, day: u8, weekday: Weekday) -> Option<Date> {
    let start = Date::from_calendar_date(year, month, day).ok()?;
    let ahead = i64::from(
        (weekday.number_days_from_monday() + 7 - start.weekday().number_days_from_monday()) % 7,
    );
    start.checked_add(Duration::days(ahead))
}

// ---- the table --------------------------------------------------------------

/// One holiday in one calendar.
#[derive(Debug, Clone, Copy)]
pub struct HolidayEntry {
    /// A stable identifier, unique within its calendar — what a client branches
    /// on and what a future translation catalogue would key from.
    pub key: &'static str,
    /// The day's name in the country's own language(s). A proper noun, not a
    /// sentence we authored.
    pub name: &'static str,
    /// How its date is found.
    pub rule: HolidayRule,
    /// The first year it was observed, when it is newer than the seed's range —
    /// Poland's Christmas Eve, a public holiday from 2025.
    pub from_year: Option<i32>,
    /// The last year it was observed — Denmark's *store bededag*, abolished
    /// from 2024.
    pub until_year: Option<i32>,
}

impl HolidayEntry {
    /// Whether this day was observed in `year` at all.
    #[must_use]
    pub fn observed_in(&self, year: i32) -> bool {
        self.from_year.is_none_or(|first| year >= first)
            && self.until_year.is_none_or(|last| year <= last)
    }
}

/// One country's public holidays, and where the list comes from.
#[derive(Debug, Clone, Copy)]
pub struct HolidayCalendar {
    /// ISO 3166-1 alpha-2, uppercase — the calendar's code on the wire.
    pub code: &'static str,
    /// The instrument that sets these days, named so a tenant can check us.
    pub source: &'static str,
    /// What this calendar deliberately leaves out, in the country's own terms.
    /// Empty when there is nothing to warn about.
    pub note: &'static str,
    /// The days themselves.
    pub entries: &'static [HolidayEntry],
}

/// A holiday resolved to a date.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Holiday {
    /// The day it falls on.
    pub day: Date,
    /// The entry's stable key.
    pub key: &'static str,
    /// The day's name in the country's own language(s).
    pub name: &'static str,
}

impl HolidayCalendar {
    /// This calendar's holidays in `year`, earliest first.
    ///
    /// Lenient about the seed's reviewed range — a year outside it answers
    /// nothing rather than inventing days. Callers that must tell "none" from
    /// "not reviewed" ask [`holiday_year_covered`] first, which is what the
    /// leave-request path does.
    #[must_use]
    pub fn in_year(&self, year: i32) -> Vec<Holiday> {
        if !(HOLIDAY_FIRST_YEAR..=HOLIDAY_LAST_YEAR).contains(&year) {
            return Vec::new();
        }
        let mut days: Vec<Holiday> = self
            .entries
            .iter()
            .filter(|entry| entry.observed_in(year))
            .filter_map(|entry| {
                entry.rule.day_in(year).map(|day| Holiday {
                    day,
                    key: entry.key,
                    name: entry.name,
                })
            })
            .collect();
        days.sort_by_key(|holiday| holiday.day);
        days
    }

    /// This calendar's holidays between `from` and `to` inclusive, earliest
    /// first. Lenient in the same way [`HolidayCalendar::in_year`] is.
    #[must_use]
    pub fn between(&self, from: Date, to: Date) -> Vec<Holiday> {
        if to < from {
            return Vec::new();
        }
        (from.year()..=to.year())
            .flat_map(|year| self.in_year(year))
            .filter(|holiday| holiday.day >= from && holiday.day <= to)
            .collect()
    }
}

/// Every calendar the seed carries, in code order.
#[must_use]
pub fn holiday_calendars() -> &'static [HolidayCalendar] {
    CALENDARS
}

/// The calendar for a country code, in any case and with surrounding space.
#[must_use]
pub fn holiday_calendar(code: &str) -> Option<&'static HolidayCalendar> {
    let wanted = code.trim().to_ascii_uppercase();
    CALENDARS.iter().find(|calendar| calendar.code == wanted)
}

/// Refuses a year the seed has not been reviewed for, naming the range it has.
///
/// # Errors
/// [`StoreError::Validation`] when `year` is outside
/// [`HOLIDAY_FIRST_YEAR`]`..=`[`HOLIDAY_LAST_YEAR`].
pub fn holiday_year_covered(year: i32) -> Result<()> {
    if (HOLIDAY_FIRST_YEAR..=HOLIDAY_LAST_YEAR).contains(&year) {
        return Ok(());
    }
    Err(StoreError::Validation(format!(
        "public holidays are seeded for {HOLIDAY_FIRST_YEAR} to {HOLIDAY_LAST_YEAR}; {year} is not \
         covered yet"
    )))
}

// ---- the seed ---------------------------------------------------------------

/// An entry observed in every covered year.
const fn h(key: &'static str, name: &'static str, rule: HolidayRule) -> HolidayEntry {
    HolidayEntry {
        key,
        name,
        rule,
        from_year: None,
        until_year: None,
    }
}

/// An entry introduced in `first`.
const fn h_from(
    key: &'static str,
    name: &'static str,
    rule: HolidayRule,
    first: i32,
) -> HolidayEntry {
    HolidayEntry {
        key,
        name,
        rule,
        from_year: Some(first),
        until_year: None,
    }
}

/// An entry abolished after `last`.
const fn h_until(
    key: &'static str,
    name: &'static str,
    rule: HolidayRule,
    last: i32,
) -> HolidayEntry {
    HolidayEntry {
        key,
        name,
        rule,
        from_year: None,
        until_year: Some(last),
    }
}

/// The same date every year.
const fn fixed(month: Month, day: u8) -> HolidayRule {
    HolidayRule::Fixed { month, day }
}

/// `offset` days from Easter Sunday.
const fn easter(offset: i16) -> HolidayRule {
    HolidayRule::Easter { offset }
}

/// The first `weekday` on or after a date.
const fn from_day(month: Month, day: u8, weekday: Weekday) -> HolidayRule {
    HolidayRule::WeekdayOnOrAfter {
        month,
        day,
        weekday,
    }
}

const AT: &[HolidayEntry] = &[
    h("new_year", "Neujahr", fixed(Month::January, 1)),
    h("epiphany", "Heilige Drei Könige", fixed(Month::January, 6)),
    h("easter_monday", "Ostermontag", easter(EASTER_MONDAY)),
    h("labour_day", "Staatsfeiertag", fixed(Month::May, 1)),
    h("ascension", "Christi Himmelfahrt", easter(ASCENSION)),
    h("whit_monday", "Pfingstmontag", easter(WHIT_MONDAY)),
    h("corpus_christi", "Fronleichnam", easter(CORPUS_CHRISTI)),
    h("assumption", "Mariä Himmelfahrt", fixed(Month::August, 15)),
    h(
        "national_day",
        "Nationalfeiertag",
        fixed(Month::October, 26),
    ),
    h("all_saints", "Allerheiligen", fixed(Month::November, 1)),
    h(
        "immaculate_conception",
        "Mariä Empfängnis",
        fixed(Month::December, 8),
    ),
    h("christmas_day", "Christtag", fixed(Month::December, 25)),
    h("st_stephens_day", "Stefanitag", fixed(Month::December, 26)),
];

const BE: &[HolidayEntry] = &[
    h(
        "new_year",
        "Nieuwjaar / Nouvel An",
        fixed(Month::January, 1),
    ),
    h(
        "easter_monday",
        "Paasmaandag / Lundi de Pâques",
        easter(EASTER_MONDAY),
    ),
    h(
        "labour_day",
        "Dag van de Arbeid / Fête du Travail",
        fixed(Month::May, 1),
    ),
    h(
        "ascension",
        "O.L.H. Hemelvaart / Ascension",
        easter(ASCENSION),
    ),
    h(
        "whit_monday",
        "Pinkstermaandag / Lundi de Pentecôte",
        easter(WHIT_MONDAY),
    ),
    h(
        "national_day",
        "Nationale feestdag / Fête nationale",
        fixed(Month::July, 21),
    ),
    h(
        "assumption",
        "O.L.V. Hemelvaart / Assomption",
        fixed(Month::August, 15),
    ),
    h(
        "all_saints",
        "Allerheiligen / Toussaint",
        fixed(Month::November, 1),
    ),
    h(
        "armistice",
        "Wapenstilstand / Armistice",
        fixed(Month::November, 11),
    ),
    h(
        "christmas_day",
        "Kerstmis / Noël",
        fixed(Month::December, 25),
    ),
];

const DE: &[HolidayEntry] = &[
    h("new_year", "Neujahr", fixed(Month::January, 1)),
    h("good_friday", "Karfreitag", easter(GOOD_FRIDAY)),
    h("easter_monday", "Ostermontag", easter(EASTER_MONDAY)),
    h("labour_day", "Tag der Arbeit", fixed(Month::May, 1)),
    h("ascension", "Christi Himmelfahrt", easter(ASCENSION)),
    h("whit_monday", "Pfingstmontag", easter(WHIT_MONDAY)),
    h(
        "unity_day",
        "Tag der Deutschen Einheit",
        fixed(Month::October, 3),
    ),
    h(
        "christmas_day",
        "Erster Weihnachtstag",
        fixed(Month::December, 25),
    ),
    h(
        "boxing_day",
        "Zweiter Weihnachtstag",
        fixed(Month::December, 26),
    ),
];

const DK: &[HolidayEntry] = &[
    h("new_year", "Nytårsdag", fixed(Month::January, 1)),
    h("maundy_thursday", "Skærtorsdag", easter(MAUNDY_THURSDAY)),
    h("good_friday", "Langfredag", easter(GOOD_FRIDAY)),
    h("easter_sunday", "Påskedag", easter(EASTER_SUNDAY)),
    h("easter_monday", "2. påskedag", easter(EASTER_MONDAY)),
    h_until(
        "great_prayer_day",
        "Store bededag",
        easter(GREAT_PRAYER_DAY),
        2023,
    ),
    h("ascension", "Kristi himmelfartsdag", easter(ASCENSION)),
    h("whit_sunday", "Pinsedag", easter(WHIT_SUNDAY)),
    h("whit_monday", "2. pinsedag", easter(WHIT_MONDAY)),
    h("christmas_day", "Juledag", fixed(Month::December, 25)),
    h("boxing_day", "2. juledag", fixed(Month::December, 26)),
];

const ES: &[HolidayEntry] = &[
    h("new_year", "Año Nuevo", fixed(Month::January, 1)),
    h("epiphany", "Epifanía del Señor", fixed(Month::January, 6)),
    h("good_friday", "Viernes Santo", easter(GOOD_FRIDAY)),
    h("labour_day", "Fiesta del Trabajo", fixed(Month::May, 1)),
    h(
        "assumption",
        "Asunción de la Virgen",
        fixed(Month::August, 15),
    ),
    h(
        "national_day",
        "Fiesta Nacional de España",
        fixed(Month::October, 12),
    ),
    h("all_saints", "Todos los Santos", fixed(Month::November, 1)),
    h(
        "constitution_day",
        "Día de la Constitución",
        fixed(Month::December, 6),
    ),
    h(
        "immaculate_conception",
        "Inmaculada Concepción",
        fixed(Month::December, 8),
    ),
    h(
        "christmas_day",
        "Natividad del Señor",
        fixed(Month::December, 25),
    ),
];

const FI: &[HolidayEntry] = &[
    h("new_year", "Uudenvuodenpäivä", fixed(Month::January, 1)),
    h("epiphany", "Loppiainen", fixed(Month::January, 6)),
    h("good_friday", "Pitkäperjantai", easter(GOOD_FRIDAY)),
    h("easter_sunday", "Pääsiäispäivä", easter(EASTER_SUNDAY)),
    h("easter_monday", "2. pääsiäispäivä", easter(EASTER_MONDAY)),
    h("labour_day", "Vappu", fixed(Month::May, 1)),
    h("ascension", "Helatorstai", easter(ASCENSION)),
    h("whit_sunday", "Helluntaipäivä", easter(WHIT_SUNDAY)),
    h(
        "midsummer_eve",
        "Juhannusaatto",
        from_day(Month::June, 19, Weekday::Friday),
    ),
    h(
        "midsummer_day",
        "Juhannuspäivä",
        from_day(Month::June, 20, Weekday::Saturday),
    ),
    h(
        "all_saints",
        "Pyhäinpäivä",
        from_day(Month::October, 31, Weekday::Saturday),
    ),
    h(
        "independence_day",
        "Itsenäisyyspäivä",
        fixed(Month::December, 6),
    ),
    h("christmas_day", "Joulupäivä", fixed(Month::December, 25)),
    h("boxing_day", "Tapaninpäivä", fixed(Month::December, 26)),
];

const FR: &[HolidayEntry] = &[
    h("new_year", "Jour de l'An", fixed(Month::January, 1)),
    h("easter_monday", "Lundi de Pâques", easter(EASTER_MONDAY)),
    h("labour_day", "Fête du Travail", fixed(Month::May, 1)),
    h("victory_1945", "Victoire 1945", fixed(Month::May, 8)),
    h("ascension", "Ascension", easter(ASCENSION)),
    h("whit_monday", "Lundi de Pentecôte", easter(WHIT_MONDAY)),
    h("national_day", "Fête nationale", fixed(Month::July, 14)),
    h("assumption", "Assomption", fixed(Month::August, 15)),
    h("all_saints", "Toussaint", fixed(Month::November, 1)),
    h("armistice", "Armistice 1918", fixed(Month::November, 11)),
    h("christmas_day", "Noël", fixed(Month::December, 25)),
];

const IE: &[HolidayEntry] = &[
    h("new_year", "New Year's Day", fixed(Month::January, 1)),
    h_from(
        "st_brigids_day",
        "Lá Fhéile Bríde",
        HolidayRule::WeekdayOnOrAfterUnless {
            month: Month::February,
            day: 1,
            weekday: Weekday::Monday,
            unless: Weekday::Friday,
        },
        2023,
    ),
    h(
        "st_patricks_day",
        "Lá Fhéile Pádraig",
        fixed(Month::March, 17),
    ),
    h("easter_monday", "Easter Monday", easter(EASTER_MONDAY)),
    h(
        "may_holiday",
        "May Day",
        from_day(Month::May, 1, Weekday::Monday),
    ),
    h(
        "june_holiday",
        "June Holiday",
        from_day(Month::June, 1, Weekday::Monday),
    ),
    h(
        "august_holiday",
        "August Holiday",
        from_day(Month::August, 1, Weekday::Monday),
    ),
    h(
        "october_holiday",
        "October Holiday",
        from_day(Month::October, 25, Weekday::Monday),
    ),
    h("christmas_day", "Christmas Day", fixed(Month::December, 25)),
    h(
        "st_stephens_day",
        "Lá Fhéile Stiofáin",
        fixed(Month::December, 26),
    ),
];

const IT: &[HolidayEntry] = &[
    h("new_year", "Capodanno", fixed(Month::January, 1)),
    h("epiphany", "Epifania", fixed(Month::January, 6)),
    h("easter_sunday", "Pasqua", easter(EASTER_SUNDAY)),
    h("easter_monday", "Lunedì dell'Angelo", easter(EASTER_MONDAY)),
    h(
        "liberation_day",
        "Festa della Liberazione",
        fixed(Month::April, 25),
    ),
    h("labour_day", "Festa del Lavoro", fixed(Month::May, 1)),
    h(
        "republic_day",
        "Festa della Repubblica",
        fixed(Month::June, 2),
    ),
    h("assumption", "Ferragosto", fixed(Month::August, 15)),
    h("all_saints", "Ognissanti", fixed(Month::November, 1)),
    h(
        "immaculate_conception",
        "Immacolata Concezione",
        fixed(Month::December, 8),
    ),
    h("christmas_day", "Natale", fixed(Month::December, 25)),
    h(
        "st_stephens_day",
        "Santo Stefano",
        fixed(Month::December, 26),
    ),
];

const LU: &[HolidayEntry] = &[
    h(
        "new_year",
        "Neijoerschdag / Jour de l'An",
        fixed(Month::January, 1),
    ),
    h(
        "easter_monday",
        "Ouschterméindeg / Lundi de Pâques",
        easter(EASTER_MONDAY),
    ),
    h(
        "labour_day",
        "Dag vun der Aarbecht / Fête du Travail",
        fixed(Month::May, 1),
    ),
    h(
        "europe_day",
        "Europadag / Journée de l'Europe",
        fixed(Month::May, 9),
    ),
    h(
        "ascension",
        "Christi Himmelfaart / Ascension",
        easter(ASCENSION),
    ),
    h(
        "whit_monday",
        "Péngschtméindeg / Lundi de Pentecôte",
        easter(WHIT_MONDAY),
    ),
    h(
        "national_day",
        "Nationalfeierdag / Fête nationale",
        fixed(Month::June, 23),
    ),
    h(
        "assumption",
        "Mariä Himmelfaart / Assomption",
        fixed(Month::August, 15),
    ),
    h(
        "all_saints",
        "Allerhellgen / Toussaint",
        fixed(Month::November, 1),
    ),
    h(
        "christmas_day",
        "Chrëschtdag / Noël",
        fixed(Month::December, 25),
    ),
    h(
        "st_stephens_day",
        "Stiefesdag / Saint-Étienne",
        fixed(Month::December, 26),
    ),
];

const MT: &[HolidayEntry] = &[
    h("new_year", "L-Ewwel tas-Sena", fixed(Month::January, 1)),
    h(
        "st_pauls_shipwreck",
        "Nawfraġju ta' San Pawl",
        fixed(Month::February, 10),
    ),
    h("st_josephs_day", "San Ġużepp", fixed(Month::March, 19)),
    h("freedom_day", "Jum il-Ħelsien", fixed(Month::March, 31)),
    h("good_friday", "Il-Ġimgħa l-Kbira", easter(GOOD_FRIDAY)),
    h("labour_day", "Jum il-Ħaddiem", fixed(Month::May, 1)),
    h("sette_giugno", "Sette Giugno", fixed(Month::June, 7)),
    h("st_peter_and_st_paul", "L-Imnarja", fixed(Month::June, 29)),
    h("assumption", "Santa Marija", fixed(Month::August, 15)),
    h("victory_day", "Jum il-Vitorja", fixed(Month::September, 8)),
    h(
        "independence_day",
        "Jum l-Indipendenza",
        fixed(Month::September, 21),
    ),
    h(
        "immaculate_conception",
        "Il-Kunċizzjoni",
        fixed(Month::December, 8),
    ),
    h(
        "republic_day",
        "Jum ir-Repubblika",
        fixed(Month::December, 13),
    ),
    h("christmas_day", "Il-Milied", fixed(Month::December, 25)),
];

const NL: &[HolidayEntry] = &[
    h("new_year", "Nieuwjaarsdag", fixed(Month::January, 1)),
    h("easter_monday", "Tweede Paasdag", easter(EASTER_MONDAY)),
    h(
        "kings_day",
        "Koningsdag",
        HolidayRule::FixedOffSunday {
            month: Month::April,
            day: 27,
            shift_days: -1,
        },
    ),
    h("ascension", "Hemelvaartsdag", easter(ASCENSION)),
    h("whit_monday", "Tweede Pinksterdag", easter(WHIT_MONDAY)),
    h(
        "christmas_day",
        "Eerste Kerstdag",
        fixed(Month::December, 25),
    ),
    h("boxing_day", "Tweede Kerstdag", fixed(Month::December, 26)),
];

const PL: &[HolidayEntry] = &[
    h("new_year", "Nowy Rok", fixed(Month::January, 1)),
    h("epiphany", "Święto Trzech Króli", fixed(Month::January, 6)),
    h("easter_sunday", "Wielkanoc", easter(EASTER_SUNDAY)),
    h(
        "easter_monday",
        "Poniedziałek Wielkanocny",
        easter(EASTER_MONDAY),
    ),
    h("labour_day", "Święto Pracy", fixed(Month::May, 1)),
    h(
        "constitution_day",
        "Święto Narodowe Trzeciego Maja",
        fixed(Month::May, 3),
    ),
    h("whit_sunday", "Zielone Świątki", easter(WHIT_SUNDAY)),
    h("corpus_christi", "Boże Ciało", easter(CORPUS_CHRISTI)),
    h(
        "assumption",
        "Wniebowzięcie Najświętszej Maryi Panny",
        fixed(Month::August, 15),
    ),
    h(
        "all_saints",
        "Wszystkich Świętych",
        fixed(Month::November, 1),
    ),
    h(
        "independence_day",
        "Narodowe Święto Niepodległości",
        fixed(Month::November, 11),
    ),
    h_from(
        "christmas_eve",
        "Wigilia Bożego Narodzenia",
        fixed(Month::December, 24),
        2025,
    ),
    h(
        "christmas_day",
        "Boże Narodzenie (pierwszy dzień)",
        fixed(Month::December, 25),
    ),
    h(
        "boxing_day",
        "Boże Narodzenie (drugi dzień)",
        fixed(Month::December, 26),
    ),
];

const PT: &[HolidayEntry] = &[
    h("new_year", "Ano Novo", fixed(Month::January, 1)),
    h("good_friday", "Sexta-feira Santa", easter(GOOD_FRIDAY)),
    h("easter_sunday", "Páscoa", easter(EASTER_SUNDAY)),
    h("freedom_day", "Dia da Liberdade", fixed(Month::April, 25)),
    h("labour_day", "Dia do Trabalhador", fixed(Month::May, 1)),
    h("corpus_christi", "Corpo de Deus", easter(CORPUS_CHRISTI)),
    h("national_day", "Dia de Portugal", fixed(Month::June, 10)),
    h(
        "assumption",
        "Assunção de Nossa Senhora",
        fixed(Month::August, 15),
    ),
    h(
        "republic_day",
        "Implantação da República",
        fixed(Month::October, 5),
    ),
    h("all_saints", "Todos os Santos", fixed(Month::November, 1)),
    h(
        "restoration_day",
        "Restauração da Independência",
        fixed(Month::December, 1),
    ),
    h(
        "immaculate_conception",
        "Imaculada Conceição",
        fixed(Month::December, 8),
    ),
    h("christmas_day", "Natal", fixed(Month::December, 25)),
];

const SE: &[HolidayEntry] = &[
    h("new_year", "Nyårsdagen", fixed(Month::January, 1)),
    h("epiphany", "Trettondedag jul", fixed(Month::January, 6)),
    h("good_friday", "Långfredagen", easter(GOOD_FRIDAY)),
    h("easter_sunday", "Påskdagen", easter(EASTER_SUNDAY)),
    h("easter_monday", "Annandag påsk", easter(EASTER_MONDAY)),
    h("labour_day", "Första maj", fixed(Month::May, 1)),
    h("ascension", "Kristi himmelsfärdsdag", easter(ASCENSION)),
    h("whit_sunday", "Pingstdagen", easter(WHIT_SUNDAY)),
    h(
        "national_day",
        "Sveriges nationaldag",
        fixed(Month::June, 6),
    ),
    h(
        "midsummer_day",
        "Midsommardagen",
        from_day(Month::June, 20, Weekday::Saturday),
    ),
    h(
        "all_saints",
        "Alla helgons dag",
        from_day(Month::October, 31, Weekday::Saturday),
    ),
    h("christmas_day", "Juldagen", fixed(Month::December, 25)),
    h("boxing_day", "Annandag jul", fixed(Month::December, 26)),
];

/// The seed, in code order. Every country [`crate::hr_statutory_leave`] carries
/// a leave minimum for has a calendar here, so a tenant seeded with a statutory
/// entitlement can also be seeded with the days that entitlement is spent
/// around.
const CALENDARS: &[HolidayCalendar] = &[
    HolidayCalendar {
        code: "AT",
        source: "Arbeitsruhegesetz (ARG) § 7 Abs. 2 — die gesetzlichen Feiertage",
        note: "",
        entries: AT,
    },
    HolidayCalendar {
        code: "BE",
        source: "Koninklijk besluit van 18 april 1974 / Arrêté royal du 18 avril 1974 — de tien \
                 wettelijke feestdagen",
        note: "Een feestdag die op een zondag valt wordt vervangen door een dag die de werkgever \
               of de sector kiest; die vervangingsdag staat niet in deze tabel. / Un jour férié \
               tombant un dimanche est remplacé par un jour choisi par l'employeur ou le secteur, \
               que cette table ne connaît pas.",
        entries: BE,
    },
    HolidayCalendar {
        code: "DE",
        source: "Feiertagsgesetze der Länder — die neun bundesweit einheitlichen Tage",
        note: "Landesfeiertage (Fronleichnam, Allerheiligen, Reformationstag und weitere) sind \
               nicht enthalten: sie gelten je Bundesland.",
        entries: DE,
    },
    HolidayCalendar {
        code: "DK",
        source: "Lov om helligdage; store bededag afskaffet ved lov nr. 214 af 06/03/2023",
        note: "",
        entries: DK,
    },
    HolidayCalendar {
        code: "ES",
        source: "Estatuto de los Trabajadores art. 37.2 — calendario laboral estatal",
        note: "Las fiestas autonómicas y locales (dos días al año) no están incluidas: dependen de \
               la comunidad y del municipio.",
        entries: ES,
    },
    HolidayCalendar {
        code: "FI",
        source: "Laki eräistä juhlapäivistä (388/1937) sekä itsenäisyyspäivälaki (388/1937)",
        note: "Jouluaatto ei ole lakisääteinen vapaapäivä, vaikka se on useimmilla aloilla vapaa.",
        entries: FI,
    },
    HolidayCalendar {
        code: "FR",
        source: "Code du travail art. L3133-1 — les onze jours fériés légaux",
        note: "Les deux jours fériés supplémentaires d'Alsace-Moselle (Vendredi saint, 26 \
               décembre) ne sont pas inclus.",
        entries: FR,
    },
    HolidayCalendar {
        code: "IE",
        source: "Organisation of Working Time Act 1997 s.21 and Sch.2, as amended in 2022",
        note: "",
        entries: IE,
    },
    HolidayCalendar {
        code: "IT",
        source: "Legge 260/1949 e D.P.R. 792/1985 — le festività nazionali",
        note: "Il santo patrono, festivo in ogni comune, non è incluso.",
        entries: IT,
    },
    HolidayCalendar {
        code: "LU",
        source: "Code du travail art. L.232-2 — les jours fériés légaux",
        note: "",
        entries: LU,
    },
    HolidayCalendar {
        code: "MT",
        source: "National Holidays and Other Public Holidays Act (Cap. 252)",
        note: "",
        entries: MT,
    },
    HolidayCalendar {
        code: "NL",
        source: "Algemene termijnenwet art. 3 — de algemeen erkende feestdagen; of een dag \
                 doorbetaald vrij is volgt uit de cao",
        note: "Goede Vrijdag en Bevrijdingsdag staan wel in de wet maar zijn voor de meeste \
               werknemers een gewone werkdag; ze staan daarom niet in deze tabel. Wie ze wel vrij \
               heeft, zet ze in de cao-afspraken van de eigen organisatie.",
        entries: NL,
    },
    HolidayCalendar {
        code: "PL",
        source: "Ustawa z dnia 18 stycznia 1951 r. o dniach wolnych od pracy; Wigilia dodana \
                 ustawą z dnia 6 grudnia 2024 r. (od 2025)",
        note: "",
        entries: PL,
    },
    HolidayCalendar {
        code: "PT",
        source: "Código do Trabalho art. 234.º — feriados obrigatórios",
        note: "Os feriados municipais e o Carnaval não são obrigatórios e não estão incluídos.",
        entries: PT,
    },
    HolidayCalendar {
        code: "SE",
        source: "Lag (1989:253) om allmänna helgdagar",
        note: "",
        entries: SE,
    },
];

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn day(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).expect("a real date")
    }

    /// Easter Sunday for every covered year, against a published table.
    #[test]
    fn the_computus_matches_the_published_easter_dates() {
        let published = [
            (2020, Month::April, 12),
            (2021, Month::April, 4),
            (2022, Month::April, 17),
            (2023, Month::April, 9),
            (2024, Month::March, 31),
            (2025, Month::April, 20),
            (2026, Month::April, 5),
            (2027, Month::March, 28),
            (2028, Month::April, 16),
            (2029, Month::April, 1),
            (2030, Month::April, 21),
            (2031, Month::April, 13),
            (2032, Month::March, 28),
            (2033, Month::April, 17),
            (2034, Month::April, 9),
            (2035, Month::March, 25),
        ];
        for (year, month, number) in published {
            assert_eq!(
                easter_sunday(year).expect("an Easter"),
                day(year, month, number),
                "Easter {year}"
            );
        }
        // Easter is always a Sunday, and always between 22 March and 25 April.
        for year in HOLIDAY_FIRST_YEAR..=HOLIDAY_LAST_YEAR {
            let easter = easter_sunday(year).expect("an Easter");
            assert_eq!(easter.weekday(), Weekday::Sunday, "Easter {year}");
            assert!(easter >= day(year, Month::March, 22), "Easter {year} early");
            assert!(easter <= day(year, Month::April, 25), "Easter {year} late");
        }
    }

    /// Every calendar is well formed: canonical code, named source, unique keys,
    /// a plausible number of days, and never two holidays on one date.
    #[test]
    fn every_calendar_is_well_formed_in_every_covered_year() {
        assert_eq!(holiday_calendars().len(), 15);
        let mut codes = HashSet::new();
        for calendar in holiday_calendars() {
            assert!(codes.insert(calendar.code), "{} twice", calendar.code);
            assert_eq!(calendar.code.len(), 2, "{} is not alpha-2", calendar.code);
            assert_eq!(
                calendar.code.to_ascii_uppercase(),
                calendar.code,
                "{} is not canonical",
                calendar.code
            );
            assert!(
                !calendar.source.is_empty(),
                "{} names no source",
                calendar.code
            );
            let mut keys = HashSet::new();
            for entry in calendar.entries {
                assert!(
                    keys.insert(entry.key),
                    "{} carries {} twice",
                    calendar.code,
                    entry.key
                );
                assert!(!entry.name.is_empty(), "{} has no name", entry.key);
            }
            for year in HOLIDAY_FIRST_YEAR..=HOLIDAY_LAST_YEAR {
                let days = calendar.in_year(year);
                assert!(
                    (7..=16).contains(&days.len()),
                    "{} has {} days in {year}",
                    calendar.code,
                    days.len()
                );
                // Two names on one date is a real thing — Luxembourg's Europe
                // Day fell on Ascension in 2024 — but only when a movable feast
                // meets a fixed date. Two *fixed* days on one date would be a
                // typo in this table, and that is what this catches.
                let mut seen: HashSet<Date> = HashSet::new();
                for holiday in &days {
                    assert_eq!(holiday.day.year(), year, "{} strayed", calendar.code);
                    if !seen.insert(holiday.day) {
                        let sharing: Vec<&HolidayEntry> = calendar
                            .entries
                            .iter()
                            .filter(|entry| entry.rule.day_in(year) == Some(holiday.day))
                            .collect();
                        assert!(
                            sharing
                                .iter()
                                .any(|entry| matches!(entry.rule, HolidayRule::Easter { .. })),
                            "{} has two fixed holidays on {}: {:?}",
                            calendar.code,
                            holiday.day,
                            sharing.iter().map(|entry| entry.key).collect::<Vec<_>>()
                        );
                    }
                }
                assert!(days.windows(2).all(|pair| pair[0].day <= pair[1].day));
            }
        }
    }

    /// The fixed and movable days a reader can check against a wall calendar.
    #[test]
    fn the_dates_are_the_dates_a_wall_calendar_shows() {
        let fr = holiday_calendar("fr").expect("France");
        let fr_2026 = fr.in_year(2026);
        let bastille = fr_2026
            .iter()
            .find(|holiday| holiday.key == "national_day")
            .expect("14 July");
        assert_eq!(bastille.day, day(2026, Month::July, 14));
        // Easter 2026 is 5 April, so Easter Monday is the 6th, Ascension the
        // 14th of May and Whit Monday the 25th.
        let by_key = |list: &[Holiday], key: &str| {
            list.iter()
                .find(|holiday| holiday.key == key)
                .expect("the day")
                .day
        };
        assert_eq!(
            by_key(&fr_2026, "easter_monday"),
            day(2026, Month::April, 6)
        );
        assert_eq!(by_key(&fr_2026, "ascension"), day(2026, Month::May, 14));
        assert_eq!(by_key(&fr_2026, "whit_monday"), day(2026, Month::May, 25));

        // Ireland's holidays are Mondays by construction: the first in May, the
        // last in October.
        let ie_2026 = holiday_calendar("IE").expect("Ireland").in_year(2026);
        assert_eq!(by_key(&ie_2026, "may_holiday"), day(2026, Month::May, 4));
        assert_eq!(
            by_key(&ie_2026, "october_holiday"),
            day(2026, Month::October, 26)
        );
        for key in ["may_holiday", "june_holiday", "august_holiday"] {
            assert_eq!(by_key(&ie_2026, key).weekday(), Weekday::Monday);
        }
        // St Brigid's Day: the first Monday in February, unless 1 February is a
        // Friday — 2030, when the day itself is the holiday.
        let brigid = |year: i32| {
            by_key(
                &holiday_calendar("IE").expect("Ireland").in_year(year),
                "st_brigids_day",
            )
        };
        assert_eq!(brigid(2026), day(2026, Month::February, 2));
        assert_eq!(day(2030, Month::February, 1).weekday(), Weekday::Friday);
        assert_eq!(brigid(2030), day(2030, Month::February, 1));

        // Sweden's Midsummer Day and All Saints' Day are Saturdays in a window.
        let se_2026 = holiday_calendar("SE").expect("Sweden").in_year(2026);
        assert_eq!(
            by_key(&se_2026, "midsummer_day"),
            day(2026, Month::June, 20)
        );
        assert_eq!(by_key(&se_2026, "all_saints").weekday(), Weekday::Saturday);
    }

    /// King's Day steps back to the Saturday when 27 April is a Sunday — 2025,
    /// and not 2026.
    #[test]
    fn kings_day_steps_off_a_sunday() {
        let kings = |year: i32| {
            holiday_calendar("NL")
                .expect("the Netherlands")
                .in_year(year)
                .into_iter()
                .find(|holiday| holiday.key == "kings_day")
                .expect("Koningsdag")
                .day
        };
        assert_eq!(day(2025, Month::April, 27).weekday(), Weekday::Sunday);
        assert_eq!(kings(2025), day(2025, Month::April, 26));
        assert_eq!(kings(2026), day(2026, Month::April, 27));
    }

    /// A day the law added or removed appears and disappears on the right year.
    #[test]
    fn a_day_the_law_changed_starts_and_stops() {
        let has = |code: &str, year: i32, key: &str| {
            holiday_calendar(code)
                .expect("a calendar")
                .in_year(year)
                .iter()
                .any(|holiday| holiday.key == key)
        };
        // Denmark abolished store bededag from 2024.
        assert!(has("DK", 2023, "great_prayer_day"));
        assert!(!has("DK", 2024, "great_prayer_day"));
        // Poland made Christmas Eve a public holiday from 2025.
        assert!(!has("PL", 2024, "christmas_eve"));
        assert!(has("PL", 2025, "christmas_eve"));
        // Ireland's St Brigid's Day is new in 2023.
        assert!(!has("IE", 2022, "st_brigids_day"));
        assert!(has("IE", 2023, "st_brigids_day"));
    }

    /// A year the seed has not been reviewed for is refused by name, and answers
    /// nothing rather than inventing days.
    #[test]
    fn a_year_outside_the_reviewed_range_is_refused_not_guessed() {
        assert!(holiday_year_covered(2026).is_ok());
        assert!(holiday_year_covered(HOLIDAY_FIRST_YEAR).is_ok());
        assert!(holiday_year_covered(HOLIDAY_LAST_YEAR).is_ok());
        let refusal = format!(
            "{:?}",
            holiday_year_covered(HOLIDAY_LAST_YEAR + 1).unwrap_err()
        );
        assert!(refusal.contains("2036"), "{refusal}");
        assert!(refusal.contains("2035"), "{refusal}");
        assert!(
            holiday_calendar("BE")
                .expect("Belgium")
                .in_year(HOLIDAY_LAST_YEAR + 1)
                .is_empty()
        );
        assert!(
            holiday_calendar("BE")
                .expect("Belgium")
                .in_year(HOLIDAY_FIRST_YEAR - 1)
                .is_empty()
        );
    }

    /// A range answers the days inside it, across a year boundary, and nothing
    /// for a backwards range.
    #[test]
    fn a_range_answers_the_days_inside_it() {
        let be = holiday_calendar("BE").expect("Belgium");
        let christmas = be.between(day(2026, Month::December, 20), day(2027, Month::January, 5));
        assert_eq!(christmas.len(), 2, "{christmas:?}");
        assert_eq!(christmas[0].day, day(2026, Month::December, 25));
        assert_eq!(christmas[1].day, day(2027, Month::January, 1));
        assert!(
            be.between(day(2026, Month::March, 5), day(2026, Month::March, 1))
                .is_empty()
        );
        // The whole year's list is the year's holidays, in order.
        assert_eq!(
            be.between(day(2026, Month::January, 1), day(2026, Month::December, 31))
                .len(),
            be.in_year(2026).len()
        );
    }

    /// An unknown code is `None`, never a guess.
    #[test]
    fn an_unknown_calendar_is_not_invented() {
        assert!(holiday_calendar("ZZ").is_none());
        assert!(holiday_calendar("").is_none());
        assert!(holiday_calendar("belgium").is_none());
        assert!(holiday_calendar(" be ").is_some(), "trimmed and folded");
    }

    /// Every country with a statutory leave figure has a calendar, so the two
    /// seeds never disagree about which member states we carry.
    #[test]
    fn every_country_with_a_leave_minimum_has_a_calendar() {
        for country in [
            "AT", "BE", "DE", "DK", "ES", "FI", "FR", "IE", "IT", "LU", "MT", "NL", "PL", "PT",
            "SE",
        ] {
            assert!(
                holiday_calendar(country).is_some(),
                "{country} has no calendar"
            );
        }
    }
}
