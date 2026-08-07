//! Recurring invoices HTTP surface (alo Billing, ADR 0035, wave B2.11) — the
//! standing arrangement that raises the same invoice again every week, month,
//! quarter or year, over [`alo_store::billing_schedules`].
//!
//! It shares the conventions of [`crate::billing_invoices`] — authenticated and
//! tenant-scoped through the account door, no validation duplicated from the
//! store, every write answered with the stored record, `PATCH` as a merge onto
//! it, the header and the lines in one body — and adds two of its own.
//!
//! - **Nothing here issues anything.** `POST /billing/schedules/run` raises
//!   *drafts*, which a colleague then reads and issues by hand. That is the
//!   whole safety property of the feature, and it is why the route answers with
//!   the documents it raised rather than a count: a caller is meant to go and
//!   look at them.
//! - **The date is the server's**, exactly as `overdue` is. A run judged
//!   against a browser's clock would let a wrong date bill next month today.
//!
//! The run route exists beside the background sweep rather than instead of it
//! ([`alo_store::store::Store::sweep_billing_schedules`], started in
//! `main.rs`): the sweep is what makes the feature work while nobody is
//! looking, and this route is what a bookkeeper clicks when they do not want to
//! wait for it. Both go through the same store call, so they cannot disagree,
//! and both are safe to run twice — an occurrence is billed once.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use time::Date;

use alo_store::billing_schedules::{
    NewSchedule, Schedule, ScheduleDocument, ScheduleEdit, ScheduleSummary,
};
use alo_store::{AccountStore, BillingCustomerId, BillingScheduleId, Cadence, NewLine};

use crate::billing::{iso, iso_date, map_store_err, parse_body, parse_iso_date};
use crate::billing_document::{LineBody, today, with_body, with_totals};
use crate::billing_invoices::document_json;
use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// The header of an arrangement as JSON, with the two derived flags a screen
/// needs.
///
/// `ended` and `due` are computed against the server's date on every read, never
/// stored: a stored "due" flag would be wrong every midnight, and a stored
/// "ended" one would have to be moved by whichever run happened to notice.
fn schedule_json(s: &Schedule, today: Date) -> Value {
    json!({
        "id": s.id.as_str(),
        "customerId": s.customer_id.as_str(),
        "name": s.name,
        "cadence": s.cadence.as_str(),
        "anchorDay": s.anchor_day,
        "startDate": iso_date(s.start_date),
        "endDate": s.end_date.map(iso_date),
        "nextRunDate": iso_date(s.next_run_date),
        "lastRunDate": s.last_run_date.map(iso_date),
        "active": s.active,
        "ended": s.is_ended(),
        "due": s.is_due(today),
        "currency": s.currency,
        "paymentTermsDays": s.payment_terms_days,
        "reference": s.reference,
        "note": s.note,
        "createdBy": s.created_by,
        "createdAt": iso(s.created_at),
        "updatedAt": iso(s.updated_at),
    })
}

/// A whole arrangement: header, template lines in print order, and what one
/// occurrence of it is worth.
fn document_body(d: &ScheduleDocument, today: Date) -> Value {
    with_raised(
        with_body(schedule_json(&d.schedule, today), &d.lines, &d.totals),
        d.raised_count,
    )
}

/// A list entry: the header and what one occurrence is worth, without the
/// template lines.
fn summary_json(s: &ScheduleSummary, today: Date) -> Value {
    with_raised(
        with_totals(schedule_json(&s.schedule, today), &s.totals),
        s.raised_count,
    )
}

/// Adds `raisedCount` — how many drafts this arrangement has produced — which
/// is what makes the difference between "set up but never run" and "running"
/// visible without a second read.
fn with_raised(mut value: Value, raised: i64) -> Value {
    if let Some(object) = value.as_object_mut() {
        object.insert("raisedCount".to_owned(), json!(raised));
    }
    value
}

/// The writable parts of an arrangement, every one optional.
///
/// The same body serves `POST` and `PATCH`; on a `PATCH` only `name`,
/// `cadence`, `endDate`, `reference`, `note` and `lines` are read, because the
/// rest is what the arrangement *is* (see
/// [`alo_store::billing_schedules::ScheduleEdit`]). Unknown fields are ignored
/// so the contract can grow additively, and the response carries the stored
/// record, which is where a caller sees that a misspelled field did nothing.
///
/// There is no `nextRunDate` and no `active` here. The first is moved only by a
/// run — a client that could set it could bill a period twice — and the second
/// has its own route, because pausing is an act, not an edit.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScheduleBody {
    #[serde(default)]
    customer_id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    cadence: Option<String>,
    #[serde(default)]
    start_date: Option<String>,
    /// `null` clears the end date ("keep going"), an absent field leaves it as
    /// it was. The two are different instructions and are kept apart, unlike
    /// the string fields where blank already means empty.
    #[serde(default, deserialize_with = "crate::billing::absent_or_null")]
    end_date: Option<Option<String>>,
    #[serde(default)]
    currency: Option<String>,
    #[serde(default)]
    payment_terms_days: Option<i32>,
    #[serde(default)]
    reference: Option<String>,
    #[serde(default)]
    note: Option<String>,
    /// The whole template, in print order. Absent on a `PATCH` leaves the
    /// stored template alone; an empty one is refused by the store, because an
    /// arrangement with nothing to bill is not an arrangement.
    #[serde(default)]
    lines: Option<Vec<LineBody>>,
}

/// Reads a cadence, refusing anything that is not one of the four.
///
/// Strict, like the invoice list's status filter and unlike the forgiving
/// boolean flags: a cadence quietly defaulted to monthly would bill a tenant's
/// customers on a rhythm nobody agreed to.
fn cadence(raw: Option<&str>) -> Result<Cadence, Problem> {
    let raw = raw.unwrap_or_default();
    Cadence::parse(raw).ok_or_else(|| {
        Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "cadence must be one of weekly, monthly, quarterly, yearly",
        )
    })
}

/// Reads a date the client sent, refusing anything that is not `YYYY-MM-DD`.
fn date(raw: &str, field: &str) -> Result<Date, Problem> {
    parse_iso_date(raw).ok_or_else(|| {
        Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("{field} must be a date as YYYY-MM-DD"),
        )
    })
}

impl ScheduleBody {
    /// The lines the body asks for, if it states any. Taken rather than
    /// cloned: a request body is read once, and the rest of it is still needed
    /// after the template has been handed over.
    fn take_lines(&mut self) -> Option<Vec<NewLine>> {
        self.lines.take().map(LineBody::into_lines)
    }

    /// The end date the body asks for: `Some(date)`, `None` to clear it, or the
    /// stored one when the field is absent.
    fn end_date(&self, stored: Option<Date>) -> Result<Option<Date>, Problem> {
        match self.end_date.as_ref() {
            None => Ok(stored),
            Some(None) => Ok(None),
            Some(Some(raw)) => date(raw, "endDate").map(Some),
        }
    }
}

/// Loads one of the tenant's arrangements, or fails with the `404` an id from
/// another tenant gets.
async fn load(acc: &AccountStore, id: &BillingScheduleId) -> Result<ScheduleDocument, Problem> {
    acc.billing_schedule(id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such recurring invoice"))
}

/// `GET /billing/schedules` → `{"schedules":[…]}` — the tenant's recurring
/// arrangements, newest first, each with what one occurrence is worth, how many
/// drafts it has raised, and whether it is due.
pub async fn list_schedules(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let schedules = account
        .acc
        .billing_schedules()
        .await
        .map_err(map_store_err)?;
    let today = today();
    Ok(Json(json!({
        "schedules": schedules.iter().map(|s| summary_json(s, today)).collect::<Vec<_>>(),
    })))
}

/// `POST /billing/schedules` `{customerId, name, cadence, startDate, lines, …}`
/// → `{"schedule":{…}}` — set up an arrangement.
///
/// The template is required and the arrangement is written with it in one
/// transaction, so there is no window in which a schedule exists with nothing
/// to bill. The currency and terms fall back to the customer's own and are then
/// snapshotted, exactly as on an invoice.
///
/// The start date is also the day of the month the arrangement is anchored to:
/// a monthly one started on the 31st bills on the 28th in February and on the
/// 31st again in March.
pub async fn create_schedule(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let mut req: ScheduleBody = parse_body(&body)?;
    // The two checks that live at the edge: which customer and which day an
    // arrangement is for are not field rules the store can own, and letting
    // either fall through would answer a question the request never asked.
    let customer_id = req
        .customer_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| {
            Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                "customerId is required to set up a recurring invoice",
            )
        })?;
    let start_date = date(
        req.start_date.as_deref().ok_or_else(|| {
            Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                "startDate is required: it is the first date this bills on",
            )
        })?,
        "startDate",
    )?;
    let input = NewSchedule {
        customer_id: BillingCustomerId::new(customer_id),
        name: req.name.clone().unwrap_or_default(),
        cadence: cadence(req.cadence.as_deref())?,
        start_date,
        end_date: req.end_date(None)?,
        currency: req.currency.clone(),
        payment_terms_days: req.payment_terms_days,
        reference: req.reference.clone().unwrap_or_default(),
        note: req.note.clone().unwrap_or_default(),
    };
    let id = account
        .acc
        .create_billing_schedule(&input, &req.take_lines().unwrap_or_default())
        .await
        .map_err(map_store_err)?;
    let document = load(&account.acc, &id).await?;
    Ok(Json(
        json!({ "schedule": document_body(&document, today()) }),
    ))
}

/// `GET /billing/schedules/{id}` → `{"schedule":{…},"invoices":[…]}` — the whole
/// arrangement with its template, plus the drafts it has raised, newest
/// occurrence first.
///
/// The two reads are answered together because the question a reader has on this
/// screen is one question: what does this bill, and what has it billed?
pub async fn get_schedule(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let id = BillingScheduleId::new(id);
    let document = load(&account.acc, &id).await?;
    let raised = account
        .acc
        .billing_invoices_from_schedule(&id)
        .await
        .map_err(map_store_err)?;
    let today = today();
    Ok(Json(json!({
        "schedule": document_body(&document, today),
        "invoices": raised
            .iter()
            .map(|s| crate::billing_invoices::summary_json(s, today))
            .collect::<Vec<_>>(),
    })))
}

/// `PATCH /billing/schedules/{id}` `{name?, cadence?, endDate?, lines?, …}` →
/// `{"schedule":{…}}` — edit what stays editable.
///
/// The customer, the currency, the terms and the start date are not among them:
/// an arrangement *is* those, and changing one would leave the drafts it has
/// already raised explained by a schedule that no longer matches them. Changing
/// the cadence does not move the next date — the occurrence already scheduled
/// stands, and the new rhythm applies from the one after it.
pub async fn update_schedule(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let mut req: ScheduleBody = parse_body(&body)?;
    let id = BillingScheduleId::new(id);
    let stored = load(&account.acc, &id).await?;
    let input = ScheduleEdit {
        name: req.name.clone().unwrap_or(stored.schedule.name.clone()),
        cadence: match req.cadence.as_deref() {
            Some(raw) => cadence(Some(raw))?,
            None => stored.schedule.cadence,
        },
        end_date: req.end_date(stored.schedule.end_date)?,
        reference: req
            .reference
            .clone()
            .unwrap_or(stored.schedule.reference.clone()),
        note: req.note.clone().unwrap_or(stored.schedule.note.clone()),
    };
    account
        .acc
        .update_billing_schedule(&id, &input, req.take_lines().as_deref())
        .await
        .map_err(map_store_err)?;
    let document = load(&account.acc, &id).await?;
    Ok(Json(
        json!({ "schedule": document_body(&document, today()) }),
    ))
}

/// `POST /billing/schedules/{id}/pause` and `/resume` → `{"schedule":{…}}`.
///
/// Pausing keeps every date intact; resuming bills the occurrences that came
/// due meanwhile, because they were months the customer was under contract for.
/// Somebody who does not want them deletes the drafts, which costs nothing — a
/// draft carries no number.
pub async fn pause_schedule(
    state: State<AppState>,
    headers: HeaderMap,
    path: Path<String>,
) -> Result<Json<Value>, Problem> {
    set_active(state, headers, path, false).await
}

/// The resume half of [`pause_schedule`].
pub async fn resume_schedule(
    state: State<AppState>,
    headers: HeaderMap,
    path: Path<String>,
) -> Result<Json<Value>, Problem> {
    set_active(state, headers, path, true).await
}

async fn set_active(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    active: bool,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let id = BillingScheduleId::new(id);
    account
        .acc
        .set_billing_schedule_active(&id, active)
        .await
        .map_err(map_store_err)?;
    let document = load(&account.acc, &id).await?;
    Ok(Json(
        json!({ "schedule": document_body(&document, today()) }),
    ))
}

/// `DELETE /billing/schedules/{id}` → `{"status":"ok"}` — remove an arrangement
/// that has **never raised anything**.
///
/// One that has is `409`: its documents point back at it, and deleting it would
/// erase where they came from. It is paused instead, which stops it just as
/// completely and leaves the history readable.
pub async fn delete_schedule(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .delete_billing_schedule(&BillingScheduleId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "status": "ok" })))
}

/// `POST /billing/schedules/run` → `{"invoices":[…]}` — raise the drafts every
/// arrangement of this tenant has come due for, as of the **server's** date.
///
/// Answered with the documents themselves, not a count: they are drafts waiting
/// to be read, and a caller that raised three invoices should be looking at
/// three invoices. An empty list is the ordinary answer — nothing was due — and
/// not a refusal.
///
/// Safe to call repeatedly: an occurrence is billed once, held so by the
/// arrangement's row lock and by a unique index on the document. This is the
/// same call the background sweep makes, so a tenant that clicks it and a
/// tenant that waits get the same drafts.
pub async fn run_schedules(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let today = today();
    let runs = account
        .acc
        .run_due_billing_schedules(today)
        .await
        .map_err(map_store_err)?;

    let mut invoices = Vec::new();
    for run in &runs {
        for id in &run.raised {
            // Read back through the ordinary document door, so a draft a run
            // raised and a draft a colleague typed are the same JSON.
            if let Some(document) = account
                .acc
                .billing_invoice(id)
                .await
                .map_err(map_store_err)?
            {
                invoices.push(document_json(&document, today));
            }
        }
    }
    Ok(Json(json!({ "invoices": invoices })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(json: Value) -> ScheduleBody {
        serde_json::from_value(json).unwrap_or_else(|e| panic!("body rejected: {e}"))
    }

    #[test]
    fn the_cadence_is_read_strictly_and_never_defaulted() {
        for (raw, expected) in [
            ("weekly", Cadence::Weekly),
            ("MONTHLY", Cadence::Monthly),
            (" quarterly ", Cadence::Quarterly),
            ("yearly", Cadence::Yearly),
        ] {
            assert_eq!(cadence(Some(raw)).ok(), Some(expected), "{raw:?}");
        }
        // Absent is not "monthly": billing a customer on a rhythm nobody stated
        // is worse than refusing the request.
        for bad in [None, Some(""), Some("daily"), Some("every month")] {
            let problem = cadence(bad).err().unwrap_or_else(|| panic!("{bad:?}"));
            assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[test]
    fn an_absent_end_date_keeps_the_stored_one_and_null_clears_it() {
        let stored =
            Date::from_calendar_date(2026, time::Month::June, 30).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            body(json!({})).end_date(Some(stored)).ok().flatten(),
            Some(stored),
            "an absent field changes nothing"
        );
        assert_eq!(
            body(json!({ "endDate": null }))
                .end_date(Some(stored))
                .ok()
                .flatten(),
            None,
            "null means: keep going"
        );
        assert_eq!(
            body(json!({ "endDate": "2027-01-31" }))
                .end_date(Some(stored))
                .ok()
                .flatten(),
            Date::from_calendar_date(2027, time::Month::January, 31).ok()
        );
        // A date that is not a date is a 422, never a silently ignored field.
        for bad in ["31/01/2027", "2027-13-01", "tomorrow", ""] {
            let problem = body(json!({ "endDate": bad }))
                .end_date(None)
                .err()
                .unwrap_or_else(|| panic!("{bad:?} should have been refused"));
            assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[test]
    fn the_next_date_and_the_paused_flag_are_not_writable_fields() {
        // They are ignored like any unknown field: a client that could set the
        // next date could bill a period twice, and pausing is its own route.
        let req = body(json!({
            "nextRunDate": "2020-01-01", "active": false, "anchorDay": 1,
            "lastRunDate": "2020-01-01", "raisedCount": 99,
        }));
        assert!(req.name.is_none() && req.cadence.is_none());
        assert!(body(json!({})).take_lines().is_none());
        assert!(req.end_date(None).ok().flatten().is_none());
    }

    #[test]
    fn a_template_reaches_the_store_as_it_was_sent() {
        let lines = body(json!({ "lines": [
            { "description": "Hosting", "unit": "month", "qtyMilli": 1000,
              "unitPriceCents": 9_900, "vatRateBp": 2100 },
            { "description": "Support", "qtyMilli": 2500, "unitPriceCents": 8_000 },
        ] }))
        .take_lines()
        .unwrap_or_else(|| panic!("lines missing"));
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].description, "Hosting");
        assert_eq!(lines[1].qty_milli, 2_500);
        assert!(body(json!({})).take_lines().is_none());
    }

    #[test]
    fn money_with_a_decimal_point_is_refused_never_rounded() {
        for bad in [
            json!({ "lines": [{ "description": "X", "unitPriceCents": 19.99 }] }),
            json!({ "paymentTermsDays": "30" }),
        ] {
            assert!(
                serde_json::from_value::<ScheduleBody>(bad.clone()).is_err(),
                "{bad} should have been refused"
            );
        }
    }
}
