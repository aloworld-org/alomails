//! **Delivering** a sales order over HTTP (alo Inventory, ADR 0035, wave
//! B5.06a) — what has gone out against an order, and the door that books a
//! consignment, over [`alo_store::inv_so_deliver`].
//!
//! `POST /inventory/sales-orders/{id}/deliveries` is one act with two
//! consequences (`docs/design/inventory.md` § Delivery): the movements **out**
//! of stock, and the order's new state. The store writes both in one
//! transaction, so this layer answers with both — the order as it now stands and
//! the delivery note — rather than making a client fetch what it just caused.
//!
//! Three things this layer decides, and nothing else:
//!
//! - **A stated consignment, or none at all.** `lines` absent means "everything
//!   still owed went out", which is what a delivery that completes the order is
//!   and what a warehouse should not have to type out. `lines: []` is not that —
//!   an empty set states that nothing went, and the store refuses it rather than
//!   guessing which of the two a client meant.
//! - **The location is required.** Where the goods were picked from is the one
//!   fact a delivery cannot infer, and a default warehouse would be a guess that
//!   takes stock off the wrong shelf.
//! - **No prices.** The delivery note carries quantities only; it travels in the
//!   box, and the person unpacking it is not the person who negotiated it. What
//!   the customer is charged is B5.06b's invoice, raised from the order.
//!
//! Every refusal is the store's, unedited, through
//! [`crate::billing::map_store_err`] — a `409` naming the line and what was
//! ordered when a consignment would over-deliver it, **a `409` naming the
//! product, the place and what is actually there when the shelf has not got the
//! goods**, a `409` naming the state when the order is not open, a `422` when
//! the source is not a place anybody can walk into, a `404` for an order or a
//! line that is not this tenant's.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::inv_so_deliver::{Delivery, DeliveryLine, NewDelivery, NewDeliveryLine};
use alo_store::{AccountStore, BillingLineId, BillingProductId, InvLocationId, InvSalesOrderId};

use crate::billing::{iso, iso_date, map_store_err, parse_body};
use crate::billing_document::today;
use crate::error::Problem;
use crate::inventory_so::document_json;
use crate::state::{AppState, authenticate};

/// One line of a delivery note: which ordered line went, how much, and the
/// movement it wrote — the id that ties this consignment to the stock ledger.
///
/// No price, deliberately: see the module's note.
fn delivery_line_json(l: &DeliveryLine) -> Value {
    json!({
        "lineId": l.so_line_id.as_str(),
        "productId": l.product_id.as_ref().map(BillingProductId::as_str),
        "description": l.description,
        "unit": l.unit,
        "qtyMilli": l.qty_milli,
        "moveId": l.move_id.as_str(),
    })
}

/// One consignment as JSON, with the place it was picked from named rather than
/// only identified — an id is not an explanation, and this list is read by a
/// person.
///
/// `noteNumber` is the delivery note's own number (`SO-2026-00001/D1`), built
/// from the order's number rather than stored beside it. It is `null` for the
/// impossible case of a delivery against an unnumbered order, which the store's
/// own guard prevents.
fn delivery_json(d: &Delivery, order_number: Option<&str>) -> Value {
    json!({
        "id": d.id.as_str(),
        "sequenceNo": d.sequence_no,
        "noteNumber": order_number.map(|number| d.note_number(number)),
        "locationId": d.location_id.as_str(),
        "locationCode": d.location_code,
        "locationName": d.location_name,
        "deliveredDate": iso_date(d.delivered_date),
        "note": d.note,
        "createdBy": d.created_by,
        "createdAt": iso(d.created_at),
        "lines": d.lines.iter().map(delivery_line_json).collect::<Vec<_>>(),
    })
}

/// One line of a consignment as a client states it.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeliveryLineBody {
    /// The order line these goods are against — the `id` of a line as the order
    /// reports it.
    line_id: String,
    /// How much went out, in milli-units. The store holds it to what is still
    /// owed on that line, and to what the shelf actually has.
    qty_milli: i64,
}

/// The body of a consignment.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeliveryBody {
    /// Where the goods were picked from. Required: it is the one fact a delivery
    /// cannot infer.
    #[serde(default)]
    location_id: Option<String>,
    /// What went out. **Absent** means everything still owed — the delivery that
    /// completes the order. An empty array is not that, and the store refuses
    /// it.
    #[serde(default)]
    lines: Option<Vec<DeliveryLineBody>>,
    /// What the person who packed it wrote.
    #[serde(default)]
    note: Option<String>,
}

impl DeliveryBody {
    /// The consignment this body asks for.
    fn into_delivery(self) -> Result<NewDelivery, Problem> {
        let location_id = self
            .location_id
            .map(|id| id.trim().to_owned())
            .filter(|id| !id.is_empty())
            .ok_or_else(|| {
                Problem::with(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "locationId is required: a delivery says where the goods were picked from",
                )
            })?;
        Ok(NewDelivery {
            location_id: InvLocationId::new(location_id),
            lines: self.lines.map(|lines| {
                lines
                    .into_iter()
                    .map(|line| NewDeliveryLine {
                        so_line_id: BillingLineId::new(line.line_id),
                        qty_milli: line.qty_milli,
                    })
                    .collect()
            }),
            note: self.note.unwrap_or_default(),
        })
    }
}

/// The order's number, or the `404` an order from another tenant gets.
///
/// Reading what has gone out against an order nobody can see must not be the way
/// to learn that it exists, so the list route asks this first rather than
/// answering an empty array for both cases. The number it hands back is what the
/// delivery notes are numbered from; it is `None` on a draft, which has no
/// deliveries to number.
async fn order_number(acc: &AccountStore, id: &InvSalesOrderId) -> Result<Option<String>, Problem> {
    acc.inv_sales_order(id)
        .await
        .map_err(map_store_err)?
        .map(|document| document.order.number)
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such sales order"))
}

/// `GET /inventory/sales-orders/{id}/deliveries` → `{"deliveries":[…]}` — what
/// has gone out against this order, newest consignment first, each with its
/// lines and the movements they wrote.
pub async fn list_deliveries(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let id = InvSalesOrderId::new(id);
    let number = order_number(&account.acc, &id).await?;
    let deliveries = account
        .acc
        .inv_sales_order_deliveries(&id)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "deliveries": deliveries
            .iter()
            .map(|d| delivery_json(d, number.as_deref()))
            .collect::<Vec<_>>(),
    })))
}

/// `POST /inventory/sales-orders/{id}/deliveries` `{locationId, lines?, note?}`
/// → `{"salesOrder":{…},"delivery":{…}}` — book a consignment.
///
/// The goods move from the location named into the tenant's `customer` location
/// and the order becomes `partially_delivered` or `delivered` — one transaction,
/// so a caller either gets both or neither. **A shelf that has not got the goods
/// refuses the whole delivery**, which is the point of the moves-only ledger.
///
/// No invoice is raised. Invoicing what has been delivered is B5.06b, and it is
/// deliberately a separate act: it draws a number in the sales ledger.
pub async fn create_delivery(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: DeliveryBody = parse_body(if body.is_empty() { b"{}" } else { &body })?;
    let consignment = req.into_delivery()?;
    let outcome = account
        .acc
        .deliver_inv_sales_order(&InvSalesOrderId::new(id), &consignment)
        .await
        .map_err(map_store_err)?;
    let number = outcome.order.order.number.clone();
    Ok(Json(json!({
        "salesOrder": document_json(&outcome.order, today()),
        "delivery": delivery_json(&outcome.delivery, number.as_deref()),
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(json: Value) -> DeliveryBody {
        serde_json::from_value(json).unwrap_or_else(|e| panic!("body rejected: {e}"))
    }

    fn delivery(json: Value) -> NewDelivery {
        body(json)
            .into_delivery()
            .unwrap_or_else(|e| panic!("rejected: {e:?}"))
    }

    #[test]
    fn a_delivery_that_completes_the_order_states_no_lines() {
        // Absent is "everything still owed"; the store resolves what that means
        // against the order, because only it knows.
        let whole = delivery(json!({ "locationId": "loc-1" }));
        assert_eq!(whole.location_id.as_str(), "loc-1");
        assert!(whole.lines.is_none());
        assert!(whole.note.is_empty());
    }

    #[test]
    fn an_empty_set_is_a_set_and_is_not_the_same_thing() {
        // `[]` states that nothing went out. It is passed through as the empty
        // set it is, and the store refuses it — never widened into "everything".
        let nothing = delivery(json!({ "locationId": "loc-1", "lines": [] }));
        assert!(nothing.lines.is_some_and(|lines| lines.is_empty()));
    }

    #[test]
    fn a_stated_consignment_carries_a_line_and_a_quantity() {
        let stated = delivery(json!({
            "locationId": " loc-1 ",
            "lines": [{ "lineId": "line-7", "qtyMilli": 2_500 }],
            "note": "two boxes, driver Kowalski",
        }));
        assert_eq!(stated.location_id.as_str(), "loc-1", "trimmed");
        let lines = stated.lines.unwrap_or_else(|| panic!("a stated set"));
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].so_line_id.as_str(), "line-7");
        assert_eq!(lines[0].qty_milli, 2_500);
        assert_eq!(stated.note, "two boxes, driver Kowalski");
    }

    #[test]
    fn a_delivery_must_say_where_the_goods_were_picked_from() {
        for missing in [
            json!({}),
            json!({ "locationId": "" }),
            json!({ "locationId": "  " }),
        ] {
            let refused = body(missing)
                .into_delivery()
                .err()
                .unwrap_or_else(|| panic!("a delivery without a place is not a delivery"));
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

    #[test]
    fn the_note_is_numbered_from_the_orders_number_when_it_has_one() {
        let consignment = Delivery {
            id: alo_store::InvSoDeliveryId::new("d"),
            sequence_no: 1,
            location_id: InvLocationId::new("loc"),
            location_code: "MAIN".to_owned(),
            location_name: "Hoofdmagazijn".to_owned(),
            delivered_date: time::Date::from_calendar_date(2026, time::Month::August, 10)
                .unwrap_or_else(|e| panic!("{e}")),
            note: String::new(),
            created_by: "u".to_owned(),
            created_at: time::OffsetDateTime::UNIX_EPOCH,
            lines: Vec::new(),
        };
        let numbered = delivery_json(&consignment, Some("SO-2026-00001"));
        assert_eq!(numbered["noteNumber"], json!("SO-2026-00001/D1"));
        let unnumbered = delivery_json(&consignment, None);
        assert_eq!(
            unnumbered["noteNumber"],
            Value::Null,
            "a number is never invented for a document that has none"
        );
    }
}
