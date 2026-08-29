//! Minimal iCalendar (RFC 5545) serialization for the calendar's `VEVENT`s —
//! the calendar sibling of [`crate::vcard`]. Slice-2 scope: `UID`, `SUMMARY`,
//! `DESCRIPTION`, `LOCATION`, `DTSTART`/`DTEND` as UTC (`…Z`) or all-day
//! (`VALUE=DATE`). A `TZID`-qualified or floating time is read as UTC — a
//! documented cut (`docs/interop.md`); clients that write UTC round-trip
//! exactly. Text values are escaped/unescaped per §3.3.11 and long lines are
//! folded at 75 octets on write and unfolded on read.

use time::{Date, Month, OffsetDateTime, Time, UtcOffset};

use crate::calendar_availability::CalendarBusySpan;
use crate::id::{CalendarId, EventId};
use crate::model::{CalendarEvent, OccurrenceOverride};

const PRODID: &str = "-//alo//calendar//EN";

/// Serialize an event as a complete single-`VEVENT` `VCALENDAR` document.
pub fn to_ics(event: &CalendarEvent) -> String {
    to_ics_at(event, OffsetDateTime::now_utc())
}

/// [`to_ics`] with a caller-supplied `DTSTAMP` instant. `DTSTAMP` is the one
/// property not derived from the event (RFC 5545 §3.8.7.2 — the moment of
/// serialization), so pinning it is what makes the output a pure function of
/// the event: the round-trip corpus proves parse → store → serialize is
/// byte-stable through this seam.
pub fn to_ics_at(event: &CalendarEvent, dtstamp: OffsetDateTime) -> String {
    let mut lines = vec![
        "BEGIN:VCALENDAR".to_owned(),
        "VERSION:2.0".to_owned(),
        format!("PRODID:{PRODID}"),
    ];
    lines.extend(vevent_lines(event, None, dtstamp));
    lines.push("END:VCALENDAR".to_owned());
    fold_join(&lines)
}

/// Serialize a recurring master plus its `RECURRENCE-ID` override instances as a
/// single `VCALENDAR` (the master `VEVENT`, then one `VEVENT` per edited
/// occurrence), so a CalDAV client renders per-occurrence edits. With no
/// overrides this equals [`to_ics`].
pub fn to_ics_series(master: &CalendarEvent, overrides: &[CalendarEvent]) -> String {
    to_ics_series_at(master, overrides, OffsetDateTime::now_utc())
}

/// [`to_ics_series`] with a caller-supplied `DTSTAMP` instant — the same
/// deterministic seam as [`to_ics_at`], so the round-trip corpus can pin a
/// multi-`VEVENT` series byte-for-byte.
pub fn to_ics_series_at(
    master: &CalendarEvent,
    overrides: &[CalendarEvent],
    dtstamp: OffsetDateTime,
) -> String {
    let mut lines = vec![
        "BEGIN:VCALENDAR".to_owned(),
        "VERSION:2.0".to_owned(),
        format!("PRODID:{PRODID}"),
    ];
    lines.extend(vevent_lines(master, None, dtstamp));
    for ov in overrides {
        lines.extend(vevent_lines(ov, None, dtstamp));
    }
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
    lines.extend(vevent_lines(
        event,
        Some(organizer),
        OffsetDateTime::now_utc(),
    ));
    lines.push("END:VCALENDAR".to_owned());
    fold_join(&lines)
}

/// Serialize a free/busy answer as a `VCALENDAR` holding one `VFREEBUSY`
/// (RFC 5545 §3.6.4): the queried window as `DTSTART`/`DTEND` and one
/// `FREEBUSY;FBTYPE=BUSY` period per span, all UTC. The component has no
/// field for a title, location, or description — busy time is the whole of
/// what this function can say, whatever the caller feeds it. `dtstamp` is
/// caller-supplied for the same reason as [`to_ics_at`]: pinned, the output
/// is a pure function of its inputs.
pub fn to_vfreebusy(
    uid: &str,
    from: OffsetDateTime,
    to: OffsetDateTime,
    busy: &[CalendarBusySpan],
    dtstamp: OffsetDateTime,
) -> String {
    let mut lines = vec![
        "BEGIN:VCALENDAR".to_owned(),
        "VERSION:2.0".to_owned(),
        format!("PRODID:{PRODID}"),
        "BEGIN:VFREEBUSY".to_owned(),
        format!("UID:{uid}"),
        format!("DTSTAMP:{}", fmt_utc(dtstamp)),
        format!("DTSTART:{}", fmt_utc(from)),
        format!("DTEND:{}", fmt_utc(to)),
    ];
    for span in busy {
        lines.push(format!(
            "FREEBUSY;FBTYPE=BUSY:{}/{}",
            fmt_utc(span.from),
            fmt_utc(span.to)
        ));
    }
    lines.push("END:VFREEBUSY".to_owned());
    lines.push("END:VCALENDAR".to_owned());
    fold_join(&lines)
}

/// The `VEVENT` body shared by [`to_ics`] and [`to_imip`]; an `organizer`, when
/// given, adds the `ORGANIZER` property (present only in scheduling messages).
fn vevent_lines(
    event: &CalendarEvent,
    organizer: Option<&str>,
    dtstamp: OffsetDateTime,
) -> Vec<String> {
    // A series that follows a zone's wall-clock serializes its date-times as
    // `;TZID=<zone>:<local>` so clients expand the recurrence in that zone
    // (no `VTIMEZONE` block — the IANA name is the definition; documented in
    // docs/interop.md). Everything else stays UTC (`…Z`). `DTSTAMP` is always
    // UTC (RFC 5545 §3.8.7.2).
    let zone = event
        .timezone
        .as_deref()
        .filter(|_| !event.all_day)
        .and_then(|name| crate::tz::zone(name).map(|z| (name, z)));
    let dt_prop = |name: &str, t: OffsetDateTime| match &zone {
        Some((tzid, z)) => format!(
            "{name};TZID={tzid}:{}",
            fmt_wall(crate::tz::utc_to_wall(t, z))
        ),
        None => format!("{name}:{}", fmt_utc(t)),
    };
    let mut lines = vec![
        "BEGIN:VEVENT".to_owned(),
        format!("UID:{}", event.id.as_str()),
        format!("DTSTAMP:{}", fmt_utc(dtstamp)),
    ];
    if let Some(org) = organizer {
        lines.push(format!("ORGANIZER:mailto:{org}"));
    }
    if event.all_day {
        lines.push(format!("DTSTART;VALUE=DATE:{}", fmt_date(event.starts_at)));
        lines.push(format!("DTEND;VALUE=DATE:{}", fmt_date(event.ends_at)));
    } else {
        lines.push(dt_prop("DTSTART", event.starts_at));
        lines.push(dt_prop("DTEND", event.ends_at));
    }
    // For an overridden/one-off instance of a series, the slot it replaces.
    if let Some(rid) = event.recurrence_id {
        if event.all_day {
            lines.push(format!("RECURRENCE-ID;VALUE=DATE:{}", fmt_date(rid)));
        } else {
            lines.push(dt_prop("RECURRENCE-ID", rid));
        }
    }
    if let Some(rrule) = &event.recurrence {
        lines.push(format!("RRULE:{rrule}"));
    }
    // Extra occurrences beyond the rule.
    for rd in &event.rdates {
        if event.all_day {
            lines.push(format!("RDATE;VALUE=DATE:{}", fmt_date(*rd)));
        } else {
            lines.push(dt_prop("RDATE", *rd));
        }
    }
    // Individually cancelled occurrences of the series.
    for ex in &event.exdates {
        if event.all_day {
            lines.push(format!("EXDATE;VALUE=DATE:{}", fmt_date(*ex)));
        } else {
            lines.push(dt_prop("EXDATE", *ex));
        }
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
    // A reminder becomes a display VALARM triggered `n` minutes before the start,
    // so the alert fires natively in the client (phone/Apple Calendar).
    if let Some(mins) = event.reminder_minutes {
        lines.push("BEGIN:VALARM".to_owned());
        lines.push("ACTION:DISPLAY".to_owned());
        lines.push(format!("DESCRIPTION:{}", escape(&event.summary)));
        lines.push(format!("TRIGGER:-PT{}M", mins.max(0)));
        lines.push("END:VALARM".to_owned());
    }
    lines.push("END:VEVENT".to_owned());
    lines
}

fn fold_join(lines: &[String]) -> String {
    lines
        .iter()
        .map(|l| fold(l))
        .collect::<Vec<_>>()
        .join("\r\n")
        + "\r\n"
}

/// Parse the first `VEVENT` in an iCalendar document into an event, using
/// `fallback_id` as the `UID` if the document omits one. The parsed event
/// deliberately keeps `recurrence_id: None` (see [`recurrence_id_of`]); a
/// multi-`VEVENT` series is read whole by [`from_ics_series`].
pub fn from_ics(text: &str, fallback_id: &str) -> Option<CalendarEvent> {
    parse_vevents(text, fallback_id)
        .into_iter()
        .next()
        .map(|v| v.event)
}

/// A client-authored series as one CalDAV resource (RFC 4791 §4.1): the master
/// `VEVENT` plus the per-occurrence state carried by its `RECURRENCE-ID`
/// instances (RFC 5545 §3.8.4.4). Cancelled instances have already been folded
/// into the master's `exdates`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedSeries {
    /// The series master (or the one-off, for a single-`VEVENT` document).
    pub master: CalendarEvent,
    /// One entry per overridden occurrence: the original slot and the fields
    /// that replace it there.
    pub overrides: Vec<(OffsetDateTime, OccurrenceOverride)>,
}

/// Parse every `VEVENT` of an iCalendar document as one series: the `VEVENT`
/// without a `RECURRENCE-ID` is the master, each one with a `RECURRENCE-ID`
/// is an override at that slot, and an instance marked `STATUS:CANCELLED`
/// becomes an `EXDATE` on the master instead. This is what a phone PUTs after
/// a per-occurrence edit (RFC 5545 §3.8.4.4, RFC 4791 §4.1).
///
/// Tolerances: a document holding only override instances keeps the first as
/// the event (the single-`VEVENT` reader's behaviour); a duplicated slot keeps
/// the last `VEVENT` (upsert semantics); a `RANGE=THISANDFUTURE` parameter is
/// read as a single-instance override (documented in `docs/interop.md`).
pub fn from_ics_series(text: &str, fallback_id: &str) -> Option<ParsedSeries> {
    let parsed = parse_vevents(text, fallback_id);
    let master_at = parsed
        .iter()
        .position(|v| v.recurrence_id.is_none())
        .unwrap_or(0);
    let mut master: Option<CalendarEvent> = None;
    let mut overrides: Vec<(OffsetDateTime, OccurrenceOverride)> = Vec::new();
    let mut cancelled_slots: Vec<OffsetDateTime> = Vec::new();
    for (i, v) in parsed.into_iter().enumerate() {
        if i == master_at {
            master = Some(v.event);
            continue;
        }
        // A second master-shaped VEVENT is not this resource's to store; skip
        // it rather than guess which of the two the client meant.
        let Some(slot) = v.recurrence_id else {
            continue;
        };
        if v.cancelled {
            cancelled_slots.push(slot);
            continue;
        }
        overrides.retain(|(s, _)| *s != slot);
        overrides.push((
            slot,
            OccurrenceOverride {
                summary: v.event.summary,
                description: v.event.description,
                location: v.event.location,
                starts_at: v.event.starts_at,
                ends_at: v.event.ends_at,
                all_day: v.event.all_day,
            },
        ));
    }
    let mut master = master?;
    for slot in cancelled_slots {
        if !master.exdates.contains(&slot) {
            master.exdates.push(slot);
        }
    }
    Some(ParsedSeries { master, overrides })
}

/// One parsed `VEVENT` block: the event fields plus the series metadata that
/// distinguishes a master from a `RECURRENCE-ID` override instance.
struct ParsedVevent {
    /// The block's fields as an event (`recurrence_id` deliberately `None` —
    /// the slot is reported separately below).
    event: CalendarEvent,
    /// The original slot this instance overrides (RFC 5545 §3.8.4.4), if any.
    recurrence_id: Option<OffsetDateTime>,
    /// `STATUS:CANCELLED` — on an override instance, a skipped slot.
    cancelled: bool,
}

/// Split a document into its top-level `VEVENT` blocks and parse each.
/// Everything outside `BEGIN:VEVENT`…`END:VEVENT` (`VCALENDAR` properties,
/// `VTIMEZONE` definitions and their `STANDARD`/`DAYLIGHT` rules) is skipped —
/// the IANA `TZID` name is the zone definition here, not the shipped block.
fn parse_vevents(text: &str, fallback_id: &str) -> Vec<ParsedVevent> {
    let unfolded = unfold(text);
    let mut out = Vec::new();
    let mut block: Option<Vec<&str>> = None;
    for line in unfolded.lines() {
        let upper = line.to_ascii_uppercase();
        match block.as_mut() {
            None => {
                if upper == "BEGIN:VEVENT" {
                    block = Some(Vec::new());
                }
            }
            Some(lines) => {
                if upper == "END:VEVENT" {
                    let done = std::mem::take(lines);
                    block = None;
                    if let Some(v) = parse_vevent(&done, fallback_id) {
                        out.push(v);
                    }
                } else {
                    lines.push(line);
                }
            }
        }
    }
    out
}

/// Parse one unfolded `VEVENT` block (its lines, without the BEGIN/END pair).
fn parse_vevent(block: &[&str], fallback_id: &str) -> Option<ParsedVevent> {
    let mut uid: Option<String> = None;
    let mut summary = String::new();
    let mut description: Option<String> = None;
    let mut location: Option<String> = None;
    let mut start: Option<(OffsetDateTime, bool)> = None;
    let mut end: Option<(OffsetDateTime, bool)> = None;
    let mut recurrence: Option<String> = None;
    let mut attendees: Vec<String> = Vec::new();
    let mut exdates: Vec<OffsetDateTime> = Vec::new();
    let mut timezone: Option<String> = None;
    let mut rdates: Vec<OffsetDateTime> = Vec::new();
    let mut reminder_minutes: Option<i32> = None;
    let mut recurrence_id: Option<OffsetDateTime> = None;
    let mut cancelled = false;
    // A VALARM is a nested block: read only its TRIGGER, and never let its
    // properties (e.g. DESCRIPTION) bleed into the event's.
    let mut in_alarm = false;

    for line in block {
        let upper = line.to_ascii_uppercase();
        if upper == "BEGIN:VALARM" {
            in_alarm = true;
            continue;
        }
        if upper == "END:VALARM" {
            in_alarm = false;
            continue;
        }
        let Some((spec, value)) = line.split_once(':') else {
            continue;
        };
        let mut segs = spec.split(';');
        let name = segs.next().unwrap_or("").to_ascii_uppercase();
        let params: Vec<&str> = segs.collect();
        let is_date = params.iter().any(|p| p.eq_ignore_ascii_case("VALUE=DATE"));
        // A `TZID=<zone>` parameter names the wall-clock's IANA zone.
        let tzid = params.iter().find_map(|p| {
            p.split_once('=')
                .filter(|(k, _)| k.eq_ignore_ascii_case("TZID"))
                .map(|(_, v)| v)
        });
        if in_alarm {
            // Only the alarm's lead time matters; the first VALARM wins.
            if name == "TRIGGER" && reminder_minutes.is_none() {
                reminder_minutes = trigger_to_minutes(value.trim());
            }
            continue;
        }
        match name.as_str() {
            "UID" => uid = Some(value.trim().to_owned()),
            "SUMMARY" => summary = unescape(value),
            "DESCRIPTION" => description = Some(unescape(value)),
            "LOCATION" => location = Some(unescape(value)),
            "DTSTART" => {
                start = parse_dt(value.trim(), is_date, tzid);
                // The series' wall-clock zone, for DST-correct recurrence
                // expansion — kept only when it resolved (an unknown zone fell
                // back to UTC above and must keep expanding as UTC).
                if !is_date
                    && !value.trim().ends_with('Z')
                    && let Some(z) = tzid
                    && crate::tz::known(z)
                {
                    timezone = Some(z.to_owned());
                }
            }
            "DTEND" => end = parse_dt(value.trim(), is_date, tzid),
            "RRULE" => recurrence = Some(value.trim().to_owned()),
            "EXDATE" => {
                // One or more excluded instants, comma-separated; the value may
                // be date-only (`VALUE=DATE`) or a UTC/zoned date-time.
                for token in value.split(',') {
                    if let Some((dt, _)) = parse_dt(token.trim(), is_date, tzid) {
                        exdates.push(dt);
                    }
                }
            }
            "RDATE" => {
                // Extra occurrence instants, same forms as EXDATE.
                // `VALUE=PERIOD` (start/duration pairs) is not modelled; such
                // values are skipped (docs/interop.md).
                if params
                    .iter()
                    .any(|p| p.eq_ignore_ascii_case("VALUE=PERIOD"))
                {
                    continue;
                }
                for token in value.split(',') {
                    if let Some((dt, _)) = parse_dt(token.trim(), is_date, tzid) {
                        rdates.push(dt);
                    }
                }
            }
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
            // The slot an override instance replaces. Any RANGE parameter is
            // ignored (read as this one instance; docs/interop.md).
            "RECURRENCE-ID" => {
                recurrence_id = parse_dt(value.trim(), is_date, tzid).map(|(dt, _)| dt);
            }
            "STATUS" => cancelled = value.trim().eq_ignore_ascii_case("CANCELLED"),
            _ => {}
        }
    }

    let (starts_at, all_day) = start?;
    let (ends_at, _) = end.unwrap_or((starts_at, all_day));
    if summary.trim().is_empty() {
        summary = "(no title)".to_owned();
    }
    Some(ParsedVevent {
        event: CalendarEvent {
            id: EventId::new(uid.unwrap_or_else(|| fallback_id.to_owned())),
            // iCalendar carries no calendar grouping; the caller (CalDAV
            // collection or RSVP → personal) sets which calendar this lands on.
            calendar_id: CalendarId::new(String::new()),
            summary,
            description: description.filter(|s| !s.is_empty()),
            location: location.filter(|s| !s.is_empty()),
            starts_at,
            ends_at,
            all_day,
            recurrence,
            attendees,
            exdates,
            timezone,
            rdates,
            // The slot travels beside the event (`ParsedVevent.recurrence_id`),
            // never on it: callers of [`from_ics`] read a master/one-off.
            recurrence_id: None,
            reminder_minutes,
            attendee_status: Vec::new(),
        },
        recurrence_id,
        cancelled,
    })
}

/// Convert a VALARM `TRIGGER` duration to a reminder lead-time in minutes before
/// the start. A negative (before-start) trigger yields that many minutes; a
/// zero/positive one is treated as "at start" (0). Weeks/days/hours/minutes of
/// the ISO-8601 duration are summed; sub-minute parts are ignored.
fn trigger_to_minutes(value: &str) -> Option<i32> {
    let neg = value.starts_with('-');
    let body = value.trim_start_matches(['-', '+']).strip_prefix('P')?;
    let (date_part, time_part) = body.split_once('T').unwrap_or((body, ""));
    let mut total: i64 = 0;
    let mut scan = |part: &str, in_time: bool| -> Option<()> {
        let mut num = String::new();
        for ch in part.chars() {
            if ch.is_ascii_digit() {
                num.push(ch);
                continue;
            }
            let n: i64 = num.parse().ok()?;
            num.clear();
            total += match (ch.to_ascii_uppercase(), in_time) {
                ('W', false) => n * 7 * 24 * 60,
                ('D', false) => n * 24 * 60,
                ('H', true) => n * 60,
                ('M', true) => n,
                ('S', true) => 0,
                _ => return None,
            };
        }
        Some(())
    };
    scan(date_part, false)?;
    scan(time_part, true)?;
    let mins = if neg { total } else { 0 };
    i32::try_from(mins.clamp(0, 40_320)).ok()
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

/// The `RECURRENCE-ID` of the first `VEVENT`, if present — the single instance
/// an iMIP `CANCEL` (or update) names, as the UTC instant of that occurrence's
/// original slot. Read on its own (not via [`from_ics`], whose parsed events
/// deliberately keep `recurrence_id: None` — the CalDAV override-sync
/// contract). Accepts the same value shapes the serializer emits: a UTC
/// date-time, a `VALUE=DATE` date, or a `TZID=<zone>` wall-clock time.
pub fn recurrence_id_of(text: &str) -> Option<OffsetDateTime> {
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
        let mut segs = spec.split(';');
        if !segs
            .next()
            .unwrap_or("")
            .eq_ignore_ascii_case("RECURRENCE-ID")
        {
            continue;
        }
        let params: Vec<&str> = segs.collect();
        let is_date = params.iter().any(|p| p.eq_ignore_ascii_case("VALUE=DATE"));
        let tzid = params.iter().find_map(|p| {
            p.split_once('=')
                .filter(|(k, _)| k.eq_ignore_ascii_case("TZID"))
                .map(|(_, v)| v)
        });
        return parse_dt(value.trim(), is_date, tzid).map(|(dt, _)| dt);
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

/// For an iMIP `REPLY`, the replying attendee's email and `PARTSTAT`
/// (`ACCEPTED` | `DECLINED` | `TENTATIVE` | `NEEDS-ACTION`). Used by the
/// organizer's side to record who responded. `None` if no attendee is present.
pub fn reply_of(text: &str) -> Option<(String, String)> {
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
        let mut params = spec.split(';');
        if !params.next().unwrap_or("").eq_ignore_ascii_case("ATTENDEE") {
            continue;
        }
        let partstat = params
            .find_map(|p| {
                p.to_ascii_uppercase()
                    .strip_prefix("PARTSTAT=")
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| "NEEDS-ACTION".to_owned());
        let v = value.trim();
        let addr = v
            .strip_prefix("mailto:")
            .or_else(|| v.strip_prefix("MAILTO:"))
            .unwrap_or(v);
        if addr.contains('@') {
            return Some((addr.to_owned(), partstat));
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

/// A wall-clock (already zone-local) date-time — the digits without a `Z`,
/// for `;TZID=`-qualified properties.
fn fmt_wall(t: OffsetDateTime) -> String {
    format!(
        "{:04}{:02}{:02}T{:02}{:02}{:02}",
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
fn parse_dt(value: &str, is_date: bool, tzid: Option<&str>) -> Option<(OffsetDateTime, bool)> {
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
    // A `TZID=` on a non-UTC (no trailing Z) value names an IANA zone: convert
    // that wall-clock time to the UTC instant. An unknown zone (e.g. a Windows
    // name) or a floating time falls back to UTC (documented, docs/interop.md).
    if !value.ends_with('Z')
        && let Some(utc) =
            tzid.and_then(|z| local_to_utc(year, month, day, hour, minute, second, z))
    {
        return Some((utc, false));
    }
    Some((OffsetDateTime::new_utc(date, time), false))
}

/// Convert a wall-clock time in the named IANA zone to a UTC `OffsetDateTime`,
/// or `None` if the zone is unknown or the civil time is invalid.
fn local_to_utc(
    year: i32,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: u8,
    tzid: &str,
) -> Option<OffsetDateTime> {
    let zone = crate::tz::zone(tzid)?;
    let date = Date::from_calendar_date(year, Month::try_from(month).ok()?, day).ok()?;
    let wall = OffsetDateTime::new_utc(date, Time::from_hms(hour, minute, second).ok()?);
    crate::tz::wall_to_utc(wall, &zone)
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
    fn vfreebusy_is_busy_periods_and_nothing_else() {
        let at = |d: u8, h: u8| {
            OffsetDateTime::new_utc(
                Date::from_calendar_date(2026, time::Month::June, d).unwrap(),
                Time::from_hms(h, 0, 0).unwrap(),
            )
        };
        let stamp = OffsetDateTime::new_utc(
            Date::from_calendar_date(2026, time::Month::January, 1).unwrap(),
            Time::from_hms(0, 0, 0).unwrap(),
        );
        let busy = [
            CalendarBusySpan {
                from: at(10, 9),
                to: at(10, 11),
            },
            CalendarBusySpan {
                from: at(11, 14),
                to: at(11, 15),
            },
        ];
        let ics = to_vfreebusy("fb-1", at(1, 0), at(30, 0), &busy, stamp);
        assert_eq!(
            ics,
            "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//alo//calendar//EN\r\n\
             BEGIN:VFREEBUSY\r\nUID:fb-1\r\nDTSTAMP:20260101T000000Z\r\n\
             DTSTART:20260601T000000Z\r\nDTEND:20260630T000000Z\r\n\
             FREEBUSY;FBTYPE=BUSY:20260610T090000Z/20260610T110000Z\r\n\
             FREEBUSY;FBTYPE=BUSY:20260611T140000Z/20260611T150000Z\r\n\
             END:VFREEBUSY\r\nEND:VCALENDAR\r\n"
        );
        // An empty window is a valid VFREEBUSY with no periods.
        let free = to_vfreebusy("fb-2", at(1, 0), at(30, 0), &[], stamp);
        assert!(free.contains("BEGIN:VFREEBUSY"));
        assert!(!free.contains("FREEBUSY;"));
    }

    #[test]
    fn timed_event_round_trips() {
        let e = CalendarEvent {
            id: EventId::new("abc123".to_owned()),
            calendar_id: CalendarId::new("cal".to_owned()),
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
            exdates: vec![],
            timezone: None,
            rdates: vec![],
            recurrence_id: None,
            reminder_minutes: None,
            attendee_status: vec![],
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
            calendar_id: CalendarId::new("cal".to_owned()),
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
            exdates: vec![],
            timezone: None,
            rdates: vec![],
            recurrence_id: None,
            reminder_minutes: None,
            attendee_status: vec![],
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
            calendar_id: CalendarId::new("cal".to_owned()),
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
            exdates: vec![],
            timezone: None,
            rdates: vec![],
            recurrence_id: None,
            reminder_minutes: None,
            attendee_status: vec![],
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
    fn exdates_round_trip() {
        let ex = OffsetDateTime::new_utc(
            Date::from_calendar_date(2026, time::Month::September, 15).unwrap(),
            Time::from_hms(9, 0, 0).unwrap(),
        );
        let e = CalendarEvent {
            id: EventId::new("series-1".to_owned()),
            calendar_id: CalendarId::new("cal".to_owned()),
            summary: "Standup".into(),
            description: None,
            location: None,
            starts_at: OffsetDateTime::new_utc(
                Date::from_calendar_date(2026, time::Month::September, 1).unwrap(),
                Time::from_hms(9, 0, 0).unwrap(),
            ),
            ends_at: OffsetDateTime::new_utc(
                Date::from_calendar_date(2026, time::Month::September, 1).unwrap(),
                Time::from_hms(9, 15, 0).unwrap(),
            ),
            all_day: false,
            recurrence: Some("FREQ=WEEKLY".to_owned()),
            attendees: vec![],
            exdates: vec![ex],
            timezone: None,
            rdates: vec![],
            recurrence_id: None,
            reminder_minutes: None,
            attendee_status: vec![],
        };
        let ics = to_ics(&e);
        assert!(ics.contains("EXDATE:20260915T090000Z"));
        let back = from_ics(&ics, "fb").unwrap();
        assert_eq!(back.exdates, vec![ex]);
    }

    #[test]
    fn reminder_round_trips_as_valarm() {
        let start = OffsetDateTime::new_utc(
            Date::from_calendar_date(2026, time::Month::September, 1).unwrap(),
            Time::from_hms(9, 0, 0).unwrap(),
        );
        let e = CalendarEvent {
            id: EventId::new("rem-1".to_owned()),
            calendar_id: CalendarId::new("cal".to_owned()),
            summary: "Standup".into(),
            description: None,
            location: None,
            starts_at: start,
            ends_at: start + time::Duration::minutes(30),
            all_day: false,
            recurrence: None,
            attendees: vec![],
            exdates: vec![],
            timezone: None,
            rdates: vec![],
            recurrence_id: None,
            reminder_minutes: Some(10),
            attendee_status: vec![],
        };
        let ics = to_ics(&e);
        assert!(ics.contains("BEGIN:VALARM"));
        assert!(ics.contains("TRIGGER:-PT10M"));
        assert_eq!(from_ics(&ics, "fb").unwrap().reminder_minutes, Some(10));
        // A VALARM DESCRIPTION must not clobber the event's fields.
        assert!(from_ics(&ics, "fb").unwrap().description.is_none());
    }

    #[test]
    fn series_serializes_master_plus_override_vevents() {
        let start = OffsetDateTime::new_utc(
            Date::from_calendar_date(2026, time::Month::September, 1).unwrap(),
            Time::from_hms(9, 0, 0).unwrap(),
        );
        let master = CalendarEvent {
            id: EventId::new("series-9".to_owned()),
            calendar_id: CalendarId::new("cal".to_owned()),
            summary: "Standup".into(),
            description: None,
            location: None,
            starts_at: start,
            ends_at: start + time::Duration::minutes(30),
            all_day: false,
            recurrence: Some("FREQ=WEEKLY".to_owned()),
            attendees: vec![],
            exdates: vec![],
            timezone: None,
            rdates: vec![],
            recurrence_id: None,
            reminder_minutes: None,
            attendee_status: vec![],
        };
        // The Sep 8 occurrence, moved to 15:00 (its slot stays Sep 8 09:00).
        let slot = start + time::Duration::weeks(1);
        let override_ev = CalendarEvent {
            starts_at: slot + time::Duration::hours(6),
            ends_at: slot + time::Duration::hours(6) + time::Duration::minutes(30),
            recurrence: None,
            recurrence_id: Some(slot),
            ..master.clone()
        };
        let ics = to_ics_series(&master, std::slice::from_ref(&override_ev));
        assert_eq!(
            ics.matches("BEGIN:VEVENT").count(),
            2,
            "master + one override"
        );
        assert!(ics.contains("RRULE:FREQ=WEEKLY"));
        assert!(ics.contains("RECURRENCE-ID:20260908T090000Z"));
        assert!(ics.contains("DTSTART:20260908T150000Z"));
    }

    #[test]
    fn tzid_datetime_converts_to_utc() {
        // 09:00 America/New_York on 2026-09-01 is EDT (UTC-4) → 13:00Z.
        let ics = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:tz1\r\n\
                   DTSTART;TZID=America/New_York:20260901T090000\r\n\
                   DTEND;TZID=America/New_York:20260901T093000\r\n\
                   SUMMARY:Zoned\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let e = from_ics(ics, "fb").unwrap();
        assert_eq!(fmt_utc(e.starts_at), "20260901T130000Z");
        assert_eq!(fmt_utc(e.ends_at), "20260901T133000Z");
        // The wall-clock's zone is kept for DST-correct recurrence expansion.
        assert_eq!(e.timezone.as_deref(), Some("America/New_York"));
    }

    #[test]
    fn unknown_tzid_falls_back_to_utc() {
        // A Windows zone name jiff can't resolve → the wall time is kept as UTC.
        let ics = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:tz2\r\n\
                   DTSTART;TZID=Eastern Standard Time:20260901T090000\r\n\
                   SUMMARY:Winzone\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let e = from_ics(ics, "fb").unwrap();
        assert_eq!(fmt_utc(e.starts_at), "20260901T090000Z");
        // The unresolvable name is not kept: expansion must stay UTC too.
        assert_eq!(e.timezone, None);
    }

    #[test]
    fn zoned_series_serializes_tzid_wall_clock_and_round_trips() {
        // A Brussels weekly with an EXDATE and an RDATE: every date-time
        // property serializes as the zone's wall-clock (no Z), and the form
        // is a fixed point through parse → serialize.
        let start = OffsetDateTime::new_utc(
            Date::from_calendar_date(2026, time::Month::October, 19).unwrap(),
            Time::from_hms(7, 0, 0).unwrap(), // 09:00 Brussels (CEST)
        );
        let e = CalendarEvent {
            id: EventId::new("tz-series".to_owned()),
            calendar_id: CalendarId::new("cal".to_owned()),
            summary: "Weekly".into(),
            description: None,
            location: None,
            starts_at: start,
            ends_at: start + time::Duration::minutes(30),
            all_day: false,
            recurrence: Some("FREQ=WEEKLY".to_owned()),
            attendees: vec![],
            // Skip Nov 2 (post-DST: 09:00 local = 08:00Z), add Thu Oct 22.
            exdates: vec![OffsetDateTime::new_utc(
                Date::from_calendar_date(2026, time::Month::November, 2).unwrap(),
                Time::from_hms(8, 0, 0).unwrap(),
            )],
            timezone: Some("Europe/Brussels".to_owned()),
            rdates: vec![OffsetDateTime::new_utc(
                Date::from_calendar_date(2026, time::Month::October, 22).unwrap(),
                Time::from_hms(7, 0, 0).unwrap(),
            )],
            recurrence_id: None,
            reminder_minutes: None,
            attendee_status: vec![],
        };
        let ics = to_ics(&e);
        assert!(ics.contains("DTSTART;TZID=Europe/Brussels:20261019T090000"));
        assert!(ics.contains("DTEND;TZID=Europe/Brussels:20261019T093000"));
        assert!(ics.contains("RDATE;TZID=Europe/Brussels:20261022T090000"));
        // The excluded instant is after the DST switch: 09:00 local again.
        assert!(ics.contains("EXDATE;TZID=Europe/Brussels:20261102T090000"));
        let back = from_ics(&ics, "fb").unwrap();
        assert_eq!(back.starts_at, e.starts_at);
        assert_eq!(back.ends_at, e.ends_at);
        assert_eq!(back.timezone, e.timezone);
        assert_eq!(back.rdates, e.rdates);
        assert_eq!(back.exdates, e.exdates);
    }

    #[test]
    fn rdate_round_trips_and_period_values_are_skipped() {
        let ics = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:rd1\r\n\
                   DTSTART:20260803T090000Z\r\nDTEND:20260803T093000Z\r\n\
                   RRULE:FREQ=WEEKLY\r\nRDATE:20260806T090000Z\r\n\
                   RDATE;VALUE=PERIOD:20260807T090000Z/PT1H\r\n\
                   SUMMARY:Standup\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let e = from_ics(ics, "fb").unwrap();
        assert_eq!(e.rdates.len(), 1, "the PERIOD value is skipped");
        assert_eq!(fmt_utc(e.rdates[0]), "20260806T090000Z");
        assert!(to_ics(&e).contains("RDATE:20260806T090000Z"));
    }

    #[test]
    fn recurrence_id_of_reads_all_value_shapes() {
        let at = |h: u8| {
            OffsetDateTime::new_utc(
                Date::from_calendar_date(2026, time::Month::September, 8).unwrap(),
                Time::from_hms(h, 0, 0).unwrap(),
            )
        };
        // A one-instance CANCEL: same UID, RECURRENCE-ID at the cancelled slot.
        let utc = "BEGIN:VCALENDAR\r\nMETHOD:CANCEL\r\nBEGIN:VEVENT\r\nUID:e1\r\n\
                   RECURRENCE-ID:20260908T090000Z\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        assert_eq!(recurrence_id_of(utc), Some(at(9)));
        // All-day instance (VALUE=DATE) → midnight UTC of that date.
        let date = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\n\
                    RECURRENCE-ID;VALUE=DATE:20260908\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        assert_eq!(recurrence_id_of(date), Some(at(0)));
        // Zoned wall-clock: 09:00 Brussels in CEST is 07:00Z.
        let zoned = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\n\
                     RECURRENCE-ID;TZID=Europe/Brussels:20260908T090000\r\n\
                     END:VEVENT\r\nEND:VCALENDAR\r\n";
        assert_eq!(recurrence_id_of(zoned), Some(at(7)));
        // A series-level CANCEL names no instance.
        let series = "BEGIN:VCALENDAR\r\nMETHOD:CANCEL\r\nBEGIN:VEVENT\r\nUID:e1\r\n\
                      END:VEVENT\r\nEND:VCALENDAR\r\n";
        assert_eq!(recurrence_id_of(series), None);
    }

    #[test]
    fn reply_of_reads_attendee_and_partstat() {
        let ics = "BEGIN:VCALENDAR\r\nMETHOD:REPLY\r\nBEGIN:VEVENT\r\nUID:e1\r\n\
                   ATTENDEE;PARTSTAT=ACCEPTED;CN=Al:mailto:al@example.test\r\n\
                   END:VEVENT\r\nEND:VCALENDAR\r\n";
        assert_eq!(
            reply_of(ics),
            Some(("al@example.test".to_owned(), "ACCEPTED".to_owned()))
        );
    }

    #[test]
    fn trigger_durations_parse_to_minutes() {
        assert_eq!(trigger_to_minutes("-PT15M"), Some(15));
        assert_eq!(trigger_to_minutes("-PT1H"), Some(60));
        assert_eq!(trigger_to_minutes("-P1D"), Some(1440));
        assert_eq!(trigger_to_minutes("-PT0S"), Some(0));
        assert_eq!(trigger_to_minutes("PT30M"), Some(0)); // after-start → at start
    }

    #[test]
    fn all_day_uses_value_date() {
        let e = CalendarEvent {
            id: EventId::new("day1".to_owned()),
            calendar_id: CalendarId::new("cal".to_owned()),
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
            exdates: vec![],
            timezone: None,
            rdates: vec![],
            recurrence_id: None,
            reminder_minutes: None,
            attendee_status: vec![],
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

    /// A two-`VEVENT` weekly series as a phone PUTs it after moving one
    /// occurrence: the master keeps its rule, the override lands at its slot.
    fn series_ics(override_first: bool) -> String {
        let master = "BEGIN:VEVENT\r\nUID:ser-1\r\nDTSTAMP:20260101T000000Z\r\n\
                      DTSTART:20260907T090000Z\r\nDTEND:20260907T093000Z\r\n\
                      RRULE:FREQ=WEEKLY\r\nSUMMARY:Standup\r\nEND:VEVENT\r\n";
        let ov = "BEGIN:VEVENT\r\nUID:ser-1\r\nDTSTAMP:20260101T000000Z\r\n\
                  RECURRENCE-ID:20260914T090000Z\r\n\
                  DTSTART:20260914T150000Z\r\nDTEND:20260914T153000Z\r\n\
                  SUMMARY:Standup (moved)\r\nEND:VEVENT\r\n";
        let (a, b) = if override_first {
            (ov, master)
        } else {
            (master, ov)
        };
        format!(
            "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Test//S//EN\r\n{a}{b}END:VCALENDAR\r\n"
        )
    }

    #[test]
    fn from_ics_series_reads_master_and_override_in_either_order() {
        for override_first in [false, true] {
            let s = from_ics_series(&series_ics(override_first), "fb").unwrap();
            assert_eq!(s.master.id.as_str(), "ser-1");
            assert_eq!(s.master.recurrence.as_deref(), Some("FREQ=WEEKLY"));
            assert_eq!(s.master.summary, "Standup");
            assert_eq!(s.overrides.len(), 1);
            let (slot, ov) = &s.overrides[0];
            assert_eq!(fmt_utc(*slot), "20260914T090000Z");
            assert_eq!(fmt_utc(ov.starts_at), "20260914T150000Z");
            assert_eq!(ov.summary, "Standup (moved)");
        }
    }

    #[test]
    fn from_ics_series_cancelled_instance_becomes_exdate() {
        let ics = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\n\
                   BEGIN:VEVENT\r\nUID:ser-2\r\nDTSTART:20260907T090000Z\r\n\
                   DTEND:20260907T093000Z\r\nRRULE:FREQ=WEEKLY\r\nSUMMARY:Standup\r\nEND:VEVENT\r\n\
                   BEGIN:VEVENT\r\nUID:ser-2\r\nRECURRENCE-ID:20260921T090000Z\r\n\
                   DTSTART:20260921T090000Z\r\nDTEND:20260921T093000Z\r\n\
                   STATUS:CANCELLED\r\nSUMMARY:Standup\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let s = from_ics_series(ics, "fb").unwrap();
        assert!(
            s.overrides.is_empty(),
            "a cancelled instance is no override"
        );
        assert_eq!(s.master.exdates.len(), 1);
        assert_eq!(fmt_utc(s.master.exdates[0]), "20260921T090000Z");
    }

    #[test]
    fn from_ics_series_single_vevent_equals_from_ics() {
        let ics = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:one\r\n\
                   DTSTART:20260901T100000Z\r\nDTEND:20260901T110000Z\r\n\
                   SUMMARY:One-off\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let s = from_ics_series(ics, "fb").unwrap();
        assert!(s.overrides.is_empty());
        assert_eq!(Some(s.master), from_ics(ics, "fb"));
    }

    #[test]
    fn from_ics_series_ignores_vtimezone_and_dedupes_slots() {
        // Apple-style: a VTIMEZONE block (whose STANDARD/DAYLIGHT rules carry
        // their own DTSTART/RRULE lines) precedes the events; a duplicated
        // override slot keeps the last VEVENT.
        let ics = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\n\
                   BEGIN:VTIMEZONE\r\nTZID:Europe/Brussels\r\n\
                   BEGIN:DAYLIGHT\r\nDTSTART:19810329T020000\r\n\
                   RRULE:FREQ=YEARLY;BYMONTH=3;BYDAY=-1SU\r\nTZOFFSETFROM:+0100\r\n\
                   TZOFFSETTO:+0200\r\nEND:DAYLIGHT\r\n\
                   BEGIN:STANDARD\r\nDTSTART:19961027T030000\r\n\
                   RRULE:FREQ=YEARLY;BYMONTH=10;BYDAY=-1SU\r\nTZOFFSETFROM:+0200\r\n\
                   TZOFFSETTO:+0100\r\nEND:STANDARD\r\nEND:VTIMEZONE\r\n\
                   BEGIN:VEVENT\r\nUID:ser-3\r\nDTSTART;TZID=Europe/Brussels:20260907T090000\r\n\
                   DTEND;TZID=Europe/Brussels:20260907T093000\r\nRRULE:FREQ=WEEKLY\r\n\
                   SUMMARY:Weekly\r\nEND:VEVENT\r\n\
                   BEGIN:VEVENT\r\nUID:ser-3\r\nRECURRENCE-ID;TZID=Europe/Brussels:20260914T090000\r\n\
                   DTSTART;TZID=Europe/Brussels:20260914T100000\r\n\
                   DTEND;TZID=Europe/Brussels:20260914T103000\r\nSUMMARY:First edit\r\nEND:VEVENT\r\n\
                   BEGIN:VEVENT\r\nUID:ser-3\r\nRECURRENCE-ID;TZID=Europe/Brussels:20260914T090000\r\n\
                   DTSTART;TZID=Europe/Brussels:20260914T140000\r\n\
                   DTEND;TZID=Europe/Brussels:20260914T143000\r\nSUMMARY:Second edit\r\nEND:VEVENT\r\n\
                   END:VCALENDAR\r\n";
        let s = from_ics_series(ics, "fb").unwrap();
        // The VTIMEZONE's rules never bleed into the master.
        assert_eq!(s.master.summary, "Weekly");
        assert_eq!(s.master.recurrence.as_deref(), Some("FREQ=WEEKLY"));
        assert_eq!(s.master.timezone.as_deref(), Some("Europe/Brussels"));
        // 09:00 Brussels (CEST, UTC+2) → 07:00Z; the duplicate slot keeps the
        // last edit.
        assert_eq!(s.overrides.len(), 1);
        let (slot, ov) = &s.overrides[0];
        assert_eq!(fmt_utc(*slot), "20260914T070000Z");
        assert_eq!(ov.summary, "Second edit");
        assert_eq!(fmt_utc(ov.starts_at), "20260914T120000Z");
    }
}
