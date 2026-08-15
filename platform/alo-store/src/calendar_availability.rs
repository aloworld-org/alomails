//! Agenda's public availability seam — the one thing an anonymous request may
//! learn from a calendar (ADR 0040 §4).
//!
//! A published site offers appointments, and the visitor taking one is nobody:
//! no session, no user, no membership in the workspace. Something still has to
//! answer *when is the owner busy*, and before this module existed the answer
//! was Sites opening the owner's whole account door and reading full events —
//! titles, guests, notes — then discarding everything but the times. The
//! boundary held by convention. This module makes it hold by construction:
//! Agenda itself answers, and the only vocabulary it will speak across this
//! seam is [`CalendarBusySpan`] — a start and an end. There is no field for
//! anything else, so nothing else can cross, whatever the calling code does.
//!
//! This file belongs to Agenda, not to Sites. Sites names a calendar it was
//! given at binding time ([`crate::site_agenda`]) and asks here; how a
//! recurring series expands, where a moved occurrence lands, that a cancelled
//! one blocks nothing, and which calendars an owner can see at all stay
//! Agenda's to decide, in one place — the seam rides
//! [`AccountStore::events_in_range`] internally rather than re-deriving any of
//! it.
//!
//! Scoping: a door is opened with a `(tenant, owner)` pair the caller must
//! already have resolved from its own trusted row (for Sites, the published
//! snapshot joined to the calendar's owner). Everything the door then reads
//! carries that pair, so a calendar of another tenant — or of another user
//! that was never shared — yields nothing, which tests prove.

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::blob::BlobStore;
use crate::error::Result;
use crate::id::{CalendarId, TenantId, UserId};
use sqlx::PgPool;

/// One span of time a calendar's owner is not free in. This type is the whole
/// of what Agenda will say to an anonymous caller: when, and nothing about
/// what.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CalendarBusySpan {
    pub from: OffsetDateTime,
    pub to: OffsetDateTime,
}

/// A read-only door onto one account's calendars that can only answer *busy
/// or not*. Open it with a tenant and owner resolved from a trusted row;
/// everything it reads is scoped to that pair.
pub struct CalendarAvailability {
    account: AccountStore,
}

impl CalendarAvailability {
    /// Opens the availability door of one account's Agenda.
    ///
    /// The caller vouches for the pair: `tenant` and `owner` must come from a
    /// row the caller already trusts (a published booking snapshot, never a
    /// request). The door then enforces the pair on every read.
    #[must_use]
    pub fn open(pool: PgPool, blobs: BlobStore, tenant: TenantId, owner: UserId) -> Self {
        Self {
            account: AccountStore {
                pool,
                blobs,
                tenant,
                user: owner,
            },
        }
    }

    /// The spans of `[from, to)` the owner is busy in on one calendar,
    /// earliest first.
    ///
    /// Recurring series are expanded, moved occurrences land where they were
    /// moved to, and cancelled ones block nothing — all through Agenda's own
    /// [`AccountStore::events_in_range`]. A calendar the opened account cannot
    /// see — another tenant's, another user's never-shared one, a deleted one,
    /// or one that never existed — is an empty answer, indistinguishable from
    /// a free week.
    ///
    /// # Errors
    /// [`StoreError::Db`](crate::error::StoreError::Db) on failure.
    pub async fn busy_spans(
        &self,
        calendar: &CalendarId,
        from: OffsetDateTime,
        to: OffsetDateTime,
    ) -> Result<Vec<CalendarBusySpan>> {
        Ok(self
            .account
            .events_in_range(from, to)
            .await?
            .into_iter()
            .filter(|event| event.calendar_id.as_str() == calendar.as_str())
            .map(|event| CalendarBusySpan {
                from: event.starts_at,
                to: event.ends_at,
            })
            .collect())
    }
}
