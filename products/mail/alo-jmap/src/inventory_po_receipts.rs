//! **Receiving** a purchase order over HTTP (alo Inventory, ADR 0035, wave
//! B5.05b) — what has arrived against an order, and the door that books an
//! arrival, over [`alo_store::inv_po_receive`].
//!
//! `POST /inventory/purchase-orders/{id}/receipts` is one act with three
//! consequences (`docs/design/inventory.md` § Receiving): the movements into
//! stock, the order's new state, and the **draft** bill for what arrived. The
//! store writes all three in one transaction, so this layer answers with all
//! three — the order as it now stands, the receipt, and the id of the bill —
//! rather than making a client fetch what it just caused.
//!
//! Three things this layer decides, and nothing else:
//!
//! - **A stated delivery, or none at all.** `lines` absent means "everything
//!   still outstanding arrived", which is what a delivery that matches the order
//!   is and what a warehouse should not have to type out. `lines: []` is not
//!   that — an empty set states that nothing arrived, and the store refuses it
//!   rather than guessing which of the two a client meant.
//! - **The location is required.** Where the goods were put is the one fact a
//!   receipt cannot infer, and a default warehouse would be a guess written into
//!   the ledger.
//! - **Nothing about the bill.** Which supplier it names, what it is numbered
//!   and what it is worth are the store's, and none of them is a request field:
//!   a bill a caller could shape is a liability a caller could invent.
//!
//! Every refusal is the store's, unedited, through
//! [`crate::billing::map_store_err`] — a `409` naming the line and what was
//! ordered when a delivery would over-receive it, a `409` naming the state when
//! the order is not open, a `422` when the destination is not a place anybody
//! can walk into, a `404` for an order or a line that is not this tenant's.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::inv_po_receive::{NewReceipt, NewReceiptLine, Receipt, ReceiptLine};
use alo_store::{AccountStore, BillingLineId, BillingProductId, InvLocationId, InvPurchaseOrderId};

use crate::billing::{iso, iso_date, map_store_err, parse_body};
use crate::billing_document::today;
use crate::error::Problem;
use crate::inventory_po::document_json;
use crate::state::{AppState, authenticate};

/// One line of a receipt: which ordered line arrived, how much, and the
/// movement it wrote — the id that ties this delivery to the stock ledger.
fn receipt_line_json(l: &ReceiptLine) -> Value {
    json!({
        "lineId": l.po_line_id.as_str(),
        "productId": l.product_id.as_ref().map(BillingProductId::as_str),
        "description": l.description,
        "qtyMilli": l.qty_milli,
        "moveId": l.move_id.as_str(),
    })
}

/// One delivery as JSON, with the place it arrived at named rather than only
/// identified — an id is not an explanation, and this list is read by a person.
///
/// `billId` is `null` for a receipt whose drafted bill has since been thrown
/// away, which is a thing a person may do to an undecided bill; what arrived
/// still arrived.
pub(crate) fn receipt_json(r: &Receipt) -> Value {
    json!({
        "id": r.id.as_str(),
        "sequenceNo": r.sequence_no,
        "locationId": r.location_id.as_str(),
        "locationCode": r.location_code,
        "locationName": r.location_name,
        "receivedDate": iso_date(r.received_date),
        "note": r.note,
        "billId": r.bill_id.as_ref().map(alo_store::BillingBillId::as_str),
        "createdBy": r.created_by,
        "createdAt": iso(r.created_at),
        "lines": r.lines.iter().map(receipt_line_json).collect::<Vec<_>>(),
    })
}

/// One line of a delivery as a client states it.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReceiptLineBody {
    /// The order line these goods are against — the `id` of a line as the order
    /// reports it.
    line_id: String,
    /// How much arrived, in milli-units. The store holds it to what is still
    /// outstanding on that line.
    qty_milli: i64,
}

/// The body of a delivery.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReceiptBody {
    /// Where the goods were put. Required: it is the one fact a receipt cannot
    /// infer.
    #[serde(default)]
    location_id: Option<String>,
    /// What arrived. **Absent** means everything still outstanding — the
    /// delivery that matches the order. An empty array is not that, and the
    /// store refuses it.
    #[serde(default)]
    lines: Option<Vec<ReceiptLineBody>>,
    /// What the person unpacking it wrote.
    #[serde(default)]
    note: Option<String>,
}

impl ReceiptBody {
    /// The delivery this body asks for.
    fn into_receipt(self) -> Result<NewReceipt, Problem> {
        let location_id = self
            .location_id
            .map(|id| id.trim().to_owned())
            .filter(|id| !id.is_empty())
            .ok_or_else(|| {
                Problem::with(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "locationId is required: a receipt says where the goods were put",
                )
            })?;
        Ok(NewReceipt {
            location_id: InvLocationId::new(location_id),
            lines: self.lines.map(|lines| {
                lines
                    .into_iter()
                    .map(|line| NewReceiptLine {
                        po_line_id: BillingLineId::new(line.line_id),
                        qty_milli: line.qty_milli,
                    })
                    .collect()
            }),
            note: self.note.unwrap_or_default(),
        })
    }
}

/// Fails with the `404` an order from another tenant gets, unless it is ours.
///
/// Reading what has arrived against an order nobody can see must not be the way
/// to learn that it exists, so the list route asks this first rather than
/// answering an empty array for both cases.
async fn require_order(acc: &AccountStore, id: &InvPurchaseOrderId) -> Result<(), Problem> {
    acc.inv_purchase_order(id)
        .await
        .map_err(map_store_err)?
        .map(|_| ())
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such purchase order"))
}

/// `GET /inventory/purchase-orders/{id}/receipts` → `{"receipts":[…]}` — what
/// has arrived against this order, newest delivery first, each with its lines
/// and the movements they wrote.
pub async fn list_receipts(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let id = InvPurchaseOrderId::new(id);
    require_order(&account.acc, &id).await?;
    let receipts = account
        .acc
        .inv_purchase_order_receipts(&id)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "receipts": receipts.iter().map(receipt_json).collect::<Vec<_>>(),
    })))
}

/// `POST /inventory/purchase-orders/{id}/receipts`
/// `{locationId, lines?, note?}` →
/// `{"purchaseOrder":{…},"receipt":{…},"billId":"…"}` — book an arrival.
///
/// The goods move from the tenant's `supplier` location into the one named, the
/// order becomes `partially_received` or `received`, and a **draft** bill is
/// raised for what arrived — one transaction, so a caller either gets all three
/// or none of them. The bill is `received`, not approved: it enters no payment
/// run until a person decides on it, and the supplier's own invoice arrives
/// later as a bill of its own.
pub async fn create_receipt(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: ReceiptBody = parse_body(if body.is_empty() { b"{}" } else { &body })?;
    let delivery = req.into_receipt()?;
    let outcome = account
        .acc
        .receive_inv_purchase_order(&InvPurchaseOrderId::new(id), &delivery)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "purchaseOrder": document_json(&outcome.order, today()),
        "receipt": receipt_json(&outcome.receipt),
        "billId": outcome.bill_id.as_str(),
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(json: Value) -> ReceiptBody {
        serde_json::from_value(json).unwrap_or_else(|e| panic!("body rejected: {e}"))
    }

    fn receipt(json: Value) -> NewReceipt {
        body(json)
            .into_receipt()
            .unwrap_or_else(|e| panic!("rejected: {e:?}"))
    }

    #[test]
    fn a_delivery_that_matches_the_order_states_no_lines() {
        // Absent is "everything still outstanding"; the store resolves what
        // that means against the order, because only it knows.
        let whole = receipt(json!({ "locationId": "loc-1" }));
        assert_eq!(whole.location_id.as_str(), "loc-1");
        assert!(whole.lines.is_none());
        assert!(whole.note.is_empty());
    }

    #[test]
    fn an_empty_set_is_a_set_and_is_not_the_same_thing() {
        // `[]` states that nothing arrived. It is passed through as the empty
        // set it is, and the store refuses it — never widened into "everything".
        let nothing = receipt(json!({ "locationId": "loc-1", "lines": [] }));
        assert!(nothing.lines.is_some_and(|lines| lines.is_empty()));
    }

    #[test]
    fn a_stated_delivery_carries_a_line_and_a_quantity() {
        let stated = receipt(json!({
            "locationId": " loc-1 ",
            "lines": [{ "lineId": "line-7", "qtyMilli": 2_500 }],
            "note": "one crate damaged",
        }));
        assert_eq!(stated.location_id.as_str(), "loc-1", "trimmed");
        let lines = stated.lines.unwrap_or_else(|| panic!("a stated set"));
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].po_line_id.as_str(), "line-7");
        assert_eq!(lines[0].qty_milli, 2_500);
        assert_eq!(stated.note, "one crate damaged");
    }

    #[test]
    fn a_receipt_must_say_where_the_goods_were_put() {
        for missing in [
            json!({}),
            json!({ "locationId": "" }),
            json!({ "locationId": "  " }),
        ] {
            let refused = body(missing)
                .into_receipt()
                .err()
                .unwrap_or_else(|| panic!("a receipt without a place is not a receipt"));
            assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
            assert!(
                refused
                    .detail
                    .as_deref()
                    .unwrap_or_default()
                    .contains("locationId"),
                "the refusal names the field"
            );
        }
    }
}
