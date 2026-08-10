//! What a supplier quotes us, per product (alo Inventory, ADR 0035, wave
//! B5.03) — the price list *they* publish, as opposed to
//! [`crate::billing_products`], which is the one *we* publish.
//!
//! One row is one offer, keyed `(tenant, supplier, product)`: their own article
//! code, what they charge, in which currency, the smallest quantity they will
//! sell, and how long it takes to arrive. It is what lets a reorder proposal
//! (B5.07) say "buy 40 from Hoffmann at €3.15 each, here in nine days" instead
//! of "you are short 40".
//!
//! Three rules shape the module (`docs/design/inventory.md`, "Suppliers"):
//!
//! - **The write is an upsert.** A second quote for the same product replaces
//!   the first rather than growing a history nobody asked for, which is what
//!   makes the route an idempotent `PUT` and lets a form save in one call.
//! - **A price here is a reference, never a snapshot.** A purchase-order line
//!   copies the price at the moment it is drafted (B5.05a) — the rule
//!   [`crate::billing_line`] holds about the sale price — so re-negotiating
//!   never rewrites an order already placed.
//! - **Both ends are checked against the tenant.** The supplier and the
//!   product are looked up through the handle's own tenant before the write,
//!   and the composite foreign keys make it structural besides: an offer
//!   cannot name another tenant's supplier or product even if this module had
//!   a bug. A guessed id from another tenant is a [`StoreError::NotFound`],
//!   the same answer as an id that never existed.
//!
//! Money is integer cents and quantity is milli-units — a thousandth of the
//! product's unit, the same precision a document line carries (B1.06). No
//! float touches this module.

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::billing_field::{bounded, currency, unit_price_cents};
use crate::error::{Result, StoreError};
use crate::id::{BillingProductId, InvSupplierId};
use crate::inv_suppliers::lead_time_days;

/// A supplier's own article code for our product — a code, not a description,
/// bounded like our own SKU.
pub const SUPPLIER_CODE_MAX_CHARS: usize = 64;
/// The largest minimum-order quantity we accept, in milli-units: a billion
/// units. A typo guard, chosen so the quantity stays inside `i64` when the
/// order line multiplies it by a price at the cap.
pub const MIN_ORDER_QTY_MAX_MILLI: i64 = 1_000_000_000_000;

/// The columns every read of an offer selects, in `PriceRow` order. The
/// product's name comes from the catalog, which is why this is a join: the
/// list surface is "what this supplier sells us", and one statement beats one
/// read per row.
const PRICE_COLS: &str = "sp.supplier_id, sp.product_id, p.name AS product_name, \
     sp.supplier_code, sp.purchase_price_cents, sp.currency, sp.min_order_qty_milli, \
     sp.lead_time_days, sp.created_by, sp.created_at, sp.updated_at";

/// The writable shape of one supplier's offer for one product.
///
/// [`Default`] gives a free offer in euro with no minimum and the supplier's
/// own lead time, so a caller can write
/// `NewSupplierPrice { purchase_price_cents, ..Default::default() }`.
#[derive(Debug, Clone)]
pub struct NewSupplierPrice {
    /// Their article code for our product — what goes on the order so their
    /// warehouse picks the right thing. Blank when they use ours.
    pub supplier_code: String,
    /// What they charge for one unit, in integer cents of [`Self::currency`].
    pub purchase_price_cents: i64,
    /// ISO 4217 currency code the offer is quoted in.
    pub currency: String,
    /// The smallest quantity they will sell, in milli-units (`500` = half a
    /// unit). Zero means no minimum.
    pub min_order_qty_milli: i64,
    /// Days between ordering and arrival for **this product**, or `None` to
    /// use the supplier's default — the common case.
    pub lead_time_days: Option<i32>,
}

impl Default for NewSupplierPrice {
    fn default() -> Self {
        Self {
            supplier_code: String::new(),
            purchase_price_cents: 0,
            currency: crate::billing_field::DEFAULT_CURRENCY.to_owned(),
            min_order_qty_milli: 0,
            lead_time_days: None,
        }
    }
}

/// One stored offer, with the name of the product it is for.
#[derive(Debug, Clone)]
pub struct SupplierPrice {
    /// The supplier quoting.
    pub supplier_id: InvSupplierId,
    /// The product quoted for.
    pub product_id: BillingProductId,
    /// The product's name in **our** catalog, for the list surface.
    pub product_name: String,
    /// Their article code; empty when they use ours.
    pub supplier_code: String,
    /// What one unit costs us, in integer cents.
    pub purchase_price_cents: i64,
    /// ISO 4217 currency code, uppercase.
    pub currency: String,
    /// The smallest quantity they will sell, in milli-units.
    pub min_order_qty_milli: i64,
    /// This product's own lead time, or `None` when the supplier's default
    /// applies.
    pub lead_time_days: Option<i32>,
    /// The user who last wrote the offer.
    pub created_by: String,
    /// When the offer was first recorded.
    pub created_at: OffsetDateTime,
    /// When it was last changed.
    pub updated_at: OffsetDateTime,
}

impl SupplierPrice {
    /// How long this product actually takes to arrive: the offer's own lead
    /// time when it states one, otherwise the supplier's default.
    ///
    /// The one piece of arithmetic in the module, and it is here rather than in
    /// the reorder proposal (B5.07) because "when will it be here" must answer
    /// the same way on every screen that asks.
    pub fn effective_lead_time_days(&self, supplier_default: i32) -> i32 {
        self.lead_time_days.unwrap_or(supplier_default)
    }
}

/// A validated, normalised offer ready to be bound into a statement.
#[derive(Debug)]
struct Normalized {
    supplier_code: String,
    purchase_price_cents: i64,
    currency: String,
    min_order_qty_milli: i64,
    lead_time_days: Option<i32>,
}

/// Validates and normalises a whole offer. Pure — no database, so the rules
/// are unit-tested directly.
fn normalize(input: &NewSupplierPrice) -> Result<Normalized> {
    if !(0..=MIN_ORDER_QTY_MAX_MILLI).contains(&input.min_order_qty_milli) {
        return Err(StoreError::Validation(format!(
            "minimum order quantity must be between 0 and {MIN_ORDER_QTY_MAX_MILLI} milli-units"
        )));
    }
    Ok(Normalized {
        supplier_code: bounded(
            "supplier code",
            &input.supplier_code,
            SUPPLIER_CODE_MAX_CHARS,
        )?,
        purchase_price_cents: unit_price_cents("purchase price", input.purchase_price_cents)?,
        currency: currency(&input.currency)?,
        min_order_qty_milli: input.min_order_qty_milli,
        lead_time_days: input.lead_time_days.map(lead_time_days).transpose()?,
    })
}

impl AccountStore {
    /// Records what a supplier quotes for a product, replacing any earlier
    /// quote for the same pair.
    ///
    /// Idempotent by construction: the same call twice leaves one row saying
    /// the same thing, which is what makes the route a `PUT`.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the supplier or the product is not this
    /// tenant's; [`StoreError::Validation`] on any field the caller can fix
    /// (over-long code, negative or absurd price, bad currency, out-of-range
    /// minimum or lead time); [`StoreError::Db`] on failure.
    pub async fn set_inv_supplier_price(
        &self,
        supplier_id: &InvSupplierId,
        product_id: &BillingProductId,
        input: &NewSupplierPrice,
    ) -> Result<()> {
        let offer = normalize(input)?;
        // Both ends, before the write: a guessed id from another tenant is a
        // NotFound, never a foreign-key error that would confirm it exists.
        self.require_tenant_supplier(Some(supplier_id)).await?;
        if self.billing_product(product_id).await?.is_none() {
            return Err(StoreError::NotFound);
        }
        sqlx::query(
            "INSERT INTO inv_supplier_products (tenant_id, supplier_id, product_id, \
                 supplier_code, purchase_price_cents, currency, min_order_qty_milli, \
                 lead_time_days, created_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) \
             ON CONFLICT (tenant_id, supplier_id, product_id) DO UPDATE SET \
                 supplier_code = EXCLUDED.supplier_code, \
                 purchase_price_cents = EXCLUDED.purchase_price_cents, \
                 currency = EXCLUDED.currency, \
                 min_order_qty_milli = EXCLUDED.min_order_qty_milli, \
                 lead_time_days = EXCLUDED.lead_time_days, \
                 updated_at = now()",
        )
        .bind(self.tenant.as_str())
        .bind(supplier_id.as_str())
        .bind(product_id.as_str())
        .bind(&offer.supplier_code)
        .bind(offer.purchase_price_cents)
        .bind(&offer.currency)
        .bind(offer.min_order_qty_milli)
        .bind(offer.lead_time_days)
        .bind(self.user.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }

    /// Everything one supplier sells us, in product-name order.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the supplier is not this tenant's;
    /// [`StoreError::Db`] on failure.
    pub async fn inv_supplier_prices(
        &self,
        supplier_id: &InvSupplierId,
    ) -> Result<Vec<SupplierPrice>> {
        self.require_tenant_supplier(Some(supplier_id)).await?;
        let rows = sqlx::query_as::<_, PriceRow>(&format!(
            "SELECT {PRICE_COLS} FROM inv_supplier_products sp \
             JOIN billing_products p ON p.tenant_id = sp.tenant_id AND p.id = sp.product_id \
             WHERE sp.tenant_id = $1 AND sp.supplier_id = $2 \
             ORDER BY lower(p.name), sp.product_id"
        ))
        .bind(self.tenant.as_str())
        .bind(supplier_id.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(PriceRow::into_price).collect())
    }

    /// Every supplier who sells us one product, in the order the offers were
    /// first recorded.
    ///
    /// The read a product drawer makes, and the one the reorder proposal
    /// (B5.07) will make per shortage.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the product is not this tenant's;
    /// [`StoreError::Db`] on failure.
    pub async fn inv_product_suppliers(
        &self,
        product_id: &BillingProductId,
    ) -> Result<Vec<SupplierPrice>> {
        if self.billing_product(product_id).await?.is_none() {
            return Err(StoreError::NotFound);
        }
        let rows = sqlx::query_as::<_, PriceRow>(&format!(
            "SELECT {PRICE_COLS} FROM inv_supplier_products sp \
             JOIN billing_products p ON p.tenant_id = sp.tenant_id AND p.id = sp.product_id \
             WHERE sp.tenant_id = $1 AND sp.product_id = $2 \
             ORDER BY sp.created_at, sp.supplier_id"
        ))
        .bind(self.tenant.as_str())
        .bind(product_id.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(PriceRow::into_price).collect())
    }

    /// Removes one offer — they no longer sell it, or never did.
    ///
    /// Deleting an offer is safe in a way deleting a supplier is not: an order
    /// already placed copied the price onto its line, so nothing that has
    /// happened depends on this row.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the supplier is not this tenant's or
    /// there is no such offer; [`StoreError::Db`] on failure.
    pub async fn remove_inv_supplier_price(
        &self,
        supplier_id: &InvSupplierId,
        product_id: &BillingProductId,
    ) -> Result<()> {
        let done = sqlx::query(
            "DELETE FROM inv_supplier_products \
             WHERE tenant_id = $1 AND supplier_id = $2 AND product_id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(supplier_id.as_str())
        .bind(product_id.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }
}

// ---- row types --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct PriceRow {
    supplier_id: String,
    product_id: String,
    product_name: String,
    supplier_code: String,
    purchase_price_cents: i64,
    currency: String,
    min_order_qty_milli: i64,
    lead_time_days: Option<i32>,
    created_by: String,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl PriceRow {
    fn into_price(self) -> SupplierPrice {
        SupplierPrice {
            supplier_id: InvSupplierId::new(self.supplier_id),
            product_id: BillingProductId::new(self.product_id),
            product_name: self.product_name,
            supplier_code: self.supplier_code,
            purchase_price_cents: self.purchase_price_cents,
            currency: self.currency,
            min_order_qty_milli: self.min_order_qty_milli,
            lead_time_days: self.lead_time_days,
            created_by: self.created_by,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::billing_field::UNIT_PRICE_MAX_CENTS;
    use crate::inv_suppliers::LEAD_TIME_MAX_DAYS;

    fn offer() -> NewSupplierPrice {
        NewSupplierPrice {
            supplier_code: "HM-4471".to_owned(),
            purchase_price_cents: 315,
            currency: "EUR".to_owned(),
            min_order_qty_milli: 10_000,
            lead_time_days: Some(9),
        }
    }

    fn invalid<T: std::fmt::Debug>(result: Result<T>) -> String {
        match result {
            Err(StoreError::Validation(msg)) => msg,
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn defaults_are_a_free_offer_with_no_minimum_and_the_suppliers_lead_time() {
        let d = NewSupplierPrice::default();
        assert_eq!(d.purchase_price_cents, 0);
        assert_eq!(d.currency, "EUR");
        assert_eq!(d.min_order_qty_milli, 0);
        assert!(
            d.lead_time_days.is_none(),
            "an offer inherits the supplier's lead time unless it says otherwise"
        );
        assert!(d.supplier_code.is_empty());
    }

    #[test]
    fn normalize_trims_the_code_and_keeps_money_exact() {
        let input = NewSupplierPrice {
            supplier_code: "  HM-4471 ".to_owned(),
            currency: "eur".to_owned(),
            ..offer()
        };
        let o = normalize(&input).unwrap_or_else(|e| panic!("rejected: {e}"));
        assert_eq!(o.supplier_code, "HM-4471");
        assert_eq!(o.currency, "EUR");
        // €3.15 stays 315 cents — a number no float round trip preserves.
        assert_eq!(o.purchase_price_cents, 315);
        assert_eq!(o.min_order_qty_milli, 10_000, "10 units, in milli-units");
    }

    #[test]
    fn the_price_is_bounded_like_every_other_price() {
        for ok in [0, 1, 315, UNIT_PRICE_MAX_CENTS] {
            let input = NewSupplierPrice {
                purchase_price_cents: ok,
                ..offer()
            };
            assert!(normalize(&input).is_ok(), "expected valid: {ok}");
        }
        for bad in [-1, UNIT_PRICE_MAX_CENTS + 1, i64::MAX] {
            let input = NewSupplierPrice {
                purchase_price_cents: bad,
                ..offer()
            };
            assert!(
                invalid(normalize(&input)).contains("purchase price"),
                "expected rejection naming the field: {bad}"
            );
        }
    }

    #[test]
    fn the_minimum_quantity_is_bounded_and_never_negative() {
        for ok in [0, 1, 10_000, MIN_ORDER_QTY_MAX_MILLI] {
            let input = NewSupplierPrice {
                min_order_qty_milli: ok,
                ..offer()
            };
            assert!(normalize(&input).is_ok(), "expected valid: {ok}");
        }
        for bad in [-1, MIN_ORDER_QTY_MAX_MILLI + 1, i64::MAX] {
            let input = NewSupplierPrice {
                min_order_qty_milli: bad,
                ..offer()
            };
            assert!(
                invalid(normalize(&input)).contains("minimum order quantity"),
                "expected rejection: {bad}"
            );
        }
    }

    #[test]
    fn an_offers_lead_time_is_optional_and_bounded() {
        let inherits = NewSupplierPrice {
            lead_time_days: None,
            ..offer()
        };
        assert_eq!(
            normalize(&inherits)
                .unwrap_or_else(|e| panic!("rejected: {e}"))
                .lead_time_days,
            None
        );
        for bad in [-1, LEAD_TIME_MAX_DAYS + 1] {
            let input = NewSupplierPrice {
                lead_time_days: Some(bad),
                ..offer()
            };
            assert!(
                invalid(normalize(&input)).contains("lead time"),
                "expected rejection: {bad}"
            );
        }
    }

    #[test]
    fn a_bad_currency_is_refused_on_the_offer_path() {
        for bad in ["", "EU", "EURO", "3UR"] {
            let input = NewSupplierPrice {
                currency: bad.to_owned(),
                ..offer()
            };
            assert!(
                invalid(normalize(&input)).contains("currency"),
                "expected rejection: {bad:?}"
            );
        }
    }

    #[test]
    fn the_effective_lead_time_prefers_the_offer_then_the_supplier() {
        let mut price = SupplierPrice {
            supplier_id: InvSupplierId::new("s"),
            product_id: BillingProductId::new("p"),
            product_name: "Blue chair".to_owned(),
            supplier_code: "HM-4471".to_owned(),
            purchase_price_cents: 315,
            currency: "EUR".to_owned(),
            min_order_qty_milli: 0,
            lead_time_days: Some(9),
            created_by: "u".to_owned(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
        };
        assert_eq!(price.effective_lead_time_days(14), 9);
        price.lead_time_days = None;
        assert_eq!(price.effective_lead_time_days(14), 14);
        // Zero on the offer is a stated fact — same-day — not an absence.
        price.lead_time_days = Some(0);
        assert_eq!(price.effective_lead_time_days(14), 0);
    }
}
