//! Reorder rules and the shortage query (alo Inventory, ADR 0035, wave B5.07;
//! `docs/design/inventory.md`, "Reorder rules and the shortage query").
//!
//! A rule is a standing instruction — *keep at least this much of that product
//! at this place, and when you fall under, bring it back up to that* — and the
//! shortage query is the fold that turns every rule a tenant holds into the
//! question a buyer actually asks on a Monday morning: **what do I need to buy,
//! how much, and from whom.**
//!
//! The arithmetic is four numbers per rule and nothing else:
//!
//! ```text
//! available = on_hand + on_order − committed
//! short when available < minimum
//! buy = max(target − available, the supplier's minimum order quantity)
//! ```
//!
//! Three decisions are worth stating here rather than leaving to be inferred.
//!
//! - **`on_order` and `committed` are computed, never stored.** `on_order` is
//!   the open remainder of every purchase-order line on a placed order;
//!   `committed` is the undelivered remainder of every confirmed sales-order
//!   line. The rejected alternative for the second is a reservation table — a
//!   row per sales-order line holding stock aside. Real systems have one, and
//!   the reason is real: with reservations you can answer "is *this* unit spoken
//!   for". We do not build it because it is a second thing that must be kept in
//!   step with the orders, and the question it answers is not the one an SME
//!   asks. They ask "do I need to buy more", and a fold over open order lines
//!   answers exactly that from data that cannot disagree with itself.
//! - **`on_order` is what makes the report readable rather than annoying.**
//!   Without it, a shortage that was ordered on Tuesday is reported again every
//!   morning until the lorry arrives, and a report that repeats itself is a
//!   report people stop reading.
//! - **The pipeline numbers are per product, not per location** — because
//!   neither document names a location until the goods are actually received or
//!   picked, and attributing an open order to a shelf would be a guess dressed
//!   as a fact. Each row therefore states `on_hand`, `on_order` and `committed`
//!   separately, so a reader can see which part of the answer came from where.
//!   With one stock location — the seeded case, and the overwhelmingly common
//!   one — the two readings coincide exactly.
//!
//! Quantities are milli-units and money is integer cents, as everywhere else in
//! the suite. No float touches this module.

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::{BillingProductId, InvLocationId, InvReorderRuleId, InvSupplierId};
use crate::inv_locations::LocationKind;
use crate::inv_stock::stock_value_cents;

/// The largest quantity a rule may state, in milli-units: a million units.
///
/// Deliberately the bound a **purchase-order line's** quantity has (B5.05a)
/// rather than the larger one a supplier's minimum order may state: what a rule
/// exists to produce is a proposed order line, so a target that could not fit on
/// one would be a number this module can compute and never act on.
pub const REORDER_QTY_MAX_MILLI: i64 = 1_000_000_000;

/// The columns every read of a rule selects, in `RuleRow` order. The product and
/// location names come from their own tables — a rule is unreadable without
/// them, and one statement beats one read per row.
const RULE_COLS: &str = "r.id, r.product_id, p.name AS product_name, p.sku, p.unit, \
     r.location_id, l.code AS location_code, l.name AS location_name, \
     r.min_qty_milli, r.target_qty_milli, r.active, \
     r.created_by, r.created_at, r.updated_at";

/// The writable shape of a rule: which pair it watches, and the two numbers.
#[derive(Debug, Clone)]
pub struct NewReorderRule {
    /// The product to watch. Must be one of this tenant's, and **stocked** — a
    /// service has no on-hand to be under.
    pub product_id: BillingProductId,
    /// The place to watch it. Must be one of this tenant's, and a real `stock`
    /// location — a minimum on the `supplier` counterparty is a minimum on a
    /// number that is negative by construction.
    pub location_id: InvLocationId,
    /// At or below this, in milli-units, the product is short.
    pub min_qty_milli: i64,
    /// What to bring it back up to, in milli-units. Never below
    /// [`Self::min_qty_milli`].
    pub target_qty_milli: i64,
    /// Whether the rule produces shortages at all. `false` parks it with its
    /// numbers intact — what a seasonal product needs out of season.
    pub active: bool,
}

/// The parts of a rule that can be changed after it is written.
///
/// The pair is **not** among them: a rule about a different product at a
/// different shelf is a different rule, and editing the pair in place would
/// silently rewrite what a screen was looking at. Change one by deleting it and
/// writing the other.
#[derive(Debug, Clone)]
pub struct ReorderLimits {
    /// The new minimum, in milli-units.
    pub min_qty_milli: i64,
    /// The new target, in milli-units.
    pub target_qty_milli: i64,
    /// Whether the rule is watched.
    pub active: bool,
}

/// One stored rule, with enough of its ends to be read on a screen.
#[derive(Debug, Clone)]
pub struct ReorderRule {
    /// Opaque id, unique within the tenant.
    pub id: InvReorderRuleId,
    /// The product watched.
    pub product_id: BillingProductId,
    /// Its name in the catalog today.
    pub product_name: String,
    /// Its SKU; empty when the tenant has not given it one.
    pub sku: String,
    /// Its unit label; empty for a unitless item.
    pub unit: String,
    /// The place watched.
    pub location_id: InvLocationId,
    /// That location's code.
    pub location_code: String,
    /// That location's name.
    pub location_name: String,
    /// At or below this, in milli-units, the product is short.
    pub min_qty_milli: i64,
    /// What a purchase brings it back up to, in milli-units.
    pub target_qty_milli: i64,
    /// Whether the rule produces shortages.
    pub active: bool,
    /// The user who wrote the rule.
    pub created_by: String,
    /// When it was written.
    pub created_at: OffsetDateTime,
    /// When it was last changed.
    pub updated_at: OffsetDateTime,
}

/// Which rules to read.
#[derive(Debug, Clone, Default)]
pub struct ReorderRuleFilter {
    /// One product across every place it is watched.
    pub product_id: Option<BillingProductId>,
    /// One place across every product watched there.
    pub location_id: Option<InvLocationId>,
    /// Whether to include the parked rules. Off by default: a rules screen
    /// answers "what are we watching".
    pub include_inactive: bool,
}

/// Who we would buy the shortage from, and on what terms — the offer
/// ([`crate::inv_supplier_prices`]) that makes a proposal possible.
#[derive(Debug, Clone)]
pub struct ShortageSupplier {
    /// The supplier.
    pub supplier_id: InvSupplierId,
    /// Their name.
    pub supplier_name: String,
    /// Their article code for our product; empty when they use ours.
    pub supplier_code: String,
    /// What they charge for one unit, in integer cents of [`Self::currency`].
    pub purchase_price_cents: i64,
    /// ISO 4217 code the offer is quoted in.
    pub currency: String,
    /// The smallest quantity they will sell, in milli-units.
    pub min_order_qty_milli: i64,
    /// How long the goods take to arrive: the offer's own lead time when it
    /// states one, otherwise the supplier's default. Resolved here so no screen
    /// re-implements the fallback.
    pub lead_time_days: i32,
}

/// One rule that has come true: what is short, by how much, how much to buy and
/// from whom.
#[derive(Debug, Clone)]
pub struct Shortage {
    /// The rule that produced the row.
    pub rule_id: InvReorderRuleId,
    /// The product that is short.
    pub product_id: BillingProductId,
    /// Its name in the catalog today.
    pub product_name: String,
    /// Its SKU; empty when it has none.
    pub sku: String,
    /// Its unit label; empty for a unitless item.
    pub unit: String,
    /// Where it is short.
    pub location_id: InvLocationId,
    /// That location's code.
    pub location_code: String,
    /// That location's name.
    pub location_name: String,
    /// The rule's minimum, in milli-units.
    pub min_qty_milli: i64,
    /// The rule's target, in milli-units.
    pub target_qty_milli: i64,
    /// What is on that shelf now, in milli-units.
    pub on_hand_qty_milli: i64,
    /// The open remainder of every placed purchase order for the product,
    /// tenant-wide, in milli-units.
    pub on_order_qty_milli: i64,
    /// The undelivered remainder of every confirmed sales order for the
    /// product, tenant-wide, in milli-units.
    pub committed_qty_milli: i64,
    /// `on_hand + on_order − committed`, in milli-units.
    pub available_qty_milli: i64,
    /// How far under the minimum that leaves it — always positive on a row that
    /// is here at all.
    pub short_by_qty_milli: i64,
    /// What to buy, in milli-units: enough to reach the target, and never less
    /// than the supplier will sell.
    pub buy_qty_milli: i64,
    /// The supplier to buy it from, when one has quoted us for it.
    pub supplier: Option<ShortageSupplier>,
    /// What [`Self::buy_qty_milli`] would cost, in integer cents — at the
    /// supplier's quoted price when there is an offer, otherwise at the
    /// product's own recorded purchase price, which is the number
    /// [`crate::inv_stock`] values the shelf by. Zero when neither is known.
    pub estimated_cost_cents: i64,
}

/// Which shortages to read.
#[derive(Debug, Clone, Default)]
pub struct ShortageFilter {
    /// One place — the question a single warehouse's buyer asks.
    pub location_id: Option<InvLocationId>,
    /// One product, across every place it is watched.
    pub product_id: Option<BillingProductId>,
    /// Only what this supplier sells us — the slice a proposal for one order
    /// needs (B5.10).
    pub supplier_id: Option<InvSupplierId>,
}

/// What is actually available to satisfy a minimum: what is on the shelf, plus
/// what has been ordered and not yet arrived, minus what has been promised and
/// not yet gone out.
///
/// Saturating, so a tenant cannot arrange for an overflow by stating enormous
/// quantities on enough open documents.
#[must_use]
pub fn available_qty_milli(on_hand: i64, on_order: i64, committed: i64) -> i64 {
    on_hand.saturating_add(on_order).saturating_sub(committed)
}

/// How much to buy to bring `available` back to `target`, never less than the
/// smallest quantity the supplier will sell.
///
/// **`min_order_qty_milli` is a floor, not a pack size.** The field says the
/// smallest quantity they will sell, so a need of 12 against a minimum of 50
/// buys 50 — and a need of 60 buys 60, not 100. Rounding up to a *multiple*
/// would be a pack size, which is a different fact the supplier has not told us.
///
/// Zero when nothing is needed, so this is safe to call on a row that turns out
/// not to be short.
#[must_use]
pub fn buy_qty_milli(target: i64, available: i64, min_order_qty_milli: i64) -> i64 {
    let need = target.saturating_sub(available);
    if need <= 0 {
        return 0;
    }
    need.max(min_order_qty_milli.max(0))
}

/// Validates the two numbers of a rule. Pure — no database, so the rules are
/// unit-tested directly.
fn check_limits(min_qty_milli: i64, target_qty_milli: i64) -> Result<()> {
    if !(0..=REORDER_QTY_MAX_MILLI).contains(&min_qty_milli) {
        return Err(StoreError::Validation(format!(
            "the minimum quantity must be between 0 and {REORDER_QTY_MAX_MILLI} milli-units"
        )));
    }
    if !(0..=REORDER_QTY_MAX_MILLI).contains(&target_qty_milli) {
        return Err(StoreError::Validation(format!(
            "the target quantity must be between 0 and {REORDER_QTY_MAX_MILLI} milli-units"
        )));
    }
    if target_qty_milli < min_qty_milli {
        return Err(StoreError::Validation(
            "the target quantity cannot be below the minimum quantity".to_owned(),
        ));
    }
    Ok(())
}

/// Turns the one unique violation this module can produce into the refusal a
/// caller could have predicted, rather than a `500`.
fn map_rule_conflict(error: sqlx::Error) -> StoreError {
    match error {
        sqlx::Error::Database(ref db) if db.code().as_deref() == Some("23505") => {
            StoreError::Conflict("this product is already watched at this location".to_owned())
        }
        other => StoreError::Db(other),
    }
}

/// The open remainder of every **placed** purchase-order line, per product.
///
/// `sent` and `partially_received` are the placed-and-unfinished states; a draft
/// is not on order (nobody has been told), and `received`/`cancelled` have
/// nothing left to arrive. Free-text lines carry no product and a negative
/// quantity is a supplier's discount, so both fall out of the fold.
const ON_ORDER_SQL: &str = "SELECT pol.product_id, \
     SUM(GREATEST(pol.qty_milli, 0) - pol.received_qty_milli)::bigint AS qty_milli \
     FROM inv_purchase_order_lines pol \
     JOIN inv_purchase_orders po ON po.tenant_id = pol.tenant_id AND po.id = pol.po_id \
     WHERE pol.tenant_id = $1 AND pol.product_id IS NOT NULL \
       AND po.status IN ('sent', 'partially_received') \
     GROUP BY pol.product_id";

/// The undelivered remainder of every **confirmed** sales-order line, per
/// product. A draft order has promised nobody anything; a delivered or
/// cancelled one has nothing left to go out.
const COMMITTED_SQL: &str = "SELECT sol.product_id, \
     SUM(GREATEST(sol.qty_milli, 0) - sol.delivered_qty_milli)::bigint AS qty_milli \
     FROM inv_sales_order_lines sol \
     JOIN inv_sales_orders so ON so.tenant_id = sol.tenant_id AND so.id = sol.so_id \
     WHERE sol.tenant_id = $1 AND sol.product_id IS NOT NULL \
       AND so.status IN ('confirmed', 'partially_delivered') \
     GROUP BY sol.product_id";

/// The one offer a proposal would be written against: the tenant's default
/// supplier for the product when that supplier actually quotes for it, else the
/// first supplier who did. Archived suppliers are never proposed.
const OFFER_SQL: &str = "SELECT sp.supplier_id, sup.name AS supplier_name, sp.supplier_code, \
         sp.purchase_price_cents, sp.currency, sp.min_order_qty_milli, \
         COALESCE(sp.lead_time_days, sup.lead_time_days) AS lead_time_days \
     FROM inv_supplier_products sp \
     JOIN inv_suppliers sup ON sup.tenant_id = sp.tenant_id AND sup.id = sp.supplier_id \
     WHERE sp.tenant_id = r.tenant_id AND sp.product_id = r.product_id \
       AND sup.archived_at IS NULL \
     ORDER BY COALESCE(sp.supplier_id = p.default_supplier_id, FALSE) DESC, \
              sp.created_at, sp.supplier_id \
     LIMIT 1";

impl AccountStore {
    /// Writes a standing instruction to keep a product stocked at a place.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the product or the location is not this
    /// tenant's; [`StoreError::Validation`] on a service product, a location
    /// that is not a real shelf, or numbers out of range;
    /// [`StoreError::Conflict`] when the pair is already watched or the location
    /// is archived; [`StoreError::Db`] on failure.
    pub async fn create_inv_reorder_rule(
        &self,
        input: &NewReorderRule,
    ) -> Result<InvReorderRuleId> {
        check_limits(input.min_qty_milli, input.target_qty_milli)?;
        self.check_reorder_ends(&input.product_id, &input.location_id)
            .await?;
        let id = InvReorderRuleId::generate();
        sqlx::query(
            "INSERT INTO inv_reorder_rules (tenant_id, id, product_id, location_id, \
                 min_qty_milli, target_qty_milli, active, created_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(input.product_id.as_str())
        .bind(input.location_id.as_str())
        .bind(input.min_qty_milli)
        .bind(input.target_qty_milli)
        .bind(input.active)
        .bind(self.user.as_str())
        .execute(&self.pool)
        .await
        .map_err(map_rule_conflict)?;
        Ok(id)
    }

    /// Changes a rule's numbers, or parks it.
    ///
    /// The pair it watches is deliberately not changeable: see
    /// [`ReorderLimits`].
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the rule is not this tenant's;
    /// [`StoreError::Validation`] on numbers out of range; [`StoreError::Db`] on
    /// failure.
    pub async fn update_inv_reorder_rule(
        &self,
        id: &InvReorderRuleId,
        limits: &ReorderLimits,
    ) -> Result<()> {
        check_limits(limits.min_qty_milli, limits.target_qty_milli)?;
        let done = sqlx::query(
            "UPDATE inv_reorder_rules SET min_qty_milli = $3, target_qty_milli = $4, \
                 active = $5, updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(limits.min_qty_milli)
        .bind(limits.target_qty_milli)
        .bind(limits.active)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Stops watching a pair altogether.
    ///
    /// Deleted rather than archived, and safe in a way deleting a location is
    /// not: a rule explains nothing that happened, and no document copied
    /// anything from it. A tenant who wants the numbers kept parks the rule
    /// instead.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the rule is not this tenant's;
    /// [`StoreError::Db`] on failure.
    pub async fn delete_inv_reorder_rule(&self, id: &InvReorderRuleId) -> Result<()> {
        let done = sqlx::query("DELETE FROM inv_reorder_rules WHERE tenant_id = $1 AND id = $2")
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .execute(&self.pool)
            .await
            .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// One rule by id, or `None` when it is not this tenant's — never a refusal
    /// that would confirm another tenant's row exists.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn inv_reorder_rule(&self, id: &InvReorderRuleId) -> Result<Option<ReorderRule>> {
        let row = sqlx::query_as::<_, RuleRow>(&format!(
            "SELECT {RULE_COLS} FROM inv_reorder_rules r \
             JOIN billing_products p ON p.tenant_id = r.tenant_id AND p.id = r.product_id \
             JOIN inv_locations l ON l.tenant_id = r.tenant_id AND l.id = r.location_id \
             WHERE r.tenant_id = $1 AND r.id = $2"
        ))
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(row.map(RuleRow::into_rule))
    }

    /// The tenant's rules, in product-name then location-code order — so the
    /// same tenant's screen reads the same way twice.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure. An unknown or foreign product or location
    /// id narrows to an empty list, never a refusal that would confirm it
    /// exists.
    pub async fn inv_reorder_rules(&self, filter: &ReorderRuleFilter) -> Result<Vec<ReorderRule>> {
        let rows = sqlx::query_as::<_, RuleRow>(&format!(
            "SELECT {RULE_COLS} FROM inv_reorder_rules r \
             JOIN billing_products p ON p.tenant_id = r.tenant_id AND p.id = r.product_id \
             JOIN inv_locations l ON l.tenant_id = r.tenant_id AND l.id = r.location_id \
             WHERE r.tenant_id = $1 \
               AND ($2::text IS NULL OR r.product_id = $2) \
               AND ($3::text IS NULL OR r.location_id = $3) \
               AND ($4 OR r.active) \
             ORDER BY lower(p.name), p.id, l.code"
        ))
        .bind(self.tenant.as_str())
        .bind(filter.product_id.as_ref().map(BillingProductId::as_str))
        .bind(filter.location_id.as_ref().map(InvLocationId::as_str))
        .bind(filter.include_inactive)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(RuleRow::into_rule).collect())
    }

    /// **What needs buying** — every active rule whose available quantity has
    /// fallen under its minimum, with how much to buy and from whom.
    ///
    /// Rules on an archived product or an archived location are silently out:
    /// a shelf we are emptying on purpose is not a shortage, and reporting it
    /// every morning is how a report loses its reader.
    ///
    /// Ordered by product name then location code, like every other inventory
    /// read.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn inv_shortages(&self, filter: &ShortageFilter) -> Result<Vec<Shortage>> {
        let rows = sqlx::query_as::<_, ShortageRow>(&format!(
            "SELECT r.id, r.product_id, p.name AS product_name, p.sku, p.unit, \
                 p.purchase_price_cents, \
                 r.location_id, l.code AS location_code, l.name AS location_name, \
                 r.min_qty_milli, r.target_qty_milli, \
                 COALESCE(s.qty_milli, 0) AS on_hand_qty_milli, \
                 COALESCE(oo.qty_milli, 0) AS on_order_qty_milli, \
                 COALESCE(cm.qty_milli, 0) AS committed_qty_milli, \
                 o.supplier_id, o.supplier_name, o.supplier_code, \
                 o.purchase_price_cents AS offer_price_cents, o.currency, \
                 o.min_order_qty_milli, o.lead_time_days \
             FROM inv_reorder_rules r \
             JOIN billing_products p ON p.tenant_id = r.tenant_id AND p.id = r.product_id \
             JOIN inv_locations l ON l.tenant_id = r.tenant_id AND l.id = r.location_id \
             LEFT JOIN inv_stock s ON s.tenant_id = r.tenant_id \
                 AND s.product_id = r.product_id AND s.location_id = r.location_id \
             LEFT JOIN ({ON_ORDER_SQL}) oo ON oo.product_id = r.product_id \
             LEFT JOIN ({COMMITTED_SQL}) cm ON cm.product_id = r.product_id \
             LEFT JOIN LATERAL ({OFFER_SQL}) o ON TRUE \
             WHERE r.tenant_id = $1 AND r.active \
               AND p.archived_at IS NULL AND l.archived_at IS NULL \
               AND COALESCE(s.qty_milli, 0) + COALESCE(oo.qty_milli, 0) \
                   - COALESCE(cm.qty_milli, 0) < r.min_qty_milli \
               AND ($2::text IS NULL OR r.location_id = $2) \
               AND ($3::text IS NULL OR r.product_id = $3) \
               AND ($4::text IS NULL OR o.supplier_id = $4) \
             ORDER BY lower(p.name), p.id, l.code"
        ))
        .bind(self.tenant.as_str())
        .bind(filter.location_id.as_ref().map(InvLocationId::as_str))
        .bind(filter.product_id.as_ref().map(BillingProductId::as_str))
        .bind(filter.supplier_id.as_ref().map(InvSupplierId::as_str))
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(ShortageRow::into_shortage).collect())
    }

    /// Checks that both ends of a rule are this tenant's and can carry one.
    ///
    /// A guessed id from another tenant is a [`StoreError::NotFound`], the same
    /// answer as an id that never existed; a real id that cannot carry a rule
    /// says exactly why.
    async fn check_reorder_ends(
        &self,
        product_id: &BillingProductId,
        location_id: &InvLocationId,
    ) -> Result<()> {
        let product = self
            .billing_product(product_id)
            .await?
            .ok_or(StoreError::NotFound)?;
        if !product.stocked {
            return Err(StoreError::Validation(
                "a reorder rule needs a stocked product; this one is a service".to_owned(),
            ));
        }
        if product.is_archived() {
            return Err(StoreError::Conflict(
                "an archived product cannot be watched".to_owned(),
            ));
        }
        let location = self
            .inv_location(location_id)
            .await?
            .ok_or(StoreError::NotFound)?;
        if location.kind != LocationKind::Stock {
            return Err(StoreError::Validation(
                "a reorder rule needs a real stock location".to_owned(),
            ));
        }
        if location.is_archived() {
            return Err(StoreError::Conflict(
                "an archived location cannot be watched".to_owned(),
            ));
        }
        Ok(())
    }
}

// ---- row types --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct RuleRow {
    id: String,
    product_id: String,
    product_name: String,
    sku: String,
    unit: String,
    location_id: String,
    location_code: String,
    location_name: String,
    min_qty_milli: i64,
    target_qty_milli: i64,
    active: bool,
    created_by: String,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl RuleRow {
    fn into_rule(self) -> ReorderRule {
        ReorderRule {
            id: InvReorderRuleId::new(self.id),
            product_id: BillingProductId::new(self.product_id),
            product_name: self.product_name,
            sku: self.sku,
            unit: self.unit,
            location_id: InvLocationId::new(self.location_id),
            location_code: self.location_code,
            location_name: self.location_name,
            min_qty_milli: self.min_qty_milli,
            target_qty_milli: self.target_qty_milli,
            active: self.active,
            created_by: self.created_by,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct ShortageRow {
    id: String,
    product_id: String,
    product_name: String,
    sku: String,
    unit: String,
    purchase_price_cents: i64,
    location_id: String,
    location_code: String,
    location_name: String,
    min_qty_milli: i64,
    target_qty_milli: i64,
    on_hand_qty_milli: i64,
    on_order_qty_milli: i64,
    committed_qty_milli: i64,
    supplier_id: Option<String>,
    supplier_name: Option<String>,
    supplier_code: Option<String>,
    offer_price_cents: Option<i64>,
    currency: Option<String>,
    min_order_qty_milli: Option<i64>,
    lead_time_days: Option<i32>,
}

impl ShortageRow {
    fn into_shortage(self) -> Shortage {
        let available = available_qty_milli(
            self.on_hand_qty_milli,
            self.on_order_qty_milli,
            self.committed_qty_milli,
        );
        let buy = buy_qty_milli(
            self.target_qty_milli,
            available,
            self.min_order_qty_milli.unwrap_or(0),
        );
        // The offer's price when there is one, else what the catalog says the
        // product costs us — the same number `inv_stock` values the shelf by, so
        // "what is there" and "what the gap costs" are quoted consistently.
        let price = self.offer_price_cents.unwrap_or(self.purchase_price_cents);
        let supplier = match (self.supplier_id, self.supplier_name) {
            (Some(id), Some(name)) => Some(ShortageSupplier {
                supplier_id: InvSupplierId::new(id),
                supplier_name: name,
                supplier_code: self.supplier_code.unwrap_or_default(),
                purchase_price_cents: self.offer_price_cents.unwrap_or(0),
                currency: self.currency.unwrap_or_default(),
                min_order_qty_milli: self.min_order_qty_milli.unwrap_or(0),
                lead_time_days: self.lead_time_days.unwrap_or(0),
            }),
            _ => None,
        };
        Shortage {
            rule_id: InvReorderRuleId::new(self.id),
            product_id: BillingProductId::new(self.product_id),
            product_name: self.product_name,
            sku: self.sku,
            unit: self.unit,
            location_id: InvLocationId::new(self.location_id),
            location_code: self.location_code,
            location_name: self.location_name,
            min_qty_milli: self.min_qty_milli,
            target_qty_milli: self.target_qty_milli,
            on_hand_qty_milli: self.on_hand_qty_milli,
            on_order_qty_milli: self.on_order_qty_milli,
            committed_qty_milli: self.committed_qty_milli,
            available_qty_milli: available,
            short_by_qty_milli: self.min_qty_milli.saturating_sub(available),
            buy_qty_milli: buy,
            supplier,
            estimated_cost_cents: stock_value_cents(buy, price),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn invalid<T: std::fmt::Debug>(result: Result<T>) -> String {
        match result {
            Err(StoreError::Validation(msg)) => msg,
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn available_is_the_shelf_plus_what_is_coming_minus_what_is_promised() {
        // 4 on the shelf, 10 ordered, 6 promised → 8 available.
        assert_eq!(available_qty_milli(4_000, 10_000, 6_000), 8_000);
        // Nothing anywhere is nothing.
        assert_eq!(available_qty_milli(0, 0, 0), 0);
        // More promised than exists is legitimately negative: it is exactly the
        // state that makes the report say "buy some".
        assert_eq!(available_qty_milli(1_000, 0, 5_000), -4_000);
    }

    #[test]
    fn available_saturates_rather_than_overflowing() {
        assert_eq!(available_qty_milli(i64::MAX, i64::MAX, 0), i64::MAX);
        assert_eq!(available_qty_milli(i64::MIN, 0, i64::MAX), i64::MIN);
    }

    #[test]
    fn the_quantity_to_buy_closes_the_gap_to_the_target() {
        // Target 20, available 8 → buy 12, no minimum in the way.
        assert_eq!(buy_qty_milli(20_000, 8_000, 0), 12_000);
        // A negative availability is part of the gap, not ignored.
        assert_eq!(buy_qty_milli(20_000, -4_000, 0), 24_000);
    }

    #[test]
    fn the_suppliers_minimum_is_a_floor_and_never_a_pack_size() {
        // Need 12, they will not sell under 50 → buy 50.
        assert_eq!(buy_qty_milli(20_000, 8_000, 50_000), 50_000);
        // Need 60 against the same minimum → buy 60, NOT rounded up to 100.
        // Their minimum says the smallest they will sell, not a multiple.
        assert_eq!(buy_qty_milli(68_000, 8_000, 50_000), 60_000);
        // Exactly the minimum stays exactly the minimum.
        assert_eq!(buy_qty_milli(58_000, 8_000, 50_000), 50_000);
    }

    #[test]
    fn nothing_needed_buys_nothing_whatever_the_minimum_order_is() {
        assert_eq!(buy_qty_milli(20_000, 20_000, 0), 0);
        assert_eq!(buy_qty_milli(20_000, 25_000, 50_000), 0);
        // A nonsense negative minimum cannot invent a purchase either.
        assert_eq!(buy_qty_milli(20_000, 20_000, -5), 0);
        assert_eq!(buy_qty_milli(20_000, 8_000, -5), 12_000);
    }

    #[test]
    fn the_limits_are_bounded_and_the_target_never_sits_under_the_minimum() {
        assert!(check_limits(0, 0).is_ok());
        assert!(
            check_limits(10_000, 10_000).is_ok(),
            "equal is 'buy back to the minimum'"
        );
        assert!(check_limits(0, REORDER_QTY_MAX_MILLI).is_ok());
        assert!(
            invalid(check_limits(-1, 10)).contains("minimum quantity"),
            "a negative minimum names the field it is about"
        );
        assert!(invalid(check_limits(0, -1)).contains("target quantity"));
        assert!(
            invalid(check_limits(
                REORDER_QTY_MAX_MILLI + 1,
                REORDER_QTY_MAX_MILLI + 1
            ))
            .contains("minimum quantity")
        );
        assert!(invalid(check_limits(0, REORDER_QTY_MAX_MILLI + 1)).contains("target quantity"));
        assert!(
            invalid(check_limits(20_000, 10_000)).contains("below the minimum"),
            "a target under the minimum proposes a purchase that arrives short"
        );
    }

    #[test]
    fn the_rule_cap_is_the_one_a_purchase_order_line_can_carry() {
        // A rule exists to produce an order line; a target that could not fit on
        // one would be a number we can compute and never act on.
        assert_eq!(REORDER_QTY_MAX_MILLI, 1_000_000_000);
    }

    #[test]
    fn the_default_rule_filter_is_what_are_we_watching() {
        let d = ReorderRuleFilter::default();
        assert!(!d.include_inactive, "a parked rule is not being watched");
        assert!(d.product_id.is_none() && d.location_id.is_none());
    }
}
