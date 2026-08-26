//! IANA time-zone lookups and wall-clock ↔ UTC conversion for the calendar
//! (`jiff` owns the tz database). Events store UTC instants; a recurring
//! series that follows a zone's wall-clock ([`crate::model::CalendarEvent::timezone`])
//! converts through here at expansion and serialization time, so a DST change
//! moves the UTC instant and never the local time.

use time::{Date, Month, OffsetDateTime, Time};

/// Whether `name` resolves in the IANA database (e.g. `Europe/Brussels`).
/// Windows display names ("Romance Standard Time") do not.
pub fn known(name: &str) -> bool {
    zone(name).is_some()
}

/// The named IANA zone, or `None` when it is unknown.
pub(crate) fn zone(name: &str) -> Option<jiff::tz::TimeZone> {
    jiff::tz::TimeZone::get(name).ok()
}

/// The wall-clock a UTC instant reads as in `zone`, re-labelled UTC — a civil
/// (local) date-time carried in the crate's `OffsetDateTime` currency, so the
/// recurrence period math can run on wall-clock values with plain arithmetic.
pub(crate) fn utc_to_wall(t: OffsetDateTime, zone: &jiff::tz::TimeZone) -> OffsetDateTime {
    let Ok(ts) = jiff::Timestamp::from_second(t.unix_timestamp()) else {
        return t;
    };
    let civil = zone.to_datetime(ts);
    civil_to_odt(&civil).unwrap_or(t)
}

/// The UTC instant whose wall-clock in `zone` is `wall` (a UTC-labelled civil
/// time from [`utc_to_wall`] or plain arithmetic on one). Disambiguation is
/// jiff's compatible mode: a time inside a DST gap moves forward, a repeated
/// (fold) time takes the earlier instant. `None` when the conversion fails.
pub(crate) fn wall_to_utc(
    wall: OffsetDateTime,
    zone: &jiff::tz::TimeZone,
) -> Option<OffsetDateTime> {
    let civil = jiff::civil::datetime(
        i16::try_from(wall.year()).ok()?,
        wall.month() as u8 as i8,
        wall.day() as i8,
        wall.hour() as i8,
        wall.minute() as i8,
        wall.second() as i8,
        0,
    );
    let zoned = civil.to_zoned(zone.clone()).ok()?;
    OffsetDateTime::from_unix_timestamp(zoned.timestamp().as_second()).ok()
}

/// A jiff civil date-time as a UTC-labelled `OffsetDateTime`.
fn civil_to_odt(dt: &jiff::civil::DateTime) -> Option<OffsetDateTime> {
    let date = Date::from_calendar_date(
        i32::from(dt.year()),
        Month::try_from(dt.month() as u8).ok()?,
        dt.day() as u8,
    )
    .ok()?;
    let time = Time::from_hms(dt.hour() as u8, dt.minute() as u8, dt.second() as u8).ok()?;
    Some(OffsetDateTime::new_utc(date, time))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn odt(y: i32, mo: u8, d: u8, h: u8, mi: u8) -> OffsetDateTime {
        OffsetDateTime::new_utc(
            Date::from_calendar_date(y, Month::try_from(mo).unwrap(), d).unwrap(),
            Time::from_hms(h, mi, 0).unwrap(),
        )
    }

    #[test]
    fn known_resolves_iana_only() {
        assert!(known("Europe/Brussels"));
        assert!(known("America/New_York"));
        assert!(!known("Romance Standard Time"));
        assert!(!known(""));
    }

    #[test]
    fn wall_round_trips_across_dst() {
        let z = zone("Europe/Brussels").unwrap();
        // CEST (UTC+2): 07:00Z reads as 09:00 local.
        assert_eq!(
            utc_to_wall(odt(2026, 10, 19, 7, 0), &z),
            odt(2026, 10, 19, 9, 0)
        );
        // CET (UTC+1) after the 2026-10-25 switch: 09:00 local is 08:00Z.
        assert_eq!(
            wall_to_utc(odt(2026, 10, 26, 9, 0), &z),
            Some(odt(2026, 10, 26, 8, 0))
        );
        // Round trip either side of the switch is the identity.
        for t in [odt(2026, 10, 19, 7, 0), odt(2026, 10, 26, 8, 0)] {
            assert_eq!(wall_to_utc(utc_to_wall(t, &z), &z), Some(t));
        }
    }

    #[test]
    fn dst_gap_time_moves_forward() {
        // 02:30 local on 2026-03-29 does not exist in Brussels (clocks jump
        // 02:00 → 03:00); compatible disambiguation lands after the gap.
        let z = zone("Europe/Brussels").unwrap();
        let resolved = wall_to_utc(odt(2026, 3, 29, 2, 30), &z).unwrap();
        assert_eq!(resolved, odt(2026, 3, 29, 1, 30));
    }
}
