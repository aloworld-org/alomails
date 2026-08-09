//! The profitability report's HTTP surface (alo Projects, ADR 0035, wave
//! B3.08) — hours × rates against a budget, over
//! [`alo_store::time_report`].
//!
//! Two representations of one read, the shape billing's VAT summary settled on
//! and CRM's pipeline report confirmed: `GET /projects/reports/profitability`
//! answers JSON for the screen and `…/profitability.csv` answers the same
//! figures as a file, both from the same store call so the file and the screen
//! cannot disagree about a cent.
//!
//! What this file owns, and nothing more:
//!
//! - **The period is stated, never guessed.** `from` and `to` are required plain
//!   days; a missing or malformed one is a `422` naming which. The optional
//!   `projectId` narrows the report to one engagement.
//! - **Nothing here computes money.** Every amount is the store's integer
//!   cents, folded through the same code a billing line uses. No float appears
//!   on this path, and no currency is added to another.
//! - **The CSV columns are a contract, in English**, read by scripts and by an
//!   accountant's own spreadsheet; what a person reads is the screen, which is
//!   translated.
//! - **The file names no customer and no person.** A project name, a currency,
//!   minutes and amounts — the report is a project aggregate, and who worked
//!   which hour is personal data that never reaches this surface
//!   (`docs/design/projects.md` § The hours of a person are personal data).
//!
//! The JSON carries the customer's **id** — the screen already holds the
//! customer list and resolves the name itself, and an id is not contact data.
//! The CSV carries neither.

use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use serde::Deserialize;
use serde_json::{Value, json};
use time::Date;

use alo_store::time_report::{
    ProfitabilityCurrency, ProfitabilityReport, ProfitabilityTotals, ProjectProfitability,
    profitability_totals,
};
use alo_store::{AccountStore, ProjectId};

use crate::billing::{iso_date, map_store_err, parse_iso_date};
// The same integer-only split into units and hundredths the e-invoice
// amounts use — a CSV cell and an XML amount are both a machine's number, and
// a fourth private copy of the conversion is a fourth place for it to drift.
use crate::billing_xml::amount;
use crate::csv;
use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// The period a report is asked for, and optionally the one engagement it is
/// about.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfitabilityQuery {
    #[serde(default)]
    from: Option<String>,
    #[serde(default)]
    to: Option<String>,
    /// One engagement, or absent for every engagement this caller can see.
    #[serde(default)]
    project_id: Option<String>,
}

impl ProfitabilityQuery {
    /// The two days and the optional engagement this query names, or the `422`
    /// that says which field is wrong.
    ///
    /// Whether the project is *this tenant's* is the store's answer, and it is
    /// a `404` — the same answer an id that never existed gets, so this is not
    /// an existence oracle.
    fn read(&self) -> Result<(Date, Date, Option<ProjectId>), Problem> {
        Ok((
            day("from", self.from.as_deref())?,
            day("to", self.to.as_deref())?,
            self.project_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| ProjectId::new(value.to_owned())),
        ))
    }
}

/// Reads one end of the period, naming it in every refusal so a caller with
/// both wrong learns which one it is looking at.
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

/// What one currency's rated hours are worth.
///
/// `unbilledNetCents` is the store's own subtraction rather than a sum the
/// client is invited to make: it is the figure somebody chases at the end of a
/// month, and a browser that computed it could disagree with the server.
fn currency_json(row: &ProfitabilityCurrency) -> Value {
    json!({
        "currency": row.currency,
        "billableMinutes": row.billable_minutes,
        "netCents": row.net_cents,
        "billedMinutes": row.billed_minutes,
        "billedNetCents": row.billed_net_cents,
        "unbilledNetCents": row.unbilled_net_cents(),
    })
}

/// One engagement's row.
///
/// The consumption figures are `null` when the engagement carries no budget —
/// a proportion of nothing is unanswered, not zero, and a client that saw `0`
/// there would draw an empty bar meaning "none of it used".
fn project_json(project: &ProjectProfitability) -> Value {
    json!({
        "projectId": project.project_id.as_str(),
        "projectName": project.project_name,
        "customerId": project.customer_id.as_str(),
        "currency": project.currency,
        "budgetMinutes": project.budget_minutes,
        "budgetCents": project.budget_cents,
        "minutes": project.minutes,
        "billableMinutes": project.billable_minutes,
        "unratedMinutes": project.unrated_minutes,
        "byCurrency": project.by_currency.iter().map(currency_json).collect::<Vec<_>>(),
        "toDateMinutes": project.to_date_minutes,
        "toDateNetCents": project.to_date_net_cents,
        "hoursConsumptionBp": project.hours_consumption_bp(),
        "budgetConsumptionBp": project.budget_consumption_bp(),
        "budgetRemainingCents": project.budget_remaining_cents(),
    })
}

/// What the whole report adds up to — one row per currency, and never a grand
/// total across them.
fn totals_json(totals: &ProfitabilityTotals) -> Value {
    json!({
        "minutes": totals.minutes,
        "billableMinutes": totals.billable_minutes,
        "unratedMinutes": totals.unrated_minutes,
        "byCurrency": totals.by_currency.iter().map(currency_json).collect::<Vec<_>>(),
    })
}

/// The whole report, with the period it covers written into it — a figure a
/// person reads has to say what it is about.
///
/// The period bounds the *work*; the to-date figures on each engagement are
/// taken at `to`, because a budget is consumed by everything spent so far and
/// not by one quarter of it.
fn report_json(report: &ProfitabilityReport, totals: &ProfitabilityTotals) -> Value {
    json!({
        "from": iso_date(report.from),
        "to": iso_date(report.to),
        "projects": report.projects.iter().map(project_json).collect::<Vec<_>>(),
        "totals": totals_json(totals),
    })
}

/// A proportion in basis points as the percentage a reader expects
/// (1200 → `12.00`), or an empty cell when there is no proportion to state.
fn percent(bp: Option<i64>) -> String {
    let Some(bp) = bp else {
        return String::new();
    };
    let sign = if bp < 0 { "-" } else { "" };
    let magnitude = i128::from(bp).unsigned_abs();
    format!("{sign}{}.{:02}", magnitude / 100, magnitude % 100)
}

/// An optional integer as a cell: the figure, or empty when it is absent.
/// Never `0` — a budget nobody set is not a budget of nothing.
fn count(value: Option<i64>) -> String {
    value.map(|v| v.to_string()).unwrap_or_default()
}

/// An optional amount as a cell, on the same rule.
fn money(cents: Option<i64>) -> String {
    cents.map(amount).unwrap_or_default()
}

/// The CSV column names — a contract, deliberately not translated.
const COLUMNS: [&str; 17] = [
    // Which kind of row you are reading — one table rather than three files:
    //   `hours` one engagement's time and its budget, in the engagement's own
    //           currency (the currency its money budget is stated in)
    //   `value` what one engagement's hours are worth in ONE currency; an
    //           engagement priced in two answers two of these
    //   `totalHours` / `totalValue` the same two shapes over the whole report
    "row",
    "project",
    "periodFrom",
    "periodTo",
    "currency",
    // The period's time. `unratedMinutes` are chargeable hours carrying no
    // rate: counted here and priced in no column, because an hour is never
    // valued at a price nobody set.
    "minutes",
    "billableMinutes",
    "unratedMinutes",
    // The period's money, in the row's currency.
    "value",
    "billed",
    "unbilled",
    // Everything up to and including `periodTo` — what the budget is consumed
    // by, and why a report of one quarter still knows the engagement is at 80%.
    "toDateMinutes",
    "toDateValue",
    "budgetMinutes",
    "budgetValue",
    "hoursUsedPercent",
    "budgetUsedPercent",
];

/// The whole report as one CSV table.
///
/// The period is repeated on every row on purpose: a row lifted into another
/// sheet still says what it is about. A cell is left **empty** rather than
/// filled with `0` wherever the figure does not apply to that row kind — a
/// column of zeros invites a total that means nothing.
fn report_csv(report: &ProfitabilityReport, totals: &ProfitabilityTotals) -> String {
    let from = iso_date(report.from);
    let to = iso_date(report.to);
    let mut out = csv::row(&COLUMNS);
    let mut write = |cells: [String; COLUMNS.len()]| {
        out.push_str(&csv::row(&cells.each_ref().map(String::as_str)))
    };

    for project in &report.projects {
        write(hours_row(project, &from, &to));
        for row in &project.by_currency {
            write(value_row("value", &project.project_name, &from, &to, row));
        }
    }

    write(total_hours_row(totals, &from, &to));
    for row in &totals.by_currency {
        write(value_row("totalValue", "", &from, &to, row));
    }
    out
}

/// One engagement's time and its budget, in the engagement's own currency —
/// which is the currency its money budget is stated in, and the only one that
/// budget is measured against.
fn hours_row(project: &ProjectProfitability, from: &str, to: &str) -> [String; COLUMNS.len()] {
    [
        "hours".to_owned(),
        project.project_name.clone(),
        from.to_owned(),
        to.to_owned(),
        project.currency.clone(),
        project.minutes.to_string(),
        project.billable_minutes.to_string(),
        project.unrated_minutes.to_string(),
        String::new(),
        String::new(),
        String::new(),
        project.to_date_minutes.to_string(),
        amount(project.to_date_net_cents),
        count(project.budget_minutes),
        money(project.budget_cents),
        percent(project.hours_consumption_bp()),
        percent(project.budget_consumption_bp()),
    ]
}

/// The report's time, over every engagement. No budget columns: budgets belong
/// to engagements, and a sum of them would be a plan nobody made.
fn total_hours_row(totals: &ProfitabilityTotals, from: &str, to: &str) -> [String; COLUMNS.len()] {
    let mut cells: [String; COLUMNS.len()] = Default::default();
    cells[0] = "totalHours".to_owned();
    cells[2] = from.to_owned();
    cells[3] = to.to_owned();
    cells[5] = totals.minutes.to_string();
    cells[6] = totals.billable_minutes.to_string();
    cells[7] = totals.unrated_minutes.to_string();
    cells
}

/// A money row — one currency's worth of one engagement, or of the whole
/// report. Its own function so the two callers cannot drift into writing
/// different columns for the same shape.
fn value_row(
    kind: &str,
    project: &str,
    from: &str,
    to: &str,
    row: &ProfitabilityCurrency,
) -> [String; COLUMNS.len()] {
    let mut cells: [String; COLUMNS.len()] = Default::default();
    cells[0] = kind.to_owned();
    cells[1] = project.to_owned();
    cells[2] = from.to_owned();
    cells[3] = to.to_owned();
    cells[4] = row.currency.clone();
    cells[6] = row.billable_minutes.to_string();
    cells[8] = amount(row.net_cents);
    cells[9] = amount(row.billed_net_cents);
    cells[10] = amount(row.unbilled_net_cents());
    cells
}

/// The file name a saved report lands under: what it is and the days it covers,
/// in ASCII, so nothing has to be escaped in the header or on a file system.
///
/// A narrowed report is named by the engagement's **id**, not its name: a name
/// is user text in any script and any language, and a file name is neither the
/// place to sanitise it nor the place to lose it. The name is inside the file,
/// on every row.
fn file_name(report: &ProfitabilityReport, project: Option<&ProjectId>) -> String {
    let scope = project.map_or_else(String::new, |id| format!("{}-", id.as_str()));
    format!(
        "profitability-{scope}{}-to-{}.csv",
        iso_date(report.from),
        iso_date(report.to)
    )
}

/// Reads the report behind both routes, so the file and the screen cannot
/// disagree about a cent.
async fn read(
    acc: &AccountStore,
    query: &ProfitabilityQuery,
) -> Result<(ProfitabilityReport, Option<ProjectId>), Problem> {
    let (from, to, project) = query.read()?;
    let report = acc
        .project_profitability(from, to, project.as_ref())
        .await
        .map_err(map_store_err)?;
    Ok((report, project))
}

/// `GET /projects/reports/profitability?from&to[&projectId]` →
/// `{"report":{…}}` — what each engagement's hours are worth over a period, and
/// how much of its budget has gone.
///
/// # Errors
/// `401` without a valid bearer token; `404` when a named project is not one
/// this caller can see, or is not client work; `422` when either day is missing
/// or malformed, or the period ends before it starts; `500` on a store failure.
pub async fn profitability_report(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ProfitabilityQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let (report, _) = read(&account.acc, &query).await?;
    let totals = profitability_totals(&report.projects);
    Ok(Json(json!({ "report": report_json(&report, &totals) })))
}

/// `GET /projects/reports/profitability.csv?from&to[&projectId]` → the same
/// figures as a file.
///
/// # Errors
/// The same four as the JSON route, for the same reasons.
pub async fn profitability_report_csv(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ProfitabilityQuery>,
) -> Result<Response, Problem> {
    let account = authenticate(&state, &headers).await?;
    let (report, project) = read(&account.acc, &query).await?;
    let totals = profitability_totals(&report.projects);
    Ok(csv::attachment(
        report_csv(&report, &totals),
        &file_name(&report, project.as_ref()),
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use alo_store::BillingCustomerId;
    use time::Month;

    fn on(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).unwrap_or_else(|e| panic!("{e}"))
    }

    fn query(from: Option<&str>, to: Option<&str>, project: Option<&str>) -> ProfitabilityQuery {
        ProfitabilityQuery {
            from: from.map(str::to_owned),
            to: to.map(str::to_owned),
            project_id: project.map(str::to_owned),
        }
    }

    fn currency(code: &str, minutes: i64, net: i64, billed: i64) -> ProfitabilityCurrency {
        ProfitabilityCurrency {
            currency: code.to_owned(),
            billable_minutes: minutes,
            net_cents: net,
            billed_minutes: minutes,
            billed_net_cents: billed,
        }
    }

    /// One August of one engagement: 12 hours logged, 10 of them chargeable at
    /// €95, half of that already on a document, against a €10 000 budget that
    /// 60 hours have already eaten into.
    fn august() -> ProfitabilityReport {
        ProfitabilityReport {
            from: on(2026, Month::August, 1),
            to: on(2026, Month::August, 31),
            projects: vec![ProjectProfitability {
                project_id: ProjectId::new("prj_1"),
                project_name: "Sunrise portal".to_owned(),
                customer_id: BillingCustomerId::new("cus_1"),
                currency: "EUR".to_owned(),
                budget_minutes: Some(6_000),
                budget_cents: Some(1_000_000),
                minutes: 720,
                billable_minutes: 600,
                unrated_minutes: 0,
                by_currency: vec![currency("EUR", 600, 95_000, 47_500)],
                to_date_minutes: 3_600,
                to_date_net_cents: 570_000,
            }],
        }
    }

    #[test]
    fn a_report_names_its_period_and_every_figure_the_screen_draws() {
        let report = august();
        let value = report_json(&report, &profitability_totals(&report.projects));
        assert_eq!(value["from"], "2026-08-01");
        assert_eq!(value["to"], "2026-08-31");
        let project = &value["projects"][0];
        assert_eq!(project["projectName"], "Sunrise portal");
        assert_eq!(project["customerId"], "cus_1");
        assert_eq!(project["minutes"], 720);
        assert_eq!(project["billableMinutes"], 600);
        assert_eq!(project["byCurrency"][0]["currency"], "EUR");
        assert_eq!(project["byCurrency"][0]["netCents"], 95_000);
        assert_eq!(project["byCurrency"][0]["unbilledNetCents"], 47_500);
        assert_eq!(project["toDateMinutes"], 3_600);
        assert_eq!(project["hoursConsumptionBp"], 6_000);
        assert_eq!(project["budgetConsumptionBp"], 5_700);
        assert_eq!(project["budgetRemainingCents"], 430_000);
        assert_eq!(value["totals"]["minutes"], 720);
        assert_eq!(value["totals"]["byCurrency"][0]["netCents"], 95_000);
    }

    #[test]
    fn an_engagement_with_no_budget_answers_null_rather_than_zero() {
        let mut unbudgeted = august();
        unbudgeted.projects[0].budget_minutes = None;
        unbudgeted.projects[0].budget_cents = None;
        let value = report_json(&unbudgeted, &profitability_totals(&unbudgeted.projects));
        let project = &value["projects"][0];
        assert!(project["budgetMinutes"].is_null());
        assert!(project["hoursConsumptionBp"].is_null());
        assert!(project["budgetConsumptionBp"].is_null());
        assert!(project["budgetRemainingCents"].is_null());
    }

    #[test]
    fn the_csv_is_one_table_of_named_row_kinds() {
        let report = august();
        let file = report_csv(&report, &profitability_totals(&report.projects));
        let lines: Vec<&str> = file.split("\r\n").filter(|l| !l.is_empty()).collect();
        assert_eq!(lines[0], COLUMNS.join(","));
        assert_eq!(
            lines[1],
            "hours,Sunrise portal,2026-08-01,2026-08-31,EUR,720,600,0,,,,3600,5700.00,6000,\
             10000.00,60.00,57.00"
        );
        assert_eq!(
            lines[2],
            "value,Sunrise portal,2026-08-01,2026-08-31,EUR,,600,,950.00,475.00,475.00,,,,,,"
        );
        assert_eq!(
            lines[3],
            "totalHours,,2026-08-01,2026-08-31,,720,600,0,,,,,,,,,"
        );
        assert_eq!(
            lines[4],
            "totalValue,,2026-08-01,2026-08-31,EUR,,600,,950.00,475.00,475.00,,,,,,"
        );
        assert_eq!(lines.len(), 5);
    }

    #[test]
    fn two_currencies_are_two_value_rows_and_are_never_added_together() {
        let mut mixed = august();
        mixed.projects[0]
            .by_currency
            .push(currency("USD", 120, 20_000, 0));
        let file = report_csv(&mixed, &profitability_totals(&mixed.projects));
        let lines: Vec<&str> = file.split("\r\n").filter(|l| !l.is_empty()).collect();
        // One `hours` row for the engagement, one `value` row per currency, and
        // the totals repeat the split rather than folding it away.
        assert_eq!(lines.len(), 7);
        assert!(lines[2].starts_with("value,Sunrise portal"));
        assert!(lines[3].contains(",USD,"));
        assert!(lines[5].starts_with("totalValue,"));
        assert!(lines[6].contains(",USD,"));
    }

    #[test]
    fn a_project_name_with_a_comma_is_quoted_not_broken() {
        let mut awkward = august();
        awkward.projects[0].project_name = "Sunrise, phase 2".to_owned();
        let file = report_csv(&awkward, &profitability_totals(&awkward.projects));
        assert!(file.contains("\"Sunrise, phase 2\""), "{file}");
    }

    #[test]
    fn a_tenant_with_no_engagements_is_a_header_and_an_empty_total() {
        let empty = ProfitabilityReport {
            projects: Vec::new(),
            ..august()
        };
        let file = report_csv(&empty, &profitability_totals(&empty.projects));
        let lines: Vec<&str> = file.split("\r\n").filter(|l| !l.is_empty()).collect();
        assert_eq!(lines.len(), 2, "the header and one totals row of zeroes");
        assert!(lines[1].starts_with("totalHours,,2026-08-01,2026-08-31,,0,0,0"));
    }

    #[test]
    fn an_absent_budget_is_an_empty_cell_and_never_a_zero() {
        let mut unbudgeted = august();
        unbudgeted.projects[0].budget_minutes = None;
        unbudgeted.projects[0].budget_cents = None;
        let file = report_csv(&unbudgeted, &profitability_totals(&unbudgeted.projects));
        let hours = file.split("\r\n").nth(1).unwrap_or_default();
        assert!(hours.ends_with(",3600,5700.00,,,,"), "{hours}");
    }

    #[test]
    fn a_saved_file_is_named_by_the_period_and_by_the_engagement_when_narrowed() {
        let report = august();
        assert_eq!(
            file_name(&report, None),
            "profitability-2026-08-01-to-2026-08-31.csv"
        );
        assert_eq!(
            file_name(&report, Some(&ProjectId::new("prj_1"))),
            "profitability-prj_1-2026-08-01-to-2026-08-31.csv"
        );
    }

    #[test]
    fn a_report_needs_both_ends_of_its_period_and_the_engagement_is_optional() {
        let (from, to, project) = query(Some(" 2026-08-01 "), Some("2026-08-31"), Some(" prj_1 "))
            .read()
            .unwrap_or_else(|e| panic!("rejected: {:?}", e.detail));
        assert_eq!(from, on(2026, Month::August, 1));
        assert_eq!(to, on(2026, Month::August, 31));
        assert_eq!(project.as_ref().map(ProjectId::as_str), Some("prj_1"));

        // A blank engagement is no engagement, not an id of one space.
        let (.., none) = query(Some("2026-08-01"), Some("2026-08-31"), Some("  "))
            .read()
            .unwrap();
        assert!(none.is_none());

        for (missing, field) in [
            (query(None, Some("2026-08-31"), None), "from"),
            (query(Some("2026-08-01"), Some("  "), None), "to"),
        ] {
            let problem = missing
                .read()
                .err()
                .unwrap_or_else(|| panic!("accepted a report with no {field}"));
            assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY);
            assert!(
                problem
                    .detail
                    .as_deref()
                    .unwrap_or_default()
                    .contains(field),
                "{field}: {:?}",
                problem.detail
            );
        }
    }

    #[test]
    fn a_period_end_is_a_plain_day_or_a_refusal() {
        for bad in ["2026-08-01T00:00:00Z", "20260801", "01/08/2026", "soon"] {
            let problem = query(Some(bad), Some("2026-08-31"), None)
                .read()
                .err()
                .unwrap_or_else(|| panic!("accepted {bad}"));
            assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}");
            assert!(
                problem
                    .detail
                    .as_deref()
                    .unwrap_or_default()
                    .contains("YYYY-MM-DD"),
                "{bad}"
            );
        }
    }

    #[test]
    fn a_proportion_is_a_percentage_with_two_decimals_and_absence_is_a_blank() {
        assert_eq!(percent(Some(0)), "0.00");
        assert_eq!(percent(Some(1_200)), "12.00");
        assert_eq!(percent(Some(10_000)), "100.00");
        assert_eq!(percent(Some(19_050)), "190.50", "an overrun is not clamped");
        assert_eq!(percent(Some(-1)), "-0.01");
        assert_eq!(percent(None), "");
        assert_eq!(count(None), "");
        assert_eq!(count(Some(0)), "0");
        assert_eq!(money(None), "");
        assert_eq!(money(Some(-1)), "-0.01");
    }
}
