//! The site's shop shelf — which stocked products a site lists for sale, and
//! what the site charges for delivery (ADR 0041, item S3.05a2).
//!
//! A shop item is a *reference*: the row stores Billing's product id and
//! nothing else, so what the item is called, what it costs and what is on the
//! shelf are asked of the owning seams ([`crate::billing_catalog_read`],
//! [`crate::inv_stock_sale`]) at every read — the same discipline as a
//! ticketed event ([`crate::site_tickets`]), with the one difference that a
//! stock item has no date and no capacity of its own: the warehouse's ledger
//! is the capacity.
//!
//! The shipping rate is the one price this module owns, because delivery is
//! something the *site* sells, not a fact any other module holds: one flat
//! rate per order, integer cents in the tenant's accounting currency. Its VAT
//! deliberately has no rate of its own — an ancillary cost follows the main
//! supply, so the order snapshots the goods' rate for both
//! ([`crate::site_stock_orders`]).
//!
//! Every statement scopes by tenant AND site, so a listing is reachable
//! neither from another tenant nor from another site of the same tenant.

use time::OffsetDateTime;

use std::collections::HashMap;

use crate::account::AccountStore;
use crate::billing_catalog_read::{BillingCatalogRead, CatalogSaleItem};
use crate::error::{Result, StoreError};
use crate::id::{BillingProductId, SiteId, SiteShopItemId};
use crate::inv_stock_sale::{InvStockSale, StockForSale, available_units};

/// Maximum products one site's shop may list. Two hundred is a full shop
/// window; a thousand is a runaway loop.
pub const SITE_SHOP_ITEM_MAX_PER_SITE: i64 = 200;

/// The most a site may charge for delivery: 100 000 cents (€1 000.00). A
/// heavier consignment than that is a freight quote, not a web checkout.
pub const SHOP_SHIPPING_MAX_CENTS: i64 = 100_000;

/// One listing as the owner sees it: the reference, and when it was listed.
/// Resolve the reference through the catalog and stock-sale seams for the
/// name, the price and the shelf count *now*.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteShopItem {
    pub id: SiteShopItemId,
    pub product: BillingProductId,
    pub created_at: OffsetDateTime,
}

/// One shelf listing resolved through both owning seams at the moment of the
/// read — what the owner's Shop screen shows. Nothing here is stored: a
/// `None` is the honest state of a reference whose product moved on, shown
/// rather than hidden so the owner can act on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteShopShelfRow {
    pub id: SiteShopItemId,
    pub product: BillingProductId,
    pub created_at: OffsetDateTime,
    /// The price list's answer now; `None` when the item has left the list
    /// (the public shop skips it, and the owner is told instead).
    pub item: Option<CatalogSaleItem>,
    /// Whole units a buyer could take right now, by the stock-sale seam's own
    /// arithmetic; `None` when the product is gone or no longer stocked.
    pub available_units: Option<i64>,
}

/// A product the shop could list: on the active price list, stocked, with
/// the shelf count a buyer would see. What the add-product picker offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteShopCandidate {
    pub item: CatalogSaleItem,
    /// Whole units a buyer could take right now — zero is still a candidate;
    /// an empty shelf is sold out, not unsellable.
    pub available_units: i64,
}

impl AccountStore {
    /// Lists a product on one of the tenant's sites' shops, after checking it
    /// against both owning seams: the price list must answer for it (the same
    /// door the shop will price it through), and Inventory must call it a
    /// stocked product (a service has no shelf, and wave two sells shelves).
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the product is not on the active price
    /// list or not a stocked one; [`StoreError::NotFound`] when the site is
    /// not this tenant's; [`StoreError::Conflict`] when the product is
    /// already listed or the shop is at [`SITE_SHOP_ITEM_MAX_PER_SITE`];
    /// [`StoreError::Db`] on failure.
    pub async fn add_site_shop_item(
        &self,
        site: &SiteId,
        product: &BillingProductId,
        now: OffsetDateTime,
    ) -> Result<SiteShopItemId> {
        let existing: Option<i64> = sqlx::query_scalar(
            "SELECT (SELECT count(*) FROM site_shop_items i \
                     WHERE i.tenant_id = s.tenant_id AND i.site_id = s.id) \
             FROM sites s WHERE s.tenant_id = $1 AND s.id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        let existing = existing.ok_or(StoreError::NotFound)?;
        if existing >= SITE_SHOP_ITEM_MAX_PER_SITE {
            return Err(StoreError::Conflict(format!(
                "a site's shop may list at most {SITE_SHOP_ITEM_MAX_PER_SITE} products"
            )));
        }
        // Checked after the site: a caller who cannot name the site is told
        // nothing about what is or is not on this tenant's price list.
        let catalog = BillingCatalogRead::open(
            self.pool.clone(),
            self.blobs.clone(),
            self.tenant.clone(),
            self.user.clone(),
        );
        if catalog.sale_item(product).await?.is_none() {
            return Err(StoreError::Validation(
                "that item is not on the price list".to_owned(),
            ));
        }
        let stock = InvStockSale::open(
            self.pool.clone(),
            self.blobs.clone(),
            self.tenant.clone(),
            self.user.clone(),
        );
        match stock.stock_for_sale(product, now).await? {
            Some(StockForSale::Stocked { .. }) => {}
            Some(StockForSale::NotStocked) => {
                return Err(StoreError::Validation(
                    "that item is not a stocked product; the shop sells from the shelf".to_owned(),
                ));
            }
            None => {
                return Err(StoreError::Validation(
                    "that item is not on the price list".to_owned(),
                ));
            }
        }
        let id = SiteShopItemId::generate();
        let inserted = sqlx::query(
            "INSERT INTO site_shop_items (tenant_id, site_id, id, product_id, created_at) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(id.as_str())
        .bind(product.as_str())
        .bind(now)
        .execute(&self.pool)
        .await;
        match inserted {
            Ok(_) => Ok(id),
            Err(sqlx::Error::Database(db))
                if db.constraint() == Some("site_shop_items_one_listing") =>
            {
                Err(StoreError::Conflict(
                    "that item is already on this site's shop".to_owned(),
                ))
            }
            Err(other) => Err(StoreError::Db(other)),
        }
    }

    /// A site's shop listings in listing order. A missing or foreign site
    /// simply has none.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn site_shop_items(&self, site: &SiteId) -> Result<Vec<SiteShopItem>> {
        let rows: Vec<(String, String, OffsetDateTime)> = sqlx::query_as(
            "SELECT id, product_id, created_at FROM site_shop_items \
             WHERE tenant_id = $1 AND site_id = $2 ORDER BY created_at, id",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows
            .into_iter()
            .map(|(id, product, created_at)| SiteShopItem {
                id: SiteShopItemId::new(id),
                product: BillingProductId::new(product),
                created_at,
            })
            .collect())
    }

    /// Takes a listing off the shop window. Orders already placed keep their
    /// own product reference, so delisting never touches a sale.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the listing is not this tenant's on this
    /// site; [`StoreError::Db`] on failure.
    pub async fn remove_site_shop_item(&self, site: &SiteId, item: &SiteShopItemId) -> Result<()> {
        let done = sqlx::query(
            "DELETE FROM site_shop_items WHERE tenant_id = $1 AND site_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(item.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// What the site charges for delivery, integer cents per order. A site
    /// that has never said ships for nothing — zero, like a shop whose
    /// counter you collect at.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn site_shop_shipping_cents(&self, site: &SiteId) -> Result<i64> {
        let cents: Option<i64> = sqlx::query_scalar(
            "SELECT shipping_cents FROM site_shop_settings WHERE tenant_id = $1 AND site_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(cents.unwrap_or(0))
    }

    /// Sets the site's flat delivery price per order.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the price is negative or over
    /// [`SHOP_SHIPPING_MAX_CENTS`]; [`StoreError::NotFound`] when the site is
    /// not this tenant's; [`StoreError::Db`] on failure.
    pub async fn set_site_shop_shipping_cents(&self, site: &SiteId, cents: i64) -> Result<()> {
        if !(0..=SHOP_SHIPPING_MAX_CENTS).contains(&cents) {
            return Err(StoreError::Validation(format!(
                "shipping must be between 0 and {SHOP_SHIPPING_MAX_CENTS} cents"
            )));
        }
        let owned: Option<i32> =
            sqlx::query_scalar("SELECT 1 FROM sites WHERE tenant_id = $1 AND id = $2")
                .bind(self.tenant.as_str())
                .bind(site.as_str())
                .fetch_optional(&self.pool)
                .await
                .map_err(StoreError::Db)?;
        if owned.is_none() {
            return Err(StoreError::NotFound);
        }
        sqlx::query(
            "INSERT INTO site_shop_settings (tenant_id, site_id, shipping_cents) \
             VALUES ($1, $2, $3) \
             ON CONFLICT (tenant_id, site_id) DO UPDATE \
                SET shipping_cents = EXCLUDED.shipping_cents, updated_at = now()",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(cents)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }

    /// The site's shop shelf as the owner's screen shows it: every listing in
    /// listing order, each resolved through the catalog and stock-sale seams
    /// at this read. One price-list read and one availability query serve the
    /// whole shelf — a full shop window must not cost 200 round trips.
    ///
    /// Returns the tenant's accounting currency alongside, so every price on
    /// the screen is expressed in the one currency Billing keeps books in.
    ///
    /// The caller resolves the site first (`require_site` on the wire): a
    /// missing or foreign site simply has an empty shelf here.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn site_shop_shelf(
        &self,
        site: &SiteId,
        now: OffsetDateTime,
    ) -> Result<(String, Vec<SiteShopShelfRow>)> {
        let rows = self.site_shop_items(site).await?;
        let catalog = BillingCatalogRead::open(
            self.pool.clone(),
            self.blobs.clone(),
            self.tenant.clone(),
            self.user.clone(),
        );
        let currency = catalog.currency().await?;
        let items = catalog.sale_items().await?;
        let shelves = self.stocked_shelf_counts(now).await?;
        Ok((
            currency,
            rows.into_iter()
                .map(|row| SiteShopShelfRow {
                    item: items.iter().find(|item| item.id == row.product).cloned(),
                    available_units: shelves.get(row.product.as_str()).copied(),
                    id: row.id,
                    product: row.product,
                    created_at: row.created_at,
                })
                .collect(),
        ))
    }

    /// The products a shop may list: the active price list narrowed to
    /// stocked items, each with the shelf count a buyer would see right now.
    /// An empty shelf is still offered — sold out is a state, not a refusal —
    /// and which of these a given site already lists is the caller's own
    /// shelf to check.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn site_shop_candidates(
        &self,
        now: OffsetDateTime,
    ) -> Result<(String, Vec<SiteShopCandidate>)> {
        let catalog = BillingCatalogRead::open(
            self.pool.clone(),
            self.blobs.clone(),
            self.tenant.clone(),
            self.user.clone(),
        );
        let currency = catalog.currency().await?;
        let items = catalog.sale_items().await?;
        let shelves = self.stocked_shelf_counts(now).await?;
        Ok((
            currency,
            items
                .into_iter()
                .filter_map(|item| {
                    shelves
                        .get(item.id.as_str())
                        .map(|units| SiteShopCandidate {
                            available_units: *units,
                            item,
                        })
                })
                .collect(),
        ))
    }

    /// Whole units a buyer could take of every stocked, active product of the
    /// tenant, keyed by product id — the stock-sale seam's arithmetic
    /// ([`available_units`]) computed in one statement instead of one per
    /// product. A product absent from the map is gone or not stocked.
    async fn stocked_shelf_counts(&self, now: OffsetDateTime) -> Result<HashMap<String, i64>> {
        let counts: Vec<(String, i64, i64)> = sqlx::query_as(
            "SELECT p.id, \
                    COALESCE((SELECT SUM(s.qty_milli) FROM inv_stock s \
                        JOIN inv_locations l \
                          ON l.tenant_id = s.tenant_id AND l.id = s.location_id \
                        WHERE s.tenant_id = p.tenant_id AND s.product_id = p.id \
                          AND l.kind = 'stock'), 0)::bigint, \
                    COALESCE((SELECT SUM(h.qty_milli) FROM inv_stock_sale_holds h \
                        WHERE h.tenant_id = p.tenant_id AND h.product_id = p.id \
                          AND h.state = 'held' AND h.expires_at > $2), 0)::bigint \
             FROM billing_products p \
             WHERE p.tenant_id = $1 AND p.stocked AND p.archived_at IS NULL",
        )
        .bind(self.tenant.as_str())
        .bind(now)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(counts
            .into_iter()
            .map(|(id, on_hand, held)| (id, available_units(on_hand, held)))
            .collect())
    }
}
