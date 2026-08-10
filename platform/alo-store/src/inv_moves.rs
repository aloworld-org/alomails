//! The move ledger — the one writer of a stock movement, and of the cached
//! balance that movement implies (alo Inventory, ADR 0035, wave B5.04a;
//! `docs/design/inventory.md`, "The move", "The cached balance" and "Negative
//! stock").
//!
//! Everything about this module follows from one sentence:
//!
//! > For every product, the sum of `qty_milli` over all movements **into**
//! > every location minus the sum over all movements **out of** every location
//! > is exactly zero.
//!
//! It is true *by construction* — every row contributes `+q` to one location
//! and `−q` to another, both of them real rows ([`crate::inv_locations`]) —
//! which is the point: it makes a whole class of bug impossible to write rather
//! than merely tested for. There is no quantity column on a product to drift
//! away from it, and `inv_stock` is a cache of the fold rather than a second
//! source of truth, kept honest by three rules:
//!
//! 1. **One writer.** [`AccountStore::record_move`] updates the cached row in
//!    the same transaction as the movement it summarises. No route, no other
//!    store function and no migration writes `inv_stock` — the discipline
//!    [`crate::fin_journal`]'s `post` established for the books.
//! 2. **Proven, not trusted.** [`AccountStore::inv_stock_folded`] recomputes
//!    the whole fold from the movements, and the property suite compares it to
//!    the cache after every write. A `verify=1` debug read is deliberately
//!    *not* offered: a discrepancy is a bug for a test to catch, not an
//!    operational condition for a tenant to inspect.
//! 3. **Rebuildable.** [`AccountStore::inv_stock_rebuild`] recreates every
//!    cached row from the ledger; its existence is what makes the cache
//!    disposable.
//!
//! **Direction is the pair of locations, never a sign.** A signed quantity with
//! one location column was the alternative; it makes "how much moved" a
//! question about absolute values and reintroduces exactly the sign confusion
//! `docs/design/finance.md` spent a section on. Quantities here are strictly
//! positive milli-units, the representation a document line already speaks
//! (B1.06), bounded by the same [`QTY_MAX_MILLI`] — so a purchase-order line
//! and the movement it produces need no conversion between them.
//!
//! **The table is append-only.** There is no update and no delete, here or at
//! any door above. A movement recorded in error is corrected by a movement in
//! the other direction with a note, because what happened, happened, and the
//! correction is itself a fact worth keeping.
//!
//! **Negative stock is refused**, at `stock` and `transit` locations, naming
//! the product, the location, what is available and what was asked for. The
//! virtual counterparties are unbounded by construction: `supplier` goes ever
//! more negative as we buy, which is the correct reading of how much has come
//! from outside. Permitting negative stock with a warning is what several
//! larger systems do, and it is rejected here because a negative balance means
//! the data is already known to be wrong — and a system that accepts a
//! known-wrong number will be asked to report on it.
//!
//! Two consequences of that rule, both deliberate:
//!
//! - **The check is against the balance now, not at `occurred_at`.**
//!   Back-dating is allowed — paperwork does catch up late — but goods physics
//!   is not retroactive: a shipment that left yesterday cannot be recorded
//!   today if the stock is not there today.
//! - **The check serialises per product-location.** The cached row's upsert
//!   holds its lock until commit, so two concurrent shipments of the last unit
//!   queue rather than race and exactly one fails. The trade
//!   [`crate::billing_sequence`] made for gapless numbering, at SME volumes,
//!   for free.

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::billing_field::bounded;
use crate::billing_line::QTY_MAX_MILLI;
use crate::error::{Result, StoreError};
use crate::id::{BillingProductId, InvLocationId, InvMoveId};
use crate::inv_locations::Location;

/// What a person typed about a movement — a sentence about a correction, not a
/// document.
pub const MOVE_NOTE_MAX_CHARS: usize = 500;
/// The id of the document a movement came from is an opaque id of ours; the
/// bound only stops a caller storing an essay in the column.
const MOVE_REF_ID_MAX_CHARS: usize = 64;
/// The most movements one ledger read returns. A warehouse's history is
/// unbounded; a screen's page is not.
pub const MOVES_PAGE_MAX: i64 = 500;

/// The columns every read of a movement selects, in `MoveRow` order.
const MOVE_COLS: &str = "m.id, m.product_id, p.name AS product_name, m.from_location_id, \
     f.code AS from_code, f.name AS from_name, m.to_location_id, t.code AS to_code, \
     t.name AS to_name, m.qty_milli, m.reason, m.note, m.ref_kind, m.ref_id, m.occurred_at, \
     m.created_by, m.created_at";

/// The joins those columns need: a movement always reads with the names of what
/// it moved and where, because an id is not an explanation.
const MOVE_JOINS: &str = "FROM inv_moves m \
     JOIN billing_products p ON p.tenant_id = m.tenant_id AND p.id = m.product_id \
     JOIN inv_locations f ON f.tenant_id = m.tenant_id AND f.id = m.from_location_id \
     JOIN inv_locations t ON t.tenant_id = m.tenant_id AND t.id = m.to_location_id";

/// Why the goods moved — a closed vocabulary, because "why is stock
/// disappearing" is a question with a small number of real answers and a
/// free-text field answers it with the empty string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MoveReason {
    /// Goods received against a purchase order (B5.05b).
    Purchase,
    /// Goods delivered against a sales order (B5.06a).
    Sale,
    /// Goods moved between two of the tenant's own locations.
    Transfer,
    /// A correction a person made and signed for (B5.04b).
    Adjustment,
    /// A stocktake variance (B5.08b).
    Count,
    /// Goods a customer sent back.
    ReturnIn,
    /// Goods sent back to a supplier.
    ReturnOut,
}

impl MoveReason {
    /// The stored word — the database value and the wire form, one spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Purchase => "purchase",
            Self::Sale => "sale",
            Self::Transfer => "transfer",
            Self::Adjustment => "adjustment",
            Self::Count => "count",
            Self::ReturnIn => "return_in",
            Self::ReturnOut => "return_out",
        }
    }

    /// Reads the stored word back.
    ///
    /// # Errors
    /// [`StoreError::Validation`] listing the accepted set.
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim() {
            "purchase" => Ok(Self::Purchase),
            "sale" => Ok(Self::Sale),
            "transfer" => Ok(Self::Transfer),
            "adjustment" => Ok(Self::Adjustment),
            "count" => Ok(Self::Count),
            "return_in" => Ok(Self::ReturnIn),
            "return_out" => Ok(Self::ReturnOut),
            _ => Err(StoreError::Validation(
                "reason must be purchase, sale, transfer, adjustment, count, return_in or \
                 return_out"
                    .to_owned(),
            )),
        }
    }
}

/// What kind of document caused a movement.
///
/// Deliberately *not* a foreign key on the row: the tables these point at
/// arrive in later items, and a movement must stay readable whatever becomes of
/// the paperwork — the rule a document line has held since B1.06.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MoveRefKind {
    /// A purchase order's receipt (B5.05b).
    PurchaseOrder,
    /// A sales order's delivery (B5.06a).
    SalesOrder,
    /// A stocktake being applied (B5.08b).
    Count,
}

impl MoveRefKind {
    /// The stored word.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PurchaseOrder => "purchase_order",
            Self::SalesOrder => "sales_order",
            Self::Count => "count",
        }
    }

    /// Reads the stored word back.
    ///
    /// # Errors
    /// [`StoreError::Validation`] listing the accepted set.
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim() {
            "purchase_order" => Ok(Self::PurchaseOrder),
            "sales_order" => Ok(Self::SalesOrder),
            "count" => Ok(Self::Count),
            _ => Err(StoreError::Validation(
                "reference kind must be purchase_order, sales_order or count".to_owned(),
            )),
        }
    }
}

/// The document a movement came from. Absent for a movement a person made
/// directly (B5.04b's transfer or adjustment).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveReference {
    /// What kind of document it is.
    pub kind: MoveRefKind,
    /// Its id, as the module that owns it minted it.
    pub id: String,
}

/// The writable shape of a movement. There is no update counterpart, by
/// design: a movement is corrected by another movement.
#[derive(Debug, Clone)]
pub struct NewMove {
    /// What moved. Must be one of this tenant's **stocked** products.
    pub product_id: BillingProductId,
    /// Where it left. Must be one of this tenant's locations, and not the same
    /// as `to_location_id`.
    pub from_location_id: InvLocationId,
    /// Where it arrived.
    pub to_location_id: InvLocationId,
    /// How much, in milli-units. Strictly positive — direction is the pair of
    /// locations.
    pub qty_milli: i64,
    /// Why.
    pub reason: MoveReason,
    /// What a person wrote about it; empty is normal for a document movement.
    pub note: String,
    /// The document that caused it, if any.
    pub reference: Option<MoveReference>,
    /// When it physically happened. `None` means now — the common case;
    /// back-dating is allowed, and validated against the stock there is *now*.
    pub occurred_at: Option<OffsetDateTime>,
}

/// A recorded movement, read back with the names of what moved and where — an
/// id is not an explanation.
#[derive(Debug, Clone)]
pub struct Move {
    /// Opaque id, unique within the tenant.
    pub id: InvMoveId,
    /// What moved.
    pub product_id: BillingProductId,
    /// What it is called, as the catalog calls it today.
    pub product_name: String,
    /// Where it left.
    pub from_location_id: InvLocationId,
    /// That location's code.
    pub from_code: String,
    /// That location's name.
    pub from_name: String,
    /// Where it arrived.
    pub to_location_id: InvLocationId,
    /// That location's code.
    pub to_code: String,
    /// That location's name.
    pub to_name: String,
    /// How much, in milli-units. Always positive.
    pub qty_milli: i64,
    /// Why it moved.
    pub reason: MoveReason,
    /// What a person wrote about it.
    pub note: String,
    /// The document that caused it, if any.
    pub reference: Option<MoveReference>,
    /// When it physically happened.
    pub occurred_at: OffsetDateTime,
    /// Who recorded it.
    pub created_by: String,
    /// When it was recorded, which is not when it happened.
    pub created_at: OffsetDateTime,
}

/// Which slice of the ledger to read. Every field narrows; all of them absent
/// is the tenant's whole history, newest first, capped at [`MOVES_PAGE_MAX`].
#[derive(Debug, Clone, Default)]
pub struct MoveFilter {
    /// One product's history.
    pub product_id: Option<BillingProductId>,
    /// Everything that touched one location, in either direction.
    pub location_id: Option<InvLocationId>,
    /// Movements that happened at or after this instant.
    pub from: Option<OffsetDateTime>,
    /// Movements that happened at or before this instant.
    pub to: Option<OffsetDateTime>,
    /// How many rows at most. Clamped to [`MOVES_PAGE_MAX`]; `None` is the cap.
    pub limit: Option<i64>,
}

/// A validated, normalised movement ready to be bound into a statement.
#[derive(Debug)]
struct Normalized {
    qty_milli: i64,
    note: String,
    ref_kind: &'static str,
    ref_id: String,
}

/// Validates and normalises the parts of a movement that need no database.
/// Pure, so the rules are unit-tested directly; the three that need a door —
/// the product is this tenant's and stocked, both locations are this tenant's,
/// and the stock is actually there — live in [`AccountStore::record_move_in`].
fn normalize(input: &NewMove) -> Result<Normalized> {
    if input.qty_milli <= 0 {
        return Err(StoreError::Validation(
            "quantity must be greater than zero — direction is the pair of locations".to_owned(),
        ));
    }
    if input.qty_milli > QTY_MAX_MILLI {
        return Err(StoreError::Validation(format!(
            "quantity must be at most {QTY_MAX_MILLI} milli-units"
        )));
    }
    if input.from_location_id == input.to_location_id {
        return Err(StoreError::Validation(
            "a movement must have two different locations".to_owned(),
        ));
    }
    let (ref_kind, ref_id) = match input.reference.as_ref() {
        Some(reference) => {
            let id = bounded("reference id", &reference.id, MOVE_REF_ID_MAX_CHARS)?;
            if id.is_empty() {
                return Err(StoreError::Validation(
                    "a reference must name the document it points at".to_owned(),
                ));
            }
            (reference.kind.as_str(), id)
        }
        None => ("", String::new()),
    };
    Ok(Normalized {
        qty_milli: input.qty_milli,
        note: bounded("note", &input.note, MOVE_NOTE_MAX_CHARS)?,
        ref_kind,
        ref_id,
    })
}

/// The refusal a caller sees when the goods are not there, naming everything
/// they need to decide what to do: which product, which place, what is on the
/// shelf and what was asked for.
///
/// Quantities are rendered as the milli-units they are stored in; the edge
/// formats them for a screen, and this message is the one a log would never
/// carry (Law 1: no note, no person, no free text).
fn short_stock(product: &str, location: &Location, available: i64, requested: i64) -> StoreError {
    StoreError::Conflict(format!(
        "{product} at {} has {available} milli-units available, and {requested} were asked for",
        location.code
    ))
}

impl AccountStore {
    /// **The only way a movement is ever recorded**, and the only writer of the
    /// cached balance.
    ///
    /// Validates the movement whole, writes the ledger row and both sides of
    /// the cached balance in **one transaction**, and refuses the whole thing
    /// if it would leave a real location holding less than nothing.
    ///
    /// # Errors
    /// [`StoreError::Validation`] on a non-positive or over-bound quantity, two
    /// identical locations, an over-long note, or a product that is not stocked;
    /// [`StoreError::NotFound`] when the product or either location is not this
    /// tenant's; [`StoreError::Conflict`] when a `stock` or `transit` location
    /// has not got the goods; [`StoreError::Db`] on failure.
    pub async fn record_move(&self, input: &NewMove) -> Result<InvMoveId> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let id = self.record_move_in(&mut tx, input).await?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(id)
    }

    /// [`AccountStore::record_move`], inside a transaction the caller owns.
    ///
    /// The movements and the document that caused them belong in **one**
    /// transaction — receiving a purchase order writes several movements and a
    /// status, and a tenant must never be left holding some of them (B5.05b).
    /// Every rule and every refusal is the public door's; only the `BEGIN` and
    /// the `COMMIT` move to the caller.
    ///
    /// # Errors
    /// Exactly [`AccountStore::record_move`]'s. A caller must **not** catch
    /// them and carry on inside the same transaction: an error here has already
    /// poisoned it, and the only correct next step is to drop it.
    pub(crate) async fn record_move_in(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        input: &NewMove,
    ) -> Result<InvMoveId> {
        let m = normalize(input)?;

        // The product must be this tenant's, and it must be a thing that has a
        // quantity at all. A service has no shelf, and a movement of one is a
        // mistake at the door rather than a zero nobody notices.
        let product: Option<(String, bool)> = sqlx::query_as(
            "SELECT name, stocked FROM billing_products WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(input.product_id.as_str())
        .fetch_optional(&mut **tx)
        .await
        .map_err(StoreError::Db)?;
        let (product_name, stocked) = product.ok_or(StoreError::NotFound)?;
        if !stocked {
            return Err(StoreError::Validation(format!(
                "{product_name} is not a stocked product, so it cannot move"
            )));
        }

        // Both ends must be this tenant's. Asking here — rather than letting
        // the composite foreign key answer with an opaque 23503 — is what lets
        // a guessed id from another tenant read as the NotFound it is, and what
        // gives the negative-stock rule the location's kind and code.
        let from = self
            .require_tenant_location(tx, &input.from_location_id)
            .await?;
        let to = self
            .require_tenant_location(tx, &input.to_location_id)
            .await?;

        let id = InvMoveId::generate();
        let occurred_at = input.occurred_at.unwrap_or_else(OffsetDateTime::now_utc);
        sqlx::query(
            "INSERT INTO inv_moves (tenant_id, id, product_id, from_location_id, \
                 to_location_id, qty_milli, reason, note, ref_kind, ref_id, occurred_at, \
                 created_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(input.product_id.as_str())
        .bind(from.id.as_str())
        .bind(to.id.as_str())
        .bind(m.qty_milli)
        .bind(input.reason.as_str())
        .bind(&m.note)
        .bind(m.ref_kind)
        .bind(&m.ref_id)
        .bind(occurred_at)
        .bind(self.user.as_str())
        .execute(&mut **tx)
        .await
        .map_err(StoreError::Db)?;

        // Both cached rows, in a **fixed order by location id**. The order is
        // not cosmetic: each upsert holds its row's lock until commit, and two
        // concurrent transfers in opposite directions between the same two
        // places would deadlock if each took its locks in its own order.
        let mut sides = [(&from, -m.qty_milli), (&to, m.qty_milli)];
        sides.sort_by(|a, b| a.0.id.as_str().cmp(b.0.id.as_str()));
        let mut left_behind = 0_i64;
        for (location, delta) in sides {
            let balance = self
                .fold_into_cache(tx, &input.product_id, &location.id, delta, occurred_at)
                .await?;
            if location.id == from.id {
                left_behind = balance;
            }
        }

        // **The negative-stock rule**, asked of the place the goods left and
        // only of a place a person could walk into. The virtual counterparties
        // are unbounded by construction — `supplier` going ever more negative
        // is the correct reading of how much has come from outside — and the
        // receiving end can only have gone up.
        if from.kind.is_real() && left_behind < 0 {
            return Err(short_stock(
                &product_name,
                &from,
                left_behind + m.qty_milli,
                m.qty_milli,
            ));
        }
        Ok(id)
    }

    /// Adds one movement's delta to one cached balance and hands back what the
    /// row now says.
    ///
    /// **The lock this takes is the whole concurrency story**: the upsert holds
    /// the row until the transaction commits, so two shipments of the last unit
    /// queue rather than race and exactly one of them is refused.
    async fn fold_into_cache(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        product: &BillingProductId,
        location: &InvLocationId,
        delta: i64,
        occurred_at: OffsetDateTime,
    ) -> Result<i64> {
        sqlx::query_scalar::<_, i64>(
            "INSERT INTO inv_stock (tenant_id, product_id, location_id, qty_milli, \
                 last_move_at) \
             VALUES ($1, $2, $3, $4, $5) \
             ON CONFLICT (tenant_id, product_id, location_id) DO UPDATE \
                 SET qty_milli = inv_stock.qty_milli + EXCLUDED.qty_milli, \
                     last_move_at = GREATEST(inv_stock.last_move_at, EXCLUDED.last_move_at) \
             RETURNING qty_milli",
        )
        .bind(self.tenant.as_str())
        .bind(product.as_str())
        .bind(location.as_str())
        .bind(delta)
        .bind(occurred_at)
        .fetch_one(&mut **tx)
        .await
        .map_err(StoreError::Db)
    }

    /// A slice of the ledger, newest first — what moved, why, and which
    /// document said so.
    ///
    /// A location filter matches **either end**: "what happened at this
    /// warehouse" is one question, not two.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure. An unknown or foreign product or location
    /// id is an empty list, never a refusal that would confirm it exists.
    pub async fn inv_moves(&self, filter: &MoveFilter) -> Result<Vec<Move>> {
        let limit = filter
            .limit
            .unwrap_or(MOVES_PAGE_MAX)
            .clamp(0, MOVES_PAGE_MAX);
        let rows = sqlx::query_as::<_, MoveRow>(&format!(
            "SELECT {MOVE_COLS} {MOVE_JOINS} \
             WHERE m.tenant_id = $1 \
               AND ($2::text IS NULL OR m.product_id = $2) \
               AND ($3::text IS NULL OR m.from_location_id = $3 OR m.to_location_id = $3) \
               AND ($4::timestamptz IS NULL OR m.occurred_at >= $4) \
               AND ($5::timestamptz IS NULL OR m.occurred_at <= $5) \
             ORDER BY m.occurred_at DESC, m.id DESC \
             LIMIT $6"
        ))
        .bind(self.tenant.as_str())
        .bind(filter.product_id.as_ref().map(BillingProductId::as_str))
        .bind(filter.location_id.as_ref().map(InvLocationId::as_str))
        .bind(filter.from)
        .bind(filter.to)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter().map(MoveRow::into_move).collect()
    }

    /// One movement of the tenant, or `None` — including when the id belongs to
    /// another tenant (indistinguishable by design).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn inv_move(&self, id: &InvMoveId) -> Result<Option<Move>> {
        let row = sqlx::query_as::<_, MoveRow>(&format!(
            "SELECT {MOVE_COLS} {MOVE_JOINS} WHERE m.tenant_id = $1 AND m.id = $2"
        ))
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        row.map(MoveRow::into_move).transpose()
    }

    /// Whether this product has ever moved — the question that decides whether
    /// it may stop being stocked ([`crate::billing_products`]).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub(crate) async fn inv_product_has_moves(&self, product: &BillingProductId) -> Result<bool> {
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM inv_moves WHERE tenant_id = $1 AND product_id = $2)",
        )
        .bind(self.tenant.as_str())
        .bind(product.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(StoreError::Db)
    }
}

// ---- row types --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct MoveRow {
    id: String,
    product_id: String,
    product_name: String,
    from_location_id: String,
    from_code: String,
    from_name: String,
    to_location_id: String,
    to_code: String,
    to_name: String,
    qty_milli: i64,
    reason: String,
    note: String,
    ref_kind: String,
    ref_id: String,
    occurred_at: OffsetDateTime,
    created_by: String,
    created_at: OffsetDateTime,
}

impl MoveRow {
    fn into_move(self) -> Result<Move> {
        Ok(Move {
            id: InvMoveId::new(self.id),
            product_id: BillingProductId::new(self.product_id),
            product_name: self.product_name,
            from_location_id: InvLocationId::new(self.from_location_id),
            from_code: self.from_code,
            from_name: self.from_name,
            to_location_id: InvLocationId::new(self.to_location_id),
            to_code: self.to_code,
            to_name: self.to_name,
            qty_milli: self.qty_milli,
            reason: MoveReason::parse(&self.reason)?,
            note: self.note,
            reference: match self.ref_kind.as_str() {
                "" => None,
                kind => Some(MoveReference {
                    kind: MoveRefKind::parse(kind)?,
                    id: self.ref_id,
                }),
            },
            occurred_at: self.occurred_at,
            created_by: self.created_by,
            created_at: self.created_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inv_locations::LocationKind;

    fn parcel() -> NewMove {
        NewMove {
            product_id: BillingProductId::new("p1".to_owned()),
            from_location_id: InvLocationId::new("l1".to_owned()),
            to_location_id: InvLocationId::new("l2".to_owned()),
            qty_milli: 1_000,
            reason: MoveReason::Transfer,
            note: String::new(),
            reference: None,
            occurred_at: None,
        }
    }

    fn invalid<T: std::fmt::Debug>(result: Result<T>) -> String {
        match result {
            Err(StoreError::Validation(msg)) => msg,
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn the_reason_vocabulary_round_trips_and_refuses_anything_else() {
        for reason in [
            MoveReason::Purchase,
            MoveReason::Sale,
            MoveReason::Transfer,
            MoveReason::Adjustment,
            MoveReason::Count,
            MoveReason::ReturnIn,
            MoveReason::ReturnOut,
        ] {
            assert_eq!(
                MoveReason::parse(reason.as_str()).unwrap_or(MoveReason::Count),
                reason
            );
        }
        for bad in ["", "shrinkage", "PURCHASE", "returnin"] {
            assert!(invalid(MoveReason::parse(bad)).contains("reason must be"));
        }
    }

    #[test]
    fn the_reference_vocabulary_round_trips_and_refuses_anything_else() {
        for kind in [
            MoveRefKind::PurchaseOrder,
            MoveRefKind::SalesOrder,
            MoveRefKind::Count,
        ] {
            assert_eq!(
                MoveRefKind::parse(kind.as_str()).unwrap_or(MoveRefKind::Count),
                kind
            );
        }
        for bad in ["", "invoice", "po"] {
            assert!(invalid(MoveRefKind::parse(bad)).contains("reference kind"));
        }
    }

    #[test]
    fn a_quantity_is_strictly_positive_and_bounded() {
        for bad in [0, -1, -1_000, i64::MIN] {
            let input = NewMove {
                qty_milli: bad,
                ..parcel()
            };
            assert!(
                invalid(normalize(&input)).contains("greater than zero"),
                "expected rejection: {bad}"
            );
        }
        for bad in [QTY_MAX_MILLI + 1, i64::MAX] {
            let input = NewMove {
                qty_milli: bad,
                ..parcel()
            };
            assert!(invalid(normalize(&input)).contains("at most"));
        }
        for ok in [1, 1_000, QTY_MAX_MILLI] {
            let input = NewMove {
                qty_milli: ok,
                ..parcel()
            };
            assert_eq!(
                normalize(&input)
                    .unwrap_or_else(|e| panic!("rejected: {e}"))
                    .qty_milli,
                ok
            );
        }
    }

    #[test]
    fn a_movement_needs_two_different_places() {
        let input = NewMove {
            to_location_id: InvLocationId::new("l1".to_owned()),
            ..parcel()
        };
        assert!(invalid(normalize(&input)).contains("two different locations"));
    }

    #[test]
    fn a_reference_is_optional_and_must_name_a_document() {
        assert_eq!(
            normalize(&parcel())
                .unwrap_or_else(|e| panic!("rejected: {e}"))
                .ref_kind,
            "",
            "a manual movement points at nothing"
        );
        let ok = normalize(&NewMove {
            reference: Some(MoveReference {
                kind: MoveRefKind::PurchaseOrder,
                id: " po-7 ".to_owned(),
            }),
            ..parcel()
        })
        .unwrap_or_else(|e| panic!("rejected: {e}"));
        assert_eq!(ok.ref_kind, "purchase_order");
        assert_eq!(ok.ref_id, "po-7");

        let blank = NewMove {
            reference: Some(MoveReference {
                kind: MoveRefKind::Count,
                id: "   ".to_owned(),
            }),
            ..parcel()
        };
        assert!(invalid(normalize(&blank)).contains("name the document"));
    }

    #[test]
    fn a_note_is_bounded() {
        let input = NewMove {
            note: "x".repeat(MOVE_NOTE_MAX_CHARS + 1),
            ..parcel()
        };
        assert!(invalid(normalize(&input)).contains("at most"));
    }

    #[test]
    fn the_short_stock_refusal_names_what_a_person_needs_and_no_more() {
        let location = Location {
            id: InvLocationId::new("l1".to_owned()),
            code: "WH1".to_owned(),
            name: "Hoofdmagazijn".to_owned(),
            kind: LocationKind::Stock,
            archived_at: None,
            created_by: "u".to_owned(),
            created_at: OffsetDateTime::now_utc(),
            updated_at: OffsetDateTime::now_utc(),
        };
        let StoreError::Conflict(message) = short_stock("Blue chair", &location, 2_000, 5_000)
        else {
            panic!("expected a Conflict");
        };
        assert!(message.contains("Blue chair"));
        assert!(message.contains("WH1"));
        assert!(message.contains("2000"), "{message}");
        assert!(message.contains("5000"), "{message}");
    }
}
