//! The public **stock checkout** of alo Sites: what an anonymous visitor may
//! learn about the goods a site sells, and the purchase they may start
//! (ADR 0041, item S3.05a2 — the store half; the shop pages on `alo-sites`
//! are the serving half, S3.05a3).
//!
//! Like every public door ([`crate::site_public_shop`],
//! [`crate::site_public_bookings`]), this module never takes a tenant from
//! outside: every read and write is anchored on a [`PublishedSite`] the Host
//! resolver produced, and a listing id is only ever looked up inside that
//! site. An unknown id, a foreign tenant's id, and another site's id on the
//! same tenant are all the same `Ok(None)`, which the wire turns into one
//! uniform 404.
//!
//! Every number a visitor sees is the owning module's answer *now*: the name
//! and the price come from Billing's catalog seam, the shelf count from
//! Inventory's stock-sale seam, and the delivery price from the site's own
//! stored rate — never a copy of any of them (ADR 0041). An item whose
//! product has left the price list, or stopped being stocked, simply is not
//! offered.
//!
//! The checkout drives the S3.05a1/a2 machinery in the only order that
//! cannot oversell: validate the buyer, **reserve the goods**, then record
//! the order against the hold. Settling is fetch-not-believe, exactly as the
//! ticket door demands: the webhook route resolves a provider payment id
//! through [`SitePublicStore::public_stock_payment_target`], asks the
//! *provider* where the payment stands, and applies that answer here.

use time::{Duration, OffsetDateTime};

use crate::error::{Result, StoreError};
use crate::id::{SiteShopItemId, SiteStockOrderId, UserId};
use crate::inv_stock_sale::{InvStockSale, StockForSale};
use crate::site_payments::SitePaymentStatus;
use crate::site_public::{PublishedSite, SitePublicStore};
use crate::site_public_shop::plausible;
use crate::site_stock_orders::{
    ShipTo, SiteStockOrderState, StockPaymentTarget, normalize_ship_to,
};
use crate::site_ticket_orders::{normalize_buyer_email, normalize_buyer_name};

/// How long a checkout may take: the life of the hold taken when a buyer
/// starts paying. The ticket door's half hour, well inside the stock seam's
/// one-hour ceiling.
pub const STOCK_CHECKOUT_HOLD_TTL: Duration = Duration::minutes(30);

/// One shop listing as a visitor may see it: the owning seams' answers now.
/// No shelf totals beyond what a buyer could take, no costs, no codes, no
/// buyer of any kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicStockItem {
    pub id: SiteShopItemId,
    /// What the price list calls the item, at this instant.
    pub name: String,
    /// Unit label ("piece"); empty for a unitless item.
    pub unit: String,
    /// What one unit costs the buyer, integer cents, VAT included — consumer
    /// prices are what is actually charged (S3.04d).
    pub unit_price_cents: i64,
    /// The currency of that price — the tenant's accounting currency.
    pub currency: String,
    /// Whole units a buyer could take right now. Zero reads "sold out".
    pub available_units: i64,
}

/// A checkout that has begun: the goods are reserved, the order is placed,
/// and this is everything the hosted-payment call needs — the order id
/// doubles as the provider's idempotency key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicStockCheckout {
    pub order: SiteStockOrderId,
    /// Whole units reserved and ordered.
    pub units: i64,
    /// What the buyer pays, integer cents, computed server-side — goods plus
    /// the site's delivery price.
    pub amount_cents: i64,
    /// The delivery part of that amount, for the checkout's own honesty.
    pub shipping_cents: i64,
    pub currency: String,
    /// What the payment is for, in the buyer's terms ("2 × Field guide").
    pub description: String,
    /// When the reserved goods lapse — the deadline the payment must beat.
    pub expires_at: OffsetDateTime,
}

/// Where one order stands, as its buyer's return page may see it. The state
/// and the handles forward — never the buyer's own details back out. The
/// failure sentence is included deliberately: when a payment could not be
/// honoured, the return page owes the buyer the honest words, and both
/// sentences this machinery writes are safe to show
/// ([`crate::site_stock_orders::STOCK_ORDER_PAID_AFTER_LAPSE`],
/// [`crate::site_stock_orders::STOCK_ORDER_GOODS_GONE`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicStockOrderStatus {
    pub state: SiteStockOrderState,
    pub units: i64,
    pub amount_cents: i64,
    pub shipping_cents: i64,
    pub currency: String,
    /// The provider's hosted page, while the order is still waiting on it —
    /// the "continue to payment" link.
    pub checkout_url: Option<String>,
    /// The provider's payment id, for the return page's own status fetch.
    /// Server-side only; a rendered page never carries it.
    pub provider_payment_id: Option<String>,
    /// Why the order stopped, when it stopped for a reason the buyer must
    /// hear.
    pub failure: Option<String>,
}

impl SitePublicStore {
    /// A published site's shop listings in listing order, each one the owning
    /// seams' answer *now* — priced by the catalog, counted by the ledger. A
    /// listing whose product no longer answers on both seams is not offered.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn public_stock_items(
        &self,
        site: &PublishedSite,
        now: OffsetDateTime,
    ) -> Result<Vec<PublicStockItem>> {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT id, product_id FROM site_shop_items \
              WHERE tenant_id = $1 AND site_id = $2 ORDER BY created_at, id",
        )
        .bind(site.tenant.as_str())
        .bind(site.site.as_str())
        .fetch_all(self.pool())
        .await
        .map_err(StoreError::Db)?;
        if rows.is_empty() {
            return Ok(Vec::new());
        }
        let Some(catalog) = self.catalog_door(site).await? else {
            return Ok(Vec::new());
        };
        let items = catalog.sale_items().await?;
        let currency = catalog.currency().await?;
        let Some(inv) = self.stock_door(site).await? else {
            return Ok(Vec::new());
        };
        let mut offers = Vec::with_capacity(rows.len());
        for (id, product_id) in rows {
            let Some(item) = items.iter().find(|item| item.id.as_str() == product_id) else {
                continue;
            };
            let product = crate::id::BillingProductId::new(product_id);
            let Some(StockForSale::Stocked { available_units }) =
                inv.stock_for_sale(&product, now).await?
            else {
                continue;
            };
            offers.push(PublicStockItem {
                id: SiteShopItemId::new(id),
                name: item.name.clone(),
                unit: item.unit.clone(),
                unit_price_cents: item.unit_price_cents,
                currency: currency.clone(),
                available_units,
            });
        }
        Ok(offers)
    }

    /// One shop listing of the published site, priced and counted now, or
    /// `None` — unknown, malformed, foreign, off the price list and no
    /// longer stocked are deliberately one answer.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn public_stock_item(
        &self,
        site: &PublishedSite,
        item_id: &str,
        now: OffsetDateTime,
    ) -> Result<Option<PublicStockItem>> {
        let Some(item_id) = plausible(item_id) else {
            return Ok(None);
        };
        let Some(product) = self.listed_product(site, item_id).await? else {
            return Ok(None);
        };
        let Some(catalog) = self.catalog_door(site).await? else {
            return Ok(None);
        };
        let Some(item) = catalog.sale_item(&product).await? else {
            return Ok(None);
        };
        let currency = catalog.currency().await?;
        let Some(inv) = self.stock_door(site).await? else {
            return Ok(None);
        };
        let Some(StockForSale::Stocked { available_units }) =
            inv.stock_for_sale(&product, now).await?
        else {
            return Ok(None);
        };
        Ok(Some(PublicStockItem {
            id: SiteShopItemId::new(item_id),
            name: item.name,
            unit: item.unit,
            unit_price_cents: item.unit_price_cents,
            currency,
            available_units,
        }))
    }

    /// What the published site charges for delivery, integer cents per
    /// order — the site's own stored rate, zero when it has never said.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn public_stock_shipping_cents(&self, site: &PublishedSite) -> Result<i64> {
        let cents: Option<i64> = sqlx::query_scalar(
            "SELECT shipping_cents FROM site_shop_settings WHERE tenant_id = $1 AND site_id = $2",
        )
        .bind(site.tenant.as_str())
        .bind(site.site.as_str())
        .fetch_optional(self.pool())
        .await
        .map_err(StoreError::Db)?;
        Ok(cents.unwrap_or(0))
    }

    /// Starts a purchase: validates the buyer and the address, **reserves the
    /// goods** for [`STOCK_CHECKOUT_HOLD_TTL`], and places the order against
    /// the hold — in that order, so nothing a visitor types can cost a
    /// reservation, and no two buyers can be sold the same last unit (the
    /// seam's lock decides).
    ///
    /// Returns `Ok(None)` when the listing is not this site's to sell
    /// (unknown, malformed, foreign — one answer).
    ///
    /// # Errors
    /// [`StoreError::Validation`] for a malformed name, email, address or
    /// unit count; [`StoreError::Conflict`] in the goods' own words — "sold
    /// out", "only N are left" — all safe to show the visitor;
    /// [`StoreError::Db`] on failure.
    #[allow(clippy::too_many_arguments)]
    pub async fn public_begin_stock_checkout(
        &self,
        site: &PublishedSite,
        item_id: &str,
        units: i64,
        buyer_name: &str,
        buyer_email: &str,
        ship_to: &ShipTo,
        now: OffsetDateTime,
    ) -> Result<Option<PublicStockCheckout>> {
        let Some(item_id) = plausible(item_id) else {
            return Ok(None);
        };
        // The typo gate runs before any goods are touched: a bad address must
        // never take (and then release) a hold real buyers are racing for.
        normalize_buyer_name(buyer_name)?;
        normalize_buyer_email(buyer_email)?;
        normalize_ship_to(ship_to)?;
        // The offer this checkout answers: the name the payment page will
        // describe, from the price list's answer now. An item that cannot be
        // offered cannot be bought.
        let Some(offer) = self.public_stock_item(site, item_id, now).await? else {
            return Ok(None);
        };
        let Some(product) = self.listed_product(site, item_id).await? else {
            return Ok(None);
        };
        let Some(owner) = self.shop_door(site).await? else {
            return Ok(None);
        };
        let Some(inv) = self.stock_door(site).await? else {
            return Ok(None);
        };
        let hold = match inv
            .reserve(&product, units, STOCK_CHECKOUT_HOLD_TTL, now)
            .await
        {
            Ok(hold) => hold,
            // The uniform miss: a product this site cannot sell answers
            // exactly like a product that never existed.
            Err(StoreError::NotFound) => return Ok(None),
            Err(other) => return Err(other),
        };
        let order = match owner
            .create_stock_order(&site.site, &hold.id, buyer_name, buyer_email, ship_to, now)
            .await
        {
            Ok(order) => order,
            Err(error) => {
                // The order never happened, so the goods go straight back on
                // sale rather than squatting out the TTL. Best effort: an
                // unreleased hold still frees itself at expiry.
                let _ = inv.release(&hold.id, now).await;
                return Err(error);
            }
        };
        let description = format!("{} × {}", order.units, offer.name);
        Ok(Some(PublicStockCheckout {
            order: order.id,
            units: order.units,
            amount_cents: order.amount_cents,
            shipping_cents: order.shipping_cents,
            currency: order.currency,
            description,
            expires_at: hold.expires_at,
        }))
    }

    /// Records the hosted payment the provider minted for a checkout this
    /// door began: the order moves to awaiting-payment with the provider's
    /// reference and checkout URL. Idempotent for a retried call with the
    /// same payment.
    ///
    /// Returns `Ok(None)` when the order is not this site's (unknown,
    /// malformed, foreign — one answer).
    ///
    /// # Errors
    /// [`StoreError::Validation`] for a malformed payment id or a checkout
    /// URL that is not https; [`StoreError::Conflict`] when the order is not
    /// open for this payment; [`StoreError::Db`] on failure.
    pub async fn public_open_stock_payment(
        &self,
        site: &PublishedSite,
        order: &SiteStockOrderId,
        provider_payment_id: &str,
        checkout_url: &str,
    ) -> Result<Option<()>> {
        let Some(door) = self.shop_door(site).await? else {
            return Ok(None);
        };
        match door
            .open_stock_payment(&site.site, order, provider_payment_id, checkout_url)
            .await
        {
            Ok(_) => Ok(Some(())),
            Err(StoreError::NotFound) => Ok(None),
            Err(other) => Err(other),
        }
    }

    /// Where one order stands, for the buyer's return page — resolved only on
    /// the site the order was placed on. Unknown, malformed and foreign order
    /// ids are one `None`. The buyer's own name and address are deliberately
    /// not in the answer: holding a return URL proves less than being the
    /// buyer.
    ///
    /// # Errors
    /// [`StoreError::Conflict`] when the stored state is unreadable;
    /// [`StoreError::Db`] on failure.
    pub async fn public_stock_order(
        &self,
        site: &PublishedSite,
        order_id: &str,
    ) -> Result<Option<PublicStockOrderStatus>> {
        let Some(order_id) = plausible(order_id) else {
            return Ok(None);
        };
        #[derive(sqlx::FromRow)]
        struct StatusRow {
            state: String,
            units: i64,
            amount_cents: i64,
            shipping_cents: i64,
            currency: String,
            checkout_url: Option<String>,
            provider_payment_id: Option<String>,
            failure: Option<String>,
        }
        let row: Option<StatusRow> = sqlx::query_as(
            "SELECT state, units, amount_cents, shipping_cents, currency, \
                    checkout_url, provider_payment_id, failure \
               FROM site_stock_orders \
              WHERE tenant_id = $1 AND site_id = $2 AND id = $3",
        )
        .bind(site.tenant.as_str())
        .bind(site.site.as_str())
        .bind(order_id)
        .fetch_optional(self.pool())
        .await
        .map_err(StoreError::Db)?;
        let Some(row) = row else { return Ok(None) };
        let state = SiteStockOrderState::parse(&row.state).ok_or_else(|| {
            StoreError::Conflict(format!(
                "stored order state '{}' is not one this release knows",
                row.state
            ))
        })?;
        Ok(Some(PublicStockOrderStatus {
            state,
            units: row.units,
            amount_cents: row.amount_cents,
            shipping_cents: row.shipping_cents,
            currency: row.currency,
            // A closed order's checkout link is dead weight at best and a
            // second charge at worst; only a waiting order still offers it.
            checkout_url: row.checkout_url.filter(|_| state.is_open()),
            provider_payment_id: row.provider_payment_id,
            failure: row.failure,
        }))
    }

    /// Which order a provider payment id belongs to — the webhook's entry
    /// point, host-independent because a webhook has no Host worth trusting.
    /// A payment id nobody holds answers `None`: an unauthenticated probe
    /// learns nothing.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn public_stock_payment_target(
        &self,
        provider_payment_id: &str,
    ) -> Result<Option<StockPaymentTarget>> {
        crate::store::Store::new(self.pool().clone(), self.blobs().clone())
            .stock_payment_target(provider_payment_id)
            .await
    }

    /// Settles the targeted order with a payment status **fetched from the
    /// provider** — never one a webhook body asserted. Idempotent
    /// throughout; every outcome (paid, paid-too-late, goods gone, dead) is
    /// the order machinery's decision ([`crate::site_stock_orders`]).
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the target's rows are gone;
    /// [`StoreError::Db`] on failure.
    pub async fn public_settle_stock_payment(
        &self,
        target: &StockPaymentTarget,
        status: SitePaymentStatus,
        now: OffsetDateTime,
    ) -> Result<()> {
        let owner: Option<String> =
            sqlx::query_scalar("SELECT created_by FROM sites WHERE tenant_id = $1 AND id = $2")
                .bind(target.tenant.as_str())
                .bind(target.site.as_str())
                .fetch_optional(self.pool())
                .await
                .map_err(StoreError::Db)?;
        let owner = owner.ok_or(StoreError::NotFound)?;
        let door = crate::account::AccountStore {
            pool: self.pool().clone(),
            blobs: self.blobs().clone(),
            tenant: target.tenant.clone(),
            user: UserId::new(owner),
        };
        door.apply_stock_payment(&target.site, &target.order, status, now)
            .await?;
        Ok(())
    }

    /// The product one of this site's own listings references, or `None` —
    /// the anchor every read and write above starts from.
    async fn listed_product(
        &self,
        site: &PublishedSite,
        item_id: &str,
    ) -> Result<Option<crate::id::BillingProductId>> {
        let product: Option<String> = sqlx::query_scalar(
            "SELECT product_id FROM site_shop_items \
              WHERE tenant_id = $1 AND site_id = $2 AND id = $3",
        )
        .bind(site.tenant.as_str())
        .bind(site.site.as_str())
        .bind(item_id)
        .fetch_optional(self.pool())
        .await
        .map_err(StoreError::Db)?;
        Ok(product.map(crate::id::BillingProductId::new))
    }

    /// Inventory's stock-sale seam of the published site's tenant, opened as
    /// the site's owner — the same `(tenant, owner)` handshake every seam
    /// door uses, both halves read from the site's own row. `None` when the
    /// site row is gone from under its publish.
    async fn stock_door(&self, site: &PublishedSite) -> Result<Option<InvStockSale>> {
        let owner: Option<String> =
            sqlx::query_scalar("SELECT created_by FROM sites WHERE tenant_id = $1 AND id = $2")
                .bind(site.tenant.as_str())
                .bind(site.site.as_str())
                .fetch_optional(self.pool())
                .await
                .map_err(StoreError::Db)?;
        Ok(owner.map(|owner| {
            InvStockSale::open(
                self.pool().clone(),
                self.blobs().clone(),
                site.tenant.clone(),
                UserId::new(owner),
            )
        }))
    }
}
