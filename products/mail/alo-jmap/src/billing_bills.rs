//! Bills over HTTP (alo Billing, ADR 0035, wave B1.24) — receiving a
//! supplier's e-invoice, over [`alo_store::billing_bills`].
//!
//! Six routes, one resource:
//!
//! - `POST /billing/bills/import` — the supplier's XML file, uploaded as its
//!   own body. A `POST` of the file itself rather than JSON with the document
//!   inside it: what a user has is a file, and asking them to escape an XML
//!   document into a JSON string first would be a worse surface for no gain.
//! - `GET /billing/bills` — what has arrived, newest document first,
//!   narrowable by status (the approval queue is `?status=received`).
//! - `GET /billing/bills/{id}` — one bill with its lines and both sets of
//!   figures.
//! - `POST …/approve`, `POST …/reject` — the decision. Two routes rather than a
//!   `PATCH` with a status field, for the same reason invoices issue through a
//!   route: what a document becomes is never a field a stale form could send.
//! - `DELETE /billing/bills/{id}` — the undo for an import that should not have
//!   happened, refused once a decision has been made.
//!
//! Conventions shared with the rest of the module ([`crate::billing`]):
//! authenticated and tenant-scoped through the account door, `Problem` errors,
//! **no validation duplicated from the store** — every rule about what an
//! e-invoice must say lives in `alo_store::billing_einvoice_import`, so the
//! agent (B1.25) and the mail path that will book an attachment cannot get a
//! second, weaker definition of a readable invoice.
//!
//! Two things are specific to bills:
//!
//! - **A refusal names the business term or the rule** — `BT-1`, `BR-CO-15`,
//!   `line 3` — because the person holding the file has to be able to tell the
//!   supplier what is wrong with it. Those messages are authored by the store
//!   and never quote the document, so they are safe to return verbatim.
//! - **Both sets of figures are reported.** `totals` is what the supplier says
//!   the document is worth; `computed` is what its lines add up to under our
//!   arithmetic. They agree — the import refuses a document where they do not —
//!   and showing both is what makes that checkable by a person.

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::{Json, body::Bytes};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::billing_bills::{Bill, BillDocument, BillStatus};
use alo_store::billing_einvoice_import::{EInvoiceSyntax, MAX_EINVOICE_BYTES};
use alo_store::{BillingBillId, Line, Totals};

use crate::billing::{iso, iso_date, map_store_err};
use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// One bill's header as JSON — what a list row needs.
fn bill_json(bill: &Bill) -> Value {
    json!({
        "id": bill.id.as_str(),
        "status": bill.status.as_str(),
        "creditNote": bill.credit_note,
        "sourceSyntax": bill.source_syntax.map(EInvoiceSyntax::as_str),
        "sourceSha256": bill.source_sha256,
        "number": bill.number,
        "issueDate": iso_date(bill.issue_date),
        "dueDate": bill.due_date.map(iso_date),
        "currency": bill.currency,
        "buyerReference": bill.buyer_reference,
        "note": bill.note,
        "paymentReference": bill.payment_reference,
        "supplier": {
            "name": bill.supplier.name,
            "vatId": bill.supplier.vat_id,
            "registrationNo": bill.supplier.legal_id,
            "addressLine1": bill.supplier.line1,
            "addressLine2": bill.supplier.line2,
            "postalCode": bill.supplier.postal_code,
            "city": bill.supplier.city,
            "country": bill.supplier.country,
            "email": bill.supplier.email,
            "iban": bill.supplier.iban,
        },
        // Every amount is integer cents, in ledger direction: negative on a
        // credit note, exactly as our own credit notes are held.
        "totals": {
            "lineTotalCents": bill.totals.line_total_cents,
            "allowanceTotalCents": bill.totals.allowance_total_cents,
            "chargeTotalCents": bill.totals.charge_total_cents,
            "taxExclusiveCents": bill.totals.tax_exclusive_cents,
            "taxTotalCents": bill.totals.tax_total_cents,
            "taxInclusiveCents": bill.totals.tax_inclusive_cents,
            "prepaidCents": bill.totals.prepaid_cents,
            "payableCents": bill.totals.payable_cents,
        },
        "importedBy": bill.imported_by,
        "importedAt": iso(bill.imported_at),
        "decidedBy": bill.decided_by,
        "decidedAt": bill.decided_at.map(iso),
    })
}

/// One line of a bill as JSON — the same shape an invoice line is reported in,
/// because it is the same line model.
fn line_json(line: &Line) -> Value {
    json!({
        "id": line.id.as_str(),
        "description": line.description,
        "unit": line.unit,
        "qtyMilli": line.qty_milli,
        "unitPriceCents": line.unit_price_cents,
        "vatRateBp": line.vat_rate_bp,
        "netCents": line.net_cents(),
    })
}

/// What the bill's lines add up to under **our** arithmetic.
fn computed_json(totals: &Totals) -> Value {
    json!({
        "netCents": totals.net_cents,
        "vatCents": totals.vat_cents,
        "grossCents": totals.gross_cents,
        "vatByRate": totals.vat_by_rate.iter().map(|rate| json!({
            "rateBp": rate.rate_bp,
            "netCents": rate.net_cents,
            "vatCents": rate.vat_cents,
        })).collect::<Vec<_>>(),
    })
}

/// A whole bill: the header, its lines, and both sets of figures.
fn document_json(document: &BillDocument) -> Value {
    let mut value = bill_json(&document.bill);
    value["lines"] = document
        .lines
        .iter()
        .map(line_json)
        .collect::<Vec<_>>()
        .into();
    value["computed"] = computed_json(&document.computed);
    value
}

/// The list filter.
#[derive(Deserialize)]
pub struct BillsQuery {
    /// `received`, `approved` or `rejected`; absent means every bill.
    #[serde(default)]
    status: Option<String>,
}

/// `POST /billing/bills/import` (the XML file as the body) → `{"bill":{…}}`.
///
/// The file must be the e-invoice **XML** — a `.xml` as a supplier sends it, or
/// the `factur-x.xml` taken out of a hybrid PDF. A PDF is recognised and
/// answered with a `422` that says exactly that, rather than a generic refusal.
///
/// Every other failure is the store's, in the store's words: a `422` naming the
/// business term or the standard's rule that the document breaks, and a `409`
/// when this supplier's document with this number has already been imported.
pub async fn import_bill(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    // The store bounds this too; refusing here means a very large upload is not
    // copied around before being refused.
    if body.len() > MAX_EINVOICE_BYTES {
        return Err(Problem::with(
            StatusCode::PAYLOAD_TOO_LARGE,
            "the e-invoice file is too large to be one",
        ));
    }
    let id = account
        .acc
        .import_billing_bill(&body)
        .await
        .map_err(map_store_err)?;
    read_bill(&account, &id).await
}

/// `GET /billing/bills?status=` → `{"bills":[…]}` — newest document first.
///
/// An unknown status is a `422` rather than an empty list: silently answering
/// "nothing" to a filter nobody implements is how a screen ends up looking
/// empty for a reason no one can find.
pub async fn list_bills(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<BillsQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let status = match query
        .status
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(raw) => Some(BillStatus::parse(raw).ok_or_else(|| {
            Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                "status must be received, approved or rejected",
            )
        })?),
        None => None,
    };
    let bills = account
        .acc
        .billing_bills(status)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "bills": bills.iter().map(bill_json).collect::<Vec<_>>(),
    })))
}

/// `GET /billing/bills/{id}` → `{"bill":{…}}` — one bill with its lines.
pub async fn get_bill(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    read_bill(&account, &BillingBillId::new(id)).await
}

/// `POST /billing/bills/{id}/approve` → `{"bill":{…}}` — we accept it.
///
/// Irreversible: an approved bill is a liability the accounts carry, and the
/// second call is a `409`. A bill accepted by mistake is corrected the way the
/// paper world corrects one — the supplier issues a credit note, which arrives
/// as a bill of its own.
pub async fn approve_bill(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    decide(state, headers, id, BillStatus::Approved).await
}

/// `POST /billing/bills/{id}/reject` → `{"bill":{…}}` — we do not accept it.
///
/// The document stays: refusing to pay an invoice is a fact worth keeping, and
/// the one a supplier will later dispute.
pub async fn reject_bill(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    decide(state, headers, id, BillStatus::Rejected).await
}

/// `DELETE /billing/bills/{id}` → `204` — the undo for the wrong file.
///
/// Only while nobody has decided: a decided bill is part of the record, and
/// removing it is a `409`.
pub async fn delete_bill(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .delete_billing_bill(&BillingBillId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(StatusCode::NO_CONTENT)
}

/// The shared body of the two decision routes: decide, then answer with the
/// stored bill, so the caller sees the decision as it was recorded rather than
/// as it was asked for.
async fn decide(
    state: AppState,
    headers: HeaderMap,
    id: String,
    decision: BillStatus,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let id = BillingBillId::new(id);
    account
        .acc
        .decide_billing_bill(&id, decision)
        .await
        .map_err(map_store_err)?;
    read_bill(&account, &id).await
}

/// Reads one bill of this account, or the `404` that is the same answer for an
/// id that never existed and for another tenant's.
async fn read_bill(
    account: &crate::state::Account,
    id: &BillingBillId,
) -> Result<Json<Value>, Problem> {
    let document = account
        .acc
        .billing_bill(id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such bill"))?;
    Ok(Json(json!({ "bill": document_json(&document) })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alo_store::billing_bills::{BillTotals, Supplier};
    use alo_store::{BillingLineId, LineFigures, VatSubtotal};
    use time::{Date, Month, OffsetDateTime};

    fn day(year: i32, month: u8, day: u8) -> Date {
        Date::from_calendar_date(year, Month::try_from(month).unwrap_or(Month::January), day)
            .unwrap_or(Date::MIN)
    }

    fn bill() -> Bill {
        Bill {
            id: BillingBillId::new("bill-1".to_owned()),
            source_syntax: Some(EInvoiceSyntax::Cii),
            source_sha256: "a".repeat(64),
            credit_note: false,
            status: BillStatus::Received,
            supplier: Supplier {
                name: "Lieferant GmbH".to_owned(),
                vat_id: "DE811907980".to_owned(),
                country: "DE".to_owned(),
                iban: "DE02120300000000202051".to_owned(),
                ..Supplier::default()
            },
            number: "R-2026-77".to_owned(),
            issue_date: day(2026, 8, 7),
            due_date: Some(day(2026, 9, 6)),
            currency: "EUR".to_owned(),
            buyer_reference: "PO-2026-4".to_owned(),
            note: String::new(),
            payment_reference: "R-2026-77".to_owned(),
            totals: BillTotals {
                line_total_cents: 110_080,
                tax_exclusive_cents: 110_080,
                tax_total_cents: 23_117,
                tax_inclusive_cents: 133_197,
                payable_cents: 133_197,
                ..BillTotals::default()
            },
            imported_by: "u-1".to_owned(),
            imported_at: OffsetDateTime::UNIX_EPOCH,
            decided_by: None,
            decided_at: None,
        }
    }

    #[test]
    fn a_bill_reports_the_suppliers_own_document() {
        let value = bill_json(&bill());
        assert_eq!(value["status"], "received");
        assert_eq!(value["number"], "R-2026-77");
        assert_eq!(value["issueDate"], "2026-08-07");
        assert_eq!(value["dueDate"], "2026-09-06");
        assert_eq!(value["sourceSyntax"], "cii");
        assert_eq!(value["supplier"]["vatId"], "DE811907980");
        assert_eq!(value["totals"]["payableCents"], json!(133_197));
        assert!(
            value["totals"]["payableCents"].is_i64(),
            "money is an integer on the wire, never a float"
        );
        assert_eq!(value["decidedBy"], json!(null));
        assert_eq!(value["decidedAt"], json!(null));
    }

    #[test]
    fn a_credit_note_is_reported_in_ledger_direction() {
        let credit = Bill {
            credit_note: true,
            totals: BillTotals {
                line_total_cents: -25_000,
                tax_exclusive_cents: -25_000,
                tax_total_cents: -5_250,
                tax_inclusive_cents: -30_250,
                payable_cents: -30_250,
                ..BillTotals::default()
            },
            ..bill()
        };
        let value = bill_json(&credit);
        assert_eq!(value["creditNote"], json!(true));
        assert_eq!(value["totals"]["payableCents"], json!(-30_250));
    }

    #[test]
    fn a_document_carries_both_what_the_supplier_says_and_what_we_compute() {
        let lines = vec![Line {
            id: BillingLineId::new("l-1".to_owned()),
            line_order: 0,
            description: "Beratung".to_owned(),
            unit: "hour".to_owned(),
            qty_milli: 8_000,
            unit_price_cents: 12_500,
            vat_rate_bp: 2100,
        }];
        let computed = alo_store::billing_totals::totals(&[LineFigures {
            qty_milli: 8_000,
            unit_price_cents: 12_500,
            vat_rate_bp: 2100,
        }]);
        let value = document_json(&BillDocument {
            bill: bill(),
            lines,
            computed,
        });
        assert_eq!(value["lines"][0]["description"], "Beratung");
        assert_eq!(value["lines"][0]["qtyMilli"], json!(8_000));
        assert_eq!(value["lines"][0]["netCents"], json!(100_000));
        assert_eq!(value["computed"]["netCents"], json!(100_000));
        assert_eq!(value["computed"]["vatCents"], json!(21_000));
        assert_eq!(value["computed"]["vatByRate"][0]["rateBp"], json!(2100));
        // The header is still all there — a document is a bill with more on it.
        assert_eq!(value["number"], "R-2026-77");
    }

    #[test]
    fn a_computed_breakdown_lists_one_row_per_rate() {
        let value = computed_json(&Totals {
            net_cents: 110_080,
            vat_cents: 23_117,
            gross_cents: 133_197,
            vat_by_rate: vec![
                VatSubtotal {
                    rate_bp: 0,
                    net_cents: 1_000,
                    vat_cents: 0,
                },
                VatSubtotal {
                    rate_bp: 2100,
                    net_cents: 109_080,
                    vat_cents: 22_907,
                },
            ],
        });
        assert_eq!(value["grossCents"], json!(133_197));
        assert_eq!(value["vatByRate"].as_array().map(Vec::len), Some(2));
        assert_eq!(value["vatByRate"][1]["vatCents"], json!(22_907));
    }
}
