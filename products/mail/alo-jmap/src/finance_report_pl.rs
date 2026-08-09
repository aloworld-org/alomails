//! **The profit and loss** over HTTP (alo Finance, ADR 0035, wave B4.11a):
//! `GET /finance/reports/pl` and its `.csv` twin.
//!
//! The gate, the dates and the spreadsheet-safety rule are
//! [`crate::finance_reports`]'; what is here is this one report's two
//! representations of [`alo_store::ProfitAndLoss`] and nothing else.
//!
//! The *comparative* period is the store's ([`alo_store::comparative_period`])
//! and is returned rather than asked for, so every caller shows the same
//! comparison and none of them computes it.

use axum::Json;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::Response;
use serde_json::{Value, json};

use alo_store::{PlLine, ProfitAndLoss};

use crate::billing::{iso_date, map_store_err};
use crate::billing_xml::amount;
use crate::csv;
use crate::error::Problem;
use crate::finance_reports::{PeriodQuery, admin, text};
use crate::state::AppState;

/// One account's line as JSON. `postings` is the current period's count — zero
/// on a line only the comparative moved, which is what says the zero is real.
fn line_json(line: &PlLine) -> Value {
    json!({
        "accountId": line.account_id.as_str(),
        "code": line.code,
        "name": line.name,
        "type": line.kind.as_str(),
        "amountCents": line.amount_cents,
        "previousCents": line.previous_cents,
        "postings": line.postings,
    })
}

/// The whole report as JSON, both periods included: a figure a human reads
/// beside last year's has to say which days each of them covers.
fn report_json(report: &ProfitAndLoss) -> Value {
    json!({
        "from": iso_date(report.from),
        "to": iso_date(report.to),
        "previousFrom": iso_date(report.previous_from),
        "previousTo": iso_date(report.previous_to),
        "currency": report.currency,
        "income": report.income.iter().map(line_json).collect::<Vec<_>>(),
        "expense": report.expense.iter().map(line_json).collect::<Vec<_>>(),
        "incomeCents": report.income_cents,
        "expenseCents": report.expense_cents,
        "resultCents": report.result_cents,
        "previousIncomeCents": report.previous_income_cents,
        "previousExpenseCents": report.previous_expense_cents,
        "previousResultCents": report.previous_result_cents,
    })
}

/// The CSV column names — a contract, deliberately not translated.
const COLUMNS: [&str; 10] = [
    // Which kind of row you are reading — one table rather than three files:
    //   `income`       one income account's line
    //   `incomeTotal`  everything earned in the period
    //   `expense`      one expense account's line
    //   `expenseTotal` everything spent
    //   `result`       income less expense: the profit or the loss
    "row",
    "periodFrom",
    "periodTo",
    // The comparative period, which the server chose and the file states, so a
    // row lifted into another sheet still says what it is being compared with.
    "previousFrom",
    "previousTo",
    "currency",
    "accountCode",
    "accountName",
    // Positive on both sides: revenue and cost are both amounts, and the
    // `result` row carries the subtraction.
    "amount",
    "previousAmount",
];

/// The whole report as one CSV table: the income lines and their total, then
/// the expense lines and theirs, then the result.
///
/// Both periods are repeated on every row on purpose — a row lifted out of the
/// file into another sheet still says which days it covers — and the total rows
/// carry no account, because they are not one.
fn report_csv(report: &ProfitAndLoss) -> String {
    let from = iso_date(report.from);
    let to = iso_date(report.to);
    let previous_from = iso_date(report.previous_from);
    let previous_to = iso_date(report.previous_to);
    let mut out = csv::row(&COLUMNS);
    let mut write = |kind: &str, code: &str, name: &str, cents: i64, previous: i64| {
        out.push_str(&csv::row(&[
            kind,
            &from,
            &to,
            &previous_from,
            &previous_to,
            &report.currency,
            code,
            name,
            &amount(cents),
            &amount(previous),
        ]));
    };
    for line in &report.income {
        write(
            "income",
            &line.code,
            &text(&line.name),
            line.amount_cents,
            line.previous_cents,
        );
    }
    write(
        "incomeTotal",
        "",
        "",
        report.income_cents,
        report.previous_income_cents,
    );
    for line in &report.expense {
        write(
            "expense",
            &line.code,
            &text(&line.name),
            line.amount_cents,
            line.previous_cents,
        );
    }
    write(
        "expenseTotal",
        "",
        "",
        report.expense_cents,
        report.previous_expense_cents,
    );
    write(
        "result",
        "",
        "",
        report.result_cents,
        report.previous_result_cents,
    );
    out
}

/// The file name a saved report lands under: what it is and the days it covers,
/// in ASCII, so nothing has to be escaped in the header or on a file system.
fn file_name(report: &ProfitAndLoss) -> String {
    format!(
        "profit-and-loss-{}-to-{}.csv",
        iso_date(report.from),
        iso_date(report.to)
    )
}

/// Reads the report behind both routes — one gate, one store call, so the file
/// an accountant opens and the table on the screen cannot disagree.
async fn read(
    state: &AppState,
    headers: &HeaderMap,
    query: &PeriodQuery,
) -> Result<ProfitAndLoss, Problem> {
    let account = admin(state, headers).await?;
    let (from, to) = query.days()?;
    account
        .acc
        .fin_profit_and_loss(from, to)
        .await
        .map_err(map_store_err)
}

/// `GET /finance/reports/pl?from&to` → `{"report":{…}}` — what the business
/// earned and spent between two days, both included, with the period of the
/// same length before it beside every figure.
///
/// Every amount is in the tenant's accounting currency, which the report
/// states, and every figure is the journal folded — the same postings the
/// ledger and the balance sheet read.
///
/// # Errors
/// `401` without a valid bearer token; `403` for a non-admin; `422` when an end
/// of the period is missing, malformed, or the period ends before it starts;
/// `500` on a store failure.
pub async fn pl_report(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PeriodQuery>,
) -> Result<Json<Value>, Problem> {
    let report = read(&state, &headers, &query).await?;
    Ok(Json(json!({ "report": report_json(&report) })))
}

/// `GET /finance/reports/pl.csv?from&to` → the same report as a CSV file.
///
/// # Errors
/// As [`pl_report`].
pub async fn pl_report_csv(
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
    use alo_store::{AccountType, FinAccountId};
    use time::{Date, Month};

    fn on(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).unwrap_or_else(|e| panic!("{e}"))
    }

    fn line(code: &str, name: &str, kind: AccountType, cents: i64, previous: i64) -> PlLine {
        PlLine {
            account_id: FinAccountId::new(format!("acc-{code}")),
            code: code.to_owned(),
            name: name.to_owned(),
            kind,
            amount_cents: cents,
            previous_cents: previous,
            postings: if cents == 0 { 0 } else { 3 },
        }
    }

    /// The year the store's golden suite seeds, as this layer receives it.
    fn year() -> ProfitAndLoss {
        ProfitAndLoss {
            from: on(2026, Month::January, 1),
            to: on(2026, Month::December, 31),
            previous_from: on(2025, Month::January, 1),
            previous_to: on(2025, Month::December, 31),
            currency: "EUR".to_owned(),
            income: vec![
                line("4000", "Sales", AccountType::Income, 140_000, 50_000),
                line("4900", "Other income", AccountType::Income, 20_000, 0),
            ],
            expense: vec![
                line("6000", "Hosting", AccountType::Expense, 20_000, 20_000),
                line("6100", "Travel", AccountType::Expense, 7_500, 0),
                line("6200", "Accountancy", AccountType::Expense, 2_500, 0),
            ],
            income_cents: 160_000,
            expense_cents: 30_000,
            result_cents: 130_000,
            previous_income_cents: 50_000,
            previous_expense_cents: 20_000,
            previous_result_cents: 30_000,
        }
    }

    #[test]
    fn the_json_states_both_periods_the_currency_and_every_figure_in_cents() {
        let value = report_json(&year());
        assert_eq!(value["from"], "2026-01-01");
        assert_eq!(value["to"], "2026-12-31");
        assert_eq!(value["previousFrom"], "2025-01-01");
        assert_eq!(value["previousTo"], "2025-12-31");
        assert_eq!(value["currency"], "EUR");
        assert_eq!(value["incomeCents"], 160_000);
        assert_eq!(value["expenseCents"], 30_000);
        assert_eq!(value["resultCents"], 130_000);
        assert_eq!(value["previousResultCents"], 30_000);
        assert_eq!(
            value["income"][0],
            json!({
                "accountId": "acc-4000",
                "code": "4000",
                "name": "Sales",
                "type": "income",
                "amountCents": 140_000,
                "previousCents": 50_000,
                "postings": 3,
            })
        );
        assert_eq!(value["expense"][2]["code"], "6200");
        assert_eq!(
            value["income"].as_array().map(Vec::len),
            Some(2),
            "the balance sheet is not on a profit and loss"
        );
    }

    #[test]
    fn the_csv_is_the_two_sides_their_totals_and_the_result() {
        let body = report_csv(&year());
        let lines: Vec<&str> = body.split("\r\n").filter(|l| !l.is_empty()).collect();
        assert_eq!(
            lines[0],
            "row,periodFrom,periodTo,previousFrom,previousTo,currency,accountCode,accountName,\
             amount,previousAmount"
        );
        assert_eq!(
            lines[1],
            "income,2026-01-01,2026-12-31,2025-01-01,2025-12-31,EUR,4000,Sales,1400.00,500.00"
        );
        assert_eq!(
            lines[3],
            "incomeTotal,2026-01-01,2026-12-31,2025-01-01,2025-12-31,EUR,,,1600.00,500.00"
        );
        assert_eq!(
            lines[7],
            "expenseTotal,2026-01-01,2026-12-31,2025-01-01,2025-12-31,EUR,,,300.00,200.00"
        );
        assert_eq!(
            lines[8],
            "result,2026-01-01,2026-12-31,2025-01-01,2025-12-31,EUR,,,1300.00,300.00"
        );
        assert_eq!(lines.len(), 9, "no other rows: {body:?}");
    }

    #[test]
    fn a_loss_prints_as_a_negative_result_and_stays_a_number() {
        let mut loss = year();
        loss.result_cents = -15_000;
        loss.previous_result_cents = -1;
        let body = report_csv(&loss);
        let result = body
            .split("\r\n")
            .find(|line| line.starts_with("result"))
            .unwrap_or_else(|| panic!("{body}"));
        assert!(
            result.ends_with(",-150.00,-0.01"),
            "a negative amount is a number, never neutralised: {result}"
        );
    }

    #[test]
    fn an_account_named_like_a_formula_cannot_become_one() {
        let mut hostile = year();
        hostile.income[0].name = "=1+1".to_owned();
        hostile.expense[0].name = "-cmd|'/c calc'!A1".to_owned();
        let body = report_csv(&hostile);
        assert!(body.contains(",'=1+1,"), "{body}");
        assert!(body.contains(",'-cmd|'/c calc'!A1,"), "{body}");
        // And an ordinary name is left exactly as the tenant typed it.
        assert!(body.contains(",Travel,"), "{body}");
    }

    #[test]
    fn a_period_that_moved_nothing_is_still_a_file_with_its_totals() {
        let empty = ProfitAndLoss {
            income: Vec::new(),
            expense: Vec::new(),
            income_cents: 0,
            expense_cents: 0,
            result_cents: 0,
            previous_income_cents: 0,
            previous_expense_cents: 0,
            previous_result_cents: 0,
            ..year()
        };
        let lines: Vec<String> = report_csv(&empty)
            .split("\r\n")
            .filter(|l| !l.is_empty())
            .map(str::to_owned)
            .collect();
        assert_eq!(lines.len(), 4, "the header and the three totals");
        assert!(lines[3].starts_with("result,"));
        assert!(lines[3].ends_with(",0.00,0.00"));
        assert_eq!(
            file_name(&empty),
            "profit-and-loss-2026-01-01-to-2026-12-31.csv"
        );
    }
}
