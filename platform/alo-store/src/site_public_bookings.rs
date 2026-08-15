//! The public **booking flow** of alo Sites: what a visitor sees as free time
//! on a published page, and the appointment they take from it.
//!
//! Like the order door ([`crate::site_public_orders`]), this module holds only
//! the bare service id a rendered page carries, and resolves it to the
//! **currently published snapshot** of that booking service. The tenant is
//! never named from outside; it comes out of the resolving read and is what
//! every following statement scopes itself to. An unknown id, a service on a
//! draft site, one whose site has been unpublished, one switched off, and one
//! whose calendar has since been deleted are all the same `Ok(None)`: the
//! public wire turns that into one uniform 404 with no existence leak.
//!
//! **Two visitors, one slot.** That race is the reason this module exists, and
//! it is settled by the database rather than by timing: a live appointment is
//! unique on `(tenant, booking, starts_at)`, the reservation takes a
//! transaction-scoped advisory lock on the calendar before it looks, and the
//! second writer is told the time has just been taken. The Agenda event written
//! afterwards is the owner's *view* of the reservation; the row is the
//! reservation. That ordering is what lets availability stay correct in the
//! instant between the two — free time subtracts booked appointments as well as
//! calendar events.
//!
//! Privacy: an appointment stores the visitor's name, an address to confirm to,
//! and the answers the owner asked for. Nothing about the connection is stored
//! or logged, and nothing is ever read back out of the calendar beyond the
//! start and end of a busy span.

use serde::{Deserialize, Serialize};
use time::{Date, Duration, OffsetDateTime};

use crate::error::{Result, StoreError};
use crate::id::{CalendarId, SiteBookingAppointmentId, SiteId, TenantId, UserId};
use crate::model::CalendarEvent;
use crate::site_booking_publish::{SiteBookingSnapshot, SiteBookingSnapshotRow};
use crate::site_booking_slots::{BookingRules, BookingSlot, free_slots, local_day};
use crate::site_bookings::{SiteBookingField, SiteBookingFieldKind};
use crate::site_public::{PublishedSite, SitePublicStore};

/// The longest id token this door will even send to the database. Real ids are
/// 22 characters (base64url of 16 random bytes); anything far outside that
/// shape is noise, not a lookup.
const BOOKING_ID_MAX_LEN: usize = 64;
/// Longest visitor name accepted.
pub const BOOKING_VISITOR_NAME_MAX_CHARS: usize = 200;
/// Longest visitor address accepted (the RFC 5321 path limit).
pub const BOOKING_VISITOR_EMAIL_MAX_CHARS: usize = 254;
/// Longest answer to one of the service's own questions.
pub const BOOKING_ANSWER_MAX_CHARS: usize = 2_000;

/// One published booking service, resolved for an anonymous visitor. The
/// tenant, the site and the calendar's owner are private: a caller holding this
/// value can ask for free times and take one, and can name no other tenant's
/// anything.
#[derive(Debug, Clone)]
pub struct PublicBookingService {
    tenant: TenantId,
    site: SiteId,
    calendar: CalendarId,
    /// The account the calendar belongs to — whose Agenda door the appointment
    /// is written through.
    owner: UserId,
    /// What was frozen into the publish the visitor is looking at.
    pub published: SiteBookingSnapshot,
}

impl PublicBookingService {
    /// The rules that decide which times exist, straight from the snapshot.
    #[must_use]
    pub fn rules(&self) -> BookingRules<'_> {
        BookingRules {
            hours: &self.published.hours,
            time_zone: &self.published.time_zone,
            duration_minutes: self.published.duration_minutes,
            buffer_minutes: self.published.buffer_minutes,
            notice_minutes: self.published.notice_minutes,
            horizon_days: self.published.horizon_days,
        }
    }
}

/// One answer as it is stored: the machine key it was asked under, the label
/// the visitor actually read, and what they typed. The label travels with the
/// answer so a question renamed next month does not rewrite an appointment
/// already taken.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookingAnswer {
    pub key: String,
    pub label: String,
    pub value: String,
}

/// What a visitor posted: the slot they picked, who they are, and their answers
/// as `(question key, answer)` pairs in whatever order the form produced.
#[derive(Debug, Clone)]
pub struct BookingRequest<'a> {
    pub starts_at: OffsetDateTime,
    pub visitor_name: &'a str,
    pub visitor_email: &'a str,
    pub answers: &'a [(String, String)],
}

/// A reservation that stuck — what the confirmation page tells the visitor.
#[derive(Debug, Clone)]
pub struct ReservedAppointment {
    pub id: SiteBookingAppointmentId,
    pub booking_name: String,
    pub starts_at: OffsetDateTime,
    pub ends_at: OffsetDateTime,
    /// The zone the published week was written in — the clock the visitor was
    /// offered the time in.
    pub time_zone: String,
    pub location: Option<String>,
    /// The capability that lets the visitor (and only whoever holds the
    /// confirmation) see, calendar-import, and cancel this one appointment —
    /// the reversibility handle of [`crate::site_booking_manage`].
    pub manage_token: String,
}

impl SitePublicStore {
    /// Resolves a booking service id to the service as the **currently
    /// published** page offers it, or `None` when it is not offered at all
    /// (see the module doc: every reason is one answer).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure; [`StoreError::Conflict`] when the stored
    /// snapshot cannot be read back.
    pub async fn public_booking(&self, booking_id: &str) -> Result<Option<PublicBookingService>> {
        if booking_id.is_empty()
            || booking_id.len() > BOOKING_ID_MAX_LEN
            || !booking_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Ok(None);
        }
        // The site's *current* publish is the one the visitor is looking at,
        // and the only one that may be booked from: a superseded publish is a
        // page nobody is being served any more. The calendar's owner is joined
        // in the same read, so a calendar that has been deleted since the
        // publish resolves to nothing bookable rather than to an empty week.
        let row: Option<ResolvedBookingRow> = sqlx::query_as(
            "SELECT sn.tenant_id, sn.site_id, c.owner_user_id AS owner, sn.booking_id, sn.name, \
                    sn.description, sn.calendar_id, sn.time_zone, sn.duration_minutes, \
                    sn.buffer_minutes, sn.notice_minutes, sn.horizon_days, sn.location, \
                    sn.hours, sn.fields, sn.active \
             FROM site_booking_snapshots sn \
             JOIN site_publishes p ON p.tenant_id = sn.tenant_id AND p.id = sn.publish_id \
             JOIN sites s ON s.tenant_id = p.tenant_id AND s.id = p.site_id \
                         AND s.published_publish_id = sn.publish_id \
             JOIN calendars c ON c.tenant_id = sn.tenant_id AND c.id = sn.calendar_id \
             WHERE sn.booking_id = $1 AND sn.active \
             LIMIT 1",
        )
        .bind(booking_id)
        .fetch_optional(self.pool())
        .await
        .map_err(StoreError::Db)?;
        let Some(row) = row else { return Ok(None) };
        let tenant = TenantId::new(row.tenant_id);
        let site = SiteId::new(row.site_id);
        let owner = UserId::new(row.owner);
        let published = row.snapshot.into_snapshot()?;
        Ok(Some(PublicBookingService {
            tenant,
            site,
            calendar: published.calendar.clone(),
            owner,
            published,
        }))
    }

    /// Everything one published site currently offers appointments for — the
    /// site-level entry ADR 0040's conversation needs: the bot (and anything
    /// else public) can ask *what can be booked here* without a page-carried
    /// service id, and gets back the same resolved services the booking
    /// section itself books through.
    ///
    /// Only the **active** services frozen into the publish `site` is serving
    /// are offered, in name order. A service whose calendar has since been
    /// deleted drops out — nothing bookable rather than an empty week — and a
    /// site with none, or one whose publish has been superseded, is an empty
    /// list. Tenancy is carried by `site` itself: a [`PublishedSite`] only
    /// ever comes out of this store's Host resolvers, so another tenant's
    /// services are unrepresentable here.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure; [`StoreError::Conflict`] when a stored
    /// snapshot cannot be read back.
    pub async fn published_availability(
        &self,
        site: &PublishedSite,
    ) -> Result<Vec<PublicBookingService>> {
        let rows: Vec<ResolvedBookingRow> = sqlx::query_as(
            "SELECT sn.tenant_id, sn.site_id, c.owner_user_id AS owner, sn.booking_id, sn.name, \
                    sn.description, sn.calendar_id, sn.time_zone, sn.duration_minutes, \
                    sn.buffer_minutes, sn.notice_minutes, sn.horizon_days, sn.location, \
                    sn.hours, sn.fields, sn.active \
             FROM site_booking_snapshots sn \
             JOIN sites s ON s.tenant_id = sn.tenant_id AND s.id = sn.site_id \
                         AND s.published_publish_id = sn.publish_id \
             JOIN calendars c ON c.tenant_id = sn.tenant_id AND c.id = sn.calendar_id \
             WHERE sn.tenant_id = $1 AND sn.site_id = $2 AND sn.publish_id = $3 AND sn.active \
             ORDER BY sn.name, sn.booking_id",
        )
        .bind(site.tenant.as_str())
        .bind(site.site.as_str())
        .bind(site.publish.as_str())
        .fetch_all(self.pool())
        .await
        .map_err(StoreError::Db)?;
        let mut services = Vec::with_capacity(rows.len());
        for row in rows {
            let tenant = TenantId::new(row.tenant_id);
            let site_id = SiteId::new(row.site_id);
            let owner = UserId::new(row.owner);
            let published = row.snapshot.into_snapshot()?;
            services.push(PublicBookingService {
                tenant,
                site: site_id,
                calendar: published.calendar.clone(),
                owner,
                published,
            });
        }
        Ok(services)
    }

    /// The times a visitor may still pick on one local day of `service`.
    ///
    /// Empty is a complete answer: a closed day, a full day, a day before the
    /// notice or past the horizon, and a service switched off all offer
    /// nothing. `now` is passed in so the caller — and its tests — decide what
    /// *soon* means.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn public_booking_slots(
        &self,
        service: &PublicBookingService,
        day: Date,
        now: OffsetDateTime,
    ) -> Result<Vec<BookingSlot>> {
        if !service.published.active {
            return Ok(Vec::new());
        }
        let rules = service.rules();
        // What the published week would offer if the owner had nothing on. No
        // candidates means no reason to read the calendar at all.
        let candidates = free_slots(&rules, day, &[], now);
        let (Some(first), Some(last)) = (candidates.first(), candidates.last()) else {
            return Ok(Vec::new());
        };
        let reach = Duration::minutes(i64::from(service.published.buffer_minutes.max(0)));
        let from = first.starts_at.saturating_sub(reach);
        let to = last.ends_at.saturating_add(reach);
        let busy = self.busy_in(service, from, to).await?;
        Ok(free_slots(&rules, day, &busy, now))
    }

    /// Everything the owner is not free for in `[from, to)`: their calendar's
    /// busy spans, asked of Agenda's own availability seam
    /// ([`crate::calendar_availability`], which can answer nothing but spans —
    /// never a title, a guest or a note), plus every live appointment already
    /// taken on that calendar — including ones whose Agenda event has not been
    /// written yet, which is what closes the gap between committing a
    /// reservation and showing it in the calendar.
    async fn busy_in(
        &self,
        service: &PublicBookingService,
        from: OffsetDateTime,
        to: OffsetDateTime,
    ) -> Result<Vec<crate::site_booking_slots::BusyInterval>> {
        let availability = crate::calendar_availability::CalendarAvailability::open(
            self.pool().clone(),
            self.blobs().clone(),
            service.tenant.clone(),
            service.owner.clone(),
        );
        let mut busy: Vec<crate::site_booking_slots::BusyInterval> = availability
            .busy_spans(&service.calendar, from, to)
            .await?
            .into_iter()
            .map(|span| crate::site_booking_slots::BusyInterval {
                from: span.from,
                to: span.to,
            })
            .collect();
        let reserved: Vec<(OffsetDateTime, OffsetDateTime)> = sqlx::query_as(
            "SELECT starts_at, ends_at FROM site_booking_appointments \
             WHERE tenant_id = $1 AND calendar_id = $2 AND status = 'booked' \
               AND starts_at < $3 AND ends_at > $4",
        )
        .bind(service.tenant.as_str())
        .bind(service.calendar.as_str())
        .bind(to)
        .bind(from)
        .fetch_all(self.pool())
        .await
        .map_err(StoreError::Db)?;
        busy.extend(
            reserved
                .into_iter()
                .map(|(from, to)| crate::site_booking_slots::BusyInterval { from, to }),
        );
        Ok(busy)
    }

    /// Takes one appointment on a published service.
    ///
    /// Returns `Ok(None)` when the service is not bookable at all (the same
    /// uniform absence as [`Self::public_booking`], re-checked here because the
    /// page in front of the visitor may be minutes old).
    ///
    /// # Errors
    /// [`StoreError::Validation`] naming what the visitor must fix — a missing
    /// name, an address that is not one, an unanswered required question, an
    /// answer that is not one of the offered ones;
    /// [`StoreError::Conflict`] when the time is no longer free, including the
    /// exact race of two visitors taking one slot; [`StoreError::Db`] on
    /// failure.
    pub async fn reserve_public_booking(
        &self,
        service: &PublicBookingService,
        request: &BookingRequest<'_>,
        now: OffsetDateTime,
    ) -> Result<Option<ReservedAppointment>> {
        if !service.published.active {
            return Ok(None);
        }
        let visitor_name =
            required_visitor_text(request.visitor_name, "name", BOOKING_VISITOR_NAME_MAX_CHARS)?;
        let visitor_email = normalize_visitor_email(request.visitor_email)?;
        let answers = normalize_answers(&service.published.fields, request.answers)?;
        let answers_json = serde_json::to_value(&answers).map_err(|error| {
            StoreError::Conflict(format!("answers could not be stored: {error}"))
        })?;

        // The slot must be one this service actually offers *now* — the page
        // the visitor is looking at may be minutes old, and a posted instant is
        // otherwise an arbitrary time.
        let day = local_day(request.starts_at, &service.published.time_zone).ok_or_else(|| {
            StoreError::Validation("that time is not one this service offers".to_owned())
        })?;
        let offered = self.public_booking_slots(service, day, now).await?;
        let Some(slot) = offered
            .into_iter()
            .find(|slot| slot.starts_at == request.starts_at)
        else {
            return Err(StoreError::Conflict(
                "that time is no longer free; please pick another".to_owned(),
            ));
        };

        let id = SiteBookingAppointmentId::generate();
        let manage_token = crate::id::generate_token();
        let mut tx = self.pool().begin().await.map_err(StoreError::Db)?;
        // One writer at a time per calendar, for the length of this
        // transaction: the overlap check below and the insert that follows it
        // are one decision, and two visitors must not interleave them. The
        // unique index on (tenant, booking, start) is the second line, and
        // holds even if this lock were never taken.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1), hashtext($2))")
            .bind(service.tenant.as_str())
            .bind(service.calendar.as_str())
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        let clash: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM site_booking_appointments \
             WHERE tenant_id = $1 AND calendar_id = $2 AND status = 'booked' \
               AND starts_at < $3 AND ends_at > $4)",
        )
        .bind(service.tenant.as_str())
        .bind(service.calendar.as_str())
        .bind(slot.ends_at)
        .bind(slot.starts_at)
        .fetch_one(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        if clash {
            return Err(StoreError::Conflict(
                "that time has just been taken; please pick another".to_owned(),
            ));
        }
        let written = sqlx::query(
            "INSERT INTO site_booking_appointments \
                 (tenant_id, site_id, id, booking_id, booking_name, calendar_id, starts_at, \
                  ends_at, time_zone, visitor_name, visitor_email, answers, manage_token) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13) \
             ON CONFLICT DO NOTHING",
        )
        .bind(service.tenant.as_str())
        .bind(service.site.as_str())
        .bind(id.as_str())
        .bind(service.published.booking_id.as_str())
        .bind(&service.published.name)
        .bind(service.calendar.as_str())
        .bind(slot.starts_at)
        .bind(slot.ends_at)
        .bind(&service.published.time_zone)
        .bind(&visitor_name)
        .bind(&visitor_email)
        .bind(sqlx::types::Json(&answers_json))
        .bind(&manage_token)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        if written.rows_affected() == 0 {
            // The unique index refused it: another visitor holds this slot.
            return Err(StoreError::Conflict(
                "that time has just been taken; please pick another".to_owned(),
            ));
        }
        tx.commit().await.map_err(StoreError::Db)?;

        // The reservation exists; the calendar event is the owner's view of it.
        // Written after the commit and through the owner's own Agenda door, so
        // it goes through Agenda's rules (and its change log) rather than
        // around them. If it cannot be written the reservation is withdrawn: an
        // appointment the owner will never see in their calendar is worse than
        // a visitor asked to try again.
        let event = CalendarEvent {
            id: crate::id::EventId::generate(),
            calendar_id: service.calendar.clone(),
            summary: format!("{} — {visitor_name}", service.published.name),
            description: Some(describe(&visitor_email, &answers)),
            location: service.published.location.clone(),
            starts_at: slot.starts_at,
            ends_at: slot.ends_at,
            all_day: false,
            recurrence: None,
            attendees: Vec::new(),
            exdates: Vec::new(),
            recurrence_id: None,
            reminder_minutes: None,
            attendee_status: Vec::new(),
        };
        let door = crate::site_agenda::agenda_door(
            self.pool().clone(),
            self.blobs().clone(),
            service.tenant.clone(),
            service.owner.clone(),
        );
        match door.create_event(&event).await {
            Ok(event_id) => {
                sqlx::query(
                    "UPDATE site_booking_appointments SET event_id = $3 \
                     WHERE tenant_id = $1 AND id = $2",
                )
                .bind(service.tenant.as_str())
                .bind(id.as_str())
                .bind(event_id.as_str())
                .execute(self.pool())
                .await
                .map_err(StoreError::Db)?;
            }
            Err(error) => {
                sqlx::query(
                    "DELETE FROM site_booking_appointments WHERE tenant_id = $1 AND id = $2",
                )
                .bind(service.tenant.as_str())
                .bind(id.as_str())
                .execute(self.pool())
                .await
                .map_err(StoreError::Db)?;
                return Err(error);
            }
        }

        Ok(Some(ReservedAppointment {
            id,
            booking_name: service.published.name.clone(),
            starts_at: slot.starts_at,
            ends_at: slot.ends_at,
            time_zone: service.published.time_zone.clone(),
            location: service.published.location.clone(),
            manage_token,
        }))
    }
}

/// What the owner reads in their calendar: how to answer the visitor, and the
/// answers to their own questions. Deliberately plain text — this is an event
/// description, seen in whatever client the owner uses.
fn describe(visitor_email: &str, answers: &[BookingAnswer]) -> String {
    let mut out = format!("Booked from your website.\nEmail: {visitor_email}");
    for answer in answers {
        out.push_str(&format!("\n{}: {}", answer.label, answer.value));
    }
    out
}

fn required_visitor_text(value: &str, what: &str, max: usize) -> Result<String> {
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

/// An address the confirmation can actually be sent to. The same shape gate the
/// order door applies: enough to catch a typo, never a claim that the mailbox
/// exists.
fn normalize_visitor_email(value: &str) -> Result<String> {
    let value = required_visitor_text(value, "email", BOOKING_VISITOR_EMAIL_MAX_CHARS)?;
    let looks_like_address = matches!(
        value.split_once('@'),
        Some((local, domain)) if !local.is_empty() && !domain.is_empty()
    );
    if !looks_like_address || value.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(StoreError::Validation(
            "email must be a valid address".to_owned(),
        ));
    }
    Ok(value)
}

/// Checks the posted answers against the questions the publish actually asked,
/// in the published order. An answer to a question this service does not ask is
/// dropped rather than refused — a page from an older publish may carry one,
/// and losing a whole booking over it would be the wrong answer to a stale
/// page.
fn normalize_answers(
    fields: &[SiteBookingField],
    posted: &[(String, String)],
) -> Result<Vec<BookingAnswer>> {
    let mut answers = Vec::with_capacity(fields.len());
    for field in fields {
        let value = posted
            .iter()
            .find(|(key, _)| key == &field.key)
            .map(|(_, value)| value.trim())
            .unwrap_or_default();
        if value.is_empty() {
            if field.required {
                return Err(StoreError::Validation(format!(
                    "{} must not be empty",
                    field.label
                )));
            }
            continue;
        }
        if value.chars().count() > BOOKING_ANSWER_MAX_CHARS {
            return Err(StoreError::Validation(format!(
                "{} must be at most {BOOKING_ANSWER_MAX_CHARS} characters",
                field.label
            )));
        }
        if field.kind == SiteBookingFieldKind::Choice
            && !field.options.iter().any(|option| option == value)
        {
            return Err(StoreError::Validation(format!(
                "{} must be one of the offered answers",
                field.label
            )));
        }
        answers.push(BookingAnswer {
            key: field.key.clone(),
            label: field.label.clone(),
            value: value.to_owned(),
        });
    }
    Ok(answers)
}

#[derive(sqlx::FromRow)]
struct ResolvedBookingRow {
    tenant_id: String,
    site_id: String,
    owner: String,
    #[sqlx(flatten)]
    snapshot: SiteBookingSnapshotRow,
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn field(
        key: &str,
        kind: SiteBookingFieldKind,
        required: bool,
        options: &[&str],
    ) -> SiteBookingField {
        SiteBookingField {
            key: key.to_owned(),
            label: format!("The {key}"),
            kind,
            required,
            options: options.iter().map(|option| (*option).to_owned()).collect(),
        }
    }

    fn posted(entries: &[(&str, &str)]) -> Vec<(String, String)> {
        entries
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect()
    }

    #[test]
    fn answers_are_stored_in_the_published_order_with_the_label_that_was_read() {
        let fields = [
            field("phone", SiteBookingFieldKind::Phone, true, &[]),
            field("cut", SiteBookingFieldKind::Choice, false, &["Dry", "Wet"]),
        ];
        let answers = normalize_answers(
            &fields,
            &posted(&[("cut", " Wet "), ("phone", " +32 2 555 01 "), ("gone", "x")]),
        )
        .unwrap();
        assert_eq!(
            answers,
            vec![
                BookingAnswer {
                    key: "phone".to_owned(),
                    label: "The phone".to_owned(),
                    value: "+32 2 555 01".to_owned(),
                },
                BookingAnswer {
                    key: "cut".to_owned(),
                    label: "The cut".to_owned(),
                    value: "Wet".to_owned(),
                },
            ]
        );
    }

    #[test]
    fn a_required_question_must_be_answered_and_a_choice_must_be_one_of_the_offered() {
        let fields = [
            field("phone", SiteBookingFieldKind::Phone, true, &[]),
            field("cut", SiteBookingFieldKind::Choice, false, &["Dry", "Wet"]),
        ];
        let missing = normalize_answers(&fields, &posted(&[("cut", "Wet")])).unwrap_err();
        assert!(format!("{missing}").contains("The phone"), "{missing}");
        let invented =
            normalize_answers(&fields, &posted(&[("phone", "01"), ("cut", "Shaved")])).unwrap_err();
        assert!(format!("{invented}").contains("The cut"), "{invented}");
        // An optional question left blank is simply absent.
        let sparse = normalize_answers(&fields, &posted(&[("phone", "01")])).unwrap();
        assert_eq!(sparse.len(), 1);
    }

    #[test]
    fn a_visitor_needs_a_name_and_something_that_could_be_an_address() {
        assert_eq!(
            normalize_visitor_email(" ada@example.test ").unwrap(),
            "ada@example.test"
        );
        for bad in [
            "",
            "   ",
            "not-an-email",
            "@example.test",
            "ada@",
            "a b@c.test",
        ] {
            assert!(
                matches!(normalize_visitor_email(bad), Err(StoreError::Validation(_))),
                "{bad} was accepted"
            );
        }
        assert!(
            required_visitor_text("  ", "name", BOOKING_VISITOR_NAME_MAX_CHARS).is_err(),
            "a blank name is not a name"
        );
    }

    #[test]
    fn the_owners_calendar_entry_says_how_to_answer_and_what_was_asked() {
        let described = describe(
            "ada@example.test",
            &[BookingAnswer {
                key: "phone".to_owned(),
                label: "Phone".to_owned(),
                value: "+32 2 555 01".to_owned(),
            }],
        );
        assert!(described.contains("ada@example.test"), "{described}");
        assert!(described.contains("Phone: +32 2 555 01"), "{described}");
    }
}
