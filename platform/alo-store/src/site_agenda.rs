//! The seam alo Sites reads Agenda through.
//!
//! Sites needs one thing from Agenda and nothing else: *which calendar may a
//! booking service be attached to, and may we write an appointment into it*.
//! This module is the only place in the Sites code that asks — every other
//! site module names an availability source by its [`CalendarId`] and comes
//! here to resolve it. Agenda's own module ([`crate::calendar`]) is not edited
//! by Sites and is not reached around: this seam is built strictly on its
//! public reads, so a change to how a calendar is shared changes Sites through
//! one function rather than through a search.
//!
//! Two rules hold here.
//!
//! * **A source must be writable.** Availability could be read from a calendar
//!   shared read-only, but the booking that follows (S2.13b) has to put the
//!   appointment somewhere. A source that cannot be written to would fail at
//!   the only moment that matters — a visitor pressing *book* — so it is
//!   refused at binding time instead, by name.
//! * **A source is resolved, never cached.** Agenda owns a calendar's
//!   lifetime; a booking service stores only the id. A calendar that has since
//!   been deleted or unshared resolves to `None`, which the editor shows as a
//!   broken connection rather than as an empty week.
//!
//! Both reads go through [`AccountStore`], so they carry the account door's
//! tenant *and* user scoping: a calendar of another tenant — or of another
//! user of the same tenant that was never shared — does not resolve.
//!
//! What is deliberately *not* here any more: reading busy time for an
//! anonymous visitor. That is Agenda's own seam now
//! ([`crate::calendar_availability`]), which can answer nothing but spans —
//! so the public path never holds a full event, by construction rather than
//! by promise. This module keeps the picker reads (which need an
//! authenticated owner anyway) and the write half below.

use sqlx::PgPool;

use crate::account::AccountStore;
use crate::blob::BlobStore;
use crate::error::Result;
use crate::id::{CalendarId, TenantId, UserId};

/// One calendar a site's booking service may be attached to, as Sites sees
/// it: an id, something to call it in a picker, and whether an appointment can
/// be written into it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteAvailabilitySource {
    /// The Agenda calendar.
    pub calendar: CalendarId,
    /// The calendar's display name, for the connection picker.
    pub name: String,
    /// Whether this account may add events to it (owner or editor). A
    /// read-only share is visible here — so the editor can say *why* it cannot
    /// be chosen — but cannot be bound.
    pub writable: bool,
}

impl AccountStore {
    /// Every calendar this account can see, as availability sources in
    /// Agenda's own order (owned first).
    ///
    /// Listing includes read-only shares deliberately: a picker that silently
    /// omitted a calendar the owner can see would be a puzzle, while one that
    /// shows it as unusable is an explanation.
    ///
    /// # Errors
    /// [`StoreError::Db`](crate::error::StoreError::Db) on failure.
    pub async fn site_availability_sources(&self) -> Result<Vec<SiteAvailabilitySource>> {
        Ok(self
            .calendars()
            .await?
            .into_iter()
            .map(|calendar| SiteAvailabilitySource {
                calendar: calendar.id,
                name: calendar.name,
                writable: calendar.role != "viewer",
            })
            .collect())
    }

    /// Resolves one availability source, or `None` when this account cannot
    /// see that calendar at all — which is the same answer for a calendar that
    /// never existed, one that has been deleted, one belonging to another user
    /// that was never shared, and one belonging to another tenant.
    ///
    /// # Errors
    /// [`StoreError::Db`](crate::error::StoreError::Db) on failure.
    pub async fn site_availability_source(
        &self,
        calendar: &CalendarId,
    ) -> Result<Option<SiteAvailabilitySource>> {
        Ok(self
            .site_availability_sources()
            .await?
            .into_iter()
            .find(|source| source.calendar.as_str() == calendar.as_str()))
    }

}

/// Opens the Agenda door of the account that owns a bound calendar — the
/// **write** half of the seam: putting the appointment a visitor took into
/// the owner's calendar, through Agenda's own rules and change log.
///
/// This is the one place a request with **no user** — an anonymous visitor
/// booking on a published site — crosses into an owner-scoped store. It is
/// deliberately here, in the seam, rather than on the public store: the public
/// door has no account access by construction ([`crate::site_public`]), and the
/// only way past that is this function, whose caller must already have resolved
/// the tenant and the calendar's owner from a published snapshot. Everything
/// the returned door then does carries the owner's own tenant and user
/// scoping. Reading, by contrast, no longer needs this door at all:
/// availability is asked of [`crate::calendar_availability`], which cannot
/// answer with anything but spans.
pub(crate) fn agenda_door(
    pool: PgPool,
    blobs: BlobStore,
    tenant: TenantId,
    owner: UserId,
) -> AccountStore {
    AccountStore {
        pool,
        blobs,
        tenant,
        user: owner,
    }
}
