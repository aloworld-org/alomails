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
use crate::model::CalendarEvent;
use sqlx::PgPool;

/// One span of time a calendar's owner is not free in. This type is the whole
/// of what Agenda will say to an anonymous caller: when, and nothing about
/// what.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CalendarBusySpan {
    pub from: OffsetDateTime,
    pub to: OffsetDateTime,
}

/// Reduce events to the minimal busy answer for `[from, to)`: each event
/// clamped to the window, empties dropped, overlaps and touching spans merged,
/// earliest first. This is the one merge every busy/free surface uses — the
/// Agenda scheduling grid and the CalDAV `free-busy-query` both speak its
/// output — so "busy" never means two different things on two wires.
#[must_use]
pub fn merged_busy_spans(
    events: &[CalendarEvent],
    from: OffsetDateTime,
    to: OffsetDateTime,
) -> Vec<CalendarBusySpan> {
    let mut spans: Vec<(OffsetDateTime, OffsetDateTime)> = events
        .iter()
        .map(|e| (e.starts_at.max(from), e.ends_at.min(to)))
        .filter(|(s, e)| e > s)
        .collect();
    spans.sort_by_key(|(s, _)| *s);
    let mut merged: Vec<CalendarBusySpan> = Vec::new();
    for (s, e) in spans {
        match merged.last_mut() {
            Some(last) if s <= last.to => last.to = last.to.max(e),
            _ => merged.push(CalendarBusySpan { from: s, to: e }),
        }
    }
    merged
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::id::EventId;

    fn at(h: u8, m: u8) -> OffsetDateTime {
        OffsetDateTime::new_utc(
            time::Date::from_calendar_date(2026, time::Month::June, 10).unwrap(),
            time::Time::from_hms(h, m, 0).unwrap(),
        )
    }

    fn event(start: OffsetDateTime, end: OffsetDateTime) -> CalendarEvent {
        CalendarEvent {
            id: EventId::new("e"),
            calendar_id: CalendarId::new("c"),
            summary: "secret title".to_owned(),
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

    #[test]
    fn spans_are_clamped_merged_and_sorted() {
        let (from, to) = (at(8, 0), at(18, 0));
        // Out of order, overlapping, touching, one straddling the window edge,
        // and one entirely outside.
        let events = vec![
            event(at(9, 30), at(10, 30)),
            event(at(9, 0), at(10, 0)),
            event(at(10, 30), at(11, 0)), // touches the merged block → joins it
            event(at(7, 0), at(8, 30)),   // clamped to the window start
            event(at(19, 0), at(20, 0)),  // outside → dropped
        ];
        let busy = merged_busy_spans(&events, from, to);
        assert_eq!(
            busy,
            vec![
                CalendarBusySpan {
                    from: at(8, 0),
                    to: at(8, 30)
                },
                CalendarBusySpan {
                    from: at(9, 0),
                    to: at(11, 0)
                },
            ]
        );
    }

    #[test]
    fn empty_and_disjoint_inputs_pass_through() {
        let (from, to) = (at(8, 0), at(18, 0));
        assert!(merged_busy_spans(&[], from, to).is_empty());
        let events = vec![event(at(9, 0), at(10, 0)), event(at(12, 0), at(13, 0))];
        assert_eq!(merged_busy_spans(&events, from, to).len(), 2);
    }
}
