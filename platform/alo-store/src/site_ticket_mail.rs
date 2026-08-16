//! Claiming paid ticket sales for the buyer's ticket email (ADR 0050, item
//! S3.04h) — the store half of the first mail alo sends a stranger
//! automatically.
//!
//! A fulfilment row ([`crate::site_ticket_fulfil`]) with a NULL `mailed_at`
//! is a sale whose buyer has not been emailed their ticket. The mail sweep in
//! alo-jmap calls [`Store::claim_ticket_mails`] on an interval; the claim
//! sets `mailed_at` **in the same statement that selects the rows**, so a
//! crash between claim and send loses a mail but can never duplicate one —
//! the right trade here, because the ticket stays reachable on the checkout
//! return page and the order-status page either way.
//!
//! Only a sale whose fulfilment act has run is offered (`description <> ''`):
//! the mail quotes the fulfilment's own record of what was sold, written once
//! in the site's language, never a second read of the price list. A sale
//! whose act permanently fails is visible on its row and simply never mails.
//!
//! The abuse ceiling decided by ADR 0050 lives in the claim itself: a tenant
//! that has been sent `daily_cap` ticket mails in the last 24 hours has its
//! remaining sales **deferred** to the next window, never dropped. Precision
//! is per-round — concurrent claims in one statement each count the already
//! -mailed rows only — so a burst can overshoot by at most one batch, which
//! is why the sweep keeps its batches small.
//!
//! Nothing here chooses an identity: the From address is the deployment's
//! own (ADR 0050), and the Reply-To is resolved by the sweep through
//! `for_tenant(claim.tenant)` — the tenant-scoped door that answers `None`
//! for any other tenant's owner.

use crate::error::{Result, StoreError};
use crate::id::{SiteId, SiteTicketFulfilmentId, TenantId, UserId};
use crate::store::Store;

/// Everything the mail sweep needs to compose and address one ticket email,
/// resolved in the claim itself so the send never re-reads what the claim
/// already proved. Every field comes from the sale's own tenant's rows — the
/// claim's joins are tenant-paired, so a foreign tenant's sale cannot carry
/// another tenant's site, order, or owner.
#[derive(Debug, Clone)]
pub struct TicketMailNotification {
    /// The tenant the sale belongs to — the only tenant whose owner may
    /// appear in the message's Reply-To.
    pub tenant: TenantId,
    /// The site's creator: the human the buyer's reply should reach.
    pub owner: UserId,
    pub site: SiteId,
    pub site_name: String,
    pub site_subdomain: String,
    /// The site's default language — the words the buyer's mail is written
    /// in, resolved by the sweep's own tables.
    pub default_locale: String,
    /// The fulfilment row this mail makes good — and the message's id seed.
    pub fulfilment: SiteTicketFulfilmentId,
    /// The ticket the buyer holds: the public page's capability, which the
    /// mail links instead of attaching a second copy of.
    pub token: String,
    /// What was sold, as fulfilment recorded it: "<product> — <date>".
    pub description: String,
    pub buyer_name: String,
    pub buyer_email: String,
    pub quantity: i32,
    /// What was paid, as the order struck it: integer cents in the tenant's
    /// accounting currency.
    pub amount_cents: i64,
    pub currency: String,
}

#[derive(sqlx::FromRow)]
struct ClaimRow {
    tenant_id: String,
    owner: String,
    site_id: String,
    site_name: String,
    site_subdomain: String,
    default_locale: String,
    fulfilment_id: String,
    token: String,
    description: String,
    buyer_name: String,
    buyer_email: String,
    quantity: i32,
    amount_cents: i64,
    currency: String,
}

impl From<ClaimRow> for TicketMailNotification {
    fn from(row: ClaimRow) -> Self {
        TicketMailNotification {
            tenant: TenantId::new(row.tenant_id),
            owner: UserId::new(row.owner),
            site: SiteId::new(row.site_id),
            site_name: row.site_name,
            site_subdomain: row.site_subdomain,
            default_locale: row.default_locale,
            fulfilment: SiteTicketFulfilmentId::new(row.fulfilment_id),
            token: row.token,
            description: row.description,
            buyer_name: row.buyer_name,
            buyer_email: row.buyer_email,
            quantity: row.quantity,
            amount_cents: row.amount_cents,
            currency: row.currency,
        }
    }
}

impl Store {
    /// Claims up to `limit` fulfilled, unmailed ticket sales, oldest first,
    /// marking each mailed in the same statement (at-most-once — see the
    /// module doc), and skipping every sale of a tenant that already
    /// received `daily_cap` ticket mails in the last 24 hours — deferred,
    /// not dropped.
    ///
    /// Concurrent sweeps cannot double-claim: the update re-checks
    /// `mailed_at IS NULL` under the row lock, so the loser updates nothing.
    ///
    /// System-level by design: the sweep spans tenants, and each returned
    /// row carries the tenant the send must scope itself to.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn claim_ticket_mails(
        &self,
        limit: i64,
        daily_cap: i64,
    ) -> Result<Vec<TicketMailNotification>> {
        let rows = sqlx::query_as::<_, ClaimRow>(
            "WITH candidate AS ( \
                SELECT f.id, f.tenant_id, f.created_at, \
                       row_number() OVER (PARTITION BY f.tenant_id \
                                          ORDER BY f.created_at, f.id) AS position \
                  FROM site_ticket_fulfilments f \
                 WHERE f.mailed_at IS NULL AND f.description <> '' \
             ), eligible AS ( \
                SELECT c.id \
                  FROM candidate c \
                 WHERE c.position \
                       + (SELECT count(*) FROM site_ticket_fulfilments s \
                           WHERE s.tenant_id = c.tenant_id \
                             AND s.mailed_at > now() - interval '24 hours') <= $2 \
                 ORDER BY c.created_at, c.id \
                 LIMIT $1 \
             ) \
             UPDATE site_ticket_fulfilments f \
                SET mailed_at = now(), updated_at = now() \
               FROM eligible e, sites s, site_ticket_orders o \
              WHERE f.id = e.id \
                AND f.mailed_at IS NULL \
                AND s.tenant_id = f.tenant_id AND s.id = f.site_id \
                AND o.tenant_id = f.tenant_id AND o.id = f.order_id \
             RETURNING f.tenant_id, s.created_by AS owner, f.site_id, \
                       s.name AS site_name, s.subdomain AS site_subdomain, \
                       s.default_locale, f.id AS fulfilment_id, f.token, \
                       f.description, o.buyer_name, o.buyer_email, \
                       o.quantity, o.amount_cents, o.currency",
        )
        .bind(limit)
        .bind(daily_cap)
        .fetch_all(self.pool())
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(TicketMailNotification::from).collect())
    }
}
