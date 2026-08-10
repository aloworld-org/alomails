//! The mileage HTTP surface (alo Finance, ADR 0035, wave B4.07) — the tenant's
//! per-kilometre rate table, and the journeys it turns into claims — over
//! [`alo_store::fin_mileage`].
//!
//! Two collections, on purpose, with two different answers to "whose is this":
//!
//! - **`/finance/mileage/rates`** is tenant-wide configuration. Everybody
//!   **reads** it, because a traveller needs to know what a kilometre is worth
//!   before deciding to drive; only an **admin writes** it
//!   (`Account::require_admin`), because a rate table anybody could raise is a
//!   self-service pay rise. This is the one privileged finance write B4.12
//!   deliberately did NOT widen to the accountant: the rate a company pays its
//!   people for driving is a pay decision the company takes, not a bookkeeping
//!   one — the accountant records journeys at it, and the design note's list of
//!   accountant writes (manual entries, matches, expense decisions, the period
//!   lock) does not include it. That gate is a decision this file makes and the
//!   store deliberately does not: the store's job is that the write is the
//!   tenant's, the edge's job is that it is the right person's.
//! - **`/finance/mileage`** is the caller's own journeys. There is no `userId`
//!   anywhere in this module, exactly as in [`crate::finance_expenses`]: a
//!   journey places a named person at an address on a date, and the store has no
//!   function on that door which takes somebody else's id. The approver sees the
//!   *claim* in the ordinary inbox; they do not get a second, mileage-shaped
//!   window onto where people have been.
//!
//! Three consequences of "mileage is a claim at a rate table" that this file
//! makes visible on the wire, each decided in `docs/design/finance.md`:
//!
//! - **The client never states the amount.** `POST /finance/mileage` takes a day
//!   and a distance; the money comes back computed from the rate in force on
//!   that day. There is no `grossCents` in the request body to send.
//! - **The rate is snapshotted**, so a journey's answer carries
//!   `rateCentsPerKm` — the rate it was paid at, not the rate the table holds
//!   today.
//! - **There is no `PATCH`.** Correcting a journey is deleting it and stating
//!   the right one, which re-reads the rate table; an edit that kept a rate
//!   picked for a day it no longer claims would be a figure nobody can derive.
//!
//! Places and reasons are personal data — they can name a clinic, a client or an
//! occasion — so nothing in this module logs one.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::{
    FinCategoryId, FinMileageId, MileageClaim, MileageRate, NewMileage, NewMileageRate, ProjectId,
};

use crate::billing::{blank_to_none, iso, iso_date, map_store_err, parse_body};
use crate::error::Problem;
use crate::finance_expenses::{expense_json, period_bounds, stated_day};
use crate::state::{AppState, authenticate};

/// One row of the rate table as JSON.
fn rate_json(rate: &MileageRate) -> Value {
    json!({
        "id": rate.id.as_str(),
        "effectiveFrom": iso_date(rate.effective_from),
        "centsPerKm": rate.cents_per_km,
        "note": rate.note,
        "createdAt": iso(rate.created_at),
        "updatedAt": iso(rate.updated_at),
    })
}

/// One journey as JSON, with the claim it became nested whole.
///
/// The claim is rendered by [`expense_json`] — the same function the claimant's
/// own expense routes use — so a mileage claim and a train ticket can never
/// start describing their status, their editability or their money differently.
fn mileage_json(claim: &MileageClaim) -> Value {
    let j = &claim.journey;
    json!({
        "id": j.id.as_str(),
        "travelledOn": iso_date(j.travelled_on),
        "kmMilli": j.km_milli,
        "rateCentsPerKm": j.rate_cents_per_km,
        "fromPlace": j.from_place,
        "toPlace": j.to_place,
        "reason": j.reason,
        "expenseId": j.expense_id.as_str(),
        "createdAt": iso(j.created_at),
        "expense": expense_json(&claim.expense),
    })
}

/// The period one read of a person's journeys asks for.
#[derive(Deserialize)]
pub struct MileageQuery {
    #[serde(default)]
    from: Option<String>,
    #[serde(default)]
    to: Option<String>,
}

/// The journey a claim is made of. No amount: the rate table decides that.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MileageBody {
    #[serde(default)]
    travelled_on: Option<String>,
    #[serde(default)]
    km_milli: Option<i64>,
    #[serde(default)]
    from_place: Option<String>,
    #[serde(default)]
    to_place: Option<String>,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    category_id: Option<String>,
    #[serde(default)]
    project_id: Option<String>,
}

/// The whole rate table, as a replace states it.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RatesBody {
    #[serde(default)]
    rates: Vec<RateBody>,
}

/// One stated rate.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RateBody {
    #[serde(default)]
    effective_from: Option<String>,
    #[serde(default)]
    cents_per_km: Option<i64>,
    #[serde(default)]
    note: Option<String>,
}

impl RatesBody {
    /// Reads the stated table into the store's shape, refusing the two things
    /// only the wire can get wrong: a day that is not a day, and a row with no
    /// rate on it at all. Everything else — the bounds, the duplicate day, the
    /// ceiling on how many rows there may be — is the store's, so client and
    /// store never disagree about which tables are legal.
    ///
    /// The row is named **1-based as the screen shows it**, matching the store's
    /// own wording for the rules it owns.
    fn read(self) -> Result<Vec<NewMileageRate>, Problem> {
        self.rates
            .into_iter()
            .enumerate()
            .map(|(index, rate)| {
                let at = index + 1;
                let stated = rate
                    .effective_from
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        Problem::with(
                            StatusCode::UNPROCESSABLE_ENTITY,
                            format!("rate {at}: effectiveFrom is required"),
                        )
                    })?;
                Ok(NewMileageRate {
                    effective_from: stated_day(&format!("rate {at}: effectiveFrom"), stated)?,
                    cents_per_km: rate.cents_per_km.ok_or_else(|| {
                        Problem::with(
                            StatusCode::UNPROCESSABLE_ENTITY,
                            format!("rate {at}: centsPerKm is required"),
                        )
                    })?,
                    note: rate.note.unwrap_or_default(),
                })
            })
            .collect()
    }
}

/// `GET /finance/mileage/rates` → `{"rates": [ … ]}` — the tenant's per-km rate
/// table, newest period first.
///
/// Readable by everybody: a traveller has to know what a kilometre is worth. An
/// **empty list is the ordinary answer** for a tenant who has not published a
/// rate — the table ships empty on purpose, because whether a given rate is
/// tax-free in a given member state is that tenant's accountant's statement, not
/// ours.
///
/// # Errors
/// `401` without a valid bearer token.
pub async fn list_mileage_rates(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let rates = account
        .acc
        .fin_mileage_rates()
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "rates": rates.iter().map(rate_json).collect::<Vec<_>>(),
    })))
}

/// `PUT /finance/mileage/rates` `{"rates":[{effectiveFrom, centsPerKm, note?}]}`
/// → `{"rates": [ … ]}` — **admin only**: replace the whole table.
///
/// A replace rather than per-row CRUD because the table is read as one document
/// — "what has this company paid per kilometre, and since when" — and editing it
/// a row at a time makes an intermediate state in which a period is missing and
/// a journey in it is refused. Replacing is safe precisely because every journey
/// snapshots the rate it was claimed at: **nothing already claimed changes when
/// the table does.**
///
/// An empty list is legal and means "we do not pay mileage".
///
/// # Errors
/// `401` without a valid bearer token; `403` when the caller is not a tenant
/// admin; `400` when the body is not JSON; `422` when a row states no day or no
/// rate, a rate is out of range, two rows start on the same day, a note is too
/// long, or there are more rows than the table may hold.
pub async fn replace_mileage_rates(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_admin()?;
    let req: RatesBody = parse_body(&body)?;
    let rates = account
        .acc
        .replace_fin_mileage_rates(&req.read()?)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "rates": rates.iter().map(rate_json).collect::<Vec<_>>(),
    })))
}

/// `GET /finance/mileage?from&to` → `{"mileage": [ … ]}` — the **caller's own**
/// journeys in a period, newest first, each with the claim it became.
///
/// # Errors
/// `401` without a valid bearer token; `422` when an end of the period is
/// missing or malformed, the period ends before it starts, or it spans more than
/// a year.
pub async fn list_mileage(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<MileageQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let (from, to) = period_bounds(query.from.as_deref(), query.to.as_deref())?;
    let journeys = account
        .acc
        .fin_mileages(from, to)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "mileage": journeys.iter().map(mileage_json).collect::<Vec<_>>(),
    })))
}

/// `POST /finance/mileage` `{travelledOn, kmMilli, fromPlace?, toPlace?, reason?,
/// categoryId?, projectId?}` → `{"mileage": {…}}` — claim a journey.
///
/// Two facts are required because a journey without them is not one: the day it
/// was driven and how far. **The amount is not one of them** — it is
/// `kmMilli × the rate in force on that day ÷ 1000`, rounded half-up, and it
/// comes back on the nested claim. A day with no published rate is a `422`
/// naming the day, never an allowance at a rate nobody published.
///
/// The claim it creates is a draft: nothing is in anybody's queue until the
/// claimant submits it with the ordinary `POST /finance/expenses/{id}/submit`.
///
/// # Errors
/// `401` without a valid bearer token; `400` when the body is not JSON; `422`
/// when a required field is missing or malformed, the distance is out of range,
/// no rate covers the travel day, or the allowance rounds to less than a cent;
/// `404` when the category or the project is not one the caller can reach —
/// existence is never disclosed.
pub async fn create_mileage(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: MileageBody = parse_body(&body)?;
    let stated = req
        .travelled_on
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                "travelledOn is required: the day of the journey is the day that picks the rate",
            )
        })?;
    let km_milli = req.km_milli.ok_or_else(|| {
        Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "kmMilli is required: a journey is a distance, in thousandths of a kilometre",
        )
    })?;
    let input = NewMileage {
        from_place: req.from_place.unwrap_or_default(),
        to_place: req.to_place.unwrap_or_default(),
        reason: req.reason.unwrap_or_default(),
        category_id: blank_to_none(req.category_id).map(FinCategoryId::new),
        project_id: blank_to_none(req.project_id).map(ProjectId::new),
        ..NewMileage::driven(stated_day("travelledOn", stated)?, km_milli)
    };
    let claim = account
        .acc
        .log_mileage(&input)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "mileage": mileage_json(&claim) })))
}

/// `DELETE /finance/mileage/{id}` → `204` — withdraw a journey, taking its claim
/// with it.
///
/// Only while the claim is still the claimant's own (draft or rejected); once it
/// has been handed in an approver is looking at it, and the refusal says to
/// withdraw the claim first. The claim is deleted and the journey goes with it by
/// the table's own cascade, so nothing is left explaining an amount that is no
/// longer there.
///
/// # Errors
/// `401` without a valid bearer token; `404` when the journey is not the
/// caller's own; `409` when its claim has been handed in.
pub async fn delete_mileage(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .delete_fin_mileage(&FinMileageId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use alo_store::{Expense, ExpenseMethod, ExpenseStatus, FinExpenseId, Mileage, UserId};
    use time::{Date, Month, OffsetDateTime};

    fn detail(problem: &Problem) -> String {
        problem.detail.clone().unwrap_or_default()
    }

    fn rates(json: &str) -> RatesBody {
        serde_json::from_str(json).expect("a body this test wrote")
    }

    #[test]
    fn a_stated_rate_table_is_read_or_refused_by_row() {
        let read = rates(
            r#"{"rates":[{"effectiveFrom":"2025-01-01","centsPerKm":30},
                         {"effectiveFrom":"2026-01-01","centsPerKm":38,"note":"BMF 2026"}]}"#,
        )
        .read()
        .expect("a table this test wrote");
        assert_eq!(read.len(), 2);
        assert_eq!(
            read[0].effective_from,
            Date::from_calendar_date(2025, Month::January, 1).unwrap()
        );
        assert_eq!(read[1].cents_per_km, 38);
        assert_eq!(read[1].note, "BMF 2026");
        assert_eq!(read[0].note, "", "an absent note is an empty one");

        // An empty table is legal: it is what "we do not pay mileage" looks
        // like, and it is how a tenant clears the table.
        assert!(rates(r#"{"rates":[]}"#).read().expect("legal").is_empty());
        assert!(rates("{}").read().expect("legal").is_empty());

        // The two things only the wire can get wrong, each naming its row.
        for (bad, expect) in [
            (r#"{"rates":[{"centsPerKm":30}]}"#, "rate 1: effectiveFrom"),
            (
                r#"{"rates":[{"effectiveFrom":"  ","centsPerKm":30}]}"#,
                "rate 1: effectiveFrom",
            ),
            (
                r#"{"rates":[{"effectiveFrom":"01/01/2026","centsPerKm":30}]}"#,
                "rate 1: effectiveFrom",
            ),
            (
                r#"{"rates":[{"effectiveFrom":"2025-01-01","centsPerKm":30},
                             {"effectiveFrom":"2026-01-01"}]}"#,
                "rate 2: centsPerKm",
            ),
        ] {
            let problem = rates(bad).read().expect_err("refused");
            assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}");
            assert!(detail(&problem).contains(expect), "{}", detail(&problem));
        }
        // A rate of zero reaches the store, which owns the range rule — the edge
        // does not get a second, quietly different opinion about it.
        assert_eq!(
            rates(r#"{"rates":[{"effectiveFrom":"2026-01-01","centsPerKm":0}]}"#)
                .read()
                .expect("the edge passes it on")[0]
                .cents_per_km,
            0
        );
    }

    #[test]
    fn a_journey_answers_with_the_rate_it_was_paid_at_and_the_claim_itself() {
        let now = OffsetDateTime::UNIX_EPOCH;
        let day = Date::from_calendar_date(2026, Month::March, 14).unwrap();
        let claim = MileageClaim {
            journey: Mileage {
                id: FinMileageId::new("mil-1".to_owned()),
                user_id: UserId::new("usr-1".to_owned()),
                travelled_on: day,
                km_milli: 125_000,
                rate_cents_per_km: 30,
                from_place: "Berlin".to_owned(),
                to_place: "München".to_owned(),
                reason: "Kundentermin".to_owned(),
                expense_id: FinExpenseId::new("exp-1".to_owned()),
                created_at: now,
            },
            expense: Expense {
                id: FinExpenseId::new("exp-1".to_owned()),
                user_id: UserId::new("usr-1".to_owned()),
                spent_on: day,
                category_id: None,
                merchant: String::new(),
                description: "Kundentermin".to_owned(),
                gross_cents: 3750,
                vat_cents: 0,
                vat_rate_bp: None,
                currency: "EUR".to_owned(),
                method: ExpenseMethod::Personal,
                project_id: None,
                receipt_node_id: None,
                status: ExpenseStatus::Draft,
                submitted_at: None,
                decided_by: None,
                decided_at: None,
                decision_note: String::new(),
                reimbursed_on: None,
                created_at: now,
                updated_at: now,
            },
        };
        let value = mileage_json(&claim);
        assert_eq!(value["kmMilli"], json!(125_000));
        assert_eq!(
            value["rateCentsPerKm"],
            json!(30),
            "the rate it was paid at, not the one the table holds today"
        );
        assert_eq!(value["travelledOn"], json!("2026-03-14"));
        assert_eq!(value["fromPlace"], json!("Berlin"));
        // The claim is rendered by the expense routes' own function, so the two
        // can never drift apart.
        assert_eq!(value["expense"]["grossCents"], json!(3750));
        assert_eq!(
            value["expense"]["netCents"],
            json!(3750),
            "no VAT on an allowance"
        );
        assert_eq!(value["expense"]["method"], json!("personal"));
        assert_eq!(value["expense"]["status"], json!("draft"));
        assert_eq!(value["expense"]["editable"], json!(true));
        // And there is no userId on the journey, for the module's whole reason.
        assert!(value.get("userId").is_none());
    }
}
