//! Rooms and resources — the things a meeting needs besides people, and the
//! one rule that makes them worth having: a room cannot be in two meetings at
//! once.
//!
//! A resource is a calendar of kind `resource` in the tenant's name (see
//! migration `0911`), reached by an address. An event books it by naming that
//! address among its attendees; the store then holds the room for the event's
//! whole span — every occurrence of a series, moved instances included — and
//! **refuses the write** if any of it collides with a booking already held.
//! Nothing else can write into a room's calendar: `editable_pred` excludes
//! kind `resource`, so the only door into a room's schedule is this one.
//!
//! Times are never stored on the booking. The link says *which* room an event
//! holds; *when* is always read back from the event, through the same
//! expansion the week grid and CalDAV use ([`crate::calendar::expand_masters`]).
//! Move the meeting and the room moves with it — there is no second copy of
//! the times to fall out of step.
//!
//! Scoping: resources belong to the tenant, so reads are tenant-scoped rather
//! than user-scoped — anyone in the workspace may see the rooms and when they
//! are taken, which is the point of a shared room. Who may *create* one is the
//! route's question (`require_admin`), as it is for every other admin surface.

use std::collections::HashMap;

use time::{Duration, OffsetDateTime};

use crate::account::AccountStore;
use crate::calendar::expand_masters;
use crate::error::{Result, StoreError};
use crate::id::{CalendarId, EventId};
use crate::model::CalendarEvent;

/// A bookable thing: a room, a car, a projector. Its name and colour live on
/// the `calendars` row it *is*; these are the facts that make it bookable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalendarResource {
    /// The resource's calendar id — also its CalDAV collection segment.
    pub id: CalendarId,
    /// Display name ("Board room").
    pub name: String,
    /// The address an event names it by (unique per tenant, case-insensitive).
    pub email: String,
    /// Where it is ("2nd floor, east wing"), or `None`.
    pub location: Option<String>,
    /// How many people it seats, or `None` when nobody said.
    pub capacity: Option<i32>,
}

/// Longest stretch a single write reserves a room for. A series with no end
/// would otherwise ask the conflict check to expand forever; a year ahead is
/// how far a room booking is worth arguing about, and the check runs again on
/// every later save. Matches the widest range the calendar API will serve.
const MAX_BOOKING_DAYS: i64 = 400;

impl CalendarResource {
    /// Checks the resource's own invariants.
    ///
    /// # Errors
    /// [`StoreError::Validation`] naming the field at fault.
    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() || self.name.chars().count() > 120 {
            return Err(StoreError::Validation(
                "a resource needs a name of 1 to 120 characters".to_owned(),
            ));
        }
        if !is_address(&self.email) {
            return Err(StoreError::Validation(format!(
                "{:?} is not an address a meeting can name (expected \"room@example.com\")",
                self.email
            )));
        }
        if self
            .location
            .as_deref()
            .is_some_and(|l| l.chars().count() > 200)
        {
            return Err(StoreError::Validation(
                "a resource's location is at most 200 characters".to_owned(),
            ));
        }
        if self.capacity.is_some_and(|c| !(1..=100_000).contains(&c)) {
            return Err(StoreError::Validation(
                "a resource seats between 1 and 100000 people".to_owned(),
            ));
        }
        Ok(())
    }
}

/// The shape an address must have to be a resource's: `local@domain`, one `@`,
/// no whitespace, both halves non-empty and the domain dotted. Deliberately
/// narrow — this address is typed by an admin once and matched against
/// attendees forever, so a value that only *nearly* parses is a room nobody
/// can book.
fn is_address(email: &str) -> bool {
    let Some((local, domain)) = email.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && !domain.is_empty()
        && !domain.contains('@')
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !email.chars().any(char::is_whitespace)
        && email.chars().count() <= 254
}

impl AccountStore {
    /// Every resource in the tenant, by name. Any member may read this — the
    /// rooms are the workspace's.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn calendar_resources(&self) -> Result<Vec<CalendarResource>> {
        let rows = sqlx::query_as::<_, ResourceRow>(
            "SELECT r.calendar_id, c.name, r.email, r.location, r.capacity \
             FROM calendar_resources r \
             JOIN calendars c ON c.tenant_id = r.tenant_id AND c.id = r.calendar_id \
             WHERE r.tenant_id = $1 ORDER BY c.name, r.calendar_id",
        )
        .bind(self.tenant.as_str())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(ResourceRow::into_resource).collect())
    }

    /// One resource by id, or `None` when the tenant has no such room.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn calendar_resource(&self, id: &CalendarId) -> Result<Option<CalendarResource>> {
        let row = sqlx::query_as::<_, ResourceRow>(
            "SELECT r.calendar_id, c.name, r.email, r.location, r.capacity \
             FROM calendar_resources r \
             JOIN calendars c ON c.tenant_id = r.tenant_id AND c.id = r.calendar_id \
             WHERE r.tenant_id = $1 AND r.calendar_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(ResourceRow::into_resource))
    }

    /// The resource an address names, or `None` — how an attendee list is read
    /// as a booking, and how free/busy answers for a room the way it answers
    /// for a person. Case-insensitive.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn calendar_resource_by_email(
        &self,
        email: &str,
    ) -> Result<Option<CalendarResource>> {
        let row = sqlx::query_as::<_, ResourceRow>(
            "SELECT r.calendar_id, c.name, r.email, r.location, r.capacity \
             FROM calendar_resources r \
             JOIN calendars c ON c.tenant_id = r.tenant_id AND c.id = r.calendar_id \
             WHERE r.tenant_id = $1 AND lower(r.email) = lower($2)",
        )
        .bind(self.tenant.as_str())
        .bind(email.trim())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(ResourceRow::into_resource))
    }

    /// Creates a resource (its `calendars` row and its facts) and returns its
    /// id. The caller's admin-ness is the route's gate; this door only
    /// enforces the tenant.
    ///
    /// # Errors
    /// [`StoreError::Validation`] per [`CalendarResource::validate`];
    /// [`StoreError::Conflict`] when the address is already a room's or a
    /// person's; [`StoreError::Db`] on failure.
    pub async fn create_calendar_resource(
        &self,
        resource: &CalendarResource,
    ) -> Result<CalendarId> {
        resource.validate()?;
        self.refuse_taken_address(&resource.email, None).await?;
        let id = CalendarId::generate();
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        sqlx::query(
            "INSERT INTO calendars (tenant_id, id, owner_user_id, name, kind) \
             VALUES ($1, $2, $3, $4, 'resource')",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(self.user.as_str())
        .bind(resource.name.trim())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        sqlx::query(
            "INSERT INTO calendar_resources (tenant_id, calendar_id, email, location, capacity) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(resource.email.trim())
        .bind(resource.location.as_deref().map(str::trim))
        .bind(resource.capacity)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(id)
    }

    /// Replaces a resource's name, address, location and capacity.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the tenant has no such resource;
    /// [`StoreError::Validation`]; [`StoreError::Conflict`] on a taken
    /// address; [`StoreError::Db`] on failure.
    pub async fn update_calendar_resource(
        &self,
        id: &CalendarId,
        resource: &CalendarResource,
    ) -> Result<()> {
        resource.validate()?;
        self.refuse_taken_address(&resource.email, Some(id)).await?;
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let done = sqlx::query(
            "UPDATE calendar_resources SET email = $3, location = $4, capacity = $5, \
                 updated_at = now() WHERE tenant_id = $1 AND calendar_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(resource.email.trim())
        .bind(resource.location.as_deref().map(str::trim))
        .bind(resource.capacity)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        sqlx::query(
            "UPDATE calendars SET name = $3, updated_at = now() \
             WHERE tenant_id = $1 AND id = $2 AND kind = 'resource'",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(resource.name.trim())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(())
    }

    /// Retires a resource. Its bookings go with it (the meetings stay — they
    /// simply no longer hold a room), which is why the address may then be
    /// reused.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the tenant has no such resource;
    /// [`StoreError::Db`] on failure.
    pub async fn delete_calendar_resource(&self, id: &CalendarId) -> Result<()> {
        // The bookings cascade from calendar_resources, which cascades from
        // the calendars row: one delete, no orphans.
        let done = sqlx::query(
            "DELETE FROM calendars WHERE tenant_id = $1 AND id = $2 AND kind = 'resource'",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Refuses an address that already names a room (other than `except`) or a
    /// person in this tenant.
    async fn refuse_taken_address(&self, email: &str, except: Option<&CalendarId>) -> Result<()> {
        let taken: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM calendar_resources \
                 WHERE tenant_id = $1 AND lower(email) = lower($2) \
                   AND ($3::text IS NULL OR calendar_id <> $3))",
        )
        .bind(self.tenant.as_str())
        .bind(email.trim())
        .bind(except.map(CalendarId::as_str))
        .fetch_one(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if taken {
            return Err(StoreError::Conflict(format!(
                "another resource already answers to {}",
                email.trim()
            )));
        }
        let is_person: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM users WHERE tenant_id = $1 AND lower(email) = lower($2))",
        )
        .bind(self.tenant.as_str())
        .bind(email.trim())
        .fetch_one(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if is_person {
            return Err(StoreError::Conflict(format!(
                "{} is a person's address, not a resource's",
                email.trim()
            )));
        }
        Ok(())
    }

    /// The occurrences booked into `resource` that fall in `[from, to)`,
    /// earliest first — whoever owns them. Expanded through the one expansion,
    /// so a moved instance is busy where it moved to and a cancelled one frees
    /// its slot.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn resource_bookings_in_range(
        &self,
        resource: &CalendarId,
        from: OffsetDateTime,
        to: OffsetDateTime,
    ) -> Result<Vec<CalendarEvent>> {
        let mut conn = self.pool.acquire().await.map_err(StoreError::Db)?;
        self.bookings_in_range(&mut conn, resource, from, to, None)
            .await
    }

    /// [`Self::resource_bookings_in_range`] on a caller-supplied connection,
    /// optionally blind to one event — the conflict check runs inside the
    /// booking transaction (so it sees the room lock it took) and must not
    /// find the meeting it is checking.
    async fn bookings_in_range(
        &self,
        conn: &mut sqlx::PgConnection,
        resource: &CalendarId,
        from: OffsetDateTime,
        to: OffsetDateTime,
        ignoring: Option<&EventId>,
    ) -> Result<Vec<CalendarEvent>> {
        // Same window rule as `events_in_range`: one-offs must overlap, a
        // series only has to start before the window ends (its later
        // occurrences may land inside).
        let rows = sqlx::query_as::<_, crate::calendar::EventRow>(
            "SELECT e.id, e.calendar_id, e.summary, e.description, e.location, e.starts_at, \
                    e.ends_at, e.all_day, e.rrule, e.attendees, e.exdates, e.tzid, e.rdates, \
                    e.reminder_minutes, e.attendee_status \
             FROM calendar_events e \
             JOIN calendar_resource_bookings b \
               ON b.tenant_id = e.tenant_id AND b.event_id = e.id \
             WHERE e.tenant_id = $1 AND b.resource_id = $2 \
               AND ($5::text IS NULL OR e.id <> $5) AND ( \
                 (e.rrule IS NULL AND jsonb_array_length(e.rdates) = 0 \
                  AND e.starts_at < $3 AND e.ends_at > $4) OR \
                 ((e.rrule IS NOT NULL OR jsonb_array_length(e.rdates) > 0) \
                  AND e.starts_at < $3)) \
             ORDER BY e.starts_at, e.id",
        )
        .bind(self.tenant.as_str())
        .bind(resource.as_str())
        .bind(to)
        .bind(from)
        .bind(ignoring.map(EventId::as_str))
        .fetch_all(&mut *conn)
        .await?;
        let masters: Vec<CalendarEvent> = rows
            .into_iter()
            .map(crate::calendar::EventRow::into_event)
            .collect();
        let series_ids: Vec<String> = masters
            .iter()
            .filter(|e| e.recurrence.is_some() || !e.rdates.is_empty())
            .map(|e| e.id.as_str().to_owned())
            .collect();
        let overrides = self.overrides_for(&series_ids).await?;
        Ok(expand_masters(masters, &overrides, from, to))
    }

    /// Holds `resources` for the event `id` — replacing whatever it held
    /// before — or refuses the lot.
    ///
    /// Called **before** the event is written, so a refusal leaves nothing
    /// behind to undo. The check is the room's own schedule against every
    /// occurrence of this event over the next [`MAX_BOOKING_DAYS`]; a series
    /// that collides anywhere is refused whole (naming the first collision),
    /// because half a booked series is not something a room can honour.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when a named resource is not this tenant's;
    /// [`StoreError::Conflict`] naming the room and the taken slot;
    /// [`StoreError::Db`] on failure.
    pub async fn book_resources(
        &self,
        id: &EventId,
        event: &CalendarEvent,
        resources: &[CalendarId],
    ) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        // Take the rooms' rows first. Two people booking the same room in the
        // same second is exactly the case this method exists for, and a check
        // that reads outside the transaction that writes cannot rule it out:
        // the lock makes the second write wait for the first to land, so it
        // sees it and refuses.
        let ids: Vec<String> = resources.iter().map(|r| r.as_str().to_owned()).collect();
        let locked: Vec<(String, String)> = sqlx::query_as(
            "SELECT r.calendar_id, c.name FROM calendar_resources r \
             JOIN calendars c ON c.tenant_id = r.tenant_id AND c.id = r.calendar_id \
             WHERE r.tenant_id = $1 AND r.calendar_id = ANY($2) \
             ORDER BY r.calendar_id FOR UPDATE OF r",
        )
        .bind(self.tenant.as_str())
        .bind(&ids)
        .fetch_all(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        if locked.len() != resources.len() {
            // A room of another tenant, or one that was retired mid-edit.
            return Err(StoreError::NotFound);
        }
        // Always clear this event's previous holds: an edit that drops the
        // room must let go of it.
        sqlx::query(
            "DELETE FROM calendar_resource_bookings WHERE tenant_id = $1 AND event_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        let (from, to) = booking_window(event);
        let wanted = expand_masters(
            vec![CalendarEvent {
                id: id.clone(),
                ..event.clone()
            }],
            &HashMap::new(),
            from,
            to,
        );
        for resource in resources {
            let held = self
                .bookings_in_range(&mut tx, resource, from, to, Some(id))
                .await?;
            if let Some(clash) = first_overlap(&wanted, &held) {
                let name = locked
                    .iter()
                    .find(|(cal, _)| cal == resource.as_str())
                    .map_or_else(|| "the resource".to_owned(), |(_, name)| name.clone());
                return Err(StoreError::Conflict(format!(
                    "{name} is already booked from {} to {}",
                    format_instant(clash.0),
                    format_instant(clash.1),
                )));
            }
            sqlx::query(
                "INSERT INTO calendar_resource_bookings (tenant_id, resource_id, event_id) \
                 VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
            )
            .bind(self.tenant.as_str())
            .bind(resource.as_str())
            .bind(id.as_str())
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        }
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(())
    }

    /// Releases every room an event holds (used when a write that had already
    /// reserved one could not go through).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn unbook_event(&self, id: &EventId) -> Result<()> {
        sqlx::query(
            "DELETE FROM calendar_resource_bookings WHERE tenant_id = $1 AND event_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }
}

/// The stretch a write reserves: the event itself for a one-off, and for a
/// series everything up to [`MAX_BOOKING_DAYS`] out — expansion stops earlier
/// on its own at a `COUNT` or an `UNTIL`.
fn booking_window(event: &CalendarEvent) -> (OffsetDateTime, OffsetDateTime) {
    let from = event.starts_at.min(event.ends_at);
    let to = if event.recurrence.is_some() || !event.rdates.is_empty() {
        from + Duration::days(MAX_BOOKING_DAYS)
    } else {
        event.ends_at.max(from + Duration::seconds(1))
    };
    (from, to)
}

/// The first instant two occurrence lists collide at, as `(start, end)` of the
/// occupied slot. Half-open: a meeting that ends exactly when the next begins
/// is not a clash, which is what back-to-back bookings depend on.
fn first_overlap(
    wanted: &[CalendarEvent],
    held: &[CalendarEvent],
) -> Option<(OffsetDateTime, OffsetDateTime)> {
    let mut clashes: Vec<(OffsetDateTime, OffsetDateTime)> = Vec::new();
    for w in wanted {
        for h in held {
            if w.starts_at < h.ends_at && h.starts_at < w.ends_at {
                clashes.push((h.starts_at, h.ends_at));
            }
        }
    }
    clashes.into_iter().min()
}

/// An instant as a refusal spells it: RFC 3339 UTC, the same currency the
/// calendar's wire uses, so a client can parse it back and show it locally.
fn format_instant(t: OffsetDateTime) -> String {
    t.format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

/// A resource joined to the `calendars` row it is.
#[derive(sqlx::FromRow)]
struct ResourceRow {
    calendar_id: String,
    name: String,
    email: String,
    location: Option<String>,
    capacity: Option<i32>,
}

impl ResourceRow {
    fn into_resource(self) -> CalendarResource {
        CalendarResource {
            id: CalendarId::new(self.calendar_id),
            name: self.name,
            email: self.email,
            location: self.location,
            capacity: self.capacity,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use time::{Date, Month, Time};

    fn odt(y: i32, mo: u8, d: u8, h: u8, mi: u8) -> OffsetDateTime {
        OffsetDateTime::new_utc(
            Date::from_calendar_date(y, Month::try_from(mo).unwrap(), d).unwrap(),
            Time::from_hms(h, mi, 0).unwrap(),
        )
    }

    fn event(start: OffsetDateTime, end: OffsetDateTime) -> CalendarEvent {
        CalendarEvent {
            id: EventId::new("e"),
            calendar_id: CalendarId::new("c"),
            summary: "Meeting".to_owned(),
            description: None,
            location: None,
            starts_at: start,
            ends_at: end,
            all_day: false,
            recurrence: None,
            attendees: Vec::new(),
            exdates: Vec::new(),
            timezone: None,
            rdates: Vec::new(),
            recurrence_id: None,
            reminder_minutes: None,
            attendee_status: Vec::new(),
        }
    }

    fn resource(email: &str) -> CalendarResource {
        CalendarResource {
            id: CalendarId::new("r"),
            name: "Board room".to_owned(),
            email: email.to_owned(),
            location: None,
            capacity: Some(8),
        }
    }

    #[test]
    fn a_resource_needs_a_name_an_address_and_a_sane_capacity() {
        resource("board@example.com").validate().unwrap();
        for bad in ["", "   ", &"x".repeat(121)] {
            let mut r = resource("board@example.com");
            r.name = bad.to_owned();
            assert!(r.validate().is_err(), "name {bad:?} accepted");
        }
        for bad in [
            "",
            "board",
            "board@",
            "@example.com",
            "board@example",
            "board@.com",
            "board@example.",
            "a@b.com c@d.com",
            "two@@example.com",
        ] {
            assert!(
                resource(bad).validate().is_err(),
                "address {bad:?} accepted"
            );
        }
        let mut r = resource("board@example.com");
        r.capacity = Some(0);
        assert!(r.validate().is_err());
        r.capacity = None;
        r.validate().unwrap();
        r.location = Some("x".repeat(201));
        assert!(r.validate().is_err());
    }

    #[test]
    fn back_to_back_bookings_do_not_clash_but_overlaps_do() {
        let wanted = vec![event(odt(2026, 9, 2, 10, 0), odt(2026, 9, 2, 11, 0))];
        // Ends exactly when the wanted slot starts, and one that starts exactly
        // when it ends: a room is free at the boundary.
        let touching = vec![
            event(odt(2026, 9, 2, 9, 0), odt(2026, 9, 2, 10, 0)),
            event(odt(2026, 9, 2, 11, 0), odt(2026, 9, 2, 12, 0)),
        ];
        assert_eq!(first_overlap(&wanted, &touching), None);
        let held = vec![
            event(odt(2026, 9, 2, 14, 0), odt(2026, 9, 2, 15, 0)),
            event(odt(2026, 9, 2, 10, 30), odt(2026, 9, 2, 11, 30)),
        ];
        // The earliest colliding slot is the one named, whatever the order.
        assert_eq!(
            first_overlap(&wanted, &held),
            Some((odt(2026, 9, 2, 10, 30), odt(2026, 9, 2, 11, 30)))
        );
    }

    #[test]
    fn the_booking_window_covers_a_one_off_exactly_and_a_series_a_year_out() {
        let one_off = event(odt(2026, 9, 2, 10, 0), odt(2026, 9, 2, 11, 0));
        assert_eq!(
            booking_window(&one_off),
            (odt(2026, 9, 2, 10, 0), odt(2026, 9, 2, 11, 0))
        );
        // A zero-length event still asks about a non-empty window, else it
        // would reserve nothing and clash with nothing.
        let instant = event(odt(2026, 9, 2, 10, 0), odt(2026, 9, 2, 10, 0));
        let (from, to) = booking_window(&instant);
        assert!(to > from);
        let series = CalendarEvent {
            recurrence: Some("FREQ=WEEKLY".to_owned()),
            ..one_off.clone()
        };
        let (from, to) = booking_window(&series);
        assert_eq!(to - from, Duration::days(MAX_BOOKING_DAYS));
        // An RDATE-only series recurs too.
        let rdates = CalendarEvent {
            rdates: vec![odt(2026, 10, 2, 10, 0)],
            ..one_off
        };
        let (from, to) = booking_window(&rdates);
        assert_eq!(to - from, Duration::days(MAX_BOOKING_DAYS));
    }

    #[test]
    fn a_refusal_spells_the_slot_in_the_wires_own_currency() {
        assert_eq!(
            format_instant(odt(2026, 9, 2, 10, 0)),
            "2026-09-02T10:00:00Z"
        );
    }
}
