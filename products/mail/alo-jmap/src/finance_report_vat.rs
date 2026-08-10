//! **The VAT-return figures** over HTTP (alo Finance, ADR 0035, wave B4.11d):
//! `GET /finance/reports/vat` and its `.csv` twin.
//!
//! The gate, the dates and the spreadsheet-safety rule are
//! [`crate::finance_reports`]'; what is here is this one report's two
//! representations of [`alo_store::VatReturn`] and nothing else.
//!
//! Two things this door states that the store does not.
//!
//! **It is the *journal's* VAT figures, and `/billing/reports/vat` is the
//! *documents'*.** Both are real reports and neither replaces the other: the
//! billing summary shows what was invoiced, per currency, with the counts
//! behind it; this one shows what the books carry, including the purchase side
//! no invoice of ours knows about, and it is the one a return is filed from.
//! They are asserted equal on the sales side by `tests/fin_vat_return.rs`.
//!
//! **The rate is printed as a percentage, as it is on a document.** `21.00`,
//! not `2100` — the same [`crate::billing_xml::percent`] an invoice, its PDF and
//! the billing summary's own file print, so a rate never reads two ways in one
//! tenant's paperwork. Basis points stay on the JSON, where a machine reads it.
//!
//! Nothing here is personal data: a rate, money, and the days it covers.

use axum::Json;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::Response;
use serde_json::{Value, json};

use alo_store::{VatReturn, VatReturnSide};

use crate::billing::{iso_date, map_store_err};
use crate::billing_xml::{amount, percent};
use crate::csv;
use crate::error::Problem;
use crate::finance_reports::{PeriodQuery, admin};
use crate::state::AppState;

/// One side of the return as JSON: the rates, their totals, and what is on
/// neither — a base with no rate stated, and tax with no rate on it.
fn side_json(side: &VatReturnSide) -> Value {
    json!({
        "rates": side
            .rates
            .iter()
            .map(|rate| json!({
                "rateBp": rate.rate_bp,
                "baseCents": rate.base_cents,
                "vatCents": rate.vat_cents,
            }))
            .collect::<Vec<_>>(),
        "baseCents": side.base_cents,
        "vatCents": side.vat_cents,
        "unratedBaseCents": side.unrated_base_cents,
        "unratedVatCents": side.unrated_vat_cents,
    })
}

/// The whole return as JSON. `netPayableCents` is positive when the tenant owes
/// the authority and negative when it is owed a refund — the screen says which
/// in words, and the number carries the sign either way.
fn report_json(report: &VatReturn) -> Value {
    json!({
        "from": iso_date(report.from),
        "to": iso_date(report.to),
        "currency": report.currency,
        "output": side_json(&report.output),
        "input": side_json(&report.input),
        "netPayableCents": report.net_payable_cents,
    })
}

/// The CSV column names — a contract, deliberately not translated.
const COLUMNS: [&str; 7] = [
    // Which kind of row you are reading — one table rather than three files:
    //   `outputRate`    one VAT rate of the sales side: base and tax charged
    //   `outputUnrated` turnover on no line of the return, and tax with no rate
    //   `outputTotal`   everything charged
    //   `inputRate`     one VAT rate of the purchase side: cost and tax paid
    //   `inputUnrated`  the same question on the purchase side
    //   `inputTotal`    everything recoverable
    //   `netPayable`    output tax less input tax — positive is owed
    "row",
    "periodFrom",
    "periodTo",
    "currency",
    // A percentage, as a document prints it: `21.00`, never `2100`. Empty on
    // the rows that are not about one rate.
    "vatRatePercent",
    // The taxable base — turnover on the output side, cost on the input one.
    "base",
    "vat",
];

/// The whole return as one CSV table: the sales rates and their total, then the
/// purchase rates and theirs, then the one figure the form asks for.
///
/// The period and the currency are repeated on every row on purpose — a row
/// lifted out of the file into another sheet still says which days it covers and
/// what unit it is in — and the `unrated` rows are written even when they are
/// zero, because their absence would read as "the question does not arise" when
/// what it means is "the answer is none".
fn report_csv(report: &VatReturn) -> String {
    let from = iso_date(report.from);
    let to = iso_date(report.to);
    let mut out = csv::row(&COLUMNS);
    let mut write = |kind: &str, rate: &str, base_cents: i64, vat_cents: i64| {
        out.push_str(&csv::row(&[
            kind,
            &from,
            &to,
            &report.currency,
            rate,
            &amount(base_cents),
            &amount(vat_cents),
        ]));
    };
    for (name, side) in [("output", &report.output), ("input", &report.input)] {
        for rate in &side.rates {
            write(
                &format!("{name}Rate"),
                &percent(rate.rate_bp),
                rate.base_cents,
                rate.vat_cents,
            );
        }
        write(
            &format!("{name}Unrated"),
            "",
            side.unrated_base_cents,
            side.unrated_vat_cents,
        );
        write(&format!("{name}Total"), "", side.base_cents, side.vat_cents);
    }
    // The figure the form asks for. It has no base of its own: a net payable is
    // a difference between two taxes, not a tax on anything.
    out.push_str(&csv::row(&[
        "netPayable",
        &from,
        &to,
        &report.currency,
        "",
        "",
        &amount(report.net_payable_cents),
    ]));
    out
}

/// The file name a saved return lands under: what it is and the days it covers,
/// in ASCII, so nothing has to be escaped in the header or on a file system.
///
/// `vat-return-…`, distinct from `/billing/reports/vat.csv`'s `vat-…`, so the
/// two files an accountant may well save for the same quarter do not overwrite
/// each other in a downloads folder.
fn file_name(report: &VatReturn) -> String {
    format!(
        "vat-return-{}-to-{}.csv",
        iso_date(report.from),
        iso_date(report.to)
    )
}

/// Reads the return behind both routes — one gate, one store call, so the file
/// an accountant opens and the table on the screen cannot disagree.
async fn read(
    state: &AppState,
    headers: &HeaderMap,
    query: &PeriodQuery,
) -> Result<VatReturn, Problem> {
    let account = admin(state, headers).await?;
    let (from, to) = query.days()?;
    account
        .acc
        .fin_vat_return(from, to)
        .await
        .map_err(map_store_err)
}

/// `GET /finance/reports/vat?from&to` → `{"report":{…}}` — the tax charged on
/// sales, the tax paid on purchases and the net payable between two days, both
/// included.
///
/// Every figure is the journal folded by VAT rate — the same postings the P&L
/// and the balance sheet read — in the tenant's accounting currency, which the
/// report states. These are **figures for a return, not a return**: filing goes
/// through the national portal (ADR 0035).
///
/// # Errors
/// `401` without a valid bearer token; `403` for a non-admin; `422` when an end
/// of the period is missing, malformed, the period ends before it starts, or the
/// period states more rates than one read can carry; `500` on a store failure.
pub async fn vat_return_report(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PeriodQuery>,
) -> Result<Json<Value>, Problem> {
    let report = read(&state, &headers, &query).await?;
    Ok(Json(json!({ "report": report_json(&report) })))
}

/// `GET /finance/reports/vat.csv?from&to` → the same return as a CSV file.
///
/// # Errors
/// As [`vat_return_report`].
pub async fn vat_return_report_csv(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PeriodQuery>,
) -> Result<Response, Problem> {
    let report = read(&state, &headers, &query).await?;
    Ok(csv::attachment(report_csv(&report), &file_name(&report)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alo_store::VatReturnRate;
    use time::{Date, Month};

    fn on(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).unwrap_or_else(|e| panic!("{e}"))
    }

    fn rate(rate_bp: i32, base_cents: i64, vat_cents: i64) -> VatReturnRate {
        VatReturnRate {
            rate_bp,
            base_cents,
            vat_cents,
        }
    }

    /// The quarter the store's golden suite seeds, as this layer receives it.
    fn quarter() -> VatReturn {
        VatReturn {
            from: on(2026, Month::July, 1),
            to: on(2026, Month::September, 30),
            currency: "EUR".to_owned(),
            output: VatReturnSide {
                rates: vec![rate(900, 25_000, 2_250), rate(2100, 100_000, 21_000)],
                base_cents: 125_000,
                vat_cents: 23_250,
                unrated_base_cents: 0,
                unrated_vat_cents: 0,
            },
            input: VatReturnSide {
                rates: vec![rate(2100, 40_000, 8_400)],
                base_cents: 40_000,
                vat_cents: 8_400,
                unrated_base_cents: 0,
                unrated_vat_cents: 0,
            },
            net_payable_cents: 14_850,
        }
    }

    #[test]
    fn the_json_states_the_period_the_currency_and_every_figure_in_cents() {
        let value = report_json(&quarter());
        assert_eq!(value["from"], "2026-07-01");
        assert_eq!(value["to"], "2026-09-30");
        assert_eq!(value["currency"], "EUR");
        assert_eq!(value["netPayableCents"], 14_850);
        assert_eq!(
            value["output"]["rates"][0],
            json!({ "rateBp": 900, "baseCents": 25_000, "vatCents": 2_250 })
        );
        assert_eq!(value["output"]["baseCents"], 125_000);
        assert_eq!(value["output"]["vatCents"], 23_250);
        assert_eq!(value["output"]["unratedBaseCents"], 0);
        assert_eq!(value["input"]["vatCents"], 8_400);
        assert_eq!(
            value["input"]["rates"].as_array().map(Vec::len),
            Some(1),
            "the purchase side is its own table"
        );
    }

    #[test]
    fn the_csv_is_the_two_sides_their_totals_and_the_one_figure_the_form_asks_for() {
        let body = report_csv(&quarter());
        let lines: Vec<&str> = body.split("\r\n").filter(|l| !l.is_empty()).collect();
        assert_eq!(
            lines[0],
            "row,periodFrom,periodTo,currency,vatRatePercent,base,vat"
        );
        assert_eq!(
            lines[1],
            "outputRate,2026-07-01,2026-09-30,EUR,9.00,250.00,22.50"
        );
        assert_eq!(
            lines[2],
            "outputRate,2026-07-01,2026-09-30,EUR,21.00,1000.00,210.00"
        );
        assert_eq!(
            lines[3],
            "outputUnrated,2026-07-01,2026-09-30,EUR,,0.00,0.00"
        );
        assert_eq!(
            lines[4],
            "outputTotal,2026-07-01,2026-09-30,EUR,,1250.00,232.50"
        );
        assert_eq!(
            lines[5],
            "inputRate,2026-07-01,2026-09-30,EUR,21.00,400.00,84.00"
        );
        assert_eq!(
            lines[6],
            "inputUnrated,2026-07-01,2026-09-30,EUR,,0.00,0.00"
        );
        assert_eq!(
            lines[7],
            "inputTotal,2026-07-01,2026-09-30,EUR,,400.00,84.00"
        );
        assert_eq!(
            lines[8], "netPayable,2026-07-01,2026-09-30,EUR,,,148.50",
            "a net payable is a difference between two taxes, not a tax on a base"
        );
        assert_eq!(lines.len(), 9, "no other rows: {body:?}");
        assert_eq!(
            file_name(&quarter()),
            "vat-return-2026-07-01-to-2026-09-30.csv"
        );
    }

    #[test]
    fn a_refund_prints_as_a_negative_and_stays_a_number() {
        let mut refund = quarter();
        refund.net_payable_cents = -6_300;
        let body = report_csv(&refund);
        let line = body
            .split("\r\n")
            .find(|line| line.starts_with("netPayable"))
            .unwrap_or_else(|| panic!("{body}"));
        assert!(
            line.ends_with(",,-63.00"),
            "a negative amount is a number, never neutralised: {line}"
        );
    }

    #[test]
    fn turnover_on_no_line_of_the_return_is_a_row_of_its_own() {
        let mut odd = quarter();
        odd.output.unrated_base_cents = 75_000;
        odd.output.unrated_vat_cents = 500;
        let body = report_csv(&odd);
        assert!(
            body.contains("outputUnrated,2026-07-01,2026-09-30,EUR,,750.00,5.00"),
            "{body}"
        );
        // …and it is not in the total, which is the whole point of reporting it
        // apart.
        assert!(body.contains("outputTotal,2026-07-01,2026-09-30,EUR,,1250.00,232.50"));
    }

    #[test]
    fn a_period_that_moved_nothing_is_still_a_file_with_its_totals() {
        let empty = VatReturn {
            output: VatReturnSide::default(),
            input: VatReturnSide::default(),
            net_payable_cents: 0,
            ..quarter()
        };
        let lines: Vec<String> = report_csv(&empty)
            .split("\r\n")
            .filter(|l| !l.is_empty())
            .map(str::to_owned)
            .collect();
        assert_eq!(
            lines.len(),
            6,
            "the header, two unrated rows, two totals, the net"
        );
        assert!(lines[5].starts_with("netPayable,"));
        assert!(lines[5].ends_with(",,0.00"));
        // The currency is on every row even when there is nothing in it: a file
        // that does not say what the nothing is nothing in is a question.
        assert!(lines.iter().skip(1).all(|line| line.contains(",EUR,")));
    }
}
