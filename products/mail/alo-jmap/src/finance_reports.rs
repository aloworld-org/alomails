//! What every finance report's door has in common (alo Finance, ADR 0035, wave
//! B4.11) — the gate, the dates a report is asked for, and the one rule about
//! text a spreadsheet will open.
//!
//! The reports themselves live one file each ([`crate::finance_report_pl`],
//! [`crate::finance_report_balance`], and the two B4.11c/d add), because each is
//! its own reason to change: a column added to the P&L is not a change to the
//! balance sheet, and a file that held both would be edited for either. What is
//! *shared* is here, once, so four reports cannot drift into four spellings of
//! the same refusal.
//!
//! Each report is served twice — `GET …/x` answers JSON for the screen and
//! `GET …/x.csv` answers the same figures as a file for the accountant.
//! Separate paths rather than one route with a `?format=`, exactly as `/print`
//! and `/pdf` are, because a URL that names its representation is the one a
//! browser saves under a sensible name and a script quotes without a query
//! string. Both call the same store function, so the file and the screen cannot
//! disagree about a cent.
//!
//! Four decisions this file makes rather than the store.
//!
//! - **The dates are stated, never guessed.** Every bound a report takes is
//!   required, is a plain day, and a missing or malformed one is a `422`. A
//!   report that quietly defaulted to "this quarter" or "today" would put a
//!   figure under a heading nobody asked for, which is the one thing a document
//!   copied into a year-end must not do. Rules about a *pair* of dates (a period
//!   may not end before it starts) belong to the store, so the two doors into it
//!   cannot drift.
//! - **Admin only, for now.** A finance report is the whole tenant's position,
//!   not the reader's own work: unlike `GET /finance/periods` (which any member
//!   may read, because knowing the books are shut stops them typing into them),
//!   this is the figure a company keeps to its officers. B4.12's accountant role
//!   widens the gate **additively** — a decision recorded in
//!   `docs/design/finance.md`, not re-taken here.
//! - **The CSV columns are a contract, in English.** They are read by scripts
//!   and by an accountant's own tooling, so they do not move with the reader's
//!   interface language; what a *person* reads is the screen, which is
//!   translated. Amounts are plain decimals with a `.` separator and no
//!   grouping, ISO dates, one row per line plus the totals.
//! - **Account names are neutralised** ([`text`]). They are the first
//!   user-authored text alo exports to a spreadsheet, and a name beginning with
//!   `=` is a formula in Excel and LibreOffice. `crate::csv` deliberately does
//!   not do this for every field — a negative amount begins with `-` and must
//!   stay a number — so the rule is stated here, where the text is chosen.
//!
//! Nothing in these reports is personal data: an account code, a name a tenant
//! gave their own chart, and money.

use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use time::Date;

use crate::billing::parse_iso_date;
use crate::error::Problem;
use crate::state::{Account, AppState, authenticate};

/// The period a report is asked for. Both ends are required and inclusive.
#[derive(Deserialize)]
pub struct PeriodQuery {
    #[serde(default)]
    from: Option<String>,
    #[serde(default)]
    to: Option<String>,
}

impl PeriodQuery {
    /// The two days this query names, or the `422` that says which end is
    /// wrong.
    ///
    /// The store owns the one rule about the *pair* (a period may not end
    /// before it starts), so the two doors into it cannot drift; what is
    /// checked here is only what a query string can get wrong on its own.
    ///
    /// # Errors
    /// [`Problem`] with `422` when either end is missing or is not a plain day.
    pub fn days(&self) -> Result<(Date, Date), Problem> {
        Ok((
            day("from", self.from.as_deref())?,
            day("to", self.to.as_deref())?,
        ))
    }
}

/// The single day a cumulative report is asked for — a balance sheet stands on
/// one date, not on a window, and the query string says which.
#[derive(Deserialize)]
pub struct OnQuery {
    #[serde(default)]
    on: Option<String>,
}

impl OnQuery {
    /// The day this query names, or the `422` that says why it is not one.
    ///
    /// # Errors
    /// [`Problem`] with `422` when `on` is missing or is not a plain day.
    pub fn day(&self) -> Result<Date, Problem> {
        day("on", self.on.as_deref())
    }
}

/// Reads one bound of a report, naming it in every refusal so a caller with
/// two wrong learns which one it is looking at.
fn day(name: &str, raw: Option<&str>) -> Result<Date, Problem> {
    let raw = raw
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("{name} is required: a report is always for a stated period"),
            )
        })?;
    parse_iso_date(raw).ok_or_else(|| {
        Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("{name} must be a date of the form YYYY-MM-DD"),
        )
    })
}

/// The one gate every finance report is behind: a valid token, and an admin
/// holding it.
///
/// One function rather than two lines in each handler, so a report added later
/// cannot be the one that forgot — and so B4.12 widens the gate in a single
/// place.
///
/// # Errors
/// [`Problem`] with `401` without a valid bearer token, `403` for a member who
/// is not an admin.
pub async fn admin(state: &AppState, headers: &HeaderMap) -> Result<Account, Problem> {
    let account = authenticate(state, headers).await?;
    account.require_admin()?;
    Ok(account)
}

/// User-authored text, made safe for a spreadsheet.
///
/// A field beginning with `=`, `+`, `-`, `@`, a tab or a carriage return is
/// evaluated as a formula by Excel and LibreOffice, and an account somebody
/// named `=cmd|…` would be a command in a file they emailed their accountant.
/// A leading apostrophe is the neutralisation both of them understand: the
/// cell shows the text, and nothing is computed. Amounts never come through
/// here — a negative one begins with `-` and must stay a number.
pub fn text(value: &str) -> String {
    if value.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{value}")
    } else {
        value.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Month;

    fn on(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).unwrap_or_else(|e| panic!("{e}"))
    }

    fn period(from: Option<&str>, to: Option<&str>) -> PeriodQuery {
        PeriodQuery {
            from: from.map(str::to_owned),
            to: to.map(str::to_owned),
        }
    }

    fn at(on: Option<&str>) -> OnQuery {
        OnQuery {
            on: on.map(str::to_owned),
        }
    }

    #[test]
    fn both_ends_of_a_period_are_required() {
        for (from, to, expected) in [
            (None, Some("2026-12-31"), "from"),
            (Some("2026-01-01"), None, "to"),
            (Some(""), Some("2026-12-31"), "from"),
            (Some("2026-01-01"), Some("   "), "to"),
        ] {
            let problem = period(from, to)
                .days()
                .err()
                .unwrap_or_else(|| panic!("{from:?}/{to:?} should have been refused"));
            assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY);
            let detail = problem.detail.unwrap_or_default();
            assert!(detail.starts_with(expected), "{detail}");
            assert!(detail.contains("required"), "{detail}");
        }
    }

    #[test]
    fn the_date_of_a_cumulative_report_is_required_too() {
        for missing in [None, Some(""), Some("  ")] {
            let problem = at(missing)
                .day()
                .err()
                .unwrap_or_else(|| panic!("{missing:?} should have been refused"));
            assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY);
            assert_eq!(
                problem.detail.as_deref(),
                Some("on is required: a report is always for a stated period")
            );
        }
    }

    #[test]
    fn a_bound_that_is_not_a_plain_day_is_refused_never_guessed_at() {
        for bad in [
            "01/01/2026",
            "2026-13-01",
            "2026-01-01T00:00:00Z",
            "January",
        ] {
            let problem = period(Some(bad), Some("2026-12-31"))
                .days()
                .err()
                .unwrap_or_else(|| panic!("{bad:?} should have been refused"));
            assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY, "{bad:?}");
            assert_eq!(
                problem.detail.as_deref(),
                Some("from must be a date of the form YYYY-MM-DD")
            );

            let problem = at(Some(bad))
                .day()
                .err()
                .unwrap_or_else(|| panic!("{bad:?} should have been refused"));
            assert_eq!(
                problem.detail.as_deref(),
                Some("on must be a date of the form YYYY-MM-DD")
            );
        }
    }

    #[test]
    fn a_well_formed_bound_is_read_as_a_plain_day() {
        let (from, to) = period(Some("2026-01-01"), Some(" 2026-12-31 "))
            .days()
            .unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(from, on(2026, Month::January, 1));
        assert_eq!(to, on(2026, Month::December, 31));
        // A backwards period is the store's refusal, not this layer's: it is a
        // rule about the pair, and one place must own it.
        assert!(
            period(Some("2026-12-31"), Some("2026-01-01"))
                .days()
                .is_ok()
        );

        assert_eq!(
            at(Some(" 2026-12-31 "))
                .day()
                .unwrap_or_else(|e| panic!("{e:?}")),
            on(2026, Month::December, 31)
        );
    }

    #[test]
    fn a_name_that_would_be_a_formula_is_neutralised_and_nothing_else_is() {
        assert_eq!(text("Sales"), "Sales");
        assert_eq!(text(""), "");
        assert_eq!(text("=1+1"), "'=1+1");
        assert_eq!(text("@home"), "'@home");
        assert_eq!(text("-cmd|'/c calc'!A1"), "'-cmd|'/c calc'!A1");
        assert_eq!(text("\tTravel"), "'\tTravel");
    }
}
