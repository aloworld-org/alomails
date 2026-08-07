//! Exchange rates over HTTP (alo Billing, ADR 0035, wave B1.21) — the reference
//! rates a tenant's documents are converted at, on top of
//! [`alo_store::billing_fx_rates`].
//!
//! Three routes, one resource:
//!
//! - `GET /billing/fx/rates` — what this tenant has, newest day first,
//!   narrowable by currency and period.
//! - `PUT /billing/fx/rates` — one rate, by hand. A `PUT` rather than a `POST`
//!   because the rate of a currency on a day is a **single addressable fact**:
//!   sending it twice is the same fact, and re-sending it with a different
//!   number is a correction, not a second rate.
//! - `POST /billing/fx/rates/import` — a published reference-rate file, pasted
//!   or uploaded as `text/csv`. A `POST`, because one request creates many rows
//!   and its effect depends on what the file says.
//!
//! Conventions shared with the rest of the module ([`crate::billing`]):
//! authenticated and tenant-scoped through the account door, `Problem` errors,
//! and **no validation duplicated from the store** — the currency shape, the
//! rate bounds and the file format are all the store's rules, and a caller gets
//! them in the store's own words.
//!
//! Two things are specific to rates:
//!
//! - **A rate is written as the decimal it is published as** (`"1.1626"`), not
//!   as micro-units. A JSON number would arrive as a float and a rate that
//!   multiplies money never passes through one; the string goes to the store's
//!   own integer parser, the same one the file import uses. The response carries
//!   both forms — `rateMicro` for arithmetic, `rate` for display — so no client
//!   ever divides one into the other.
//! - **The import never fetches anything.** alo does not reach a third party
//!   from a tenant's request; the file is supplied by the caller, which is also
//!   what makes the conversion auditable (`docs/design/billing.md`).

use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::{Json, body::Bytes};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::billing_fx::format_rate;
use alo_store::billing_fx_rates::{FxImport, FxRate};

use crate::billing::{iso, iso_date, map_store_err, parse_body, parse_iso_date};
use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// The most a pasted or uploaded rate file may weigh: the whole published
/// history is a little over a megabyte, so this admits it with room to spare and
/// still refuses to read an arbitrary upload into memory.
const MAX_IMPORT_BYTES: usize = 8 * 1024 * 1024;

/// One stored rate as JSON.
fn rate_json(rate: &FxRate) -> Value {
    json!({
        "currency": rate.currency,
        "date": iso_date(rate.rate_date),
        // Both forms of the same integer: the micro-units a caller computes
        // with, and the decimal a person reads. Never a float.
        "rateMicro": rate.rate_micro,
        "rate": format_rate(rate.rate_micro),
        "source": rate.source.as_str(),
        "updatedBy": rate.updated_by,
        "updatedAt": iso(rate.updated_at),
    })
}

/// What an import did, in the words a confirmation needs.
fn import_json(summary: &FxImport) -> Value {
    json!({
        "rates": summary.rates,
        "days": summary.days,
        "currencies": summary.currencies,
        "from": summary.from.map(iso_date),
        "to": summary.to.map(iso_date),
    })
}

/// The list filter: a currency and a period, all optional.
#[derive(Deserialize)]
pub struct RatesQuery {
    #[serde(default)]
    currency: Option<String>,
    #[serde(default)]
    from: Option<String>,
    #[serde(default)]
    to: Option<String>,
}

/// One end of the period, or the `422` that names which end is wrong.
///
/// Unlike the VAT report's, both ends are **optional** here: a rate list with no
/// period is "everything I have", which is a sensible screen, whereas a VAT
/// summary without a period would put a figure under a heading nobody asked for.
/// A *stated* end still has to be a plain day.
fn day(name: &str, raw: Option<&str>) -> Result<Option<time::Date>, Problem> {
    let Some(raw) = raw.map(str::trim).filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    parse_iso_date(raw)
        .map(Some)
        .ok_or_else(|| bad_request(format!("{name} must be a date of the form YYYY-MM-DD")))
}

/// A `422` carrying the rule the caller broke.
fn bad_request(detail: String) -> Problem {
    Problem::with(StatusCode::UNPROCESSABLE_ENTITY, detail)
}

/// One rate as a client writes it.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RateBody {
    /// ISO 4217 code of the quoted currency.
    #[serde(default)]
    currency: String,
    /// The publication day, `YYYY-MM-DD`.
    #[serde(default)]
    date: String,
    /// The rate as published — units of `currency` per one euro — written as a
    /// decimal string (`"1.1626"`).
    #[serde(default)]
    rate: String,
}

/// `GET /billing/fx/rates?currency&from&to` → `{"rates":[…]}`.
///
/// Newest publication day first. Absent filters mean "everything", and the store
/// caps the answer.
pub async fn list_rates(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<RatesQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let from = day("from", query.from.as_deref())?;
    let to = day("to", query.to.as_deref())?;
    let rates = account
        .acc
        .billing_fx_rate_list(query.currency.as_deref(), from, to)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "rates": rates.iter().map(rate_json).collect::<Vec<_>>(),
    })))
}

/// `PUT /billing/fx/rates` `{"currency","date","rate"}` → `{"rate":{…}}` — one
/// rate, entered by hand.
///
/// Writing the same currency and day again **replaces** the rate: that is how a
/// typo, or a published correction, is fixed. Documents already issued are
/// unaffected — each carries its own frozen snapshot — so correcting the table
/// can never restate a document a customer already holds.
pub async fn put_rate(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: RateBody = parse_body(&body)?;
    let on = parse_iso_date(req.date.trim())
        .ok_or_else(|| bad_request("date must be a day of the form YYYY-MM-DD".to_owned()))?;
    // The rate text goes through the store's own integer parser — the same one
    // the file import uses — so "1,1626" and "1.16260001" are refused in one
    // place, in one wording.
    let rate_micro = alo_store::billing_fx::parse_rate(&req.rate).map_err(map_store_err)?;
    account
        .acc
        .save_billing_fx_rate(&req.currency, on, rate_micro)
        .await
        .map_err(map_store_err)?;
    // Read back rather than echoed: the answer is the stored row, with its
    // canonical currency code, its source and who wrote it.
    let stored = account
        .acc
        .billing_fx_rate_on(&req.currency, on)
        .await
        .map_err(map_store_err)?
        .ok_or_else(Problem::server_error)?;
    Ok(Json(json!({ "rate": rate_json(&stored) })))
}

/// `POST /billing/fx/rates/import` (`text/csv` body) → `{"import":{…}}` — a
/// published euro reference-rate file.
///
/// All or nothing: a file with one bad cell leaves the table exactly as it was
/// (`alo_store::billing_fx_ecb`), because a half-imported day would convert the
/// next document issued from rates the tenant believes it imported in full.
///
/// The body is read as text, not JSON: what a user has is a file, and asking
/// them to wrap a spreadsheet in JSON would mean escaping it correctly first.
pub async fn import_rates(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    if body.len() > MAX_IMPORT_BYTES {
        return Err(Problem::with(
            StatusCode::PAYLOAD_TOO_LARGE,
            "the rate file is too large; import it in parts",
        ));
    }
    let text = std::str::from_utf8(&body)
        .map_err(|_| bad_request("the rate file must be UTF-8 text".to_owned()))?;
    let summary = account
        .acc
        .import_billing_fx_rates(text)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "import": import_json(&summary) })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alo_store::billing_fx_rates::FxRateSource;
    use time::{Date, Month, OffsetDateTime};

    fn on(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).unwrap_or_else(|e| panic!("{e}"))
    }

    fn stored() -> FxRate {
        FxRate {
            currency: "USD".to_owned(),
            rate_date: on(2026, Month::August, 7),
            rate_micro: 1_162_600,
            source: FxRateSource::Ecb,
            updated_by: "u-1".to_owned(),
            updated_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn a_rate_is_reported_as_both_an_integer_and_the_decimal_it_was_published_as() {
        let value = rate_json(&stored());
        assert_eq!(value["currency"], "USD");
        assert_eq!(value["date"], "2026-08-07");
        assert_eq!(value["rateMicro"], json!(1_162_600));
        assert_eq!(value["rate"], "1.1626");
        assert_eq!(value["source"], "ecb");
        assert!(
            value["rateMicro"].is_i64(),
            "a rate is an integer on the wire, never a float"
        );
    }

    #[test]
    fn an_import_reports_what_it_wrote_and_the_days_it_spans() {
        let value = import_json(&FxImport {
            rates: 120,
            days: 4,
            currencies: 30,
            from: Some(on(2026, Month::August, 3)),
            to: Some(on(2026, Month::August, 7)),
        });
        assert_eq!(value["rates"], json!(120));
        assert_eq!(value["days"], json!(4));
        assert_eq!(value["currencies"], json!(30));
        assert_eq!(value["from"], "2026-08-03");
        assert_eq!(value["to"], "2026-08-07");
        // A file with no data rows spans no days, and says so with nulls rather
        // than with a made-up range.
        let empty = import_json(&FxImport::default());
        assert_eq!(empty["rates"], json!(0));
        assert_eq!(empty["from"], json!(null));
        assert_eq!(empty["to"], json!(null));
    }

    #[test]
    fn a_period_is_optional_on_a_rate_list_but_a_stated_end_must_be_a_plain_day() {
        assert_eq!(day("from", None).unwrap_or_default(), None);
        assert_eq!(day("from", Some("  ")).unwrap_or_default(), None);
        assert_eq!(
            day("to", Some(" 2026-08-07 ")).unwrap_or_default(),
            Some(on(2026, Month::August, 7))
        );
        for bad in ["07/08/2026", "2026-13-01", "2026-08-07T00:00:00Z", "today"] {
            let problem = day("from", Some(bad))
                .err()
                .unwrap_or_else(|| panic!("{bad:?} should have been refused"));
            assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY);
            assert_eq!(
                problem.detail.as_deref(),
                Some("from must be a date of the form YYYY-MM-DD")
            );
        }
    }

    #[test]
    fn a_rate_body_reads_the_decimal_as_written_and_never_as_a_number() {
        let body: RateBody =
            serde_json::from_str(r#"{"currency":"usd","date":"2026-08-07","rate":"1.1626"}"#)
                .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(body.currency, "usd", "the store uppercases it");
        assert_eq!(
            alo_store::billing_fx::parse_rate(&body.rate).unwrap_or_default(),
            1_162_600
        );
        // A JSON number is refused outright: a rate must not arrive as a float,
        // because a float is what makes 1.1626 sometimes 1.16259999.
        assert!(
            serde_json::from_str::<RateBody>(
                r#"{"currency":"USD","date":"2026-08-07","rate":1.1626}"#
            )
            .is_err()
        );
    }
}
