//! Sales orders over HTTP (alo Inventory, ADR 0035, wave B5.06a) — the life of
//! an order a customer placed with us, over [`alo_store::inv_so`]: draft CRUD,
//! the status-filtered list, confirming, and giving up on one.
//!
//! It is deliberately the shape of [`crate::inventory_po`] mirrored, because the
//! two documents are mirror images: authenticated and tenant-scoped through the
//! account door, no validation duplicated from the store, the header and the
//! lines in one body, `PATCH` as a merge onto the stored record, money rendered
//! by the server and never computed by a client, and a strict `?status=` filter
//! (`422` on an unrecognised value).
//!
//! What is its own:
//!
//! - **A line may name a product.** That link is what a delivery turns into a
//!   movement **out** of stock, so it is part of the line rather than a parallel
//!   array, and the store holds every id in it to this tenant's catalog before a
//!   single row is written.
//! - **`late` is computed, never stored** ([`SalesOrder::is_late`]) — like an
//!   invoice's `overdue`, a stored flag would be wrong every midnight. It says
//!   the goods we promised are past the day we promised them, and it is `false`
//!   for an order nobody confirmed and for one already finished with.
//! - **Confirming is a `POST` of its own**, not a field on the `PATCH`. It draws
//!   the number and freezes the document, and that must not happen because an
//!   editor submitted a stale form. It writes no mail: confirming records an
//!   answer we already gave the customer, and sending them the confirmation is
//!   an ordinary letter through the one audited submission path.
//! - **Cancelling a part-delivered order asks for `shortClose`.** Closing the
//!   remainder for good is a decision; the store refuses without it, and this
//!   layer does not decide it on the caller's behalf.
//!
//! Delivering is its own module ([`crate::inventory_so_deliveries`]): it is what
//! moves stock, and it is a document of its own rather than a transition
//! somebody types.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use time::Date;

use alo_store::inv_so::{
    NewSalesOrder, SalesOrder, SalesOrderDocument, SalesOrderSummary, SoStatus,
};
use alo_store::inv_so_invoice::invoiceable;
use alo_store::inv_so_lines::{NewSoLine, SoLine};
use alo_store::{
    AccountStore, BillingCustomerId, BillingProductId, BillingQuoteId, InvSalesOrderId,
};

use crate::billing::{absent_or_null, iso, iso_date, map_store_err, parse_body, parse_iso_date};
use crate::billing_document::{LineBody, today, totals_json};
use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// The header of an order as JSON, with the derived `late` flag.
///
/// `number` and `confirmedDate` are `null` until the order is confirmed — it has
/// not consumed a number — and `closedDate` is `null` until it is delivered or
/// cancelled, which is how a client tells the phases of its life apart without
/// parsing `status`.
fn order_json(o: &SalesOrder, customer_name: &str, today: Date) -> Value {
    json!({
        "id": o.id.as_str(),
        "customerId": o.customer_id.as_str(),
        "customerName": customer_name,
        "status": o.status.as_str(),
        "currency": o.currency,
        "number": o.number,
        "confirmedDate": o.confirmed_date.map(iso_date),
        "expectedDate": o.expected_date.map(iso_date),
        "closedDate": o.closed_date.map(iso_date),
        "late": o.is_late(today),
        "reference": o.reference,
        "note": o.note,
        // The offer this order came from, or null for one taken over a counter
        // or a telephone. Additive: `billing_invoices` has carried the same link
        // for the other branch of an acceptance since B1.12.
        "quoteId": o.quote_id.as_ref().map(BillingQuoteId::as_str),
        "createdBy": o.created_by,
        "createdAt": iso(o.created_at),
        "updatedAt": iso(o.updated_at),
    })
}

/// One ordered line: the shared document line, plus the catalog item a delivery
/// will take out of stock (`null` for a charge in words), plus how much of it
/// already has gone and how much of it is already billed.
///
/// `outstandingQtyMilli` is derived, never stored — what a picking list shows in
/// its "still to go" column — and it is `0` on a charge in words, because
/// assembly does not leave on a pallet and must not hold an order open.
///
/// `invoiceableQtyMilli` is derived too, and by the **store's** own rule
/// ([`alo_store::inv_so_invoice::invoiceable`]) rather than by arithmetic here:
/// what a screen offers to bill and what pressing the button actually bills have
/// to be the same number, computed once. It is `0` on a line with nothing left
/// to bill, which is why it is passed in rather than computed per line — a
/// charge in words waits for the first consignment, and that is a fact about the
/// order, not about the line.
fn line_json(l: &SoLine, invoiceable_qty_milli: i64) -> Value {
    json!({
        "id": l.line.id.as_str(),
        "productId": l.product_id.as_ref().map(BillingProductId::as_str),
        "description": l.line.description,
        "unit": l.line.unit,
        "qtyMilli": l.line.qty_milli,
        "deliveredQtyMilli": l.delivered_qty_milli,
        "outstandingQtyMilli": l.outstanding_qty_milli(),
        "invoicedQtyMilli": l.invoiced_qty_milli,
        "invoiceableQtyMilli": invoiceable_qty_milli,
        "unitPriceCents": l.line.unit_price_cents,
        "vatRateBp": l.line.vat_rate_bp,
        "netCents": l.line.net_cents(),
    })
}

/// The lines of one order as JSON, each carrying what an invoice raised right
/// now would take from it.
fn lines_json(lines: &[SoLine]) -> Vec<Value> {
    let carrying = invoiceable(lines);
    lines
        .iter()
        .map(|line| {
            let qty = carrying
                .iter()
                .find(|c| c.so_line_id.as_str() == line.line.id.as_str())
                .map_or(0, |c| c.line.qty_milli);
            line_json(line, qty)
        })
        .collect()
}

/// A whole order: header, lines in print order, totals.
///
/// Shared with the delivering route ([`crate::inventory_so_deliveries`]), which
/// answers with the order a consignment just advanced: two shapes for one record
/// would be two things for a client to learn.
pub(crate) fn document_json(d: &SalesOrderDocument, today: Date) -> Value {
    let mut header = order_json(&d.order, &d.customer_name, today);
    if let Some(object) = header.as_object_mut() {
        object.insert("lines".to_owned(), Value::Array(lines_json(&d.lines)));
        object.insert("totals".to_owned(), totals_json(&d.totals));
    }
    header
}

/// A list entry: the header and what the order is worth, without the lines.
fn summary_json(s: &SalesOrderSummary, today: Date) -> Value {
    let mut header = order_json(&s.order, &s.customer_name, today);
    if let Some(object) = header.as_object_mut() {
        object.insert("totals".to_owned(), totals_json(&s.totals));
    }
    header
}

/// One ordered line as sent by a client: the shared line body, plus the product
/// it sells.
///
/// Flattened rather than nested so a sales-order line reads on the wire exactly
/// as an invoice line does with one field more — the same names, the same units,
/// the same defaults, and the store's own `422` when a field breaks its rule.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SoLineBody {
    #[serde(default)]
    product_id: Option<String>,
    #[serde(flatten)]
    line: LineBody,
}

impl SoLineBody {
    /// The writable line this body asks for. A blank product id is no product at
    /// all — a cleared picker sends `""` — and the store states that rule.
    fn into_line(self) -> NewSoLine {
        NewSoLine {
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
/// never re-read the customer's current one.
fn editable(o: &SalesOrder) -> NewSalesOrder {
    NewSalesOrder {
        customer_id: o.customer_id.clone(),
        currency: Some(o.currency.clone()),
        expected_date: o.expected_date,
        reference: o.reference.clone(),
        note: o.note.clone(),
        // Carried through, never re-stated. Where an order came from is
        // provenance rather than a field: an edit must not lose it, and no
        // request may rewrite it — see [`OrderBody`].
        quote_id: o.quote_id.clone(),
    }
}

/// The writable parts of an order, every one optional.
///
/// The same body serves `POST` (merged onto a blank header for the named
/// customer) and `PATCH` (merged onto the stored one). Unknown fields are
/// ignored so the contract can grow additively; the response carries the stored
/// document, which is where a caller sees that a misspelled field did nothing.
///
/// There is no `status`, `number`, `confirmedDate` or `closedDate` here. They
/// are not writable from a request at all: they move only through the lifecycle
/// routes.
///
/// **Nor is there a `quoteId`.** Where an order came from is provenance, written
/// once by the acceptance that produced it and never afterwards: a request that
/// could restate it could claim an order came from an offer it did not, which is
/// the one thing the link exists to answer.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OrderBody {
    #[serde(default)]
    customer_id: Option<String>,
    #[serde(default)]
    currency: Option<String>,
    /// `null` clears the promise, an absent field leaves it alone — the
    /// distinction [`absent_or_null`] exists for.
    #[serde(default, deserialize_with = "absent_or_null")]
    expected_date: Option<Option<String>>,
    #[serde(default)]
    reference: Option<String>,
    #[serde(default)]
    note: Option<String>,
    /// The whole line set, in print order. Absent leaves the stored lines alone;
    /// `[]` empties the order, which is a legitimate thing to do to a draft.
    #[serde(default)]
    lines: Option<Vec<SoLineBody>>,
}

impl OrderBody {
    /// Whether the body says anything about the header at all.
    ///
    /// A `PATCH` carrying only `lines` must not touch the header: replaying the
    /// stored header would re-resolve the customer, and a draft whose customer
    /// was archived after it was raised would then refuse to have its lines
    /// edited — a dead end with no way out but deleting the draft.
    fn states_header(&self) -> bool {
        self.customer_id.is_some()
            || self.currency.is_some()
            || self.expected_date.is_some()
            || self.reference.is_some()
            || self.note.is_some()
    }

    /// Merges the stated header fields onto `base`, leaving the rest as they
    /// were. A date that is not a date is the caller's `422` rather than a
    /// silently dropped field.
    fn header(&self, base: NewSalesOrder) -> Result<NewSalesOrder, Problem> {
        let expected_date = match self.expected_date.as_ref() {
            None => base.expected_date,
            Some(None) => None,
            Some(Some(stated)) => Some(expected_date(stated)?),
        };
        Ok(NewSalesOrder {
            customer_id: self
                .customer_id
                .clone()
                .map_or(base.customer_id, BillingCustomerId::new),
            currency: self.currency.clone().or(base.currency),
            expected_date,
            reference: self.reference.clone().unwrap_or(base.reference),
            note: self.note.clone().unwrap_or(base.note),
            quote_id: base.quote_id,
        })
    }

    /// The line set the body asks for, if it states one.
    fn lines(self) -> Option<Vec<NewSoLine>> {
        self.lines
            .map(|lines| lines.into_iter().map(SoLineBody::into_line).collect())
    }
}

/// Reads the day the goods were promised, refusing text that is not one.
///
/// A **day**, not an instant, like every other business date on this service:
/// goods are promised for a date a person writes on a form, and giving it a time
/// and a zone would invite two clients to disagree about which day that was.
/// Blank is not "no date" here — the way to clear a promise is `null`, and a
/// form that sends `""` for "unknown" is corrected once rather than guessed at
/// every time.
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
async fn load(acc: &AccountStore, id: &InvSalesOrderId) -> Result<SalesOrderDocument, Problem> {
    acc.inv_sales_order(id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such sales order"))
}

/// Query string of the list route.
#[derive(Deserialize)]
pub struct ListQuery {
    /// `status=draft|confirmed|partially_delivered|delivered|cancelled`; absent
    /// lists everything.
    #[serde(default)]
    status: Option<String>,
}

/// Reads the status filter, refusing a value that is not one of the five.
///
/// A blank value means "no filter" (a UI whose select is on "all" sends an empty
/// parameter), and the comparison is case-insensitive; anything else is a `422`
/// naming what is accepted. Deliberately strict, like every other list in the
/// business modules: a filter that silently widened to "everything" would show a
/// warehouse cancelled orders among the ones it is picking.
fn status_filter(raw: Option<&str>) -> Result<Option<SoStatus>, Problem> {
    let Some(raw) = raw.map(str::trim).filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    SoStatus::parse(&raw.to_ascii_lowercase())
        .map(Some)
        .ok_or_else(|| {
            Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                "status must be one of draft, confirmed, partially_delivered, delivered, cancelled",
            )
        })
}

/// `GET /inventory/sales-orders[?status=confirmed]` → `{"salesOrders":[…]}` —
/// the tenant's orders, newest first, each with its customer, its computed
/// totals and its `late` flag, but without its lines.
pub async fn list_sales_orders(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ListQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let status = status_filter(q.status.as_deref())?;
    let orders = account
        .acc
        .inv_sales_orders(status)
        .await
        .map_err(map_store_err)?;
    let today = today();
    Ok(Json(json!({
        "salesOrders": orders.iter().map(|s| summary_json(s, today)).collect::<Vec<_>>(),
    })))
}

/// `POST /inventory/sales-orders` `{customerId, lines?, …}` →
/// `{"salesOrder":{…}}` — raise a **draft** order. Only `customerId` is
/// required; the currency falls back to the customer's own and is then
/// snapshotted on the document.
///
/// The lines are written after the header, in their own transaction, and a line
/// that names a product not in this catalog takes the whole set down with it —
/// the draft is then an empty one the caller can correct, never a document with
/// half of what was sent.
pub async fn create_sales_order(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: OrderBody = parse_body(&body)?;
    // The one check that lives at the edge: who an order is for is not a field
    // rule the store can own, and letting an absent id fall through would answer
    // "no such customer" (404) to a request that never named one.
    if req
        .customer_id
        .as_ref()
        .is_none_or(|id| id.trim().is_empty())
    {
        return Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "customerId is required to raise a sales order",
        ));
    }
    let header = req.header(NewSalesOrder::for_customer(BillingCustomerId::new("")))?;
    let lines = req.lines();
    let id = account
        .acc
        .create_inv_sales_order(&header)
        .await
        .map_err(map_store_err)?;
    if let Some(lines) = lines.as_deref() {
        account
            .acc
            .set_inv_sales_order_lines(&id, lines)
            .await
            .map_err(map_store_err)?;
    }
    let document = load(&account.acc, &id).await?;
    Ok(Json(
        json!({ "salesOrder": document_json(&document, today()) }),
    ))
}

/// `GET /inventory/sales-orders/{id}` → `{"salesOrder":{…}}` — the whole order
/// with its lines and totals.
pub async fn get_sales_order(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let document = load(&account.acc, &InvSalesOrderId::new(id)).await?;
    Ok(Json(
        json!({ "salesOrder": document_json(&document, today()) }),
    ))
}

/// `PATCH /inventory/sales-orders/{id}` `{…, lines?}` → `{"salesOrder":{…}}` —
/// edit a **draft**: merge the stated header fields onto the stored ones, and
/// replace the line set if one is sent.
///
/// An order that has been confirmed refuses the whole request with a `409`
/// naming its state; that refusal comes from the store, under the row lock the
/// write itself takes, so an edit that raced a confirmation is refused rather
/// than applied to a document the customer already holds.
pub async fn update_sales_order(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: OrderBody = parse_body(&body)?;
    let id = InvSalesOrderId::new(id);
    let stored = load(&account.acc, &id).await?;
    let header = match req.states_header() {
        true => Some(req.header(editable(&stored.order))?),
        false => None,
    };
    let lines = req.lines();
    if let Some(header) = header {
        account
            .acc
            .update_inv_sales_order(&id, &header)
            .await
            .map_err(map_store_err)?;
    }
    if let Some(lines) = lines.as_deref() {
        account
            .acc
            .set_inv_sales_order_lines(&id, lines)
            .await
            .map_err(map_store_err)?;
    }
    let document = load(&account.acc, &id).await?;
    Ok(Json(
        json!({ "salesOrder": document_json(&document, today()) }),
    ))
}

/// `DELETE /inventory/sales-orders/{id}` → `{"status":"ok"}` — discard a
/// **draft** and its lines.
///
/// The only order that is ever removed: a draft never consumed a number and was
/// never promised to anybody. One that has been confirmed is cancelled (`409`
/// here), which keeps it readable under the number the customer holds.
pub async fn delete_sales_order(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .delete_inv_sales_order(&InvSalesOrderId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "status": "ok" })))
}

/// The body of a confirmation: whether the caller accepts promising goods that
/// are not there.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConfirmBody {
    /// `true` takes the order even though the goods are neither on the shelf nor
    /// on their way — a backorder, which the shortage report then shows. Needed
    /// only when the stock will not cover it: an empty body confirms an order the
    /// warehouse can back, which is the ordinary case.
    #[serde(default)]
    allow_backorder: bool,
}

/// `POST /inventory/sales-orders/{id}/confirm` `{"allowBackorder":false}` →
/// `{"salesOrder":{…}}` — say yes to the customer: draw `SO-YYYY-NNNNN`, stamp
/// today, freeze the document.
///
/// **No stock moves and nothing is reserved.** A sales order is a promise; goods
/// move when they are picked, which is what a delivery is for. Confirming twice
/// is a `409` naming the state, so one document can never carry two numbers, and
/// an order with no lines is a `422` — a confirmation of nothing promises
/// nothing.
///
/// **An order the warehouse cannot back is a `409` naming the product and the
/// shortfall**, so two people cannot each sell the last fan (ADR 0054 §3).
/// `allowBackorder: true` takes it anyway — promising goods you intend to buy is
/// ordinary trade, and the shortage report is where that decision shows up. As
/// with `shortClose` on a cancellation, the store refuses without it rather than
/// deciding on a seller's behalf.
///
/// Nothing is emailed. Confirming records an answer we already gave the
/// customer; sending them the confirmation is a letter the tenant writes,
/// through the one audited submission path.
pub async fn confirm_sales_order(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: ConfirmBody = parse_body(if body.is_empty() { b"{}" } else { &body })?;
    let document = account
        .acc
        .confirm_inv_sales_order(&InvSalesOrderId::new(id), req.allow_backorder)
        .await
        .map_err(map_store_err)?;
    Ok(Json(
        json!({ "salesOrder": document_json(&document, today()) }),
    ))
}

/// The body of a cancellation: whether the caller accepts closing the remainder.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CancelBody {
    /// `true` accepts what has already gone out as the whole of the order.
    /// Required only when some of it has: an empty body cancels an order nothing
    /// has gone out against, which is the ordinary case.
    #[serde(default)]
    short_close: bool,
}

/// `POST /inventory/sales-orders/{id}/cancel` `{"shortClose":false}` →
/// `{"salesOrder":{…}}` — the order will not be fulfilled, or not fulfilled
/// further.
///
/// Terminal, and stamped with the day. An order cancelled while still a draft is
/// kept rather than deleted, because the decision to drop it is worth having on
/// the record; one cancelled after it was confirmed keeps the number the
/// customer holds.
///
/// **A part-delivered order needs `shortClose: true`**: cancelling it closes the
/// remainder for good and leaves the customer to be invoiced for what they
/// received, and the store refuses the request without it (`409`) rather than
/// deciding that on a seller's behalf. **Nothing is un-delivered** — what has
/// gone out has gone out, and goods that come back are a return.
///
/// Nothing is emailed. Telling the customer is a letter the tenant writes.
pub async fn cancel_sales_order(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: CancelBody = parse_body(if body.is_empty() { b"{}" } else { &body })?;
    let document = account
        .acc
        .cancel_inv_sales_order(&InvSalesOrderId::new(id), req.short_close)
        .await
        .map_err(map_store_err)?;
    Ok(Json(
        json!({ "salesOrder": document_json(&document, today()) }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(json: Value) -> OrderBody {
        serde_json::from_value(json).unwrap_or_else(|e| panic!("body rejected: {e}"))
    }

    fn base() -> NewSalesOrder {
        NewSalesOrder {
            customer_id: BillingCustomerId::new("cus-1"),
            currency: Some("CHF".to_owned()),
            expected_date: Date::from_calendar_date(2026, time::Month::September, 1).ok(),
            reference: "Their PO 4711".to_owned(),
            note: "Deliver before noon".to_owned(),
            quote_id: Some(BillingQuoteId::new("quo-1")),
        }
    }

    fn merged(json: Value) -> NewSalesOrder {
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
        assert_eq!(unchanged.customer_id.as_str(), "cus-1");
        assert_eq!(unchanged.currency.as_deref(), Some("CHF"));
        assert_eq!(unchanged.reference, "Their PO 4711");
        assert!(unchanged.expected_date.is_some());
    }

    #[test]
    fn no_request_can_state_or_clear_where_an_order_came_from() {
        // Provenance, not a field. An order that could be told it came from an
        // offer it did not come from would make the link worthless in exactly
        // the case it exists for, so `quoteId` is absent from the body: sending
        // one is an unknown field, ignored like any other.
        let carried = merged(json!({ "quoteId": "quo-somebody-elses" }));
        assert_eq!(
            carried.quote_id.as_ref().map(BillingQuoteId::as_str),
            Some("quo-1"),
            "a stated quote id must be ignored, never adopted"
        );
        // Nor can it be cleared, by null or by editing everything around it.
        let still_there = merged(json!({ "quoteId": null, "reference": "New ref" }));
        assert_eq!(
            still_there.quote_id.as_ref().map(BillingQuoteId::as_str),
            Some("quo-1")
        );
        assert_eq!(still_there.reference, "New ref", "the rest still merges");
        // And a body naming only the quote states no header at all, so a PATCH
        // carrying it does not even re-resolve the customer.
        assert!(!body(json!({ "quoteId": "quo-2" })).states_header());
    }

    #[test]
    fn a_patch_that_only_carries_lines_leaves_the_header_alone() {
        // Replaying the header would re-resolve the customer, and a draft whose
        // customer was archived afterwards could then never be edited again.
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
            "null clears the promise"
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
                    "unitPriceCents": 8_600,
                    "vatRateBp": 1900,
                },
                { "description": "Delivery", "qtyMilli": 1_000, "unitPriceCents": 4_500 },
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
        assert_eq!(lines[0].line.unit_price_cents, 8_600);
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
            status_filter(Some("PARTIALLY_DELIVERED")).unwrap_or(None),
            Some(SoStatus::PartiallyDelivered)
        );
        // A purchase order's vocabulary is not a sales order's: `sent` and
        // `received` name states this document does not have, and widening them
        // to something near enough would show a warehouse the wrong list.
        for bad in [
            "open",
            "partially delivered",
            "sent",
            "received",
            "draft,confirmed",
        ] {
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
    fn cancelling_does_not_close_a_remainder_unless_it_is_asked_for() {
        let default: CancelBody = serde_json::from_slice(b"{}").unwrap_or_else(|e| panic!("{e}"));
        assert!(
            !default.short_close,
            "an empty body cancels; it does not close a part-delivered order's remainder"
        );
        let stated: CancelBody =
            serde_json::from_slice(br#"{"shortClose":true}"#).unwrap_or_else(|e| panic!("{e}"));
        assert!(stated.short_close);
    }
}
