//! **Invoicing** a sales order over HTTP (alo Inventory, ADR 0035, wave B5.06b)
//! — the door that bills what has gone out, over
//! [`alo_store::inv_so_invoice`], and the list of what has been billed already.
//!
//! `POST /inventory/sales-orders/{id}/invoice` raises a **draft** invoice in alo
//! Billing for what has been **delivered and not yet invoiced**
//! (`docs/design/inventory.md` § The invoice). Three things this layer decides,
//! and nothing else:
//!
//! - **The body is empty.** There is nothing for a caller to state: what may be
//!   billed is what the warehouse actually shipped, and letting a client choose
//!   quantities would make the one number a customer's document rests on a
//!   number a client could invent. A tenant who wants different lines edits the
//!   draft in billing, which is what a draft is for.
//! - **It answers with both sides.** The order as it now stands — its lines
//!   carrying their new invoiced figures — and the raising, so a client never
//!   has to re-fetch what it just caused.
//! - **Nothing about the invoice's content.** Which lines it ends up with, what
//!   it is numbered, when it is issued: all billing's, from the moment it
//!   exists. This route hands back its id and what was carried onto it, and
//!   `GET /billing/invoices/{invoiceId}` is where the document is read.
//!
//! Every refusal is the store's, unedited, through
//! [`crate::billing::map_store_err`] — a `409` when the order is still a draft,
//! a `422` naming the order when there is nothing left to bill (the ordinary way
//! a second press ends), a `422` when the customer has since been archived, a
//! `404` for an order that is not this tenant's.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use serde_json::{Value, json};

use alo_store::inv_so_invoice::{SalesOrderInvoice, SalesOrderInvoiceLine};
use alo_store::{AccountStore, InvSalesOrderId};

use crate::billing::{iso, map_store_err};
use crate::billing_document::today;
use crate::error::Problem;
use crate::inventory_so::document_json;
use crate::state::{AppState, authenticate};

/// One line of a raising: which ordered line contributed, and how much of it.
///
/// No price and no total. What the customer is charged is the invoice's own
/// arithmetic, computed by billing from the lines it holds; a second set of
/// money here would be a figure that can disagree with the document.
fn invoice_line_json(l: &SalesOrderInvoiceLine) -> Value {
    json!({
        "lineId": l.so_line_id.as_str(),
        "qtyMilli": l.qty_milli,
    })
}

/// One raising as JSON: the document it created, where that document has got to,
/// and what it carried.
///
/// `invoiceNumber` is `null` while the invoice is still a draft — a draft has
/// consumed nothing from the gapless series — and `invoiceStatus` is how a
/// reader sees that a voided document has released what it carried.
fn invoice_json(i: &SalesOrderInvoice) -> Value {
    json!({
        "id": i.id.as_str(),
        "invoiceId": i.invoice_id.as_str(),
        "invoiceNumber": i.invoice_number,
        "invoiceStatus": i.invoice_status.as_str(),
        "createdBy": i.created_by,
        "createdAt": iso(i.created_at),
        "lines": i.lines.iter().map(invoice_line_json).collect::<Vec<_>>(),
    })
}

/// The `404` an order from another tenant gets.
///
/// Reading what has been invoiced against an order nobody can see must not be
/// the way to learn that it exists, so the list route asks this first rather
/// than answering an empty array for both cases.
async fn seen(acc: &AccountStore, id: &InvSalesOrderId) -> Result<(), Problem> {
    acc.inv_sales_order(id)
        .await
        .map_err(map_store_err)?
        .map(|_| ())
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such sales order"))
}

/// `GET /inventory/sales-orders/{id}/invoices` → `{"invoices":[…]}` — what has
/// been billed against this order, newest first, each with the ordered lines and
/// quantities it carried.
pub async fn list_invoices(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let id = InvSalesOrderId::new(id);
    seen(&account.acc, &id).await?;
    let invoices = account
        .acc
        .inv_sales_order_invoices(&id)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "invoices": invoices.iter().map(invoice_json).collect::<Vec<_>>(),
    })))
}

/// `POST /inventory/sales-orders/{id}/invoice` →
/// `{"salesOrder":{…},"invoice":{…}}` — raise a draft invoice for what has gone
/// out and not yet been billed.
///
/// The document, its lines and the record of what was carried are one
/// transaction, so a caller either gets all of it or none. The invoice is a
/// **draft**: it carries no number, consumes nothing from the gapless series,
/// and is issued — if it ever is — by a human through billing's own route.
pub async fn create_invoice(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let outcome = account
        .acc
        .invoice_inv_sales_order(&InvSalesOrderId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "salesOrder": document_json(&outcome.order, today()),
        "invoice": invoice_json(&outcome.invoice),
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alo_store::billing_invoices::InvoiceStatus;
    use alo_store::inv_so_invoice::SalesOrderInvoice;
    use alo_store::{BillingInvoiceId, BillingLineId, InvSoInvoiceId};
    use time::OffsetDateTime;

    fn raising(number: Option<&str>, status: InvoiceStatus) -> SalesOrderInvoice {
        SalesOrderInvoice {
            id: InvSoInvoiceId::new("soi"),
            invoice_id: BillingInvoiceId::new("inv"),
            invoice_number: number.map(str::to_owned),
            invoice_status: status,
            created_by: "u".to_owned(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            lines: vec![
                SalesOrderInvoiceLine {
                    so_line_id: BillingLineId::new("l1"),
                    qty_milli: 2_500,
                },
                SalesOrderInvoiceLine {
                    so_line_id: BillingLineId::new("l2"),
                    qty_milli: -1_000,
                },
            ],
        }
    }

    #[test]
    fn a_raising_names_the_document_and_what_it_carried() {
        let json = invoice_json(&raising(Some("INV-2026-00007"), InvoiceStatus::Issued));
        assert_eq!(json["invoiceId"], json!("inv"));
        assert_eq!(json["invoiceNumber"], json!("INV-2026-00007"));
        assert_eq!(json["invoiceStatus"], json!("issued"));
        assert_eq!(json["lines"][0]["lineId"], json!("l1"));
        assert_eq!(json["lines"][0]["qtyMilli"], json!(2_500));
        // A discount granted in words travels as the negative quantity it is.
        assert_eq!(json["lines"][1]["qtyMilli"], json!(-1_000));
        assert!(
            json["lines"][0].get("unitPriceCents").is_none(),
            "money is the invoice's, computed once, by billing"
        );
    }

    #[test]
    fn a_draft_invoice_reports_no_number_rather_than_an_invented_one() {
        let json = invoice_json(&raising(None, InvoiceStatus::Draft));
        assert_eq!(json["invoiceNumber"], Value::Null);
        assert_eq!(json["invoiceStatus"], json!("draft"));
    }
}
