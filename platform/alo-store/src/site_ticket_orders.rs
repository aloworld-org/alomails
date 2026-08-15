//! The ticket order — order → payment-reference → paid, with a webhook that
//! can only make alo look, never make it believe (ADR 0041, item S3.04c).
//!
//! [`crate::site_ticket_holds`] is the seat accounting and deliberately holds
//! no buyer; this module is where the buyer lives. An order records who is
//! paying, the price they were shown — the price list's answer at the moment
//! the order was placed, computed server-side through Billing's catalog seam
//! and stored as the record of the sale, exactly as a domain purchase records
//! the quote its buyer approved — and where the hosted payment stands.
//!
//! The state machine is the row: `pending` (placed, provider not asked) →
//! `awaiting_payment` (the provider minted a payment; the buyer is on its
//! hosted page) → `paid` (money confirmed, the hold completed in the same
//! transaction). `failed`, `cancelled` and `expired` end the road and give
//! the seats back. Three rules hold everywhere:
//!
//! - **A retry never sells twice.** The hold id is the creation's replay
//!   token (one order per hold, by unique index); the order id is the
//!   provider call's idempotency key; and [`apply_ticket_payment`] applied
//!   twice with the same status returns the same row — a webhook delivered
//!   five times is one sale.
//! - **A webhook is a doorbell.** The public wire will hand
//!   [`Store::ticket_payment_target`] nothing but a provider payment id; the
//!   row names its own tenant, and the status that settles the order is
//!   fetched from the provider through the [`crate::site_payments`] boundary
//!   — never read from the webhook body. An unauthenticated POST can make
//!   alo ask a question, not assert an answer.
//! - **Money that moved is never silently lost.** A payment confirmed after
//!   the hold lapsed cannot be given seats that may have been resold; the
//!   order fails **visibly**, with a sentence that names the refund, rather
//!   than pretending the sale happened or vanishing the money.
//!
//! Who calls this from the public wire — the shop checkout and the webhook
//! route on `alo-sites` — is S3.04f/g; fulfilment (the ticket by email, the
//! invoice, the CRM contact) is S3.04d.
//!
//! [`apply_ticket_payment`]: AccountStore::apply_ticket_payment

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::billing_catalog_read::BillingCatalogRead;
use crate::error::{Result, StoreError};
use crate::id::{SiteId, SiteTicketEventId, SiteTicketHoldId, SiteTicketOrderId, TenantId};
use crate::site_domain_purchases::validate_payment_reference;
use crate::site_payments::{SitePaymentError, SitePaymentStatus, validate_payment_url};
use crate::store::Store;

/// Most orders one list read returns.
pub const MAX_SITE_TICKET_ORDERS: i64 = 50;

/// Longest buyer name an order accepts.
pub const TICKET_BUYER_NAME_MAX_CHARS: usize = 200;

/// Longest buyer email an order accepts — the RFC's own ceiling, as the
/// booking door applies it.
pub const TICKET_BUYER_EMAIL_MAX_CHARS: usize = 254;

/// What an order says when the money arrived after the seats were gone. The
/// sentence names the refund: this is a support conversation, not a sale.
pub const TICKET_ORDER_PAID_AFTER_LAPSE: &str = "the payment was received after the seat hold had expired; \
     the seats may have been resold — this payment needs a refund";

/// Where one order stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiteTicketOrderState {
    /// Placed; no payment provider has been asked yet.
    Pending,
    /// The provider minted a payment; the buyer is on its hosted page.
    AwaitingPayment,
    /// The money moved and the hold completed. A sale, forever.
    Paid,
    /// The payment failed, or arrived too late to be honoured
    /// ([`SiteTicketOrder::failure`] says which, when it needs saying).
    Failed,
    /// The buyer cancelled on the provider's page.
    Cancelled,
    /// The provider's checkout lapsed before the buyer finished.
    Expired,
}

impl SiteTicketOrderState {
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

/// One ticket order, as the tenant sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteTicketOrder {
    pub id: SiteTicketOrderId,
    pub event: SiteTicketEventId,
    /// The hold whose seats this order buys — also the creation's replay
    /// token: one hold is one order, ever.
    pub hold: SiteTicketHoldId,
    pub quantity: i32,
    pub buyer_name: String,
    pub buyer_email: String,
    /// One seat's price as it was struck, integer cents.
    pub unit_price_cents: i64,
    /// What the buyer pays: `quantity × unit_price_cents`, computed
    /// server-side at creation.
    pub amount_cents: i64,
    /// VAT rate in basis points, as Billing held it when the order was
    /// placed.
    pub vat_rate_bp: i32,
    /// The tenant's accounting currency at the moment of sale.
    pub currency: String,
    pub state: SiteTicketOrderState,
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
pub struct TicketPaymentTarget {
    /// The tenant whose order this is — resolved from the row itself, never
    /// from anything the webhook claimed.
    pub tenant: TenantId,
    pub site: SiteId,
    pub order: SiteTicketOrderId,
}

const ORDER_COLUMNS: &str = "id, event_id, hold_id, quantity, buyer_name, buyer_email, \
     unit_price_cents, amount_cents, vat_rate_bp, currency, state, \
     provider_payment_id, checkout_url, failure, paid_at, created_at, updated_at";

#[derive(sqlx::FromRow)]
struct OrderRow {
    id: String,
    event_id: String,
    hold_id: String,
    quantity: i32,
    buyer_name: String,
    buyer_email: String,
    unit_price_cents: i64,
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
    fn state(&self) -> Result<SiteTicketOrderState> {
        SiteTicketOrderState::parse(&self.state).ok_or_else(|| {
            StoreError::Conflict(format!(
                "stored order state '{}' is not one this release knows",
                self.state
            ))
        })
    }

    fn into_order(self) -> Result<SiteTicketOrder> {
        let state = self.state()?;
        Ok(SiteTicketOrder {
            id: SiteTicketOrderId::new(self.id),
            event: SiteTicketEventId::new(self.event_id),
            hold: SiteTicketHoldId::new(self.hold_id),
            quantity: self.quantity,
            buyer_name: self.buyer_name,
            buyer_email: self.buyer_email,
            unit_price_cents: self.unit_price_cents,
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

fn normalize_buyer_name(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(StoreError::Validation("name must not be empty".to_owned()));
    }
    if value.chars().count() > TICKET_BUYER_NAME_MAX_CHARS {
        return Err(StoreError::Validation(format!(
            "name must be at most {TICKET_BUYER_NAME_MAX_CHARS} characters"
        )));
    }
    if value.chars().any(char::is_control) {
        return Err(StoreError::Validation(
            "name may not contain control characters".to_owned(),
        ));
    }
    Ok(value.to_owned())
}

/// The same shape gate the booking and order doors apply: enough to catch a
/// typo, never a claim that the mailbox exists.
fn normalize_buyer_email(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > TICKET_BUYER_EMAIL_MAX_CHARS {
        return Err(StoreError::Validation(format!(
            "email must be 1-{TICKET_BUYER_EMAIL_MAX_CHARS} characters"
        )));
    }
    let looks_like_address = matches!(
        value.split_once('@'),
        Some((local, domain)) if !local.is_empty() && !domain.is_empty()
    );
    if !looks_like_address || value.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(StoreError::Validation(
            "email must be a valid address".to_owned(),
        ));
    }
    Ok(value.to_owned())
}

/// Maps this table's unique indexes onto sentences.
fn map_order_unique(error: sqlx::Error) -> StoreError {
    if let sqlx::Error::Database(ref database) = error {
        match database.constraint() {
            Some("site_ticket_orders_one_per_hold") => {
                return StoreError::Conflict("those seats already have an order".to_owned());
            }
            Some("site_ticket_orders_one_payment") => {
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
    order: &SiteTicketOrderId,
) -> Result<OrderRow> {
    sqlx::query_as::<_, OrderRow>(&format!(
        "SELECT {ORDER_COLUMNS} FROM site_ticket_orders \
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
    /// Places an order for the seats one live hold reserves: who is buying,
    /// and the price they are shown — read from Billing's catalog seam at
    /// this instant and stored as the record of the sale.
    ///
    /// Idempotent under the hold: one hold is one order, so a double-clicked
    /// buy button reaches the order it already made (same buyer), and a
    /// different buyer on the same hold is refused rather than quietly
    /// replaced.
    ///
    /// # Errors
    /// [`StoreError::Validation`] for a malformed name or email;
    /// [`StoreError::NotFound`] when the hold is not this tenant's on this
    /// site; [`StoreError::Conflict`] when the hold is not live, the ticket
    /// left the price list, or the hold's order belongs to a different buyer;
    /// [`StoreError::Db`] on failure.
    pub async fn create_ticket_order(
        &self,
        site: &SiteId,
        hold: &SiteTicketHoldId,
        buyer_name: &str,
        buyer_email: &str,
        now: OffsetDateTime,
    ) -> Result<SiteTicketOrder> {
        let buyer_name = normalize_buyer_name(buyer_name)?;
        let buyer_email = normalize_buyer_email(buyer_email)?;
        // Anchored at the site: a caller who cannot name the site learns
        // nothing — not the hold's existence, not the price list's answer.
        let found: Option<(String, i32, String, OffsetDateTime, String)> = sqlx::query_as(
            "SELECT h.event_id, h.quantity, h.state, h.expires_at, e.product_id \
               FROM site_ticket_holds h \
               JOIN site_ticket_events e \
                 ON e.tenant_id = h.tenant_id AND e.id = h.event_id \
              WHERE h.tenant_id = $1 AND h.site_id = $2 AND h.id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(hold.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        let (event_id, quantity, hold_state, expires_at, product_id) =
            found.ok_or(StoreError::NotFound)?;
        match hold_state.as_str() {
            "held" if expires_at > now => {}
            "held" | "expired" => {
                return Err(StoreError::Conflict(
                    "this hold has expired; hold the seats again before ordering".to_owned(),
                ));
            }
            "released" => {
                return Err(StoreError::Conflict(
                    "this hold was released; hold the seats again before ordering".to_owned(),
                ));
            }
            _ => {
                return Err(StoreError::Conflict(
                    "these seats have already been bought".to_owned(),
                ));
            }
        }
        // The price is the price list's answer, through the same seam the
        // shop renders from — never a figure the request carried.
        let door = BillingCatalogRead::open(
            self.pool.clone(),
            self.blobs.clone(),
            self.tenant.clone(),
            self.user.clone(),
        );
        let item = door
            .sale_item(&crate::id::BillingProductId::new(product_id))
            .await?
            .ok_or_else(|| {
                StoreError::Conflict(
                    "this event's ticket is no longer on the price list".to_owned(),
                )
            })?;
        let currency = door.currency().await?;
        let amount_cents = i64::from(quantity)
            .checked_mul(item.unit_price_cents)
            .ok_or_else(|| StoreError::Validation("that amount is not a price".to_owned()))?;
        let id = SiteTicketOrderId::generate();
        let inserted = sqlx::query_as::<_, OrderRow>(&format!(
            "INSERT INTO site_ticket_orders \
                (tenant_id, site_id, id, event_id, hold_id, quantity, \
                 buyer_name, buyer_email, unit_price_cents, amount_cents, \
                 vat_rate_bp, currency, created_at, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $13) \
          RETURNING {ORDER_COLUMNS}"
        ))
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(id.as_str())
        .bind(&event_id)
        .bind(hold.as_str())
        .bind(quantity)
        .bind(&buyer_name)
        .bind(&buyer_email)
        .bind(item.unit_price_cents)
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
                        "SELECT {ORDER_COLUMNS} FROM site_ticket_orders \
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
                            "those seats already have an order for a different buyer".to_owned(),
                        ));
                    }
                }
                Err(mapped)
            }
        }
    }

    /// Records the hosted payment the provider minted for an order: its
    /// opaque id and where its checkout lives, moving the order to
    /// [`AwaitingPayment`](SiteTicketOrderState::AwaitingPayment).
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
    pub async fn open_ticket_payment(
        &self,
        site: &SiteId,
        order: &SiteTicketOrderId,
        provider_payment_id: &str,
        checkout_url: &str,
    ) -> Result<SiteTicketOrder> {
        let reference = validate_payment_reference(provider_payment_id)?;
        validate_payment_url(checkout_url, "checkout").map_err(|error| match error {
            SitePaymentError::Validation(message) => StoreError::Validation(message),
            other => StoreError::Validation(other.to_string()),
        })?;
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let row = locked_order(&mut tx, self.tenant.as_str(), site, order).await?;
        match row.state()? {
            SiteTicketOrderState::Pending => {}
            SiteTicketOrderState::AwaitingPayment => {
                if row.provider_payment_id.as_deref() == Some(reference.as_str()) {
                    tx.commit().await.map_err(StoreError::Db)?;
                    return row.into_order();
                }
                return Err(StoreError::Conflict(
                    "this order is already waiting for another payment".to_owned(),
                ));
            }
            SiteTicketOrderState::Paid => {
                return Err(StoreError::Conflict(
                    "this order has already been paid".to_owned(),
                ));
            }
            SiteTicketOrderState::Failed
            | SiteTicketOrderState::Cancelled
            | SiteTicketOrderState::Expired => {
                return Err(StoreError::Conflict(
                    "this order is closed; place a new one".to_owned(),
                ));
            }
        }
        let row = sqlx::query_as::<_, OrderRow>(&format!(
            "UPDATE site_ticket_orders \
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
    /// replayed five times is one sale.
    ///
    /// [`Paid`](SitePaymentStatus::Paid) completes the hold and the order in
    /// one transaction, under the same per-event lock the buyers take. A
    /// payment confirmed after the hold lapsed cannot be given seats that may
    /// have been resold: the order fails **visibly**, its
    /// [`failure`](SiteTicketOrder::failure) naming the refund. The dead
    /// statuses (failed, canceled, expired) close the order and give the
    /// seats back. [`Open`](SitePaymentStatus::Open) changes nothing. A paid
    /// order is never unsold by a late status of any kind.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the order is not this tenant's on this
    /// site; [`StoreError::Conflict`] when a stored state is unreadable;
    /// [`StoreError::Db`] on failure.
    pub async fn apply_ticket_payment(
        &self,
        site: &SiteId,
        order: &SiteTicketOrderId,
        status: SitePaymentStatus,
        now: OffsetDateTime,
    ) -> Result<SiteTicketOrder> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let row = locked_order(&mut tx, self.tenant.as_str(), site, order).await?;
        let state = row.state()?;
        let event = SiteTicketEventId::new(row.event_id.clone());
        // A sale is a sale: no later status, of any kind, unsells it.
        if state == SiteTicketOrderState::Paid {
            tx.commit().await.map_err(StoreError::Db)?;
            return row.into_order();
        }
        let outcome = match status {
            // The payment is still underway; there is nothing to record.
            SitePaymentStatus::Open => {
                tx.commit().await.map_err(StoreError::Db)?;
                return row.into_order();
            }
            SitePaymentStatus::Paid => {
                // The seat count and both writes are one decision, under the
                // same lock the buyers take.
                crate::site_ticket_holds::lock_ticket_event(&mut tx, &self.tenant, &event).await?;
                let completed = sqlx::query(
                    "UPDATE site_ticket_holds \
                        SET state = 'completed', completed_at = $4 \
                      WHERE tenant_id = $1 AND site_id = $2 AND id = $3 \
                        AND state = 'held' AND expires_at > $4",
                )
                .bind(self.tenant.as_str())
                .bind(site.as_str())
                .bind(&row.hold_id)
                .bind(now)
                .execute(&mut *tx)
                .await
                .map_err(StoreError::Db)?;
                if completed.rows_affected() == 1 {
                    Settled::Paid
                } else {
                    // The money moved but the seats are gone. Fail visibly,
                    // naming the refund — never a sale of resold seats, and
                    // never money that silently vanishes.
                    Settled::PaidTooLate
                }
            }
            SitePaymentStatus::Failed => Settled::Dead(SiteTicketOrderState::Failed),
            SitePaymentStatus::Canceled => Settled::Dead(SiteTicketOrderState::Cancelled),
            SitePaymentStatus::Expired => Settled::Dead(SiteTicketOrderState::Expired),
        };
        let row = match outcome {
            Settled::Paid => sqlx::query_as::<_, OrderRow>(&format!(
                "UPDATE site_ticket_orders \
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
            Settled::PaidTooLate => sqlx::query_as::<_, OrderRow>(&format!(
                "UPDATE site_ticket_orders \
                        SET state = 'failed', failure = $4, updated_at = now() \
                      WHERE tenant_id = $1 AND site_id = $2 AND id = $3 \
                  RETURNING {ORDER_COLUMNS}"
            ))
            .bind(self.tenant.as_str())
            .bind(site.as_str())
            .bind(order.as_str())
            .bind(TICKET_ORDER_PAID_AFTER_LAPSE)
            .fetch_one(&mut *tx)
            .await
            .map_err(StoreError::Db)?,
            Settled::Dead(dead) => {
                if state == dead {
                    tx.commit().await.map_err(StoreError::Db)?;
                    return row.into_order();
                }
                // The buyer is not coming back; the seats go back on sale.
                // Releasing a hold that already lapsed is harmless — it
                // stopped counting at its expiry.
                crate::site_ticket_holds::lock_ticket_event(&mut tx, &self.tenant, &event).await?;
                sqlx::query(
                    "UPDATE site_ticket_holds SET state = 'released' \
                      WHERE tenant_id = $1 AND site_id = $2 AND id = $3 \
                        AND state = 'held'",
                )
                .bind(self.tenant.as_str())
                .bind(site.as_str())
                .bind(&row.hold_id)
                .execute(&mut *tx)
                .await
                .map_err(StoreError::Db)?;
                sqlx::query_as::<_, OrderRow>(&format!(
                    "UPDATE site_ticket_orders \
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

    /// One order of one of the tenant's sites, or `None`.
    ///
    /// # Errors
    /// [`StoreError::Conflict`] when the stored state is unreadable;
    /// [`StoreError::Db`] on failure.
    pub async fn site_ticket_order(
        &self,
        site: &SiteId,
        order: &SiteTicketOrderId,
    ) -> Result<Option<SiteTicketOrder>> {
        let row = sqlx::query_as::<_, OrderRow>(&format!(
            "SELECT {ORDER_COLUMNS} FROM site_ticket_orders \
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

    /// A site's ticket orders, newest first, at most
    /// [`MAX_SITE_TICKET_ORDERS`]. A missing or foreign site simply has none.
    ///
    /// # Errors
    /// [`StoreError::Conflict`] when a stored state is unreadable;
    /// [`StoreError::Db`] on failure.
    pub async fn site_ticket_orders(&self, site: &SiteId) -> Result<Vec<SiteTicketOrder>> {
        let rows = sqlx::query_as::<_, OrderRow>(&format!(
            "SELECT {ORDER_COLUMNS} FROM site_ticket_orders \
              WHERE tenant_id = $1 AND site_id = $2 \
              ORDER BY created_at DESC, id LIMIT $3"
        ))
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(MAX_SITE_TICKET_ORDERS)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter().map(OrderRow::into_order).collect()
    }
}

/// What a paid settle decided, before the row is written.
enum Settled {
    Paid,
    PaidTooLate,
    Dead(SiteTicketOrderState),
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
    pub async fn ticket_payment_target(
        &self,
        provider_payment_id: &str,
    ) -> Result<Option<TicketPaymentTarget>> {
        let Ok(reference) = validate_payment_reference(provider_payment_id) else {
            return Ok(None);
        };
        let found: Option<(String, String, String)> = sqlx::query_as(
            "SELECT tenant_id, site_id, id FROM site_ticket_orders \
              WHERE provider_payment_id = $1",
        )
        .bind(&reference)
        .fetch_optional(self.pool())
        .await
        .map_err(StoreError::Db)?;
        Ok(found.map(|(tenant, site, order)| TicketPaymentTarget {
            tenant: TenantId::new(tenant),
            site: SiteId::new(site),
            order: SiteTicketOrderId::new(order),
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
            SiteTicketOrderState::Pending,
            SiteTicketOrderState::AwaitingPayment,
            SiteTicketOrderState::Paid,
            SiteTicketOrderState::Failed,
            SiteTicketOrderState::Cancelled,
            SiteTicketOrderState::Expired,
        ] {
            assert_eq!(SiteTicketOrderState::parse(state.as_str()), Some(state));
        }
        assert_eq!(SiteTicketOrderState::parse("haggled"), None);
    }

    #[test]
    fn only_the_two_early_states_are_open() {
        assert!(SiteTicketOrderState::Pending.is_open());
        assert!(SiteTicketOrderState::AwaitingPayment.is_open());
        for closed in [
            SiteTicketOrderState::Paid,
            SiteTicketOrderState::Failed,
            SiteTicketOrderState::Cancelled,
            SiteTicketOrderState::Expired,
        ] {
            assert!(!closed.is_open());
        }
    }

    #[test]
    fn the_buyer_gates_catch_typos_not_people() {
        assert_eq!(normalize_buyer_name("  Maud Adams ").unwrap(), "Maud Adams");
        assert!(normalize_buyer_name("   ").is_err());
        assert!(normalize_buyer_name(&"m".repeat(TICKET_BUYER_NAME_MAX_CHARS + 1)).is_err());
        assert!(normalize_buyer_name("line\u{7}bell").is_err());

        assert_eq!(
            normalize_buyer_email(" maud@example.org ").unwrap(),
            "maud@example.org"
        );
        for bad in [
            "",
            "no-at-sign",
            "@nolocal.org",
            "nodomain@",
            "two words@x.y",
        ] {
            assert!(normalize_buyer_email(bad).is_err(), "{bad:?} was accepted");
        }
    }
}
