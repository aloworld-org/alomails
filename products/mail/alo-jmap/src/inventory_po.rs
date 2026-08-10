//! Purchase orders over HTTP (alo Inventory, ADR 0035, wave B5.05a) — the life
//! of an order we place with a supplier, over [`alo_store::inv_po`]: draft CRUD,
//! the status-filtered list, and giving up on one.
//!
//! It is deliberately the shape of [`crate::billing_quotes`], because an order
//! is the same kind of object as an offer: authenticated and tenant-scoped
//! through the account door, no validation duplicated from the store, the
//! header and the lines in one body, `PATCH` as a merge onto the stored record,
//! money rendered by the server and never computed by a client, and a strict
//! `?status=` filter (`422` on an unrecognised value).
//!
//! What is its own:
//!
//! - **A line may name a product.** That link is what receiving (B5.05b) turns
//!   into a movement into stock, so it is part of the line rather than a
//!   parallel array, and the store holds every id in it to this tenant's
//!   catalog before a single row is written.
//! - **`late` is computed, never stored** ([`PurchaseOrder::is_late`]) — like an
//!   invoice's `overdue`, a stored flag would be wrong every midnight. It says
//!   the goods we are waiting for are past the day we expected them, and it is
//!   `false` for an order nobody placed and for one already finished with.
//! - **Cancelling a part-delivered order asks for `shortClose`.** Accepting a
//!   short delivery as final is a decision; the store refuses without it, and
//!   this layer does not decide it on the caller's behalf.
//!
//! **Sending is its own module** ([`crate::inventory_po_send`]), and the paper
//! is [`crate::inventory_po_print`]'s. `POST …/{id}/send` moves the order to
//! *sent* **and** writes the covering mail draft with the order attached, in one
//! act — a purchase order's *sent* state means precisely "we have asked them",
//! and a route that moved the state without writing the mail would let a tenant
//! hold an order marked sent that nobody ever sent
//! (`docs/design/inventory.md`).
//!
//! Lifecycle transitions are their own `POST`s, never fields on the `PATCH`: a
//! transition is a decision with a date, and it must not happen because an
//! editor submitted a stale form.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use time::Date;

use alo_store::inv_po::{
    NewPurchaseOrder, PoStatus, PurchaseOrder, PurchaseOrderDocument, PurchaseOrderSummary,
};
use alo_store::inv_po_lines::{NewPoLine, PoLine};
use alo_store::{AccountStore, BillingProductId, InvPurchaseOrderId, InvSupplierId};

use crate::billing::{absent_or_null, iso, iso_date, map_store_err, parse_body, parse_iso_date};
use crate::billing_document::{LineBody, today, totals_json};
use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// The header of an order as JSON, with the derived `late` flag.
///
/// `number` and `orderedDate` are `null` until the order is sent — it has not
/// consumed a number — and `closedDate` is `null` until it is received or
/// cancelled, which is how a client tells the phases of its life apart without
/// parsing `status`.
fn order_json(o: &PurchaseOrder, supplier_name: &str, today: Date) -> Value {
    json!({
        "id": o.id.as_str(),
        "supplierId": o.supplier_id.as_str(),
        "supplierName": supplier_name,
        "status": o.status.as_str(),
        "currency": o.currency,
        "number": o.number,
        "orderedDate": o.ordered_date.map(iso_date),
        "expectedDate": o.expected_date.map(iso_date),
        "closedDate": o.closed_date.map(iso_date),
        "late": o.is_late(today),
        "reference": o.reference,
        "note": o.note,
        "createdBy": o.created_by,
        "createdAt": iso(o.created_at),
        "updatedAt": iso(o.updated_at),
    })
}

/// One ordered line: the shared document line, plus the catalog item it will
/// put into stock when it arrives (`null` for a charge in words), plus how much
/// of it already has.
///
/// `outstandingQtyMilli` is derived, never stored — what a receipt sheet shows
/// in its "still to come" column — and it is `0` on a charge in words, because
/// freight does not arrive on a pallet and must not hold an order open.
fn line_json(l: &PoLine) -> Value {
    json!({
        "id": l.line.id.as_str(),
        "productId": l.product_id.as_ref().map(BillingProductId::as_str),
        "description": l.line.description,
        "unit": l.line.unit,
        "qtyMilli": l.line.qty_milli,
        "receivedQtyMilli": l.received_qty_milli,
        "outstandingQtyMilli": l.outstanding_qty_milli(),
        "unitPriceCents": l.line.unit_price_cents,
        "vatRateBp": l.line.vat_rate_bp,
        "netCents": l.line.net_cents(),
    })
}

/// A whole order: header, lines in print order, totals.
///
/// Shared with the placing route ([`crate::inventory_po_send`]), which answers
/// with the order it just numbered: two shapes for one record would be two
/// things for a client to learn.
pub(crate) fn document_json(d: &PurchaseOrderDocument, today: Date) -> Value {
    let mut header = order_json(&d.order, &d.supplier_name, today);
    if let Some(object) = header.as_object_mut() {
        object.insert(
            "lines".to_owned(),
            Value::Array(d.lines.iter().map(line_json).collect()),
        );
        object.insert("totals".to_owned(), totals_json(&d.totals));
    }
    header
}

/// A list entry: the header and what the order is worth, without the lines.
fn summary_json(s: &PurchaseOrderSummary, today: Date) -> Value {
    let mut header = order_json(&s.order, &s.supplier_name, today);
    if let Some(object) = header.as_object_mut() {
        object.insert("totals".to_owned(), totals_json(&s.totals));
    }
    header
}

/// One ordered line as sent by a client: the shared line body, plus the product
/// it orders.
///
/// Flattened rather than nested so a purchase-order line reads on the wire
/// exactly as an invoice line does with one field more — the same names, the
/// same units, the same defaults, and the store's own `422` when a field breaks
/// its rule.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PoLineBody {
    #[serde(default)]
    product_id: Option<String>,
    #[serde(flatten)]
    line: LineBody,
}

impl PoLineBody {
    /// The writable line this body asks for. A blank product id is no product
    /// at all — a cleared picker sends `""` — and the store states that rule.
    fn into_line(self) -> NewPoLine {
        NewPoLine {
            product_id: self.product_id.map(BillingProductId::new),
            line: self.line.into_line(),
        }
    }
}

/// The stored header as writable input — the base a `PATCH` merges onto.
///
/// The "take the default" currency is handed over as a stated value (`Some`),
/// because on an existing order it was resolved when the order was raised: a
/// `PATCH` that does not mention the currency must keep the document's own,
/// never re-read the supplier's current one.
fn editable(o: &PurchaseOrder) -> NewPurchaseOrder {
    NewPurchaseOrder {
        supplier_id: o.supplier_id.clone(),
        currency: Some(o.currency.clone()),
        expected_date: o.expected_date,
        reference: o.reference.clone(),
        note: o.note.clone(),
    }
}

/// The writable parts of an order, every one optional.
///
/// The same body serves `POST` (merged onto a blank header for the named
/// supplier) and `PATCH` (merged onto the stored one). Unknown fields are
/// ignored so the contract can grow additively; the response carries the stored
/// document, which is where a caller sees that a misspelled field did nothing.
///
/// There is no `status`, `number`, `orderedDate` or `closedDate` here. They are
/// not writable from a request at all: they move only through the lifecycle
/// routes.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OrderBody {
    #[serde(default)]
    supplier_id: Option<String>,
    #[serde(default)]
    currency: Option<String>,
    /// `null` clears the expectation, an absent field leaves it alone — the
    /// distinction [`absent_or_null`] exists for.
    #[serde(default, deserialize_with = "absent_or_null")]
    expected_date: Option<Option<String>>,
    #[serde(default)]
    reference: Option<String>,
    #[serde(default)]
    note: Option<String>,
    /// The whole line set, in print order. Absent leaves the stored lines
    /// alone; `[]` empties the order, which is a legitimate thing to do to a
    /// draft.
    #[serde(default)]
    lines: Option<Vec<PoLineBody>>,
}

impl OrderBody {
    /// Whether the body says anything about the header at all.
    ///
    /// A `PATCH` carrying only `lines` must not touch the header: replaying the
    /// stored header would re-resolve the supplier, and a draft whose supplier
    /// was archived after it was raised would then refuse to have its lines
    /// edited — a dead end with no way out but deleting the draft.
    fn states_header(&self) -> bool {
        self.supplier_id.is_some()
            || self.currency.is_some()
            || self.expected_date.is_some()
            || self.reference.is_some()
            || self.note.is_some()
    }

    /// Merges the stated header fields onto `base`, leaving the rest as they
    /// were. A date that is not a date is the caller's `422` rather than a
    /// silently dropped field.
    fn header(&self, base: NewPurchaseOrder) -> Result<NewPurchaseOrder, Problem> {
        let expected_date = match self.expected_date.as_ref() {
            None => base.expected_date,
            Some(None) => None,
            Some(Some(stated)) => Some(expected_date(stated)?),
        };
        Ok(NewPurchaseOrder {
            supplier_id: self
                .supplier_id
                .clone()
                .map_or(base.supplier_id, InvSupplierId::new),
            currency: self.currency.clone().or(base.currency),
            expected_date,
            reference: self.reference.clone().unwrap_or(base.reference),
            note: self.note.clone().unwrap_or(base.note),
        })
    }

    /// The line set the body asks for, if it states one.
    fn lines(self) -> Option<Vec<NewPoLine>> {
        self.lines
            .map(|lines| lines.into_iter().map(PoLineBody::into_line).collect())
    }
}

/// Reads the day the goods are expected, refusing text that is not one.
///
/// A **day**, not an instant, like every other business date on this service:
/// goods arrive on a date the warehouse writes on a form, and giving it a time
/// and a zone would invite two clients to disagree about which day that was.
/// Blank is not "no date" here — the way to clear an expectation is `null`, and
/// a form that sends `""` for "unknown" is corrected once rather than guessed
/// at every time.
fn expected_date(raw: &str) -> Result<Date, Problem> {
    parse_iso_date(raw.trim()).ok_or_else(|| {
        Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "expectedDate must be a date as YYYY-MM-DD, or null for none",
        )
    })
}

/// Loads one of the tenant's orders, or fails with the `404` an id from another
/// tenant gets.
async fn load(
    acc: &AccountStore,
    id: &InvPurchaseOrderId,
) -> Result<PurchaseOrderDocument, Problem> {
    acc.inv_purchase_order(id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such purchase order"))
}

/// Query string of the list route.
#[derive(Deserialize)]
pub struct ListQuery {
    /// `status=draft|sent|partially_received|received|cancelled`; absent lists
    /// everything.
    #[serde(default)]
    status: Option<String>,
}

/// Reads the status filter, refusing a value that is not one of the five.
///
/// A blank value means "no filter" (a UI whose select is on "all" sends an
/// empty parameter), and the comparison is case-insensitive; anything else is a
/// `422` naming what is accepted. Deliberately strict, like every other list in
/// the business modules: a filter that silently widened to "everything" would
/// show a buyer cancelled orders among the ones they are waiting for.
fn status_filter(raw: Option<&str>) -> Result<Option<PoStatus>, Problem> {
    let Some(raw) = raw.map(str::trim).filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    PoStatus::parse(&raw.to_ascii_lowercase())
        .map(Some)
        .ok_or_else(|| {
            Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                "status must be one of draft, sent, partially_received, received, cancelled",
            )
        })
}

/// `GET /inventory/purchase-orders[?status=sent]` → `{"purchaseOrders":[…]}` —
/// the tenant's orders, newest first, each with its supplier, its computed
/// totals and its `late` flag, but without its lines.
pub async fn list_purchase_orders(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ListQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let status = status_filter(q.status.as_deref())?;
    let orders = account
        .acc
        .inv_purchase_orders(status)
        .await
        .map_err(map_store_err)?;
    let today = today();
    Ok(Json(json!({
        "purchaseOrders": orders.iter().map(|s| summary_json(s, today)).collect::<Vec<_>>(),
    })))
}

/// `POST /inventory/purchase-orders` `{supplierId, lines?, …}` →
/// `{"purchaseOrder":{…}}` — raise a **draft** order. Only `supplierId` is
/// required; the currency falls back to the supplier's own and is then
/// snapshotted on the document.
///
/// The lines are written after the header, in their own transaction, and a line
/// that names a product not in this catalog takes the whole set down with it —
/// the draft is then an empty one the caller can correct, never a document with
/// half of what was sent.
pub async fn create_purchase_order(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: OrderBody = parse_body(&body)?;
    // The one check that lives at the edge: who an order is placed with is not
    // a field rule the store can own, and letting an absent id fall through
    // would answer "no such supplier" (404) to a request that never named one.
    if req
        .supplier_id
        .as_ref()
        .is_none_or(|id| id.trim().is_empty())
    {
        return Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "supplierId is required to raise a purchase order",
        ));
    }
    let header = req.header(NewPurchaseOrder::for_supplier(InvSupplierId::new("")))?;
    let lines = req.lines();
    let id = account
        .acc
        .create_inv_purchase_order(&header)
        .await
        .map_err(map_store_err)?;
    if let Some(lines) = lines.as_deref() {
        account
            .acc
            .set_inv_purchase_order_lines(&id, lines)
            .await
            .map_err(map_store_err)?;
    }
    let document = load(&account.acc, &id).await?;
    Ok(Json(
        json!({ "purchaseOrder": document_json(&document, today()) }),
    ))
}

/// `GET /inventory/purchase-orders/{id}` → `{"purchaseOrder":{…}}` — the whole
/// order with its lines and totals.
pub async fn get_purchase_order(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let document = load(&account.acc, &InvPurchaseOrderId::new(id)).await?;
    Ok(Json(
        json!({ "purchaseOrder": document_json(&document, today()) }),
    ))
}

/// `PATCH /inventory/purchase-orders/{id}` `{…, lines?}` →
/// `{"purchaseOrder":{…}}` — edit a **draft**: merge the stated header fields
/// onto the stored ones, and replace the line set if one is sent.
///
/// An order that has been placed refuses the whole request with a `409` naming
/// its state; that refusal comes from the store, under the row lock the write
/// itself takes, so an edit that raced a send is refused rather than applied to
/// a document the supplier already holds.
pub async fn update_purchase_order(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: OrderBody = parse_body(&body)?;
    let id = InvPurchaseOrderId::new(id);
    let stored = load(&account.acc, &id).await?;
    let header = match req.states_header() {
        true => Some(req.header(editable(&stored.order))?),
        false => None,
    };
    let lines = req.lines();
    if let Some(header) = header {
        account
            .acc
            .update_inv_purchase_order(&id, &header)
            .await
            .map_err(map_store_err)?;
    }
    if let Some(lines) = lines.as_deref() {
        account
            .acc
            .set_inv_purchase_order_lines(&id, lines)
            .await
            .map_err(map_store_err)?;
    }
    let document = load(&account.acc, &id).await?;
    Ok(Json(
        json!({ "purchaseOrder": document_json(&document, today()) }),
    ))
}

/// `DELETE /inventory/purchase-orders/{id}` → `{"status":"ok"}` — discard a
/// **draft** and its lines.
///
/// The only order that is ever removed: a draft never consumed a number and was
/// never placed with anybody. One that has been sent is cancelled (`409` here),
/// which keeps it readable under the number the supplier holds.
pub async fn delete_purchase_order(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .delete_inv_purchase_order(&InvPurchaseOrderId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "status": "ok" })))
}

/// The body of a cancellation: whether the caller accepts a short delivery.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CancelBody {
    /// `true` accepts what has already arrived as the whole of the order.
    /// Required only when some of it has: an empty body cancels an order
    /// nothing has arrived against, which is the ordinary case.
    #[serde(default)]
    short_close: bool,
}

/// `POST /inventory/purchase-orders/{id}/cancel` `{"shortClose":false}` →
/// `{"purchaseOrder":{…}}` — stop expecting the goods.
///
/// Terminal, and stamped with the day. An order cancelled while still a draft
/// is kept rather than deleted, because the decision to drop it is worth
/// having on the record; one cancelled after it was sent keeps the number the
/// supplier holds.
///
/// **A part-delivered order needs `shortClose: true`**: giving up on it accepts
/// the short delivery as final, and the store refuses the request without it
/// (`409`) rather than deciding that on a buyer's behalf.
///
/// Nothing is emailed. Telling the supplier is a letter the tenant writes, and
/// it goes through the one audited submission path like every other message
/// this product composes.
pub async fn cancel_purchase_order(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: CancelBody = parse_body(if body.is_empty() { b"{}" } else { &body })?;
    let document = account
        .acc
        .cancel_inv_purchase_order(&InvPurchaseOrderId::new(id), req.short_close)
        .await
        .map_err(map_store_err)?;
    Ok(Json(
        json!({ "purchaseOrder": document_json(&document, today()) }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(json: Value) -> OrderBody {
        serde_json::from_value(json).unwrap_or_else(|e| panic!("body rejected: {e}"))
    }

    fn base() -> NewPurchaseOrder {
        NewPurchaseOrder {
            supplier_id: InvSupplierId::new("sup-1"),
            currency: Some("CHF".to_owned()),
            expected_date: Date::from_calendar_date(2026, time::Month::September, 1).ok(),
            reference: "Project Falkenstein".to_owned(),
            note: "Rear entrance".to_owned(),
        }
    }

    fn merged(json: Value) -> NewPurchaseOrder {
        body(json)
            .header(base())
            .unwrap_or_else(|e| panic!("rejected: {e:?}"))
    }

    #[test]
    fn an_empty_patch_changes_nothing_and_says_so() {
        let request = body(json!({}));
        assert!(
            !request.states_header(),
            "a body that states no header field must not replay the stored one"
        );
        let unchanged = merged(json!({}));
        assert_eq!(unchanged.supplier_id.as_str(), "sup-1");
        assert_eq!(unchanged.currency.as_deref(), Some("CHF"));
        assert_eq!(unchanged.reference, "Project Falkenstein");
        assert!(unchanged.expected_date.is_some());
    }

    #[test]
    fn a_patch_that_only_carries_lines_leaves_the_header_alone() {
        // Replaying the header would re-resolve the supplier, and a draft whose
        // supplier was archived afterwards could then never be edited again.
        let request = body(json!({ "lines": [] }));
        assert!(!request.states_header());
        assert!(
            request.lines().is_some_and(|lines| lines.is_empty()),
            "an empty set is a set: it empties the order"
        );
    }

    #[test]
    fn an_expected_date_is_stated_cleared_or_refused() {
        let stated = merged(json!({ "expectedDate": "2026-10-15" }));
        assert_eq!(
            stated.expected_date.map(|d| d.to_string()),
            Some("2026-10-15".to_owned())
        );
        assert!(
            merged(json!({ "expectedDate": Value::Null }))
                .expected_date
                .is_none(),
            "null clears the expectation"
        );
        for bad in ["15/10/2026", "2026-10-15T00:00:00Z", "next tuesday", ""] {
            let refused = body(json!({ "expectedDate": bad }))
                .header(base())
                .err()
                .unwrap_or_else(|| panic!("{bad:?} is not a day"));
            assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[test]
    fn a_line_carries_its_product_and_the_shared_line_fields() {
        let lines = body(json!({
            "lines": [
                {
                    "productId": "prod-1",
                    "description": "Blue chair",
                    "unit": "piece",
                    "qtyMilli": 4_000,
                    "unitPriceCents": 4_300,
                    "vatRateBp": 1900,
                },
                { "description": "Freight", "qtyMilli": 1_000, "unitPriceCents": 2_500 },
            ]
        }))
        .lines()
        .unwrap_or_else(|| panic!("a stated set is a set"));
        assert_eq!(lines.len(), 2);
        assert_eq!(
            lines[0].product_id.as_ref().map(BillingProductId::as_str),
            Some("prod-1")
        );
        assert_eq!(lines[0].line.qty_milli, 4_000);
        assert_eq!(lines[0].line.unit_price_cents, 4_300);
        assert!(
            lines[1].product_id.is_none(),
            "a line without a product is a charge in words"
        );
        assert_eq!(
            lines[1].line.vat_rate_bp, 0,
            "unstated is zero, not a guess"
        );
    }

    #[test]
    fn the_status_filter_is_strict_and_case_insensitive() {
        assert_eq!(status_filter(None).unwrap_or(None), None);
        assert_eq!(status_filter(Some("  ")).unwrap_or(None), None);
        assert_eq!(
            status_filter(Some("PARTIALLY_RECEIVED")).unwrap_or(None),
            Some(PoStatus::PartiallyReceived)
        );
        for bad in ["open", "partially received", "sent!", "draft,sent"] {
            let refused = status_filter(Some(bad))
                .err()
                .unwrap_or_else(|| panic!("{bad:?} must be refused, never widened"));
            assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
            assert!(
                refused
                    .detail
                    .as_deref()
                    .unwrap_or_default()
                    .contains("cancelled"),
                "the refusal lists what is accepted"
            );
        }
    }

    #[test]
    fn cancelling_does_not_accept_a_shortfall_unless_it_is_asked_for() {
        let default: CancelBody = serde_json::from_slice(b"{}").unwrap_or_else(|e| panic!("{e}"));
        assert!(
            !default.short_close,
            "an empty body cancels; it does not accept a short delivery"
        );
        let stated: CancelBody =
            serde_json::from_slice(br#"{"shortClose":true}"#).unwrap_or_else(|e| panic!("{e}"));
        assert!(stated.short_close);
    }
}
