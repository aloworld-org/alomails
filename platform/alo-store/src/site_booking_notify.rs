//! Turning new site appointments into pending owner notifications (ADR 0036;
//! the booking flow of [`crate::site_public_bookings`]), the third sibling of
//! [`crate::site_form_notify`] and [`crate::site_order_notify`].
//!
//! An appointment row with a NULL `notified_at` is one nobody has been told
//! about; the notifier sweep in alo-jmap calls
//! [`Store::claim_booking_notifications`] on an interval, builds one internal
//! message per claimed appointment, and delivers it through the **account
//! door** of the site's creator. Nothing here sends outbound mail: the
//! visitor's address travels as `Reply-To`, so answering is one deliberate
//! reply by the owner.
//!
//! Claiming is **at-most-once**: rows are marked notified in the same statement
//! that reads them, so a crash between claim and delivery loses a notification
//! but can never duplicate one. That is the right trade twice over here — the
//! appointment is already in the owner's Agenda calendar the moment it is
//! taken, so the notification is the second telling, never the only one.
//!
//! Only a **completed** reservation is offered: `status = 'booked'` and an
//! Agenda event actually written (`event_id IS NOT NULL`). The reservation
//! commits before its event, and a reservation whose event cannot be written is
//! withdrawn again — so a row still without an event is one the visitor was
//! never confirmed, and telling the owner about it would be telling them about
//! a booking that does not exist.

use serde_json::Value;
use time::OffsetDateTime;

use crate::error::{Result, StoreError};
use crate::id::{SiteBookingAppointmentId, TenantId, UserId};
use crate::site_public_bookings::BookingAnswer;
use crate::store::Store;

/// Everything the notifier needs to build and deliver one booking
/// notification: the appointment as the visitor took it, and the owning site's
/// context, resolved in the claim itself.
#[derive(Debug, Clone)]
pub struct BookingNotification {
    /// The tenant the appointment belongs to — the only tenant whose inbox this
    /// notification may reach.
    pub tenant: TenantId,
    /// The site's creator: the account whose inbox receives the message.
    pub owner: UserId,
    pub site_name: String,
    pub site_subdomain: String,
    pub appointment: SiteBookingAppointmentId,
    /// The service's name as it was published when the slot was taken.
    pub booking_name: String,
    pub visitor_name: String,
    pub visitor_email: String,
    pub starts_at: OffsetDateTime,
    pub ends_at: OffsetDateTime,
    /// IANA zone the published week was written in — the clock the visitor was
    /// offered the time in, and the one the owner should read it in.
    pub time_zone: String,
    /// The answers to the service's own questions, in the published order.
    pub answers: Vec<BookingAnswer>,
    /// When the appointment was taken.
    pub created_at: OffsetDateTime,
}

impl Store {
    /// Claims up to `limit` appointments awaiting notification, oldest first,
    /// marking each notified in the same statement (at-most-once — see the
    /// module doc). Concurrent sweeps skip each other's locked rows rather than
    /// double-claiming (`FOR UPDATE SKIP LOCKED`).
    ///
    /// System-level by design: the sweep spans tenants, and each returned row
    /// carries the tenant + owner the delivery must scope itself to.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn claim_booking_notifications(
        &self,
        limit: i64,
    ) -> Result<Vec<BookingNotification>> {
        let rows = sqlx::query_as::<_, ClaimRow>(
            "UPDATE site_booking_appointments a \
                SET notified_at = now() \
               FROM sites s \
              WHERE s.tenant_id = a.tenant_id AND s.id = a.site_id \
                AND (a.tenant_id, a.id) IN ( \
                    SELECT tenant_id, id FROM site_booking_appointments \
                     WHERE notified_at IS NULL AND status = 'booked' \
                       AND event_id IS NOT NULL \
                     ORDER BY created_at, id \
                     LIMIT $1 \
                     FOR UPDATE SKIP LOCKED) \
             RETURNING a.tenant_id, s.created_by AS owner, s.name AS site_name, \
                       s.subdomain AS site_subdomain, a.id, a.booking_name, \
                       a.visitor_name, a.visitor_email, a.starts_at, a.ends_at, \
                       a.time_zone, a.answers, a.created_at",
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
    id: String,
    booking_name: String,
    visitor_name: String,
    visitor_email: String,
    starts_at: OffsetDateTime,
    ends_at: OffsetDateTime,
    time_zone: String,
    answers: sqlx::types::Json<Value>,
    created_at: OffsetDateTime,
}

impl ClaimRow {
    fn into_notification(self) -> BookingNotification {
        // Answers that cannot be read back are dropped rather than allowed to
        // lose the whole notification: the time, the service and the visitor
        // are what the owner needs, and the appointment itself still carries
        // the answers verbatim.
        let answers: Vec<BookingAnswer> =
            serde_json::from_value(self.answers.0).unwrap_or_default();
        BookingNotification {
            tenant: TenantId::new(self.tenant_id),
            owner: UserId::new(self.owner),
            site_name: self.site_name,
            site_subdomain: self.site_subdomain,
            appointment: SiteBookingAppointmentId::new(self.id),
            booking_name: self.booking_name,
            visitor_name: self.visitor_name,
            visitor_email: self.visitor_email,
            starts_at: self.starts_at,
            ends_at: self.ends_at,
            time_zone: self.time_zone,
            answers,
            created_at: self.created_at,
        }
    }
}
