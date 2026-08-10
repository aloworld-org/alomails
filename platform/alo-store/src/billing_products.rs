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
//!
//! # The catalog (alo Inventory, wave B5.02)
//!
//! A warehouse needs five more facts about the same rows — an SKU, a barcode,
//! whether the thing is *stocked* or a service, what it costs us, and a photo
//! — and they live here rather than in a sibling `inv_items` table
//! (`docs/design/inventory.md`, "The catalog"): a product is one thing in a
//! tenant's head, and a two-table split immediately raises the question of
//! what a row in one and not the other means.
//!
//! Three rules that arrive with them:
//!
//! - **SKU and barcode are unique within the tenant, never globally.** The
//!   indexes are partial and tenant-scoped; a global one would leak the
//!   existence of another tenant's product through a constraint violation,
//!   and would be wrong on the facts besides — two businesses legitimately
//!   stock the same GTIN. A collision inside the tenant is a
//!   [`StoreError::Conflict`] naming which field collided.
//! - **A barcode is check-digit validated** ([`crate::inv_barcode`]) so a
//!   misread scan is refused at the door rather than discovered when the
//!   wrong item ships.
//! - **`stocked` decides whether the move ledger accepts the product at all**
//!   ([`crate::inv_moves`] refuses to move a service). Turning it *off* for a
//!   product that already carries movements is a [`StoreError::Conflict`] as of
//!   B5.04a: those quantities are a claim about a thing with a shelf, and
//!   un-saying it would leave them describing nothing. Archiving the product is
//!   the way out, and it keeps every movement explainable.
//!
//! The photo is a Drive node referenced by id and never copied, gated on write
//! by [`AccountStore::drive_require_read`] — the caller must be able to see
//! the node they attach, so a guessed id attaches nothing (B4.05a's rule).
//!
//! `default_supplier_id` — reserved by B5.02 and deliberately unwritable until
//! there was a supplier table to point at — is **writable as of B5.03**. It is
//! held to the same rule as the photo: the id must be one of this tenant's own
//! suppliers ([`crate::inv_suppliers`]), checked before the write and made
//! structural by a composite foreign key, so a guessed id links nothing and
//! answers [`StoreError::NotFound`].

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::billing_field::{bounded, required, unit_price_cents, vat_rate_bp};
use crate::error::{Result, StoreError};
use crate::id::{BillingProductId, DriveNodeId, InvSupplierId};
use crate::inv_barcode;

/// A product name is what lands in the line description — generous but
/// bounded. The invoice line it gets copied onto (B1.06) will carry the same
/// bound, so a name can never be truncated on its way into a document.
pub const PRODUCT_NAME_MAX_CHARS: usize = 200;
/// A unit label is a word, not a sentence ("hour", "piece", "kg", "Stunde").
pub const PRODUCT_UNIT_MAX_CHARS: usize = 32;
/// An SKU is the tenant's own code for the item — a code, not a description.
/// Generous enough for the "AB-1234-BLUE-XL" shape real catalogues use.
pub const PRODUCT_SKU_MAX_CHARS: usize = 64;

/// The columns every read of a product selects, in `ProductRow` order.
const PRODUCT_COLS: &str = "id, name, unit, unit_price_cents, vat_rate_bp, sku, barcode, \
     stocked, purchase_price_cents, photo_node_id, default_supplier_id, archived_at, \
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
    /// The tenant's own code for the item. Blank is legitimate — a services
    /// business has none — and only a non-blank one is unique.
    pub sku: String,
    /// The code on the box: GTIN-8/12/13/14, check-digit validated and stored
    /// as digits. Blank when the item has no barcode.
    pub barcode: String,
    /// Whether the item has a quantity at all. `false` (the default) means a
    /// service, which the move ledger refuses to move.
    pub stocked: bool,
    /// What we pay for one unit, in integer cents of the tenant's currency.
    pub purchase_price_cents: i64,
    /// The product photo in Drive, referenced and never copied. The caller
    /// must be able to read the node.
    pub photo_node_id: Option<DriveNodeId>,
    /// Who we usually buy it from — the seed of a reorder proposal (B5.07).
    /// Must be one of **this tenant's** suppliers (B5.03).
    pub default_supplier_id: Option<InvSupplierId>,
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
    /// The tenant's own code for the item; empty when it has none.
    pub sku: String,
    /// The GTIN on the box, digits only; empty when it has none.
    pub barcode: String,
    /// Whether the item carries a quantity (B5.04a's ledger reads this).
    pub stocked: bool,
    /// What one unit costs us, in integer cents.
    pub purchase_price_cents: i64,
    /// The product photo in Drive, if one is attached.
    pub photo_node_id: Option<DriveNodeId>,
    /// The supplier we usually buy it from, if one is set.
    pub default_supplier_id: Option<InvSupplierId>,
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
    sku: String,
    barcode: String,
    stocked: bool,
    purchase_price_cents: i64,
}

/// Validates and normalises a whole product. Pure — no database, so the rules
/// are unit-tested directly. (The one rule that needs a door, that a photo is
/// a node the caller can see, is [`AccountStore::require_product_links`].)
fn normalize(input: &NewProduct) -> Result<Normalized> {
    Ok(Normalized {
        name: required("name", &input.name, PRODUCT_NAME_MAX_CHARS)?,
        unit: bounded("unit", &input.unit, PRODUCT_UNIT_MAX_CHARS)?,
        unit_price_cents: unit_price_cents("unit price", input.unit_price_cents)?,
        vat_rate_bp: vat_rate_bp(input.vat_rate_bp)?,
        sku: bounded("SKU", &input.sku, PRODUCT_SKU_MAX_CHARS)?,
        // The verdict is the barcode module's; the message it produces names
        // the rule and never the code, so it can be handed straight on.
        barcode: inv_barcode::canonicalize(&input.barcode)
            .map_err(|e| StoreError::Validation(e.to_string()))?
            .unwrap_or_default(),
        stocked: input.stocked,
        purchase_price_cents: unit_price_cents("purchase price", input.purchase_price_cents)?,
    })
}

/// Turns the catalog's two uniqueness rules into typed answers naming the
/// field that collided, and leaves every other database failure alone.
///
/// Both indexes are partial and tenant-scoped, so a conflict here is always
/// **this** tenant's own other product — never a signal about somebody else's
/// catalog.
fn map_product_conflict(error: sqlx::Error) -> StoreError {
    // The default-supplier key is the race window: the supplier existed when
    // the write checked and was gone (with its tenant) before the row landed.
    // Same answer as an id that was never this tenant's.
    if let sqlx::Error::Database(ref db) = error
        && db.constraint() == Some("billing_products_default_supplier_fk")
    {
        return StoreError::NotFound;
    }
    match error {
        sqlx::Error::Database(ref db) if db.code().as_deref() == Some("23505") => {
            match db.constraint().unwrap_or_default() {
                "billing_products_sku_unique" => {
                    StoreError::Conflict("another product already has this SKU".to_owned())
                }
                "billing_products_barcode_unique" => {
                    StoreError::Conflict("another product already has this barcode".to_owned())
                }
                _ => StoreError::Conflict("unique constraint".to_owned()),
            }
        }
        other => StoreError::Db(other),
    }
}

impl AccountStore {
    /// Confirms every thing a product points at is one the caller can reach.
    ///
    /// Two pointers today. The photo is a Drive node they may **read**, so a
    /// guessed node id attaches nothing; the default supplier must be one of
    /// **this tenant's** suppliers (B5.03), so a guessed supplier id links
    /// nothing. Both answer [`StoreError::NotFound`] when the target is
    /// somebody else's — never a refusal that would confirm it exists.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] / [`StoreError::Db`].
    async fn require_product_links(&self, input: &NewProduct) -> Result<()> {
        if let Some(photo) = input.photo_node_id.as_ref() {
            self.drive_require_read(photo).await?;
        }
        self.require_tenant_supplier(input.default_supplier_id.as_ref())
            .await?;
        Ok(())
    }

    /// Creates an active product.
    ///
    /// # Errors
    /// [`StoreError::Validation`] on any field the caller can fix (blank or
    /// over-long name, over-long unit or SKU, negative or absurd price, VAT
    /// rate outside 0–10 000 bp, a barcode whose check digit does not match);
    /// [`StoreError::Conflict`] when the SKU or barcode is already another of
    /// this tenant's products'; [`StoreError::NotFound`] when the photo is not
    /// a node the caller can read or the default supplier is not this tenant's;
    /// [`StoreError::Db`] on failure.
    pub async fn create_billing_product(&self, input: &NewProduct) -> Result<BillingProductId> {
        let p = normalize(input)?;
        self.require_product_links(input).await?;
        let id = BillingProductId::generate();
        sqlx::query(
            "INSERT INTO billing_products (tenant_id, id, name, unit, unit_price_cents, \
                 vat_rate_bp, sku, barcode, stocked, purchase_price_cents, photo_node_id, \
                 default_supplier_id, created_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(&p.name)
        .bind(&p.unit)
        .bind(p.unit_price_cents)
        .bind(p.vat_rate_bp)
        .bind(&p.sku)
        .bind(&p.barcode)
        .bind(p.stocked)
        .bind(p.purchase_price_cents)
        .bind(input.photo_node_id.as_ref().map(DriveNodeId::as_str))
        .bind(
            input
                .default_supplier_id
                .as_ref()
                .map(InvSupplierId::as_str),
        )
        .bind(self.user.as_str())
        .execute(&self.pool)
        .await
        .map_err(map_product_conflict)?;
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

    /// The tenant's product carrying this barcode, or `None`.
    ///
    /// The read a scanner makes (B5.09c): a code off a box, one product or
    /// nothing. At most one row can match, because the barcode index is unique
    /// within the tenant — and only within it, so this can never see another
    /// tenant's stock even when two businesses sell the same GTIN. A code that
    /// fails validation is `None` rather than an error: a bad scan found
    /// nothing, which is what the person holding the scanner needs to hear.
    /// Archived products match, so a discontinued item still explains itself
    /// when its box turns up in a corner of the warehouse.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn billing_product_by_barcode(&self, barcode: &str) -> Result<Option<Product>> {
        let Ok(Some(code)) = inv_barcode::canonicalize(barcode) else {
            return Ok(None);
        };
        let row = sqlx::query_as::<_, ProductRow>(&format!(
            "SELECT {PRODUCT_COLS} FROM billing_products WHERE tenant_id = $1 AND barcode = $2"
        ))
        .bind(self.tenant.as_str())
        .bind(&code)
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
    /// [`StoreError::Validation`], [`StoreError::Conflict`] and the photo's
    /// [`StoreError::NotFound`] exactly as for create; [`StoreError::NotFound`]
    /// when the product isn't the tenant's; [`StoreError::Db`] on failure.
    pub async fn update_billing_product(
        &self,
        id: &BillingProductId,
        input: &NewProduct,
    ) -> Result<()> {
        let p = normalize(input)?;
        self.require_product_links(input).await?;
        // **Un-stocking a product that has moved is refused** (B5.04a). The
        // ledger's quantities are a claim about a thing with a shelf; saying it
        // never had one leaves those movements describing a service, and the
        // stock screen quietly stops showing goods that are still in the
        // building. The way out is to archive the product, which keeps every
        // movement explainable.
        if !p.stocked
            && self.inv_product_has_moves(id).await?
            && self
                .billing_product(id)
                .await?
                .is_some_and(|stored| stored.stocked)
        {
            return Err(StoreError::Conflict(
                "a product that has stock movements cannot stop being stocked".to_owned(),
            ));
        }
        let done = sqlx::query(
            "UPDATE billing_products SET name = $3, unit = $4, unit_price_cents = $5, \
                 vat_rate_bp = $6, sku = $7, barcode = $8, stocked = $9, \
                 purchase_price_cents = $10, photo_node_id = $11, \
                 default_supplier_id = $12, updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(&p.name)
        .bind(&p.unit)
        .bind(p.unit_price_cents)
        .bind(p.vat_rate_bp)
        .bind(&p.sku)
        .bind(&p.barcode)
        .bind(p.stocked)
        .bind(p.purchase_price_cents)
        .bind(input.photo_node_id.as_ref().map(DriveNodeId::as_str))
        .bind(
            input
                .default_supplier_id
                .as_ref()
                .map(InvSupplierId::as_str),
        )
        .execute(&self.pool)
        .await
        .map_err(map_product_conflict)?;
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
    sku: String,
    barcode: String,
    stocked: bool,
    purchase_price_cents: i64,
    photo_node_id: Option<String>,
    default_supplier_id: Option<String>,
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
            sku: self.sku,
            barcode: self.barcode,
            stocked: self.stocked,
            purchase_price_cents: self.purchase_price_cents,
            photo_node_id: self.photo_node_id.map(DriveNodeId::new),
            default_supplier_id: self.default_supplier_id.map(InvSupplierId::new),
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
            ..Default::default()
        }
    }

    /// A stocked item, the catalog's other half.
    fn chair() -> NewProduct {
        NewProduct {
            name: "Blue chair".to_owned(),
            unit: "piece".to_owned(),
            unit_price_cents: 4_900,
            vat_rate_bp: 2100,
            sku: "CH-BLUE-01".to_owned(),
            barcode: "4006381333931".to_owned(),
            stocked: true,
            purchase_price_cents: 2_150,
            photo_node_id: None,
            default_supplier_id: None,
        }
    }

    fn invalid<T: std::fmt::Debug>(result: Result<T>) -> String {
        match result {
            Err(StoreError::Validation(msg)) => msg,
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn defaults_are_a_free_zero_rated_unitless_service() {
        let d = NewProduct::default();
        assert_eq!(d.unit_price_cents, 0);
        assert_eq!(d.vat_rate_bp, 0);
        assert!(d.unit.is_empty());
        // The catalog half: nothing is stocked until somebody says so, so no
        // existing tenant acquires a stock ledger by upgrade.
        assert!(!d.stocked);
        assert!(d.sku.is_empty());
        assert!(d.barcode.is_empty());
        assert_eq!(d.purchase_price_cents, 0);
        assert!(d.photo_node_id.is_none());
        // Nothing points at a supplier until somebody picks one (B5.03).
        assert!(d.default_supplier_id.is_none());
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

    // ---- the catalog (B5.02) ------------------------------------------------

    #[test]
    fn a_stocked_item_keeps_its_catalog_facts() {
        let p = normalize(&chair()).unwrap_or_else(|e| panic!("rejected: {e}"));
        assert_eq!(p.sku, "CH-BLUE-01");
        assert_eq!(p.barcode, "4006381333931");
        assert!(p.stocked);
        // Both prices are integers and they are different numbers: what we
        // charge is not what we pay, and neither is ever a float.
        assert_eq!(p.unit_price_cents, 4_900);
        assert_eq!(p.purchase_price_cents, 2_150);
    }

    #[test]
    fn sku_is_optional_trimmed_and_bounded() {
        let none = NewProduct {
            sku: "   ".to_owned(),
            ..chair()
        };
        assert_eq!(
            normalize(&none)
                .unwrap_or_else(|e| panic!("rejected: {e}"))
                .sku,
            "",
            "a services business has no SKU"
        );
        let padded = NewProduct {
            sku: "  CH-BLUE-01 ".to_owned(),
            ..chair()
        };
        assert_eq!(
            normalize(&padded)
                .unwrap_or_else(|e| panic!("rejected: {e}"))
                .sku,
            "CH-BLUE-01",
            "stored trimmed, so the unique index compares like with like"
        );
        let long = NewProduct {
            sku: "x".repeat(PRODUCT_SKU_MAX_CHARS + 1),
            ..chair()
        };
        assert!(invalid(normalize(&long)).contains("SKU"));
        let at_bound = NewProduct {
            sku: "x".repeat(PRODUCT_SKU_MAX_CHARS),
            ..chair()
        };
        assert!(normalize(&at_bound).is_ok());
    }

    #[test]
    fn a_barcode_is_canonicalised_and_check_digit_validated() {
        // Separators are how a code is read off a box; they are presentation.
        let spaced = NewProduct {
            barcode: " 400-638 133 393 1 ".to_owned(),
            ..chair()
        };
        assert_eq!(
            normalize(&spaced)
                .unwrap_or_else(|e| panic!("rejected: {e}"))
                .barcode,
            "4006381333931"
        );
        // No barcode is the normal case for plenty of stock.
        let none = NewProduct {
            barcode: String::new(),
            ..chair()
        };
        assert_eq!(
            normalize(&none)
                .unwrap_or_else(|e| panic!("rejected: {e}"))
                .barcode,
            ""
        );
        // A typo, a short code and a letter are all refused at the door —
        // and the refusal never echoes the code back.
        for bad in ["4006381333930", "12345", "40063813339A1"] {
            let input = NewProduct {
                barcode: bad.to_owned(),
                ..chair()
            };
            let message = invalid(normalize(&input));
            assert!(message.contains("barcode"), "unhelpful message: {message}");
            assert!(
                !message.contains(bad),
                "the message carried the code: {message}"
            );
        }
    }

    #[test]
    fn a_leading_zero_survives_normalisation() {
        // The reason the column is text: these are two different boxes.
        let short = NewProduct {
            barcode: "012345678905".to_owned(),
            ..chair()
        };
        assert_eq!(
            normalize(&short)
                .unwrap_or_else(|e| panic!("rejected: {e}"))
                .barcode,
            "012345678905"
        );
    }

    #[test]
    fn purchase_price_is_money_and_is_bounded_like_a_price() {
        for ok in [0, 1, 2_150, UNIT_PRICE_MAX_CENTS] {
            let input = NewProduct {
                purchase_price_cents: ok,
                ..chair()
            };
            assert!(normalize(&input).is_ok(), "expected valid: {ok}");
        }
        for bad in [-1, UNIT_PRICE_MAX_CENTS + 1, i64::MAX] {
            let input = NewProduct {
                purchase_price_cents: bad,
                ..chair()
            };
            assert!(
                invalid(normalize(&input)).contains("purchase price"),
                "expected rejection naming the field: {bad}"
            );
        }
    }
}
