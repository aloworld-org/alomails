//! The CRM reports HTTP surface (alo CRM, ADR 0035, wave B2.08) — value by
//! stage and win/loss for a board, over [`alo_store::crm_report`].
//!
//! Two representations of one read, the same shape billing's VAT summary
//! settled on (`crate::billing_reports`): `GET /crm/reports/pipeline` answers
//! JSON for the screen and `GET /crm/reports/pipeline.csv` answers the same
//! figures as a file. Separate paths rather than one route with a `?format=`
//! — which is what the design note sketched — because a URL that names its
//! representation is the one a browser saves under a sensible name and a script
//! quotes without a query string, and because two modules answering "give me
//! the CSV" two different ways is a seam a reader has to remember.
//!
//! Three rules it keeps, all of them billing's for the same reasons:
//!
//! - **The period is stated, never guessed.** `from` and `to` are required
//!   plain days; a missing or malformed one is a `422`. They bound the *closed*
//!   deals only — the value-by-stage rows are the open board as it stands,
//!   which the store documents and the JSON says out loud with `openAsOf`.
//! - **The CSV columns are a contract, in English**, read by scripts and by a
//!   sales manager's own spreadsheet; what a person reads is the screen, which
//!   is translated.
//! - **The file names no customer.** Columns, counts and amounts — a board's
//!   shape, not its parties. A forecast mailed round the company should not
//!   carry a customer list with it.

use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use serde::Deserialize;
use serde_json::{Value, json};
use time::{Date, OffsetDateTime};

use alo_store::crm_report::{PipelineCurrency, PipelineReport, PipelineStageRow, PipelineTally};
use alo_store::{AccountStore, CrmPipelineId};

use crate::billing::{iso, iso_date, map_store_err, parse_iso_date};
use crate::csv;
use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// The board and the period a report is asked for. All three are required: a
/// report that quietly defaulted to "this quarter of whichever board came
/// first" would put a figure under a heading nobody asked for.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportQuery {
    #[serde(default)]
    pipeline_id: Option<String>,
    #[serde(default)]
    from: Option<String>,
    #[serde(default)]
    to: Option<String>,
}

impl ReportQuery {
    /// The board and the two days this query names, or the `422` that says
    /// which one is wrong.
    ///
    /// The board is only turned into an id here; whether it is *this tenant's*
    /// is the store's answer, and it is a `404` — the same answer an id that
    /// never existed gets, so this is not an existence oracle.
    fn read(&self) -> Result<(CrmPipelineId, Date, Date), Problem> {
        let pipeline = self
            .pipeline_id
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| {
                Problem::with(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "pipelineId is required: a report is always about one board",
                )
            })?;
        Ok((
            CrmPipelineId::new(pipeline.to_owned()),
            day("from", self.from.as_deref())?,
            day("to", self.to.as_deref())?,
        ))
    }
}

/// Reads one end of the period, naming it in every refusal so a caller with
/// both wrong learns which one it is looking at.
fn day(name: &str, raw: Option<&str>) -> Result<Date, Problem> {
    let raw = raw
        .map(str::trim)
        .filter(|v| !v.is_empty())
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

/// A count of deals and what they are worth.
fn tally_json(t: PipelineTally) -> Value {
    json!({ "dealCount": t.deal_count, "valueCents": t.value_cents })
}

/// One column's row: the open deals standing in it, in this group's currency.
fn stage_json(s: &PipelineStageRow) -> Value {
    json!({
        "stageId": s.stage_id.as_str(),
        "name": s.name,
        "isWon": s.is_won,
        "isLost": s.is_lost,
        "open": tally_json(s.open),
    })
}

/// One currency's whole answer.
///
/// `winRateBp` is `null` when nothing closed in the period: a win rate over no
/// deals is unanswered, not zero, and a client that saw `0` there would draw a
/// bar meaning "we lost everything".
fn currency_json(c: &PipelineCurrency) -> Value {
    json!({
        "currency": c.currency,
        "stages": c.stages.iter().map(stage_json).collect::<Vec<_>>(),
        "open": tally_json(c.open),
        "won": tally_json(c.won),
        "lost": tally_json(c.lost),
        "winRateBp": c.win_rate_bp(),
    })
}

/// The whole report, with the board and period it covers written into it — a
/// figure a person reads has to say what it is about.
///
/// `openAsOf` is the instant the open rows were counted at, and it is here
/// because the period does **not** bound them: value by stage is the board as
/// it stands now, while `won` and `lost` are the deals that closed between
/// `from` and `to`.
pub(crate) fn report_json(report: &PipelineReport, at: OffsetDateTime) -> Value {
    json!({
        "pipelineId": report.pipeline_id.as_str(),
        "pipelineName": report.pipeline_name,
        "from": iso_date(report.from),
        "to": iso_date(report.to),
        "openAsOf": iso(at),
        "currencies": report.currencies.iter().map(currency_json).collect::<Vec<_>>(),
    })
}

/// An integer-cents amount as the decimal a spreadsheet reads: two decimals, a
/// `.` separator, no grouping. Integer-only, like every other conversion of
/// money in alo — the cents are split into whole units and hundredths, never
/// divided by 100.0.
fn amount(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let abs = i128::from(cents).abs();
    format!("{sign}{}.{:02}", abs / 100, abs % 100)
}

/// A rate in basis points as the percentage a reader expects (7500 → `75.00`).
fn percent(bp: i32) -> String {
    let sign = if bp < 0 { "-" } else { "" };
    let abs = i64::from(bp).abs();
    format!("{sign}{}.{:02}", abs / 100, abs % 100)
}

/// The CSV column names — a contract, deliberately not translated.
const COLUMNS: [&str; 9] = [
    // Which kind of row you are reading — one table rather than three files:
    //   `stage` the open deals standing in one column of the board
    //   `open`  everything still open in that currency, the sum of its `stage`
    //           rows
    //   `won` / `lost` what closed inside the period
    "row",
    "pipeline",
    "periodFrom",
    "periodTo",
    "currency",
    "stage",
    "deals",
    "value",
    // The share of the period's closed deals that were won, as a percentage.
    // Written on the `won` row only, and empty when nothing closed — a rate
    // over no deals is unanswered, and `0.00` would be a lie.
    "winRatePercent",
];

/// The whole report as one CSV table: the open board column by column, its
/// total, then the period's outcomes — repeated per currency, which is the one
/// grouping this report has (it never converts between them).
///
/// The board and the period are repeated on every row on purpose: a row lifted
/// into another sheet still says what it is about.
fn report_csv(report: &PipelineReport) -> String {
    let from = iso_date(report.from);
    let to = iso_date(report.to);
    let mut out = csv::row(&COLUMNS);
    let mut write = |kind: &str, currency: &str, stage: &str, t: PipelineTally, rate: &str| {
        out.push_str(&csv::row(&[
            kind,
            &report.pipeline_name,
            &from,
            &to,
            currency,
            stage,
            &t.deal_count.to_string(),
            &amount(t.value_cents),
            rate,
        ]));
    };
    for c in &report.currencies {
        for s in &c.stages {
            write("stage", &c.currency, &s.name, s.open, "");
        }
        write("open", &c.currency, "", c.open, "");
        write(
            "won",
            &c.currency,
            "",
            c.won,
            &c.win_rate_bp().map(percent).unwrap_or_default(),
        );
        write("lost", &c.currency, "", c.lost, "");
    }
    out
}

/// The file name a saved report lands under: what it is and the days it covers,
/// in ASCII, so nothing has to be escaped in the header or on a file system.
///
/// The board's **id**, not its name: a name is user text in any script and any
/// language, and a file name is neither the place to sanitise it nor the place
/// to lose it. The name is inside the file, on every row.
fn file_name(report: &PipelineReport) -> String {
    format!(
        "pipeline-{}-{}-to-{}.csv",
        report.pipeline_id.as_str(),
        iso_date(report.from),
        iso_date(report.to)
    )
}

/// Reads the report behind both routes, so the file and the screen cannot
/// disagree about a cent.
async fn read(acc: &AccountStore, query: &ReportQuery) -> Result<PipelineReport, Problem> {
    let (pipeline, from, to) = query.read()?;
    acc.crm_pipeline_report(&pipeline, from, to)
        .await
        .map_err(map_store_err)
}

/// `GET /crm/reports/pipeline?pipelineId&from&to` → `{"report":{…}}` — what
/// stands on a board now, and what it won and lost between two days.
pub async fn pipeline_report(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ReportQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let report = read(&account.acc, &query).await?;
    Ok(Json(json!({
        "report": report_json(&report, OffsetDateTime::now_utc()),
    })))
}

/// `GET /crm/reports/pipeline.csv?pipelineId&from&to` → the same figures as a
/// file.
pub async fn pipeline_report_csv(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ReportQuery>,
) -> Result<Response, Problem> {
    let account = authenticate(&state, &headers).await?;
    let report = read(&account.acc, &query).await?;
    Ok(csv::attachment(report_csv(&report), &file_name(&report)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alo_store::CrmStageId;
    use time::Month;

    fn on(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).unwrap_or_else(|e| panic!("{e}"))
    }

    fn query(pipeline: Option<&str>, from: Option<&str>, to: Option<&str>) -> ReportQuery {
        ReportQuery {
            pipeline_id: pipeline.map(str::to_owned),
            from: from.map(str::to_owned),
            to: to.map(str::to_owned),
        }
    }

    fn tally(deal_count: i64, value_cents: i64) -> PipelineTally {
        PipelineTally {
            deal_count,
            value_cents,
        }
    }

    fn stage(name: &str, is_won: bool, is_lost: bool, open: PipelineTally) -> PipelineStageRow {
        PipelineStageRow {
            stage_id: CrmStageId::new(format!("stg_{name}")),
            name: name.to_owned(),
            is_won,
            is_lost,
            open,
        }
    }

    /// A quarter of one EUR board: 12 500 open across two columns, three deals
    /// won and one lost.
    fn quarter() -> PipelineReport {
        PipelineReport {
            pipeline_id: CrmPipelineId::new("pip_1"),
            pipeline_name: "Sales".to_owned(),
            from: on(2026, Month::July, 1),
            to: on(2026, Month::September, 30),
            currencies: vec![PipelineCurrency {
                currency: "EUR".to_owned(),
                stages: vec![
                    stage("New", false, false, tally(2, 250_000)),
                    stage("Proposal", false, false, tally(1, 1_000_000)),
                    stage("Won", true, false, PipelineTally::default()),
                    stage("Lost", false, true, PipelineTally::default()),
                ],
                open: tally(3, 1_250_000),
                won: tally(3, 900_000),
                lost: tally(1, 50_000),
            }],
        }
    }

    #[test]
    fn a_report_names_its_board_its_period_and_the_instant_it_was_counted() {
        let at = OffsetDateTime::UNIX_EPOCH;
        let value = report_json(&quarter(), at);
        assert_eq!(value["pipelineId"], "pip_1");
        assert_eq!(value["pipelineName"], "Sales");
        assert_eq!(value["from"], "2026-07-01");
        assert_eq!(value["to"], "2026-09-30");
        assert_eq!(value["openAsOf"], iso(at));
        let eur = &value["currencies"][0];
        assert_eq!(eur["currency"], "EUR");
        assert_eq!(eur["open"]["valueCents"], 1_250_000);
        assert_eq!(eur["won"]["dealCount"], 3);
        assert_eq!(eur["winRateBp"], 7_500);
        assert_eq!(eur["stages"][0]["name"], "New");
        assert_eq!(eur["stages"][0]["open"]["valueCents"], 250_000);
        assert_eq!(eur["stages"][2]["isWon"], true);
    }

    #[test]
    fn a_win_rate_over_nothing_is_null_rather_than_zero() {
        let mut nothing_closed = quarter();
        nothing_closed.currencies[0].won = PipelineTally::default();
        nothing_closed.currencies[0].lost = PipelineTally::default();
        let value = report_json(&nothing_closed, OffsetDateTime::UNIX_EPOCH);
        assert!(value["currencies"][0]["winRateBp"].is_null());
    }

    #[test]
    fn the_csv_is_one_table_of_named_row_kinds() {
        let file = report_csv(&quarter());
        let lines: Vec<&str> = file.split("\r\n").filter(|l| !l.is_empty()).collect();
        assert_eq!(lines[0], COLUMNS.join(","));
        assert_eq!(
            lines[1],
            "stage,Sales,2026-07-01,2026-09-30,EUR,New,2,2500.00,"
        );
        assert_eq!(
            lines[2],
            "stage,Sales,2026-07-01,2026-09-30,EUR,Proposal,1,10000.00,"
        );
        assert_eq!(
            lines[5],
            "open,Sales,2026-07-01,2026-09-30,EUR,,3,12500.00,"
        );
        assert_eq!(
            lines[6],
            "won,Sales,2026-07-01,2026-09-30,EUR,,3,9000.00,75.00"
        );
        assert_eq!(lines[7], "lost,Sales,2026-07-01,2026-09-30,EUR,,1,500.00,");
        assert_eq!(lines.len(), 8, "header, four columns, and three totals");
    }

    #[test]
    fn a_board_name_with_a_comma_is_quoted_not_broken() {
        let mut awkward = quarter();
        awkward.pipeline_name = "Sales, EU".to_owned();
        let file = report_csv(&awkward);
        assert!(file.contains("\"Sales, EU\""), "{file}");
    }

    #[test]
    fn an_empty_board_is_a_header_and_nothing_else() {
        let mut empty = quarter();
        empty.currencies.clear();
        let file = report_csv(&empty);
        assert_eq!(file, csv::row(&COLUMNS));
    }

    #[test]
    fn a_saved_file_is_named_by_the_board_id_and_the_period() {
        assert_eq!(
            file_name(&quarter()),
            "pipeline-pip_1-2026-07-01-to-2026-09-30.csv"
        );
    }

    #[test]
    fn a_report_needs_a_board_and_both_ends_of_its_period() {
        let (id, from, to) = query(Some(" pip_1 "), Some("2026-07-01"), Some("2026-09-30"))
            .read()
            .unwrap_or_else(|e| panic!("rejected: {:?}", e.detail));
        assert_eq!(id.as_str(), "pip_1");
        assert_eq!(from, on(2026, Month::July, 1));
        assert_eq!(to, on(2026, Month::September, 30));

        for (missing, field) in [
            (
                query(None, Some("2026-07-01"), Some("2026-09-30")),
                "pipelineId",
            ),
            (query(Some("pip_1"), None, Some("2026-09-30")), "from"),
            (query(Some("pip_1"), Some("2026-07-01"), Some("  ")), "to"),
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
        for bad in ["2026-07-01T00:00:00Z", "20260701", "01/07/2026", "soon"] {
            let problem = query(Some("pip_1"), Some(bad), Some("2026-09-30"))
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
    fn money_is_split_into_units_and_hundredths_never_divided() {
        assert_eq!(amount(0), "0.00");
        assert_eq!(amount(5), "0.05");
        assert_eq!(amount(1_250_000), "12500.00");
        assert_eq!(amount(-1), "-0.01");
        assert_eq!(amount(i64::MIN), "-92233720368547758.08");
    }

    #[test]
    fn a_rate_is_a_percentage_with_two_decimals() {
        assert_eq!(percent(0), "0.00");
        assert_eq!(percent(3_333), "33.33");
        assert_eq!(percent(10_000), "100.00");
    }
}
