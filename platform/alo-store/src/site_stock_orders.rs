//! The stock order — order → payment-reference → paid, where "paid" claims
//! real goods off a real shelf (ADR 0041, item S3.05a2).
//!
//! Inventory's stock-sale hold ([`crate::inv_stock_sale`]) is pure quantity
//! accounting and deliberately holds no buyer; this module is where the buyer
//! lives. An order records who is paying, where the goods go, and the price
//! they were shown — the price list's answer plus the site's own flat
//! shipping rate at the moment the order was placed, computed server-side and
//! stored as the record of the sale. The VAT rate snapshotted is the goods'
//! rate, and the shipping follows it: an ancillary cost takes the main
//! supply's treatment.
//!
//! The state machine and its three rules are the ticket order's
//! ([`crate::site_ticket_orders`]), because they were never about tickets:
//! a retry never sells twice (one order per hold; the order id is the
//! provider call's idempotency key; a settle applied twice returns the same
//! row), a webhook is a doorbell (the status that settles is fetched from the
//! provider, never read from the webhook body), and money that moved is never
//! silently lost. What is new here is what "paid" *does*: settling a stock
//! order claims the hold through Inventory's own seam, which records the real
//! outbound movement — so a sale is a shelf count dropping, not a flag.
//!
//! That claim can honestly fail, and the failure is the point of this item:
//! the warehouse's own doors do not honour shop holds (Inventory's documented
//! decision), so goods can leave stock under a live hold. A payment whose
//! goods are gone — or whose hold lapsed before the money arrived — closes
//! the order **visibly**, its [`failure`](SiteStockOrder::failure) naming the
//! refund, and never records a movement of goods that are not there. A silent
//! oversell is the one outcome this module makes impossible.

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::billing_catalog_read::BillingCatalogRead;
use crate::error::{Result, StoreError};
use crate::id::{BillingProductId, InvStockHoldId, SiteId, SiteStockOrderId, TenantId};
use crate::inv_stock_sale::{InvStockHoldState, InvStockSale};
use crate::site_domain_purchases::validate_payment_reference;
use crate::site_payments::{SitePaymentError, SitePaymentStatus, validate_payment_url};
use crate::site_ticket_orders::{normalize_buyer_email, normalize_buyer_name};
use crate::store::Store;

/// Most orders one list read returns.
pub const MAX_SITE_STOCK_ORDERS: i64 = 50;

/// Longest address line, city and postcode an order accepts.
pub const SHIP_TO_LINE_MAX_CHARS: usize = 200;
pub const SHIP_TO_CITY_MAX_CHARS: usize = 100;
pub const SHIP_TO_POSTCODE_MAX_CHARS: usize = 20;

/// What an order says when the money arrived after the hold had lapsed or
/// been released. The sentence names the refund: this is a support
/// conversation, not a sale.
pub const STOCK_ORDER_PAID_AFTER_LAPSE: &str = "the payment was received after the stock hold had lapsed; \
     the goods may have been resold — this payment needs a refund";

/// What an order says when the warehouse's own doors took the goods before
/// the sale could claim them. Honest scarcity, never a movement of goods that
/// are not there.
pub const STOCK_ORDER_GOODS_GONE: &str = "the goods left stock before this sale could be completed; \
     nothing has shipped — this payment needs a refund";

/// Where one order stands. Same words, stored tokens and openness rule as
/// the ticket order's — a return page can treat both alike.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiteStockOrderState {
    /// Placed; no payment provider has been asked yet.
    Pending,
    /// The provider minted a payment; the buyer is on its hosted page.
    AwaitingPayment,
    /// The money moved and the goods were claimed off the shelf. A sale,
    /// forever.
    Paid,
    /// The payment failed, or its goods could not be claimed
    /// ([`SiteStockOrder::failure`] says which, when it needs saying).
    Failed,
    /// The buyer cancelled on the provider's page.
    Cancelled,
    /// The provider's checkout lapsed before the buyer finished.
    Expired,
}

impl SiteStockOrderState {
    /// The stable token this state is stored and named by on the wire.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::AwaitingPayment => "awaiting_payment",
            Self::Paid => "paid",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Expired => "expired",
        }
    }

    /// Reads a stored token back.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "awaiting_payment" => Some(Self::AwaitingPayment),
            "paid" => Some(Self::Paid),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            "expired" => Some(Self::Expired),
            _ => None,
        }
    }

    /// Whether this order is still on its way to being a sale.
    #[must_use]
    pub fn is_open(self) -> bool {
        matches!(self, Self::Pending | Self::AwaitingPayment)
    }
}

/// Where the goods go — the buyer's own statement, validated as a typo gate,
/// never verified as a deliverable address (the tenant ships, alo records).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShipTo {
    /// Street and number, one line.
    pub line: String,
    pub city: String,
    pub postcode: String,
    /// ISO 3166-1 alpha-2, uppercase.
    pub country: String,
}

/// One stock order, as the tenant sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteStockOrder {
    pub id: SiteStockOrderId,
    /// Billing's id for what was sold — resolve through the catalog seam for
    /// the name *now*; the price below is the one that was struck.
    pub product: BillingProductId,
    /// The Inventory hold whose goods this order buys — also the creation's
    /// replay token: one hold is one order, ever.
    pub hold: InvStockHoldId,
    /// Whole units bought.
    pub units: i64,
    pub buyer_name: String,
    pub buyer_email: String,
    pub ship_to: ShipTo,
    /// One unit's price as it was struck, integer cents.
    pub unit_price_cents: i64,
    /// The site's flat delivery price as it stood, integer cents.
    pub shipping_cents: i64,
    /// What the buyer pays: `units × unit_price_cents + shipping_cents`,
    /// computed server-side at creation.
    pub amount_cents: i64,
    /// VAT rate in basis points — the goods' rate, which the shipping
    /// follows.
    pub vat_rate_bp: i32,
    /// The tenant's accounting currency at the moment of sale.
    pub currency: String,
    pub state: SiteStockOrderState,
    /// The provider's opaque id for the hosted payment. Never parsed here.
    pub provider_payment_id: Option<String>,
    /// The provider's hosted page — where the buyer pays.
    pub checkout_url: Option<String>,
    /// Why this order stopped, in words the tenant can act on.
    pub failure: Option<String>,
    pub paid_at: Option<OffsetDateTime>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

/// Which tenant's door a provider payment id belongs behind — the whole of
/// what the webhook's unauthenticated caller can learn from alo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StockPaymentTarget {
    /// The tenant whose order this is — resolved from the row itself, never
    /// from anything the webhook claimed.
    pub tenant: TenantId,
    pub site: SiteId,
    pub order: SiteStockOrderId,
}

const ORDER_COLUMNS: &str = "id, product_id, hold_id, units, buyer_name, buyer_email, \
     ship_to_line, ship_to_city, ship_to_postcode, ship_to_country, \
     unit_price_cents, shipping_cents, amount_cents, vat_rate_bp, currency, state, \
     provider_payment_id, checkout_url, failure, paid_at, created_at, updated_at";

#[derive(sqlx::FromRow)]
struct OrderRow {
    id: String,
    product_id: String,
    hold_id: String,
    units: i64,
    buyer_name: String,
    buyer_email: String,
    ship_to_line: String,
    ship_to_city: String,
    ship_to_postcode: String,
    ship_to_country: String,
    unit_price_cents: i64,
    shipping_cents: i64,
    amount_cents: i64,
    vat_rate_bp: i32,
    currency: String,
    state: String,
    provider_payment_id: Option<String>,
    checkout_url: Option<String>,
    failure: Option<String>,
    paid_at: Option<OffsetDateTime>,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl OrderRow {
    fn state(&self) -> Result<SiteStockOrderState> {
        SiteStockOrderState::parse(&self.state).ok_or_else(|| {
            StoreError::Conflict(format!(
                "stored order state '{}' is not one this release knows",
                self.state
            ))
        })
    }

    fn into_order(self) -> Result<SiteStockOrder> {
        let state = self.state()?;
        Ok(SiteStockOrder {
            id: SiteStockOrderId::new(self.id),
            product: BillingProductId::new(self.product_id),
            hold: InvStockHoldId::new(self.hold_id),
            units: self.units,
            buyer_name: self.buyer_name,
            buyer_email: self.buyer_email,
            ship_to: ShipTo {
                line: self.ship_to_line,
                city: self.ship_to_city,
                postcode: self.ship_to_postcode,
                country: self.ship_to_country,
            },
            unit_price_cents: self.unit_price_cents,
            shipping_cents: self.shipping_cents,
            amount_cents: self.amount_cents,
            vat_rate_bp: self.vat_rate_bp,
            currency: self.currency,
            state,
            provider_payment_id: self.provider_payment_id,
            checkout_url: self.checkout_url,
            failure: self.failure,
            paid_at: self.paid_at,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

/// The address typo gate: trimmed, bounded, printable, the country a real
/// two-letter code shape — enough to catch a slip, never a claim the door
/// exists.
pub(crate) fn normalize_ship_to(input: &ShipTo) -> Result<ShipTo> {
    let field = |value: &str, what: &str, max: usize| -> Result<String> {
        let value = value.trim();
        if value.is_empty() {
            return Err(StoreError::Validation(format!("{what} must not be empty")));
        }
        if value.chars().count() > max {
            return Err(StoreError::Validation(format!(
                "{what} must be at most {max} characters"
            )));
        }
        if value.chars().any(char::is_control) {
            return Err(StoreError::Validation(format!(
                "{what} may not contain control characters"
            )));
        }
        Ok(value.to_owned())
    };
    let country = input.country.trim();
    if country.len() != 2 || !country.bytes().all(|b| b.is_ascii_alphabetic()) {
        return Err(StoreError::Validation(
            "country must be a two-letter code".to_owned(),
        ));
    }
    Ok(ShipTo {
        line: field(&input.line, "address", SHIP_TO_LINE_MAX_CHARS)?,
        city: field(&input.city, "city", SHIP_TO_CITY_MAX_CHARS)?,
        postcode: field(&input.postcode, "postcode", SHIP_TO_POSTCODE_MAX_CHARS)?,
        country: country.to_ascii_uppercase(),
    })
}

/// Maps this table's unique indexes onto sentences.
fn map_order_unique(error: sqlx::Error) -> StoreError {
    if let sqlx::Error::Database(ref database) = error {
        match database.constraint() {
            Some("site_stock_orders_one_per_hold") => {
                return StoreError::Conflict("those goods already have an order".to_owned());
            }
            Some("site_stock_orders_one_payment") => {
                return StoreError::Conflict(
                    "that payment already belongs to another order".to_owned(),
                );
            }
            _ => {}
        }
    }
    error.into()
}

/// One order row, locked for the length of the transaction so two settles of
/// the same order cannot interleave.
async fn locked_order(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant: &str,
    site: &SiteId,
    order: &SiteStockOrderId,
) -> Result<OrderRow> {
    sqlx::query_as::<_, OrderRow>(&format!(
        "SELECT {ORDER_COLUMNS} FROM site_stock_orders \
          WHERE tenant_id = $1 AND site_id = $2 AND id = $3 FOR UPDATE"
    ))
    .bind(tenant)
    .bind(site.as_str())
    .bind(order.as_str())
    .fetch_optional(&mut **tx)
    .await
    .map_err(StoreError::Db)?
    .ok_or(StoreError::NotFound)
}

impl AccountStore {
    /// Places an order for the goods one live Inventory hold reserves: who is
    /// buying, where it ships, and the price they are shown — the price
    /// list's answer plus the site's own shipping rate, read at this instant
    /// and stored as the record of the sale. The product must still be on
    /// this site's shop: a hold is tenant-wide, and the listing is what
    /// anchors the sale to the site.
    ///
    /// Idempotent under the hold: one hold is one order, so a double-clicked
    /// buy button reaches the order it already made (same buyer), and a
    /// different buyer on the same hold is refused rather than quietly
    /// replaced.
    ///
    /// # Errors
    /// [`StoreError::Validation`] for a malformed name, email or address;
    /// [`StoreError::NotFound`] when the site is not this tenant's or the
    /// hold is not this tenant's; [`StoreError::Conflict`] when the hold is
    /// not live, the product left this site's shop or the price list, or the
    /// hold's order belongs to a different buyer; [`StoreError::Db`] on
    /// failure.
    #[allow(clippy::too_many_lines)]
    pub async fn create_stock_order(
        &self,
        site: &SiteId,
        hold: &InvStockHoldId,
        buyer_name: &str,
        buyer_email: &str,
        ship_to: &ShipTo,
        now: OffsetDateTime,
    ) -> Result<SiteStockOrder> {
        let buyer_name = normalize_buyer_name(buyer_name)?;
        let buyer_email = normalize_buyer_email(buyer_email)?;
        let ship_to = normalize_ship_to(ship_to)?;
        // Anchored at the site first: a caller who cannot name the site
        // learns nothing — not the hold's existence, not the price list's
        // answer.
        let stood = self
            .stock_sale_door()
            .stock_hold(hold, now)
            .await?
            .ok_or(StoreError::NotFound)?;
        match stood.state {
            InvStockHoldState::Held => {}
            InvStockHoldState::Expired => {
                return Err(StoreError::Conflict(
                    "this hold has expired; reserve the goods again before ordering".to_owned(),
                ));
            }
            InvStockHoldState::Released => {
                return Err(StoreError::Conflict(
                    "this hold was released; reserve the goods again before ordering".to_owned(),
                ));
            }
            InvStockHoldState::Completed => {
                return Err(StoreError::Conflict(
                    "these goods have already been bought".to_owned(),
                ));
            }
        }
        let listed: Option<i64> = sqlx::query_scalar(
            "SELECT (SELECT count(*) FROM site_shop_items i \
                     WHERE i.tenant_id = s.tenant_id AND i.site_id = s.id \
                       AND i.product_id = $3) \
             FROM sites s WHERE s.tenant_id = $1 AND s.id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(stood.product.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        let listed = listed.ok_or(StoreError::NotFound)?;
        if listed == 0 {
            return Err(StoreError::Conflict(
                "this item is no longer on this site's shop".to_owned(),
            ));
        }
        // The price is the price list's answer, through the same seam the
        // shop renders from — never a figure the request carried. Shipping is
        // the site's own stored rate, read the same way.
        let door = BillingCatalogRead::open(
            self.pool.clone(),
            self.blobs.clone(),
            self.tenant.clone(),
            self.user.clone(),
        );
        let item = door.sale_item(&stood.product).await?.ok_or_else(|| {
            StoreError::Conflict("this item is no longer on the price list".to_owned())
        })?;
        let currency = door.currency().await?;
        let shipping_cents = self.site_shop_shipping_cents(site).await?;
        let goods_cents = stood
            .units
            .checked_mul(item.unit_price_cents)
            .ok_or_else(|| StoreError::Validation("that amount is not a price".to_owned()))?;
        let amount_cents = goods_cents
            .checked_add(shipping_cents)
            .ok_or_else(|| StoreError::Validation("that amount is not a price".to_owned()))?;
        let id = SiteStockOrderId::generate();
        let inserted = sqlx::query_as::<_, OrderRow>(&format!(
            "INSERT INTO site_stock_orders \
                (tenant_id, site_id, id, product_id, hold_id, units, \
                 buyer_name, buyer_email, \
                 ship_to_line, ship_to_city, ship_to_postcode, ship_to_country, \
                 unit_price_cents, shipping_cents, amount_cents, \
                 vat_rate_bp, currency, created_at, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, \
                     $13, $14, $15, $16, $17, $18, $18) \
          RETURNING {ORDER_COLUMNS}"
        ))
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(id.as_str())
        .bind(stood.product.as_str())
        .bind(hold.as_str())
        .bind(stood.units)
        .bind(&buyer_name)
        .bind(&buyer_email)
        .bind(&ship_to.line)
        .bind(&ship_to.city)
        .bind(&ship_to.postcode)
        .bind(&ship_to.country)
        .bind(item.unit_price_cents)
        .bind(shipping_cents)
        .bind(amount_cents)
        .bind(item.vat_rate_bp)
        .bind(&currency)
        .bind(now)
        .fetch_one(&self.pool)
        .await;
        match inserted {
            Ok(row) => row.into_order(),
            Err(error) => {
                let mapped = map_order_unique(error);
                // One order per hold: a replay by the same buyer reaches the
                // order it already made.
                if let StoreError::Conflict(_) = &mapped {
                    let existing = sqlx::query_as::<_, OrderRow>(&format!(
                        "SELECT {ORDER_COLUMNS} FROM site_stock_orders \
                          WHERE tenant_id = $1 AND site_id = $2 AND hold_id = $3"
                    ))
                    .bind(self.tenant.as_str())
                    .bind(site.as_str())
                    .bind(hold.as_str())
                    .fetch_optional(&self.pool)
                    .await
                    .map_err(StoreError::Db)?;
                    if let Some(row) = existing {
                        if row.buyer_name == buyer_name && row.buyer_email == buyer_email {
                            return row.into_order();
                        }
                        return Err(StoreError::Conflict(
                            "those goods already have an order for a different buyer".to_owned(),
                        ));
                    }
                }
                Err(mapped)
            }
        }
    }

    /// Records the hosted payment the provider minted for an order: its
    /// opaque id and where its checkout lives, moving the order to
    /// [`AwaitingPayment`](SiteStockOrderState::AwaitingPayment).
    ///
    /// Repeating the call with the same payment returns the row unchanged; a
    /// *different* payment for an order already waiting is refused — one
    /// order, one payment.
    ///
    /// # Errors
    /// [`StoreError::Validation`] for a malformed payment id or a checkout
    /// URL that is not https; [`StoreError::NotFound`] when the order is not
    /// this tenant's on this site; [`StoreError::Conflict`] when the order is
    /// not open for a payment or the payment settles another order;
    /// [`StoreError::Db`] on failure.
    pub async fn open_stock_payment(
        &self,
        site: &SiteId,
        order: &SiteStockOrderId,
        provider_payment_id: &str,
        checkout_url: &str,
    ) -> Result<SiteStockOrder> {
        let reference = validate_payment_reference(provider_payment_id)?;
        validate_payment_url(checkout_url, "checkout").map_err(|error| match error {
            SitePaymentError::Validation(message) => StoreError::Validation(message),
            other => StoreError::Validation(other.to_string()),
        })?;
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let row = locked_order(&mut tx, self.tenant.as_str(), site, order).await?;
        match row.state()? {
            SiteStockOrderState::Pending => {}
            SiteStockOrderState::AwaitingPayment => {
                if row.provider_payment_id.as_deref() == Some(reference.as_str()) {
                    tx.commit().await.map_err(StoreError::Db)?;
                    return row.into_order();
                }
                return Err(StoreError::Conflict(
                    "this order is already waiting for another payment".to_owned(),
                ));
            }
            SiteStockOrderState::Paid => {
                return Err(StoreError::Conflict(
                    "this order has already been paid".to_owned(),
                ));
            }
            SiteStockOrderState::Failed
            | SiteStockOrderState::Cancelled
            | SiteStockOrderState::Expired => {
                return Err(StoreError::Conflict(
                    "this order is closed; place a new one".to_owned(),
                ));
            }
        }
        let row = sqlx::query_as::<_, OrderRow>(&format!(
            "UPDATE site_stock_orders \
                SET state = 'awaiting_payment', provider_payment_id = $4, \
                    checkout_url = $5, updated_at = now() \
              WHERE tenant_id = $1 AND site_id = $2 AND id = $3 \
          RETURNING {ORDER_COLUMNS}"
        ))
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(order.as_str())
        .bind(&reference)
        .bind(checkout_url)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_order_unique)?;
        tx.commit().await.map_err(StoreError::Db)?;
        row.into_order()
    }

    /// Settles an order with a payment status **fetched from the provider** —
    /// the webhook made us look, this is what we saw. Idempotent throughout:
    /// the same status applied twice returns the same row, and a webhook
    /// replayed five times is one sale and one movement (the claim's own
    /// idempotency guarantees the goods move once).
    ///
    /// [`Paid`](SitePaymentStatus::Paid) claims the hold through Inventory's
    /// stock-sale seam, which records the real outbound movement — then marks
    /// the order paid. The two honest failures both close the order visibly,
    /// naming the refund, and move nothing: a payment confirmed after the
    /// hold lapsed ([`STOCK_ORDER_PAID_AFTER_LAPSE`]), and goods the
    /// warehouse's own doors took first ([`STOCK_ORDER_GOODS_GONE`] — the
    /// hold is released so what remains goes back on sale). The dead statuses
    /// (failed, canceled, expired) close the order and release the goods. A
    /// paid order is never unsold by a late status of any kind.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the order is not this tenant's on this
    /// site; [`StoreError::Conflict`] when a stored state is unreadable;
    /// [`StoreError::Db`] on failure.
    pub async fn apply_stock_payment(
        &self,
        site: &SiteId,
        order: &SiteStockOrderId,
        status: SitePaymentStatus,
        now: OffsetDateTime,
    ) -> Result<SiteStockOrder> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let row = locked_order(&mut tx, self.tenant.as_str(), site, order).await?;
        let state = row.state()?;
        // A sale is a sale: no later status, of any kind, unsells it.
        if state == SiteStockOrderState::Paid {
            tx.commit().await.map_err(StoreError::Db)?;
            return row.into_order();
        }
        let inv = self.stock_sale_door();
        let hold = InvStockHoldId::new(row.hold_id.clone());
        let outcome = match status {
            // The payment is still underway; there is nothing to record.
            SitePaymentStatus::Open => {
                tx.commit().await.map_err(StoreError::Db)?;
                return row.into_order();
            }
            SitePaymentStatus::Paid => {
                let stood = inv
                    .stock_hold(&hold, now)
                    .await?
                    .ok_or(StoreError::NotFound)?;
                match stood.state {
                    // A crash between claim and this write: the movement was
                    // recorded; finishing the order is all that is left.
                    InvStockHoldState::Completed => Settled::Paid,
                    InvStockHoldState::Held => {
                        // The claim is Inventory's own act: it re-checks the
                        // shelf under its lock and either records the sale's
                        // movement or refuses whole. The move note is the
                        // order id — the sale's own reference, no invented
                        // words.
                        match inv.claim(&hold, order.as_str(), now).await {
                            Ok(_) => Settled::Paid,
                            Err(StoreError::Conflict(_)) => {
                                // The freshest truth decides which honest
                                // failure this is: lapsed between the reads,
                                // or the warehouse got there first.
                                let fresh = inv
                                    .stock_hold(&hold, now)
                                    .await?
                                    .ok_or(StoreError::NotFound)?;
                                match fresh.state {
                                    InvStockHoldState::Completed => Settled::Paid,
                                    InvStockHoldState::Held => {
                                        // Goods gone: the sale is off, so the
                                        // hold is released and what remains
                                        // goes back on open sale.
                                        let _ = inv.release(&hold, now).await;
                                        Settled::Refund(STOCK_ORDER_GOODS_GONE)
                                    }
                                    _ => Settled::Refund(STOCK_ORDER_PAID_AFTER_LAPSE),
                                }
                            }
                            Err(other) => return Err(other),
                        }
                    }
                    InvStockHoldState::Expired | InvStockHoldState::Released => {
                        Settled::Refund(STOCK_ORDER_PAID_AFTER_LAPSE)
                    }
                }
            }
            SitePaymentStatus::Failed => Settled::Dead(SiteStockOrderState::Failed),
            SitePaymentStatus::Canceled => Settled::Dead(SiteStockOrderState::Cancelled),
            SitePaymentStatus::Expired => Settled::Dead(SiteStockOrderState::Expired),
        };
        let row = match outcome {
            Settled::Paid => sqlx::query_as::<_, OrderRow>(&format!(
                "UPDATE site_stock_orders \
                        SET state = 'paid', paid_at = $4, updated_at = now() \
                      WHERE tenant_id = $1 AND site_id = $2 AND id = $3 \
                  RETURNING {ORDER_COLUMNS}"
            ))
            .bind(self.tenant.as_str())
            .bind(site.as_str())
            .bind(order.as_str())
            .bind(now)
            .fetch_one(&mut *tx)
            .await
            .map_err(StoreError::Db)?,
            Settled::Refund(failure) => sqlx::query_as::<_, OrderRow>(&format!(
                "UPDATE site_stock_orders \
                        SET state = 'failed', failure = $4, updated_at = now() \
                      WHERE tenant_id = $1 AND site_id = $2 AND id = $3 \
                  RETURNING {ORDER_COLUMNS}"
            ))
            .bind(self.tenant.as_str())
            .bind(site.as_str())
            .bind(order.as_str())
            .bind(failure)
            .fetch_one(&mut *tx)
            .await
            .map_err(StoreError::Db)?,
            Settled::Dead(dead) => {
                if state == dead {
                    tx.commit().await.map_err(StoreError::Db)?;
                    return row.into_order();
                }
                // The buyer is not coming back; the goods go back on sale.
                // Releasing a hold that already lapsed is a success — it
                // stopped counting at its expiry.
                let _ = inv.release(&hold, now).await;
                sqlx::query_as::<_, OrderRow>(&format!(
                    "UPDATE site_stock_orders \
                        SET state = $4, updated_at = now() \
                      WHERE tenant_id = $1 AND site_id = $2 AND id = $3 \
                  RETURNING {ORDER_COLUMNS}"
                ))
                .bind(self.tenant.as_str())
                .bind(site.as_str())
                .bind(order.as_str())
                .bind(dead.as_str())
                .fetch_one(&mut *tx)
                .await
                .map_err(StoreError::Db)?
            }
        };
        tx.commit().await.map_err(StoreError::Db)?;
        row.into_order()
    }

    /// One stock order of one of the tenant's sites, or `None`.
    ///
    /// # Errors
    /// [`StoreError::Conflict`] when the stored state is unreadable;
    /// [`StoreError::Db`] on failure.
    pub async fn site_stock_order(
        &self,
        site: &SiteId,
        order: &SiteStockOrderId,
    ) -> Result<Option<SiteStockOrder>> {
        let row = sqlx::query_as::<_, OrderRow>(&format!(
            "SELECT {ORDER_COLUMNS} FROM site_stock_orders \
              WHERE tenant_id = $1 AND site_id = $2 AND id = $3"
        ))
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(order.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        row.map(OrderRow::into_order).transpose()
    }

    /// A site's stock orders, newest first, at most
    /// [`MAX_SITE_STOCK_ORDERS`]. A missing or foreign site simply has none.
    ///
    /// # Errors
    /// [`StoreError::Conflict`] when a stored state is unreadable;
    /// [`StoreError::Db`] on failure.
    pub async fn site_stock_orders(&self, site: &SiteId) -> Result<Vec<SiteStockOrder>> {
        let rows = sqlx::query_as::<_, OrderRow>(&format!(
            "SELECT {ORDER_COLUMNS} FROM site_stock_orders \
              WHERE tenant_id = $1 AND site_id = $2 \
              ORDER BY created_at DESC, id LIMIT $3"
        ))
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(MAX_SITE_STOCK_ORDERS)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter().map(OrderRow::into_order).collect()
    }

    /// Inventory's stock-sale seam, opened as this door's own `(tenant,
    /// user)` — the handshake the seam requires, vouched for by the account
    /// door itself.
    fn stock_sale_door(&self) -> InvStockSale {
        InvStockSale::open(
            self.pool.clone(),
            self.blobs.clone(),
            self.tenant.clone(),
            self.user.clone(),
        )
    }
}

/// What a settle decided, before the row is written. `Refund` closes the
/// order as failed with the sentence that names the refund.
enum Settled {
    Paid,
    Refund(&'static str),
    Dead(SiteStockOrderState),
}

impl Store {
    /// Which tenant's door a provider payment id belongs behind — the
    /// webhook's entry point. The webhook names a payment and nothing else;
    /// the row itself names the tenant, and the caller then opens that
    /// tenant's own door and asks the *provider* where the payment stands.
    ///
    /// A payment id nobody holds answers `None` — an unauthenticated probe
    /// learns nothing, not even that it guessed a real shape.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn stock_payment_target(
        &self,
        provider_payment_id: &str,
    ) -> Result<Option<StockPaymentTarget>> {
        let Ok(reference) = validate_payment_reference(provider_payment_id) else {
            return Ok(None);
        };
        let found: Option<(String, String, String)> = sqlx::query_as(
            "SELECT tenant_id, site_id, id FROM site_stock_orders \
              WHERE provider_payment_id = $1",
        )
        .bind(&reference)
        .fetch_optional(self.pool())
        .await
        .map_err(StoreError::Db)?;
        Ok(found.map(|(tenant, site, order)| StockPaymentTarget {
            tenant: TenantId::new(tenant),
            site: SiteId::new(site),
            order: SiteStockOrderId::new(order),
        }))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn states_round_trip_and_unknown_words_refuse() {
        for state in [
            SiteStockOrderState::Pending,
            SiteStockOrderState::AwaitingPayment,
            SiteStockOrderState::Paid,
            SiteStockOrderState::Failed,
            SiteStockOrderState::Cancelled,
            SiteStockOrderState::Expired,
        ] {
            assert_eq!(SiteStockOrderState::parse(state.as_str()), Some(state));
        }
        assert_eq!(SiteStockOrderState::parse("bartered"), None);
    }

    #[test]
    fn only_the_two_early_states_are_open() {
        assert!(SiteStockOrderState::Pending.is_open());
        assert!(SiteStockOrderState::AwaitingPayment.is_open());
        for closed in [
            SiteStockOrderState::Paid,
            SiteStockOrderState::Failed,
            SiteStockOrderState::Cancelled,
            SiteStockOrderState::Expired,
        ] {
            assert!(!closed.is_open());
        }
    }

    #[test]
    fn the_address_gate_catches_typos_not_people() {
        let good = ShipTo {
            line: "  Keizersgracht 1 ".to_owned(),
            city: "Amsterdam".to_owned(),
            postcode: "1015 CS".to_owned(),
            country: "nl".to_owned(),
        };
        let normalized = normalize_ship_to(&good).unwrap();
        assert_eq!(normalized.line, "Keizersgracht 1");
        assert_eq!(normalized.country, "NL");

        for (field, value) in [
            ("line", ""),
            ("line", &"x".repeat(SHIP_TO_LINE_MAX_CHARS + 1)),
            ("city", "line\u{7}bell"),
            ("postcode", &"9".repeat(SHIP_TO_POSTCODE_MAX_CHARS + 1)),
            ("country", "NLD"),
            ("country", "n1"),
            ("country", ""),
        ] {
            let mut bad = good.clone();
            match field {
                "line" => bad.line = value.to_owned(),
                "city" => bad.city = value.to_owned(),
                "postcode" => bad.postcode = value.to_owned(),
                _ => bad.country = value.to_owned(),
            }
            assert!(
                matches!(normalize_ship_to(&bad), Err(StoreError::Validation(_))),
                "{field}={value:?} was accepted"
            );
        }
    }

    #[test]
    fn both_refund_sentences_name_the_refund() {
        assert!(STOCK_ORDER_PAID_AFTER_LAPSE.contains("refund"));
        assert!(STOCK_ORDER_GOODS_GONE.contains("refund"));
    }
}
