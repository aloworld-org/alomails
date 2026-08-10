//! **Delivering** a sales order (alo Inventory, ADR 0035, wave B5.06a) — the
//! goods leaving, and the two things that happen in the one transaction that
//! books them (`docs/design/inventory.md` § Delivery):
//!
//! 1. **Movements out**, one per line that went: from the place the goods were
//!    picked to the tenant's virtual `customer` location, reason `sale`,
//!    referencing the order.
//! 2. **The order's state**: the delivered quantity accumulates on each ordered
//!    line, and the order becomes `partially_delivered` or — when every line has
//!    gone in full — `delivered`, stamped with the day it closed.
//!
//! The delivery itself is the **delivery note**: the document that travels in
//! the box, numbered within its order (`SO-2026-00001/D1`), carrying the lines
//! and the quantities **and no prices** — the person unpacking the box is not
//! the person who negotiated it. This module stores that document; rendering it
//! as paper is the print layer's.
//!
//! No invoice is raised here. Invoicing what has been delivered and not yet
//! invoiced is B5.06b, and the decision it rests on is already recorded: **at
//! delivery, not at order**, because invoicing goods that may never leave is a
//! VAT event asserted on a hope.
//!
//! **Both, or neither.** The movements, the accumulators and the status are one
//! transaction: a tenant is never left holding an order that says goods went out
//! while the ledger says they are still on the shelf. The movements go through
//! [`AccountStore::record_move_in`], so the cached balance and the negative
//! stock rule are the ledger's own and not restated here.
//!
//! **You cannot ship what you do not have.** The negative-stock rule applies
//! here in its sharpest form, and the refusal names the product, the place and
//! what is actually available — the whole moves-only design exists to make that
//! refusal trustworthy. It is the ledger's refusal, raised at the movement, so a
//! delivery that would take a warehouse below zero leaves the order, the
//! accumulators and the ledger exactly as they were.
//!
//! **Over-delivery is refused**, with a [`StoreError::Conflict`] naming the
//! line, what was ordered, what has already gone and what the total would
//! become. Sending a customer more than they ordered is either a gift or a
//! mistake, and both are recorded as what they are: a delivery of what was
//! ordered plus a manual adjustment with a reason ([`crate::inv_adjust`]).
//!
//! **Under-delivery is ordinary.** Partial deliveries are the normal case: the
//! order stays open, and what is still owed is the difference between the
//! ordered and delivered quantities on its lines.

use time::{Date, OffsetDateTime};

use crate::account::AccountStore;
use crate::billing_field::bounded;
use crate::error::{Result, StoreError};
use crate::id::{
    BillingLineId, BillingProductId, InvLocationId, InvMoveId, InvSalesOrderId, InvSoDeliveryId,
};
use crate::inv_locations::{Location, LocationKind};
use crate::inv_moves::{MoveReason, MoveRefKind, MoveReference, NewMove};
use crate::inv_so::{SalesOrderDocument, SoStatus};
use crate::inv_so_lines::{self, SoLine};

/// What a person wrote on the delivery — "two boxes, driver Kowalski", not a
/// document. The same bound a movement's note carries, and the database's own
/// CHECK.
pub const DELIVERY_NOTE_MAX_CHARS: usize = 500;

/// The columns every read of a delivery selects, in `DeliveryRow` order.
const DELIVERY_COLS: &str = "d.id, d.sequence_no, d.location_id, l.code AS location_code, \
     l.name AS location_name, d.delivered_date, d.note, d.created_by, d.created_at";

/// One ordered line, and how much of it went out now.
#[derive(Debug, Clone)]
pub struct NewDeliveryLine {
    /// The line of the order these goods are against.
    pub so_line_id: BillingLineId,
    /// How much went, in milli-units. Strictly positive.
    pub qty_milli: i64,
}

/// A delivery, as the person who packed it states it.
#[derive(Debug, Clone)]
pub struct NewDelivery {
    /// Where the goods were picked from. One of this tenant's **real**
    /// locations — a warehouse, a shop floor, a van.
    pub location_id: InvLocationId,
    /// What went out, line by line — or `None` for **everything still
    /// outstanding**, which is what a delivery that completes the order is, and
    /// the case a warehouse should not have to type out.
    pub lines: Option<Vec<NewDeliveryLine>>,
    /// What the person who packed it wrote.
    pub note: String,
}

/// One line of a stored delivery: which ordered line went, how much, and the
/// movement it wrote.
///
/// Deliberately **no price**. This is the line of a delivery note, and a
/// delivery note carries quantities only.
#[derive(Debug, Clone)]
pub struct DeliveryLine {
    /// The ordered line these goods were against.
    pub so_line_id: BillingLineId,
    /// The catalog item that moved. `None` only once the product has since been
    /// deleted from the catalog — a delivery is always of goods.
    pub product_id: Option<BillingProductId>,
    /// The ordered line's description, as it was agreed.
    pub description: String,
    /// The ordered line's unit, so the note reads "4 pieces" rather than "4".
    pub unit: String,
    /// How much went out, in milli-units.
    pub qty_milli: i64,
    /// The movement out of stock this line wrote.
    pub move_id: InvMoveId,
}

/// A stored delivery: one consignment against one sales order — the delivery
/// note.
#[derive(Debug, Clone)]
pub struct Delivery {
    /// Opaque id, unique within the tenant.
    pub id: InvSoDeliveryId,
    /// Which consignment against this order it is — 1 for the first.
    pub sequence_no: i32,
    /// Where the goods were picked from.
    pub location_id: InvLocationId,
    /// That location's code.
    pub location_code: String,
    /// That location's name.
    pub location_name: String,
    /// The day they left, from the database's clock.
    pub delivered_date: Date,
    /// What the person who packed it wrote.
    pub note: String,
    /// Who booked it.
    pub created_by: String,
    /// When it was booked, which is not when the goods left.
    pub created_at: OffsetDateTime,
    /// What went out, in the order's own line order.
    pub lines: Vec<DeliveryLine>,
}

impl Delivery {
    /// The delivery note's number: the order's own number, and which
    /// consignment it was — `SO-2026-00001/D1`.
    ///
    /// Built rather than stored, from the order's number the caller already
    /// holds: two copies of one number are two things that can disagree.
    pub fn note_number(&self, order_number: &str) -> String {
        format!("{order_number}/D{}", self.sequence_no)
    }
}

/// Everything a booked delivery changed: the order as it now stands, and the
/// delivery note itself.
#[derive(Debug, Clone)]
pub struct DeliveryOutcome {
    /// The order, re-read: new status, new delivered quantities, same totals.
    pub order: SalesOrderDocument,
    /// The delivery as stored.
    pub delivery: Delivery,
}

/// One ordered line paired with the quantity of it that went out — the resolved
/// form the movements are built from.
#[derive(Debug)]
struct Picking<'a> {
    /// 1-based position on the order, for a refusal a person can act on.
    position: usize,
    /// The ordered line: its product, its words, its quantity.
    line: &'a SoLine,
    /// How much went now, in milli-units.
    qty_milli: i64,
}

/// Resolves what a caller says went out against what the order says is still
/// owed. Pure, so every refusal is unit-tested without a database.
///
/// `asked` is `None` for "everything still outstanding" — the delivery that
/// completes the order, which is the ordinary case and should not have to be
/// typed out. Stated lines are taken in the caller's order and held to four
/// rules: the line must be on this order, it must be goods, the quantity must be
/// positive, and it must not exceed what is still owed.
///
/// # Errors
/// [`StoreError::NotFound`] when a stated line is not on this order (an id from
/// another order — or another tenant's — is never confirmed);
/// [`StoreError::Validation`] on a charge in words, a non-positive quantity, the
/// same line twice, or a delivery that books nothing at all;
/// [`StoreError::Conflict`] on an over-delivery.
fn resolve<'a>(lines: &'a [SoLine], asked: Option<&[NewDeliveryLine]>) -> Result<Vec<Picking<'a>>> {
    let Some(asked) = asked else {
        let outstanding: Vec<Picking<'a>> = lines
            .iter()
            .enumerate()
            .filter(|(_, line)| line.outstanding_qty_milli() > 0)
            .map(|(index, line)| Picking {
                position: index + 1,
                line,
                qty_milli: line.outstanding_qty_milli(),
            })
            .collect();
        if outstanding.is_empty() {
            return Err(StoreError::Validation(
                "everything on this order has already gone out; there is nothing left to deliver"
                    .to_owned(),
            ));
        }
        return Ok(outstanding);
    };

    let mut pickings: Vec<Picking<'a>> = Vec::with_capacity(asked.len());
    for entry in asked {
        let wanted = entry.so_line_id.as_str();
        let (index, line) = lines
            .iter()
            .enumerate()
            .find(|(_, line)| line.line.id.as_str() == wanted)
            .ok_or(StoreError::NotFound)?;
        let position = index + 1;
        if pickings.iter().any(|picked| picked.position == position) {
            return Err(StoreError::Validation(format!(
                "line {position} is booked twice in one delivery; state what went out once"
            )));
        }
        if !line.is_goods() {
            return Err(StoreError::Validation(format!(
                "line {position} is a charge in words, not goods; nothing leaves against it"
            )));
        }
        if entry.qty_milli <= 0 {
            return Err(StoreError::Validation(format!(
                "line {position}: a delivery books what went out, so the quantity must be more \
                 than nothing"
            )));
        }
        let outstanding = line.outstanding_qty_milli();
        if entry.qty_milli > outstanding {
            return Err(StoreError::Conflict(format!(
                "line {position} ({}): {} milli-units were ordered and {} have already gone out, \
                 so {} more would make {}; deliver what was ordered and record the rest as an \
                 adjustment with a reason",
                line.line.description,
                line.line.qty_milli,
                line.delivered_qty_milli,
                entry.qty_milli,
                line.delivered_qty_milli.saturating_add(entry.qty_milli),
            )));
        }
        pickings.push(Picking {
            position,
            line,
            qty_milli: entry.qty_milli,
        });
    }
    if pickings.is_empty() {
        return Err(StoreError::Validation(
            "a delivery must say what went out; it books at least one line".to_owned(),
        ));
    }
    Ok(pickings)
}

/// The status the order is in once these pickings are applied: `delivered` when
/// every line of goods has gone in full, `partially_delivered` otherwise.
///
/// A charge in words never holds an order open — assembly does not leave on a
/// pallet — which [`SoLine::is_fully_delivered`] states once for both callers.
fn status_after(lines: &[SoLine], pickings: &[Picking<'_>]) -> SoStatus {
    let complete = lines.iter().enumerate().all(|(index, line)| {
        let picked: i64 = pickings
            .iter()
            .filter(|picking| picking.position == index + 1)
            .map(|picking| picking.qty_milli)
            .sum();
        line.outstanding_qty_milli() <= picked
    });
    if complete {
        SoStatus::Delivered
    } else {
        SoStatus::PartiallyDelivered
    }
}

impl AccountStore {
    /// Books a consignment against one of this tenant's **open** orders: the
    /// movements out of stock and the order's new state, in one transaction, and
    /// stores the delivery note that describes them.
    ///
    /// The day is the database's `CURRENT_DATE`, read inside that transaction
    /// and never supplied by a caller, like every other business date this store
    /// stamps.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the order — or a line the caller names — is
    /// not this tenant's; [`StoreError::Conflict`] when the order is not open
    /// ([`SoStatus::ensure_deliverable`]), the delivery would over-deliver a
    /// line, or **the place it is picked from has not got the goods**;
    /// [`StoreError::Validation`] when the source is not a real place of this
    /// tenant's, the tenant has no `customer` location to deliver to, or the
    /// stated lines break a rule; [`StoreError::Db`] on failure. Every one of
    /// them leaves the order and the ledger exactly as they were.
    pub async fn deliver_inv_sales_order(
        &self,
        id: &InvSalesOrderId,
        input: &NewDelivery,
    ) -> Result<DeliveryOutcome> {
        let note = bounded("note", &input.note, DELIVERY_NOTE_MAX_CHARS)?;
        // **Whose order it is, first.** Not the authority — the lock below is —
        // but the order of refusals is itself a tenancy rule: a delivery booked
        // against another tenant's order must be a bare `NotFound`, never a
        // complaint about the caller's own warehouse, which would say that the
        // order was at least worth looking at.
        self.sales_order_status(id).await?.ensure_deliverable()?;

        // Both ends of every movement, resolved before anything is written. The
        // goods go to the tenant's virtual `customer` location — absent only for
        // a tenant who deleted what they were seeded with, and the document is
        // then refused rather than booked somewhere plausible.
        let from = self.source(&input.location_id).await?;
        let to = self
            .inv_location_of_kind(LocationKind::Customer)
            .await?
            .ok_or_else(|| {
                StoreError::Validation(
                    "this workspace has no customer location to deliver goods to; open Inventory \
                     once to have the standard locations created"
                        .to_owned(),
                )
            })?;

        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        // The status under the row lock is the authority: a cancellation or a
        // second consignment that left while this one was being typed queues
        // here and then sees the state this transaction wrote.
        let locked = lock_status(&mut tx, self.tenant.as_str(), id.as_str()).await?;
        locked.ensure_deliverable()?;

        let lines = inv_so_lines::read(&mut *tx, self.tenant.as_str(), id.as_str()).await?;
        let pickings = resolve(&lines, input.lines.as_deref())?;
        let after = status_after(&lines, &pickings);
        if after != locked {
            locked.ensure_transition(after)?;
        }

        let today: Date = sqlx::query_scalar("SELECT CURRENT_DATE")
            .fetch_one(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        // Under the order's row lock, so two consignments booked at the same
        // instant cannot both be "the second one" — and cannot number one
        // delivery note twice.
        let sequence_no: i32 = sqlx::query_scalar(
            "SELECT coalesce(max(sequence_no), 0) + 1 FROM inv_so_deliveries \
             WHERE tenant_id = $1 AND so_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_one(&mut *tx)
        .await
        .map_err(StoreError::Db)?;

        // The movements, and the accumulator each one advances. The ledger's own
        // door writes both sides of the cached balance and refuses a movement
        // that would leave a real place holding less than nothing — which is
        // exactly "you cannot ship what you do not have".
        let mut moves: Vec<InvMoveId> = Vec::with_capacity(pickings.len());
        for picking in &pickings {
            let product = picking.line.product_id.clone().ok_or_else(|| {
                // Unreachable: `resolve` refuses a line that is not goods. Kept
                // as a refusal rather than an unwrap, because the alternative is
                // a panic in a transaction that has already written movements.
                StoreError::Validation(format!(
                    "line {} is a charge in words, not goods; nothing leaves against it",
                    picking.position
                ))
            })?;
            let move_id = self
                .record_move_in(
                    &mut tx,
                    &NewMove {
                        product_id: product,
                        from_location_id: from.id.clone(),
                        to_location_id: to.id.clone(),
                        qty_milli: picking.qty_milli,
                        reason: MoveReason::Sale,
                        reason_code: None,
                        note: String::new(),
                        reference: Some(MoveReference {
                            kind: MoveRefKind::SalesOrder,
                            id: id.as_str().to_owned(),
                        }),
                        occurred_at: None,
                    },
                )
                .await?;
            sqlx::query(
                "UPDATE inv_sales_order_lines \
                    SET delivered_qty_milli = delivered_qty_milli + $3 \
                 WHERE tenant_id = $1 AND id = $2",
            )
            .bind(self.tenant.as_str())
            .bind(picking.line.line.id.as_str())
            .bind(picking.qty_milli)
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
            moves.push(move_id);
        }

        // The order's new state. `closed_date` is set exactly when it closes,
        // which the database's own CHECK also insists on.
        sqlx::query(
            "UPDATE inv_sales_orders \
                SET status = $3, \
                    closed_date = CASE WHEN $3 = 'delivered' THEN $4::date ELSE closed_date END, \
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

        let delivery_id = InvSoDeliveryId::generate();
        sqlx::query(
            "INSERT INTO inv_so_deliveries (tenant_id, id, so_id, location_id, sequence_no, \
                 delivered_date, note, created_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(self.tenant.as_str())
        .bind(delivery_id.as_str())
        .bind(id.as_str())
        .bind(from.id.as_str())
        .bind(sequence_no)
        .bind(today)
        .bind(&note)
        .bind(self.user.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;

        for (picking, move_id) in pickings.iter().zip(&moves) {
            sqlx::query(
                "INSERT INTO inv_so_delivery_lines (tenant_id, id, delivery_id, so_line_id, \
                     qty_milli, move_id) \
                 VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(self.tenant.as_str())
            .bind(BillingLineId::generate().as_str())
            .bind(delivery_id.as_str())
            .bind(picking.line.line.id.as_str())
            .bind(picking.qty_milli)
            .bind(move_id.as_str())
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        }
        tx.commit().await.map_err(StoreError::Db)?;

        let order = self
            .inv_sales_order(id)
            .await?
            .ok_or(StoreError::NotFound)?;
        let delivery = self
            .inv_sales_order_delivery(&delivery_id)
            .await?
            .ok_or(StoreError::NotFound)?;
        Ok(DeliveryOutcome { order, delivery })
    }

    /// Where the goods may be picked from: one of this tenant's **real**, active
    /// places.
    ///
    /// A virtual counterparty is refused by name — picking from `customer` would
    /// be shipping goods we have already shipped, and picking from `supplier`
    /// would be shipping goods that never arrived.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the location is not this tenant's;
    /// [`StoreError::Validation`] when it is virtual or archived;
    /// [`StoreError::Db`] on failure.
    async fn source(&self, id: &InvLocationId) -> Result<Location> {
        let location = self.inv_location(id).await?.ok_or(StoreError::NotFound)?;
        if !location.kind.is_real() {
            return Err(StoreError::Validation(format!(
                "goods cannot be picked from {}: it is not a place anybody can walk into",
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

    /// What has gone out against one of this tenant's orders, newest
    /// consignment first, each with its lines in the order's own line order.
    ///
    /// An id that is not this tenant's — or not an order at all — is an empty
    /// list, never a refusal that would confirm it exists.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn inv_sales_order_deliveries(
        &self,
        so_id: &InvSalesOrderId,
    ) -> Result<Vec<Delivery>> {
        let rows = sqlx::query_as::<_, DeliveryRow>(&format!(
            "SELECT {DELIVERY_COLS} FROM inv_so_deliveries d \
             JOIN inv_locations l ON l.tenant_id = d.tenant_id AND l.id = d.location_id \
             WHERE d.tenant_id = $1 AND d.so_id = $2 \
             ORDER BY d.sequence_no DESC"
        ))
        .bind(self.tenant.as_str())
        .bind(so_id.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        self.with_delivery_lines(rows).await
    }

    /// One delivery of this tenant, or `None` — including when the id belongs to
    /// another tenant (indistinguishable by design).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn inv_sales_order_delivery(&self, id: &InvSoDeliveryId) -> Result<Option<Delivery>> {
        let rows = sqlx::query_as::<_, DeliveryRow>(&format!(
            "SELECT {DELIVERY_COLS} FROM inv_so_deliveries d \
             JOIN inv_locations l ON l.tenant_id = d.tenant_id AND l.id = d.location_id \
             WHERE d.tenant_id = $1 AND d.id = $2"
        ))
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(self.with_delivery_lines(rows).await?.into_iter().next())
    }

    /// Fills in the lines of a set of deliveries in one further statement, not
    /// one per delivery.
    async fn with_delivery_lines(&self, rows: Vec<DeliveryRow>) -> Result<Vec<Delivery>> {
        if rows.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<String> = rows.iter().map(|row| row.id.clone()).collect();
        let lines = sqlx::query_as::<_, DeliveryLineRow>(
            "SELECT dl.delivery_id, dl.so_line_id, dl.qty_milli, dl.move_id, \
                 sl.product_id, sl.description, sl.unit \
             FROM inv_so_delivery_lines dl \
             JOIN inv_sales_order_lines sl \
               ON sl.tenant_id = dl.tenant_id AND sl.id = dl.so_line_id \
             WHERE dl.tenant_id = $1 AND dl.delivery_id = ANY($2) \
             ORDER BY sl.line_order",
        )
        .bind(self.tenant.as_str())
        .bind(&ids)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;

        Ok(rows
            .into_iter()
            .map(|row| {
                let mine: Vec<DeliveryLine> = lines
                    .iter()
                    .filter(|line| line.delivery_id == row.id)
                    .map(DeliveryLineRow::to_line)
                    .collect();
                row.into_delivery(mine)
            })
            .collect())
    }
}

/// Takes the order's row lock inside `tx` and returns its status.
///
/// A duplicate in spirit of [`crate::inv_so_confirm`]'s, and deliberately not
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
struct DeliveryRow {
    id: String,
    sequence_no: i32,
    location_id: String,
    location_code: String,
    location_name: String,
    delivered_date: Date,
    note: String,
    created_by: String,
    created_at: OffsetDateTime,
}

impl DeliveryRow {
    fn into_delivery(self, lines: Vec<DeliveryLine>) -> Delivery {
        Delivery {
            id: InvSoDeliveryId::new(self.id),
            sequence_no: self.sequence_no,
            location_id: InvLocationId::new(self.location_id),
            location_code: self.location_code,
            location_name: self.location_name,
            delivered_date: self.delivered_date,
            note: self.note,
            created_by: self.created_by,
            created_at: self.created_at,
            lines,
        }
    }
}

#[derive(sqlx::FromRow)]
struct DeliveryLineRow {
    delivery_id: String,
    so_line_id: String,
    qty_milli: i64,
    move_id: String,
    product_id: Option<String>,
    description: String,
    unit: String,
}

impl DeliveryLineRow {
    fn to_line(&self) -> DeliveryLine {
        DeliveryLine {
            so_line_id: BillingLineId::new(self.so_line_id.clone()),
            product_id: self.product_id.clone().map(BillingProductId::new),
            description: self.description.clone(),
            unit: self.unit.clone(),
            qty_milli: self.qty_milli,
            move_id: InvMoveId::new(self.move_id.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::billing_line::Line;

    /// An ordered line as a delivery reads it back.
    fn line(id: &str, product: Option<&str>, ordered: i64, delivered: i64) -> SoLine {
        SoLine {
            product_id: product.map(BillingProductId::new),
            line: Line {
                id: BillingLineId::new(id),
                line_order: 0,
                description: "Blue chair".to_owned(),
                unit: "piece".to_owned(),
                qty_milli: ordered,
                unit_price_cents: 8_600,
                vat_rate_bp: 1900,
            },
            delivered_qty_milli: delivered,
        }
    }

    fn asked(id: &str, qty_milli: i64) -> NewDeliveryLine {
        NewDeliveryLine {
            so_line_id: BillingLineId::new(id),
            qty_milli,
        }
    }

    fn refusal(lines: &[SoLine], asked: &[NewDeliveryLine]) -> String {
        match resolve(lines, Some(asked)) {
            Err(StoreError::Validation(message) | StoreError::Conflict(message)) => message,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn an_unstated_delivery_books_everything_still_owed() {
        // The ordinary case: the van took what the order still owes, and nobody
        // should have to type it out line by line.
        let lines = [
            line("l1", Some("p1"), 4_000, 1_000),
            line("l2", None, 1_000, 0),
            line("l3", Some("p2"), 2_000, 2_000),
        ];
        let picked = resolve(&lines, None).unwrap_or_else(|e| panic!("rejected: {e:?}"));
        assert_eq!(picked.len(), 1, "words and finished lines are not picked");
        assert_eq!(picked[0].position, 1);
        assert_eq!(picked[0].qty_milli, 3_000, "what is still owed");
    }

    #[test]
    fn a_delivery_against_a_finished_order_books_nothing_and_says_so() {
        let lines = [line("l1", Some("p1"), 4_000, 4_000)];
        match resolve(&lines, None) {
            Err(StoreError::Validation(message)) => {
                assert!(message.contains("already gone out"), "{message}");
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
        let picked = resolve(&lines, Some(&[asked("l2", 500), asked("l1", 4_000)]))
            .unwrap_or_else(|e| panic!("rejected: {e:?}"));
        assert_eq!(picked.len(), 2);
        assert_eq!((picked[0].position, picked[0].qty_milli), (2, 500));
        assert_eq!((picked[1].position, picked[1].qty_milli), (1, 4_000));
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
    fn nothing_leaves_against_a_charge_in_words() {
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
    fn an_order_is_delivered_only_when_every_line_of_goods_has_gone() {
        let lines = [
            line("l1", Some("p1"), 4_000, 1_000),
            line("l2", Some("p2"), 2_000, 0),
            line("l3", None, 1_000, 0),
        ];
        let part = resolve(&lines, Some(&[asked("l1", 3_000)]))
            .unwrap_or_else(|e| panic!("rejected: {e:?}"));
        assert_eq!(status_after(&lines, &part), SoStatus::PartiallyDelivered);

        let rest = resolve(&lines, None).unwrap_or_else(|e| panic!("rejected: {e:?}"));
        assert_eq!(
            status_after(&lines, &rest),
            SoStatus::Delivered,
            "a charge in words never holds an order open"
        );
    }

    #[test]
    fn the_delivery_note_is_numbered_within_its_order() {
        let note = Delivery {
            id: InvSoDeliveryId::new("d"),
            sequence_no: 2,
            location_id: InvLocationId::new("loc"),
            location_code: "MAIN".to_owned(),
            location_name: "Hoofdmagazijn".to_owned(),
            delivered_date: Date::from_calendar_date(2026, time::Month::August, 10)
                .unwrap_or_else(|e| panic!("{e}")),
            note: String::new(),
            created_by: "u".to_owned(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            lines: Vec::new(),
        };
        assert_eq!(note.note_number("SO-2026-00001"), "SO-2026-00001/D2");
    }
}
