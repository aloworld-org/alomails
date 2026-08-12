//! Turning new site orders into pending owner notifications (ADR 0036; the
//! order form of ADR 0041's no-checkout wave), the sibling of
//! [`crate::site_form_notify`].
//!
//! An order row with a NULL `notified_at` is one nobody has been told about;
//! the notifier sweep in alo-jmap calls [`Store::claim_order_notifications`]
//! on an interval, builds one internal message per claimed order, and delivers
//! it through the **account door** of the site's creator. Nothing here sends
//! outbound mail: the customer's address travels as `Reply-To`, so answering
//! is one deliberate reply by the owner.
//!
//! Claiming is **at-most-once**: rows are marked notified in the same
//! statement that reads them, so a crash between claim and delivery loses a
//! notification but can never duplicate one — and the order itself stays in
//! the owner's order list either way, so nothing is silently lost.

use time::OffsetDateTime;

use crate::error::{Result, StoreError};
use crate::id::{SiteOrderId, TenantId, UserId};
use crate::site_orders::SiteOrderLine;
use crate::store::Store;

/// Everything the notifier needs to build and deliver one order
/// notification: the order, its lines, and the owning site's context,
/// resolved in the claim itself.
#[derive(Debug, Clone)]
pub struct OrderNotification {
    /// The tenant the order belongs to — the only tenant whose inbox this
    /// notification may reach.
    pub tenant: TenantId,
    /// The site's creator: the account whose inbox receives the message.
    pub owner: UserId,
    pub site_name: String,
    pub site_subdomain: String,
    pub catalog_name: String,
    pub order: SiteOrderId,
    pub customer_name: String,
    pub customer_email: String,
    pub customer_phone: Option<String>,
    pub note: Option<String>,
    /// ISO 4217 code the line prices and the total are denominated in.
    pub currency: String,
    pub total_cents: i64,
    pub received_at: OffsetDateTime,
    /// What was ordered, in the sequence it was requested.
    pub lines: Vec<SiteOrderLine>,
}

impl Store {
    /// Claims up to `limit` orders awaiting notification, oldest first,
    /// marking each notified in the same statement (at-most-once — see the
    /// module doc). Concurrent sweeps skip each other's locked rows rather
    /// than double-claiming (`FOR UPDATE SKIP LOCKED`).
    ///
    /// System-level by design: the sweep spans tenants, and each returned row
    /// carries the tenant + owner the delivery must scope itself to.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn claim_order_notifications(&self, limit: i64) -> Result<Vec<OrderNotification>> {
        let rows = sqlx::query_as::<_, ClaimRow>(
            "UPDATE site_orders o \
                SET notified_at = now() \
               FROM sites s \
              WHERE s.tenant_id = o.tenant_id AND s.id = o.site_id \
                AND (o.tenant_id, o.id) IN ( \
                    SELECT tenant_id, id FROM site_orders \
                     WHERE notified_at IS NULL \
                     ORDER BY received_at, id \
                     LIMIT $1 \
                     FOR UPDATE SKIP LOCKED) \
             RETURNING o.tenant_id, s.created_by AS owner, s.name AS site_name, \
                       s.subdomain AS site_subdomain, o.catalog_name, o.id, \
                       o.customer_name, o.customer_email, o.customer_phone, o.note, \
                       o.currency, o.total_cents, o.received_at",
        )
        .bind(limit)
        .fetch_all(self.pool())
        .await
        .map_err(StoreError::Db)?;
        if rows.is_empty() {
            return Ok(Vec::new());
        }

        // The lines of exactly the claimed orders, addressed as (tenant, order)
        // pairs so the read cannot widen past what this sweep claimed.
        let tenants: Vec<String> = rows.iter().map(|row| row.tenant_id.clone()).collect();
        let orders: Vec<String> = rows.iter().map(|row| row.id.clone()).collect();
        let line_rows = sqlx::query_as::<_, ClaimLineRow>(
            "SELECT tenant_id, order_id, position, item_slug, item_name, quantity, \
                    unit_price_cents, line_total_cents \
             FROM site_order_lines \
             WHERE (tenant_id, order_id) IN (SELECT * FROM unnest($1::text[], $2::text[])) \
             ORDER BY position",
        )
        .bind(&tenants)
        .bind(&orders)
        .fetch_all(self.pool())
        .await
        .map_err(StoreError::Db)?;

        Ok(rows
            .into_iter()
            .map(|row| {
                let lines = line_rows
                    .iter()
                    .filter(|line| line.tenant_id == row.tenant_id && line.order_id == row.id)
                    .map(ClaimLineRow::to_line)
                    .collect();
                row.into_notification(lines)
            })
            .collect())
    }
}

#[derive(sqlx::FromRow)]
struct ClaimRow {
    tenant_id: String,
    owner: String,
    site_name: String,
    site_subdomain: String,
    catalog_name: String,
    id: String,
    customer_name: String,
    customer_email: String,
    customer_phone: Option<String>,
    note: Option<String>,
    currency: String,
    total_cents: i64,
    received_at: OffsetDateTime,
}

impl ClaimRow {
    fn into_notification(self, lines: Vec<SiteOrderLine>) -> OrderNotification {
        OrderNotification {
            tenant: TenantId::new(self.tenant_id),
            owner: UserId::new(self.owner),
            site_name: self.site_name,
            site_subdomain: self.site_subdomain,
            catalog_name: self.catalog_name,
            order: SiteOrderId::new(self.id),
            customer_name: self.customer_name,
            customer_email: self.customer_email,
            customer_phone: self.customer_phone,
            note: self.note,
            currency: self.currency,
            total_cents: self.total_cents,
            received_at: self.received_at,
            lines,
        }
    }
}

#[derive(sqlx::FromRow)]
struct ClaimLineRow {
    tenant_id: String,
    order_id: String,
    position: i32,
    item_slug: String,
    item_name: String,
    quantity: i32,
    unit_price_cents: Option<i64>,
    line_total_cents: Option<i64>,
}

impl ClaimLineRow {
    fn to_line(&self) -> SiteOrderLine {
        SiteOrderLine {
            position: self.position,
            item_slug: self.item_slug.clone(),
            item_name: self.item_name.clone(),
            quantity: self.quantity,
            unit_price_cents: self.unit_price_cents,
            line_total_cents: self.line_total_cents,
        }
    }
}
