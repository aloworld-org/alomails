//! Turning a published week into the times a visitor may actually pick.
//!
//! This module is deliberately pure: opening hours, busy intervals, a day and
//! an instant go in; a list of free slots comes out. Nothing here touches the
//! database, which is what lets the awkward parts — daylight saving, the notice
//! a business asks for, the quiet gap it keeps between appointments — be tested
//! exhaustively and cheaply.
//!
//! Three rules decide what is offered.
//!
//! * **The week grants, the calendar only takes away.** A slot exists because
//!   the owner declared an opening window that holds it; a busy calendar
//!   interval or an existing reservation then removes it. An empty calendar is
//!   never an invitation on a day the owner is closed.
//! * **The clock is the owner's, and it is a real clock.** Windows are minutes
//!   of a local day in the service's own IANA zone, so an appointment at nine
//!   is at nine on both sides of a daylight-saving change. An hour that does
//!   not exist on the day the clocks go forward is silently not offered — a
//!   time nobody can arrive at is not a slot — and an hour that happens twice
//!   is offered once.
//! * **Every boundary is half-open.** A slot `[start, end)` and a busy interval
//!   `[from, to)` collide only when they genuinely overlap once the buffer is
//!   applied on both sides, so an appointment ending exactly when the next
//!   window's appointment begins is not a clash.

use time::{Date, Duration, OffsetDateTime};

use crate::site_bookings::SiteBookingWindow;

/// One offered appointment time, as instants. The visitor sees it in the
/// service's zone; everything stored and compared is UTC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BookingSlot {
    pub starts_at: OffsetDateTime,
    pub ends_at: OffsetDateTime,
}

/// Everything about a published service that decides which slots exist. Taken
/// by reference from the frozen snapshot — never from the editable row, which
/// may already say something else.
#[derive(Debug, Clone, Copy)]
pub struct BookingRules<'a> {
    /// The weekly opening pattern, ISO weekdays with minute-of-day bounds.
    pub hours: &'a [SiteBookingWindow],
    /// IANA zone the windows are written in.
    pub time_zone: &'a str,
    pub duration_minutes: i32,
    /// Quiet time kept on each side of an appointment.
    pub buffer_minutes: i32,
    /// Shortest notice a visitor may book at.
    pub notice_minutes: i32,
    /// How far ahead the public calendar opens.
    pub horizon_days: i32,
}

/// A span of time the owner is not free in — a calendar event or an
/// appointment already reserved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BusyInterval {
    pub from: OffsetDateTime,
    pub to: OffsetDateTime,
}

/// The free slots of one local day, in chronological order.
///
/// `busy` may hold anything; only what overlaps matters. `now` is the instant
/// the visitor is asking at, which is what the notice and the horizon are
/// measured from — passed in rather than read here so the caller's tests can
/// stand anywhere in time.
#[must_use]
pub fn free_slots(
    rules: &BookingRules<'_>,
    day: Date,
    busy: &[BusyInterval],
    now: OffsetDateTime,
) -> Vec<BookingSlot> {
    let Ok(zone) = jiff::tz::TimeZone::get(rules.time_zone) else {
        // A zone this build cannot resolve is refused at write time
        // (`site_bookings`), so reaching here means a snapshot older than the
        // zone database. Offering nothing is the only safe answer.
        return Vec::new();
    };
    let duration = i64::from(rules.duration_minutes.max(1));
    let buffer = Duration::minutes(i64::from(rules.buffer_minutes.max(0)));
    let step = duration + i64::from(rules.buffer_minutes.max(0));
    let earliest = now.saturating_add(Duration::minutes(i64::from(rules.notice_minutes.max(0))));
    let latest = now.saturating_add(Duration::days(i64::from(rules.horizon_days.max(0))));
    let weekday = iso_weekday(day);

    let mut slots = Vec::new();
    let mut windows: Vec<&SiteBookingWindow> = rules
        .hours
        .iter()
        .filter(|window| window.weekday == weekday)
        .collect();
    windows.sort_by_key(|window| window.start_minute);
    for window in windows {
        let mut start_minute = i64::from(window.start_minute);
        while start_minute + duration <= i64::from(window.end_minute) {
            let this_start = start_minute;
            start_minute += step;
            let Some(starts_at) = local_instant(day, this_start, &zone) else {
                // The clocks went forward over this wall time: nobody can
                // arrive at it, so it is not offered.
                continue;
            };
            let Some(ends_at) = starts_at.checked_add(Duration::minutes(duration)) else {
                continue;
            };
            if starts_at < earliest || starts_at > latest {
                continue;
            }
            if busy
                .iter()
                .any(|interval| collides(starts_at, ends_at, interval, buffer))
            {
                continue;
            }
            slots.push(BookingSlot { starts_at, ends_at });
        }
    }
    slots.sort_by_key(|slot| slot.starts_at);
    slots.dedup_by_key(|slot| slot.starts_at);
    slots
}

/// Whether a candidate slot and a busy interval overlap once the service's
/// quiet gap is applied on both sides.
fn collides(
    starts_at: OffsetDateTime,
    ends_at: OffsetDateTime,
    interval: &BusyInterval,
    buffer: Duration,
) -> bool {
    let from = interval.from.saturating_sub(buffer);
    let to = interval.to.saturating_add(buffer);
    starts_at < to && from < ends_at
}

/// The local calendar day an instant falls on in `time_zone` — the inverse of
/// what the slot builder does, and the reason it lives here: one module owns
/// the conversion in both directions, so the day a slot is offered on and the
/// day a posted time is checked against can never disagree.
///
/// `None` when the zone cannot be resolved.
#[must_use]
pub fn local_day(instant: OffsetDateTime, time_zone: &str) -> Option<Date> {
    let (day, _) = local_wall_clock(instant, time_zone)?;
    Some(day)
}

/// The wall clock an instant shows in `time_zone`: the local day and its
/// `(hour, minute)`. What a visitor is told a time is.
///
/// `None` when the zone cannot be resolved.
#[must_use]
pub fn local_wall_clock(instant: OffsetDateTime, time_zone: &str) -> Option<(Date, (u8, u8))> {
    let zone = jiff::tz::TimeZone::get(time_zone).ok()?;
    let zoned = jiff::Timestamp::from_second(instant.unix_timestamp())
        .ok()?
        .to_zoned(zone);
    let civil = zoned.datetime();
    let day = Date::from_calendar_date(
        i32::from(civil.year()),
        time::Month::try_from(u8::try_from(civil.month()).ok()?).ok()?,
        u8::try_from(civil.day()).ok()?,
    )
    .ok()?;
    Some((
        day,
        (
            u8::try_from(civil.hour()).ok()?,
            u8::try_from(civil.minute()).ok()?,
        ),
    ))
}

/// The instant a wall-clock minute of `day` names in `zone`, or `None` when
/// that wall time does not exist there (a daylight-saving gap). A wall time
/// that happens twice resolves to its first occurrence.
fn local_instant(
    day: Date,
    minute_of_day: i64,
    zone: &jiff::tz::TimeZone,
) -> Option<OffsetDateTime> {
    let hour = i8::try_from(minute_of_day / 60).ok()?;
    let minute = i8::try_from(minute_of_day % 60).ok()?;
    let civil = jiff::civil::datetime(
        i16::try_from(day.year()).ok()?,
        i8::try_from(u8::from(day.month())).ok()?,
        i8::try_from(day.day()).ok()?,
        hour,
        minute,
        0,
        0,
    );
    let zoned = civil.to_zoned(zone.clone()).ok()?;
    // `to_zoned` shifts a nonexistent wall time forward rather than refusing
    // it. Comparing what came back with what was asked for is how a gap is
    // told apart from an ordinary time — and it costs one comparison.
    if zoned.datetime() != civil {
        return None;
    }
    OffsetDateTime::from_unix_timestamp(zoned.timestamp().as_second()).ok()
}

/// ISO 8601 weekday of a date: 1 is Monday, 7 is Sunday — the numbering the
/// stored week uses.
fn iso_weekday(day: Date) -> i32 {
    i32::from(day.weekday().number_from_monday())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use time::Month;

    fn rules<'a>(hours: &'a [SiteBookingWindow], zone: &'a str) -> BookingRules<'a> {
        BookingRules {
            hours,
            time_zone: zone,
            duration_minutes: 30,
            buffer_minutes: 0,
            notice_minutes: 0,
            horizon_days: 60,
        }
    }

    fn window(weekday: i32, start_minute: i32, end_minute: i32) -> SiteBookingWindow {
        SiteBookingWindow {
            weekday,
            start_minute,
            end_minute,
        }
    }

    fn day(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).unwrap()
    }

    fn utc(year: i32, month: Month, d: u8, hour: u8, minute: u8) -> OffsetDateTime {
        day(year, month, d)
            .with_hms(hour, minute, 0)
            .unwrap()
            .assume_utc()
    }

    /// A Wednesday in Brussels, well before any daylight-saving edge.
    const WEDNESDAY: (i32, Month, u8) = (2026, Month::September, 16);

    fn wednesday() -> Date {
        day(WEDNESDAY.0, WEDNESDAY.1, WEDNESDAY.2)
    }

    fn long_before() -> OffsetDateTime {
        utc(2026, Month::September, 1, 8, 0)
    }

    #[test]
    fn a_window_is_cut_into_slots_of_the_appointment_length() {
        // Wednesday 09:00–11:00 local (CEST, UTC+2) at thirty minutes each.
        let hours = [window(3, 540, 660)];
        let slots = free_slots(
            &rules(&hours, "Europe/Brussels"),
            wednesday(),
            &[],
            long_before(),
        );
        assert_eq!(slots.len(), 4);
        assert_eq!(slots[0].starts_at, utc(2026, Month::September, 16, 7, 0));
        assert_eq!(slots[0].ends_at, utc(2026, Month::September, 16, 7, 30));
        assert_eq!(slots[3].starts_at, utc(2026, Month::September, 16, 8, 30));
        // The last slot ends exactly on the window's edge; nothing runs past it.
        assert_eq!(slots[3].ends_at, utc(2026, Month::September, 16, 9, 0));
    }

    #[test]
    fn a_day_the_owner_is_closed_offers_nothing_however_empty_the_calendar_is() {
        // Only Monday is open; the Wednesday asked about is not.
        let hours = [window(1, 540, 660)];
        assert!(
            free_slots(
                &rules(&hours, "Europe/Brussels"),
                wednesday(),
                &[],
                long_before(),
            )
            .is_empty()
        );
    }

    #[test]
    fn a_busy_interval_removes_exactly_the_slots_it_overlaps() {
        let hours = [window(3, 540, 660)];
        // 09:30–10:00 local is taken.
        let busy = [BusyInterval {
            from: utc(2026, Month::September, 16, 7, 30),
            to: utc(2026, Month::September, 16, 8, 0),
        }];
        let slots = free_slots(
            &rules(&hours, "Europe/Brussels"),
            wednesday(),
            &busy,
            long_before(),
        );
        let starts: Vec<OffsetDateTime> = slots.iter().map(|slot| slot.starts_at).collect();
        assert_eq!(
            starts,
            vec![
                utc(2026, Month::September, 16, 7, 0),
                utc(2026, Month::September, 16, 8, 0),
                utc(2026, Month::September, 16, 8, 30),
            ]
        );
    }

    #[test]
    fn an_appointment_touching_a_busy_edge_is_still_free_without_a_buffer() {
        let hours = [window(3, 540, 660)];
        // Exactly the second slot's span: the first and third still stand.
        let busy = [BusyInterval {
            from: utc(2026, Month::September, 16, 7, 30),
            to: utc(2026, Month::September, 16, 8, 0),
        }];
        let slots = free_slots(
            &rules(&hours, "Europe/Brussels"),
            wednesday(),
            &busy,
            long_before(),
        );
        assert!(
            slots
                .iter()
                .any(|slot| slot.ends_at == utc(2026, Month::September, 16, 7, 30))
        );
    }

    #[test]
    fn a_buffer_widens_both_the_gap_between_slots_and_the_reach_of_busy_time() {
        let hours = [window(3, 540, 660)];
        let mut with_buffer = rules(&hours, "Europe/Brussels");
        with_buffer.buffer_minutes = 15;
        let slots = free_slots(&with_buffer, wednesday(), &[], long_before());
        // 09:00, 09:45, 10:30 — a quarter of an hour of quiet after each, and
        // a fourth would end after the window.
        assert_eq!(
            slots.iter().map(|s| s.starts_at).collect::<Vec<_>>(),
            vec![
                utc(2026, Month::September, 16, 7, 0),
                utc(2026, Month::September, 16, 7, 45),
                utc(2026, Month::September, 16, 8, 30),
            ]
        );
        // A meeting at 10:00–10:15 local costs the 09:45 slot, which would
        // otherwise have ended just as it started. The 10:30 one survives: the
        // meeting's own quarter of quiet ends exactly as it begins, and the
        // boundary is half-open on both sides — the gap is kept, not doubled.
        let busy = [BusyInterval {
            from: utc(2026, Month::September, 16, 8, 0),
            to: utc(2026, Month::September, 16, 8, 15),
        }];
        let after = free_slots(&with_buffer, wednesday(), &busy, long_before());
        assert_eq!(
            after.iter().map(|s| s.starts_at).collect::<Vec<_>>(),
            vec![
                utc(2026, Month::September, 16, 7, 0),
                utc(2026, Month::September, 16, 8, 30),
            ]
        );
    }

    #[test]
    fn the_notice_hides_everything_too_soon_and_the_horizon_everything_too_far() {
        let hours = [window(3, 540, 660)];
        let mut asks_notice = rules(&hours, "Europe/Brussels");
        asks_notice.notice_minutes = 24 * 60;
        // Asking on the Tuesday morning, a day's notice removes the whole
        // Wednesday morning bar the last slot.
        let asked_at = utc(2026, Month::September, 15, 8, 15);
        let slots = free_slots(&asks_notice, wednesday(), &[], asked_at);
        assert_eq!(
            slots.iter().map(|s| s.starts_at).collect::<Vec<_>>(),
            vec![utc(2026, Month::September, 16, 8, 30)]
        );

        let mut near_horizon = rules(&hours, "Europe/Brussels");
        near_horizon.horizon_days = 7;
        assert!(
            free_slots(
                &near_horizon,
                wednesday(),
                &[],
                utc(2026, Month::September, 1, 8, 0)
            )
            .is_empty(),
            "a day past the horizon offers nothing"
        );
    }

    #[test]
    fn the_hour_that_never_happens_is_not_offered_and_the_rest_of_that_day_is() {
        // 29 March 2026, Brussels: 02:00 jumps to 03:00. A service open
        // 01:00–05:00 offers 01:00, 01:30, then 03:00 onwards.
        let hours = [window(7, 60, 300)];
        let spring_forward = day(2026, Month::March, 29);
        let slots = free_slots(
            &rules(&hours, "Europe/Brussels"),
            spring_forward,
            &[],
            utc(2026, Month::March, 1, 0, 0),
        );
        let local: Vec<String> = slots
            .iter()
            .map(|slot| {
                jiff::Timestamp::from_second(slot.starts_at.unix_timestamp())
                    .unwrap()
                    .to_zoned(jiff::tz::TimeZone::get("Europe/Brussels").unwrap())
                    .strftime("%H:%M")
                    .to_string()
            })
            .collect();
        assert_eq!(
            local,
            vec!["01:00", "01:30", "03:00", "03:30", "04:00", "04:30"]
        );
    }

    #[test]
    fn the_hour_that_happens_twice_is_offered_once() {
        // 25 October 2026, Brussels: 03:00 falls back to 02:00. The 02:00 and
        // 02:30 wall times exist twice; each is offered a single time.
        let hours = [window(7, 60, 300)];
        let fall_back = day(2026, Month::October, 25);
        let slots = free_slots(
            &rules(&hours, "Europe/Brussels"),
            fall_back,
            &[],
            utc(2026, Month::October, 1, 0, 0),
        );
        let starts: Vec<OffsetDateTime> = slots.iter().map(|slot| slot.starts_at).collect();
        let mut unique = starts.clone();
        unique.dedup();
        assert_eq!(starts, unique);
        // 01:00 CEST (23:00 UTC the day before) through 04:30 CET: eight wall
        // times, none repeated.
        assert_eq!(starts.len(), 8);
    }

    #[test]
    fn two_windows_on_one_day_are_both_offered_in_order() {
        let hours = [window(3, 780, 900), window(3, 540, 660)];
        let slots = free_slots(
            &rules(&hours, "Europe/Brussels"),
            wednesday(),
            &[],
            long_before(),
        );
        assert_eq!(slots.len(), 8);
        assert!(
            slots
                .windows(2)
                .all(|pair| pair[0].starts_at < pair[1].starts_at)
        );
        assert_eq!(slots[4].starts_at, utc(2026, Month::September, 16, 11, 0));
    }
}
