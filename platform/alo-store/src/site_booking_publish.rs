//! Freezing a booking service into a publish, and reading it back.
//!
//! The sibling of [`crate::site_catalog_publish`], and it freezes the same
//! class of thing for the same reason: a published page must offer the
//! appointment length, the week and the questions that were true when the owner
//! pressed publish. An owner who shortens a consultation on Tuesday afternoon
//! has not changed what the page promised on Tuesday morning.
//!
//! One thing is deliberately **not** frozen: whether a particular time is free.
//! Availability is read live against the bound calendar every time a visitor
//! looks ([`crate::site_public_bookings`]) — a snapshot of free time would be
//! stale before it was written.

use serde_json::Value;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::{CalendarId, SiteBookingId, SiteId, SitePublishId};
use crate::site_bookings::{SiteBookingField, SiteBookingWindow};
use crate::site_model::{Section, SectionsEnvelope};

/// One booking service as a publish froze it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteBookingSnapshot {
    pub booking_id: SiteBookingId,
    pub name: String,
    pub description: Option<String>,
    /// The Agenda calendar availability is read against and the appointment is
    /// written into.
    pub calendar: CalendarId,
    pub time_zone: String,
    pub duration_minutes: i32,
    pub buffer_minutes: i32,
    pub notice_minutes: i32,
    pub horizon_days: i32,
    pub location: Option<String>,
    pub hours: Vec<SiteBookingWindow>,
    /// The extra questions asked on top of name and email.
    pub fields: Vec<SiteBookingField>,
    /// Whether the service was taking bookings at publish time.
    pub active: bool,
}

impl AccountStore {
    /// The immutable booking snapshots belonging to one tenant-owned publish.
    /// A foreign site/publish pair is indistinguishable from an empty result.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure; [`StoreError::Conflict`] when
    /// a stored snapshot cannot be read back.
    pub async fn site_publish_booking_snapshots(
        &self,
        site: &SiteId,
        publish: &SitePublishId,
    ) -> Result<Vec<SiteBookingSnapshot>> {
        let rows = sqlx::query_as::<_, SiteBookingSnapshotRow>(
            "SELECT booking_id, name, description, calendar_id, time_zone, duration_minutes, \
                    buffer_minutes, notice_minutes, horizon_days, location, hours, fields, active \
             FROM site_booking_snapshots \
             WHERE tenant_id = $1 AND site_id = $2 AND publish_id = $3 \
             ORDER BY booking_id",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(publish.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter()
            .map(SiteBookingSnapshotRow::into_snapshot)
            .collect()
    }

    /// Resolves one booking service exactly as publishing would, without
    /// writing a snapshot — the honest draft preview of what the next publish
    /// will offer.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the service is not this tenant's on this
    /// site; [`StoreError::Db`] on failure; [`StoreError::Conflict`] when the
    /// stored row cannot be read back.
    pub async fn site_booking_preview(
        &self,
        site: &SiteId,
        booking: &SiteBookingId,
    ) -> Result<SiteBookingSnapshot> {
        let row = sqlx::query_as::<_, SiteBookingSnapshotRow>(
            "SELECT id AS booking_id, name, description, calendar_id, time_zone, \
                    duration_minutes, buffer_minutes, notice_minutes, horizon_days, location, \
                    hours, fields, active \
             FROM site_bookings \
             WHERE tenant_id = $1 AND site_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(booking.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        row.ok_or(StoreError::NotFound)?.into_snapshot()
    }

    /// Freezes every booking service referenced by the pages of this publish.
    /// Called inside the publish transaction, so a service that cannot be
    /// frozen refuses the whole publish rather than producing a page that
    /// offers an appointment nothing can take.
    pub(crate) async fn freeze_referenced_bookings(
        &self,
        site: &SiteId,
        publish: &SitePublishId,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> Result<()> {
        let section_values = sqlx::query_scalar::<_, sqlx::types::Json<Value>>(
            "SELECT sections FROM site_page_snapshots \
             WHERE tenant_id = $1 AND publish_id = $2 ORDER BY page_id, locale",
        )
        .bind(self.tenant.as_str())
        .bind(publish.as_str())
        .fetch_all(&mut **tx)
        .await
        .map_err(StoreError::Db)?;
        let mut referenced = std::collections::BTreeSet::new();
        for stored in section_values {
            let envelope = SectionsEnvelope::from_value(stored.0).map_err(|error| {
                StoreError::Conflict(format!("site page has invalid booking content: {error}"))
            })?;
            for section in envelope.sections {
                if let Section::Booking(booking) = section {
                    referenced.insert(booking.booking_id.as_str().to_owned());
                }
            }
        }
        for booking in referenced {
            // One statement copies the editable row into the publish, scoped by
            // tenant AND site: a page cannot freeze another site's service even
            // inside the same tenant, and a section pointing at a service that
            // has since been deleted refuses the publish by name.
            let frozen = sqlx::query(
                "INSERT INTO site_booking_snapshots \
                    (tenant_id, publish_id, booking_id, site_id, name, description, calendar_id, \
                     time_zone, duration_minutes, buffer_minutes, notice_minutes, horizon_days, \
                     location, hours, fields, active) \
                 SELECT b.tenant_id, $3, b.id, b.site_id, b.name, b.description, b.calendar_id, \
                        b.time_zone, b.duration_minutes, b.buffer_minutes, b.notice_minutes, \
                        b.horizon_days, b.location, b.hours, b.fields, b.active \
                 FROM site_bookings b \
                 WHERE b.tenant_id = $1 AND b.site_id = $2 AND b.id = $4",
            )
            .bind(self.tenant.as_str())
            .bind(site.as_str())
            .bind(publish.as_str())
            .bind(&booking)
            .execute(&mut **tx)
            .await
            .map_err(StoreError::Db)?;
            if frozen.rows_affected() == 0 {
                return Err(StoreError::Conflict(
                    "a page offers a bookable service that no longer exists".to_owned(),
                ));
            }
        }
        Ok(())
    }
}

#[derive(sqlx::FromRow)]
pub(crate) struct SiteBookingSnapshotRow {
    booking_id: String,
    name: String,
    description: Option<String>,
    calendar_id: String,
    time_zone: String,
    duration_minutes: i32,
    buffer_minutes: i32,
    notice_minutes: i32,
    horizon_days: i32,
    location: Option<String>,
    hours: sqlx::types::Json<Value>,
    fields: sqlx::types::Json<Value>,
    active: bool,
}

impl SiteBookingSnapshotRow {
    pub(crate) fn into_snapshot(self) -> Result<SiteBookingSnapshot> {
        let hours: Vec<SiteBookingWindow> = serde_json::from_value(self.hours.0).map_err(|_| {
            StoreError::Conflict("booking snapshot has unreadable opening hours".to_owned())
        })?;
        let fields: Vec<SiteBookingField> =
            serde_json::from_value(self.fields.0).map_err(|_| {
                StoreError::Conflict("booking snapshot has unreadable questions".to_owned())
            })?;
        Ok(SiteBookingSnapshot {
            booking_id: SiteBookingId::new(self.booking_id),
            name: self.name,
            description: self.description,
            calendar: CalendarId::new(self.calendar_id),
            time_zone: self.time_zone,
            duration_minutes: self.duration_minutes,
            buffer_minutes: self.buffer_minutes,
            notice_minutes: self.notice_minutes,
            horizon_days: self.horizon_days,
            location: self.location,
            hours,
            fields,
            active: self.active,
        })
    }
}
