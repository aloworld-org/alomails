//! The billable handoff's HTTP surface (alo Projects, ADR 0035, wave B3.06) —
//! what could be billed to a customer, and the draft invoice a selection of it
//! becomes — over [`alo_store::time_invoice`].
//!
//! Two routes, and every rule about which hours may travel is the store's
//! ([`alo_store::AccountStore::bill_time_entries`]); this file adds only what an
//! edge owns:
//!
//! - **The word `hour`.** The unit label on a line a customer reads is picked
//!   from `?lang=` through [`crate::projects::hour_words_for`], because the store
//!   writes no untranslated words onto a document.
//! - **Nothing here computes money.** `netCents` on a group and the totals
//!   beside it are the store's fold over the same rows the invoice will carry,
//!   so a figure on this screen and a figure on the printed document cannot
//!   disagree. No float appears on this path.
//! - **The answer names the draft and nothing more.** A handoff raises an
//!   ordinary draft invoice; the client reads it back through
//!   `GET /billing/invoices/{id}`, which is the one surface that renders a
//!   document. The audit layer files this act against that invoice's id
//!   (`projects.invoice.create`), so "which hours went onto this document, and
//!   who sent them there" is answerable.
//! - **A refused handoff answers a count.** `422`/`409` bodies say how many of
//!   the selected hours broke the rule, never which person worked them: an
//!   aggregate is what this surface is allowed to disclose.
//!
//! The unbilled view is a **tenant-wide aggregate on the account door** — an
//! invoice carries the team's hours, not the caller's — and it answers with
//! projects, minutes and money and never with who worked when
//! (`docs/design/projects.md` § The hours of a person are personal data).

use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::time_invoice::{UnbilledGroup, UnbilledTotals, unbilled_totals};
use alo_store::{BillingCustomerId, TimeBilling, TimeEntryId};

use crate::billing::{blank_to_none, map_store_err, parse_body, parse_iso_date};
use crate::error::Problem;
use crate::projects::hour_words_for;
use crate::state::{AppState, authenticate};

/// One group of unbilled hours as JSON — what a line of the invoice would say.
///
/// `entryIds` is what the client sends back to raise the document: the selection
/// is made from the very rows the fold was computed over, so a screen cannot
/// select an hour the view never showed it.
fn group_json(group: &UnbilledGroup) -> Value {
    json!({
        "projectId": group.project_id.as_str(),
        "projectName": group.project_name,
        "rateCents": group.rate_cents,
        "currency": group.currency,
        "minutes": group.minutes,
        "netCents": group.net_cents,
        "entryIds": group
            .entry_ids
            .iter()
            .map(TimeEntryId::as_str)
            .collect::<Vec<_>>(),
    })
}

/// The view's totals: minutes, and money per currency — never across one.
fn totals_json(totals: &UnbilledTotals) -> Value {
    json!({
        "minutes": totals.minutes,
        "unratedMinutes": totals.unrated_minutes,
        "byCurrency": totals
            .by_currency
            .iter()
            .map(|row| json!({
                "currency": row.currency,
                "minutes": row.minutes,
                "netCents": row.net_cents,
            }))
            .collect::<Vec<_>>(),
    })
}

/// The query of `GET /projects/unbilled`.
#[derive(Deserialize)]
pub struct UnbilledQuery {
    #[serde(default, rename = "customerId")]
    customer_id: Option<String>,
    /// The last day to include — an invoice is raised for a period. Absent means
    /// everything eligible.
    #[serde(default)]
    to: Option<String>,
}

/// The body of `POST /projects/invoices`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HandoffBody {
    #[serde(default)]
    customer_id: Option<String>,
    /// The VAT rate every line is billed at, in basis points. Required, and
    /// never guessed on the tenant's behalf: picking a rate for somebody is a
    /// compliance statement made by a machine.
    #[serde(default)]
    vat_rate_bp: Option<i32>,
    /// The document's currency, or absent for the customer's own.
    #[serde(default)]
    currency: Option<String>,
    #[serde(default)]
    entry_ids: Option<Vec<String>>,
}

/// The language the words on the document are written in.
#[derive(Deserialize)]
pub struct LangQuery {
    #[serde(default)]
    lang: Option<String>,
}

/// Reads a required id from a body, naming it in the refusal.
fn required_id(name: &str, raw: Option<&str>) -> Result<String, Problem> {
    raw.map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| {
            Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("{name} is required"),
            )
        })
}

/// `GET /projects/unbilled?customerId=[&to=]` →
/// `{"groups": [ … ], "totals": {…}}` — every approved, billable, unbilled hour
/// worked for one customer, grouped exactly the way the invoice would group it.
///
/// An unrated group comes back with `rateCents` and `netCents` null: hours
/// nobody has priced are shown so somebody can price them, never valued at zero
/// and never quietly dropped from a screen whose whole job is "what is owed to
/// us".
///
/// # Errors
/// `401` without a valid bearer token; `404` when the customer is not this
/// tenant's — existence is never disclosed; `422` when `customerId` is missing,
/// `to` is malformed, or more hours match than one document may carry (a period
/// to narrow, never a list to truncate); `500` on a store failure.
pub async fn list_unbilled(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<UnbilledQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let customer = BillingCustomerId::new(required_id("customerId", query.customer_id.as_deref())?);
    let to = match blank_to_none(query.to) {
        None => None,
        Some(stated) => Some(parse_iso_date(&stated).ok_or_else(|| {
            Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                "to must be a date of the form YYYY-MM-DD",
            )
        })?),
    };
    let groups = account
        .acc
        .unbilled_time(&customer, to)
        .await
        .map_err(map_store_err)?;
    let totals = unbilled_totals(&groups);
    Ok(Json(json!({
        "customerId": customer.as_str(),
        "groups": groups.iter().map(group_json).collect::<Vec<_>>(),
        "totals": totals_json(&totals),
    })))
}

/// `POST /projects/invoices[?lang=]`
/// `{customerId, vatRateBp, currency?, entryIds: [ … ]}` →
/// `{"id": …, "entries": n, "lines": n, "minutes": n}`.
///
/// Raises a **draft** invoice carrying the selected hours, one line per
/// (project, rate), and stamps those hours with it. It issues nothing and sends
/// nothing: a human reads the draft and issues it through billing's own route,
/// which is the same rule the won-deal handoff and the agent tools hold.
///
/// The whole call is one transaction in the store, so a partial document — half
/// the hours billed, half still open — cannot exist. Deleting the draft, or
/// voiding it once issued, releases the hours back to the unbilled view.
///
/// # Errors
/// `401` without a valid bearer token; `404` when the customer, or any named
/// hour, is not this tenant's; `422` when a required field is missing, the
/// selection is empty or too long, or hours are proposals, not billable, worked
/// for another customer, unrated, or priced in another currency — each naming
/// how many; `409` when hours are already on a document, are in a week nobody
/// has approved, or changed under the call; `500` on a store failure.
pub async fn create_invoice(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<LangQuery>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: HandoffBody = parse_body(&body)?;
    let vat_rate_bp = req.vat_rate_bp.ok_or_else(|| {
        Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "vatRateBp is required: a line is billed at a rate somebody stated",
        )
    })?;
    let billing = TimeBilling {
        customer_id: BillingCustomerId::new(required_id("customerId", req.customer_id.as_deref())?),
        vat_rate_bp,
        currency: blank_to_none(req.currency),
        unit: hour_words_for(query.lang.as_deref().unwrap_or_default())
            .hour
            .to_owned(),
        entry_ids: req
            .entry_ids
            .unwrap_or_default()
            .into_iter()
            .map(TimeEntryId::new)
            .collect(),
    };
    let draft = account
        .acc
        .bill_time_entries(&billing)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        // `id` and not `invoiceId`, so the audit layer files this act against
        // the document it created (`crate::audit_record`).
        "id": draft.invoice_id.as_str(),
        "entries": draft.entries,
        "lines": draft.lines,
        "minutes": draft.minutes,
    })))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    use alo_store::ProjectId;

    #[test]
    fn a_required_field_names_itself_in_the_refusal() {
        let problem = required_id("customerId", Some("  ")).expect_err("refused");
        assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(
            problem
                .detail
                .as_deref()
                .unwrap_or_default()
                .contains("customerId")
        );
        assert_eq!(required_id("customerId", Some(" c1 ")).unwrap(), "c1");
    }

    #[test]
    fn an_unrated_group_is_shown_with_no_money_at_all() {
        let group = UnbilledGroup {
            project_id: ProjectId::new("p1".to_owned()),
            project_name: "Discovery".to_owned(),
            rate_cents: None,
            currency: None,
            minutes: 45,
            entry_ids: vec![TimeEntryId::new("e1".to_owned())],
            net_cents: None,
        };
        let value = group_json(&group);
        assert_eq!(value["minutes"], json!(45));
        assert!(value["rateCents"].is_null());
        assert!(value["netCents"].is_null(), "never priced at zero");
        assert_eq!(value["entryIds"], json!(["e1"]));
    }

    #[test]
    fn the_totals_carry_one_row_per_currency() {
        let group = |currency: &str, minutes: i64, net: i64| UnbilledGroup {
            project_id: ProjectId::new("p1".to_owned()),
            project_name: "Portal".to_owned(),
            rate_cents: Some(9_500),
            currency: Some(currency.to_owned()),
            minutes,
            entry_ids: Vec::new(),
            net_cents: Some(net),
        };
        let totals = unbilled_totals(&[group("EUR", 60, 9_500), group("USD", 60, 10_000)]);
        let value = totals_json(&totals);
        assert_eq!(value["minutes"], json!(120));
        assert_eq!(value["byCurrency"].as_array().unwrap().len(), 2);
        assert_eq!(value["byCurrency"][0]["currency"], json!("EUR"));
        assert_eq!(value["byCurrency"][0]["netCents"], json!(9_500));
    }

    #[test]
    fn the_unit_label_follows_the_callers_language() {
        assert_eq!(hour_words_for("fr").hour, "heure");
        assert_eq!(hour_words_for("nl-BE").hour, "uur");
        assert_eq!(hour_words_for("").hour, "hour");
    }
}
