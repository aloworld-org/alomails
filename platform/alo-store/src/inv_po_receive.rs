//! **Receiving** a purchase order (alo Inventory, ADR 0035, wave B5.05b) — the
//! goods arriving, and the three things that happen in the one transaction that
//! books them (`docs/design/inventory.md`, "Receiving, and the three-way-lite
//! match"):
//!
//! 1. **Movements in**, one per line that arrived: from the tenant's virtual
//!    `supplier` location to the place the goods were actually put, reason
//!    `purchase`, referencing the order.
//! 2. **The order's state**: the received quantity accumulates on each ordered
//!    line, and the order becomes `partially_received` or — when every line has
//!    arrived in full — `received`, stamped with the day it closed.
//! 3. **A draft bill**: a [`crate::billing_bills`] record in status `received`,
//!    carrying the supplier as our master record has them, the lines exactly as
//!    they arrived, and the prices we agreed.
//!
//! The third is the *lite* in "three-way match, lite", and *lite* is the honest
//! word. A full three-way match compares the order, the goods receipt **and**
//! the supplier's own invoice, and blocks payment on a discrepancy. What is
//! built here is the first two legs: the receipt is matched against the order —
//! an over-receipt is refused, below — and the bill it drafts states what we
//! *ordered and received*, not what the supplier billed. Their real invoice
//! arrives later through [`crate::billing_einvoice_import`] as a **second**
//! bill, and reconciling the two is the third leg, named in the design note's
//! cuts and deliberately not built.
//!
//! **All three, or none.** The movements, the accumulators, the status and the
//! bill are one transaction: a tenant is never left holding stock no document
//! explains, nor a bill for goods the ledger never saw. The movements go through
//! [`AccountStore::record_move_in`], so the cached balance and the negative
//! stock rule are the ledger's own and not restated here.
//!
//! **Over-receipt is refused**, with a [`StoreError::Conflict`] naming the line,
//! what was ordered, what has already arrived and what the total would become.
//! The alternative — a tolerance percentage, which real procurement systems have
//! — needs to know what the right tolerance is, and the right tolerance is a
//! per-supplier commercial agreement we have no field for and no way to guess. A
//! genuine over-delivery is a receipt of what was ordered plus a manual
//! adjustment with a reason ([`crate::inv_adjust`]): two calls, and a person's
//! note explaining the third pallet.
//!
//! **Under-receipt is ordinary.** Partial deliveries are the normal case: the
//! order stays open, and what is still to come is the difference between the
//! ordered and received quantities on its lines.

use time::{Date, OffsetDateTime};

use crate::account::AccountStore;
use crate::billing_bills::{BillTotals, NewBill, Supplier as BillSupplier};
use crate::billing_field::bounded;
use crate::billing_line::NewLine;
use crate::billing_totals::{LineFigures, totals};
use crate::error::{Result, StoreError};
use crate::id::{
    BillingBillId, BillingLineId, BillingProductId, InvLocationId, InvMoveId, InvPoReceiptId,
    InvPurchaseOrderId, InvSupplierId,
};
use crate::inv_locations::{Location, LocationKind};
use crate::inv_moves::{MoveReason, MoveRefKind, MoveReference, NewMove};
use crate::inv_po::{PoStatus, PurchaseOrderDocument};
use crate::inv_po_lines::{self, PoLine};

/// What a person wrote on the delivery — "one crate damaged", not a document.
/// The same bound a movement's note carries, and the database's own CHECK.
pub const RECEIPT_NOTE_MAX_CHARS: usize = 500;

/// The columns every read of a receipt selects, in `ReceiptRow` order.
const RECEIPT_COLS: &str = "r.id, r.sequence_no, r.location_id, l.code AS location_code, \
     l.name AS location_name, r.received_date, r.note, r.bill_id, r.created_by, r.created_at";

/// One ordered line, and how much of it arrived now.
#[derive(Debug, Clone)]
pub struct NewReceiptLine {
    /// The line of the order these goods are against.
    pub po_line_id: BillingLineId,
    /// How much arrived, in milli-units. Strictly positive.
    pub qty_milli: i64,
}

/// A delivery, as the person unpacking it states it.
#[derive(Debug, Clone)]
pub struct NewReceipt {
    /// Where the goods were put. One of this tenant's **real** locations — a
    /// warehouse, a shop floor, a van.
    pub location_id: InvLocationId,
    /// What arrived, line by line — or `None` for **everything still
    /// outstanding**, which is what a delivery that matches the order is, and
    /// the case a warehouse should not have to type out.
    pub lines: Option<Vec<NewReceiptLine>>,
    /// What the person who unpacked it wrote.
    pub note: String,
}

/// One line of a stored receipt: which ordered line arrived, how much, and the
/// movement it wrote.
#[derive(Debug, Clone)]
pub struct ReceiptLine {
    /// The ordered line these goods were against.
    pub po_line_id: BillingLineId,
    /// The catalog item that moved. `None` only once the product has since been
    /// deleted from the catalog — a receipt is always of goods.
    pub product_id: Option<BillingProductId>,
    /// The ordered line's description, as it was agreed.
    pub description: String,
    /// How much arrived, in milli-units.
    pub qty_milli: i64,
    /// The movement into stock this line wrote.
    pub move_id: InvMoveId,
}

/// A stored receipt: one delivery against one order.
#[derive(Debug, Clone)]
pub struct Receipt {
    /// Opaque id, unique within the tenant.
    pub id: InvPoReceiptId,
    /// Which delivery against this order it is — 1 for the first.
    pub sequence_no: i32,
    /// Where the goods were put.
    pub location_id: InvLocationId,
    /// That location's code.
    pub location_code: String,
    /// That location's name.
    pub location_name: String,
    /// The day they arrived, from the database's clock.
    pub received_date: Date,
    /// What the person who unpacked it wrote.
    pub note: String,
    /// The bill this receipt drafted, while it still exists — an undecided bill
    /// may be thrown away, and what arrived still arrived.
    pub bill_id: Option<BillingBillId>,
    /// Who booked it.
    pub created_by: String,
    /// When it was booked, which is not when the goods arrived.
    pub created_at: OffsetDateTime,
    /// What arrived, in the order's own line order.
    pub lines: Vec<ReceiptLine>,
}

/// Everything a booked delivery changed: the order as it now stands, the
/// receipt itself, and the bill it drafted.
#[derive(Debug, Clone)]
pub struct ReceiptOutcome {
    /// The order, re-read: new status, new received quantities, same totals.
    pub order: PurchaseOrderDocument,
    /// The receipt as stored.
    pub receipt: Receipt,
    /// The draft bill raised for what arrived.
    pub bill_id: BillingBillId,
}

/// One ordered line paired with the quantity of it that arrived — the resolved
/// form both the movements and the bill's lines are built from.
#[derive(Debug)]
struct Booking<'a> {
    /// 1-based position on the order, for a refusal a person can act on.
    position: usize,
    /// The ordered line: its product, its words, its price.
    line: &'a PoLine,
    /// How much arrived now, in milli-units.
    qty_milli: i64,
}

/// Resolves what a caller says arrived against what the order says is still
/// outstanding. Pure, so every refusal is unit-tested without a database.
///
/// `asked` is `None` for "everything still outstanding" — a delivery that
/// matches the order, which is the ordinary case and should not have to be
/// typed out. Stated lines are taken in the caller's order and held to four
/// rules: the line must be on this order, it must be goods, the quantity must be
/// positive, and it must not exceed what is still to come.
///
/// # Errors
/// [`StoreError::NotFound`] when a stated line is not on this order (an id from
/// another order — or another tenant's — is never confirmed);
/// [`StoreError::Validation`] on a charge in words, a non-positive quantity, the
/// same line twice, or a receipt that books nothing at all;
/// [`StoreError::Conflict`] on an over-receipt.
fn resolve<'a>(lines: &'a [PoLine], asked: Option<&[NewReceiptLine]>) -> Result<Vec<Booking<'a>>> {
    let Some(asked) = asked else {
        let outstanding: Vec<Booking<'a>> = lines
            .iter()
            .enumerate()
            .filter(|(_, line)| line.outstanding_qty_milli() > 0)
            .map(|(index, line)| Booking {
                position: index + 1,
                line,
                qty_milli: line.outstanding_qty_milli(),
            })
            .collect();
        if outstanding.is_empty() {
            return Err(StoreError::Validation(
                "everything on this order has already arrived; there is nothing left to receive"
                    .to_owned(),
            ));
        }
        return Ok(outstanding);
    };

    let mut bookings: Vec<Booking<'a>> = Vec::with_capacity(asked.len());
    for entry in asked {
        let wanted = entry.po_line_id.as_str();
        let (index, line) = lines
            .iter()
            .enumerate()
            .find(|(_, line)| line.line.id.as_str() == wanted)
            .ok_or(StoreError::NotFound)?;
        let position = index + 1;
        if bookings.iter().any(|booked| booked.position == position) {
            return Err(StoreError::Validation(format!(
                "line {position} is booked twice in one delivery; state what arrived once"
            )));
        }
        if !line.is_goods() {
            return Err(StoreError::Validation(format!(
                "line {position} is a charge in words, not goods; nothing arrives against it"
            )));
        }
        if entry.qty_milli <= 0 {
            return Err(StoreError::Validation(format!(
                "line {position}: a receipt books what arrived, so the quantity must be more \
                 than nothing"
            )));
        }
        let outstanding = line.outstanding_qty_milli();
        if entry.qty_milli > outstanding {
            return Err(StoreError::Conflict(format!(
                "line {position} ({}): {} milli-units were ordered and {} have already arrived, \
                 so {} more would make {}; receive what was ordered and record the rest as an \
                 adjustment with a reason",
                line.line.description,
                line.line.qty_milli,
                line.received_qty_milli,
                entry.qty_milli,
                line.received_qty_milli.saturating_add(entry.qty_milli),
            )));
        }
        bookings.push(Booking {
            position,
            line,
            qty_milli: entry.qty_milli,
        });
    }
    if bookings.is_empty() {
        return Err(StoreError::Validation(
            "a receipt must say what arrived; it books at least one line".to_owned(),
        ));
    }
    Ok(bookings)
}

/// The status the order is in once these bookings are applied: `received` when
/// every line of goods has arrived in full, `partially_received` otherwise.
///
/// A charge in words never holds an order open — freight does not arrive on a
/// pallet — which [`PoLine::is_fully_received`] states once for both callers.
fn status_after(lines: &[PoLine], bookings: &[Booking<'_>]) -> PoStatus {
    let complete = lines.iter().enumerate().all(|(index, line)| {
        let booked: i64 = bookings
            .iter()
            .filter(|booking| booking.position == index + 1)
            .map(|booking| booking.qty_milli)
            .sum();
        line.outstanding_qty_milli() <= booked
    });
    if complete {
        PoStatus::Received
    } else {
        PoStatus::PartiallyReceived
    }
}

/// The number the drafted bill carries: the order's own number, and which
/// delivery it was — `PO-2026-00001/R1`.
///
/// It is deliberately **ours** and says so. A bill is keyed by
/// `(supplier, number)`, so a number of our own shape can never collide with the
/// supplier's real invoice when that arrives, and a person reading the list can
/// see at a glance which document is theirs and which is our record of a
/// delivery.
fn drafted_bill_number(order_number: &str, sequence_no: i32) -> String {
    format!("{order_number}/R{sequence_no}")
}

/// The totals a drafted bill states: our own arithmetic over the lines that
/// arrived, in the one shape [`BillTotals`] holds.
///
/// There is no document-level allowance or charge on a bill we drafted — those
/// are things a supplier writes on their invoice — and nothing is prepaid, so
/// the amount due is the gross.
fn drafted_totals(figures: &[LineFigures]) -> BillTotals {
    let computed = totals(figures);
    BillTotals {
        line_total_cents: computed.net_cents,
        allowance_total_cents: 0,
        charge_total_cents: 0,
        tax_exclusive_cents: computed.net_cents,
        tax_total_cents: computed.vat_cents,
        tax_inclusive_cents: computed.gross_cents,
        prepaid_cents: 0,
        payable_cents: computed.gross_cents,
    }
}

impl AccountStore {
    /// Books a delivery against one of this tenant's **open** orders: the
    /// movements into stock, the order's new state, and the draft bill for what
    /// arrived — in one transaction.
    ///
    /// The day is the database's `CURRENT_DATE`, read inside that transaction
    /// and never supplied by a caller, like every other business date this store
    /// stamps.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the order — or a line the caller names — is
    /// not this tenant's; [`StoreError::Conflict`] when the order is not open
    /// ([`PoStatus::ensure_receivable`]) or the delivery would over-receive a
    /// line; [`StoreError::Validation`] when the destination is not a real place
    /// of this tenant's, the tenant has no `supplier` location to receive from,
    /// or the stated lines break a rule; [`StoreError::Db`] on failure. Every one
    /// of them leaves the order, the ledger and the bills exactly as they were.
    pub async fn receive_inv_purchase_order(
        &self,
        id: &InvPurchaseOrderId,
        input: &NewReceipt,
    ) -> Result<ReceiptOutcome> {
        let note = bounded("note", &input.note, RECEIPT_NOTE_MAX_CHARS)?;
        // **Whose order it is, first.** Not the authority — the lock below is —
        // but the order of refusals is itself a tenancy rule: a delivery booked
        // against another tenant's order must be a bare `NotFound`, never a
        // complaint about the caller's own warehouse, which would say that the
        // order was at least worth looking at.
        self.purchase_order_status(id).await?.ensure_receivable()?;

        // Both ends of every movement, resolved before anything is written. The
        // goods come from the tenant's virtual `supplier` location — absent only
        // for a tenant who deleted what they were seeded with, and the document
        // is then refused rather than booked somewhere plausible.
        let from = self
            .inv_location_of_kind(LocationKind::Supplier)
            .await?
            .ok_or_else(|| {
                StoreError::Validation(
                    "this workspace has no supplier location to receive goods from; open \
                     Inventory once to have the standard locations created"
                        .to_owned(),
                )
            })?;
        let to = self.destination(&input.location_id).await?;

        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        // The status under the row lock is the authority: a cancellation or a
        // second delivery that arrived while this one was being typed queues
        // here and then sees the state this transaction wrote.
        let locked = lock_status(&mut tx, self.tenant.as_str(), id.as_str()).await?;
        locked.ensure_receivable()?;

        let lines = inv_po_lines::read(&mut *tx, self.tenant.as_str(), id.as_str()).await?;
        let bookings = resolve(&lines, input.lines.as_deref())?;
        let after = status_after(&lines, &bookings);
        if after != locked {
            locked.ensure_transition(after)?;
        }

        let today: Date = sqlx::query_scalar("SELECT CURRENT_DATE")
            .fetch_one(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        // Under the order's row lock, so two deliveries booked at the same
        // instant cannot both be "the second one" — and cannot draft one bill
        // number twice.
        let sequence_no: i32 = sqlx::query_scalar(
            "SELECT coalesce(max(sequence_no), 0) + 1 FROM inv_po_receipts \
             WHERE tenant_id = $1 AND po_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_one(&mut *tx)
        .await
        .map_err(StoreError::Db)?;

        // The movements, and the accumulator each one advances. The ledger's own
        // door writes both sides of the cached balance and refuses a movement
        // that would leave a real place holding less than nothing.
        let mut moves: Vec<InvMoveId> = Vec::with_capacity(bookings.len());
        for booking in &bookings {
            let product = booking.line.product_id.clone().ok_or_else(|| {
                // Unreachable: `resolve` refuses a line that is not goods. Kept
                // as a refusal rather than an unwrap, because the alternative is
                // a panic in a transaction that has already written movements.
                StoreError::Validation(format!(
                    "line {} is a charge in words, not goods; nothing arrives against it",
                    booking.position
                ))
            })?;
            let move_id = self
                .record_move_in(
                    &mut tx,
                    &NewMove {
                        product_id: product,
                        from_location_id: from.id.clone(),
                        to_location_id: to.id.clone(),
                        qty_milli: booking.qty_milli,
                        reason: MoveReason::Purchase,
                        reason_code: None,
                        note: String::new(),
                        reference: Some(MoveReference {
                            kind: MoveRefKind::PurchaseOrder,
                            id: id.as_str().to_owned(),
                        }),
                        occurred_at: None,
                    },
                )
                .await?;
            sqlx::query(
                "UPDATE inv_purchase_order_lines \
                    SET received_qty_milli = received_qty_milli + $3 \
                 WHERE tenant_id = $1 AND id = $2",
            )
            .bind(self.tenant.as_str())
            .bind(booking.line.line.id.as_str())
            .bind(booking.qty_milli)
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
            moves.push(move_id);
        }

        // The order's new state. `closed_date` is set exactly when it closes,
        // which the database's own CHECK also insists on.
        sqlx::query(
            "UPDATE inv_purchase_orders \
                SET status = $3, \
                    closed_date = CASE WHEN $3 = 'received' THEN $4::date ELSE closed_date END, \
                    updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(after.as_str())
        .bind(today)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;

        let bill_id = self
            .draft_bill_for(&mut tx, id, sequence_no, today, &bookings)
            .await?;

        let receipt_id = InvPoReceiptId::generate();
        sqlx::query(
            "INSERT INTO inv_po_receipts (tenant_id, id, po_id, location_id, sequence_no, \
                 received_date, note, bill_id, created_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(self.tenant.as_str())
        .bind(receipt_id.as_str())
        .bind(id.as_str())
        .bind(to.id.as_str())
        .bind(sequence_no)
        .bind(today)
        .bind(&note)
        .bind(bill_id.as_str())
        .bind(self.user.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;

        for (booking, move_id) in bookings.iter().zip(&moves) {
            sqlx::query(
                "INSERT INTO inv_po_receipt_lines (tenant_id, id, receipt_id, po_line_id, \
                     qty_milli, move_id) \
                 VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(self.tenant.as_str())
            .bind(BillingLineId::generate().as_str())
            .bind(receipt_id.as_str())
            .bind(booking.line.line.id.as_str())
            .bind(booking.qty_milli)
            .bind(move_id.as_str())
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        }
        tx.commit().await.map_err(StoreError::Db)?;

        let order = self
            .inv_purchase_order(id)
            .await?
            .ok_or(StoreError::NotFound)?;
        let receipt = self
            .inv_purchase_order_receipt(&receipt_id)
            .await?
            .ok_or(StoreError::NotFound)?;
        Ok(ReceiptOutcome {
            order,
            receipt,
            bill_id,
        })
    }

    /// Where the goods may be put: one of this tenant's **real**, active places.
    ///
    /// A virtual counterparty is refused by name — "the goods arrived at the
    /// supplier" is not a sentence, and booking a delivery into `adjustment`
    /// would make the correction ledger the warehouse.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the location is not this tenant's;
    /// [`StoreError::Validation`] when it is virtual or archived;
    /// [`StoreError::Db`] on failure.
    async fn destination(&self, id: &InvLocationId) -> Result<Location> {
        let location = self.inv_location(id).await?.ok_or(StoreError::NotFound)?;
        if !location.kind.is_real() {
            return Err(StoreError::Validation(format!(
                "goods cannot be received into {}: it is not a place anybody can walk into",
                location.code
            )));
        }
        if location.is_archived() {
            return Err(StoreError::Validation(format!(
                "{} is archived; choose a location that is still in use",
                location.code
            )));
        }
        Ok(location)
    }

    /// Drafts the bill for what arrived, inside the receiving transaction.
    ///
    /// The supplier is copied from **our** master record rather than from the
    /// order, because a bill has to be readable as a document about a company:
    /// their address, their VAT id, the account we pay into. The lines are the
    /// ones that arrived, at the prices the order agreed.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the supplier has vanished — impossible
    /// while the order's foreign key holds; [`StoreError::Db`] when a placed
    /// order carries no number, which is corrupt data rather than input;
    /// otherwise [`AccountStore::create_billing_bill_in`]'s.
    async fn draft_bill_for(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        id: &InvPurchaseOrderId,
        sequence_no: i32,
        today: Date,
        bookings: &[Booking<'_>],
    ) -> Result<BillingBillId> {
        let header: (String, Option<String>, String, String) = sqlx::query_as(
            "SELECT supplier_id, number, currency, reference FROM inv_purchase_orders \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&mut **tx)
        .await
        .map_err(StoreError::Db)?
        .ok_or(StoreError::NotFound)?;
        let (supplier_id, number, currency, reference) = header;
        let number = number.ok_or_else(|| {
            StoreError::Db(sqlx::Error::Decode(
                "a placed purchase order carries no number".into(),
            ))
        })?;

        let supplier = self
            .inv_supplier(&InvSupplierId::new(supplier_id))
            .await?
            .ok_or(StoreError::NotFound)?;
        let lines: Vec<NewLine> = bookings
            .iter()
            .map(|booking| NewLine {
                description: booking.line.line.description.clone(),
                unit: booking.line.line.unit.clone(),
                qty_milli: booking.qty_milli,
                unit_price_cents: booking.line.line.unit_price_cents,
                vat_rate_bp: booking.line.line.vat_rate_bp,
            })
            .collect();
        let figures: Vec<LineFigures> = lines
            .iter()
            .map(|line| LineFigures {
                qty_milli: line.qty_milli,
                unit_price_cents: line.unit_price_cents,
                vat_rate_bp: line.vat_rate_bp,
            })
            .collect();
        // Their terms, counted from the day the goods arrived. A supplier who
        // has stated none is due on receipt, which is what 0 days means.
        let due_date = today
            .checked_add(time::Duration::days(i64::from(
                supplier.payment_terms_days.max(0),
            )))
            .unwrap_or(today);

        let bill = NewBill {
            // Read from no file: this is our own record of a delivery, not the
            // supplier's document, and it must never be mistaken for one.
            source_syntax: None,
            source_sha256: String::new(),
            credit_note: false,
            supplier: BillSupplier {
                name: supplier.name,
                vat_id: supplier.vat_id.unwrap_or_default(),
                legal_id: supplier.registration_no,
                line1: supplier.address_line1,
                line2: supplier.address_line2,
                postal_code: supplier.postal_code,
                city: supplier.city,
                country: supplier.country,
                email: supplier.email.unwrap_or_default(),
                iban: supplier.iban.unwrap_or_default(),
            },
            number: drafted_bill_number(&number, sequence_no),
            issue_date: Some(today),
            due_date: Some(due_date),
            currency,
            // What we asked them to quote back: our own reference for the order,
            // carried onto the bill so the two documents read as one story.
            buyer_reference: reference,
            note: String::new(),
            payment_reference: String::new(),
            totals: drafted_totals(&figures),
            lines,
        };
        self.create_billing_bill_in(tx, &bill).await
    }

    /// What has arrived against one of this tenant's orders, newest delivery
    /// first, each with its lines in the order's own line order.
    ///
    /// An id that is not this tenant's — or not an order at all — is an empty
    /// list, never a refusal that would confirm it exists.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn inv_purchase_order_receipts(
        &self,
        po_id: &InvPurchaseOrderId,
    ) -> Result<Vec<Receipt>> {
        let rows = sqlx::query_as::<_, ReceiptRow>(&format!(
            "SELECT {RECEIPT_COLS} FROM inv_po_receipts r \
             JOIN inv_locations l ON l.tenant_id = r.tenant_id AND l.id = r.location_id \
             WHERE r.tenant_id = $1 AND r.po_id = $2 \
             ORDER BY r.sequence_no DESC"
        ))
        .bind(self.tenant.as_str())
        .bind(po_id.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        self.with_lines(rows).await
    }

    /// One receipt of this tenant, or `None` — including when the id belongs to
    /// another tenant (indistinguishable by design).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn inv_purchase_order_receipt(&self, id: &InvPoReceiptId) -> Result<Option<Receipt>> {
        let rows = sqlx::query_as::<_, ReceiptRow>(&format!(
            "SELECT {RECEIPT_COLS} FROM inv_po_receipts r \
             JOIN inv_locations l ON l.tenant_id = r.tenant_id AND l.id = r.location_id \
             WHERE r.tenant_id = $1 AND r.id = $2"
        ))
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(self.with_lines(rows).await?.into_iter().next())
    }

    /// Fills in the lines of a set of receipts in one further statement, not one
    /// per receipt.
    async fn with_lines(&self, rows: Vec<ReceiptRow>) -> Result<Vec<Receipt>> {
        if rows.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<String> = rows.iter().map(|row| row.id.clone()).collect();
        let lines = sqlx::query_as::<_, ReceiptLineRow>(
            "SELECT rl.receipt_id, rl.po_line_id, rl.qty_milli, rl.move_id, \
                 pl.product_id, pl.description \
             FROM inv_po_receipt_lines rl \
             JOIN inv_purchase_order_lines pl \
               ON pl.tenant_id = rl.tenant_id AND pl.id = rl.po_line_id \
             WHERE rl.tenant_id = $1 AND rl.receipt_id = ANY($2) \
             ORDER BY pl.line_order",
        )
        .bind(self.tenant.as_str())
        .bind(&ids)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;

        Ok(rows
            .into_iter()
            .map(|row| {
                let mine: Vec<ReceiptLine> = lines
                    .iter()
                    .filter(|line| line.receipt_id == row.id)
                    .map(ReceiptLineRow::to_line)
                    .collect();
                row.into_receipt(mine)
            })
            .collect())
    }
}

/// Takes the order's row lock inside `tx` and returns its status.
///
/// A duplicate in spirit of [`crate::inv_po_send`]'s, and deliberately not
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
) -> Result<PoStatus> {
    let stored: Option<String> = sqlx::query_scalar(
        "SELECT status FROM inv_purchase_orders WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
    )
    .bind(tenant)
    .bind(id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(StoreError::Db)?;
    let stored = stored.ok_or(StoreError::NotFound)?;
    PoStatus::parse(&stored).ok_or_else(|| {
        StoreError::Db(sqlx::Error::Decode(
            "inv_purchase_orders.status is not a known status".into(),
        ))
    })
}

// ---- row types --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct ReceiptRow {
    id: String,
    sequence_no: i32,
    location_id: String,
    location_code: String,
    location_name: String,
    received_date: Date,
    note: String,
    bill_id: Option<String>,
    created_by: String,
    created_at: OffsetDateTime,
}

impl ReceiptRow {
    fn into_receipt(self, lines: Vec<ReceiptLine>) -> Receipt {
        Receipt {
            id: InvPoReceiptId::new(self.id),
            sequence_no: self.sequence_no,
            location_id: InvLocationId::new(self.location_id),
            location_code: self.location_code,
            location_name: self.location_name,
            received_date: self.received_date,
            note: self.note,
            bill_id: self.bill_id.map(BillingBillId::new),
            created_by: self.created_by,
            created_at: self.created_at,
            lines,
        }
    }
}

#[derive(sqlx::FromRow)]
struct ReceiptLineRow {
    receipt_id: String,
    po_line_id: String,
    qty_milli: i64,
    move_id: String,
    product_id: Option<String>,
    description: String,
}

impl ReceiptLineRow {
    fn to_line(&self) -> ReceiptLine {
        ReceiptLine {
            po_line_id: BillingLineId::new(self.po_line_id.clone()),
            product_id: self.product_id.clone().map(BillingProductId::new),
            description: self.description.clone(),
            qty_milli: self.qty_milli,
            move_id: InvMoveId::new(self.move_id.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::billing_line::Line;

    /// An ordered line as receiving reads it back.
    fn line(id: &str, product: Option<&str>, ordered: i64, received: i64) -> PoLine {
        PoLine {
            product_id: product.map(BillingProductId::new),
            line: Line {
                id: BillingLineId::new(id),
                line_order: 0,
                description: "Blue chair".to_owned(),
                unit: "piece".to_owned(),
                qty_milli: ordered,
                unit_price_cents: 4_300,
                vat_rate_bp: 1900,
            },
            received_qty_milli: received,
        }
    }

    fn asked(id: &str, qty_milli: i64) -> NewReceiptLine {
        NewReceiptLine {
            po_line_id: BillingLineId::new(id),
            qty_milli,
        }
    }

    fn refusal(lines: &[PoLine], asked: &[NewReceiptLine]) -> String {
        match resolve(lines, Some(asked)) {
            Err(StoreError::Validation(message) | StoreError::Conflict(message)) => message,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn an_unstated_delivery_books_everything_still_outstanding() {
        // The ordinary case: the lorry brought what the order asked for, and
        // nobody should have to type it out line by line.
        let lines = [
            line("l1", Some("p1"), 4_000, 1_000),
            line("l2", None, 1_000, 0),
            line("l3", Some("p2"), 2_000, 2_000),
        ];
        let booked = resolve(&lines, None).unwrap_or_else(|e| panic!("rejected: {e:?}"));
        assert_eq!(booked.len(), 1, "words and finished lines are not booked");
        assert_eq!(booked[0].position, 1);
        assert_eq!(booked[0].qty_milli, 3_000, "what is still to come");
    }

    #[test]
    fn a_delivery_against_a_finished_order_books_nothing_and_says_so() {
        let lines = [line("l1", Some("p1"), 4_000, 4_000)];
        match resolve(&lines, None) {
            Err(StoreError::Validation(message)) => {
                assert!(message.contains("already arrived"), "{message}");
            }
            other => panic!("expected a Validation refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_stated_delivery_is_taken_line_by_line() {
        let lines = [
            line("l1", Some("p1"), 4_000, 0),
            line("l2", Some("p2"), 2_000, 0),
        ];
        let booked = resolve(&lines, Some(&[asked("l2", 500), asked("l1", 4_000)]))
            .unwrap_or_else(|e| panic!("rejected: {e:?}"));
        assert_eq!(booked.len(), 2);
        assert_eq!((booked[0].position, booked[0].qty_milli), (2, 500));
        assert_eq!((booked[1].position, booked[1].qty_milli), (1, 4_000));
    }

    #[test]
    fn more_than_was_ordered_is_refused_and_the_refusal_says_what_to_do() {
        let lines = [line("l1", Some("p1"), 4_000, 2_500)];
        let message = match resolve(&lines, Some(&[asked("l1", 2_000)])) {
            Err(StoreError::Conflict(message)) => message,
            other => panic!("expected a Conflict, got {other:?}"),
        };
        assert!(message.contains("line 1"), "{message}");
        assert!(
            message.contains("4000") && message.contains("2500"),
            "{message}"
        );
        assert!(
            message.contains("4500"),
            "the total it would make: {message}"
        );
        assert!(message.contains("adjustment"), "{message}");
        // Exactly what is outstanding is fine — that is a completed line.
        assert!(resolve(&lines, Some(&[asked("l1", 1_500)])).is_ok());
    }

    #[test]
    fn a_line_that_is_not_on_this_order_is_never_confirmed_to_exist() {
        let lines = [line("l1", Some("p1"), 4_000, 0)];
        match resolve(&lines, Some(&[asked("someone-elses-line", 1_000)])) {
            Err(StoreError::NotFound) => {}
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    #[test]
    fn nothing_arrives_against_a_charge_in_words() {
        let lines = [line("l1", None, 1_000, 0)];
        let message = refusal(&lines, &[asked("l1", 1_000)]);
        assert!(message.contains("charge in words"), "{message}");
    }

    #[test]
    fn a_quantity_is_positive_and_a_line_is_booked_once() {
        let lines = [line("l1", Some("p1"), 4_000, 0)];
        for nothing in [0, -1, -4_000] {
            let message = refusal(&lines, &[asked("l1", nothing)]);
            assert!(message.contains("more than nothing"), "{message}");
        }
        let twice = refusal(&lines, &[asked("l1", 1_000), asked("l1", 1_000)]);
        assert!(twice.contains("twice"), "{twice}");
    }

    #[test]
    fn an_empty_stated_set_is_not_a_delivery() {
        let lines = [line("l1", Some("p1"), 4_000, 0)];
        let message = refusal(&lines, &[]);
        assert!(message.contains("at least one line"), "{message}");
    }

    #[test]
    fn an_order_is_received_only_when_every_line_of_goods_has_arrived() {
        let lines = [
            line("l1", Some("p1"), 4_000, 1_000),
            line("l2", Some("p2"), 2_000, 0),
            line("l3", None, 1_000, 0),
        ];
        let part = resolve(&lines, Some(&[asked("l1", 3_000)]))
            .unwrap_or_else(|e| panic!("rejected: {e:?}"));
        assert_eq!(status_after(&lines, &part), PoStatus::PartiallyReceived);

        let rest = resolve(&lines, None).unwrap_or_else(|e| panic!("rejected: {e:?}"));
        assert_eq!(
            status_after(&lines, &rest),
            PoStatus::Received,
            "a charge in words never holds an order open"
        );
    }

    #[test]
    fn the_drafted_bill_is_numbered_as_ours_and_says_which_delivery_it_was() {
        assert_eq!(
            drafted_bill_number("PO-2026-00001", 1),
            "PO-2026-00001/R1",
            "a shape a supplier's own numbering can never collide with"
        );
        assert_eq!(drafted_bill_number("PO-2026-00042", 3), "PO-2026-00042/R3");
    }

    #[test]
    fn the_drafted_totals_are_our_arithmetic_over_what_arrived() {
        // Nothing is allowed, charged or prepaid on a bill we drafted: those are
        // things a supplier writes on their invoice.
        let figures = [LineFigures {
            qty_milli: 4_000,
            unit_price_cents: 4_300,
            vat_rate_bp: 1900,
        }];
        let stated = drafted_totals(&figures);
        assert_eq!(stated.line_total_cents, 17_200);
        assert_eq!(stated.tax_exclusive_cents, 17_200);
        assert_eq!(stated.tax_total_cents, 3_268);
        assert_eq!(stated.tax_inclusive_cents, 20_468);
        assert_eq!(stated.payable_cents, 20_468);
        assert_eq!(stated.allowance_total_cents, 0);
        assert_eq!(stated.charge_total_cents, 0);
        assert_eq!(stated.prepaid_cents, 0);
    }
}
