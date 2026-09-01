//! Cash-flow forecasting over the same receivable and payable ledgers as the
//! Finance aged reports. The browser supplies dates and scenario delays, never
//! totals: every amount and projected balance is folded here.

use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use time::{Date, Duration};

use alo_store::{AgedReport, AgedSide};

use crate::billing::{iso_date, map_store_err};
use crate::error::Problem;
use crate::finance_reports::{day, reader};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct ForecastQuery {
    #[serde(default)]
    on: Option<String>,
    #[serde(default)]
    horizon: Option<i64>,
    #[serde(default, rename = "receivableDelay")]
    receivable_delay: Option<i64>,
    #[serde(default, rename = "payableDelay")]
    payable_delay: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Point {
    from: Date,
    to: Date,
    incoming_cents: i64,
    outgoing_cents: i64,
    projected_cents: Option<i64>,
}

fn delay(value: Option<i64>, name: &str) -> Result<i64, Problem> {
    let value = value.unwrap_or(0);
    if !(-30..=90).contains(&value) {
        return Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("{name} must be between -30 and 90 days"),
        ));
    }
    Ok(value)
}

fn add(
    report: &AgedReport,
    on: Date,
    horizon: i64,
    shift: i64,
    incoming: bool,
    points: &mut [Point],
) {
    for document in report.parties.iter().flat_map(|party| &party.documents) {
        let Some(amount) = document.base_open_cents else {
            continue;
        };
        let expected = (document.due_date + Duration::days(shift)).max(on);
        let days = (expected - on).whole_days();
        if days > horizon {
            continue;
        }
        let index = usize::try_from(days / 7)
            .unwrap_or(0)
            .min(points.len().saturating_sub(1));
        if incoming {
            points[index].incoming_cents += amount;
        } else {
            points[index].outgoing_cents += amount;
        }
    }
}

fn project(
    receivables: &AgedReport,
    payables: &AgedReport,
    on: Date,
    horizon: i64,
    receivable_delay: i64,
    payable_delay: i64,
    opening: Option<i64>,
) -> Vec<Point> {
    let count = usize::try_from((horizon + 6) / 7).unwrap_or(1);
    let mut points = (0..count)
        .map(|index| {
            let start = on + Duration::days(i64::try_from(index).unwrap_or(0) * 7);
            Point {
                from: start,
                to: (start + Duration::days(6)).min(on + Duration::days(horizon)),
                incoming_cents: 0,
                outgoing_cents: 0,
                projected_cents: None,
            }
        })
        .collect::<Vec<_>>();
    add(
        receivables,
        on,
        horizon,
        receivable_delay,
        true,
        &mut points,
    );
    add(payables, on, horizon, payable_delay, false, &mut points);
    let mut balance = opening;
    for point in &mut points {
        balance = balance.map(|current| current + point.incoming_cents - point.outgoing_cents);
        point.projected_cents = balance;
    }
    points
}

/// `GET /finance/forecast?on&horizon&receivableDelay&payableDelay` returns the
/// weekly cash movement expected from open customer and supplier documents.
pub async fn cash_forecast(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ForecastQuery>,
) -> Result<Json<Value>, Problem> {
    let account = reader(&state, &headers).await?;
    let on = day("on", query.on.as_deref())?;
    let horizon = query.horizon.unwrap_or(30);
    if !matches!(horizon, 30 | 60 | 90) {
        return Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "horizon must be 30, 60 or 90 days".to_owned(),
        ));
    }
    let receivable_delay = delay(query.receivable_delay, "receivableDelay")?;
    let payable_delay = delay(query.payable_delay, "payableDelay")?;
    let (receivables, payables, statements) = tokio::try_join!(
        account.acc.fin_aged(on, AgedSide::Receivable),
        account.acc.fin_aged(on, AgedSide::Payable),
        account.acc.bank_statements(),
    )
    .map_err(map_store_err)?;
    let opening = statements.first().and_then(|statement| {
        (statement.currency == receivables.currency)
            .then_some(statement.closing_balance_cents)
            .flatten()
    });
    let points = project(
        &receivables,
        &payables,
        on,
        horizon,
        receivable_delay,
        payable_delay,
        opening,
    );
    Ok(Json(json!({
        "forecast": {
            "on": iso_date(on), "through": iso_date(on + Duration::days(horizon)),
            "horizonDays": horizon, "currency": receivables.currency,
            "openingBalanceCents": opening,
            "receivableDelayDays": receivable_delay, "payableDelayDays": payable_delay,
            "unconvertedReceivables": receivables.unconverted_count,
            "unconvertedPayables": payables.unconverted_count,
            "points": points.iter().map(|point| json!({
                "from": iso_date(point.from), "to": iso_date(point.to),
                "incomingCents": point.incoming_cents, "outgoingCents": point.outgoing_cents,
                "netCents": point.incoming_cents - point.outgoing_cents,
                "projectedBalanceCents": point.projected_cents,
            })).collect::<Vec<_>>()
        }
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alo_store::{AgedBucket, AgedBuckets, AgedDocument, AgedParty};
    use time::Month;

    fn date(day: u8) -> Date {
        Date::from_calendar_date(2026, Month::September, day)
            .unwrap_or_else(|error| panic!("{error}"))
    }

    fn report(side: AgedSide, due: u8, cents: i64) -> AgedReport {
        AgedReport {
            on: date(1),
            side,
            currency: "EUR".to_owned(),
            parties: vec![AgedParty {
                party_id: "party-1".to_owned(),
                name: "Party".to_owned(),
                buckets: AgedBuckets::default(),
                unconverted_count: 0,
                documents: vec![AgedDocument {
                    document_id: "doc-1".to_owned(),
                    number: "1".to_owned(),
                    issue_date: date(1),
                    due_date: date(due),
                    days_overdue: 0,
                    bucket: AgedBucket::Current,
                    currency: "EUR".to_owned(),
                    open_cents: cents,
                    base_open_cents: Some(cents),
                    is_credit_note: false,
                }],
            }],
            buckets: AgedBuckets::default(),
            unconverted_count: 0,
            document_count: 1,
        }
    }

    #[test]
    fn scenarios_shift_documents_and_keep_the_running_balance_on_the_server() {
        let receivable = report(AgedSide::Receivable, 8, 10_000);
        let payable = report(AgedSide::Payable, 10, 3_000);
        let points = project(&receivable, &payable, date(1), 30, 7, 0, Some(20_000));
        assert_eq!(points[1].outgoing_cents, 3_000);
        assert_eq!(points[2].incoming_cents, 10_000);
        assert_eq!(points[2].projected_cents, Some(27_000));
    }

    #[test]
    fn scenario_delays_are_bounded() {
        assert!(delay(Some(90), "delay").is_ok());
        assert!(delay(Some(91), "delay").is_err());
    }
}
