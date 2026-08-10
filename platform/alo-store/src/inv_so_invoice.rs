//! **Invoicing** a sales order (alo Inventory, ADR 0035, wave B5.06b) — the
//! seam between what left the warehouse and what the customer is charged for
//! (`docs/design/inventory.md` § The invoice).
//!
//! One act: [`AccountStore::invoice_inv_sales_order`] raises a **draft** invoice
//! in alo Billing carrying what has been **delivered and not yet invoiced**, one
//! line per ordered line, at the price the order snapshotted — and records
//! against the order which document carried which quantity.
//!
//! # At delivery, not at order
//!
//! The decision is *when*, and it is the design note's: invoicing an order
//! before it ships means invoicing goods that may never leave, which is a VAT
//! event asserted on a hope. Invoicing what has actually gone out is the accrual
//! basis `docs/design/finance.md` already commits to, and it makes partial
//! deliveries invoice correctly with no extra concept: deliver half, invoice
//! half; deliver the rest, invoice the rest.
//!
//! # A charge in words rides on the first invoice
//!
//! A line that names no product — delivery to the third floor, assembly, a
//! discount granted — never leaves on a pallet, so "what was delivered" cannot
//! answer for it. It is billed **once, in full**, on the first invoice raised
//! against the order, and only once goods have actually gone out ([`due`]).
//! Both halves of that matter: charging for delivery before anything is
//! delivered would bill a customer for a van that never came, and prorating the
//! charge across consignments would be arithmetic nobody agreed to. An order
//! that sells no goods at all is the one exception — there is no first
//! consignment to wait for, so its charges are billable as soon as it is
//! confirmed.
//!
//! # One-way, one-shot, and a fold rather than a counter
//!
//! Like [`crate::crm_handoff`] and [`crate::time_invoice`], this seam raises a
//! draft billing owns from that moment: it never issues, never sends, and never
//! touches a document it did not just create. What is new is the idempotency,
//! and it is deliberately **derived**: how much of a line is already billed is a
//! sum over `inv_so_invoice_lines` that counts only documents which still stand
//! ([`crate::inv_so_lines::read`]). Throwing away the draft therefore releases
//! what it carried, because the cascade removes the link rows with it; voiding
//! an issued document releases it too, because the fold skips a voided invoice.
//! A **credit note does not release**: crediting corrects a document, the goods
//! stay billed against the original, and re-billing them would charge a customer
//! twice for one delivery.
//!
//! # What it never invents
//!
//! No text. The lines are the order's own words, prices and rates, and nothing
//! here writes a sentence onto a document a customer reads — the store has no
//! language ([`crate::time_invoice`]'s rule, for the same reason). The
//! customer's own reference travels from the order to the invoice, because their
//! PO number belongs on both; the order's internal note does not.

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::billing_invoices::{InvoiceStatus, NewInvoice};
use crate::billing_line::{INVOICE_LINES, NewLine};
use crate::error::{Result, StoreError};
use crate::id::{
    BillingCustomerId, BillingInvoiceId, BillingLineId, InvSalesOrderId, InvSoInvoiceId,
};
use crate::inv_so::{SalesOrderDocument, SoStatus};
use crate::inv_so_lines::{self, SoLine};

/// The columns every read of a raising selects, in `InvoiceRow` order.
const INVOICE_COLS: &str = "si.id, si.invoice_id, bi.number AS invoice_number, \
     bi.status AS invoice_status, si.created_by, si.created_at";

/// What one ordered line would put on an invoice raised now: the line as the
/// document will carry it, and how much of it that is.
///
/// Built by [`invoiceable`] and nothing else, so the quantity a screen shows in
/// its "to invoice" column and the quantity the document carries are the same
/// number computed by the same code.
#[derive(Debug, Clone)]
pub struct Invoiceable {
    /// 1-based position on the order — what a refusal a person can act on names.
    pub position: usize,
    /// The ordered line this comes from.
    pub so_line_id: BillingLineId,
    /// The line as the invoice will carry it: the order's words, its price and
    /// its rate, with the quantity that is billable now.
    pub line: NewLine,
}

/// One invoice raised from an order, as it reads back: which document, where it
/// has got to, and what it carried.
#[derive(Debug, Clone)]
pub struct SalesOrderInvoice {
    /// Opaque id of the raising itself, unique within the tenant.
    pub id: InvSoInvoiceId,
    /// The billing document it created.
    pub invoice_id: BillingInvoiceId,
    /// That document's number, `None` while it is still a draft.
    pub invoice_number: Option<String>,
    /// Where that document has got to — a voided one has released what it
    /// carried, and this is how a reader sees why.
    pub invoice_status: InvoiceStatus,
    /// Who raised it.
    pub created_by: String,
    /// When it was raised.
    pub created_at: OffsetDateTime,
    /// What it carried, in the order's own line order.
    pub lines: Vec<SalesOrderInvoiceLine>,
}

/// One line of a raising: which ordered line contributed, and how much of it.
#[derive(Debug, Clone)]
pub struct SalesOrderInvoiceLine {
    /// The ordered line.
    pub so_line_id: BillingLineId,
    /// How much of it this document carried, in milli-units — negative for a
    /// discount granted in words.
    pub qty_milli: i64,
}

/// Everything raising an invoice changed: the order as it now stands (its lines
/// carrying their new invoiced quantities) and the raising itself.
#[derive(Debug, Clone)]
pub struct SalesOrderInvoiceOutcome {
    /// The order, re-read.
    pub order: SalesOrderDocument,
    /// The document that was raised, and what it carried.
    pub invoice: SalesOrderInvoice,
}

/// Whether the order's charges in words are billable yet: something has gone
/// out, or the order sells no goods at all and there is nothing to wait for.
///
/// Pure, and stated once because it is the whole of the charge rule. The second
/// clause is not a special case for its own sake: an order of nothing but
/// services would otherwise carry lines that could never be billed, which is
/// money silently left on the table.
fn due(lines: &[SoLine]) -> bool {
    lines
        .iter()
        .any(|line| line.is_goods() && line.delivered_qty_milli > 0)
        || !lines.iter().any(SoLine::is_goods)
}

/// What an invoice raised right now would carry, line by line, in the order's
/// own line order — the empty vector when there is nothing left to bill.
///
/// Goods contribute what has been **delivered and not yet invoiced**; a charge
/// in words contributes its whole quantity, once, when [`due`]. A line that
/// contributes nothing is left out entirely rather than carried as a zero: a
/// zero-quantity line on a customer's document is noise that has to be explained
/// to them.
///
/// Pure over lines already read, so the rules are unit-tested without a
/// database.
#[must_use]
pub fn invoiceable(lines: &[SoLine]) -> Vec<Invoiceable> {
    let charges_due = due(lines);
    lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| {
            let qty_milli = billable_qty(line, charges_due);
            (qty_milli != 0).then(|| Invoiceable {
                position: index + 1,
                so_line_id: line.line.id.clone(),
                line: NewLine {
                    description: line.line.description.clone(),
                    unit: line.line.unit.clone(),
                    qty_milli,
                    unit_price_cents: line.line.unit_price_cents,
                    vat_rate_bp: line.line.vat_rate_bp,
                },
            })
        })
        .collect()
}

/// How much of one line is billable now.
///
/// Goods: what has gone out, less what is already on a document that stands.
/// Never negative — an over-delivery is refused before it is written and a
/// credit note is billing's own act, so an invoiced figure above the delivered
/// one can only mean a document was corrected downward, and the answer to that
/// is nothing further to bill rather than a negative line nobody asked for.
///
/// A charge in words: its whole quantity, less what has been billed, and only
/// when charges are due. It may be negative, because a discount granted in
/// words is a negative quantity and is granted exactly once.
fn billable_qty(line: &SoLine, charges_due: bool) -> i64 {
    if line.is_goods() {
        return (line.delivered_qty_milli - line.invoiced_qty_milli).max(0);
    }
    if !charges_due {
        return 0;
    }
    line.line.qty_milli - line.invoiced_qty_milli
}

impl AccountStore {
    /// Raises a **draft invoice** for what has been delivered against one of
    /// this tenant's orders and not yet invoiced, and records it against the
    /// order.
    ///
    /// All of it is one transaction under the order's row lock: either the
    /// document exists with its lines and the link that says what it carried, or
    /// nothing was written. Two callers pressing the button at the same instant
    /// serialise here, and the second sees an order with nothing left to bill
    /// rather than raising a duplicate document.
    ///
    /// The invoice is a draft like any other — no number, no dates, freely
    /// editable, deletable — and it is issued, if it ever is, through billing's
    /// own route by a human who has read it.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the order is not this tenant's;
    /// [`StoreError::Conflict`] when it is still a draft
    /// ([`SoStatus::ensure_invoiceable`]); [`StoreError::Validation`] when there
    /// is nothing left to bill (the message names the order) or when the
    /// customer has since been archived; [`StoreError::Db`] on failure.
    pub async fn invoice_inv_sales_order(
        &self,
        id: &InvSalesOrderId,
    ) -> Result<SalesOrderInvoiceOutcome> {
        // **Whose order it is, first.** Not the authority — the lock below is —
        // but the order of refusals is itself a tenancy rule: another tenant's
        // id must be a bare `NotFound`, never a complaint about a document the
        // caller is not allowed to know exists.
        self.sales_order_status(id).await?.ensure_invoiceable()?;

        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let locked = lock_status(&mut tx, self.tenant.as_str(), id.as_str()).await?;
        locked.ensure_invoiceable()?;

        // Read under the same lock the write goes through, so a consignment
        // that left while this was being typed either lands first (and is
        // invoiced with it) or waits.
        let header_row = self.locked_order_header(&mut tx, id).await?;
        let lines = inv_so_lines::read(&mut *tx, self.tenant.as_str(), id.as_str()).await?;
        let carrying = invoiceable(&lines);
        if carrying.is_empty() {
            return Err(StoreError::Validation(nothing_to_bill(
                header_row.number.as_deref(),
                &lines,
            )));
        }

        // The customer is resolved on this transaction's connection, so an
        // archived one refuses here rather than after a document exists. The
        // currency is the order's snapshot: a customer re-denominated since the
        // order was taken must not restate what they were quoted.
        let invoice_header = self
            .normalize_invoice_in(
                &mut tx,
                &NewInvoice {
                    currency: Some(header_row.currency.clone()),
                    // Their own reference travels; our internal note does not.
                    reference: header_row.reference.clone(),
                    ..NewInvoice::for_customer(BillingCustomerId::new(
                        header_row.customer_id.clone(),
                    ))
                },
            )
            .await?;
        let invoice_id = self.insert_draft_invoice(&mut tx, &invoice_header).await?;
        let document_lines: Vec<NewLine> = carrying.iter().map(|c| c.line.clone()).collect();
        INVOICE_LINES
            .replace(
                &mut tx,
                self.tenant.as_str(),
                invoice_id.as_str(),
                &document_lines,
            )
            .await?;

        let raising = InvSoInvoiceId::generate();
        sqlx::query(
            "INSERT INTO inv_so_invoices (tenant_id, id, so_id, invoice_id, created_by) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(self.tenant.as_str())
        .bind(raising.as_str())
        .bind(id.as_str())
        .bind(invoice_id.as_str())
        .bind(self.user.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        for carried in &carrying {
            sqlx::query(
                "INSERT INTO inv_so_invoice_lines (tenant_id, id, so_invoice_id, so_line_id, \
                     qty_milli) VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(self.tenant.as_str())
            .bind(BillingLineId::generate().as_str())
            .bind(raising.as_str())
            .bind(carried.so_line_id.as_str())
            .bind(carried.line.qty_milli)
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        }
        // The order's own timestamp moves: what it still owes the sales ledger
        // has changed, even though no field of the header did.
        sqlx::query(
            "UPDATE inv_sales_orders SET updated_at = now() WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;

        let order = self
            .inv_sales_order(id)
            .await?
            .ok_or(StoreError::NotFound)?;
        let invoice = self
            .inv_sales_order_invoice(&raising)
            .await?
            .ok_or(StoreError::NotFound)?;
        Ok(SalesOrderInvoiceOutcome { order, invoice })
    }

    /// The few header facts a raising needs, read inside the transaction that
    /// already holds the order's lock.
    async fn locked_order_header(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        id: &InvSalesOrderId,
    ) -> Result<OrderHeaderRow> {
        sqlx::query_as::<_, OrderHeaderRow>(
            "SELECT customer_id, currency, number, reference \
             FROM inv_sales_orders WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&mut **tx)
        .await
        .map_err(StoreError::Db)?
        .ok_or(StoreError::NotFound)
    }

    /// What has been invoiced against one of this tenant's orders, newest
    /// first, each with the lines it carried.
    ///
    /// An id that is not this tenant's — or not an order at all — is an empty
    /// list, never a refusal that would confirm it exists.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn inv_sales_order_invoices(
        &self,
        so_id: &InvSalesOrderId,
    ) -> Result<Vec<SalesOrderInvoice>> {
        let rows = sqlx::query_as::<_, InvoiceRow>(&format!(
            "SELECT {INVOICE_COLS} FROM inv_so_invoices si \
             JOIN billing_invoices bi ON bi.tenant_id = si.tenant_id AND bi.id = si.invoice_id \
             WHERE si.tenant_id = $1 AND si.so_id = $2 \
             ORDER BY si.created_at DESC, si.id DESC"
        ))
        .bind(self.tenant.as_str())
        .bind(so_id.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        self.with_invoice_lines(rows).await
    }

    /// One raising of this tenant, or `None` — including when the id belongs to
    /// another tenant (indistinguishable by design).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn inv_sales_order_invoice(
        &self,
        id: &InvSoInvoiceId,
    ) -> Result<Option<SalesOrderInvoice>> {
        let rows = sqlx::query_as::<_, InvoiceRow>(&format!(
            "SELECT {INVOICE_COLS} FROM inv_so_invoices si \
             JOIN billing_invoices bi ON bi.tenant_id = si.tenant_id AND bi.id = si.invoice_id \
             WHERE si.tenant_id = $1 AND si.id = $2"
        ))
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(self.with_invoice_lines(rows).await?.into_iter().next())
    }

    /// Fills in the lines of a set of raisings in one further statement, not one
    /// per raising.
    async fn with_invoice_lines(&self, rows: Vec<InvoiceRow>) -> Result<Vec<SalesOrderInvoice>> {
        if rows.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<String> = rows.iter().map(|row| row.id.clone()).collect();
        let lines = sqlx::query_as::<_, InvoiceLineRow>(
            "SELECT il.so_invoice_id, il.so_line_id, il.qty_milli \
             FROM inv_so_invoice_lines il \
             JOIN inv_sales_order_lines sl \
               ON sl.tenant_id = il.tenant_id AND sl.id = il.so_line_id \
             WHERE il.tenant_id = $1 AND il.so_invoice_id = ANY($2) \
             ORDER BY sl.line_order",
        )
        .bind(self.tenant.as_str())
        .bind(&ids)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;

        rows.into_iter()
            .map(|row| {
                let mine: Vec<SalesOrderInvoiceLine> = lines
                    .iter()
                    .filter(|line| line.so_invoice_id == row.id)
                    .map(InvoiceLineRow::to_line)
                    .collect();
                row.into_invoice(mine)
            })
            .collect()
    }
}

/// The refusal when there is nothing left to bill, which is the ordinary way a
/// second press of the button ends.
///
/// It names the order when the order has a number, and says *why* there is
/// nothing — waiting for goods to go out reads very differently from having
/// billed everything already, and a person acts on the difference.
fn nothing_to_bill(number: Option<&str>, lines: &[SoLine]) -> String {
    let named = number.map_or_else(
        || "this sales order".to_owned(),
        |number| format!("sales order {number}"),
    );
    if lines
        .iter()
        .any(|line| line.is_goods() && line.delivered_qty_milli > 0)
    {
        return format!(
            "everything that has gone out against {named} is already on an invoice; deliver more \
             of it before invoicing it again"
        );
    }
    format!("nothing has gone out against {named} yet, and an invoice carries what was delivered")
}

/// Takes the order's row lock inside `tx` and returns its status.
///
/// A duplicate in spirit of [`crate::inv_so_deliver`]'s, and deliberately not
/// shared with it for the reason stated there: a lock helper reached into from
/// another module is how a lock ends up taken in one place and relied on in
/// another.
///
/// # Errors
/// [`StoreError::NotFound`] when the id is absent **or another tenant's**;
/// [`StoreError::Db`] on failure or on a status the code does not know.
async fn lock_status(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant: &str,
    id: &str,
) -> Result<SoStatus> {
    let stored: Option<String> = sqlx::query_scalar(
        "SELECT status FROM inv_sales_orders WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
    )
    .bind(tenant)
    .bind(id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(StoreError::Db)?;
    let stored = stored.ok_or(StoreError::NotFound)?;
    SoStatus::parse(&stored).ok_or_else(|| {
        StoreError::Db(sqlx::Error::Decode(
            "inv_sales_orders.status is not a known status".into(),
        ))
    })
}

// ---- row types --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct OrderHeaderRow {
    customer_id: String,
    currency: String,
    number: Option<String>,
    reference: String,
}

#[derive(sqlx::FromRow)]
struct InvoiceRow {
    id: String,
    invoice_id: String,
    invoice_number: Option<String>,
    invoice_status: String,
    created_by: String,
    created_at: OffsetDateTime,
}

impl InvoiceRow {
    fn into_invoice(self, lines: Vec<SalesOrderInvoiceLine>) -> Result<SalesOrderInvoice> {
        let invoice_status = InvoiceStatus::parse(&self.invoice_status).ok_or_else(|| {
            StoreError::Db(sqlx::Error::Decode(
                "billing_invoices.status is not a known status".into(),
            ))
        })?;
        Ok(SalesOrderInvoice {
            id: InvSoInvoiceId::new(self.id),
            invoice_id: BillingInvoiceId::new(self.invoice_id),
            invoice_number: self.invoice_number,
            invoice_status,
            created_by: self.created_by,
            created_at: self.created_at,
            lines,
        })
    }
}

#[derive(sqlx::FromRow)]
struct InvoiceLineRow {
    so_invoice_id: String,
    so_line_id: String,
    qty_milli: i64,
}

impl InvoiceLineRow {
    fn to_line(&self) -> SalesOrderInvoiceLine {
        SalesOrderInvoiceLine {
            so_line_id: BillingLineId::new(self.so_line_id.clone()),
            qty_milli: self.qty_milli,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::billing_line::Line;
    use crate::id::BillingProductId;

    /// An ordered line as the invoicing read sees it: what was ordered, what has
    /// gone out, and what is already on a document.
    fn line(
        id: &str,
        product: Option<&str>,
        ordered: i64,
        delivered: i64,
        invoiced: i64,
    ) -> SoLine {
        SoLine {
            product_id: product.map(BillingProductId::new),
            line: Line {
                id: BillingLineId::new(id),
                line_order: 0,
                description: "Blue chair".to_owned(),
                unit: "piece".to_owned(),
                qty_milli: ordered,
                unit_price_cents: 8_600,
                vat_rate_bp: 2100,
            },
            delivered_qty_milli: delivered,
            invoiced_qty_milli: invoiced,
        }
    }

    /// A charge in words: no product, never delivered, billed once.
    fn words(id: &str, qty: i64, invoiced: i64) -> SoLine {
        let mut charge = line(id, None, qty, 0, invoiced);
        charge.line.description = "Delivery to the third floor".to_owned();
        charge.line.unit = String::new();
        charge.line.unit_price_cents = 4_500;
        charge
    }

    #[test]
    fn an_invoice_carries_what_went_out_and_not_what_was_ordered() {
        // Four chairs ordered, two and a half delivered, nothing billed yet.
        let lines = [line("l1", Some("p1"), 4_000, 2_500, 0)];
        let carrying = invoiceable(&lines);
        assert_eq!(carrying.len(), 1);
        assert_eq!(carrying[0].position, 1);
        assert_eq!(carrying[0].so_line_id.as_str(), "l1");
        assert_eq!(carrying[0].line.qty_milli, 2_500, "delivered, not ordered");
        assert_eq!(carrying[0].line.description, "Blue chair");
        assert_eq!(carrying[0].line.unit, "piece");
        assert_eq!(
            carrying[0].line.unit_price_cents, 8_600,
            "the order's snapshot, not today's catalog"
        );
        assert_eq!(carrying[0].line.vat_rate_bp, 2100);
    }

    #[test]
    fn a_second_invoice_carries_only_what_is_new() {
        // The rest of the line went out after the first document was raised.
        let lines = [line("l1", Some("p1"), 4_000, 4_000, 2_500)];
        let carrying = invoiceable(&lines);
        assert_eq!(carrying.len(), 1);
        assert_eq!(carrying[0].line.qty_milli, 1_500);
    }

    #[test]
    fn nothing_delivered_since_the_last_invoice_carries_nothing_at_all() {
        let lines = [line("l1", Some("p1"), 4_000, 2_500, 2_500)];
        assert!(invoiceable(&lines).is_empty());
    }

    #[test]
    fn goods_that_have_not_gone_out_are_never_billed() {
        let lines = [line("l1", Some("p1"), 4_000, 0, 0)];
        assert!(
            invoiceable(&lines).is_empty(),
            "a VAT event asserted on a hope"
        );
    }

    #[test]
    fn a_charge_in_words_rides_on_the_first_invoice_and_never_on_the_second() {
        let first = [
            line("l1", Some("p1"), 4_000, 2_500, 0),
            words("l2", 1_000, 0),
        ];
        let carrying = invoiceable(&first);
        assert_eq!(carrying.len(), 2);
        assert_eq!(carrying[1].so_line_id.as_str(), "l2");
        assert_eq!(carrying[1].line.qty_milli, 1_000, "in full, once");

        let second = [
            line("l1", Some("p1"), 4_000, 4_000, 2_500),
            words("l2", 1_000, 1_000),
        ];
        let again = invoiceable(&second);
        assert_eq!(again.len(), 1, "the charge is not billed twice");
        assert_eq!(again[0].so_line_id.as_str(), "l1");
    }

    #[test]
    fn a_charge_waits_until_something_has_actually_gone_out() {
        // Billing the delivery before the van came would charge for a journey
        // nobody made.
        let waiting = [line("l1", Some("p1"), 4_000, 0, 0), words("l2", 1_000, 0)];
        assert!(invoiceable(&waiting).is_empty());
        assert!(!due(&waiting));
    }

    #[test]
    fn an_order_that_sells_no_goods_bills_its_charges_straight_away() {
        // There is no first consignment to wait for; waiting forever would be
        // money left on the table.
        let services = [words("l1", 1_000, 0)];
        assert!(due(&services));
        let carrying = invoiceable(&services);
        assert_eq!(carrying.len(), 1);
        assert_eq!(carrying[0].line.qty_milli, 1_000);
    }

    #[test]
    fn a_discount_granted_in_words_is_carried_once_and_stays_negative() {
        let mut discount = words("l2", -1_000, 0);
        discount.line.description = "Trade discount".to_owned();
        let lines = [line("l1", Some("p1"), 4_000, 4_000, 0), discount.clone()];
        let carrying = invoiceable(&lines);
        assert_eq!(carrying.len(), 2);
        assert_eq!(carrying[1].line.qty_milli, -1_000, "never flipped");

        discount.invoiced_qty_milli = -1_000;
        let settled = [line("l1", Some("p1"), 4_000, 4_000, 4_000), discount];
        assert!(invoiceable(&settled).is_empty(), "granted exactly once");
    }

    #[test]
    fn a_line_that_would_carry_nothing_is_left_off_the_document() {
        // The second line has gone out in full and been billed; a zero-quantity
        // line on a customer's invoice is noise they have to be told to ignore.
        let lines = [
            line("l1", Some("p1"), 4_000, 2_500, 0),
            line("l2", Some("p2"), 2_000, 2_000, 2_000),
        ];
        let carrying = invoiceable(&lines);
        assert_eq!(carrying.len(), 1);
        assert_eq!(carrying[0].so_line_id.as_str(), "l1");
        assert_eq!(carrying[0].position, 1);
    }

    #[test]
    fn a_line_billed_beyond_what_went_out_asks_for_nothing_further() {
        // Reachable only by a document corrected downward in billing; the
        // answer is nothing left to bill, never a negative line on a new one.
        let lines = [line("l1", Some("p1"), 4_000, 2_500, 4_000)];
        assert!(invoiceable(&lines).is_empty());
    }

    #[test]
    fn the_refusal_says_which_of_the_two_reasons_it_is() {
        let waiting = [line("l1", Some("p1"), 4_000, 0, 0)];
        let message = nothing_to_bill(Some("SO-2026-00001"), &waiting);
        assert!(message.contains("SO-2026-00001"), "{message}");
        assert!(message.contains("nothing has gone out"), "{message}");

        let billed = [line("l1", Some("p1"), 4_000, 4_000, 4_000)];
        let message = nothing_to_bill(Some("SO-2026-00001"), &billed);
        assert!(message.contains("already on an invoice"), "{message}");
        assert!(message.contains("deliver more"), "{message}");

        // An order with no number is still named, without inventing one.
        let unnumbered = nothing_to_bill(None, &billed);
        assert!(unnumbered.contains("this sales order"), "{unnumbered}");
    }
}
