//! Public holidays over HTTP (alo HR, ADR 0035, wave B6.04) — over
//! [`alo_store::hr_holidays`].
//!
//! Two surfaces, and one door decision between them:
//!
//! - `GET /hr/holidays?calendar=&year=` — the days themselves. **Every member's
//!   read.** Whether 21 July is free is not a personnel matter, and the screen
//!   that shows somebody their leave has to draw the days their request will not
//!   be charged for.
//! - `GET`/`PUT /hr/holiday-calendars` — which calendars this company observes,
//!   and which one its leave arithmetic uses. **Reading is every member's,
//!   writing is HR's** — the same correction B6.03b made to the design's route
//!   table for leave policies, for the same reason: what a company observes is a
//!   rule it publishes to its staff, and an employee whose Christmas week costs
//!   four days is entitled to know why.
//!
//! Nothing here carries personal data: a calendar is a fact about a country and
//! a choice about a company, so every refusal may name what was wrong with it.

use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::hr_holiday_seed::{
    HOLIDAY_FIRST_YEAR, HOLIDAY_LAST_YEAR, HolidayCalendar, holiday_calendar, holiday_calendars,
    holiday_year_covered,
};
use alo_store::{HolidaySelection, TenantHolidays};

use crate::billing::{iso, map_store_err, parse_body};
use crate::billing_document::today;
use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// A calendar as JSON — its code, the instrument behind it, and what it
/// deliberately leaves out.
fn calendar_json(calendar: &HolidayCalendar) -> Value {
    json!({
        "code": calendar.code,
        "source": calendar.source,
        "note": calendar.note,
    })
}

/// A tenant's choice as JSON, beside every calendar it could have chosen — one
/// read is enough to draw the whole screen.
fn selection_json(selection: &HolidaySelection, is_hr: bool) -> Value {
    json!({
        "calendars": selection.calendars,
        "defaultCalendar": selection.default_calendar,
        "chosenBy": selection.chosen_by,
        "updatedAt": iso(selection.updated_at),
        "available": holiday_calendars().iter().map(calendar_json).collect::<Vec<_>>(),
        "firstYear": HOLIDAY_FIRST_YEAR,
        "lastYear": HOLIDAY_LAST_YEAR,
        "hr": is_hr,
    })
}

/// Query string of the days route.
#[derive(Deserialize)]
pub struct DaysQuery {
    /// The calendar to read. Absent means the company's own default.
    #[serde(default)]
    calendar: Option<String>,
    /// The year to read. Absent means this year — the ordinary call from a
    /// screen showing the current leave year.
    #[serde(default)]
    year: Option<i32>,
}

/// `GET /hr/holidays?calendar=BE&year=2026` →
/// `{"calendar":{…},"year":2026,"holidays":[{"date","key","name"}]}`
///
/// The default calendar is the company's own, so the ordinary call is
/// `?year=2026`. A company that observes nothing answers `"calendar": null` and
/// an empty list rather than an error: observing nothing is a valid choice, and
/// the screen says so.
///
/// # Errors
/// `401` without a valid bearer token; `422` for a calendar we do not carry or a
/// year the seed has not been reviewed for — the two must not look alike, which
/// is why an uncovered year is refused rather than answered with nothing.
pub async fn list_holidays(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<DaysQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let hr = state.store.for_tenant(account.tenant.clone());
    let observed = match q
        .calendar
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())
    {
        Some(code) => {
            let calendar = holiday_calendar(code).ok_or_else(|| {
                Problem::with(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    format!(
                        "there is no public-holiday calendar for {code}; the ones we carry are {}",
                        holiday_calendars()
                            .iter()
                            .map(|calendar| calendar.code)
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                )
            })?;
            TenantHolidays::for_calendar(calendar.code)
        }
        None => hr.hr_holidays().await.map_err(map_store_err)?,
    };
    let year = q.year.unwrap_or_else(|| today().year());
    holiday_year_covered(year).map_err(map_store_err)?;
    let calendar = observed.calendar_code().and_then(holiday_calendar);
    let holidays = calendar
        .map(|calendar| calendar.in_year(year))
        .unwrap_or_default();
    Ok(Json(json!({
        "calendar": calendar.map(calendar_json),
        "year": year,
        "holidays": holidays.iter().map(|holiday| json!({
            "date": holiday.day.to_string(),
            "key": holiday.key,
            "name": holiday.name,
        })).collect::<Vec<_>>(),
    })))
}

/// `GET /hr/holiday-calendars` → the choice, and everything it could be.
///
/// **Seeds on first read**, from the country the company invoices under: a
/// Belgian company observes the Belgian calendar without pressing anything, the
/// same way it gets a Belgian statutory leave policy. A country the seed does
/// not carry produces an explicit empty choice, so the screen can say "we do not
/// carry your country's calendar yet" rather than showing nothing.
///
/// # Errors
/// `401` without a valid bearer token.
pub async fn get_calendars(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let is_hr = account.require_hr().is_ok();
    let hr = state.store.for_tenant(account.tenant.clone());
    let selection = hr
        .ensure_hr_holiday_selection(&account.user)
        .await
        .map_err(map_store_err)?;
    Ok(Json(selection_json(&selection, is_hr)))
}

/// The body of the choice.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CalendarsBody {
    /// The calendars to observe. An empty list is a real choice: observe none.
    calendars: Vec<String>,
    /// Which of them the leave arithmetic uses. Absent means the first.
    #[serde(default)]
    default_calendar: Option<String>,
}

/// `PUT /hr/holiday-calendars` `{"calendars":["BE","NL"],"defaultCalendar":"BE"}`
/// → the choice as `GET` returns it — **HR only**.
///
/// A `PUT` rather than a `POST` because the choice is one value that is replaced
/// whole: sending two calendars means the company observes exactly those two.
///
/// Changing the default changes what leave costs from that moment — balances are
/// folded, never stored, so a company that adds its calendar in March sees
/// January's approved leave recomputed with January's holidays in it. That is
/// the correct answer and a surprising one, so the screen says it before it
/// saves.
///
/// # Errors
/// `401`/`403` per the HR door; `422` for a calendar we do not carry, more than
/// ten of them, or a default that is not among those observed.
pub async fn put_calendars(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_hr()?;
    let req: CalendarsBody = parse_body(&body)?;
    let hr = state.store.for_tenant(account.tenant.clone());
    let selection = hr
        .set_hr_holiday_selection(
            &req.calendars,
            req.default_calendar.as_deref(),
            &account.user,
        )
        .await
        .map_err(map_store_err)?;
    Ok(Json(selection_json(&selection, true)))
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    fn body(value: Value) -> CalendarsBody {
        serde_json::from_value(value).unwrap_or_else(|e| panic!("body rejected: {e}"))
    }

    #[test]
    fn a_choice_of_none_is_a_choice() {
        let empty = body(json!({ "calendars": [] }));
        assert!(empty.calendars.is_empty());
        assert!(empty.default_calendar.is_none());
    }

    #[test]
    fn the_default_is_optional_and_read_when_given() {
        let one = body(json!({ "calendars": ["BE"] }));
        assert_eq!(one.calendars, vec!["BE".to_owned()]);
        assert!(one.default_calendar.is_none(), "the first is the default");
        let two = body(json!({ "calendars": ["BE", "NL"], "defaultCalendar": "NL" }));
        assert_eq!(two.default_calendar.as_deref(), Some("NL"));
    }

    /// The seed's own vocabulary, as the wire spells it — a client may branch on
    /// these codes.
    #[test]
    fn every_calendar_is_offered_with_its_source() {
        for calendar in holiday_calendars() {
            let json = calendar_json(calendar);
            assert_eq!(json["code"], calendar.code);
            assert!(!json["source"].as_str().unwrap_or_default().is_empty());
        }
    }
}
