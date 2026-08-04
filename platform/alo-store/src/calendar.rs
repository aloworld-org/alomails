//! Calendar events, tenant/user-scoped through the account door exactly like
//! [`crate::contacts`]. Slice 1 is a single implicit calendar per user with
//! plain (non-recurring) timed or all-day events; recurrence, attendees,
//! multiple calendars, and the CalDAV sync-token/modseq wiring come in later
//! slices. Every statement carries `tenant_id = $tenant AND user_id = $user`,
//! so isolation is inherited from `AccountStore` and never assumed.

use time::format_description::well_known::Rfc3339;
use time::{Date, Duration, Month, OffsetDateTime};

use crate::account::AccountStore;
use crate::changes::{self, Change, TYPE_EVENT};
use crate::error::{Result, StoreError};
use crate::id::{CalendarId, EventId};
use crate::model::{Calendar, CalendarEvent, CalendarGrant, OccurrenceOverride};
use std::collections::HashMap;

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
        // Non-recurring events that overlap the window, plus every recurring
        // master that starts before the window ends (its occurrences may land
        // inside — expanded below). The master's own `starts_at`/`ends_at` are
        // the first occurrence.
        let visible = visible_pred();
        let sql = format!(
            "SELECT e.id, e.calendar_id, e.summary, e.description, e.location, e.starts_at, \
                    e.ends_at, e.all_day, e.rrule, e.attendees, e.exdates \
             FROM calendar_events e \
             WHERE e.tenant_id = $1 AND e.calendar_id IN ( \
                 SELECT c.id FROM calendars c WHERE c.tenant_id = $1 AND {visible}) AND ( \
                 (e.rrule IS NULL AND e.starts_at < $3 AND e.ends_at > $4) OR \
                 (e.rrule IS NOT NULL AND e.starts_at < $3)) \
             ORDER BY e.starts_at, e.id",
        );
        let rows = sqlx::query_as::<_, EventRow>(&sql)
            .bind(self.tenant.as_str())
            .bind(self.user.as_str())
            .bind(to)
            .bind(from)
            .fetch_all(&self.pool)
            .await?;
        let masters: Vec<CalendarEvent> = rows.into_iter().map(EventRow::into_event).collect();
        // Per-occurrence overrides for the recurring series in view. Fetched by
        // series id (the masters are already visible-scoped, so their overrides
        // are too); each replaces one occurrence in place.
        let series_ids: Vec<String> = masters
            .iter()
            .filter(|e| e.recurrence.is_some())
            .map(|e| e.id.as_str().to_owned())
            .collect();
        let overrides = self.overrides_for(&series_ids).await?;

        let mut out = Vec::new();
        for event in masters {
            if event.recurrence.is_none() {
                out.push(event);
                continue;
            }
            let ovs = overrides.get(event.id.as_str());
            let slots: Vec<OffsetDateTime> = ovs
                .map(|v| v.iter().map(|(slot, _)| *slot).collect())
                .unwrap_or_default();
            out.extend(expand_occurrences(&event, from, to, &slots));
            // Emit each override that lands in the window, in place of the
            // default occurrence it replaced (which expansion skipped).
            if let Some(ovs) = ovs {
                for (slot, ov) in ovs {
                    if ov.ends_at > from && ov.starts_at < to {
                        out.push(CalendarEvent {
                            id: event.id.clone(),
                            calendar_id: event.calendar_id.clone(),
                            summary: ov.summary.clone(),
                            description: ov.description.clone(),
                            location: ov.location.clone(),
                            starts_at: ov.starts_at,
                            ends_at: ov.ends_at,
                            all_day: ov.all_day,
                            recurrence: event.recurrence.clone(),
                            attendees: event.attendees.clone(),
                            exdates: Vec::new(),
                            recurrence_id: Some(*slot),
                        });
                    }
                }
            }
        }
        out.sort_by_key(|e| e.starts_at);
        Ok(out)
    }

    /// Loads every per-occurrence override for the given series, grouped by
    /// series id (slot → the overridden fields). Tenant-scoped.
    async fn overrides_for(
        &self,
        series_ids: &[String],
    ) -> Result<HashMap<String, Vec<(OffsetDateTime, OccurrenceOverride)>>> {
        if series_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows = sqlx::query_as::<_, OverrideRow>(
            "SELECT series_id, recurrence_id, summary, description, location, starts_at, ends_at, all_day \
             FROM calendar_event_overrides WHERE tenant_id = $1 AND series_id = ANY($2)",
        )
        .bind(self.tenant.as_str())
        .bind(series_ids)
        .fetch_all(&self.pool)
        .await?;
        let mut map: HashMap<String, Vec<(OffsetDateTime, OccurrenceOverride)>> = HashMap::new();
        for r in rows {
            map.entry(r.series_id).or_default().push((
                r.recurrence_id,
                OccurrenceOverride {
                    summary: r.summary,
                    description: r.description,
                    location: r.location,
                    starts_at: r.starts_at,
                    ends_at: r.ends_at,
                    all_day: r.all_day,
                },
            ));
        }
        Ok(map)
    }

    /// Every event in the account's calendar, earliest first. Used by the
    /// CalDAV collection listing (`PROPFIND` Depth:1, `calendar-query`), which
    /// returns the whole collection and lets the client narrow.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn all_events(&self) -> Result<Vec<CalendarEvent>> {
        let rows = sqlx::query_as::<_, EventRow>(
            "SELECT id, calendar_id, summary, description, location, starts_at, ends_at, all_day, rrule, attendees, exdates \
             FROM calendar_events WHERE tenant_id = $1 AND user_id = $2 \
             ORDER BY starts_at, id",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(EventRow::into_event).collect())
    }

    /// One event by id, or `None` when it is not this account's.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn event(&self, id: &EventId) -> Result<Option<CalendarEvent>> {
        let visible = visible_pred();
        let sql = format!(
            "SELECT e.id, e.calendar_id, e.summary, e.description, e.location, e.starts_at, \
                    e.ends_at, e.all_day, e.rrule, e.attendees, e.exdates \
             FROM calendar_events e \
             WHERE e.tenant_id = $1 AND e.id = $3 AND e.calendar_id IN ( \
                 SELECT c.id FROM calendars c WHERE c.tenant_id = $1 AND {visible})",
        );
        let row = sqlx::query_as::<_, EventRow>(&sql)
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
        // Only place an event on a calendar the caller can edit.
        if !self.can_edit_calendar(&event.calendar_id).await? {
            return Err(StoreError::NotFound);
        }
        let id = EventId::generate();
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        sqlx::query(
            "INSERT INTO calendar_events \
             (tenant_id, user_id, id, calendar_id, summary, description, location, \
              starts_at, ends_at, all_day, rrule, attendees, exdates) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(id.as_str())
        .bind(event.calendar_id.as_str())
        .bind(&event.summary)
        .bind(&event.description)
        .bind(&event.location)
        .bind(event.starts_at)
        .bind(event.ends_at)
        .bind(event.all_day)
        .bind(&event.recurrence)
        .bind(sqlx::types::Json(&event.attendees))
        .bind(exdates_to_json(&event.exdates))
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        changes::bump_and_record(
            &mut tx,
            self.tenant.as_str(),
            self.user.as_str(),
            &[Change::created(TYPE_EVENT, id.as_str())],
        )
        .await?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(id)
    }

    /// Creates or replaces an event at a **caller-chosen** id (CalDAV PUT: the
    /// client owns the resource href = iCalendar UID). Returns whether it was a
    /// create (`true`) or a replace. Advances the account modseq.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn put_event(&self, id: &EventId, event: &CalendarEvent) -> Result<bool> {
        if !self.can_edit_calendar(&event.calendar_id).await? {
            return Err(StoreError::NotFound);
        }
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let existed: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM calendar_events \
             WHERE tenant_id = $1 AND user_id = $2 AND id = $3)",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(id.as_str())
        .fetch_one(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        sqlx::query(
            "INSERT INTO calendar_events \
             (tenant_id, user_id, id, calendar_id, summary, description, location, \
              starts_at, ends_at, all_day, rrule, attendees, exdates) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13) \
             ON CONFLICT (tenant_id, user_id, id) DO UPDATE SET \
               calendar_id = EXCLUDED.calendar_id, \
               summary = EXCLUDED.summary, description = EXCLUDED.description, \
               location = EXCLUDED.location, starts_at = EXCLUDED.starts_at, \
               ends_at = EXCLUDED.ends_at, all_day = EXCLUDED.all_day, \
               rrule = EXCLUDED.rrule, attendees = EXCLUDED.attendees, \
               exdates = EXCLUDED.exdates, updated_at = now()",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(id.as_str())
        .bind(event.calendar_id.as_str())
        .bind(&event.summary)
        .bind(&event.description)
        .bind(&event.location)
        .bind(event.starts_at)
        .bind(event.ends_at)
        .bind(event.all_day)
        .bind(&event.recurrence)
        .bind(sqlx::types::Json(&event.attendees))
        .bind(exdates_to_json(&event.exdates))
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        let change = if existed {
            Change::updated(TYPE_EVENT, id.as_str())
        } else {
            Change::created(TYPE_EVENT, id.as_str())
        };
        changes::bump_and_record(&mut tx, self.tenant.as_str(), self.user.as_str(), &[change])
            .await?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(!existed)
    }

    /// Replaces an event's fields.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the event is not this account's;
    /// [`StoreError::Db`] on failure.
    pub async fn update_event(&self, id: &EventId, event: &CalendarEvent) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let editable = editable_pred();
        let sql = format!(
            "UPDATE calendar_events AS e SET summary = $4, description = $5, location = $6, \
                    starts_at = $7, ends_at = $8, all_day = $9, rrule = $10, attendees = $11, \
                    exdates = $12, calendar_id = $13, updated_at = now() \
             WHERE e.tenant_id = $1 AND e.id = $3 AND e.calendar_id IN ( \
                 SELECT c.id FROM calendars c WHERE c.tenant_id = $1 AND {editable})",
        );
        let done = sqlx::query(&sql)
            .bind(self.tenant.as_str())
            .bind(self.user.as_str())
            .bind(id.as_str())
            .bind(&event.summary)
            .bind(&event.description)
            .bind(&event.location)
            .bind(event.starts_at)
            .bind(event.ends_at)
            .bind(event.all_day)
            .bind(&event.recurrence)
            .bind(sqlx::types::Json(&event.attendees))
            .bind(exdates_to_json(&event.exdates))
            .bind(event.calendar_id.as_str())
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        changes::bump_and_record(
            &mut tx,
            self.tenant.as_str(),
            self.user.as_str(),
            &[Change::updated(TYPE_EVENT, id.as_str())],
        )
        .await?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(())
    }

    /// Deletes an event (idempotent to the caller only in that a missing event
    /// is [`StoreError::NotFound`]).
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the event is not this account's;
    /// [`StoreError::Db`] on failure.
    pub async fn delete_event(&self, id: &EventId) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let editable = editable_pred();
        let sql = format!(
            "DELETE FROM calendar_events AS e WHERE e.tenant_id = $1 AND e.id = $3 \
             AND e.calendar_id IN (SELECT c.id FROM calendars c WHERE c.tenant_id = $1 AND {editable})",
        );
        let done = sqlx::query(&sql)
            .bind(self.tenant.as_str())
            .bind(self.user.as_str())
            .bind(id.as_str())
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        // Deleting the series discards any per-occurrence overrides it carried.
        sqlx::query("DELETE FROM calendar_event_overrides WHERE tenant_id = $1 AND series_id = $2")
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        changes::bump_and_record(
            &mut tx,
            self.tenant.as_str(),
            self.user.as_str(),
            &[Change::destroyed(TYPE_EVENT, id.as_str())],
        )
        .await?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(())
    }

    /// Excludes a single occurrence of a recurring event (adds an `EXDATE`): the
    /// series stays, but the instance starting exactly at `occurrence` is
    /// skipped. Idempotent — excluding an already-excluded instant is a no-op.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the event is not this account's;
    /// [`StoreError::Db`] on failure.
    pub async fn exclude_occurrence(&self, id: &EventId, occurrence: OffsetDateTime) -> Result<()> {
        let Some(mut event) = self.event(id).await? else {
            return Err(StoreError::NotFound);
        };
        // Skipping a slot discards any in-place edit (override) it carried.
        sqlx::query(
            "DELETE FROM calendar_event_overrides \
             WHERE tenant_id = $1 AND series_id = $2 AND recurrence_id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(occurrence)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if !event.exdates.contains(&occurrence) {
            event.exdates.push(occurrence);
            self.update_event(id, &event).await?;
        }
        Ok(())
    }

    /// Overrides a single occurrence of a recurring series in place (iCalendar
    /// `RECURRENCE-ID`): the instance whose original start is `recurrence_id`
    /// takes on `ov`'s fields (possibly a new time), while the rest of the
    /// series is untouched. Upserts, so re-editing the same instance replaces
    /// the prior override. The caller must be able to edit the series' calendar.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the series is not visible/editable to the
    /// caller; [`StoreError::Db`] on failure.
    pub async fn override_occurrence(
        &self,
        series: &EventId,
        recurrence_id: OffsetDateTime,
        ov: &OccurrenceOverride,
    ) -> Result<()> {
        // Resolve the series through the account door (view access) and confirm
        // edit access to its calendar — same gate as editing the series itself.
        let Some(master) = self.event(series).await? else {
            return Err(StoreError::NotFound);
        };
        if !self.can_edit_calendar(&master.calendar_id).await? {
            return Err(StoreError::NotFound);
        }
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        sqlx::query(
            "INSERT INTO calendar_event_overrides \
                 (tenant_id, user_id, series_id, recurrence_id, summary, description, \
                  location, starts_at, ends_at, all_day) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) \
             ON CONFLICT (tenant_id, series_id, recurrence_id) DO UPDATE SET \
                 summary = EXCLUDED.summary, description = EXCLUDED.description, \
                 location = EXCLUDED.location, starts_at = EXCLUDED.starts_at, \
                 ends_at = EXCLUDED.ends_at, all_day = EXCLUDED.all_day, updated_at = now()",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(series.as_str())
        .bind(recurrence_id)
        .bind(&ov.summary)
        .bind(&ov.description)
        .bind(&ov.location)
        .bind(ov.starts_at)
        .bind(ov.ends_at)
        .bind(ov.all_day)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        // The series' sync state advances so CalDAV/JMAP clients re-read it.
        changes::bump_and_record(
            &mut tx,
            self.tenant.as_str(),
            self.user.as_str(),
            &[Change::updated(TYPE_EVENT, series.as_str())],
        )
        .await?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(())
    }

    /// The owner's calendars, creation order. Ensures the personal calendar
    /// exists first, so every account always has at least one.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn calendars(&self) -> Result<Vec<Calendar>> {
        self.ensure_personal_calendar().await?;
        // Owned first, then shared; each row carries the viewer's role.
        let editor = grant_exists(true);
        let visible = visible_pred();
        let sql = format!(
            "SELECT c.id, c.owner_user_id, c.name, c.color, c.kind, \
                CASE WHEN c.owner_user_id = $2 THEN 'owner' \
                     WHEN {editor} THEN 'editor' ELSE 'viewer' END AS role \
             FROM calendars c \
             WHERE c.tenant_id = $1 AND {visible} \
             ORDER BY (c.owner_user_id = $2) DESC, c.created_at, c.id",
        );
        let rows = sqlx::query_as::<_, CalendarRow>(&sql)
            .bind(self.tenant.as_str())
            .bind(self.user.as_str())
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(CalendarRow::into_calendar).collect())
    }

    /// Whether the viewer may edit the given calendar (owner, or an `editor`
    /// grant to them directly or via a group). Tenant-scoped.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn can_edit_calendar(&self, id: &CalendarId) -> Result<bool> {
        let editable = editable_pred();
        let sql = format!(
            "SELECT EXISTS (SELECT 1 FROM calendars c WHERE c.tenant_id = $1 AND c.id = $3 AND {editable})",
        );
        sqlx::query_scalar(&sql)
            .bind(self.tenant.as_str())
            .bind(self.user.as_str())
            .bind(id.as_str())
            .fetch_one(&self.pool)
            .await
            .map_err(StoreError::Db)
    }

    /// Shares a calendar the caller **owns** with a subject (a user or group) at
    /// a role. Upserts the role if the share exists. Only the owner may share.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the calendar is not the caller's own;
    /// [`StoreError::Db`] on failure.
    pub async fn grant_calendar(
        &self,
        id: &CalendarId,
        subject_kind: &str,
        subject_id: &str,
        role: &str,
    ) -> Result<()> {
        if !self.owns_calendar(id).await? {
            return Err(StoreError::NotFound);
        }
        sqlx::query(
            "INSERT INTO calendar_grants (tenant_id, calendar_id, subject_kind, subject_id, role) \
             VALUES ($1, $2, $3, $4, $5) \
             ON CONFLICT (tenant_id, calendar_id, subject_kind, subject_id) \
             DO UPDATE SET role = EXCLUDED.role",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(subject_kind)
        .bind(subject_id)
        .bind(role)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }

    /// Removes a share from a calendar the caller owns.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the calendar is not the caller's own;
    /// [`StoreError::Db`] on failure.
    pub async fn revoke_calendar(
        &self,
        id: &CalendarId,
        subject_kind: &str,
        subject_id: &str,
    ) -> Result<()> {
        if !self.owns_calendar(id).await? {
            return Err(StoreError::NotFound);
        }
        sqlx::query(
            "DELETE FROM calendar_grants \
             WHERE tenant_id = $1 AND calendar_id = $2 AND subject_kind = $3 AND subject_id = $4",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(subject_kind)
        .bind(subject_id)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }

    /// The shares on a calendar the caller owns (who it's shared with).
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the calendar is not the caller's own;
    /// [`StoreError::Db`] on failure.
    pub async fn calendar_grants(&self, id: &CalendarId) -> Result<Vec<CalendarGrant>> {
        if !self.owns_calendar(id).await? {
            return Err(StoreError::NotFound);
        }
        let rows: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT subject_kind, subject_id, role FROM calendar_grants \
             WHERE tenant_id = $1 AND calendar_id = $2 ORDER BY subject_kind, subject_id",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(subject_kind, subject_id, role)| CalendarGrant {
                subject_kind,
                subject_id,
                role,
            })
            .collect())
    }

    /// Whether this account owns the calendar (tenant-scoped).
    async fn owns_calendar(&self, id: &CalendarId) -> Result<bool> {
        sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM calendars \
             WHERE tenant_id = $1 AND owner_user_id = $2 AND id = $3)",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(id.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(StoreError::Db)
    }

    /// The user's personal (default) calendar id, creating it if absent. The id
    /// is deterministic (`cal_personal_<user>`) so it is stable across calls.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn ensure_personal_calendar(&self) -> Result<CalendarId> {
        let id = format!("cal_personal_{}", self.user.as_str());
        sqlx::query(
            "INSERT INTO calendars (tenant_id, id, owner_user_id, name, kind) \
             VALUES ($1, $2, $3, 'Personal', 'personal') \
             ON CONFLICT (tenant_id, id) DO NOTHING",
        )
        .bind(self.tenant.as_str())
        .bind(&id)
        .bind(self.user.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(CalendarId::new(id))
    }

    /// Creates a calendar owned by this user; returns its id.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn create_calendar(&self, name: &str, color: Option<&str>) -> Result<CalendarId> {
        let id = CalendarId::generate();
        sqlx::query(
            "INSERT INTO calendars (tenant_id, id, owner_user_id, name, color, kind) \
             VALUES ($1, $2, $3, $4, $5, 'shared')",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(self.user.as_str())
        .bind(name)
        .bind(color)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(id)
    }

    /// Renames / recolours a calendar the user owns.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when it is not the user's; [`StoreError::Db`].
    pub async fn update_calendar(
        &self,
        id: &CalendarId,
        name: &str,
        color: Option<&str>,
    ) -> Result<()> {
        let done = sqlx::query(
            "UPDATE calendars SET name = $4, color = $5, updated_at = now() \
             WHERE tenant_id = $1 AND owner_user_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(id.as_str())
        .bind(name)
        .bind(color)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Deletes a calendar the user owns **and all its events**. The personal
    /// calendar is protected and cannot be deleted.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when it is not the user's;
    /// [`StoreError::Conflict`] for the personal calendar; [`StoreError::Db`].
    pub async fn delete_calendar(&self, id: &CalendarId) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let kind: Option<String> = sqlx::query_scalar(
            "SELECT kind FROM calendars WHERE tenant_id = $1 AND owner_user_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(id.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        match kind.as_deref() {
            None => return Err(StoreError::NotFound),
            Some("personal") => {
                return Err(StoreError::Conflict(
                    "the personal calendar cannot be deleted".to_owned(),
                ));
            }
            _ => {}
        }
        // Drop any per-occurrence overrides of events in this calendar first.
        sqlx::query(
            "DELETE FROM calendar_event_overrides WHERE tenant_id = $1 AND series_id IN \
                 (SELECT id FROM calendar_events WHERE tenant_id = $1 AND calendar_id = $2)",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        sqlx::query(
            "DELETE FROM calendar_events WHERE tenant_id = $1 AND user_id = $2 AND calendar_id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(id.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        sqlx::query(
            "DELETE FROM calendars WHERE tenant_id = $1 AND owner_user_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(id.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        changes::bump_and_record(
            &mut tx,
            self.tenant.as_str(),
            self.user.as_str(),
            &[Change::destroyed(TYPE_EVENT, id.as_str())],
        )
        .await?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(())
    }
}

/// A raw `calendars` row.
#[derive(sqlx::FromRow)]
struct CalendarRow {
    id: String,
    owner_user_id: String,
    name: String,
    color: Option<String>,
    kind: String,
    #[sqlx(default)]
    role: Option<String>,
}

impl CalendarRow {
    fn into_calendar(self) -> Calendar {
        Calendar {
            id: CalendarId::new(self.id),
            owner: self.owner_user_id,
            name: self.name,
            color: self.color,
            kind: self.kind,
            role: self.role.unwrap_or_else(|| "owner".to_owned()),
        }
    }
}

/// SQL predicate `EXISTS(...)` — the viewer (tenant `$1`, user `$2`) has a grant
/// on calendar `c`. `editor_only` narrows it to `editor` grants. Matches a grant
/// to the user directly, or to any group the user belongs to. Tenant-scoped.
fn grant_exists(editor_only: bool) -> String {
    let role = if editor_only {
        " AND g.role = 'editor'"
    } else {
        ""
    };
    format!(
        "EXISTS (SELECT 1 FROM calendar_grants g \
         WHERE g.tenant_id = $1 AND g.calendar_id = c.id{role} AND ( \
           (g.subject_kind = 'user' AND g.subject_id = $2) \
           OR (g.subject_kind = 'group' AND g.subject_id IN \
              (SELECT group_id FROM group_members WHERE tenant_id = $1 AND user_id = $2))))"
    )
}

/// SQL predicate (calendar aliased `c`, viewer `$2`): the viewer may *see* the
/// calendar — they own it or hold any grant (direct or via a group).
fn visible_pred() -> String {
    let grant = grant_exists(false);
    format!("(c.owner_user_id = $2 OR {grant})")
}

/// SQL predicate (calendar aliased `c`, viewer `$2`): the viewer may *edit* the
/// calendar — they own it or hold an `editor` grant (direct or via a group).
fn editable_pred() -> String {
    let grant = grant_exists(true);
    format!("(c.owner_user_id = $2 OR {grant})")
}

/// Recurrence frequency (the `FREQ` of a supported `RRULE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Freq {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

/// Upper bound on generated occurrences per master, so an open-ended rule cannot
/// blow up a range query (~10 years of a daily event).
const MAX_OCCURRENCES: usize = 3660;

/// Expand a recurring master into the occurrences overlapping `[from, to)`. Each
/// occurrence is the master with `starts_at`/`ends_at` shifted — the series
/// shares one id and duration (per-occurrence exceptions are a later slice). An
/// unparseable rule degrades to the single master event.
fn expand_occurrences(
    master: &CalendarEvent,
    from: OffsetDateTime,
    to: OffsetDateTime,
    overridden: &[OffsetDateTime],
) -> Vec<CalendarEvent> {
    let Some((freq, interval, count, until)) = master.recurrence.as_deref().and_then(parse_rrule)
    else {
        return vec![master.clone()];
    };
    let duration = master.ends_at - master.starts_at;
    let mut out = Vec::new();
    let mut occ = master.starts_at;
    let mut total = 0usize;
    while total < MAX_OCCURRENCES && occ < to {
        if until.is_some_and(|u| occ > u) {
            break;
        }
        let Some(occ_end) = occ.checked_add(duration) else {
            break;
        };
        // Skip occurrences the owner cancelled (EXDATE) or moved/edited via a
        // per-occurrence override (those are emitted from the overrides table).
        let excluded = master.exdates.contains(&occ) || overridden.contains(&occ);
        if occ_end > from && !excluded {
            out.push(CalendarEvent {
                starts_at: occ,
                ends_at: occ_end,
                // The slot this occurrence fills — its stable edit/skip handle.
                recurrence_id: Some(occ),
                ..master.clone()
            });
        }
        total += 1;
        if count.is_some_and(|c| total >= c) {
            break;
        }
        let Some(next) = advance(occ, freq, interval) else {
            break;
        };
        if next <= occ {
            break; // guard against a non-advancing rule
        }
        occ = next;
    }
    out
}

fn advance(dt: OffsetDateTime, freq: Freq, interval: i64) -> Option<OffsetDateTime> {
    match freq {
        Freq::Daily => dt.checked_add(Duration::days(interval)),
        Freq::Weekly => dt.checked_add(Duration::weeks(interval)),
        Freq::Monthly => Some(add_months(dt, interval)),
        Freq::Yearly => Some(add_months(dt, interval * 12)),
    }
}

/// Add `months` calendar months, clamping the day to the target month's length
/// (Jan 31 + 1 month → Feb 28) and keeping the time of day.
fn add_months(dt: OffsetDateTime, months: i64) -> OffsetDateTime {
    let total = dt.year() as i64 * 12 + (dt.month() as i64 - 1) + months;
    let year = total.div_euclid(12) as i32;
    let month = Month::try_from((total.rem_euclid(12) + 1) as u8).unwrap_or(Month::January);
    let date = Date::from_calendar_date(year, month, dt.day())
        .or_else(|_| Date::from_calendar_date(year, month, 28))
        .unwrap_or_else(|_| dt.date());
    dt.replace_date(date)
}

/// Parse the supported subset of an `RRULE`: `FREQ` (required) plus optional
/// `INTERVAL`, `COUNT`, `UNTIL`. `BYDAY`/`BYMONTHDAY` etc. are ignored (an
/// occurrence keeps the master's weekday/day-of-month). `None` if no `FREQ`.
fn parse_rrule(rule: &str) -> Option<(Freq, i64, Option<usize>, Option<OffsetDateTime>)> {
    let mut freq = None;
    let mut interval = 1i64;
    let mut count = None;
    let mut until = None;
    for part in rule.trim().trim_start_matches("RRULE:").split(';') {
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        match key.trim().to_ascii_uppercase().as_str() {
            "FREQ" => {
                freq = match value.trim().to_ascii_uppercase().as_str() {
                    "DAILY" => Some(Freq::Daily),
                    "WEEKLY" => Some(Freq::Weekly),
                    "MONTHLY" => Some(Freq::Monthly),
                    "YEARLY" => Some(Freq::Yearly),
                    _ => None,
                };
            }
            "INTERVAL" => interval = value.trim().parse::<i64>().unwrap_or(1).max(1),
            "COUNT" => count = value.trim().parse().ok(),
            "UNTIL" => until = parse_until(value.trim()),
            _ => {}
        }
    }
    freq.map(|f| (f, interval, count, until))
}

/// Parse an `UNTIL` (`YYYYMMDD` or `YYYYMMDDTHHMMSS[Z]`) to a UTC instant; a
/// date-only value is taken as the end of that day (inclusive).
fn parse_until(value: &str) -> Option<OffsetDateTime> {
    let digits = value.trim_end_matches('Z');
    let year: i32 = digits.get(0..4)?.parse().ok()?;
    let month: u8 = digits.get(4..6)?.parse().ok()?;
    let day: u8 = digits.get(6..8)?.parse().ok()?;
    let date = Date::from_calendar_date(year, Month::try_from(month).ok()?, day).ok()?;
    let time = if digits.contains('T') {
        time::Time::from_hms(
            digits.get(9..11)?.parse().ok()?,
            digits.get(11..13)?.parse().ok()?,
            digits.get(13..15).and_then(|s| s.parse().ok()).unwrap_or(0),
        )
        .ok()?
    } else {
        time::Time::from_hms(23, 59, 59).ok()?
    };
    Some(OffsetDateTime::new_utc(date, time))
}

/// A raw `calendar_events` row.
#[derive(sqlx::FromRow)]
struct EventRow {
    id: String,
    calendar_id: String,
    summary: String,
    description: Option<String>,
    location: Option<String>,
    starts_at: OffsetDateTime,
    ends_at: OffsetDateTime,
    all_day: bool,
    rrule: Option<String>,
    attendees: sqlx::types::Json<Vec<String>>,
    // Stored as RFC 3339 strings: `time::OffsetDateTime` isn't serde-encodable
    // (the crate's serde feature is off), and strings keep the column readable.
    exdates: sqlx::types::Json<Vec<String>>,
}

/// A raw `calendar_event_overrides` row (a single overridden occurrence).
#[derive(sqlx::FromRow)]
struct OverrideRow {
    series_id: String,
    recurrence_id: OffsetDateTime,
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
            calendar_id: CalendarId::new(self.calendar_id),
            summary: self.summary,
            description: self.description,
            location: self.location,
            starts_at: self.starts_at,
            ends_at: self.ends_at,
            all_day: self.all_day,
            recurrence: self.rrule,
            attendees: self.attendees.0,
            exdates: exdates_from_json(self.exdates.0),
            // A stored row is always a master or one-off (overrides live in
            // their own table); expansion stamps the slot on each occurrence.
            recurrence_id: None,
        }
    }
}

/// Encode EXDATEs as an RFC 3339 string array for the JSONB column.
fn exdates_to_json(exdates: &[OffsetDateTime]) -> sqlx::types::Json<Vec<String>> {
    sqlx::types::Json(
        exdates
            .iter()
            .map(|t| t.format(&Rfc3339).unwrap_or_default())
            .collect(),
    )
}

/// Decode stored EXDATE strings back to instants, dropping any unparseable one.
fn exdates_from_json(raw: Vec<String>) -> Vec<OffsetDateTime> {
    raw.iter()
        .filter_map(|s| OffsetDateTime::parse(s, &Rfc3339).ok())
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use time::Time;

    fn ev(rrule: Option<&str>, y: i32, mo: u8, d: u8, h: u8) -> CalendarEvent {
        let start = OffsetDateTime::new_utc(
            Date::from_calendar_date(y, Month::try_from(mo).unwrap(), d).unwrap(),
            Time::from_hms(h, 0, 0).unwrap(),
        );
        CalendarEvent {
            id: EventId::new("m1".to_owned()),
            calendar_id: CalendarId::new("cal".to_owned()),
            summary: "Standup".to_owned(),
            description: None,
            location: None,
            starts_at: start,
            ends_at: start + Duration::minutes(30),
            all_day: false,
            recurrence: rrule.map(str::to_owned),
            attendees: vec![],
            exdates: vec![],
            recurrence_id: None,
        }
    }

    fn at(y: i32, mo: u8, d: u8) -> OffsetDateTime {
        OffsetDateTime::new_utc(
            Date::from_calendar_date(y, Month::try_from(mo).unwrap(), d).unwrap(),
            Time::MIDNIGHT,
        )
    }

    #[test]
    fn parses_rrule() {
        let (f, i, c, u) = parse_rrule("FREQ=WEEKLY;INTERVAL=2;COUNT=3").unwrap();
        assert_eq!(f, Freq::Weekly);
        assert_eq!(i, 2);
        assert_eq!(c, Some(3));
        assert!(u.is_none());
        assert!(parse_rrule("INTERVAL=2").is_none()); // no FREQ
    }

    #[test]
    fn weekly_expands_within_range() {
        let e = ev(Some("FREQ=WEEKLY"), 2026, 8, 3, 9);
        let occ = expand_occurrences(&e, at(2026, 8, 1), at(2026, 8, 31), &[]);
        assert_eq!(occ.len(), 4, "Aug 3/10/17/24 (31 is exclusive)");
        assert_eq!(occ[0].starts_at, e.starts_at);
        assert_eq!(occ[1].starts_at, e.starts_at + Duration::weeks(1));
        assert!(occ.iter().all(|o| o.id.as_str() == "m1"));
        assert!(
            occ.iter()
                .all(|o| (o.ends_at - o.starts_at) == Duration::minutes(30))
        );
    }

    #[test]
    fn exdate_skips_one_occurrence() {
        let mut e = ev(Some("FREQ=WEEKLY"), 2026, 8, 3, 9);
        // Cancel just Aug 10; the rest of the series stays.
        e.exdates.push(e.starts_at + Duration::weeks(1));
        let occ = expand_occurrences(&e, at(2026, 8, 1), at(2026, 8, 31), &[]);
        assert_eq!(occ.len(), 3, "Aug 3/17/24 (10 excluded)");
        assert!(
            occ.iter()
                .all(|o| o.starts_at != e.starts_at + Duration::weeks(1))
        );
        assert_eq!(occ[0].starts_at, e.starts_at); // Aug 3 kept
        assert_eq!(occ[1].starts_at, e.starts_at + Duration::weeks(2)); // Aug 17
    }

    #[test]
    fn override_skips_the_default_and_stamps_the_slot() {
        let e = ev(Some("FREQ=WEEKLY"), 2026, 8, 3, 9);
        let moved = e.starts_at + Duration::weeks(1); // the Aug 10 slot is edited
        let occ = expand_occurrences(&e, at(2026, 8, 1), at(2026, 8, 31), &[moved]);
        // The overridden slot's default is omitted (the override is emitted
        // separately from the overrides table); Aug 3/17/24 remain.
        assert_eq!(occ.len(), 3);
        assert!(occ.iter().all(|o| o.starts_at != moved));
        // Every occurrence carries its original slot as the recurrence id.
        assert!(occ.iter().all(|o| o.recurrence_id == Some(o.starts_at)));
    }

    #[test]
    fn count_caps_the_series() {
        let e = ev(Some("FREQ=DAILY;COUNT=3"), 2026, 8, 3, 9);
        let occ = expand_occurrences(&e, at(2026, 8, 1), at(2026, 9, 1), &[]);
        assert_eq!(occ.len(), 3);
    }

    #[test]
    fn until_stops_the_series() {
        let e = ev(Some("FREQ=DAILY;UNTIL=20260805"), 2026, 8, 3, 9);
        let occ = expand_occurrences(&e, at(2026, 8, 1), at(2026, 9, 1), &[]);
        assert_eq!(occ.len(), 3, "Aug 3/4/5, inclusive of the UNTIL day");
    }

    #[test]
    fn add_months_clamps_day() {
        let jan31 = OffsetDateTime::new_utc(
            Date::from_calendar_date(2026, Month::January, 31).unwrap(),
            Time::from_hms(9, 0, 0).unwrap(),
        );
        let feb = add_months(jan31, 1);
        assert_eq!(feb.month(), Month::February);
        assert_eq!(feb.day(), 28);
        assert_eq!(feb.hour(), 9);
    }
}
