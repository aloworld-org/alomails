//! The public **ticket checkout** of alo Sites: what an anonymous visitor may
//! learn about a site's ticketed events, and the purchase they may start
//! (ADR 0041, item S3.04f — the store half; the `/tix/*` pages on `alo-sites`
//! are the serving half).
//!
//! Like every public door ([`crate::site_public_orders`],
//! [`crate::site_public_bookings`]), this module never takes a tenant from
//! outside: every read and write is anchored on a [`PublishedSite`] the Host
//! resolver produced, and an event id is only ever looked up inside that
//! site. An unknown id, a foreign tenant's id, and another site's id on the
//! same tenant are all the same `Ok(None)`, which the wire turns into one
//! uniform 404.
//!
//! What the door answers is deliberately less than what the owner knows. A
//! visitor sees the price list's answer *now* — the name and the price come
//! from Billing's catalog seam at every read, never from a stored copy
//! (ADR 0041: price is one number) — plus when the event starts and how many
//! seats a buyer could still take. An event whose product has left the price
//! list simply is not offered: a shop can never sell the past.
//!
//! The checkout itself is the machinery S3.04b/c built, driven in the only
//! order that cannot oversell: validate the buyer, **hold the seats**, then
//! record the order against the hold. The hold's TTL is
//! [`TICKET_CHECKOUT_HOLD_TTL`] — long enough to pay on a hosted page, short
//! enough that an abandoned basket frees its seats while the event still
//! sells. Settling is fetch-not-believe, exactly as the order module demands:
//! the webhook route resolves a provider payment id through
//! [`SitePublicStore::public_ticket_payment_target`], asks the *provider*
//! where the payment stands, and applies that answer here.

use time::{Duration, OffsetDateTime};

use crate::account::AccountStore;
use crate::billing_catalog_read::BillingCatalogRead;
use crate::error::{Result, StoreError};
use crate::id::{SiteTicketEventId, SiteTicketOrderId, TenantId, UserId};
use crate::site_payments::SitePaymentStatus;
use crate::site_public::{PublishedSite, SitePublicStore};
use crate::site_ticket_orders::{
    SiteTicketOrderState, TicketPaymentTarget, normalize_buyer_email, normalize_buyer_name,
};

/// The longest id token this door will even send to the database. Real ids
/// are 22 characters (base64url of 16 random bytes); anything far outside
/// that shape is noise, not a lookup.
const SHOP_ID_MAX_LEN: usize = 64;

/// How long a checkout may take: the life of the hold taken when a buyer
/// starts paying. Half an hour is a hosted payment page with room to find a
/// card, and well inside the machinery's one-hour ceiling.
pub const TICKET_CHECKOUT_HOLD_TTL: Duration = Duration::minutes(30);

/// One ticketed event as a visitor may see it: the price list's answer now,
/// the when, and what is left. No capacity, no sales figures, no buyer of
/// any kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicTicketEvent {
    pub id: SiteTicketEventId,
    /// What the price list calls the item, at this instant.
    pub name: String,
    pub starts_at: OffsetDateTime,
    /// What one seat costs the buyer, integer cents, VAT included — consumer
    /// prices are what is actually charged (S3.04d).
    pub unit_price_cents: i64,
    /// The currency of that price — the tenant's accounting currency.
    pub currency: String,
    /// Seats a buyer could take right now. Zero or below reads "sold out".
    pub remaining: i64,
}

/// A checkout that has begun: the seats are held, the order is placed, and
/// this is everything the hosted-payment call needs — the order id doubles as
/// the provider's idempotency key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicTicketCheckout {
    pub order: SiteTicketOrderId,
    pub quantity: i32,
    /// What the buyer pays, integer cents, computed server-side.
    pub amount_cents: i64,
    pub currency: String,
    /// What the payment is for, in the buyer's terms
    /// ("2 × Letterpress workshop — 2026-09-16").
    pub description: String,
    /// When the held seats lapse — the deadline the payment must beat.
    pub expires_at: OffsetDateTime,
}

/// Where one order stands, as its buyer's return page may see it. The state
/// and the handles forward — never the buyer's own details back out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicTicketOrderStatus {
    pub state: SiteTicketOrderState,
    pub quantity: i32,
    pub amount_cents: i64,
    pub currency: String,
    /// The provider's hosted page, while the order is still waiting on it —
    /// the "continue to payment" link.
    pub checkout_url: Option<String>,
    /// The provider's payment id, for the return page's own status fetch.
    /// Server-side only; a rendered page never carries it.
    pub provider_payment_id: Option<String>,
    /// The ticket, once fulfilment has minted it (`/t/{token}`).
    pub ticket_token: Option<String>,
}

#[derive(sqlx::FromRow)]
struct PublicEventRow {
    id: String,
    product_id: String,
    starts_at: OffsetDateTime,
    capacity: i32,
    committed: i64,
}

impl PublicEventRow {
    fn into_event(
        self,
        item: &crate::billing_catalog_read::CatalogSaleItem,
        currency: &str,
    ) -> PublicTicketEvent {
        PublicTicketEvent {
            id: SiteTicketEventId::new(self.id),
            name: item.name.clone(),
            starts_at: self.starts_at,
            unit_price_cents: item.unit_price_cents,
            currency: currency.to_owned(),
            remaining: i64::from(self.capacity) - self.committed,
        }
    }
}

const PUBLIC_EVENT_COLUMNS: &str = "e.id, e.product_id, e.starts_at, e.capacity, \
     (SELECT COALESCE(SUM(h.quantity), 0) FROM site_ticket_holds h \
       WHERE h.tenant_id = e.tenant_id AND h.event_id = e.id \
         AND (h.state = 'completed' \
              OR (h.state = 'held' AND h.expires_at > $3))) AS committed";

impl SitePublicStore {
    /// A published site's upcoming ticketed events, in start order, each
    /// priced by the price list's answer *now*. An event that has started, or
    /// whose product no longer answers on the catalog seam, is not offered.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn public_ticket_events(
        &self,
        site: &PublishedSite,
        now: OffsetDateTime,
    ) -> Result<Vec<PublicTicketEvent>> {
        let rows: Vec<PublicEventRow> = sqlx::query_as(&format!(
            "SELECT {PUBLIC_EVENT_COLUMNS} FROM site_ticket_events e \
              WHERE e.tenant_id = $1 AND e.site_id = $2 AND e.starts_at > $3 \
              ORDER BY e.starts_at, e.id"
        ))
        .bind(site.tenant.as_str())
        .bind(site.site.as_str())
        .bind(now)
        .fetch_all(self.pool())
        .await
        .map_err(StoreError::Db)?;
        if rows.is_empty() {
            return Ok(Vec::new());
        }
        let door = self.catalog_door(site).await?;
        let Some(door) = door else {
            return Ok(Vec::new());
        };
        let items = door.sale_items().await?;
        let currency = door.currency().await?;
        Ok(rows
            .into_iter()
            .filter_map(|row| {
                items
                    .iter()
                    .find(|item| item.id.as_str() == row.product_id)
                    .map(|item| row.into_event(item, &currency))
            })
            .collect())
    }

    /// One upcoming ticketed event of the published site, priced now, or
    /// `None` — unknown, malformed, foreign, already started, and no longer
    /// on the price list are deliberately one answer.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn public_ticket_event(
        &self,
        site: &PublishedSite,
        event_id: &str,
        now: OffsetDateTime,
    ) -> Result<Option<PublicTicketEvent>> {
        let Some(event_id) = plausible(event_id) else {
            return Ok(None);
        };
        let row: Option<PublicEventRow> = sqlx::query_as(&format!(
            "SELECT {PUBLIC_EVENT_COLUMNS} FROM site_ticket_events e \
              WHERE e.tenant_id = $1 AND e.site_id = $2 AND e.starts_at > $3 AND e.id = $4"
        ))
        .bind(site.tenant.as_str())
        .bind(site.site.as_str())
        .bind(now)
        .bind(event_id)
        .fetch_optional(self.pool())
        .await
        .map_err(StoreError::Db)?;
        let Some(row) = row else { return Ok(None) };
        let Some(door) = self.catalog_door(site).await? else {
            return Ok(None);
        };
        let currency = door.currency().await?;
        let item = door
            .sale_item(&crate::id::BillingProductId::new(row.product_id.clone()))
            .await?;
        Ok(item.map(|item| row.into_event(&item, &currency)))
    }

    /// Starts a purchase: validates the buyer, **holds the seats** for
    /// [`TICKET_CHECKOUT_HOLD_TTL`], and places the order against the hold —
    /// in that order, so nothing a visitor types can cost a seat, and no two
    /// buyers can be sold the same one (the hold machinery's lock decides).
    ///
    /// Returns `Ok(None)` when the event is not this site's to sell
    /// (unknown, malformed, foreign — one answer).
    ///
    /// # Errors
    /// [`StoreError::Validation`] for a malformed name, email or quantity;
    /// [`StoreError::Conflict`] in the seats' own words — "sold out", "only N
    /// seats are left", "this event has already started" — all safe to show
    /// the visitor; [`StoreError::Db`] on failure.
    pub async fn public_begin_ticket_checkout(
        &self,
        site: &PublishedSite,
        event_id: &str,
        quantity: i32,
        buyer_name: &str,
        buyer_email: &str,
        now: OffsetDateTime,
    ) -> Result<Option<PublicTicketCheckout>> {
        let Some(event_id) = plausible(event_id) else {
            return Ok(None);
        };
        // The typo gate runs before any seat is touched: a bad address must
        // never take (and then release) a hold real buyers are racing for.
        normalize_buyer_name(buyer_name)?;
        normalize_buyer_email(buyer_email)?;
        // The offer this checkout answers: also the name and the day the
        // payment page will describe, from the price list's answer now. An
        // event that cannot be offered cannot be bought.
        let Some(offer) = self.public_ticket_event(site, event_id, now).await? else {
            return Ok(None);
        };
        let Some(door) = self.shop_door(site).await? else {
            return Ok(None);
        };
        let event = SiteTicketEventId::new(event_id);
        let hold = match door
            .take_ticket_hold(&site.site, &event, quantity, TICKET_CHECKOUT_HOLD_TTL, now)
            .await
        {
            Ok(hold) => hold,
            // The uniform miss: an event this site cannot sell answers
            // exactly like an event that never existed.
            Err(StoreError::NotFound) => return Ok(None),
            Err(other) => return Err(other),
        };
        let order = match door
            .create_ticket_order(&site.site, &hold.id, buyer_name, buyer_email, now)
            .await
        {
            Ok(order) => order,
            Err(error) => {
                // The order never happened, so the seats go straight back on
                // sale rather than squatting out the TTL. Best effort: an
                // unreleased hold still frees itself at expiry.
                let _ = door.release_ticket_hold(&site.site, &hold.id, now).await;
                return Err(error);
            }
        };
        // The words on the provider's payment page. The date is the event's
        // UTC day, as the ticket page shows it until the venue zone lands.
        let description = format!(
            "{} × {}",
            order.quantity,
            describe_purchase(&offer.name, offer.starts_at)
        );
        Ok(Some(PublicTicketCheckout {
            order: order.id,
            quantity: order.quantity,
            amount_cents: order.amount_cents,
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
    pub async fn public_open_ticket_payment(
        &self,
        site: &PublishedSite,
        order: &SiteTicketOrderId,
        provider_payment_id: &str,
        checkout_url: &str,
    ) -> Result<Option<()>> {
        let Some(door) = self.shop_door(site).await? else {
            return Ok(None);
        };
        match door
            .open_ticket_payment(&site.site, order, provider_payment_id, checkout_url)
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
    pub async fn public_ticket_order(
        &self,
        site: &PublishedSite,
        order_id: &str,
    ) -> Result<Option<PublicTicketOrderStatus>> {
        let Some(order_id) = plausible(order_id) else {
            return Ok(None);
        };
        #[derive(sqlx::FromRow)]
        struct StatusRow {
            state: String,
            quantity: i32,
            amount_cents: i64,
            currency: String,
            checkout_url: Option<String>,
            provider_payment_id: Option<String>,
            ticket_token: Option<String>,
        }
        let row: Option<StatusRow> = sqlx::query_as(
            "SELECT o.state, o.quantity, o.amount_cents, o.currency, o.checkout_url, \
                    o.provider_payment_id, f.token AS ticket_token \
               FROM site_ticket_orders o \
               LEFT JOIN site_ticket_fulfilments f \
                 ON f.tenant_id = o.tenant_id AND f.order_id = o.id AND f.token <> '' \
              WHERE o.tenant_id = $1 AND o.site_id = $2 AND o.id = $3",
        )
        .bind(site.tenant.as_str())
        .bind(site.site.as_str())
        .bind(order_id)
        .fetch_optional(self.pool())
        .await
        .map_err(StoreError::Db)?;
        let Some(row) = row else { return Ok(None) };
        let state = SiteTicketOrderState::parse(&row.state).ok_or_else(|| {
            StoreError::Conflict(format!(
                "stored order state '{}' is not one this release knows",
                row.state
            ))
        })?;
        Ok(Some(PublicTicketOrderStatus {
            state,
            quantity: row.quantity,
            amount_cents: row.amount_cents,
            currency: row.currency,
            // A closed order's checkout link is dead weight at best and a
            // second charge at worst; only a waiting order still offers it.
            checkout_url: row.checkout_url.filter(|_| state.is_open()),
            provider_payment_id: row.provider_payment_id,
            ticket_token: row.ticket_token,
        }))
    }

    /// Which order a provider payment id belongs to — the webhook's entry
    /// point, host-independent because a webhook has no Host worth trusting.
    /// A payment id nobody holds answers `None`: an unauthenticated probe
    /// learns nothing.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn public_ticket_payment_target(
        &self,
        provider_payment_id: &str,
    ) -> Result<Option<TicketPaymentTarget>> {
        crate::store::Store::new(self.pool().clone(), self.blobs().clone())
            .ticket_payment_target(provider_payment_id)
            .await
    }

    /// Settles the targeted order with a payment status **fetched from the
    /// provider** — never one a webhook body asserted. Idempotent throughout;
    /// every outcome (paid, paid-too-late, dead) is the order machinery's
    /// decision ([`crate::site_ticket_orders`]).
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the target's rows are gone;
    /// [`StoreError::Db`] on failure.
    pub async fn public_settle_ticket_payment(
        &self,
        target: &TicketPaymentTarget,
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
        let door = self.tenant_door(target.tenant.clone(), UserId::new(owner));
        door.apply_ticket_payment(&target.site, &target.order, status, now)
            .await?;
        Ok(())
    }

    /// The catalog seam of the published site's tenant, opened as the site's
    /// owner — the same `(tenant, owner)` handshake every seam door uses,
    /// both halves read from the site's own row. `None` when the site row is
    /// gone from under its publish.
    pub(crate) async fn catalog_door(
        &self,
        site: &PublishedSite,
    ) -> Result<Option<BillingCatalogRead>> {
        let owner: Option<String> =
            sqlx::query_scalar("SELECT created_by FROM sites WHERE tenant_id = $1 AND id = $2")
                .bind(site.tenant.as_str())
                .bind(site.site.as_str())
                .fetch_optional(self.pool())
                .await
                .map_err(StoreError::Db)?;
        Ok(owner.map(|owner| {
            BillingCatalogRead::open(
                self.pool().clone(),
                self.blobs().clone(),
                site.tenant.clone(),
                UserId::new(owner),
            )
        }))
    }

    /// The ticket machinery of the published site's tenant, opened as the
    /// site's owner. Same handshake as [`Self::catalog_door`].
    pub(crate) async fn shop_door(&self, site: &PublishedSite) -> Result<Option<AccountStore>> {
        let owner: Option<String> =
            sqlx::query_scalar("SELECT created_by FROM sites WHERE tenant_id = $1 AND id = $2")
                .bind(site.tenant.as_str())
                .bind(site.site.as_str())
                .fetch_optional(self.pool())
                .await
                .map_err(StoreError::Db)?;
        Ok(owner.map(|owner| self.tenant_door(site.tenant.clone(), UserId::new(owner))))
    }

    /// An [`AccountStore`] over this door's own pool — the internal handle
    /// the public wrappers drive the S3.04b/c machinery through. Never
    /// exposed: the public service still cannot open a tenant door.
    fn tenant_door(&self, tenant: TenantId, owner: UserId) -> AccountStore {
        AccountStore {
            pool: self.pool().clone(),
            blobs: self.blobs().clone(),
            tenant,
            user: owner,
        }
    }
}

/// The payment page's one line for what is being bought:
/// "<product name> — <UTC day>". Separate so the format is testable without a
/// database.
fn describe_purchase(name: &str, starts_at: OffsetDateTime) -> String {
    format!(
        "{name} — {:04}-{:02}-{:02}",
        starts_at.year(),
        u8::from(starts_at.month()),
        starts_at.day()
    )
}

/// The same shape gate every public door applies: refuse anything that cannot
/// be a minted id before the database is involved at all.
pub(crate) fn plausible(id: &str) -> Option<&str> {
    let id = id.trim();
    (!id.is_empty()
        && id.len() <= SHOP_ID_MAX_LEN
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'))
    .then_some(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_mintable_shape_reaches_the_database() {
        assert_eq!(plausible("  ev-1_A  "), Some("ev-1_A"));
        assert!(plausible("").is_none());
        assert!(plausible("   ").is_none());
        assert!(plausible(&"x".repeat(65)).is_none());
        assert!(plausible("two words").is_none());
        assert!(plausible("ev;drop").is_none());
    }

    #[test]
    fn a_purchase_is_described_by_name_and_utc_day() {
        let starts = time::macros::datetime!(2026-09-16 19:30 UTC);
        assert_eq!(
            describe_purchase("Letterpress workshop", starts),
            "Letterpress workshop — 2026-09-16"
        );
    }
}
