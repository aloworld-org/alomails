//! Reading the European Central Bank's euro reference-rate file (alo Billing,
//! ADR 0035, wave B1.21).
//!
//! The ECB publishes one euro against every quoted currency on each TARGET
//! working day, at around 16:00 CET, as a small CSV: a header naming the
//! currencies and one row per day.
//!
//! ```text
//! Date, USD, JPY, BGN, ...
//! 07 August 2026, 1.1626, 171.42, 1.9558, ...
//! ```
//!
//! This module is **pure** — it turns that text into rows, and nothing else. It
//! performs **no network access**: a sovereignty product does not silently reach
//! a third party from a tenant's transaction, and the loop that built this is
//! forbidden from external calls at all. The file arrives because somebody
//! uploaded or pasted it (`/billing/fx/rates/import`), which is also what makes
//! the import auditable — a tenant can say exactly which published file its
//! books were converted from.
//!
//! What it accepts, and why each is deliberate:
//!
//! - **Both published shapes.** `eurofxref.csv` (one day), `eurofxref-hist.csv`
//!   (every day since 1999) and the 90-day file have the same layout, so one
//!   parser reads all three and a tenant can seed history and then keep it up
//!   to date with the daily file.
//! - **ISO days only** (`2026-08-07`). The published file uses ISO in the
//!   historical variants and `07 August 2026` in some daily ones; a
//!   month *name* is a language, and reading one would mean carrying a table of
//!   month names per locale and guessing which locale a file is in. An ISO day
//!   is unambiguous, and a caller with a name-formatted file converts one
//!   column rather than risking a rate landing on the wrong day.
//! - **`N/A` is a gap, not an error.** The ECB stops and starts quoting
//!   currencies (a member state joining the euro, a market closing), and those
//!   cells read `N/A`. A file that is 90 % gaps must still import the 10 % that
//!   is real.
//! - **A euro column is ignored.** The file quotes *against* the euro, so a
//!   `EUR` column would be the constant 1 and storing it would invite a second,
//!   redundant answer to "what is the euro worth in euro".
//! - **A malformed rate is refused, and the whole import fails.** Half an
//!   imported day is worse than none: a document issued the next minute would be
//!   converted from a file the tenant believes it imported in full. The refusal
//!   names the row and the column so the file can be fixed.
//!
//! The rows come back exactly as the file states them — the day, the currency,
//! and the micro-units of that currency per one euro
//! ([`crate::billing_fx::RATE_SCALE`]). Which of them are new, which are
//! corrections, and which tenant they belong to is [`crate::billing_fx_rates`]'s
//! business.

use time::Date;
use time::format_description::well_known::Iso8601;

use crate::billing_field::currency;
use crate::billing_fx::parse_rate;
use crate::error::{Result, StoreError};

/// The currency the file quotes everything against. A column for it carries no
/// information and is skipped.
pub const QUOTED_AGAINST: &str = "EUR";

/// The most rows one import may carry: the full historical file is a little
/// over 7 000 days, so this admits it whole with room for the decades to come
/// and still bounds what one request can make the database do.
pub const MAX_DAYS: usize = 20_000;

/// The most currencies one file may quote. The ECB quotes around thirty; a file
/// with hundreds of columns is not the file this parser is for.
pub const MAX_CURRENCIES: usize = 200;

/// One published rate: on this day, one euro bought this much of this currency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishedRate {
    /// The day the rate was published.
    pub day: Date,
    /// ISO 4217 code of the quoted currency, uppercase.
    pub currency: String,
    /// Micro-units of `currency` per one euro.
    pub rate_micro: i64,
}

/// Reads a euro reference-rate file into the rates it states, in file order.
///
/// The result may be empty — a header with no data rows is a legitimate (if
/// useless) file, and reporting "0 rates imported" is more honest than
/// inventing an error the file does not contain.
///
/// # Errors
/// [`StoreError::Validation`] naming the row and column when the header is not
/// a reference-rate header, a day is not an ISO date, a rate is not a positive
/// decimal, a currency code is not three letters, or the file is larger than
/// [`MAX_DAYS`] × [`MAX_CURRENCIES`].
pub fn parse_reference_rates(text: &str) -> Result<Vec<PublishedRate>> {
    let mut rows = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .enumerate();

    let (_, header) = rows.next().ok_or_else(|| {
        StoreError::Validation(
            "the file is empty; a reference-rate file starts with a Date header row".to_owned(),
        )
    })?;
    let currencies = header_currencies(header)?;

    let mut out = Vec::new();
    let mut days = 0_usize;
    for (index, line) in rows {
        // Row numbers are the file's own, counted from 1 including the header,
        // so a person looking at the file in a spreadsheet finds the row we
        // mean.
        let row = index + 1;
        days += 1;
        if days > MAX_DAYS {
            return Err(StoreError::Validation(format!(
                "the file holds more than {MAX_DAYS} days; split it"
            )));
        }
        let mut cells = line.split(',').map(str::trim);
        let day = cells.next().unwrap_or_default();
        let day = Date::parse(day, &Iso8601::DATE).map_err(|_| {
            StoreError::Validation(format!(
                "row {row}: the first column must be a day of the form YYYY-MM-DD"
            ))
        })?;
        for (column, cell) in cells.enumerate() {
            // A row with more *values* than the header names currencies is a
            // misread file, not a generous one — most often a rate written with
            // a comma for its decimal point, which would otherwise import as a
            // whole number and misstate every amount it converts. A trailing
            // separator (the daily file ends its rows with one) is empty and is
            // therefore not a value.
            if column >= currencies.len() {
                if cell.is_empty() {
                    continue;
                }
                return Err(StoreError::Validation(format!(
                    "row {row}: more values than the header names currencies — check for a \
                     rate written with a ',' decimal separator"
                )));
            }
            // A currency-less column (the euro's) is skipped, and a blank cell
            // is a gap, not a zero.
            let Some(code) = currencies.get(column).filter(|code| !code.is_empty()) else {
                continue;
            };
            if cell.is_empty() || cell.eq_ignore_ascii_case("N/A") {
                continue;
            }
            let rate_micro = parse_rate(cell).map_err(|_| {
                StoreError::Validation(format!(
                    "row {row}, column {code}: the rate must be a positive decimal with at \
                     most 6 decimal places"
                ))
            })?;
            out.push(PublishedRate {
                day,
                currency: code.clone(),
                rate_micro,
            });
        }
    }
    Ok(out)
}

/// The currency of each column after the first, with the euro column (and any
/// trailing empty one) reduced to a blank that the reader skips.
fn header_currencies(header: &str) -> Result<Vec<String>> {
    let mut cells = header.split(',').map(str::trim);
    let first = cells.next().unwrap_or_default();
    if !first.eq_ignore_ascii_case("date") {
        return Err(StoreError::Validation(
            "the first column of a reference-rate file must be headed Date".to_owned(),
        ));
    }
    let mut currencies = Vec::new();
    for cell in cells {
        if currencies.len() >= MAX_CURRENCIES {
            return Err(StoreError::Validation(format!(
                "the file quotes more than {MAX_CURRENCIES} currencies; it is not a \
                 reference-rate file"
            )));
        }
        if cell.is_empty() || cell.eq_ignore_ascii_case(QUOTED_AGAINST) {
            // Blank keeps the column *positions* aligned with the data rows,
            // which is why this is not simply filtered out.
            currencies.push(String::new());
            continue;
        }
        currencies.push(currency(cell).map_err(|_| {
            StoreError::Validation(
                "every column after Date must be headed by a three-letter ISO 4217 code".to_owned(),
            )
        })?);
    }
    if currencies.iter().all(String::is_empty) {
        return Err(StoreError::Validation(
            "the file quotes no currency; a reference-rate file has a currency column per rate"
                .to_owned(),
        ));
    }
    Ok(currencies)
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Month;

    fn day(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).unwrap_or_else(|e| panic!("{e}"))
    }

    fn message(result: Result<Vec<PublishedRate>>) -> String {
        match result {
            Err(StoreError::Validation(msg)) => msg,
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    /// The daily file's shape: spaces after the separators and a trailing one.
    const DAILY: &str = "Date, USD, JPY, BGN, CZK\n2026-08-07, 1.1626, 171.42, 1.9558, 24.615, \n";

    #[test]
    fn the_daily_file_reads_as_one_days_rates() {
        let rates = parse_reference_rates(DAILY).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(
            rates,
            vec![
                PublishedRate {
                    day: day(2026, Month::August, 7),
                    currency: "USD".to_owned(),
                    rate_micro: 1_162_600,
                },
                PublishedRate {
                    day: day(2026, Month::August, 7),
                    currency: "JPY".to_owned(),
                    rate_micro: 171_420_000,
                },
                PublishedRate {
                    day: day(2026, Month::August, 7),
                    currency: "BGN".to_owned(),
                    rate_micro: 1_955_800,
                },
                PublishedRate {
                    day: day(2026, Month::August, 7),
                    currency: "CZK".to_owned(),
                    rate_micro: 24_615_000,
                },
            ],
            "in file order, one row per quoted currency"
        );
    }

    #[test]
    fn the_historical_file_reads_every_day_it_states() {
        // Newest-first, as published, and CRLF as served.
        let text = "Date,USD,JPY\r\n2026-08-07,1.1626,171.42\r\n2026-08-06,1.1602,170.98\r\n";
        let rates = parse_reference_rates(text).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(rates.len(), 4);
        assert_eq!(rates[0].day, day(2026, Month::August, 7));
        assert_eq!(rates[3].day, day(2026, Month::August, 6));
        assert_eq!(rates[3].currency, "JPY");
        assert_eq!(rates[3].rate_micro, 170_980_000);
    }

    #[test]
    fn a_gap_is_skipped_and_the_rest_of_the_day_still_imports() {
        // `N/A` is what the file says for a currency it has stopped quoting;
        // an empty cell is the same fact with less ceremony.
        let text = "Date,USD,RUB,TRY\n2026-08-07,1.1626,N/A,\n";
        let rates = parse_reference_rates(text).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(rates.len(), 1);
        assert_eq!(rates[0].currency, "USD");
        // Case does not decide whether a cell is a gap.
        let lower = parse_reference_rates("Date,USD,RUB\n2026-08-07,1.1626,n/a\n")
            .unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(lower.len(), 1);
    }

    #[test]
    fn a_euro_column_is_ignored_rather_than_stored_as_one() {
        let text = "Date,EUR,USD\n2026-08-07,1.0000,1.1626\n";
        let rates = parse_reference_rates(text).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(rates.len(), 1, "the euro is what everything is quoted in");
        assert_eq!(rates[0].currency, "USD");
        assert_eq!(rates[0].rate_micro, 1_162_600);
    }

    #[test]
    fn a_header_that_is_not_a_reference_rate_header_is_refused() {
        for bad in [
            "",
            "   \n\n",
            "Day,USD\n2026-08-07,1.1626\n",
            "Date\n2026-08-07\n",
            "Date,EUR\n2026-08-07,1.0\n",
            "Date,DOLLAR\n2026-08-07,1.1626\n",
        ] {
            assert!(
                matches!(parse_reference_rates(bad), Err(StoreError::Validation(_))),
                "expected refusal: {bad:?}"
            );
        }
        assert!(message(parse_reference_rates("Day,USD\n")).contains("headed Date"));
        assert!(
            message(parse_reference_rates("Date,EUR\n")).contains("quotes no currency"),
            "a file of nothing but the euro column quotes nothing"
        );
    }

    #[test]
    fn a_day_that_is_not_iso_is_refused_naming_its_row() {
        for bad in [
            "Date,USD\n07 August 2026,1.1626\n",
            "Date,USD\n07/08/2026,1.1626\n",
            "Date,USD\n2026-13-01,1.1626\n",
            "Date,USD\n,1.1626\n",
        ] {
            let detail = message(parse_reference_rates(bad));
            assert!(detail.starts_with("row 2:"), "{detail}");
            assert!(detail.contains("YYYY-MM-DD"), "{detail}");
        }
    }

    #[test]
    fn a_malformed_rate_fails_the_whole_import_naming_row_and_column() {
        let detail = message(parse_reference_rates(
            "Date,USD,JPY\n2026-08-07,1.1626,171.42\n2026-08-06,1.16,17O.98\n",
        ));
        assert_eq!(
            detail,
            "row 3, column JPY: the rate must be a positive decimal with at most 6 decimal places",
            "the row is the file's own row number, header included"
        );
        // Half an imported day is worse than none: nothing comes back at all.
        assert!(parse_reference_rates("Date,USD\n2026-08-07,0\n").is_err());
        assert!(parse_reference_rates("Date,USD\n2026-08-07,-1.5\n").is_err());
        assert!(parse_reference_rates("Date,USD\n2026-08-07,1,1626\n").is_err());
    }

    #[test]
    fn a_file_larger_than_the_bound_is_refused_rather_than_executed() {
        let mut text = String::from("Date,USD\n");
        for _ in 0..=MAX_DAYS {
            text.push_str("2026-08-07,1.1626\n");
        }
        assert!(message(parse_reference_rates(&text)).contains("more than"));
        let wide: String = (0..=MAX_CURRENCIES)
            .map(|_| ",USD".to_owned())
            .collect::<String>();
        assert!(message(parse_reference_rates(&format!("Date{wide}\n"))).contains("more than"));
    }

    #[test]
    fn a_row_with_fewer_cells_than_the_header_imports_what_it_has() {
        // A truncated last line is common in a hand-edited file; the currencies
        // it does state are still stated.
        let rates = parse_reference_rates("Date,USD,JPY,CHF\n2026-08-07,1.1626\n")
            .unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(rates.len(), 1);
        assert_eq!(rates[0].currency, "USD");
    }
}
