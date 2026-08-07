//! Paying suppliers over HTTP (alo Billing, ADR 0035, wave B2.12) — one route
//! that turns approved bills into a SEPA credit-transfer file the tenant's bank
//! executes.
//!
//! `POST /billing/bills/sepa.xml` → the `pain.001` message as a download.
//!
//! **A `POST` that answers with a file**, which no other billing route does, and
//! for a reason: this is not a rendering of something already stored, it is an
//! *instruction being given*. The bills it covers are stamped with the run in
//! the same call, so the same liability is not handed to the bank twice, and a
//! `GET` that quietly changed what a second `GET` would return would be a lie
//! about the method. The file is the response body because that is what the
//! caller does with it — upload it to their bank — and storing a copy nobody
//! asked for would put a payment instruction in a blob store for no reader.
//!
//! Three steps, in this order, and the order is the safety property:
//!
//! 1. **Plan** ([`alo_store::AccountStore::plan_sepa_payment_file`]) — read the
//!    bills, refuse anything that cannot be paid, and mint the run's identity.
//!    Nothing is written.
//! 2. **Write and check the message** ([`crate::billing_pain001`],
//!    [`crate::billing_pain001_rules`]) — and if our own file breaks the
//!    standard, fail here, with nothing recorded and no bill looking paid.
//! 3. **Record** — under each bill's row lock, re-checking every rule, so two
//!    bookkeepers exporting the same bill at the same moment produce exactly one
//!    instruction.
//!
//! Which `pain.001` version a bank wants is the caller's to state
//! (`"version": "pain.001.001.09"`); the default is the older `.03`, which is
//! what a bank that has said nothing accepts.

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use serde::Deserialize;
use time::OffsetDateTime;

use alo_store::BillingBillId;

use crate::billing::{map_store_err, parse_body, parse_iso_date};
use crate::billing_pain001::{Pain001Version, file_name, render};
use crate::billing_pain001_rules::violations;
use crate::billing_xml::response;
use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// What a payment run is asked for.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaymentRunBody {
    /// The bills to pay, by id. Explicit, never "everything payable": a run
    /// that swept up whatever happened to be approved would be one forgotten
    /// approval away from paying a bill nobody looked at.
    #[serde(default)]
    bill_ids: Vec<String>,
    /// The day the bank should execute, `YYYY-MM-DD`. Absent means today.
    #[serde(default)]
    execution_date: Option<String>,
    /// `pain.001.001.03` (the default) or `pain.001.001.09`.
    #[serde(default)]
    version: Option<String>,
    /// Deliberately export a bill that is already in a file — because that file
    /// was never executed. Absent is `false`, which is what protects a supplier
    /// from being paid twice.
    #[serde(default)]
    repeat: bool,
}

/// `POST /billing/bills/sepa.xml` → the `pain.001` file.
///
/// `422` when a bill cannot be paid by SEPA credit transfer or the tenant's own
/// account is not stated, `409` when a bill is undecided, rejected or already in
/// a file, `404` when an id is not this tenant's — the same answer an id that
/// never existed gets.
pub async fn export_payment_file(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, Problem> {
    let account = authenticate(&state, &headers).await?;
    let request: PaymentRunBody = parse_body(&body)?;

    let version = match request
        .version
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        Some(raw) => Pain001Version::parse(raw).ok_or_else(|| {
            Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                "version must be pain.001.001.03 or pain.001.001.09",
            )
        })?,
        None => Pain001Version::default(),
    };
    let now = OffsetDateTime::now_utc();
    let execution_date = match request
        .execution_date
        .as_deref()
        .map(str::trim)
        .filter(|d| !d.is_empty())
    {
        Some(raw) => parse_iso_date(raw).ok_or_else(|| {
            Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                "executionDate must be a date written YYYY-MM-DD",
            )
        })?,
        None => now.date(),
    };
    let ids: Vec<BillingBillId> = request
        .bill_ids
        .iter()
        .map(|id| BillingBillId::new(id.clone()))
        .collect();

    let file = account
        .acc
        .plan_sepa_payment_file(&ids, execution_date, request.repeat)
        .await
        .map_err(map_store_err)?;

    let xml = render(&file, now, version);
    let broken = violations(&xml, version);
    if !broken.is_empty() {
        // Our own bug, never the caller's: everything the caller controls was
        // refused by the store above. Nothing is recorded, so the run can be
        // asked for again once this is fixed — and the rules that broke are
        // named in the log, which carries no payment data of any kind.
        tracing::error!(
            rules = ?broken.iter().map(|v| v.rule).collect::<Vec<_>>(),
            "the SEPA payment file we produced breaks the standard; nothing was recorded"
        );
        return Err(Problem::server_error());
    }

    account
        .acc
        .record_sepa_payment_file(&file, request.repeat)
        .await
        .map_err(map_store_err)?;

    Ok(response(xml, &file_name(&file)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(json: &str) -> PaymentRunBody {
        parse_body(json.as_bytes()).unwrap_or_else(|_| panic!("the body did not parse"))
    }

    #[test]
    fn a_run_states_its_bills_and_defaults_the_rest() {
        let request = body(r#"{"billIds":["b-1","b-2"]}"#);
        assert_eq!(request.bill_ids.len(), 2);
        assert_eq!(request.execution_date, None);
        assert_eq!(request.version, None);
        assert!(!request.repeat, "repeating is never the default");
    }

    #[test]
    fn every_field_is_read_the_way_the_route_documents_it() {
        let request = body(
            r#"{"billIds":["b-1"],"executionDate":"2026-08-10",
                "version":"pain.001.001.09","repeat":true}"#,
        );
        assert_eq!(request.execution_date.as_deref(), Some("2026-08-10"));
        assert_eq!(
            Pain001Version::parse(request.version.as_deref().unwrap_or_default()),
            Some(Pain001Version::V09)
        );
        assert!(request.repeat);
    }

    #[test]
    fn an_empty_body_is_a_run_with_no_bills_rather_than_a_parse_failure() {
        // The refusal is the store's — "a payment file must pay at least one
        // bill" — so that one rule about what a run is lives in one place.
        let request = body("{}");
        assert!(request.bill_ids.is_empty());
    }
}
