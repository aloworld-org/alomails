//! Billing's public catalog seam — the one thing a site's shop may read from
//! the price list (ADR 0041).
//!
//! The shop is a surface, not a system: it renders the **same** catalog
//! Billing sells from, and the moment it keeps a copy of a price there are two
//! numbers that can disagree and the whole argument for building rather than
//! integrating is gone. So a site stores *references* — a product id in a shop
//! section — and asks here, at render and at sale, what the item is called and
//! what it costs **now**. There is deliberately no write method on this door
//! and no table behind it: it reads Billing's own rows through Billing's own
//! reads, and nothing else.
//!
//! This file belongs to Billing, not to Sites. What an item is, what it costs,
//! and when it stops being sold stay Billing's to decide, in Billing's own
//! words: the reads ride [`AccountStore::billing_products`] and
//! [`AccountStore::billing_product`], so the price list's ordering and the
//! archive rule (`docs/design/billing.md` — archived, never deleted) are
//! decided once, there. An archived item simply stops answering, exactly as it
//! drops out of Billing's own pickers — a shop can never sell the past.
//!
//! What the seam will not carry is as deliberate as what it will:
//! [`CatalogSaleItem`] is the whole of the vocabulary, and it has no field for
//! what the tenant *pays* ([`purchase_price_cents`]), who they buy from, their
//! internal codes, or any workspace identity — so none of it can reach a
//! public surface whatever the calling code does. The conversion in
//! [`sale_item_of`] destructures [`Product`] exhaustively on purpose: a column
//! Billing adds tomorrow does not cross this seam until somebody decides it
//! should.
//!
//! Scoping: the door is opened with a `(tenant, owner)` pair the caller must
//! already have resolved from its own trusted row (for Sites, the site's
//! record — never a request), the same handshake every seam door uses
//! ([`crate::calendar_availability`], [`crate::crm_lead_capture`]). Billing's
//! price list is tenant-wide, so today the reads turn on the tenant alone; the
//! owner names who the door acts as, and is what any later owner-scoped read
//! would be held to.
//!
//! [`purchase_price_cents`]: Product::purchase_price_cents

use sqlx::PgPool;

use crate::account::AccountStore;
use crate::billing_products::Product;
use crate::blob::BlobStore;
use crate::error::Result;
use crate::id::{BillingProductId, DriveNodeId, TenantId, UserId};

/// What Billing will say about one item across this seam — the buyer's facts,
/// and nothing else. The price is integer cents in the tenant's accounting
/// currency ([`BillingCatalogRead::currency`]); the VAT rate is basis points,
/// carried as Billing holds it so the tax rule (S3.04e) works from the real
/// figure rather than a rendering of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogSaleItem {
    /// Billing's own id — the reference a shop section stores *instead of* a
    /// copy of any of the fields below.
    pub id: BillingProductId,
    /// What the item is called on the price list, and so on the shelf.
    pub name: String,
    /// Unit label ("hour", "piece"); empty for a unitless item.
    pub unit: String,
    /// What one unit costs the buyer, in integer cents.
    pub unit_price_cents: i64,
    /// VAT rate in basis points (2100 = 21 %).
    pub vat_rate_bp: i32,
    /// The product photo in Drive, referenced and never copied — the card
    /// image, when the tenant attached one.
    pub photo_node_id: Option<DriveNodeId>,
}

/// A read-only door onto one tenant's price list that can only answer *what is
/// sold, and for how much*. Open it with a tenant and owner resolved from a
/// trusted row; everything it reads is scoped to that tenant.
pub struct BillingCatalogRead {
    account: AccountStore,
}

impl BillingCatalogRead {
    /// Opens the catalog door of one tenant's Billing.
    ///
    /// The caller vouches for the pair: `tenant` and `owner` must come from a
    /// row the caller already trusts (a site's own record, never a request).
    #[must_use]
    pub fn open(pool: PgPool, blobs: BlobStore, tenant: TenantId, owner: UserId) -> Self {
        Self {
            account: AccountStore {
                pool,
                blobs,
                tenant,
                user: owner,
            },
        }
    }

    /// The tenant's sellable items in name order — the active price list,
    /// exactly as Billing's own pickers see it. In Billing's catalog the only
    /// publish state is archival, so active *is* sellable; which of these a
    /// site actually offers is the shop section's naming, stored as
    /// references.
    ///
    /// # Errors
    /// [`StoreError::Db`](crate::error::StoreError::Db) on failure.
    pub async fn sale_items(&self) -> Result<Vec<CatalogSaleItem>> {
        Ok(self
            .account
            .billing_products(false)
            .await?
            .into_iter()
            .map(sale_item_of)
            .collect())
    }

    /// One sellable item, or `None` — including when the id is archived,
    /// another tenant's, or was never anything (indistinguishable by design,
    /// so a stale or guessed reference shows nothing rather than selling the
    /// past or leaking that a foreign id exists).
    ///
    /// # Errors
    /// [`StoreError::Db`](crate::error::StoreError::Db) on failure.
    pub async fn sale_item(&self, id: &BillingProductId) -> Result<Option<CatalogSaleItem>> {
        Ok(self
            .account
            .billing_product(id)
            .await?
            .filter(|product| !product.is_archived())
            .map(sale_item_of))
    }

    /// The currency every price this door answers is expressed in — the
    /// tenant's accounting currency, read from Billing's own settings so the
    /// shop never keeps a currency of its own. Never blank: a tenant that has
    /// not said otherwise keeps books in
    /// [`DEFAULT_CURRENCY`](crate::billing_field::DEFAULT_CURRENCY).
    ///
    /// # Errors
    /// [`StoreError::Db`](crate::error::StoreError::Db) on failure.
    pub async fn currency(&self) -> Result<String> {
        self.account.billing_base_currency().await
    }
}

/// The buyer's view of a product. Exhaustive on purpose — every withheld field
/// is withheld by name, and a field Billing adds tomorrow fails to compile
/// here until somebody decides whether it crosses.
fn sale_item_of(product: Product) -> CatalogSaleItem {
    let Product {
        id,
        name,
        unit,
        unit_price_cents,
        vat_rate_bp,
        // The tenant's own codes are for their warehouse, not their buyers.
        sku: _,
        barcode: _,
        // How stock crosses is wave two's decision (S3.05a), through
        // Inventory's own seam — never a copied count here.
        stocked: _,
        // What the tenant pays, and who they buy from, stay inside.
        purchase_price_cents: _,
        default_supplier_id: _,
        // Callers only ever see active items; the reads above filter.
        archived_at: _,
        // Workspace identities and bookkeeping never reach a public surface.
        created_by: _,
        created_at: _,
        updated_at: _,
        photo_node_id,
    } = product;
    CatalogSaleItem {
        id,
        name,
        unit,
        unit_price_cents,
        vat_rate_bp,
        photo_node_id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::OffsetDateTime;

    /// A product carrying every private fact the seam must withhold.
    fn chair() -> Product {
        Product {
            id: BillingProductId::new("prod-chair".to_owned()),
            name: "Blue chair".to_owned(),
            unit: "piece".to_owned(),
            unit_price_cents: 4_900,
            vat_rate_bp: 2100,
            sku: "CH-BLUE-01".to_owned(),
            barcode: "4006381333931".to_owned(),
            stocked: true,
            purchase_price_cents: 2_150,
            photo_node_id: Some(DriveNodeId::new("node-photo".to_owned())),
            default_supplier_id: None,
            archived_at: None,
            created_by: "user-owner".to_owned(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn the_vocabulary_is_the_buyers_facts_and_nothing_else() {
        // Exhaustive: these six fields are the whole of what can cross. A
        // field added to CatalogSaleItem fails to compile here until this
        // proof names it.
        let CatalogSaleItem {
            id,
            name,
            unit,
            unit_price_cents,
            vat_rate_bp,
            photo_node_id,
        } = sale_item_of(chair());
        assert_eq!(id.as_str(), "prod-chair");
        assert_eq!(name, "Blue chair");
        assert_eq!(unit, "piece");
        assert_eq!(unit_price_cents, 4_900);
        assert_eq!(vat_rate_bp, 2100);
        assert_eq!(
            photo_node_id.as_ref().map(DriveNodeId::as_str),
            Some("node-photo")
        );
    }

    #[test]
    fn the_price_that_crosses_is_the_buyers_not_the_tenants() {
        let item = sale_item_of(chair());
        // The sale price crosses exactly; the purchase price has no field to
        // travel in, so the strongest claim available is that the one money
        // figure on the item is the sale figure.
        assert_eq!(item.unit_price_cents, 4_900);
        let rendered = format!("{item:?}");
        assert!(
            !rendered.contains("2150") && !rendered.contains("2_150"),
            "the tenant's cost leaked into the item: {rendered}"
        );
        assert!(
            !rendered.contains("CH-BLUE-01") && !rendered.contains("4006381333931"),
            "an internal code leaked into the item: {rendered}"
        );
        assert!(
            !rendered.contains("user-owner"),
            "a workspace identity leaked into the item: {rendered}"
        );
    }
}
