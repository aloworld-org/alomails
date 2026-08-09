//! Billing reports HTTP surface (alo Billing, ADR 0035, wave B1.20) — the VAT
//! summary of a period, over [`alo_store::billing_vat_report`].
//!
//! Two representations of one read: `GET /billing/reports/vat` answers JSON for
//! the screen, and `GET /billing/reports/vat.csv` answers the same figures as a
//! file for the accountant. They are separate paths rather than one route with
//! a `?format=`, exactly as `/print` and `/pdf` are: a URL that names its
//! representation is the one a browser can save under a sensible name and a
//! script can quote without a query string.
//!
//! It shares the conventions of [`crate::billing_invoices`] — authenticated and
//! tenant-scoped through the account door, `Problem` errors, no validation
//! duplicated from the store — and adds three of its own.
//!
//! - **The period is stated, never guessed.** Both `from` and `to` are
//!   required, both are plain days (`YYYY-MM-DD`), and a missing or malformed
//!   one is a `422`. A report that quietly defaulted to "this quarter" would
//!   put a figure under a heading the caller never asked for, which is the one
//!   thing a document copied onto a tax return must not do.
//! - **The CSV columns are a contract, in English.** They are read by scripts
//!   and by an accountant's own tooling, so they do not move with the user's
//!   interface language; what a *person* reads is the screen, which is
//!   translated. The amounts are plain decimals with a `.` separator and no
//!   grouping, for the same reason.
//! - **The file carries no customer data at all** — currencies, rates,
//!   amounts and counts, and nothing that names anybody. A summary that leaked
//!   a customer list into an emailed spreadsheet would be a promise broken for
//!   no gain.
//!
//! Since B1.21 both representations also carry the period **in the tenant's
//! accounting currency** — the figure a return is actually filed from, each
//! document converted at the rate frozen on it. Where any document could not be
//! converted, the count of those says so beside the total: a total that is
//! quietly missing a document is the one thing a tax figure must never be.

use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use serde::Deserialize;
use serde_json::{Value, json};
use time::Date;

use alo_store::billing_vat_report::{VatPeriod, VatPeriodBase, VatPeriodCurrency, VatPeriodRate};

use crate::billing::{iso_date, map_store_err, parse_iso_date};
use crate::csv;
use crate::error::Problem;
use crate::state::{AppState, authenticate};

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
    /// before it starts); what is checked here is only what a query string can
    /// get wrong on its own — an end that is absent, blank, or not a plain day.
    fn days(&self) -> Result<(Date, Date), Problem> {
        Ok((
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
                format!("{name} is required: a VAT summary is always for a stated period"),
            )
        })?;
    parse_iso_date(raw).ok_or_else(|| {
        Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("{name} must be a date of the form YYYY-MM-DD"),
        )
    })
}

/// One currency group as JSON.
fn currency_json(c: &VatPeriodCurrency) -> Value {
    json!({
        "currency": c.currency,
        "invoiceCount": c.invoice_count,
        "creditNoteCount": c.credit_note_count,
        "netCents": c.net_cents,
        "vatCents": c.vat_cents,
        "grossCents": c.gross_cents,
        // Named as a document's own breakdown is (`totals.vatByRate`), and in
        // the same shape, so a client reads one thing in both places.
        "byRate": c.by_rate.iter().map(rate_json).collect::<Vec<_>>(),
        // What this group contributes to the accounting-currency total, at the
        // rate frozen on each of its own documents.
        "baseNetCents": c.base_net_cents,
        "baseVatCents": c.base_vat_cents,
        "baseGrossCents": c.base_gross_cents,
        "unconvertedCount": c.unconverted_count,
    })
}

/// One rate's row of a breakdown — the same shape wherever a breakdown appears,
/// so a client reads one thing in a currency group and in the base total.
fn rate_json(r: &VatPeriodRate) -> Value {
    json!({
        "rateBp": r.rate_bp,
        "netCents": r.net_cents,
        "vatCents": r.vat_cents,
    })
}

/// The whole period in the accounting currency: the figure a VAT return is
/// copied from, and how many documents are missing from it.
fn base_json(b: &VatPeriodBase) -> Value {
    json!({
        "currency": b.currency,
        "netCents": b.net_cents,
        "vatCents": b.vat_cents,
        "grossCents": b.gross_cents,
        "byRate": b.by_rate.iter().map(rate_json).collect::<Vec<_>>(),
        "unconvertedCount": b.unconverted_count,
    })
}

/// The whole report as JSON, the period included: a figure a human copies onto
/// a return has to say which days it covers.
fn report_json(period: &VatPeriod) -> Value {
    json!({
        "from": iso_date(period.from),
        "to": iso_date(period.to),
        "currencies": period.currencies.iter().map(currency_json).collect::<Vec<_>>(),
        "base": base_json(&period.base),
    })
}

/// An integer-cents amount as the decimal a spreadsheet reads: two decimals, a
/// `.` separator, no grouping, and a leading `-` when it is negative.
///
/// Integer-only, like every other conversion of money in alo: the cents are
/// split into whole units and hundredths and printed, never divided by 100.0.
/// The absolute value is taken in `i128` so `i64::MIN` — which has no `i64`
/// absolute value — prints rather than panicking.
fn amount(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let abs = i128::from(cents).abs();
    format!("{sign}{}.{:02}", abs / 100, abs % 100)
}

/// A rate in basis points as the percentage a reader expects (2100 → `21.00`).
fn percent(rate_bp: i32) -> String {
    let sign = if rate_bp < 0 { "-" } else { "" };
    let abs = i64::from(rate_bp).abs();
    format!("{sign}{}.{:02}", abs / 100, abs % 100)
}

/// The CSV column names — a contract, deliberately not translated.
///
/// Grown additively at B1.21: `unconverted` is appended (a new column at the
/// end, never a reordering), and two new `row` kinds appear. A consumer reading
/// by column name is unaffected; one reading positionally still finds every
/// column it knew where it was.
const COLUMNS: [&str; 11] = [
    // Which kind of row you are reading — one table rather than four files:
    //   `rate`      a VAT rate's subtotal in the currency it was billed in
    //   `total`     that currency's own line
    //   `baseRate`  a VAT rate's subtotal across every currency, in the
    //               tenant's accounting currency
    //   `baseTotal` the period's total in that currency — the figure a VAT
    //               return is copied from
    "row",
    "periodFrom",
    "periodTo",
    "currency",
    "vatRatePercent",
    "net",
    "vat",
    "gross",
    "invoices",
    "creditNotes",
    // How many documents are NOT in this row's figures because their exchange
    // rate could not be applied. Empty on a row where the question does not
    // arise; `0` — or a number — on the rows that are converted totals.
    "unconverted",
];

/// The whole report as one CSV table: a `rate` row per VAT rate in each
/// currency, then that currency's `total` row, and finally the same period in
/// the accounting currency as `baseRate` rows and one `baseTotal` — the figure a
/// return is copied from.
///
/// The period is repeated on every row on purpose — a row lifted out of the
/// file into another sheet still says which days it covers — and the counts
/// appear only on the `total` row, because a document is counted once whatever
/// how many rates it used.
///
/// The `baseTotal` row is written even for an empty period: a file that does not
/// say which currency it was summarised in is a question rather than an answer.
fn report_csv(period: &VatPeriod) -> String {
    let from = iso_date(period.from);
    let to = iso_date(period.to);
    let mut out = csv::row(&COLUMNS);
    for c in &period.currencies {
        for r in &c.by_rate {
            out.push_str(&csv::row(&[
                "rate",
                &from,
                &to,
                &c.currency,
                &percent(r.rate_bp),
                &amount(r.net_cents),
                &amount(r.vat_cents),
                &amount(r.net_cents.saturating_add(r.vat_cents)),
                "",
                "",
                "",
            ]));
        }
        out.push_str(&csv::row(&[
            "total",
            &from,
            &to,
            &c.currency,
            "",
            &amount(c.net_cents),
            &amount(c.vat_cents),
            &amount(c.gross_cents),
            &c.invoice_count.to_string(),
            &c.credit_note_count.to_string(),
            &c.unconverted_count.to_string(),
        ]));
    }
    // Then the same period once more, in the currency the books are kept in:
    // every document at the rate frozen on it, which is the figure that goes on
    // the return. It is emitted even for an empty period, so a file always says
    // which currency it was summarised in.
    for r in &period.base.by_rate {
        out.push_str(&csv::row(&[
            "baseRate",
            &from,
            &to,
            &period.base.currency,
            &percent(r.rate_bp),
            &amount(r.net_cents),
            &amount(r.vat_cents),
            &amount(r.net_cents.saturating_add(r.vat_cents)),
            "",
            "",
            "",
        ]));
    }
    out.push_str(&csv::row(&[
        "baseTotal",
        &from,
        &to,
        &period.base.currency,
        "",
        &amount(period.base.net_cents),
        &amount(period.base.vat_cents),
        &amount(period.base.gross_cents),
        "",
        "",
        &period.base.unconverted_count.to_string(),
    ]));
    out
}

/// The file name a saved summary lands under: the report and the days it
/// covers, in ASCII, so nothing has to be escaped in the header or on a file
/// system.
fn file_name(period: &VatPeriod) -> String {
    format!(
        "vat-{}-to-{}.csv",
        iso_date(period.from),
        iso_date(period.to)
    )
}

/// `GET /billing/reports/vat?from&to` → `{"report":{…}}` — what was billed at
/// each VAT rate between two days, both included.
///
/// Computed from the documents themselves on every call: only those that stand
/// (`issued` and `paid`), judged on the issue date frozen on them, with credit
/// notes subtracting and each currency kept apart
/// (`docs/design/billing.md` § VAT summary).
pub async fn vat_report(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PeriodQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let (from, to) = query.days()?;
    let period = account
        .acc
        .billing_vat_period(from, to)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "report": report_json(&period) })))
}

/// `GET /billing/reports/vat.csv?from&to` → the same summary as a CSV file.
///
/// The same store read behind both routes, so the file an accountant opens and
/// the table the tenant is looking at cannot disagree about a cent.
pub async fn vat_report_csv(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PeriodQuery>,
) -> Result<Response, Problem> {
    let account = authenticate(&state, &headers).await?;
    let (from, to) = query.days()?;
    let period = account
        .acc
        .billing_vat_period(from, to)
        .await
        .map_err(map_store_err)?;
    Ok(csv::attachment(report_csv(&period), &file_name(&period)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alo_store::billing_vat_report::{VatPeriodBase, VatPeriodCurrency, VatPeriodRate};
    use time::Month;

    fn on(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).unwrap_or_else(|e| panic!("{e}"))
    }

    fn query(from: Option<&str>, to: Option<&str>) -> PeriodQuery {
        PeriodQuery {
            from: from.map(str::to_owned),
            to: to.map(str::to_owned),
        }
    }

    /// The quarter the store test seeds, with its hand-computed figures. A
    /// single-currency tenant, so its base side is its own figures unmoved.
    fn quarter() -> VatPeriod {
        let by_rate = vec![
            VatPeriodRate {
                rate_bp: 900,
                net_cents: 25_000,
                vat_cents: 2_250,
            },
            VatPeriodRate {
                rate_bp: 2100,
                net_cents: 102_997,
                vat_cents: 21_630,
            },
        ];
        VatPeriod {
            from: on(2025, Month::July, 1),
            to: on(2025, Month::September, 30),
            currencies: vec![VatPeriodCurrency {
                currency: "EUR".to_owned(),
                invoice_count: 5,
                credit_note_count: 1,
                net_cents: 127_997,
                vat_cents: 23_880,
                gross_cents: 151_877,
                by_rate: by_rate.clone(),
                base_net_cents: 127_997,
                base_vat_cents: 23_880,
                base_gross_cents: 151_877,
                unconverted_count: 0,
            }],
            base: VatPeriodBase {
                currency: "EUR".to_owned(),
                net_cents: 127_997,
                vat_cents: 23_880,
                gross_cents: 151_877,
                by_rate,
                unconverted_count: 0,
            },
        }
    }

    /// The same quarter with a dollar group whose documents are converted at
    /// 1 EUR = 1.1626 USD, and one that could not be converted at all.
    fn two_currencies() -> VatPeriod {
        let mut period = quarter();
        period.currencies.push(VatPeriodCurrency {
            currency: "USD".to_owned(),
            invoice_count: 2,
            credit_note_count: 0,
            net_cents: 70_000,
            vat_cents: 0,
            gross_cents: 70_000,
            by_rate: vec![VatPeriodRate {
                rate_bp: 0,
                net_cents: 70_000,
                vat_cents: 0,
            }],
            base_net_cents: 43_007,
            base_vat_cents: 0,
            base_gross_cents: 43_007,
            unconverted_count: 1,
        });
        period.base.net_cents += 43_007;
        period.base.gross_cents += 43_007;
        period.base.by_rate.insert(
            0,
            VatPeriodRate {
                rate_bp: 0,
                net_cents: 43_007,
                vat_cents: 0,
            },
        );
        period.base.unconverted_count = 1;
        period
    }

    #[test]
    fn both_ends_of_the_period_are_required() {
        for (from, to, expected) in [
            (None, Some("2025-09-30"), "from"),
            (Some("2025-07-01"), None, "to"),
            (Some(""), Some("2025-09-30"), "from"),
            (Some("2025-07-01"), Some("   "), "to"),
        ] {
            let problem = query(from, to)
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
    fn an_end_that_is_not_a_plain_day_is_refused_never_guessed_at() {
        for bad in ["01/07/2025", "2025-13-01", "2025-07-01T00:00:00Z", "July"] {
            let problem = query(Some(bad), Some("2025-09-30"))
                .days()
                .err()
                .unwrap_or_else(|| panic!("{bad:?} should have been refused"));
            assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY, "{bad:?}");
            assert_eq!(
                problem.detail.as_deref(),
                Some("from must be a date of the form YYYY-MM-DD")
            );
        }
    }

    #[test]
    fn a_well_formed_period_is_read_as_two_plain_days() {
        let (from, to) = query(Some("2025-07-01"), Some(" 2025-09-30 "))
            .days()
            .unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(from, on(2025, Month::July, 1));
        assert_eq!(to, on(2025, Month::September, 30));
        // A backwards period is the store's refusal, not this layer's: it is a
        // rule about the pair, and one place must own it.
        assert!(query(Some("2025-09-30"), Some("2025-07-01")).days().is_ok());
    }

    #[test]
    fn cents_print_as_the_decimal_a_spreadsheet_reads() {
        assert_eq!(amount(0), "0.00");
        assert_eq!(amount(5), "0.05");
        assert_eq!(amount(127_997), "1279.97");
        assert_eq!(amount(-10_500), "-105.00");
        assert_eq!(amount(-7), "-0.07");
        // Total, for a caller that ignores the store's bounds: no panic, no
        // wrapped sign.
        assert_eq!(amount(i64::MIN), "-92233720368547758.08");
    }

    #[test]
    fn a_rate_prints_as_a_percentage() {
        assert_eq!(percent(0), "0.00");
        assert_eq!(percent(900), "9.00");
        assert_eq!(percent(2100), "21.00");
        assert_eq!(percent(2_150), "21.50");
        assert_eq!(percent(10_000), "100.00");
    }

    #[test]
    fn the_json_report_states_its_period_and_every_figure_in_cents() {
        let value = report_json(&quarter());
        assert_eq!(value["from"], "2025-07-01");
        assert_eq!(value["to"], "2025-09-30");
        let eur = &value["currencies"][0];
        assert_eq!(eur["currency"], "EUR");
        assert_eq!(eur["invoiceCount"], 5);
        assert_eq!(eur["creditNoteCount"], 1);
        assert_eq!(eur["netCents"], 127_997);
        assert_eq!(eur["vatCents"], 23_880);
        assert_eq!(eur["grossCents"], 151_877);
        assert_eq!(
            eur["byRate"],
            json!([
                { "rateBp": 900, "netCents": 25_000, "vatCents": 2_250 },
                { "rateBp": 2100, "netCents": 102_997, "vatCents": 21_630 },
            ])
        );
        // And the figure the return is filed from, in the currency the books are
        // kept in — here the same figure, because the tenant bills in it.
        assert_eq!(value["base"]["currency"], "EUR");
        assert_eq!(value["base"]["netCents"], 127_997);
        assert_eq!(value["base"]["vatCents"], 23_880);
        assert_eq!(value["base"]["grossCents"], 151_877);
        assert_eq!(value["base"]["unconvertedCount"], 0);
        assert_eq!(value["base"]["byRate"], eur["byRate"]);
        assert_eq!(eur["baseNetCents"], 127_997);
        assert_eq!(eur["unconvertedCount"], 0);
    }

    #[test]
    fn a_second_currency_reports_its_own_figures_and_what_it_contributes() {
        let value = report_json(&two_currencies());
        let usd = &value["currencies"][1];
        assert_eq!(usd["currency"], "USD");
        assert_eq!(usd["netCents"], 70_000, "in dollars, as billed");
        assert_eq!(usd["baseNetCents"], 43_007, "in euro, as booked");
        assert_eq!(
            usd["unconvertedCount"], 1,
            "and one document that is in neither figure, said out loud"
        );
        assert_eq!(value["base"]["netCents"], 127_997 + 43_007);
        assert_eq!(value["base"]["unconvertedCount"], 1);
    }

    #[test]
    fn the_csv_is_one_table_of_rate_rows_and_a_total_row() {
        let body = report_csv(&quarter());
        let lines: Vec<&str> = body.split("\r\n").filter(|l| !l.is_empty()).collect();
        assert_eq!(
            lines[0],
            "row,periodFrom,periodTo,currency,vatRatePercent,net,vat,gross,invoices,creditNotes,\
             unconverted"
        );
        assert_eq!(
            lines[1],
            "rate,2025-07-01,2025-09-30,EUR,9.00,250.00,22.50,272.50,,,"
        );
        assert_eq!(
            lines[2],
            "rate,2025-07-01,2025-09-30,EUR,21.00,1029.97,216.30,1246.27,,,"
        );
        assert_eq!(
            lines[3], "total,2025-07-01,2025-09-30,EUR,,1279.97,238.80,1518.77,5,1,0",
            "the counts are on the total row, where a document is counted once"
        );
        // Then the same period in the accounting currency: the return's figures.
        assert_eq!(
            lines[4],
            "baseRate,2025-07-01,2025-09-30,EUR,9.00,250.00,22.50,272.50,,,"
        );
        assert_eq!(
            lines[5],
            "baseRate,2025-07-01,2025-09-30,EUR,21.00,1029.97,216.30,1246.27,,,"
        );
        assert_eq!(
            lines[6],
            "baseTotal,2025-07-01,2025-09-30,EUR,,1279.97,238.80,1518.77,,,0"
        );
        assert_eq!(lines.len(), 7, "no other rows: {body:?}");
    }

    #[test]
    fn the_csv_says_how_many_documents_are_missing_from_a_converted_total() {
        let body = report_csv(&two_currencies());
        let lines: Vec<&str> = body.split("\r\n").filter(|l| !l.is_empty()).collect();
        assert_eq!(
            lines[5], "total,2025-07-01,2025-09-30,USD,,700.00,0.00,700.00,2,0,1",
            "the dollar group carries its own unconverted count"
        );
        let base_total = lines
            .iter()
            .find(|l| l.starts_with("baseTotal"))
            .unwrap_or_else(|| panic!("{body}"));
        assert_eq!(
            *base_total, "baseTotal,2025-07-01,2025-09-30,EUR,,1710.04,238.80,1948.84,,,1",
            "a total that is missing a document says so on the same row"
        );
    }

    #[test]
    fn an_empty_period_still_answers_a_file_with_its_header() {
        let empty = VatPeriod {
            from: on(2026, Month::January, 1),
            to: on(2026, Month::March, 31),
            currencies: Vec::new(),
            base: VatPeriodBase {
                currency: "EUR".to_owned(),
                ..VatPeriodBase::default()
            },
        };
        // The header, and the one row that says what the nothing is nothing in.
        assert_eq!(
            report_csv(&empty),
            format!(
                "{}{}",
                csv::row(&COLUMNS),
                csv::row(&[
                    "baseTotal",
                    "2026-01-01",
                    "2026-03-31",
                    "EUR",
                    "",
                    "0.00",
                    "0.00",
                    "0.00",
                    "",
                    "",
                    "0",
                ])
            )
        );
        assert_eq!(file_name(&empty), "vat-2026-01-01-to-2026-03-31.csv");
    }

    #[test]
    fn the_summary_is_saved_under_the_days_it_covers() {
        // The headers the file is served under are [`crate::csv::attachment`]'s
        // and are asserted there, once, for every export in alo.
        assert_eq!(file_name(&quarter()), "vat-2025-07-01-to-2025-09-30.csv");
    }
}
