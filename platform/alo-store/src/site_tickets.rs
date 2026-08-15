//! Ticketed events — the dated products a site sells seats to (ADR 0041,
//! wave one of alo Commerce).
//!
//! An event is already three things alo has: a calendar entry, a catalog
//! product, and an invoice. What this module owns is the one fact none of
//! them hold: **how many seats there are**. Everything else is a reference —
//! [`SiteTicketEvent::product`] points into Billing's price list, and the
//! name, the price and the VAT rate are asked of the catalog seam
//! ([`crate::billing_catalog_read`]) at render and at sale, never copied. A
//! create or a product change is validated against that seam, so an event can
//! only ever sell something the tenant's own price list answers for.
//!
//! Selling the seats — the hold that makes overselling impossible — is the
//! sibling module [`crate::site_ticket_holds`]; this one is the owner's model
//! of what is on sale. Every statement scopes by tenant AND site, so an event
//! is reachable neither from another tenant nor from another site of the
//! same tenant.

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::billing_catalog_read::BillingCatalogRead;
use crate::error::{Result, StoreError};
use crate::id::{BillingProductId, SiteId, SiteTicketEventId};

/// Maximum ticketed events one site may hold. Events are dated, so a busy
/// venue accumulates them; two hundred is a full calendar, a thousand is a
/// runaway loop.
pub const SITE_TICKET_EVENT_MAX_PER_SITE: i64 = 200;
/// The most seats one event may offer. Matches the migration's CHECK.
pub const SITE_TICKET_EVENT_MAX_CAPACITY: i32 = 100_000;

/// One ticketed event as the owner sees it: when it happens, how many seats
/// it has, and which price-list item a seat is sold as.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteTicketEvent {
    pub id: SiteTicketEventId,
    /// The price-list reference — resolve through
    /// [`BillingCatalogRead::sale_item`] for the name and the price *now*.
    pub product: BillingProductId,
    pub starts_at: OffsetDateTime,
    pub capacity: i32,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[derive(sqlx::FromRow)]
struct SiteTicketEventRow {
    id: String,
    product_id: String,
    starts_at: OffsetDateTime,
    capacity: i32,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl SiteTicketEventRow {
    fn into_event(self) -> SiteTicketEvent {
        SiteTicketEvent {
            id: SiteTicketEventId::new(self.id),
            product: BillingProductId::new(self.product_id),
            starts_at: self.starts_at,
            capacity: self.capacity,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

impl AccountStore {
    /// Creates a ticketed event on one of the tenant's sites, after checking
    /// the product against the tenant's own catalog seam — an event can only
    /// sell what the price list answers for.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the capacity is out of range or the
    /// product is not on the active price list (archived, foreign and
    /// never-existed are indistinguishable by design);
    /// [`StoreError::NotFound`] when the site is not this tenant's;
    /// [`StoreError::Conflict`] when the site is already at
    /// [`SITE_TICKET_EVENT_MAX_PER_SITE`]; [`StoreError::Db`] on failure.
    pub async fn create_site_ticket_event(
        &self,
        site: &SiteId,
        product: &BillingProductId,
        starts_at: OffsetDateTime,
        capacity: i32,
    ) -> Result<SiteTicketEventId> {
        validate_capacity(capacity)?;
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let existing: Option<i64> = sqlx::query_scalar(
            "SELECT (SELECT count(*) FROM site_ticket_events e \
                     WHERE e.tenant_id = s.tenant_id AND e.site_id = s.id) \
             FROM sites s WHERE s.tenant_id = $1 AND s.id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        let existing = existing.ok_or(StoreError::NotFound)?;
        if existing >= SITE_TICKET_EVENT_MAX_PER_SITE {
            return Err(StoreError::Conflict(format!(
                "a site may offer at most {SITE_TICKET_EVENT_MAX_PER_SITE} ticketed events"
            )));
        }
        // Checked after the site: a caller who cannot name the site is told
        // nothing about what is or is not on this tenant's price list.
        self.require_sale_item(product).await?;
        let id = SiteTicketEventId::generate();
        sqlx::query(
            "INSERT INTO site_ticket_events \
                (tenant_id, site_id, id, product_id, starts_at, capacity) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(id.as_str())
        .bind(product.as_str())
        .bind(starts_at)
        .bind(capacity)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(id)
    }

    /// A site's ticketed events in start order. A missing or foreign site
    /// simply has none.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn site_ticket_events(&self, site: &SiteId) -> Result<Vec<SiteTicketEvent>> {
        let rows = sqlx::query_as::<_, SiteTicketEventRow>(
            "SELECT id, product_id, starts_at, capacity, created_at, updated_at \
             FROM site_ticket_events \
             WHERE tenant_id = $1 AND site_id = $2 ORDER BY starts_at, id",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows
            .into_iter()
            .map(SiteTicketEventRow::into_event)
            .collect())
    }

    /// One ticketed event of one of the tenant's sites, or `None`.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn site_ticket_event(
        &self,
        site: &SiteId,
        event: &SiteTicketEventId,
    ) -> Result<Option<SiteTicketEvent>> {
        let row = sqlx::query_as::<_, SiteTicketEventRow>(
            "SELECT id, product_id, starts_at, capacity, created_at, updated_at \
             FROM site_ticket_events \
             WHERE tenant_id = $1 AND site_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(event.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(row.map(SiteTicketEventRow::into_event))
    }

    /// Changes an event's capacity. Growing is always allowed; shrinking is
    /// refused below the seats already committed (sold plus live holds), so
    /// no edit can make availability negative — the check runs under the same
    /// per-event lock the hold machinery takes, so a concurrent buyer cannot
    /// slip between the count and the write.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the capacity is out of range;
    /// [`StoreError::NotFound`] when the event is not this tenant's on this
    /// site; [`StoreError::Conflict`] when the new capacity is below the
    /// seats already committed; [`StoreError::Db`] on failure.
    pub async fn set_site_ticket_capacity(
        &self,
        site: &SiteId,
        event: &SiteTicketEventId,
        capacity: i32,
        now: OffsetDateTime,
    ) -> Result<()> {
        validate_capacity(capacity)?;
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        crate::site_ticket_holds::lock_ticket_event(&mut tx, &self.tenant, event).await?;
        let committed =
            crate::site_ticket_holds::seats_committed(&mut tx, &self.tenant, site, event, now)
                .await?
                .ok_or(StoreError::NotFound)?;
        if i64::from(capacity) < committed {
            return Err(StoreError::Conflict(format!(
                "{committed} seats are already sold or on hold; capacity cannot go below that"
            )));
        }
        let done = sqlx::query(
            "UPDATE site_ticket_events SET capacity = $4, updated_at = now() \
             WHERE tenant_id = $1 AND site_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(event.as_str())
        .bind(capacity)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(())
    }

    /// Deletes an event nobody has bought a seat to. Once a seat is sold the
    /// event is a record of a sale and stays; live and abandoned holds go
    /// with it (a hold is pre-payment, and its buyer is told at completion).
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the event is not this tenant's on this
    /// site; [`StoreError::Conflict`] when seats have been sold;
    /// [`StoreError::Db`] on failure.
    pub async fn delete_site_ticket_event(
        &self,
        site: &SiteId,
        event: &SiteTicketEventId,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        crate::site_ticket_holds::lock_ticket_event(&mut tx, &self.tenant, event).await?;
        let sold: Option<i64> = sqlx::query_scalar(
            "SELECT (SELECT count(*) FROM site_ticket_holds h \
                     WHERE h.tenant_id = e.tenant_id AND h.event_id = e.id \
                       AND h.state = 'completed') \
             FROM site_ticket_events e \
             WHERE e.tenant_id = $1 AND e.site_id = $2 AND e.id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(event.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        let sold = sold.ok_or(StoreError::NotFound)?;
        if sold > 0 {
            return Err(StoreError::Conflict(
                "tickets have been sold to this event; it can no longer be deleted".to_owned(),
            ));
        }
        sqlx::query(
            "DELETE FROM site_ticket_events WHERE tenant_id = $1 AND site_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(event.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(())
    }

    /// The product must be one the tenant's own catalog seam answers for —
    /// the same door the shop will price it through, so a reference that can
    /// be stored is a reference that can be sold.
    async fn require_sale_item(&self, product: &BillingProductId) -> Result<()> {
        let door = BillingCatalogRead::open(
            self.pool.clone(),
            self.blobs.clone(),
            self.tenant.clone(),
            self.user.clone(),
        );
        if door.sale_item(product).await?.is_none() {
            return Err(StoreError::Validation(
                "that item is not on the price list".to_owned(),
            ));
        }
        Ok(())
    }
}

fn validate_capacity(capacity: i32) -> Result<()> {
    if !(1..=SITE_TICKET_EVENT_MAX_CAPACITY).contains(&capacity) {
        return Err(StoreError::Validation(format!(
            "capacity must be between 1 and {SITE_TICKET_EVENT_MAX_CAPACITY} seats"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capacity_bounds_are_one_to_the_ceiling() {
        assert!(validate_capacity(1).is_ok());
        assert!(validate_capacity(SITE_TICKET_EVENT_MAX_CAPACITY).is_ok());
        for out in [0, -1, SITE_TICKET_EVENT_MAX_CAPACITY + 1] {
            assert!(
                matches!(validate_capacity(out), Err(StoreError::Validation(_))),
                "{out} was accepted"
            );
        }
    }
}
