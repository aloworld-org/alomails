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
    ];
    lines.extend(vevent_lines(event, None));
    lines.push("END:VCALENDAR".to_owned());
    fold_join(&lines)
}

/// Serialize an iMIP scheduling message (`VCALENDAR` with a `METHOD` and an
/// `ORGANIZER`) for emailing an invitation/update/cancel. `method` is e.g.
/// `REQUEST`; `organizer` is the sender's email.
pub fn to_imip(event: &CalendarEvent, organizer: &str, method: &str) -> String {
    let mut lines = vec![
        "BEGIN:VCALENDAR".to_owned(),
        "VERSION:2.0".to_owned(),
        format!("PRODID:{PRODID}"),
        format!("METHOD:{method}"),
    ];
    lines.extend(vevent_lines(event, Some(organizer)));
    lines.push("END:VCALENDAR".to_owned());
    fold_join(&lines)
}

/// The `VEVENT` body shared by [`to_ics`] and [`to_imip`]; an `organizer`, when
/// given, adds the `ORGANIZER` property (present only in scheduling messages).
fn vevent_lines(event: &CalendarEvent, organizer: Option<&str>) -> Vec<String> {
    let mut lines = vec![
        "BEGIN:VEVENT".to_owned(),
        format!("UID:{}", event.id.as_str()),
        format!("DTSTAMP:{}", fmt_utc(OffsetDateTime::now_utc())),
    ];
    if let Some(org) = organizer {
        lines.push(format!("ORGANIZER:mailto:{org}"));
    }
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
    for a in &event.attendees {
        lines.push(format!(
            "ATTENDEE;ROLE=REQ-PARTICIPANT;RSVP=TRUE;PARTSTAT=NEEDS-ACTION:mailto:{a}"
        ));
    }
    lines.push("END:VEVENT".to_owned());
    lines
}

fn fold_join(lines: &[String]) -> String {
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
    let mut attendees: Vec<String> = Vec::new();

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
            "ATTENDEE" => {
                let addr = value
                    .trim()
                    .strip_prefix("mailto:")
                    .or_else(|| value.trim().strip_prefix("MAILTO:"))
                    .unwrap_or(value.trim());
                if addr.contains('@') {
                    attendees.push(addr.to_owned());
                }
            }
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
        attendees,
    })
}

/// The scheduling `METHOD` of an iCalendar message (`REQUEST`, `REPLY`,
/// `CANCEL`, …), uppercased — a `VCALENDAR`-level property, so it is read
/// outside any `VEVENT`. `None` for a plain calendar with no method. This is
/// what distinguishes an inbound invitation (`REQUEST`) from other traffic.
pub fn method_of(text: &str) -> Option<String> {
    let unfolded = unfold(text);
    let mut in_event = false;
    for line in unfolded.lines() {
        let upper = line.to_ascii_uppercase();
        if upper == "BEGIN:VEVENT" {
            in_event = true;
            continue;
        }
        if upper == "END:VEVENT" {
            in_event = false;
            continue;
        }
        if in_event {
            continue;
        }
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.eq_ignore_ascii_case("METHOD") {
            return Some(value.trim().to_ascii_uppercase());
        }
    }
    None
}

/// The `UID` of the first `VEVENT`, if present — the stable key that ties a
/// CANCEL/REPLY back to the original event. Read on its own (not via
/// [`from_ics`]) so a minimal CANCEL that omits `DTSTART` still identifies what
/// to remove.
pub fn uid_of(text: &str) -> Option<String> {
    let unfolded = unfold(text);
    let mut in_event = false;
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
        if spec.eq_ignore_ascii_case("UID") && !value.trim().is_empty() {
            return Some(value.trim().to_owned());
        }
    }
    None
}

/// The `ORGANIZER` address of the first `VEVENT` (any `mailto:` prefix and
/// parameters stripped), if present. A REPLY must be addressed here, and the
/// stored event does not keep it — it is read from the inbound invitation.
pub fn organizer_of(text: &str) -> Option<String> {
    let unfolded = unfold(text);
    let mut in_event = false;
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
        let name = spec.split(';').next().unwrap_or("");
        if name.eq_ignore_ascii_case("ORGANIZER") {
            let v = value.trim();
            let addr = v
                .strip_prefix("mailto:")
                .or_else(|| v.strip_prefix("MAILTO:"))
                .unwrap_or(v);
            if addr.contains('@') {
                return Some(addr.to_owned());
            }
        }
    }
    None
}

/// Build an iMIP `REPLY`: a `VCALENDAR` carrying one attendee's participation
/// status back to the organizer. `partstat` is `ACCEPTED`, `DECLINED`, or
/// `TENTATIVE`. A REPLY speaks for a single attendee, so only `attendee` is
/// listed — with the original `UID` and `ORGANIZER` so the organizer's client
/// matches it to the event.
pub fn to_reply(event: &CalendarEvent, organizer: &str, attendee: &str, partstat: &str) -> String {
    let mut lines = vec![
        "BEGIN:VCALENDAR".to_owned(),
        "VERSION:2.0".to_owned(),
        format!("PRODID:{PRODID}"),
        "METHOD:REPLY".to_owned(),
        "BEGIN:VEVENT".to_owned(),
        format!("UID:{}", event.id.as_str()),
        format!("DTSTAMP:{}", fmt_utc(OffsetDateTime::now_utc())),
        format!("ORGANIZER:mailto:{organizer}"),
        format!("ATTENDEE;PARTSTAT={partstat}:mailto:{attendee}"),
    ];
    if event.all_day {
        lines.push(format!("DTSTART;VALUE=DATE:{}", fmt_date(event.starts_at)));
        lines.push(format!("DTEND;VALUE=DATE:{}", fmt_date(event.ends_at)));
    } else {
        lines.push(format!("DTSTART:{}", fmt_utc(event.starts_at)));
        lines.push(format!("DTEND:{}", fmt_utc(event.ends_at)));
    }
    lines.push(format!("SUMMARY:{}", escape(&event.summary)));
    lines.push("SEQUENCE:0".to_owned());
    lines.push("END:VEVENT".to_owned());
    lines.push("END:VCALENDAR".to_owned());
    fold_join(&lines)
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
    let mut chunk = 75;
    while i < bytes.len() {
        // Take up to `chunk` more octets, but don't split a UTF-8 sequence:
        // back off to a char boundary. `chunk` is measured from `i`, the start
        // of this run — a fixed absolute limit would slice backwards once past
        // the first fold.
        let mut end = (i + chunk).min(bytes.len());
        while end > i && end < bytes.len() && (bytes[end] & 0xC0) == 0x80 {
            end -= 1;
        }
        if i > 0 {
            out.push_str("\r\n ");
        }
        out.push_str(&line[i..end]);
        i = end;
        chunk = 74; // continuation lines carry a leading space, so one octet less
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
            attendees: vec!["guest@example.com".to_owned()],
        };
        let ics = to_ics(&e);
        // The ATTENDEE line exceeds 75 octets and folds; unfold to check it.
        assert!(unfold(&ics).contains("mailto:guest@example.com"));
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
        assert_eq!(back.attendees, vec!["guest@example.com".to_owned()]);
        assert!(!back.all_day);
    }

    #[test]
    fn imip_request_carries_method_and_organizer() {
        let e = CalendarEvent {
            id: EventId::new("mtg-1".to_owned()),
            summary: "Kickoff".into(),
            description: None,
            location: None,
            starts_at: OffsetDateTime::new_utc(
                Date::from_calendar_date(2026, time::Month::September, 1).unwrap(),
                Time::from_hms(14, 0, 0).unwrap(),
            ),
            ends_at: OffsetDateTime::new_utc(
                Date::from_calendar_date(2026, time::Month::September, 1).unwrap(),
                Time::from_hms(15, 0, 0).unwrap(),
            ),
            all_day: false,
            recurrence: None,
            attendees: vec!["guest@example.com".to_owned()],
        };
        let msg = to_imip(&e, "owner@alomails.com", "REQUEST");
        assert!(msg.contains("METHOD:REQUEST"));
        assert!(msg.contains("ORGANIZER:mailto:owner@alomails.com"));
        // The ATTENDEE line folds past 75 octets; check it unfolded.
        assert!(unfold(&msg).contains(
            "ATTENDEE;ROLE=REQ-PARTICIPANT;RSVP=TRUE;PARTSTAT=NEEDS-ACTION:mailto:guest@example.com"
        ));
        assert!(msg.contains("UID:mtg-1"));
        // A plain export never advertises a scheduling method or organizer.
        assert!(!to_ics(&e).contains("METHOD:"));
        assert!(!to_ics(&e).contains("ORGANIZER:"));
    }

    #[test]
    fn reads_method_and_organizer_and_replies() {
        let invite = concat!(
            "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nMETHOD:REQUEST\r\n",
            "BEGIN:VEVENT\r\nUID:evt-9\r\nORGANIZER;CN=Boss:mailto:boss@example.com\r\n",
            "DTSTART:20260910T130000Z\r\nDTEND:20260910T140000Z\r\nSUMMARY:Kickoff\r\n",
            "ATTENDEE;PARTSTAT=NEEDS-ACTION:mailto:me@alomails.com\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n"
        );
        assert_eq!(method_of(invite).as_deref(), Some("REQUEST"));
        assert_eq!(organizer_of(invite).as_deref(), Some("boss@example.com"));
        assert_eq!(uid_of(invite).as_deref(), Some("evt-9"));
        // A minimal CANCEL (no DTSTART) is still identified by its UID + method.
        let cancel = "BEGIN:VCALENDAR\r\nMETHOD:CANCEL\r\nBEGIN:VEVENT\r\nUID:evt-9\r\nSTATUS:CANCELLED\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        assert_eq!(method_of(cancel).as_deref(), Some("CANCEL"));
        assert_eq!(uid_of(cancel).as_deref(), Some("evt-9"));
        // No method / organizer on a plain export.
        let plain = to_ics(&CalendarEvent {
            id: EventId::new("x".to_owned()),
            summary: "Plain".into(),
            description: None,
            location: None,
            starts_at: OffsetDateTime::new_utc(
                Date::from_calendar_date(2026, time::Month::September, 10).unwrap(),
                Time::from_hms(13, 0, 0).unwrap(),
            ),
            ends_at: OffsetDateTime::new_utc(
                Date::from_calendar_date(2026, time::Month::September, 10).unwrap(),
                Time::from_hms(14, 0, 0).unwrap(),
            ),
            all_day: false,
            recurrence: None,
            attendees: vec![],
        });
        assert_eq!(method_of(&plain), None);
        assert_eq!(organizer_of(&plain), None);

        // A REPLY carries the same UID, the organizer, and just the responder.
        let event = from_ics(invite, "fallback").unwrap();
        let reply = to_reply(&event, "boss@example.com", "me@alomails.com", "ACCEPTED");
        assert!(reply.contains("METHOD:REPLY"));
        assert!(reply.contains("UID:evt-9"));
        assert!(reply.contains("ORGANIZER:mailto:boss@example.com"));
        assert!(unfold(&reply).contains("ATTENDEE;PARTSTAT=ACCEPTED:mailto:me@alomails.com"));
        assert_eq!(method_of(&reply).as_deref(), Some("REPLY"));
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
            attendees: vec![],
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
