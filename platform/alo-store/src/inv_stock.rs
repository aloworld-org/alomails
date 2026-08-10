//! On-hand — the reads over what [`crate::inv_moves`] wrote (alo Inventory,
//! ADR 0035, wave B5.04a; `docs/design/inventory.md`, "The cached balance").
//!
//! Three reads and nothing else, because this file owns no rule: what is where
//! ([`AccountStore::inv_stock`]), how much of one thing is at one place
//! ([`AccountStore::inv_on_hand`]), and what the ledger says the answer *should*
//! be ([`AccountStore::inv_stock_folded`]) — the recomputation that makes the
//! cache trustworthy rather than merely fast. Nothing here writes the cache;
//! [`AccountStore::record_move`] is its single writer, and the one exception is
//! [`AccountStore::inv_stock_rebuild`], which exists so the cache is disposable
//! and is called by no route.
//!
//! **The value of stock is computed here, in integer cents, from the purchase
//! price.** Quantity is in milli-units, so a value is `qty × price ÷ 1000`
//! rounded once, half away from zero — [`crate::billing_totals`]' convention,
//! reused rather than restated, because a warehouse valued by one rounding rule
//! and a document totalled by another disagree by a cent at exactly the moment
//! somebody is reconciling them. Purchase price, not sale price: stock on hand
//! is what it cost us, which is also what a balance sheet means by it.
//!
//! A stocked product with no movements has **no row** — neither here nor in the
//! cache — and reads as zero. That is deliberate: a table with a row per
//! product per location for every tenant is mostly zeros, and "we have none"
//! and "we have never had any" are the same answer to the only question a
//! shortage query asks.

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::billing_totals::{div_round_half_away, to_i64};
use crate::error::{Result, StoreError};
use crate::id::{BillingProductId, InvLocationId};
use crate::inv_locations::LocationKind;

/// The columns a stock read selects, in `StockRow` order.
const STOCK_COLS: &str = "s.product_id, p.name AS product_name, p.sku, \
     p.purchase_price_cents, s.location_id, l.code AS location_code, l.name AS location_name, \
     l.kind AS location_kind, s.qty_milli, s.last_move_at";

/// What is where: one product at one location, with what it is worth.
#[derive(Debug, Clone)]
pub struct StockLevel {
    /// The product.
    pub product_id: BillingProductId,
    /// What it is called, as the catalog calls it today.
    pub product_name: String,
    /// Its SKU; empty when the tenant has not given it one.
    pub sku: String,
    /// The place.
    pub location_id: InvLocationId,
    /// That location's code.
    pub location_code: String,
    /// That location's name.
    pub location_name: String,
    /// What kind of place it is — a real one holds goods, a virtual one holds
    /// the counterpart of goods that came from or went to the outside world.
    pub location_kind: LocationKind,
    /// How much, in milli-units. Signed: negative is legitimate on a virtual
    /// counterparty and impossible on a real location.
    pub qty_milli: i64,
    /// What that quantity cost, in integer cents at the product's purchase
    /// price. Zero when the tenant has not recorded a purchase price.
    pub value_cents: i64,
    /// The latest movement folded into this figure — what a screen shows as
    /// "as of".
    pub last_move_at: OffsetDateTime,
}

/// Which slice of the stock picture to read.
///
/// The default is the stock screen's own question — what is on the shelves,
/// right now — and the two flags below say what that deliberately leaves out.
#[derive(Debug, Clone, Default)]
pub struct StockFilter {
    /// One product across every location.
    pub product_id: Option<BillingProductId>,
    /// One location across every product.
    pub location_id: Option<InvLocationId>,
    /// Whether to include the four virtual counterparties. Off by default: a
    /// stock screen answers "what have we got", and `supplier` holding minus
    /// four hundred is an accounting fact rather than a shelf.
    pub include_virtual: bool,
    /// Whether to include rows that have fallen back to zero. Off by default —
    /// a product that came and went is not stock — but on when a person is
    /// looking at one product's history.
    pub include_zero: bool,
}

/// One balance as the **ledger** states it, recomputed from the movements
/// rather than read from the cache. The proof read, and the rebuild's input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StockBalance {
    /// The product.
    pub product_id: BillingProductId,
    /// The place.
    pub location_id: InvLocationId,
    /// What the movements add up to, in milli-units.
    pub qty_milli: i64,
    /// The latest movement that contributed to it.
    pub last_move_at: OffsetDateTime,
}

/// What a quantity of a product is worth, in integer cents.
///
/// Pure, and the only place stock is valued: `qty × price ÷ 1000`, rounded once
/// and half away from zero, so a negative balance on a virtual counterparty
/// values as the exact mirror of the positive one it came from.
pub fn stock_value_cents(qty_milli: i64, purchase_price_cents: i64) -> i64 {
    to_i64(div_round_half_away(
        i128::from(qty_milli) * i128::from(purchase_price_cents),
        1_000,
    ))
}

impl AccountStore {
    /// What is where — the stock screen's read, and the one the shortage query
    /// (B5.07) will make across every stocked product at once.
    ///
    /// Ordered by product name then location code, so the same tenant's screen
    /// reads the same way twice.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure. An unknown or foreign product or location
    /// id is an empty list, never a refusal that would confirm it exists.
    pub async fn inv_stock(&self, filter: &StockFilter) -> Result<Vec<StockLevel>> {
        let rows = sqlx::query_as::<_, StockRow>(&format!(
            "SELECT {STOCK_COLS} FROM inv_stock s \
             JOIN billing_products p ON p.tenant_id = s.tenant_id AND p.id = s.product_id \
             JOIN inv_locations l ON l.tenant_id = s.tenant_id AND l.id = s.location_id \
             WHERE s.tenant_id = $1 \
               AND ($2::text IS NULL OR s.product_id = $2) \
               AND ($3::text IS NULL OR s.location_id = $3) \
               AND ($4 OR l.kind IN ('stock', 'transit')) \
               AND ($5 OR s.qty_milli <> 0) \
             ORDER BY lower(p.name), p.id, l.code"
        ))
        .bind(self.tenant.as_str())
        .bind(filter.product_id.as_ref().map(BillingProductId::as_str))
        .bind(filter.location_id.as_ref().map(InvLocationId::as_str))
        .bind(filter.include_virtual)
        .bind(filter.include_zero)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter().map(StockRow::into_level).collect()
    }

    /// How much of one product is at one place, in milli-units.
    ///
    /// Zero when nothing has ever moved there — "we have none" and "we have
    /// never had any" are the same answer to the question being asked. A
    /// product or location that is not this tenant's is zero for the same
    /// reason it is absent from every other read: existence is never disclosed.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn inv_on_hand(
        &self,
        product: &BillingProductId,
        location: &InvLocationId,
    ) -> Result<i64> {
        sqlx::query_scalar::<_, Option<i64>>(
            "SELECT qty_milli FROM inv_stock \
             WHERE tenant_id = $1 AND product_id = $2 AND location_id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(product.as_str())
        .bind(location.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)
        .map(|found| found.flatten().unwrap_or(0))
    }

    /// **What the ledger says**, recomputed from every movement this tenant has
    /// ever recorded: the fold the cache is a cache of.
    ///
    /// This is the function that makes the cache trustworthy rather than merely
    /// fast — the property suite compares it to `inv_stock` after every write,
    /// and [`AccountStore::inv_stock_rebuild`] writes its result back. It is
    /// deliberately **not** reachable from any route: a discrepancy is a bug
    /// for a test to catch, not an operational condition for a tenant to
    /// inspect.
    ///
    /// Ordered by product then location, so two folds of the same ledger are
    /// comparable element by element.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn inv_stock_folded(&self) -> Result<Vec<StockBalance>> {
        let rows = sqlx::query_as::<_, BalanceRow>(
            "SELECT product_id, location_id, SUM(delta)::bigint AS qty_milli, \
                 MAX(occurred_at) AS last_move_at \
             FROM ( \
                 SELECT product_id, to_location_id AS location_id, qty_milli AS delta, \
                        occurred_at \
                 FROM inv_moves WHERE tenant_id = $1 \
                 UNION ALL \
                 SELECT product_id, from_location_id AS location_id, -qty_milli AS delta, \
                        occurred_at \
                 FROM inv_moves WHERE tenant_id = $1 \
             ) sides \
             GROUP BY product_id, location_id \
             ORDER BY product_id, location_id",
        )
        .bind(self.tenant.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(BalanceRow::into_balance).collect())
    }

    /// The cached balances as stored, in the same order
    /// [`AccountStore::inv_stock_folded`] returns — so a test can compare the
    /// two and say which pair disagrees.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn inv_stock_cached(&self) -> Result<Vec<StockBalance>> {
        let rows = sqlx::query_as::<_, BalanceRow>(
            "SELECT product_id, location_id, qty_milli, last_move_at FROM inv_stock \
             WHERE tenant_id = $1 ORDER BY product_id, location_id",
        )
        .bind(self.tenant.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(BalanceRow::into_balance).collect())
    }

    /// Rebuilds every cached balance from the ledger, and returns how many rows
    /// the tenant now has.
    ///
    /// The cache's disposability, made real: it exists so that a maintenance
    /// command can restore the invariant after a bug, and its existence is what
    /// lets the rest of the module treat `inv_stock` as an optimisation rather
    /// than a record. **No route calls it**, and none will — a tenant cannot
    /// ask for their books to be recomputed, because the answer must never have
    /// depended on their asking.
    ///
    /// One transaction: the tenant's cache is never half-old.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn inv_stock_rebuild(&self) -> Result<usize> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        sqlx::query("DELETE FROM inv_stock WHERE tenant_id = $1")
            .bind(self.tenant.as_str())
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        let written = sqlx::query(
            "INSERT INTO inv_stock (tenant_id, product_id, location_id, qty_milli, last_move_at) \
             SELECT $1, product_id, location_id, SUM(delta)::bigint, MAX(occurred_at) \
             FROM ( \
                 SELECT product_id, to_location_id AS location_id, qty_milli AS delta, \
                        occurred_at \
                 FROM inv_moves WHERE tenant_id = $1 \
                 UNION ALL \
                 SELECT product_id, from_location_id AS location_id, -qty_milli AS delta, \
                        occurred_at \
                 FROM inv_moves WHERE tenant_id = $1 \
             ) sides \
             GROUP BY product_id, location_id",
        )
        .bind(self.tenant.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(usize::try_from(written.rows_affected()).unwrap_or(usize::MAX))
    }
}

// ---- row types --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct StockRow {
    product_id: String,
    product_name: String,
    sku: String,
    purchase_price_cents: i64,
    location_id: String,
    location_code: String,
    location_name: String,
    location_kind: String,
    qty_milli: i64,
    last_move_at: OffsetDateTime,
}

impl StockRow {
    fn into_level(self) -> Result<StockLevel> {
        Ok(StockLevel {
            product_id: BillingProductId::new(self.product_id),
            product_name: self.product_name,
            sku: self.sku,
            location_id: InvLocationId::new(self.location_id),
            location_code: self.location_code,
            location_name: self.location_name,
            location_kind: LocationKind::parse(&self.location_kind)?,
            qty_milli: self.qty_milli,
            value_cents: stock_value_cents(self.qty_milli, self.purchase_price_cents),
            last_move_at: self.last_move_at,
        })
    }
}

#[derive(sqlx::FromRow)]
struct BalanceRow {
    product_id: String,
    location_id: String,
    qty_milli: i64,
    last_move_at: OffsetDateTime,
}

impl BalanceRow {
    fn into_balance(self) -> StockBalance {
        StockBalance {
            product_id: BillingProductId::new(self.product_id),
            location_id: InvLocationId::new(self.location_id),
            qty_milli: self.qty_milli,
            last_move_at: self.last_move_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_whole_unit_values_at_its_purchase_price() {
        assert_eq!(stock_value_cents(1_000, 2_150), 2_150);
        assert_eq!(stock_value_cents(12_000, 2_150), 25_800);
        assert_eq!(stock_value_cents(0, 2_150), 0);
        assert_eq!(
            stock_value_cents(5_000, 0),
            0,
            "no price recorded, no value"
        );
    }

    #[test]
    fn a_part_unit_rounds_once_half_away_from_zero() {
        // 0.5 × 3 cents = 1.5 → 2, and its mirror → −2, so a movement and its
        // reversal value out to exactly nothing.
        assert_eq!(stock_value_cents(500, 3), 2);
        assert_eq!(stock_value_cents(-500, 3), -2);
        assert_eq!(stock_value_cents(500, 3) + stock_value_cents(-500, 3), 0);
        // 1.5 kg at 99 cents is 148.5 → 149.
        assert_eq!(stock_value_cents(1_500, 99), 149);
    }

    #[test]
    fn a_virtual_counterpartys_negative_balance_values_as_the_mirror() {
        // The supplier location holds minus what we bought; the two together
        // are the zero the whole ledger is built on.
        let bought = stock_value_cents(40_000, 1_299);
        let owed = stock_value_cents(-40_000, 1_299);
        assert_eq!(bought + owed, 0);
        assert_eq!(bought, 51_960);
    }

    #[test]
    fn the_default_filter_is_the_stock_screens_question() {
        let d = StockFilter::default();
        assert!(
            !d.include_virtual,
            "'what have we got' is about shelves, not counterparties"
        );
        assert!(!d.include_zero, "a product that came and went is not stock");
    }
}
