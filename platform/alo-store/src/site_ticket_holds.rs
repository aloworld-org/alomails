//! The hold that makes overselling impossible (ADR 0041 — "capacity is a
//! hold, not a check; this is the first commit, not hardening").
//!
//! Check-then-create is a race two buyers will win simultaneously, so a seat
//! is **reserved before payment**: a hold is taken while the buyer is still
//! deciding, counts against the event's capacity from that instant, and stops
//! counting the instant it expires. The buyer who finishes has their hold
//! completed by the payment path (S3.04c); the buyer who walks away releases
//! it, or simply lets it lapse — expiry is a time predicate
//! (`expires_at > now`), not a sweeper, so an abandoned checkout never blocks
//! a live buyer for even a second and no background job is needed for
//! correctness.
//!
//! Two buyers, one seat — the race this module exists for — is settled by the
//! database rather than by timing, the same way the booking reservation
//! settles it ([`crate::site_public_bookings`]): every state-changing path
//! takes a transaction-scoped advisory lock on `(tenant, event)` before it
//! counts, so the count and the write are one decision and the second writer
//! is told "sold out" rather than given the same seat.
//!
//! Privacy: a hold is pure seat accounting and stores **no buyer identity of
//! any kind** — no name, no address, no token. Who bought lives where the
//! sale puts it (the order, the invoice, the CRM card — S3.04c/d), in
//! records the tenant already owns. The tenancy proof and the columns-of-the-
//! table proof live in `tests/site_ticket_holds.rs`.

use time::{Duration, OffsetDateTime};

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::{SiteId, SiteTicketEventId, SiteTicketHoldId, TenantId};

/// The most seats one hold may cover — a buyer's basket, not a scalper's
/// script. A visitor wanting more is a phone call, not a checkout.
pub const TICKET_HOLD_MAX_QUANTITY: i32 = 20;
/// The shortest life a hold may be given — under a minute is a race with the
/// payment page itself.
pub const TICKET_HOLD_MIN_TTL: Duration = Duration::minutes(1);
/// The longest life a hold may be given — beyond an hour an abandoned basket
/// is squatting seats real buyers want.
pub const TICKET_HOLD_MAX_TTL: Duration = Duration::hours(1);

/// Where one hold stands. `Expired` is derived on read: a stored `held` row
/// whose expiry has passed already counts for nothing, whether or not
/// anything has tidied it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiteTicketHoldState {
    /// Counting against capacity until `expires_at`.
    Held,
    /// A seat sold — counts against capacity forever.
    Completed,
    /// The buyer walked away; the seats are free again.
    Released,
    /// The buyer never finished; the seats freed themselves at expiry.
    Expired,
}

/// One hold as the machinery reports it. No buyer fields exist to expose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteTicketHold {
    pub id: SiteTicketHoldId,
    pub event: SiteTicketEventId,
    pub quantity: i32,
    pub state: SiteTicketHoldState,
    pub expires_at: OffsetDateTime,
    pub created_at: OffsetDateTime,
}

/// The seat arithmetic of one event at one instant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TicketAvailability {
    pub capacity: i32,
    /// Seats on completed holds — sold, counting forever.
    pub sold: i64,
    /// Seats on live holds — counting until they complete, release or expire.
    pub held: i64,
    /// What a new buyer could take right now.
    pub remaining: i64,
}

#[derive(sqlx::FromRow)]
struct HoldRow {
    id: String,
    event_id: String,
    quantity: i32,
    state: String,
    expires_at: OffsetDateTime,
    created_at: OffsetDateTime,
}

impl HoldRow {
    fn into_hold(self, now: OffsetDateTime) -> Result<SiteTicketHold> {
        let state = match self.state.as_str() {
            "held" if self.expires_at <= now => SiteTicketHoldState::Expired,
            "held" => SiteTicketHoldState::Held,
            "completed" => SiteTicketHoldState::Completed,
            "released" => SiteTicketHoldState::Released,
            "expired" => SiteTicketHoldState::Expired,
            other => {
                return Err(StoreError::Conflict(format!(
                    "stored hold state '{other}' is not one this release knows"
                )));
            }
        };
        Ok(SiteTicketHold {
            id: SiteTicketHoldId::new(self.id),
            event: SiteTicketEventId::new(self.event_id),
            quantity: self.quantity,
            state,
            expires_at: self.expires_at,
            created_at: self.created_at,
        })
    }
}

/// One writer at a time per event, for the length of the transaction: the
/// seat count and the write that follows it are one decision, and two buyers
/// must not interleave them.
pub(crate) async fn lock_ticket_event(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant: &TenantId,
    event: &SiteTicketEventId,
) -> Result<()> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1), hashtext($2))")
        .bind(tenant.as_str())
        .bind(event.as_str())
        .execute(&mut **tx)
        .await
        .map_err(StoreError::Db)?;
    Ok(())
}

/// Seats committed on one event at `now` — sold plus live holds — or `None`
/// when the event is not this tenant's on this site. Runs inside the caller's
/// transaction so it can sit under the advisory lock.
pub(crate) async fn seats_committed(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant: &TenantId,
    site: &SiteId,
    event: &SiteTicketEventId,
    now: OffsetDateTime,
) -> Result<Option<i64>> {
    sqlx::query_scalar(
        "SELECT (SELECT COALESCE(SUM(h.quantity), 0) FROM site_ticket_holds h \
                 WHERE h.tenant_id = e.tenant_id AND h.event_id = e.id \
                   AND (h.state = 'completed' \
                        OR (h.state = 'held' AND h.expires_at > $4))) \
         FROM site_ticket_events e \
         WHERE e.tenant_id = $1 AND e.site_id = $2 AND e.id = $3",
    )
    .bind(tenant.as_str())
    .bind(site.as_str())
    .bind(event.as_str())
    .bind(now)
    .fetch_optional(&mut **tx)
    .await
    .map_err(StoreError::Db)
}

impl AccountStore {
    /// Reserves `quantity` seats on an event, for `ttl` from `now` — the
    /// step that comes **before** payment. The count and the insert run under
    /// the per-event advisory lock, so of two simultaneous buyers after the
    /// last seat exactly one gets it and the other is told so.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the quantity or the ttl is out of
    /// range; [`StoreError::NotFound`] when the event is not this tenant's on
    /// this site; [`StoreError::Conflict`] when the event has started or the
    /// seats are not there — "sold out", or how many are left when some are;
    /// [`StoreError::Db`] on failure.
    pub async fn take_ticket_hold(
        &self,
        site: &SiteId,
        event: &SiteTicketEventId,
        quantity: i32,
        ttl: Duration,
        now: OffsetDateTime,
    ) -> Result<SiteTicketHold> {
        if !(1..=TICKET_HOLD_MAX_QUANTITY).contains(&quantity) {
            return Err(StoreError::Validation(format!(
                "a hold may cover between 1 and {TICKET_HOLD_MAX_QUANTITY} seats"
            )));
        }
        if !(TICKET_HOLD_MIN_TTL..=TICKET_HOLD_MAX_TTL).contains(&ttl) {
            return Err(StoreError::Validation(
                "a hold must last between one minute and one hour".to_owned(),
            ));
        }
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        lock_ticket_event(&mut tx, &self.tenant, event).await?;
        let found: Option<(i32, OffsetDateTime)> = sqlx::query_as(
            "SELECT capacity, starts_at FROM site_ticket_events \
             WHERE tenant_id = $1 AND site_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(event.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        let (capacity, starts_at) = found.ok_or(StoreError::NotFound)?;
        if starts_at <= now {
            return Err(StoreError::Conflict(
                "this event has already started".to_owned(),
            ));
        }
        let committed = seats_committed(&mut tx, &self.tenant, site, event, now)
            .await?
            .ok_or(StoreError::NotFound)?;
        let remaining = i64::from(capacity) - committed;
        if remaining < i64::from(quantity) {
            return Err(StoreError::Conflict(if remaining <= 0 {
                "sold out".to_owned()
            } else if remaining == 1 {
                "only 1 seat is left".to_owned()
            } else {
                format!("only {remaining} seats are left")
            }));
        }
        let id = SiteTicketHoldId::generate();
        let expires_at = now + ttl;
        sqlx::query(
            "INSERT INTO site_ticket_holds \
                (tenant_id, site_id, id, event_id, quantity, expires_at, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(id.as_str())
        .bind(event.as_str())
        .bind(quantity)
        .bind(expires_at)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(SiteTicketHold {
            id,
            event: event.clone(),
            quantity,
            state: SiteTicketHoldState::Held,
            expires_at,
            created_at: now,
        })
    }

    /// One hold as it stands at `now`, or `None`. A stored `held` row past
    /// its expiry reads as [`SiteTicketHoldState::Expired`].
    ///
    /// # Errors
    /// [`StoreError::Conflict`] when the stored state is unreadable;
    /// [`StoreError::Db`] on failure.
    pub async fn site_ticket_hold(
        &self,
        site: &SiteId,
        hold: &SiteTicketHoldId,
        now: OffsetDateTime,
    ) -> Result<Option<SiteTicketHold>> {
        let row = sqlx::query_as::<_, HoldRow>(
            "SELECT id, event_id, quantity, state, expires_at, created_at \
             FROM site_ticket_holds \
             WHERE tenant_id = $1 AND site_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(hold.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        row.map(|row| row.into_hold(now)).transpose()
    }

    /// Marks a hold's seats as **sold** — what the payment path calls when
    /// the money is confirmed (S3.04c). Only a live, unexpired hold can
    /// complete: a buyer whose hold lapsed mid-payment is refused here and
    /// handled by the payment flow, never given seats that may since have
    /// been sold again. Completing an already-completed hold returns it
    /// unchanged, so a retried payment webhook is harmless.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the hold is not this tenant's on this
    /// site; [`StoreError::Conflict`] when it expired or was released;
    /// [`StoreError::Db`] on failure.
    pub async fn complete_ticket_hold(
        &self,
        site: &SiteId,
        hold: &SiteTicketHoldId,
        now: OffsetDateTime,
    ) -> Result<SiteTicketHold> {
        let row = sqlx::query_as::<_, HoldRow>(
            "UPDATE site_ticket_holds SET state = 'completed', completed_at = $4 \
             WHERE tenant_id = $1 AND site_id = $2 AND id = $3 \
               AND state = 'held' AND expires_at > $4 \
             RETURNING id, event_id, quantity, state, expires_at, created_at",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(hold.as_str())
        .bind(now)
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if let Some(row) = row {
            return row.into_hold(now);
        }
        // The guarded update refused: say why, in the buyer's terms.
        let stood = self
            .site_ticket_hold(site, hold, now)
            .await?
            .ok_or(StoreError::NotFound)?;
        match stood.state {
            SiteTicketHoldState::Completed => Ok(stood),
            SiteTicketHoldState::Expired => Err(StoreError::Conflict(
                "this hold has expired and its seats have been released".to_owned(),
            )),
            SiteTicketHoldState::Released => Err(StoreError::Conflict(
                "this hold was released".to_owned(),
            )),
            // Unreachable in practice: a live hold satisfies the guard. If a
            // racing writer flipped it between the two reads, answer with the
            // freshest truth we have.
            SiteTicketHoldState::Held => Err(StoreError::Conflict(
                "this hold could not be completed; try again".to_owned(),
            )),
        }
    }

    /// Gives a hold's seats back — the buyer walked away. Releasing a hold
    /// that already lapsed or was already released is a success (the seats
    /// are free either way, and a cancel button may be pressed twice); a
    /// completed hold is a sale and cannot be released here.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the hold is not this tenant's on this
    /// site; [`StoreError::Conflict`] when it was already completed;
    /// [`StoreError::Db`] on failure.
    pub async fn release_ticket_hold(
        &self,
        site: &SiteId,
        hold: &SiteTicketHoldId,
        now: OffsetDateTime,
    ) -> Result<SiteTicketHold> {
        let row = sqlx::query_as::<_, HoldRow>(
            "UPDATE site_ticket_holds SET state = 'released' \
             WHERE tenant_id = $1 AND site_id = $2 AND id = $3 AND state = 'held' \
             RETURNING id, event_id, quantity, state, expires_at, created_at",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(hold.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if let Some(row) = row {
            return row.into_hold(now);
        }
        let stood = self
            .site_ticket_hold(site, hold, now)
            .await?
            .ok_or(StoreError::NotFound)?;
        match stood.state {
            SiteTicketHoldState::Released | SiteTicketHoldState::Expired => Ok(stood),
            SiteTicketHoldState::Completed => Err(StoreError::Conflict(
                "this purchase is complete; seats can no longer be released".to_owned(),
            )),
            SiteTicketHoldState::Held => Err(StoreError::Conflict(
                "this hold could not be released; try again".to_owned(),
            )),
        }
    }

    /// The seat arithmetic of one event at `now`.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the event is not this tenant's on this
    /// site; [`StoreError::Db`] on failure.
    pub async fn ticket_availability(
        &self,
        site: &SiteId,
        event: &SiteTicketEventId,
        now: OffsetDateTime,
    ) -> Result<TicketAvailability> {
        let found: Option<(i32, i64, i64)> = sqlx::query_as(
            "SELECT e.capacity, \
                    (SELECT COALESCE(SUM(h.quantity), 0) FROM site_ticket_holds h \
                     WHERE h.tenant_id = e.tenant_id AND h.event_id = e.id \
                       AND h.state = 'completed'), \
                    (SELECT COALESCE(SUM(h.quantity), 0) FROM site_ticket_holds h \
                     WHERE h.tenant_id = e.tenant_id AND h.event_id = e.id \
                       AND h.state = 'held' AND h.expires_at > $4) \
             FROM site_ticket_events e \
             WHERE e.tenant_id = $1 AND e.site_id = $2 AND e.id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(event.as_str())
        .bind(now)
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        let (capacity, sold, held) = found.ok_or(StoreError::NotFound)?;
        Ok(TicketAvailability {
            capacity,
            sold,
            held,
            remaining: i64::from(capacity) - sold - held,
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn row(state: &str, expires_in_seconds: i64) -> HoldRow {
        HoldRow {
            id: "hold".to_owned(),
            event_id: "event".to_owned(),
            quantity: 2,
            state: state.to_owned(),
            expires_at: OffsetDateTime::UNIX_EPOCH + Duration::seconds(expires_in_seconds),
            created_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn a_stored_hold_past_its_expiry_reads_as_expired() {
        let now = OffsetDateTime::UNIX_EPOCH + Duration::seconds(100);
        let live = row("held", 101).into_hold(now).unwrap();
        assert_eq!(live.state, SiteTicketHoldState::Held);
        let lapsed = row("held", 100).into_hold(now).unwrap();
        assert_eq!(lapsed.state, SiteTicketHoldState::Expired);
        let tidied = row("expired", 100).into_hold(now).unwrap();
        assert_eq!(tidied.state, SiteTicketHoldState::Expired);
    }

    #[test]
    fn terminal_states_read_as_themselves_and_unknown_words_refuse() {
        let now = OffsetDateTime::UNIX_EPOCH;
        assert_eq!(
            row("completed", -1).into_hold(now).unwrap().state,
            SiteTicketHoldState::Completed
        );
        assert_eq!(
            row("released", -1).into_hold(now).unwrap().state,
            SiteTicketHoldState::Released
        );
        assert!(matches!(
            row("haggled", 1).into_hold(now),
            Err(StoreError::Conflict(_))
        ));
    }
}
