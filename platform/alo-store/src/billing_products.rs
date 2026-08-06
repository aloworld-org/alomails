//! Billing products — the tenant's price list (alo Billing, ADR 0035, wave
//! B1), reached through the account door like [`crate::billing_customers`].
//!
//! A product is a **source** for a document line, not a dependency of one.
//! Picking a product copies its name, unit, price and VAT rate onto the line
//! at that moment (`docs/design/billing.md`); editing the price list
//! afterwards never rewrites a document that was already raised. That rule is
//! why this module has no reference from lines back to products, and why a
//! product is **archived, never deleted** — an item that is no longer sold
//! disappears from the pickers while last year's books stay explainable.
//!
//! Products are **tenant-wide**: the predicate on every statement is
//! `tenant_id`, taken from the handle and never from request input. Input is
//! normalised once, in [`normalize`], and the same normalisation runs for
//! create and update, so a field can never be stored two different ways
//! depending on which door it came through. Money is integer cents and the
//! VAT rate is basis points — no float touches this module.

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::billing_field::{bounded, required, unit_price_cents, vat_rate_bp};
use crate::error::{Result, StoreError};
use crate::id::BillingProductId;

/// A product name is what lands in the line description — generous but
/// bounded. The invoice line it gets copied onto (B1.06) will carry the same
/// bound, so a name can never be truncated on its way into a document.
pub const PRODUCT_NAME_MAX_CHARS: usize = 200;
/// A unit label is a word, not a sentence ("hour", "piece", "kg", "Stunde").
pub const PRODUCT_UNIT_MAX_CHARS: usize = 32;

/// The columns every read of a product selects, in `ProductRow` order.
const PRODUCT_COLS: &str = "id, name, unit, unit_price_cents, vat_rate_bp, archived_at, \
     created_by, created_at, updated_at";

/// The writable shape of a product, used for both create and update (an
/// update is a full replace — the route layer merges a partial `PATCH` onto
/// the stored record before calling).
///
/// [`Default`] gives a free, zero-rated, unitless item, so a caller can write
/// `NewProduct { name, unit_price_cents, vat_rate_bp, ..Default::default() }`.
/// Only `name` is required.
#[derive(Debug, Clone, Default)]
pub struct NewProduct {
    /// What the line is called when this product is picked. Required,
    /// non-blank.
    pub name: String,
    /// Unit label shown on the line; empty for a unitless item.
    pub unit: String,
    /// Price of one unit in integer cents, in the tenant's default currency.
    pub unit_price_cents: i64,
    /// VAT rate in basis points (2100 = 21 %).
    pub vat_rate_bp: i32,
}

/// A stored product.
#[derive(Debug, Clone)]
pub struct Product {
    /// Opaque id, unique within the tenant.
    pub id: BillingProductId,
    /// What the line is called when this product is picked.
    pub name: String,
    /// Unit label; empty for a unitless item.
    pub unit: String,
    /// Price of one unit in integer cents.
    pub unit_price_cents: i64,
    /// VAT rate in basis points.
    pub vat_rate_bp: i32,
    /// When the product was archived; `None` while active.
    pub archived_at: Option<OffsetDateTime>,
    /// The user who created the record.
    pub created_by: String,
    /// Creation time.
    pub created_at: OffsetDateTime,
    /// Last modification time.
    pub updated_at: OffsetDateTime,
}

impl Product {
    /// Whether the product is archived — hidden from the pickers, still
    /// readable so an old document can be explained.
    pub fn is_archived(&self) -> bool {
        self.archived_at.is_some()
    }
}

/// A validated, normalised product ready to be bound into a statement.
#[derive(Debug)]
struct Normalized {
    name: String,
    unit: String,
    unit_price_cents: i64,
    vat_rate_bp: i32,
}

/// Validates and normalises a whole product. Pure — no database, so the rules
/// are unit-tested directly.
fn normalize(input: &NewProduct) -> Result<Normalized> {
    Ok(Normalized {
        name: required("name", &input.name, PRODUCT_NAME_MAX_CHARS)?,
        unit: bounded("unit", &input.unit, PRODUCT_UNIT_MAX_CHARS)?,
        unit_price_cents: unit_price_cents("unit price", input.unit_price_cents)?,
        vat_rate_bp: vat_rate_bp(input.vat_rate_bp)?,
    })
}

impl AccountStore {
    /// Creates an active product.
    ///
    /// # Errors
    /// [`StoreError::Validation`] on any field the caller can fix (blank or
    /// over-long name, over-long unit, negative or absurd price, VAT rate
    /// outside 0–10 000 bp); [`StoreError::Db`] on failure.
    pub async fn create_billing_product(&self, input: &NewProduct) -> Result<BillingProductId> {
        let p = normalize(input)?;
        let id = BillingProductId::generate();
        sqlx::query(
            "INSERT INTO billing_products (tenant_id, id, name, unit, unit_price_cents, \
                 vat_rate_bp, created_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(&p.name)
        .bind(&p.unit)
        .bind(p.unit_price_cents)
        .bind(p.vat_rate_bp)
        .bind(self.user.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(id)
    }

    /// The tenant's price list in name order. Archived products are excluded
    /// unless `include_archived`, and then sort after the active ones.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn billing_products(&self, include_archived: bool) -> Result<Vec<Product>> {
        let rows = sqlx::query_as::<_, ProductRow>(&format!(
            "SELECT {PRODUCT_COLS} FROM billing_products \
             WHERE tenant_id = $1 AND ($2 OR archived_at IS NULL) \
             ORDER BY (archived_at IS NOT NULL), lower(name), id"
        ))
        .bind(self.tenant.as_str())
        .bind(include_archived)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(ProductRow::into_product).collect())
    }

    /// One product of the tenant, or `None` — including when the id belongs
    /// to another tenant (indistinguishable by design).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn billing_product(&self, id: &BillingProductId) -> Result<Option<Product>> {
        let row = sqlx::query_as::<_, ProductRow>(&format!(
            "SELECT {PRODUCT_COLS} FROM billing_products WHERE tenant_id = $1 AND id = $2"
        ))
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(row.map(ProductRow::into_product))
    }

    /// Replaces every writable field of a product. Archiving is a separate
    /// operation ([`AccountStore::set_billing_product_archived`]) so an
    /// ordinary price edit can never drop an item out of the pickers by
    /// accident.
    ///
    /// A new price applies to documents raised **from now on**; lines already
    /// written keep the price they snapshotted.
    ///
    /// # Errors
    /// [`StoreError::Validation`] as for create; [`StoreError::NotFound`] when
    /// the product isn't the tenant's; [`StoreError::Db`] on failure.
    pub async fn update_billing_product(
        &self,
        id: &BillingProductId,
        input: &NewProduct,
    ) -> Result<()> {
        let p = normalize(input)?;
        let done = sqlx::query(
            "UPDATE billing_products SET name = $3, unit = $4, unit_price_cents = $5, \
                 vat_rate_bp = $6, updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(&p.name)
        .bind(&p.unit)
        .bind(p.unit_price_cents)
        .bind(p.vat_rate_bp)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Archives or restores a product. Archiving is the only removal there
    /// is. Idempotent — archiving an archived product keeps the original
    /// archive time.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the product isn't the tenant's;
    /// [`StoreError::Db`] on failure.
    pub async fn set_billing_product_archived(
        &self,
        id: &BillingProductId,
        archived: bool,
    ) -> Result<()> {
        let done = sqlx::query(
            "UPDATE billing_products \
             SET archived_at = CASE WHEN $3 THEN COALESCE(archived_at, now()) ELSE NULL END, \
                 updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(archived)
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
struct ProductRow {
    id: String,
    name: String,
    unit: String,
    unit_price_cents: i64,
    vat_rate_bp: i32,
    archived_at: Option<OffsetDateTime>,
    created_by: String,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl ProductRow {
    fn into_product(self) -> Product {
        Product {
            id: BillingProductId::new(self.id),
            name: self.name,
            unit: self.unit,
            unit_price_cents: self.unit_price_cents,
            vat_rate_bp: self.vat_rate_bp,
            archived_at: self.archived_at,
            created_by: self.created_by,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::billing_field::{UNIT_PRICE_MAX_CENTS, VAT_RATE_MAX_BP};

    fn consulting() -> NewProduct {
        NewProduct {
            name: "Consulting".to_owned(),
            unit: "hour".to_owned(),
            unit_price_cents: 12_000,
            vat_rate_bp: 2100,
        }
    }

    fn invalid<T: std::fmt::Debug>(result: Result<T>) -> String {
        match result {
            Err(StoreError::Validation(msg)) => msg,
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn defaults_are_a_free_zero_rated_unitless_item() {
        let d = NewProduct::default();
        assert_eq!(d.unit_price_cents, 0);
        assert_eq!(d.vat_rate_bp, 0);
        assert!(d.unit.is_empty());
    }

    #[test]
    fn normalize_trims_and_keeps_money_exact() {
        let input = NewProduct {
            name: "  Consulting  ".to_owned(),
            unit: " hour ".to_owned(),
            ..consulting()
        };
        let p = normalize(&input).unwrap_or_else(|e| panic!("rejected: {e}"));
        assert_eq!(p.name, "Consulting");
        assert_eq!(p.unit, "hour");
        // The price is carried through as the integer it came in as — a value
        // that no float round-trip could preserve exactly.
        assert_eq!(p.unit_price_cents, 12_000);
        assert_eq!(p.vat_rate_bp, 2100);
    }

    #[test]
    fn name_is_required_and_bounded() {
        for blank in ["", "   ", "\t\n"] {
            let input = NewProduct {
                name: blank.to_owned(),
                ..consulting()
            };
            assert!(invalid(normalize(&input)).contains("name"));
        }
        let input = NewProduct {
            name: "x".repeat(PRODUCT_NAME_MAX_CHARS + 1),
            ..consulting()
        };
        assert!(invalid(normalize(&input)).contains("at most"));
        // Exactly at the bound is fine.
        let input = NewProduct {
            name: "x".repeat(PRODUCT_NAME_MAX_CHARS),
            ..consulting()
        };
        assert!(normalize(&input).is_ok());
    }

    #[test]
    fn unit_is_optional_and_bounded() {
        let unitless = NewProduct {
            unit: String::new(),
            ..consulting()
        };
        assert_eq!(
            normalize(&unitless)
                .unwrap_or_else(|e| panic!("rejected: {e}"))
                .unit,
            ""
        );
        let input = NewProduct {
            unit: "x".repeat(PRODUCT_UNIT_MAX_CHARS + 1),
            ..consulting()
        };
        assert!(invalid(normalize(&input)).contains("unit"));
    }

    #[test]
    fn price_is_non_negative_and_capped() {
        for ok in [0, 1, 12_000, UNIT_PRICE_MAX_CENTS] {
            let input = NewProduct {
                unit_price_cents: ok,
                ..consulting()
            };
            assert!(normalize(&input).is_ok(), "expected valid: {ok}");
        }
        for bad in [-1, -12_000, UNIT_PRICE_MAX_CENTS + 1, i64::MAX] {
            let input = NewProduct {
                unit_price_cents: bad,
                ..consulting()
            };
            assert!(
                invalid(normalize(&input)).contains("unit price"),
                "expected rejection: {bad}"
            );
        }
    }

    #[test]
    fn vat_rate_is_in_basis_points() {
        // The real European spread, plus the exempt/reverse-charge zero.
        for ok in [0, 500, 600, 900, 1900, 2100, 2500, VAT_RATE_MAX_BP] {
            let input = NewProduct {
                vat_rate_bp: ok,
                ..consulting()
            };
            assert!(normalize(&input).is_ok(), "expected valid: {ok}");
        }
        for bad in [-1, VAT_RATE_MAX_BP + 1, i32::MAX] {
            let input = NewProduct {
                vat_rate_bp: bad,
                ..consulting()
            };
            assert!(
                invalid(normalize(&input)).contains("VAT rate"),
                "expected rejection: {bad}"
            );
        }
    }
}
