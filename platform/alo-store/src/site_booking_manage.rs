//! The visitor's side of a reservation, after it is taken: seeing it,
//! importing it into their own calendar, and **cancelling** it.
//!
//! This module is what makes a booking a *reversible* act in the ADR 0040 §2
//! sense — enforced in code, not in a prompt. The assistant (and the plain
//! booking form) may create an appointment because the visitor can undo it
//! themselves: every reservation mints an opaque `manage_token`, and whoever
//! holds that token — it travels only in the visitor's own confirmation — can
//! read exactly one appointment's name and time, download its iCalendar view,
//! and cancel it up to the moment it starts. Nothing else: not the owner's
//! calendar, not another appointment, not even the answers the visitor
//! themselves typed.
//!
//! Scoping: the token is globally unique, but every read here additionally
//! requires the [`PublishedSite`] the request arrived on — a token minted on
//! one site answers nothing on any other host, so a leaked token cannot even
//! be *probed* against the wrong site. Unknown, foreign-site, and foreign-
//! tenant tokens are all the same `None`, and the public wire turns that into
//! its one uniform 404.
//!
//! Cancelling flips the row's status — which frees the slot, since
//! availability only ever subtracts `status = 'booked'` — and then removes
//! the owner's calendar event through the same Agenda door that wrote it, so
//! the removal rides Agenda's own rules and change log. A calendar or event
//! already gone is fine: the reservation ledger is the fact, the event is the
//! owner's view of it.

use time::OffsetDateTime;

use crate::error::{Result, StoreError};
use crate::id::{SiteBookingAppointmentId, UserId};
use crate::site_public::{PublishedSite, SitePublicStore};

/// The longest token this door will even send to the database — real tokens
/// are 22 characters (base64url of 16 random bytes).
const MANAGE_TOKEN_MAX_LEN: usize = 64;

/// One appointment as its own visitor may see it: what was booked and when —
/// never the owner's calendar, never the answers.
#[derive(Debug, Clone)]
pub struct ManagedAppointment {
    pub id: SiteBookingAppointmentId,
    pub booking_name: String,
    pub starts_at: OffsetDateTime,
    pub ends_at: OffsetDateTime,
    /// The zone the time was offered in — the clock the confirmation shows.
    pub time_zone: String,
    /// `true` while the reservation stands; `false` once cancelled.
    pub booked: bool,
}

/// What cancelling by token came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CancelOutcome {
    /// The reservation is withdrawn and the slot is free again.
    Cancelled { booking_name: String },
    /// It was already cancelled — telling the visitor so is more honest than
    /// pretending to do it again.
    AlreadyCancelled { booking_name: String },
    /// The appointment has already started; undoing the past is not offered.
    TooLate { booking_name: String },
}

#[derive(sqlx::FromRow)]
struct ManagedRow {
    id: String,
    booking_name: String,
    starts_at: OffsetDateTime,
    ends_at: OffsetDateTime,
    time_zone: String,
    status: String,
}

/// What the cancel path reads before deciding: the appointment's state plus
/// whose Agenda door the event removal must go through.
#[derive(sqlx::FromRow)]
struct CancelRow {
    status: String,
    booking_name: String,
    starts_at: OffsetDateTime,
    event_id: Option<String>,
    owner: Option<String>,
}

impl SitePublicStore {
    /// Resolves a manage token **on the site it was minted for**, or `None` —
    /// one answer for unknown, malformed, foreign-site and foreign-tenant
    /// tokens alike.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn managed_appointment(
        &self,
        site: &PublishedSite,
        token: &str,
    ) -> Result<Option<ManagedAppointment>> {
        let Some(token) = plausible(token) else {
            return Ok(None);
        };
        let row: Option<ManagedRow> = sqlx::query_as(
            "SELECT id, booking_name, starts_at, ends_at, time_zone, status \
             FROM site_booking_appointments \
             WHERE tenant_id = $1 AND site_id = $2 AND manage_token = $3",
        )
        .bind(site.tenant.as_str())
        .bind(site.site.as_str())
        .bind(token)
        .fetch_optional(self.pool())
        .await
        .map_err(StoreError::Db)?;
        Ok(row.map(|row| ManagedAppointment {
            id: SiteBookingAppointmentId::new(row.id),
            booking_name: row.booking_name,
            starts_at: row.starts_at,
            ends_at: row.ends_at,
            time_zone: row.time_zone,
            booked: row.status == "booked",
        }))
    }

    /// Cancels the appointment a token stands for, on the site it was minted
    /// for. `Ok(None)` is the same uniform absence as
    /// [`Self::managed_appointment`]; a reservation that still stands is
    /// withdrawn and its owner-calendar event removed.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn cancel_managed_appointment(
        &self,
        site: &PublishedSite,
        token: &str,
        now: OffsetDateTime,
    ) -> Result<Option<CancelOutcome>> {
        let Some(token) = plausible(token) else {
            return Ok(None);
        };
        // Ownership of the event is resolved in the same read: the calendar's
        // current owner is whoever's Agenda door the removal must go through.
        // A calendar deleted since booking leaves owner NULL — the event rows
        // are gone with it, and there is nothing left to remove.
        let row: Option<CancelRow> = sqlx::query_as(
            "SELECT a.status, a.booking_name, a.starts_at, a.event_id, \
                    c.owner_user_id AS owner \
             FROM site_booking_appointments a \
             LEFT JOIN calendars c ON c.tenant_id = a.tenant_id AND c.id = a.calendar_id \
             WHERE a.tenant_id = $1 AND a.site_id = $2 AND a.manage_token = $3",
        )
        .bind(site.tenant.as_str())
        .bind(site.site.as_str())
        .bind(token)
        .fetch_optional(self.pool())
        .await
        .map_err(StoreError::Db)?;
        let Some(CancelRow {
            status,
            booking_name,
            starts_at,
            event_id,
            owner,
        }) = row
        else {
            return Ok(None);
        };
        if status != "booked" {
            return Ok(Some(CancelOutcome::AlreadyCancelled { booking_name }));
        }
        if starts_at <= now {
            return Ok(Some(CancelOutcome::TooLate { booking_name }));
        }
        // The status flip is the cancellation: the guarded WHERE keeps two
        // concurrent cancels (or a cancel racing the start of the meeting)
        // from both claiming to have done it.
        let done = sqlx::query(
            "UPDATE site_booking_appointments SET status = 'cancelled' \
             WHERE tenant_id = $1 AND site_id = $2 AND manage_token = $3 \
               AND status = 'booked' AND starts_at > $4",
        )
        .bind(site.tenant.as_str())
        .bind(site.site.as_str())
        .bind(token)
        .bind(now)
        .execute(self.pool())
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Ok(Some(CancelOutcome::AlreadyCancelled { booking_name }));
        }
        // The owner's view follows the fact. Through the same door that wrote
        // it, so Agenda's change log tells their clients; an event or calendar
        // already gone is not a failure of the cancellation.
        if let (Some(event_id), Some(owner)) = (event_id, owner) {
            let door = crate::site_agenda::agenda_door(
                self.pool().clone(),
                self.blobs().clone(),
                site.tenant.clone(),
                UserId::new(owner),
            );
            match door.delete_event(&crate::id::EventId::new(event_id)).await {
                Ok(()) | Err(StoreError::NotFound) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(Some(CancelOutcome::Cancelled { booking_name }))
    }
}

/// The shape gate before any token reaches the database.
fn plausible(token: &str) -> Option<&str> {
    let token = token.trim();
    (!token.is_empty()
        && token.len() <= MANAGE_TOKEN_MAX_LEN
        && token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'))
    .then_some(token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_plausible_token_reaches_the_database() {
        assert_eq!(plausible(" abc-DEF_09 "), Some("abc-DEF_09"));
        assert_eq!(plausible(""), None);
        assert_eq!(plausible("   "), None);
        assert_eq!(plausible("has space"), None);
        assert_eq!(plausible("semi;colon"), None);
        assert_eq!(plausible(&"x".repeat(MANAGE_TOKEN_MAX_LEN + 1)), None);
    }
}
