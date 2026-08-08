//! Turning new site-form submissions into pending owner notifications
//! (ADR 0036, `docs/design/sites.md` form flow). A submission row with a
//! NULL `notified_at` is one nobody has been told about; the notifier
//! sweep in alo-jmap calls [`Store::claim_form_notifications`] on an
//! interval, builds an internal message per claimed row, and delivers it
//! through the **account door** of the site's creator — the same
//! system-level sweep posture as [`Store::sweep_snoozes`].
//!
//! Claiming is **at-most-once**: rows are marked notified up front, in the
//! same statement that reads them, so a crash between claim and delivery
//! loses a notification but can never duplicate one. That is the right
//! trade here — the submission row itself stays visible in the owner's
//! submissions list either way, so nothing is ever silently lost.

use time::OffsetDateTime;

use crate::error::{Result, StoreError};
use crate::id::{SiteFormSubmissionId, TenantId, UserId};
use crate::store::Store;

/// Everything the notifier needs to build and deliver one owner
/// notification: the submission's posted fields plus the owning site's
/// context, resolved in the claim itself so delivery needs no further
/// lookups into sites the sweep did not claim.
#[derive(Debug, Clone)]
pub struct FormNotification {
    /// The tenant the submission belongs to — the only tenant whose inbox
    /// the notification may reach.
    pub tenant: TenantId,
    /// The site's creator: the account whose inbox receives the message.
    pub owner: UserId,
    pub site_name: String,
    pub site_subdomain: String,
    pub form_name: String,
    pub submission: SiteFormSubmissionId,
    pub sender_name: String,
    pub sender_email: String,
    pub message: String,
    pub received_at: OffsetDateTime,
}

impl Store {
    /// Claims up to `limit` submissions awaiting notification, oldest
    /// first, marking each notified in the same statement (at-most-once —
    /// see the module doc). Concurrent sweeps skip each other's locked
    /// rows rather than double-claiming (`FOR UPDATE SKIP LOCKED`).
    ///
    /// System-level by design: the sweep spans tenants, and each returned
    /// row carries the tenant + owner the delivery must scope itself to.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn claim_form_notifications(&self, limit: i64) -> Result<Vec<FormNotification>> {
        let rows = sqlx::query_as::<_, ClaimRow>(
            "UPDATE site_form_submissions sub \
                SET notified_at = now() \
               FROM site_forms f \
               JOIN sites s ON s.tenant_id = f.tenant_id AND s.id = f.site_id \
              WHERE f.tenant_id = sub.tenant_id AND f.id = sub.form_id \
                AND (sub.tenant_id, sub.id) IN ( \
                    SELECT tenant_id, id FROM site_form_submissions \
                     WHERE notified_at IS NULL \
                     ORDER BY received_at, id \
                     LIMIT $1 \
                     FOR UPDATE SKIP LOCKED) \
             RETURNING sub.tenant_id, s.created_by AS owner, s.name AS site_name, \
                       s.subdomain AS site_subdomain, f.name AS form_name, \
                       sub.id, sub.sender_name, sub.sender_email, sub.message, \
                       sub.received_at",
        )
        .bind(limit)
        .fetch_all(self.pool())
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(ClaimRow::into_notification).collect())
    }
}

#[derive(sqlx::FromRow)]
struct ClaimRow {
    tenant_id: String,
    owner: String,
    site_name: String,
    site_subdomain: String,
    form_name: String,
    id: String,
    sender_name: String,
    sender_email: String,
    message: String,
    received_at: OffsetDateTime,
}

impl ClaimRow {
    fn into_notification(self) -> FormNotification {
        FormNotification {
            tenant: TenantId::new(self.tenant_id),
            owner: UserId::new(self.owner),
            site_name: self.site_name,
            site_subdomain: self.site_subdomain,
            form_name: self.form_name,
            submission: SiteFormSubmissionId::new(self.id),
            sender_name: self.sender_name,
            sender_email: self.sender_email,
            message: self.message,
            received_at: self.received_at,
        }
    }
}
