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

/// One `STANDARD`/`DAYLIGHT` observance of a zone (RFC 5545 §3.6.5): when its
/// rule took effect, the UTC offsets either side of that onset, and the zone's
/// abbreviation and DST flag while the rule holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Observance {
    /// The UTC instant the rule took effect — `None` when the zone has no
    /// transition on record before the queried span (a fixed-offset zone),
    /// where the rule has always held.
    pub(crate) utc_onset: Option<OffsetDateTime>,
    /// The UTC offset in force immediately before the onset, in seconds
    /// (equal to `offset_to_secs` for a fixed-offset zone).
    pub(crate) offset_from_secs: i32,
    /// The UTC offset the rule switches to, in seconds.
    pub(crate) offset_to_secs: i32,
    /// The zone's abbreviation under this rule (`CEST`, `JST`, `+02`, …).
    pub(crate) abbreviation: String,
    /// Whether the rule is a daylight-saving observance.
    pub(crate) dst: bool,
}

/// The observances in force across `[from, to]`: the rule holding at `from`,
/// then one entry per transition up to and including `to` — the bounded set a
/// `VTIMEZONE` block needs, never the zone's whole history. Ordered by onset.
/// Empty only when `from` is outside jiff's representable range.
pub(crate) fn observances(
    zone: &jiff::tz::TimeZone,
    from: OffsetDateTime,
    to: OffsetDateTime,
) -> Vec<Observance> {
    // Query one second past `from`, so a transition exactly at the span's
    // start counts as the rule in force rather than being skipped between
    // the strictly-before and strictly-after iterators.
    let Ok(ts_from) = jiff::Timestamp::from_second(from.unix_timestamp().saturating_add(1)) else {
        return Vec::new();
    };
    let ts_to = jiff::Timestamp::from_second(to.unix_timestamp().max(from.unix_timestamp()))
        .unwrap_or(ts_from);
    let mut before = zone.preceding(ts_from);
    let mut out = Vec::new();
    match before.next() {
        Some(t) => {
            // The offset stepped from is the previous rule's; a zone whose
            // history starts here keeps its own (a no-op "from").
            let prior = before
                .next()
                .map_or_else(|| t.offset().seconds(), |p| p.offset().seconds());
            out.push(Observance {
                utc_onset: OffsetDateTime::from_unix_timestamp(t.timestamp().as_second()).ok(),
                offset_from_secs: prior,
                offset_to_secs: t.offset().seconds(),
                abbreviation: t.abbreviation().to_owned(),
                dst: t.dst().is_dst(),
            });
        }
        None => {
            // No transition ever before the span: a fixed-offset zone.
            let info = zone.to_offset_info(ts_from);
            out.push(Observance {
                utc_onset: None,
                offset_from_secs: info.offset().seconds(),
                offset_to_secs: info.offset().seconds(),
                abbreviation: info.abbreviation().to_owned(),
                dst: info.dst().is_dst(),
            });
        }
    }
    let mut prev_offset = out[0].offset_to_secs;
    for t in zone.following(ts_from) {
        if t.timestamp() > ts_to {
            break;
        }
        out.push(Observance {
            utc_onset: OffsetDateTime::from_unix_timestamp(t.timestamp().as_second()).ok(),
            offset_from_secs: prev_offset,
            offset_to_secs: t.offset().seconds(),
            abbreviation: t.abbreviation().to_owned(),
            dst: t.dst().is_dst(),
        });
        prev_offset = t.offset().seconds();
    }
    out
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
    fn observances_cover_the_span_and_nothing_more() {
        let z = zone("Europe/Brussels").unwrap();
        // A span inside one rule: just the rule in force (CEST, entered from
        // CET at the 2026-03-29 01:00Z switch).
        let summer = observances(&z, odt(2026, 6, 1, 0, 0), odt(2026, 6, 30, 0, 0));
        assert_eq!(summer.len(), 1);
        assert_eq!(summer[0].utc_onset, Some(odt(2026, 3, 29, 1, 0)));
        assert_eq!(summer[0].offset_from_secs, 3600);
        assert_eq!(summer[0].offset_to_secs, 7200);
        assert_eq!(summer[0].abbreviation, "CEST");
        assert!(summer[0].dst);
        // A span crossing the 2026-10-25 end of DST adds the CET rule.
        let cross = observances(&z, odt(2026, 10, 19, 7, 0), odt(2026, 11, 2, 8, 0));
        assert_eq!(cross.len(), 2);
        assert_eq!(cross[1].utc_onset, Some(odt(2026, 10, 25, 1, 0)));
        assert_eq!(cross[1].offset_from_secs, 7200);
        assert_eq!(cross[1].offset_to_secs, 3600);
        assert_eq!(cross[1].abbreviation, "CET");
        assert!(!cross[1].dst);
    }

    #[test]
    fn fixed_offset_zone_is_one_observance_since_forever() {
        // Etc/GMT-2 (UTC+2, POSIX sign inversion) has no transitions at all.
        let z = zone("Etc/GMT-2").unwrap();
        let obs = observances(&z, odt(2026, 6, 1, 0, 0), odt(2026, 6, 30, 0, 0));
        assert_eq!(obs.len(), 1);
        assert_eq!(obs[0].utc_onset, None);
        assert_eq!(obs[0].offset_from_secs, 7200);
        assert_eq!(obs[0].offset_to_secs, 7200);
        assert!(!obs[0].dst);
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
