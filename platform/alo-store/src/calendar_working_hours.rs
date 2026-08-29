//! A person's working schedule — days, hours and zone — and the expansion
//! that turns it into *outside-hours* spans over a window (the second span
//! kind Agenda's scheduling speaks beside "busy").
//!
//! The schedule is wall-clock on purpose: "Mon–Fri 09:00–17:00 in
//! Europe/Brussels" must survive a DST switch as 09:00–17:00 local, which a
//! pair of stored instants cannot. Conversion to UTC happens here, per day,
//! through the calendar's one zone seam ([`crate::tz`]), so the free/busy
//! wire and the store never disagree about where 09:00 falls.
//!
//! No row means the default — Mon–Fri 09:00–17:00 in the person's own zone —
//! so schedules work before anyone opens a settings screen.

use time::{Duration, OffsetDateTime, Time, Weekday};

use crate::account::AccountStore;
use crate::calendar_availability::CalendarBusySpan;
use crate::error::{Result, StoreError};
use crate::tz;

/// One person's working schedule: which weekdays, the daily window, and the
/// zone whose wall-clock the window follows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkingHours {
    /// Working weekdays as a bitmask, bit 0 = Monday … bit 6 = Sunday.
    /// Zero is valid: a person with no working days is outside hours always.
    pub days: u8,
    /// The window's start, in minutes after local midnight (`0..=1439`).
    pub start_minute: u16,
    /// The window's exclusive end, in minutes after local midnight
    /// (`1..=1440`, strictly after `start_minute`).
    pub end_minute: u16,
    /// The IANA zone the window's wall-clock follows, or `None` for the
    /// person's own profile zone (falling back to UTC when that is unknown).
    pub zone: Option<String>,
}

/// Mon–Fri as a [`WorkingHours::days`] bitmask.
const WEEKDAYS_MASK: u8 = 0b0001_1111;
/// Every representable day bit set.
const ALL_DAYS_MASK: u8 = 0b0111_1111;

impl Default for WorkingHours {
    /// The schedule everyone has until they say otherwise: Mon–Fri
    /// 09:00–17:00 in their own zone.
    fn default() -> Self {
        Self {
            days: WEEKDAYS_MASK,
            start_minute: 9 * 60,
            end_minute: 17 * 60,
            zone: None,
        }
    }
}

impl WorkingHours {
    /// Whether `day` is a working day under this schedule.
    #[must_use]
    pub fn works_on(&self, day: Weekday) -> bool {
        self.days & (1 << day.number_days_from_monday()) != 0
    }

    /// Checks the schedule holds its own invariants.
    ///
    /// # Errors
    /// [`StoreError::Validation`] naming the field that is out of range: a
    /// day bit above Sunday, a window that ends at or before its start or
    /// runs past 24:00, or a zone that is not an IANA name.
    pub fn validate(&self) -> Result<()> {
        if self.days > ALL_DAYS_MASK {
            return Err(StoreError::Validation(
                "working days must be the seven weekday bits, Monday first".to_owned(),
            ));
        }
        if self.start_minute >= self.end_minute || self.end_minute > 1440 {
            return Err(StoreError::Validation(
                "working hours must end after they start, within one day".to_owned(),
            ));
        }
        if let Some(zone) = self.zone.as_deref()
            && !tz::known(zone)
        {
            return Err(StoreError::Validation(format!(
                "unknown time zone {zone:?} (an IANA name like \"Europe/Brussels\")"
            )));
        }
        Ok(())
    }
}

/// The spans of `[from, to)` that fall **outside** `hours` — nights, weekends
/// and non-working days, merged and earliest first. The complement of the
/// schedule's working windows, computed day by day in the schedule's zone
/// (`hours.zone`, else `fallback_zone`, else UTC), so a DST switch moves the
/// UTC window and never the local one. Same span currency as
/// [`merged_busy_spans`](crate::merged_busy_spans): free/busy serves both
/// kinds side by side and they never overlap-merge with each other.
#[must_use]
pub fn outside_hours_spans(
    hours: &WorkingHours,
    fallback_zone: Option<&str>,
    from: OffsetDateTime,
    to: OffsetDateTime,
) -> Vec<CalendarBusySpan> {
    if to <= from {
        return Vec::new();
    }
    let zone = hours
        .zone
        .as_deref()
        .or(fallback_zone)
        .and_then(tz::zone)
        .unwrap_or(jiff::tz::TimeZone::UTC);

    // Every working window intersecting [from, to) starts on a local date
    // between the local dates of the window's ends (a daily window never
    // crosses local midnight), so walking those dates covers them all.
    let mut working: Vec<(OffsetDateTime, OffsetDateTime)> = Vec::new();
    let mut date = tz::utc_to_wall(from, &zone).date();
    let last = tz::utc_to_wall(to, &zone).date();
    while date <= last {
        if hours.works_on(date.weekday()) {
            let start_wall = OffsetDateTime::new_utc(date, Time::MIDNIGHT)
                + Duration::minutes(i64::from(hours.start_minute));
            let end_wall = OffsetDateTime::new_utc(date, Time::MIDNIGHT)
                + Duration::minutes(i64::from(hours.end_minute));
            if let (Some(start), Some(end)) = (
                tz::wall_to_utc(start_wall, &zone),
                tz::wall_to_utc(end_wall, &zone),
            ) && end > start
            {
                working.push((start, end));
            }
        }
        match date.next_day() {
            Some(next) => date = next,
            None => break,
        }
    }

    // Complement within [from, to). The windows are chronological (one per
    // ascending local day) and never overlap, so one forward cursor suffices.
    let mut outside = Vec::new();
    let mut cursor = from;
    for (start, end) in working {
        if start >= to {
            break;
        }
        if start > cursor {
            outside.push(CalendarBusySpan {
                from: cursor,
                to: start,
            });
        }
        cursor = cursor.max(end);
        if cursor >= to {
            break;
        }
    }
    if cursor < to {
        outside.push(CalendarBusySpan { from: cursor, to });
    }
    outside
}

impl AccountStore {
    /// This person's working schedule, or the default (Mon–Fri 09:00–17:00 in
    /// their own zone) when they have never set one.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn working_hours(&self) -> Result<WorkingHours> {
        let row = sqlx::query_as::<_, (i16, i16, i16, Option<String>)>(
            "SELECT days, start_minute, end_minute, zone FROM calendar_working_hours \
             WHERE tenant_id = $1 AND user_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .fetch_optional(&self.pool)
        .await?;
        Ok(match row {
            Some((days, start, end, zone)) => WorkingHours {
                // The database CHECKs pin the ranges; a row that still fails
                // the cast is data corruption, answered as the default rather
                // than a panic.
                days: u8::try_from(days).unwrap_or(WEEKDAYS_MASK),
                start_minute: u16::try_from(start).unwrap_or(9 * 60),
                end_minute: u16::try_from(end).unwrap_or(17 * 60),
                zone,
            },
            None => WorkingHours::default(),
        })
    }

    /// Sets this person's working schedule. Upsert; `updated_at` is bumped.
    ///
    /// # Errors
    /// [`StoreError::Validation`] per [`WorkingHours::validate`];
    /// [`StoreError::Db`] on failure.
    pub async fn set_working_hours(&self, hours: &WorkingHours) -> Result<()> {
        hours.validate()?;
        sqlx::query(
            "INSERT INTO calendar_working_hours \
             (tenant_id, user_id, days, start_minute, end_minute, zone) \
             VALUES ($1, $2, $3, $4, $5, $6) \
             ON CONFLICT (tenant_id, user_id) DO UPDATE \
             SET days = $3, start_minute = $4, end_minute = $5, zone = $6, \
                 updated_at = now()",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(i16::from(hours.days))
        .bind(i16::try_from(hours.start_minute).unwrap_or(9 * 60))
        .bind(i16::try_from(hours.end_minute).unwrap_or(17 * 60))
        .bind(hours.zone.as_deref())
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use time::{Date, Month};

    fn odt(y: i32, mo: u8, d: u8, h: u8, mi: u8) -> OffsetDateTime {
        OffsetDateTime::new_utc(
            Date::from_calendar_date(y, Month::try_from(mo).unwrap(), d).unwrap(),
            Time::from_hms(h, mi, 0).unwrap(),
        )
    }

    fn span(from: OffsetDateTime, to: OffsetDateTime) -> CalendarBusySpan {
        CalendarBusySpan { from, to }
    }

    #[test]
    fn the_default_is_mon_fri_nine_to_five() {
        let hours = WorkingHours::default();
        assert!(hours.works_on(Weekday::Monday));
        assert!(hours.works_on(Weekday::Friday));
        assert!(!hours.works_on(Weekday::Saturday));
        assert!(!hours.works_on(Weekday::Sunday));
        assert_eq!(hours.start_minute, 540);
        assert_eq!(hours.end_minute, 1020);
        assert_eq!(hours.zone, None);
        hours.validate().unwrap();
    }

    #[test]
    fn a_working_day_in_utc_leaves_night_and_evening_outside() {
        // Tue 2026-09-01, whole day, no zone anywhere → UTC.
        let (from, to) = (odt(2026, 9, 1, 0, 0), odt(2026, 9, 2, 0, 0));
        let outside = outside_hours_spans(&WorkingHours::default(), None, from, to);
        assert_eq!(
            outside,
            vec![
                span(from, odt(2026, 9, 1, 9, 0)),
                span(odt(2026, 9, 1, 17, 0), to),
            ]
        );
    }

    #[test]
    fn a_weekend_day_is_outside_end_to_end() {
        // Sat 2026-09-05.
        let (from, to) = (odt(2026, 9, 5, 0, 0), odt(2026, 9, 6, 0, 0));
        let outside = outside_hours_spans(&WorkingHours::default(), None, from, to);
        assert_eq!(outside, vec![span(from, to)]);
    }

    #[test]
    fn a_window_inside_working_hours_has_nothing_outside() {
        let (from, to) = (odt(2026, 9, 1, 10, 0), odt(2026, 9, 1, 12, 0));
        assert!(outside_hours_spans(&WorkingHours::default(), None, from, to).is_empty());
    }

    #[test]
    fn the_zone_moves_the_window_and_dst_moves_it_again() {
        let hours = WorkingHours {
            zone: Some("Europe/Brussels".to_owned()),
            ..WorkingHours::default()
        };
        // Mon 2026-10-19 (CEST, UTC+2): 09:00–17:00 local is 07:00–15:00Z.
        let (from, to) = (odt(2026, 10, 19, 0, 0), odt(2026, 10, 20, 0, 0));
        assert_eq!(
            outside_hours_spans(&hours, None, from, to),
            vec![
                span(from, odt(2026, 10, 19, 7, 0)),
                span(odt(2026, 10, 19, 15, 0), to),
            ]
        );
        // Mon 2026-10-26, after the 10-25 switch (CET, UTC+1): 08:00–16:00Z.
        let (from, to) = (odt(2026, 10, 26, 0, 0), odt(2026, 10, 27, 0, 0));
        assert_eq!(
            outside_hours_spans(&hours, None, from, to),
            vec![
                span(from, odt(2026, 10, 26, 8, 0)),
                span(odt(2026, 10, 26, 16, 0), to),
            ]
        );
    }

    #[test]
    fn the_fallback_zone_speaks_when_the_schedule_has_none() {
        // Same Monday, schedule zone unset, person's profile says Brussels.
        let (from, to) = (odt(2026, 10, 19, 0, 0), odt(2026, 10, 20, 0, 0));
        let outside =
            outside_hours_spans(&WorkingHours::default(), Some("Europe/Brussels"), from, to);
        assert_eq!(outside[0].to, odt(2026, 10, 19, 7, 0));
        // An unknown fallback falls back once more, to UTC.
        let outside = outside_hours_spans(&WorkingHours::default(), Some("Not/AZone"), from, to);
        assert_eq!(outside[0].to, odt(2026, 10, 19, 9, 0));
    }

    #[test]
    fn a_full_day_schedule_marks_only_the_off_days() {
        // 00:00–24:00 every day but Sunday: Sat 24:00 → Mon 00:00 via the
        // end_minute == 1440 edge (24:00 is the next local midnight).
        let hours = WorkingHours {
            days: 0b0111_1111 & !(1 << 6),
            start_minute: 0,
            end_minute: 1440,
            zone: None,
        };
        // Sat 2026-09-05 .. Tue 2026-09-08.
        let (from, to) = (odt(2026, 9, 5, 0, 0), odt(2026, 9, 8, 0, 0));
        assert_eq!(
            outside_hours_spans(&hours, None, from, to),
            vec![span(odt(2026, 9, 6, 0, 0), odt(2026, 9, 7, 0, 0))]
        );
    }

    #[test]
    fn no_working_days_means_the_whole_window_is_outside() {
        let hours = WorkingHours {
            days: 0,
            ..WorkingHours::default()
        };
        let (from, to) = (odt(2026, 9, 1, 0, 0), odt(2026, 9, 8, 0, 0));
        assert_eq!(
            outside_hours_spans(&hours, None, from, to),
            vec![span(from, to)]
        );
    }

    #[test]
    fn an_empty_or_backwards_window_is_empty() {
        let at = odt(2026, 9, 1, 12, 0);
        assert!(outside_hours_spans(&WorkingHours::default(), None, at, at).is_empty());
        assert!(
            outside_hours_spans(&WorkingHours::default(), None, at, at - Duration::hours(1))
                .is_empty()
        );
    }

    #[test]
    fn validation_names_each_broken_invariant() {
        let bad_days = WorkingHours {
            days: 0b1000_0000,
            ..WorkingHours::default()
        };
        assert!(matches!(
            bad_days.validate(),
            Err(StoreError::Validation(msg)) if msg.contains("days")
        ));
        let backwards = WorkingHours {
            start_minute: 600,
            end_minute: 540,
            ..WorkingHours::default()
        };
        assert!(matches!(
            backwards.validate(),
            Err(StoreError::Validation(msg)) if msg.contains("end after")
        ));
        let past_midnight = WorkingHours {
            end_minute: 1441,
            ..WorkingHours::default()
        };
        assert!(past_midnight.validate().is_err());
        let bad_zone = WorkingHours {
            zone: Some("Romance Standard Time".to_owned()),
            ..WorkingHours::default()
        };
        assert!(matches!(
            bad_zone.validate(),
            Err(StoreError::Validation(msg)) if msg.contains("Romance Standard Time")
        ));
    }
}
