//! Minimal iCalendar (RFC 5545) serialization for the calendar's `VEVENT`s —
//! the calendar sibling of [`crate::vcard`]. Slice-2 scope: `UID`, `SUMMARY`,
//! `DESCRIPTION`, `LOCATION`, `DTSTART`/`DTEND` as UTC (`…Z`) or all-day
//! (`VALUE=DATE`). A `TZID`-qualified or floating time is read as UTC — a
//! documented cut (`docs/interop.md`); clients that write UTC round-trip
//! exactly. Text values are escaped/unescaped per §3.3.11 and long lines are
//! folded at 75 octets on write and unfolded on read.

use time::{Date, Month, OffsetDateTime, Time, UtcOffset};

use crate::id::EventId;
use crate::model::CalendarEvent;

const PRODID: &str = "-//alo//calendar//EN";

/// Serialize an event as a complete single-`VEVENT` `VCALENDAR` document.
pub fn to_ics(event: &CalendarEvent) -> String {
    let mut lines = vec![
        "BEGIN:VCALENDAR".to_owned(),
        "VERSION:2.0".to_owned(),
        format!("PRODID:{PRODID}"),
        "BEGIN:VEVENT".to_owned(),
        format!("UID:{}", event.id.as_str()),
        format!("DTSTAMP:{}", fmt_utc(OffsetDateTime::now_utc())),
    ];
    if event.all_day {
        lines.push(format!("DTSTART;VALUE=DATE:{}", fmt_date(event.starts_at)));
        lines.push(format!("DTEND;VALUE=DATE:{}", fmt_date(event.ends_at)));
    } else {
        lines.push(format!("DTSTART:{}", fmt_utc(event.starts_at)));
        lines.push(format!("DTEND:{}", fmt_utc(event.ends_at)));
    }
    if let Some(rrule) = &event.recurrence {
        lines.push(format!("RRULE:{rrule}"));
    }
    lines.push(format!("SUMMARY:{}", escape(&event.summary)));
    if let Some(loc) = &event.location {
        lines.push(format!("LOCATION:{}", escape(loc)));
    }
    if let Some(desc) = &event.description {
        lines.push(format!("DESCRIPTION:{}", escape(desc)));
    }
    lines.push("END:VEVENT".to_owned());
    lines.push("END:VCALENDAR".to_owned());
    lines.iter().map(|l| fold(l)).collect::<Vec<_>>().join("\r\n") + "\r\n"
}

/// Parse the first `VEVENT` in an iCalendar document into an event, using
/// `fallback_id` as the `UID` if the document omits one.
pub fn from_ics(text: &str, fallback_id: &str) -> Option<CalendarEvent> {
    let unfolded = unfold(text);
    let mut in_event = false;
    let mut uid: Option<String> = None;
    let mut summary = String::new();
    let mut description: Option<String> = None;
    let mut location: Option<String> = None;
    let mut start: Option<(OffsetDateTime, bool)> = None;
    let mut end: Option<(OffsetDateTime, bool)> = None;
    let mut recurrence: Option<String> = None;

    for line in unfolded.lines() {
        let upper = line.to_ascii_uppercase();
        if upper == "BEGIN:VEVENT" {
            in_event = true;
            continue;
        }
        if upper == "END:VEVENT" {
            break;
        }
        if !in_event {
            continue;
        }
        let Some((spec, value)) = line.split_once(':') else {
            continue;
        };
        let mut parts = spec.split(';');
        let name = parts.next().unwrap_or("").to_ascii_uppercase();
        let is_date = parts.any(|p| p.eq_ignore_ascii_case("VALUE=DATE"));
        match name.as_str() {
            "UID" => uid = Some(value.trim().to_owned()),
            "SUMMARY" => summary = unescape(value),
            "DESCRIPTION" => description = Some(unescape(value)),
            "LOCATION" => location = Some(unescape(value)),
            "DTSTART" => start = parse_dt(value.trim(), is_date),
            "DTEND" => end = parse_dt(value.trim(), is_date),
            "RRULE" => recurrence = Some(value.trim().to_owned()),
            _ => {}
        }
    }

    let (starts_at, all_day) = start?;
    let (ends_at, _) = end.unwrap_or((starts_at, all_day));
    if summary.trim().is_empty() {
        summary = "(no title)".to_owned();
    }
    Some(CalendarEvent {
        id: EventId::new(uid.unwrap_or_else(|| fallback_id.to_owned())),
        summary,
        description: description.filter(|s| !s.is_empty()),
        location: location.filter(|s| !s.is_empty()),
        starts_at,
        ends_at,
        all_day,
        recurrence,
    })
}

fn fmt_utc(t: OffsetDateTime) -> String {
    let t = t.to_offset(UtcOffset::UTC);
    format!(
        "{:04}{:02}{:02}T{:02}{:02}{:02}Z",
        t.year(),
        t.month() as u8,
        t.day(),
        t.hour(),
        t.minute(),
        t.second()
    )
}

fn fmt_date(t: OffsetDateTime) -> String {
    let t = t.to_offset(UtcOffset::UTC);
    format!("{:04}{:02}{:02}", t.year(), t.month() as u8, t.day())
}

/// Parse an iCalendar date-time from its digits. `Z`/naive → UTC;
/// `VALUE=DATE` (or a bare `YYYYMMDD`) → midnight UTC, all-day.
fn parse_dt(value: &str, is_date: bool) -> Option<(OffsetDateTime, bool)> {
    let digits = value.trim_end_matches('Z');
    let year: i32 = digits.get(0..4)?.parse().ok()?;
    let month: u8 = digits.get(4..6)?.parse().ok()?;
    let day: u8 = digits.get(6..8)?.parse().ok()?;
    let date = Date::from_calendar_date(year, Month::try_from(month).ok()?, day).ok()?;
    if is_date || !digits.contains('T') {
        return Some((OffsetDateTime::new_utc(date, Time::MIDNIGHT), true));
    }
    let t = &digits[9..]; // after the 'T' at index 8
    let hour: u8 = t.get(0..2)?.parse().ok()?;
    let minute: u8 = t.get(2..4)?.parse().ok()?;
    let second: u8 = t.get(4..6).and_then(|s| s.parse().ok()).unwrap_or(0);
    let time = Time::from_hms(hour, minute, second).ok()?;
    Some((OffsetDateTime::new_utc(date, time), false))
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace(';', "\\;")
        .replace(',', "\\,")
        .replace('\n', "\\n")
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') | Some('N') => out.push('\n'),
                Some(other) => out.push(other),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Fold a content line at 75 octets (continuations begin with a space).
fn fold(line: &str) -> String {
    if line.len() <= 75 {
        return line.to_owned();
    }
    let bytes = line.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    let mut limit = 75;
    while i < bytes.len() {
        // Don't split a UTF-8 sequence: back off to a char boundary.
        let mut end = limit.min(bytes.len());
        while end < bytes.len() && (bytes[end] & 0xC0) == 0x80 {
            end -= 1;
        }
        if i > 0 {
            out.push_str("\r\n ");
        }
        out.push_str(&line[i..end]);
        i = end;
        limit = 74; // subsequent lines carry a leading space
    }
    out
}

/// Join folded lines (a line starting with space/tab continues the prior one).
fn unfold(text: &str) -> String {
    let mut out = String::new();
    for line in text.split("\r\n").flat_map(|l| l.split('\n')) {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if let Some(rest) = line.strip_prefix(' ').or_else(|| line.strip_prefix('\t')) {
            out.push_str(rest);
        } else {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(line);
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn timed_event_round_trips() {
        let e = CalendarEvent {
            id: EventId::new("abc123".to_owned()),
            summary: "Team sync; weekly".into(),
            description: Some("Line1\nLine2".into()),
            location: Some("Room A".into()),
            starts_at: OffsetDateTime::new_utc(
                Date::from_calendar_date(2026, time::Month::August, 15).unwrap(),
                Time::from_hms(9, 0, 0).unwrap(),
            ),
            ends_at: OffsetDateTime::new_utc(
                Date::from_calendar_date(2026, time::Month::August, 15).unwrap(),
                Time::from_hms(10, 30, 0).unwrap(),
            ),
            all_day: false,
            recurrence: Some("FREQ=WEEKLY".to_owned()),
        };
        let ics = to_ics(&e);
        assert!(ics.contains("DTSTART:20260815T090000Z"));
        assert!(ics.contains("DTEND:20260815T103000Z"));
        assert!(ics.contains("SUMMARY:Team sync\\; weekly"));
        assert!(ics.contains("RRULE:FREQ=WEEKLY"));
        let back = from_ics(&ics, "fallback").unwrap();
        assert_eq!(back.id.as_str(), "abc123");
        assert_eq!(back.summary, "Team sync; weekly");
        assert_eq!(back.description.as_deref(), Some("Line1\nLine2"));
        assert_eq!(back.location.as_deref(), Some("Room A"));
        assert_eq!(back.starts_at, e.starts_at);
        assert_eq!(back.ends_at, e.ends_at);
        assert_eq!(back.recurrence.as_deref(), Some("FREQ=WEEKLY"));
        assert!(!back.all_day);
    }

    #[test]
    fn all_day_uses_value_date() {
        let e = CalendarEvent {
            id: EventId::new("day1".to_owned()),
            summary: "Holiday".into(),
            description: None,
            location: None,
            starts_at: OffsetDateTime::new_utc(
                Date::from_calendar_date(2026, time::Month::December, 25).unwrap(),
                Time::MIDNIGHT,
            ),
            ends_at: OffsetDateTime::new_utc(
                Date::from_calendar_date(2026, time::Month::December, 26).unwrap(),
                Time::MIDNIGHT,
            ),
            all_day: true,
            recurrence: None,
        };
        let ics = to_ics(&e);
        assert!(ics.contains("DTSTART;VALUE=DATE:20261225"));
        let back = from_ics(&ics, "fallback").unwrap();
        assert!(back.all_day);
        assert_eq!(back.starts_at, e.starts_at);
    }

    #[test]
    fn parses_apple_style_uid_fallback() {
        let ics = "BEGIN:VEVENT\r\nSUMMARY:No uid\r\nDTSTART:20260101T120000Z\r\nEND:VEVENT";
        let back = from_ics(ics, "fb-id").unwrap();
        assert_eq!(back.id.as_str(), "fb-id");
        assert_eq!(back.summary, "No uid");
    }
}
