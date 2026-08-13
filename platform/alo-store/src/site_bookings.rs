//! alo Sites booking services — what a visitor may book on a published site.
//!
//! A booking service is three things kept together because they are decided
//! together: the **availability source** (an Agenda calendar, resolved through
//! the Sites-owned seam in [`crate::site_agenda`] and never reached for
//! directly), the **weekly pattern** the owner is open for it, and the
//! **questions** a visitor answers when booking it.
//!
//! Two shapes deserve their names.
//!
//! * **Opening hours are declared, not inferred.** A free calendar is not an
//!   invitation: a dentist whose Sunday is empty is not open on Sunday. The
//!   weekly windows say when the service is offered, and the calendar can only
//!   ever take slots away ([`crate::site_agenda`], S2.13b). Weekdays are ISO
//!   8601 — 1 is Monday, 7 is Sunday — and the bounds are minutes from
//!   midnight in the service's own [`time_zone`](SiteBooking::time_zone), so a
//!   change of daylight saving moves the appointments with the clock rather
//!   than an hour off it.
//! * **Name and email are structural.** They are not in
//!   [`fields`](SiteBooking::fields) and cannot be removed: an appointment
//!   nobody can be told about, or reminded of, is not a booking. The field
//!   schema is what a *particular* business needs on top of them — a phone
//!   number, a car registration, which treatment.
//!
//! Every statement scopes by tenant AND site, so a booking service can be
//! reached neither from another tenant nor from another site of the same
//! tenant. Nothing here is public: taking a booking is the public flow's job
//! (S2.13b), and no visitor data is stored by this module at all.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::{CalendarId, SiteBookingId, SiteId};

/// Maximum booking services one site may hold. A site offers a handful of
/// things to book; a hundred is a runaway loop, not a business.
pub const SITE_BOOKING_MAX_PER_SITE: i64 = 20;
/// Maximum length of the service's public name ("Thirty-minute consultation").
pub const SITE_BOOKING_NAME_MAX_CHARS: usize = 120;
/// Maximum length of the description shown beside it.
pub const SITE_BOOKING_DESCRIPTION_MAX_CHARS: usize = 2_000;
/// Maximum length of the where-it-happens line ("Second floor, ring the bell").
pub const SITE_BOOKING_LOCATION_MAX_CHARS: usize = 300;
/// Maximum length of an IANA time zone name.
pub const SITE_BOOKING_TIME_ZONE_MAX_CHARS: usize = 64;
/// Shortest appointment that can be offered.
pub const SITE_BOOKING_MIN_DURATION_MINUTES: i32 = 5;
/// Longest appointment that can be offered — a working day.
pub const SITE_BOOKING_MAX_DURATION_MINUTES: i32 = 480;
/// Longest quiet time kept after an appointment.
pub const SITE_BOOKING_MAX_BUFFER_MINUTES: i32 = 240;
/// Longest notice a visitor may be asked for — thirty days.
pub const SITE_BOOKING_MAX_NOTICE_MINUTES: i32 = 43_200;
/// Furthest ahead the public calendar may open.
pub const SITE_BOOKING_MAX_HORIZON_DAYS: i32 = 365;
/// Maximum weekly opening windows — three a day, every day.
pub const SITE_BOOKING_MAX_WINDOWS: usize = 21;
/// Maximum extra questions asked on top of name and email.
pub const SITE_BOOKING_MAX_FIELDS: usize = 8;
/// Maximum length of a question's stable key.
pub const SITE_BOOKING_FIELD_KEY_MAX_CHARS: usize = 40;
/// Maximum length of a question's visible label.
pub const SITE_BOOKING_FIELD_LABEL_MAX_CHARS: usize = 120;
/// Maximum number of answers a choice question may offer.
pub const SITE_BOOKING_FIELD_MAX_OPTIONS: usize = 20;
/// Maximum length of one such answer.
pub const SITE_BOOKING_FIELD_OPTION_MAX_CHARS: usize = 120;
/// Minutes in a day — the exclusive upper bound of a window.
const MINUTES_IN_DAY: i32 = 24 * 60;

/// One weekly window the service is offered in, in the service's own time
/// zone. `weekday` is ISO 8601 (1 = Monday … 7 = Sunday) and the bounds are
/// minutes from midnight, `end_minute` exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SiteBookingWindow {
    pub weekday: i32,
    pub start_minute: i32,
    pub end_minute: i32,
}

/// What kind of answer a question takes. Deliberately small: every kind here
/// is something a public form can validate and an owner can read at a glance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SiteBookingFieldKind {
    /// One line of text.
    Text,
    /// Several lines ("anything we should know?").
    LongText,
    /// A telephone number.
    Phone,
    /// One of the offered answers, and nothing else.
    Choice,
}

impl SiteBookingFieldKind {
    /// The exact wire/stored word for this kind.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            SiteBookingFieldKind::Text => "text",
            SiteBookingFieldKind::LongText => "long_text",
            SiteBookingFieldKind::Phone => "phone",
            SiteBookingFieldKind::Choice => "choice",
        }
    }

    /// Parses one of the four words. An unknown one names all four rather than
    /// falling back to text, because a question silently changing kind changes
    /// what a visitor is asked.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the word is not one of the four.
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "text" => Ok(SiteBookingFieldKind::Text),
            "long_text" => Ok(SiteBookingFieldKind::LongText),
            "phone" => Ok(SiteBookingFieldKind::Phone),
            "choice" => Ok(SiteBookingFieldKind::Choice),
            other => Err(StoreError::Validation(format!(
                "{other} is not a question kind; use text, long_text, phone, or choice"
            ))),
        }
    }
}

/// One extra question a visitor answers when booking.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SiteBookingField {
    /// Stable key the answer is stored under — lowercase letters, digits and
    /// underscores. It outlives the label: renaming the question must not
    /// orphan the answers already taken.
    pub key: String,
    /// What the visitor reads.
    pub label: String,
    pub kind: SiteBookingFieldKind,
    pub required: bool,
    /// The offered answers, for [`SiteBookingFieldKind::Choice`] only.
    #[serde(default)]
    pub options: Vec<String>,
}

/// One bookable service of a site, as its owner sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteBooking {
    pub id: SiteBookingId,
    pub name: String,
    pub description: Option<String>,
    /// The Agenda calendar this service is read against and booked into,
    /// resolved through [`AccountStore::site_availability_source`].
    pub calendar: CalendarId,
    /// IANA zone the opening hours are written in.
    pub time_zone: String,
    pub duration_minutes: i32,
    /// Quiet time kept after each appointment.
    pub buffer_minutes: i32,
    /// Shortest notice a visitor may book at.
    pub notice_minutes: i32,
    /// How far ahead the public calendar opens.
    pub horizon_days: i32,
    pub location: Option<String>,
    pub hours: Vec<SiteBookingWindow>,
    /// The extra questions; name and email are always asked and are not here.
    pub fields: Vec<SiteBookingField>,
    /// Off means the service exists but takes no bookings.
    pub active: bool,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

/// Complete input for a booking service. Every write replaces the whole shape:
/// hours, questions and the source are one decision, and a partial write could
/// leave a public page offering slots the owner never agreed to.
pub struct SiteBookingInput<'a> {
    pub name: &'a str,
    pub description: Option<&'a str>,
    pub calendar: &'a CalendarId,
    pub time_zone: &'a str,
    pub duration_minutes: i32,
    pub buffer_minutes: i32,
    pub notice_minutes: i32,
    pub horizon_days: i32,
    pub location: Option<&'a str>,
    pub hours: &'a [SiteBookingWindow],
    pub fields: &'a [SiteBookingField],
    pub active: bool,
}

/// The validated, owned form of one write — what actually reaches SQL.
struct BookingWrite {
    name: String,
    description: Option<String>,
    time_zone: String,
    location: Option<String>,
    hours: serde_json::Value,
    fields: serde_json::Value,
}

impl AccountStore {
    /// Creates a booking service on one of the tenant's sites, after resolving
    /// the availability source through the Agenda seam.
    ///
    /// # Errors
    /// [`StoreError::Validation`] on any rule below — a blank name, an unknown
    /// time zone, an out-of-range duration, overlapping or impossible opening
    /// windows, a malformed question, or a calendar that is visible but not
    /// writable; [`StoreError::NotFound`] when the site or the calendar is not
    /// reachable from this account; [`StoreError::Conflict`] when the site is
    /// already at [`SITE_BOOKING_MAX_PER_SITE`]; [`StoreError::Db`] on failure.
    pub async fn create_site_booking(
        &self,
        site: &SiteId,
        input: &SiteBookingInput<'_>,
    ) -> Result<SiteBookingId> {
        let write = self.validate_site_booking(input).await?;
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let existing: Option<i64> = sqlx::query_scalar(
            "SELECT (SELECT count(*) FROM site_bookings b \
                     WHERE b.tenant_id = s.tenant_id AND b.site_id = s.id) \
             FROM sites s WHERE s.tenant_id = $1 AND s.id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        let existing = existing.ok_or(StoreError::NotFound)?;
        if existing >= SITE_BOOKING_MAX_PER_SITE {
            return Err(StoreError::Conflict(format!(
                "a site may offer at most {SITE_BOOKING_MAX_PER_SITE} bookable services"
            )));
        }
        let id = SiteBookingId::generate();
        sqlx::query(
            "INSERT INTO site_bookings \
                (tenant_id, site_id, id, name, description, calendar_id, time_zone, \
                 duration_minutes, buffer_minutes, notice_minutes, horizon_days, \
                 location, hours, fields, active) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(id.as_str())
        .bind(&write.name)
        .bind(&write.description)
        .bind(input.calendar.as_str())
        .bind(&write.time_zone)
        .bind(input.duration_minutes)
        .bind(input.buffer_minutes)
        .bind(input.notice_minutes)
        .bind(input.horizon_days)
        .bind(&write.location)
        .bind(sqlx::types::Json(&write.hours))
        .bind(sqlx::types::Json(&write.fields))
        .bind(input.active)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(id)
    }

    /// A site's booking services in stable creation order. A missing or
    /// foreign site simply has none.
    ///
    /// # Errors
    /// [`StoreError::Conflict`] when a stored row carries an unreadable
    /// pattern or question set; [`StoreError::Db`] on failure.
    pub async fn site_bookings(&self, site: &SiteId) -> Result<Vec<SiteBooking>> {
        let rows = sqlx::query_as::<_, SiteBookingRow>(
            "SELECT id, name, description, calendar_id, time_zone, duration_minutes, \
                    buffer_minutes, notice_minutes, horizon_days, location, hours, fields, \
                    active, created_at, updated_at \
             FROM site_bookings \
             WHERE tenant_id = $1 AND site_id = $2 ORDER BY created_at, id",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter().map(SiteBookingRow::into_booking).collect()
    }

    /// One booking service of one of the tenant's sites.
    ///
    /// # Errors
    /// [`StoreError::Conflict`] when the stored row is unreadable;
    /// [`StoreError::Db`] on failure.
    pub async fn site_booking(
        &self,
        site: &SiteId,
        booking: &SiteBookingId,
    ) -> Result<Option<SiteBooking>> {
        let row = sqlx::query_as::<_, SiteBookingRow>(
            "SELECT id, name, description, calendar_id, time_zone, duration_minutes, \
                    buffer_minutes, notice_minutes, horizon_days, location, hours, fields, \
                    active, created_at, updated_at \
             FROM site_bookings \
             WHERE tenant_id = $1 AND site_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(booking.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        row.map(SiteBookingRow::into_booking).transpose()
    }

    /// Replaces a booking service whole, revalidating the complete new shape
    /// including its availability source.
    ///
    /// # Errors
    /// As [`Self::create_site_booking`], except that
    /// [`StoreError::NotFound`] here means the service is not this tenant's on
    /// this site.
    pub async fn update_site_booking(
        &self,
        site: &SiteId,
        booking: &SiteBookingId,
        input: &SiteBookingInput<'_>,
    ) -> Result<()> {
        let write = self.validate_site_booking(input).await?;
        let done = sqlx::query(
            "UPDATE site_bookings SET name = $4, description = $5, calendar_id = $6, \
                    time_zone = $7, duration_minutes = $8, buffer_minutes = $9, \
                    notice_minutes = $10, horizon_days = $11, location = $12, hours = $13, \
                    fields = $14, active = $15, updated_at = now() \
             WHERE tenant_id = $1 AND site_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(booking.as_str())
        .bind(&write.name)
        .bind(&write.description)
        .bind(input.calendar.as_str())
        .bind(&write.time_zone)
        .bind(input.duration_minutes)
        .bind(input.buffer_minutes)
        .bind(input.notice_minutes)
        .bind(input.horizon_days)
        .bind(&write.location)
        .bind(sqlx::types::Json(&write.hours))
        .bind(sqlx::types::Json(&write.fields))
        .bind(input.active)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Removes a booking service. The calendar it pointed at is untouched —
    /// Sites never deletes anything Agenda owns.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when it is not this tenant's on this site;
    /// [`StoreError::Db`] on failure.
    pub async fn delete_site_booking(&self, site: &SiteId, booking: &SiteBookingId) -> Result<()> {
        let done = sqlx::query(
            "DELETE FROM site_bookings WHERE tenant_id = $1 AND site_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(booking.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// The whole write gate: content rules first (they cost nothing), then the
    /// one query that resolves the availability source.
    async fn validate_site_booking(&self, input: &SiteBookingInput<'_>) -> Result<BookingWrite> {
        let write = validate_booking_shape(input)?;
        let source = self
            .site_availability_source(input.calendar)
            .await?
            .ok_or(StoreError::NotFound)?;
        if !source.writable {
            return Err(StoreError::Validation(format!(
                "{} is shared with you for reading only; a booking service needs a calendar \
                 you can add appointments to",
                source.name
            )));
        }
        Ok(write)
    }
}

/// Everything that can be decided without touching the database.
fn validate_booking_shape(input: &SiteBookingInput<'_>) -> Result<BookingWrite> {
    let name = required_text(input.name, "booking name", SITE_BOOKING_NAME_MAX_CHARS)?;
    let description = optional_text(
        input.description,
        "booking description",
        SITE_BOOKING_DESCRIPTION_MAX_CHARS,
    )?;
    let location = optional_text(
        input.location,
        "booking location",
        SITE_BOOKING_LOCATION_MAX_CHARS,
    )?;
    let time_zone = validate_time_zone(input.time_zone)?;
    validate_minutes(
        input.duration_minutes,
        "appointment length",
        SITE_BOOKING_MIN_DURATION_MINUTES,
        SITE_BOOKING_MAX_DURATION_MINUTES,
    )?;
    validate_minutes(
        input.buffer_minutes,
        "the gap kept after an appointment",
        0,
        SITE_BOOKING_MAX_BUFFER_MINUTES,
    )?;
    validate_minutes(
        input.notice_minutes,
        "the notice asked for",
        0,
        SITE_BOOKING_MAX_NOTICE_MINUTES,
    )?;
    validate_minutes(
        input.horizon_days,
        "how far ahead bookings open",
        1,
        SITE_BOOKING_MAX_HORIZON_DAYS,
    )?;
    let hours = validate_hours(input.hours, input.duration_minutes)?;
    let fields = validate_fields(input.fields)?;
    Ok(BookingWrite {
        name,
        description,
        time_zone,
        location,
        hours: serde_json::to_value(hours)
            .map_err(|error| StoreError::Conflict(format!("invalid opening hours: {error}")))?,
        fields: serde_json::to_value(fields)
            .map_err(|error| StoreError::Conflict(format!("invalid booking questions: {error}")))?,
    })
}

fn required_text(value: &str, what: &str, max: usize) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(StoreError::Validation(format!("{what} must not be empty")));
    }
    if value.chars().count() > max {
        return Err(StoreError::Validation(format!(
            "{what} must be at most {max} characters"
        )));
    }
    Ok(value.to_owned())
}

fn optional_text(value: Option<&str>, what: &str, max: usize) -> Result<Option<String>> {
    match value.map(str::trim).filter(|value| !value.is_empty()) {
        None => Ok(None),
        Some(value) => Ok(Some(required_text(value, what, max)?)),
    }
}

/// A time zone the server can actually convert with — the same IANA database
/// the calendar path reads TZID-qualified times through. A name we cannot
/// resolve would publish a week of slots at the wrong hour.
fn validate_time_zone(value: &str) -> Result<String> {
    let value = required_text(value, "time zone", SITE_BOOKING_TIME_ZONE_MAX_CHARS)?;
    if jiff::tz::TimeZone::get(&value).is_err() {
        return Err(StoreError::Validation(format!(
            "{value} is not a known time zone; use an IANA name such as Europe/Brussels"
        )));
    }
    Ok(value)
}

fn validate_minutes(value: i32, what: &str, min: i32, max: i32) -> Result<()> {
    if value < min || value > max {
        return Err(StoreError::Validation(format!(
            "{what} must be between {min} and {max}"
        )));
    }
    Ok(())
}

/// The weekly pattern: at least one window, each a real span of a real day,
/// long enough to hold the appointment it offers, and no two on the same day
/// overlapping. Returned sorted, so two owners who typed the same week in a
/// different order store the same row.
fn validate_hours(hours: &[SiteBookingWindow], duration: i32) -> Result<Vec<SiteBookingWindow>> {
    if hours.is_empty() {
        return Err(StoreError::Validation(
            "opening hours must have at least one window".to_owned(),
        ));
    }
    if hours.len() > SITE_BOOKING_MAX_WINDOWS {
        return Err(StoreError::Validation(format!(
            "opening hours must have at most {SITE_BOOKING_MAX_WINDOWS} windows"
        )));
    }
    for window in hours {
        if !(1..=7).contains(&window.weekday) {
            return Err(StoreError::Validation(format!(
                "{} is not a weekday; use 1 for Monday through 7 for Sunday",
                window.weekday
            )));
        }
        if window.start_minute < 0 || window.end_minute > MINUTES_IN_DAY {
            return Err(StoreError::Validation(format!(
                "an opening window must lie within the day (0 to {MINUTES_IN_DAY} minutes)"
            )));
        }
        if window.end_minute <= window.start_minute {
            return Err(StoreError::Validation(
                "an opening window must end after it starts".to_owned(),
            ));
        }
        if window.end_minute - window.start_minute < duration {
            return Err(StoreError::Validation(format!(
                "an opening window of {} minutes is shorter than the {duration}-minute \
                 appointment it offers",
                window.end_minute - window.start_minute
            )));
        }
    }
    let mut sorted = hours.to_vec();
    sorted.sort_by_key(|window| (window.weekday, window.start_minute, window.end_minute));
    for pair in sorted.windows(2) {
        let (earlier, later) = (pair[0], pair[1]);
        if earlier.weekday == later.weekday && later.start_minute < earlier.end_minute {
            return Err(StoreError::Validation(
                "two opening windows on the same day must not overlap".to_owned(),
            ));
        }
    }
    Ok(sorted)
}

/// The extra questions: bounded in number, each with a stable machine key, a
/// visible label, and options exactly when it is a choice.
fn validate_fields(fields: &[SiteBookingField]) -> Result<Vec<SiteBookingField>> {
    if fields.len() > SITE_BOOKING_MAX_FIELDS {
        return Err(StoreError::Validation(format!(
            "a booking may ask at most {SITE_BOOKING_MAX_FIELDS} extra questions"
        )));
    }
    let mut validated: Vec<SiteBookingField> = Vec::with_capacity(fields.len());
    for field in fields {
        let key = validate_field_key(&field.key)?;
        if validated.iter().any(|seen| seen.key == key) {
            return Err(StoreError::Validation(format!(
                "two questions share the key {key}; each answer needs its own"
            )));
        }
        let label = required_text(
            &field.label,
            "question label",
            SITE_BOOKING_FIELD_LABEL_MAX_CHARS,
        )?;
        let options = validate_field_options(field)?;
        validated.push(SiteBookingField {
            key,
            label,
            kind: field.kind,
            required: field.required,
            options,
        });
    }
    Ok(validated)
}

/// A key an answer is stored under, so it must survive a rename: lowercase
/// letters, digits and underscores, starting with a letter.
fn validate_field_key(key: &str) -> Result<String> {
    let key = required_text(key, "question key", SITE_BOOKING_FIELD_KEY_MAX_CHARS)?;
    let starts_with_letter = key.chars().next().is_some_and(|c| c.is_ascii_lowercase());
    let allowed = key
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if !starts_with_letter || !allowed {
        return Err(StoreError::Validation(format!(
            "question key {key} must start with a lowercase letter and use only lowercase \
             letters, digits, and underscores"
        )));
    }
    Ok(key)
}

fn validate_field_options(field: &SiteBookingField) -> Result<Vec<String>> {
    if field.kind != SiteBookingFieldKind::Choice {
        if field.options.is_empty() {
            return Ok(Vec::new());
        }
        return Err(StoreError::Validation(format!(
            "question {} offers answers but is not a choice question",
            field.key.trim()
        )));
    }
    if field.options.len() < 2 {
        return Err(StoreError::Validation(format!(
            "choice question {} must offer at least two answers",
            field.key.trim()
        )));
    }
    if field.options.len() > SITE_BOOKING_FIELD_MAX_OPTIONS {
        return Err(StoreError::Validation(format!(
            "a choice question may offer at most {SITE_BOOKING_FIELD_MAX_OPTIONS} answers"
        )));
    }
    let mut options: Vec<String> = Vec::with_capacity(field.options.len());
    for option in &field.options {
        let option = required_text(option, "choice answer", SITE_BOOKING_FIELD_OPTION_MAX_CHARS)?;
        if options.contains(&option) {
            return Err(StoreError::Validation(format!(
                "choice question {} offers {option} twice",
                field.key.trim()
            )));
        }
        options.push(option);
    }
    Ok(options)
}

#[derive(sqlx::FromRow)]
struct SiteBookingRow {
    id: String,
    name: String,
    description: Option<String>,
    calendar_id: String,
    time_zone: String,
    duration_minutes: i32,
    buffer_minutes: i32,
    notice_minutes: i32,
    horizon_days: i32,
    location: Option<String>,
    hours: sqlx::types::Json<serde_json::Value>,
    fields: sqlx::types::Json<serde_json::Value>,
    active: bool,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl SiteBookingRow {
    fn into_booking(self) -> Result<SiteBooking> {
        let hours: Vec<SiteBookingWindow> = serde_json::from_value(self.hours.0).map_err(|_| {
            StoreError::Conflict("booking service has unreadable opening hours".to_owned())
        })?;
        let fields: Vec<SiteBookingField> =
            serde_json::from_value(self.fields.0).map_err(|_| {
                StoreError::Conflict("booking service has unreadable questions".to_owned())
            })?;
        Ok(SiteBooking {
            id: SiteBookingId::new(self.id),
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
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn window(weekday: i32, start_minute: i32, end_minute: i32) -> SiteBookingWindow {
        SiteBookingWindow {
            weekday,
            start_minute,
            end_minute,
        }
    }

    fn field(key: &str, kind: SiteBookingFieldKind, options: &[&str]) -> SiteBookingField {
        SiteBookingField {
            key: key.to_owned(),
            label: "Question".to_owned(),
            kind,
            required: false,
            options: options.iter().map(|option| (*option).to_owned()).collect(),
        }
    }

    #[test]
    fn a_week_is_stored_sorted_whatever_order_it_was_typed_in() {
        let typed = [
            window(3, 540, 720),
            window(1, 780, 1_020),
            window(1, 540, 720),
        ];
        let stored = validate_hours(&typed, 30).unwrap();
        assert_eq!(
            stored,
            vec![
                window(1, 540, 720),
                window(1, 780, 1_020),
                window(3, 540, 720)
            ]
        );
    }

    #[test]
    fn overlapping_windows_on_the_same_day_are_refused_but_the_same_hours_on_another_day_are_not() {
        let clash = validate_hours(&[window(2, 540, 720), window(2, 660, 780)], 30).unwrap_err();
        assert!(format!("{clash}").contains("overlap"), "{clash}");
        validate_hours(&[window(2, 540, 720), window(3, 540, 720)], 30).unwrap();
    }

    #[test]
    fn a_window_shorter_than_the_appointment_it_offers_is_refused_by_name() {
        let refused = validate_hours(&[window(1, 540, 560)], 30).unwrap_err();
        let said = format!("{refused}");
        assert!(said.contains("20 minutes"), "{said}");
        assert!(said.contains("30-minute"), "{said}");
    }

    #[test]
    fn a_day_outside_the_week_and_a_span_outside_the_day_are_refused() {
        assert!(validate_hours(&[window(0, 540, 720)], 30).is_err());
        assert!(validate_hours(&[window(8, 540, 720)], 30).is_err());
        assert!(validate_hours(&[window(1, 540, 1_500)], 30).is_err());
        assert!(validate_hours(&[window(1, 720, 540)], 30).is_err());
        assert!(validate_hours(&[], 30).is_err());
    }

    #[test]
    fn choice_questions_need_two_distinct_answers_and_other_kinds_need_none() {
        validate_fields(&[field("cut", SiteBookingFieldKind::Choice, &["Dry", "Wet"])]).unwrap();
        assert!(validate_fields(&[field("cut", SiteBookingFieldKind::Choice, &["Dry"])]).is_err());
        assert!(
            validate_fields(&[field("cut", SiteBookingFieldKind::Choice, &["Dry", "Dry"])])
                .is_err()
        );
        assert!(validate_fields(&[field("note", SiteBookingFieldKind::Text, &["Dry"])]).is_err());
        validate_fields(&[field("note", SiteBookingFieldKind::Text, &[])]).unwrap();
    }

    #[test]
    fn question_keys_are_machine_stable_and_unique() {
        assert!(validate_fields(&[field("Phone", SiteBookingFieldKind::Text, &[])]).is_err());
        assert!(validate_fields(&[field("1phone", SiteBookingFieldKind::Text, &[])]).is_err());
        assert!(validate_fields(&[field("phone-2", SiteBookingFieldKind::Text, &[])]).is_err());
        validate_fields(&[field("phone_2", SiteBookingFieldKind::Text, &[])]).unwrap();
        let duplicated = validate_fields(&[
            field("phone", SiteBookingFieldKind::Text, &[]),
            field("phone", SiteBookingFieldKind::Phone, &[]),
        ])
        .unwrap_err();
        assert!(format!("{duplicated}").contains("phone"));
    }

    #[test]
    fn a_time_zone_must_be_one_the_server_can_convert_with() {
        assert_eq!(
            validate_time_zone("Europe/Brussels").unwrap(),
            "Europe/Brussels"
        );
        let refused = validate_time_zone("Middle/Earth").unwrap_err();
        assert!(
            format!("{refused}").contains("Europe/Brussels"),
            "{refused}"
        );
    }

    #[test]
    fn the_four_question_kinds_round_trip_and_a_fifth_names_them_all() {
        for kind in [
            SiteBookingFieldKind::Text,
            SiteBookingFieldKind::LongText,
            SiteBookingFieldKind::Phone,
            SiteBookingFieldKind::Choice,
        ] {
            assert_eq!(SiteBookingFieldKind::parse(kind.as_str()).unwrap(), kind);
        }
        let refused = SiteBookingFieldKind::parse("dropdown").unwrap_err();
        assert!(format!("{refused}").contains("long_text"), "{refused}");
    }
}
