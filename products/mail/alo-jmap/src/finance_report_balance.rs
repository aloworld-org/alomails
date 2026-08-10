//! **The balance sheet** over HTTP (alo Finance, ADR 0035, wave B4.11b):
//! `GET /finance/reports/balance?on` and its `.csv` twin.
//!
//! The gate, the date and the spreadsheet-safety rule are
//! [`crate::finance_reports`]'; what is here is this one report's two
//! representations of [`alo_store::BalanceSheet`] and nothing else.
//!
//! Three things this layer says out loud that the P&L does not have to.
//!
//! - **One date, not a period.** A balance sheet is cumulative by definition:
//!   every posting on or before `on` counts, back to the day the books opened.
//!   `?from` is not accepted, because "what was in the bank between March and
//!   June" is a ledger question and answering it here would produce a sheet that
//!   does not balance.
//! - **The sheet says whether it balances.** `differenceCents` and `balances`
//!   are on the wire and `difference` is a row of the file — zero on every
//!   honest set of books (P10), and stated rather than assumed because the
//!   figure a broken sheet prints looks exactly like a correct one. A screen
//!   that finds a non-zero difference must say so; it must not round it into
//!   equity.
//! - **The result is beside equity, not inside it.** alo writes no year-end
//!   closing entry, so income less expense to the date is its own figure. That
//!   is what makes `assets = liabilities + equity + result` hold, and it is also
//!   what an accountant expects to see on a set of books nobody has closed.
//!
//! *Rejected: a comparative column.* The P&L derives one (the period of the same
//! length before it) because a period has an obvious predecessor. A balance
//! sheet's is the **previous financial year end**, which is a fact about the
//! tenant's fiscal calendar rather than about the date asked for — and "the same
//! day a year earlier" would be a guess printed under a heading nobody chose.
//! A caller who wants two dates asks twice, which is honest and is one line.

use axum::Json;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::Response;
use serde_json::{Value, json};

use alo_store::{BalanceLine, BalanceSheet};

use crate::billing::{iso_date, map_store_err};
use crate::billing_xml::amount;
use crate::csv;
use crate::error::Problem;
use crate::finance_reports::{OnQuery, reader, text};
use crate::state::AppState;

/// One account's line as JSON. `role` is the posting-rule job the account does,
/// so a screen can show the bank apart from the receivables without reading
/// codes it does not own; it is `null` on an ordinary account.
fn line_json(line: &BalanceLine) -> Value {
    json!({
        "accountId": line.account_id.as_str(),
        "code": line.code,
        "name": line.name,
        "type": line.kind.as_str(),
        "role": line.role.map(alo_store::AccountRole::as_str),
        "amountCents": line.amount_cents,
        "postings": line.postings,
    })
}

/// The whole sheet as JSON: the three sides, the result nothing has closed into
/// equity, and the two totals that must be equal.
fn report_json(sheet: &BalanceSheet) -> Value {
    json!({
        "on": iso_date(sheet.on),
        "currency": sheet.currency,
        "assets": sheet.assets.iter().map(line_json).collect::<Vec<_>>(),
        "liabilities": sheet.liabilities.iter().map(line_json).collect::<Vec<_>>(),
        "equity": sheet.equity.iter().map(line_json).collect::<Vec<_>>(),
        "assetCents": sheet.asset_cents,
        "liabilityCents": sheet.liability_cents,
        "equityCents": sheet.equity_cents,
        "resultCents": sheet.result_cents,
        "liabilityEquityCents": sheet.liability_equity_cents,
        "differenceCents": sheet.difference_cents,
        "balances": sheet.balances(),
    })
}

/// The CSV column names — a contract, deliberately not translated.
const COLUMNS: [&str; 6] = [
    // Which kind of row you are reading — one table rather than three files:
    //   `asset`               one asset account's balance
    //   `assetTotal`          everything owned
    //   `liability`           one liability account's balance
    //   `liabilityTotal`      everything owed
    //   `equity`              one equity account's balance
    //   `equityTotal`         the owners' stake as booked
    //   `result`              income less expense, which no entry has closed
    //   `liabilityEquityTotal` owed + equity + result: what must equal the assets
    //   `difference`          assets less that — zero on every honest sheet
    "row",
    // The one date. Repeated on every row on purpose: a row lifted out of the
    // file into another sheet still says what day it stands on.
    "on",
    "currency",
    "accountCode",
    "accountName",
    "amount",
];

/// The whole sheet as one CSV table, in the order an accountant reads it: what
/// is owned, then what is owed, then equity, then the result, then the two
/// figures that prove it balances.
fn report_csv(sheet: &BalanceSheet) -> String {
    let on = iso_date(sheet.on);
    let mut out = csv::row(&COLUMNS);
    let mut write = |kind: &str, code: &str, name: &str, cents: i64| {
        out.push_str(&csv::row(&[
            kind,
            &on,
            &sheet.currency,
            code,
            name,
            &amount(cents),
        ]));
    };
    for (kind, total_kind, lines, total) in [
        ("asset", "assetTotal", &sheet.assets, sheet.asset_cents),
        (
            "liability",
            "liabilityTotal",
            &sheet.liabilities,
            sheet.liability_cents,
        ),
        ("equity", "equityTotal", &sheet.equity, sheet.equity_cents),
    ] {
        for line in lines {
            write(kind, &line.code, &text(&line.name), line.amount_cents);
        }
        // The total rows carry no account, because they are not one.
        write(total_kind, "", "", total);
    }
    write("result", "", "", sheet.result_cents);
    write("liabilityEquityTotal", "", "", sheet.liability_equity_cents);
    write("difference", "", "", sheet.difference_cents);
    out
}

/// The file name a saved sheet lands under: what it is and the day it stands
/// on, in ASCII, so nothing has to be escaped in the header or on a file system.
fn file_name(sheet: &BalanceSheet) -> String {
    format!("balance-sheet-{}.csv", iso_date(sheet.on))
}

/// Reads the sheet behind both routes — one gate, one store call, so the file
/// an accountant opens and the table on the screen cannot disagree.
async fn read(
    state: &AppState,
    headers: &HeaderMap,
    query: &OnQuery,
) -> Result<BalanceSheet, Problem> {
    let account = reader(state, headers).await?;
    let on = query.day()?;
    account
        .acc
        .fin_balance_sheet(on)
        .await
        .map_err(map_store_err)
}

/// `GET /finance/reports/balance?on` → `{"report":{…}}` — what the business
/// owns, what it owes, its equity and the result no entry has closed into it,
/// on one day.
///
/// Every amount is in the tenant's accounting currency, which the report states,
/// and every figure is the journal folded — the same postings the ledger and the
/// P&L read.
///
/// # Errors
/// `401` without a valid bearer token; `403` for a member who is neither an
/// admin nor an accountant; `422` when `on` is missing or malformed; `500` on a
/// store failure.
pub async fn balance_report(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<OnQuery>,
) -> Result<Json<Value>, Problem> {
    let sheet = read(&state, &headers, &query).await?;
    Ok(Json(json!({ "report": report_json(&sheet) })))
}

/// `GET /finance/reports/balance.csv?on` → the same sheet as a CSV file.
///
/// # Errors
/// As [`balance_report`].
pub async fn balance_report_csv(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<OnQuery>,
) -> Result<Response, Problem> {
    let sheet = read(&state, &headers, &query).await?;
    Ok(csv::attachment(report_csv(&sheet), &file_name(&sheet)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alo_store::{AccountRole, AccountType, FinAccountId};
    use time::{Date, Month};

    fn on(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).unwrap_or_else(|e| panic!("{e}"))
    }

    fn line(
        code: &str,
        name: &str,
        kind: AccountType,
        role: Option<AccountRole>,
        cents: i64,
    ) -> BalanceLine {
        BalanceLine {
            account_id: FinAccountId::new(format!("acc-{code}")),
            code: code.to_owned(),
            name: name.to_owned(),
            kind,
            role,
            amount_cents: cents,
            postings: 4,
        }
    }

    /// The year end the store's golden suite seeds, as this layer receives it.
    fn year_end() -> BalanceSheet {
        BalanceSheet {
            on: on(2026, Month::December, 31),
            currency: "EUR".to_owned(),
            assets: vec![
                line(
                    "1000",
                    "Bank",
                    AccountType::Asset,
                    Some(AccountRole::Bank),
                    106_000,
                ),
                line(
                    "1100",
                    "Trade receivables",
                    AccountType::Asset,
                    Some(AccountRole::Ar),
                    122_600,
                ),
            ],
            liabilities: vec![
                line(
                    "2000",
                    "Trade payables",
                    AccountType::Liability,
                    Some(AccountRole::Ap),
                    35_000,
                ),
                line(
                    "2100",
                    "VAT payable",
                    AccountType::Liability,
                    Some(AccountRole::VatOutput),
                    33_600,
                ),
            ],
            equity: Vec::new(),
            asset_cents: 228_600,
            liability_cents: 68_600,
            equity_cents: 0,
            result_cents: 160_000,
            liability_equity_cents: 228_600,
            difference_cents: 0,
        }
    }

    #[test]
    fn the_json_states_the_date_the_currency_and_every_figure_in_cents() {
        let value = report_json(&year_end());
        assert_eq!(value["on"], "2026-12-31");
        assert_eq!(value["currency"], "EUR");
        assert_eq!(value["assetCents"], 228_600);
        assert_eq!(value["liabilityCents"], 68_600);
        assert_eq!(value["equityCents"], 0);
        assert_eq!(value["resultCents"], 160_000);
        assert_eq!(value["liabilityEquityCents"], 228_600);
        assert_eq!(value["differenceCents"], 0);
        assert_eq!(value["balances"], true);
        assert_eq!(
            value["assets"][0],
            json!({
                "accountId": "acc-1000",
                "code": "1000",
                "name": "Bank",
                "type": "asset",
                "role": "bank",
                "amountCents": 106_000,
                "postings": 4,
            })
        );
        assert_eq!(value["liabilities"][1]["code"], "2100");
        assert_eq!(
            value["equity"].as_array().map(Vec::len),
            Some(0),
            "an equity account nobody has posted to is not a line"
        );
    }

    #[test]
    fn an_ordinary_account_carries_no_role_rather_than_an_invented_one() {
        let mut sheet = year_end();
        sheet.assets[0].role = None;
        let value = report_json(&sheet);
        assert_eq!(value["assets"][0]["role"], Value::Null);
    }

    #[test]
    fn the_csv_is_the_three_sides_their_totals_the_result_and_the_proof() {
        let body = report_csv(&year_end());
        let lines: Vec<&str> = body.split("\r\n").filter(|l| !l.is_empty()).collect();
        assert_eq!(lines[0], "row,on,currency,accountCode,accountName,amount");
        assert_eq!(lines[1], "asset,2026-12-31,EUR,1000,Bank,1060.00");
        assert_eq!(
            lines[2],
            "asset,2026-12-31,EUR,1100,Trade receivables,1226.00"
        );
        assert_eq!(lines[3], "assetTotal,2026-12-31,EUR,,,2286.00");
        assert_eq!(lines[6], "liabilityTotal,2026-12-31,EUR,,,686.00");
        assert_eq!(
            lines[7], "equityTotal,2026-12-31,EUR,,,0.00",
            "an empty side is a zero, not a missing row"
        );
        assert_eq!(lines[8], "result,2026-12-31,EUR,,,1600.00");
        assert_eq!(
            lines[9], "liabilityEquityTotal,2026-12-31,EUR,,,2286.00",
            "the same figure as the assets, which is what balancing means"
        );
        assert_eq!(lines[10], "difference,2026-12-31,EUR,,,0.00");
        assert_eq!(lines.len(), 11, "no other rows: {body:?}");
    }

    #[test]
    fn a_sheet_that_does_not_balance_prints_the_difference_rather_than_hiding_it() {
        let mut broken = year_end();
        broken.liability_equity_cents = 188_600;
        broken.difference_cents = 40_000;
        let value = report_json(&broken);
        assert_eq!(value["differenceCents"], 40_000);
        assert_eq!(value["balances"], false);
        let body = report_csv(&broken);
        assert!(
            body.contains("difference,2026-12-31,EUR,,,400.00"),
            "{body}"
        );
    }

    #[test]
    fn a_negative_figure_stays_a_number_and_a_hostile_name_cannot_become_a_formula() {
        let mut sheet = year_end();
        sheet.assets[0].amount_cents = -5_000;
        sheet.assets[0].name = "=1+1".to_owned();
        sheet.result_cents = -1;
        let body = report_csv(&sheet);
        assert!(
            body.contains("asset,2026-12-31,EUR,1000,'=1+1,-50.00"),
            "{body}"
        );
        assert!(body.contains("result,2026-12-31,EUR,,,-0.01"), "{body}");
        // And an ordinary name is left exactly as the tenant typed it.
        assert!(body.contains(",Trade receivables,"), "{body}");
    }

    #[test]
    fn a_day_before_the_books_opened_is_still_a_file_with_its_totals() {
        let empty = BalanceSheet {
            on: on(2019, Month::December, 31),
            assets: Vec::new(),
            liabilities: Vec::new(),
            equity: Vec::new(),
            asset_cents: 0,
            liability_cents: 0,
            equity_cents: 0,
            result_cents: 0,
            liability_equity_cents: 0,
            difference_cents: 0,
            ..year_end()
        };
        let lines: Vec<String> = report_csv(&empty)
            .split("\r\n")
            .filter(|l| !l.is_empty())
            .map(str::to_owned)
            .collect();
        assert_eq!(
            lines.len(),
            7,
            "the header, the three side totals, and the three figures under them"
        );
        assert!(lines[6].starts_with("difference,"));
        assert!(lines[6].ends_with(",0.00"));
        assert_eq!(file_name(&empty), "balance-sheet-2019-12-31.csv");
    }
}
