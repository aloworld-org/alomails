//! Calendar events, tenant/user-scoped through the account door exactly like
//! [`crate::contacts`]. Slice 1 is a single implicit calendar per user with
//! plain (non-recurring) timed or all-day events; recurrence, attendees,
//! multiple calendars, and the CalDAV sync-token/modseq wiring come in later
//! slices. Every statement carries `tenant_id = $tenant AND user_id = $user`,
//! so isolation is inherited from `AccountStore` and never assumed.

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::EventId;
use crate::model::CalendarEvent;

impl AccountStore {
    /// The account's events overlapping the half-open window `[from, to)`,
    /// earliest first. An event overlaps when it starts before `to` and ends
    /// after `from`.
    ///
    /// # Errors
    /// [`StoreError::Db`](crate::error::StoreError::Db) on failure.
    pub async fn events_in_range(
        &self,
        from: OffsetDateTime,
        to: OffsetDateTime,
    ) -> Result<Vec<CalendarEvent>> {
        let rows = sqlx::query_as::<_, EventRow>(
            "SELECT id, summary, description, location, starts_at, ends_at, all_day \
             FROM calendar_events \
             WHERE tenant_id = $1 AND user_id = $2 AND starts_at < $3 AND ends_at > $4 \
             ORDER BY starts_at, id",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(to)
        .bind(from)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(EventRow::into_event).collect())
    }

    /// One event by id, or `None` when it is not this account's.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn event(&self, id: &EventId) -> Result<Option<CalendarEvent>> {
        let row = sqlx::query_as::<_, EventRow>(
            "SELECT id, summary, description, location, starts_at, ends_at, all_day \
             FROM calendar_events WHERE tenant_id = $1 AND user_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(EventRow::into_event))
    }

    /// Creates an event and returns its id. The caller validates the fields
    /// (non-empty summary, `ends_at >= starts_at`); the store persists what it
    /// is given.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn create_event(&self, event: &CalendarEvent) -> Result<EventId> {
        let id = EventId::generate();
        sqlx::query(
            "INSERT INTO calendar_events \
             (tenant_id, user_id, id, summary, description, location, \
              starts_at, ends_at, all_day) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(id.as_str())
        .bind(&event.summary)
        .bind(&event.description)
        .bind(&event.location)
        .bind(event.starts_at)
        .bind(event.ends_at)
        .bind(event.all_day)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(id)
    }

    /// Replaces an event's fields.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the event is not this account's;
    /// [`StoreError::Db`] on failure.
    pub async fn update_event(&self, id: &EventId, event: &CalendarEvent) -> Result<()> {
        let done = sqlx::query(
            "UPDATE calendar_events SET summary = $4, description = $5, location = $6, \
                    starts_at = $7, ends_at = $8, all_day = $9, updated_at = now() \
             WHERE tenant_id = $1 AND user_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(id.as_str())
        .bind(&event.summary)
        .bind(&event.description)
        .bind(&event.location)
        .bind(event.starts_at)
        .bind(event.ends_at)
        .bind(event.all_day)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Deletes an event (idempotent to the caller only in that a missing event
    /// is [`StoreError::NotFound`]).
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the event is not this account's;
    /// [`StoreError::Db`] on failure.
    pub async fn delete_event(&self, id: &EventId) -> Result<()> {
        let done = sqlx::query(
            "DELETE FROM calendar_events WHERE tenant_id = $1 AND user_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(id.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }
}

/// A raw `calendar_events` row.
#[derive(sqlx::FromRow)]
struct EventRow {
    id: String,
    summary: String,
    description: Option<String>,
    location: Option<String>,
    starts_at: OffsetDateTime,
    ends_at: OffsetDateTime,
    all_day: bool,
}

impl EventRow {
    fn into_event(self) -> CalendarEvent {
        CalendarEvent {
            id: EventId::new(self.id),
            summary: self.summary,
            description: self.description,
            location: self.location,
            starts_at: self.starts_at,
            ends_at: self.ends_at,
            all_day: self.all_day,
        }
    }
}
