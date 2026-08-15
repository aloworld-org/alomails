//! Executing `meeting_prep` and `reschedule_event` — the two Agenda tools that
//! act on **one meeting the user names** (ADR 0034, queue item A2.6).
//!
//! Together in one file because they are one problem twice: before either can
//! do anything, the words a person said ("the Delaunay review", "standup") have
//! to become exactly one sitting in the diary, and getting that wrong means
//! briefing somebody on the wrong meeting or moving one they never mentioned.
//! [`resolve_meeting`] is that step, written once.
//!
//! Four rules shape the module:
//!
//! - **A meeting has no id the user knows.** The model passes the title
//!   verbatim and, when it must, the day; nothing here accepts an identifier,
//!   so nothing here can be pointed at a record the asker never saw.
//! - **A title that matches several sittings is a question.** A weekly standup
//!   is a dozen meetings over the next fortnight, and choosing the next one is
//!   a guess that reschedules the wrong Tuesday. The refusal lists the days.
//! - **A briefing is written from the meeting, not from its name.** The prep
//!   returns the event, the emails that match its title, and the text of what
//!   is attached to them — the same reach `attachment_read` has, through the
//!   caller's own [`alo_store::AccountStore::workspace_search`], because an
//!   Agenda agent is not offered Drive's tools and must not need them.
//! - **Moving a meeting moves the time and nothing else.** The title, the
//!   guests, the place, the notes and the reminder are carried across
//!   unchanged; one sitting of a series moves on its own (an iCalendar
//!   `RECURRENCE-ID` override) and the rest of the series stays where it is.
//!   Nothing here can cancel a meeting — deletion stays a human act in the
//!   calendar.

use axum::Json;
use serde_json::{Value, json};
use time::format_description::well_known::Rfc3339;
use time::{Duration, OffsetDateTime, Time};

use alo_store::{CalendarEvent, MessageId, OccurrenceOverride};

use crate::agent_args::{string_arg, unprocessable};
use crate::agent_drive::{DEFAULT_TEXT_CHARS, looks_textual, textual_type};
use crate::agent_reads::iso;
use crate::billing::{map_store_err, parse_iso_date};
use crate::error::Problem;
use crate::mime_read::{self, Attachment};
use crate::state::Account;

/// How far ahead a meeting is looked for when the user did not say a day. Far
/// enough to cover "the review" that is three weeks out, near enough that a
/// yearly series does not turn one title into a page of dates.
const LOOK_AHEAD_DAYS: i64 = 60;

/// …and how far back. A meeting is prepared before it happens, but "what did we
/// say we would do at yesterday's review" is an ordinary question.
const LOOK_BACK_DAYS: i64 = 7;

/// How many of the caller's emails a meeting's title is matched against, and
/// how many of them are opened. Opening one means parsing its MIME, so the two
/// numbers are deliberately different: a title can match a folder's worth of
/// mail, and a briefing is written from the nearest few.
const MAX_CANDIDATES: i64 = 10;
const MAX_OPENED: usize = 3;

/// The largest message this parses — the same ceiling `attachment_read` uses.
const MAX_MESSAGE_BYTES: usize = 25 * 1024 * 1024;

/// How much of an email's own body goes into a briefing.
const PREVIEW_CHARS: usize = 600;

/// How many attachments are read out in full, across the whole prep. A briefing
/// is a page, not a mailbox.
const MAX_ATTACHMENTS_READ: usize = 2;

/// `meeting_prep` — one meeting, the mail that goes with it, and what is
/// attached to that mail.
///
/// # Errors
/// 422 when no meeting was named, when nothing in the diary matches the name,
/// or when several sittings do; the store's own failure otherwise.
pub async fn execute_meeting_prep(account: &Account, args: &Value) -> Result<Json<Value>, Problem> {
    let event = resolve_meeting(account, args).await?;

    // The mail that mentions it, through the caller's own search — the same
    // door the search box is, so a colleague's correspondence is not among the
    // things that can come back.
    let hits = account
        .acc
        .workspace_search(&event.summary, MAX_CANDIDATES)
        .await
        .map_err(map_store_err)?;
    let mut thread = Vec::new();
    let mut read_out = 0usize;
    for hit in hits.into_iter().filter(|hit| hit.kind == "message") {
        let opened = thread.len() < MAX_OPENED;
        let message = account
            .acc
            .message(&MessageId::new(hit.id.clone()))
            .await
            .ok();
        let (from, at) = message.as_ref().map_or((String::new(), None), |message| {
            (
                message.from_addr.clone(),
                Some(message.sent_at.unwrap_or(message.received_at)),
            )
        });
        let mut entry = json!({
            "subject": hit.title,
            "from": from,
            "at": at.map(iso),
            // Said plainly, because an email that was listed and not opened is
            // not evidence of anything: the model must not brief from a subject
            // line as though it had read the message.
            "opened": opened,
        });
        if opened {
            // The message's bytes are fetched once and everything below is read
            // out of them: the body, what is attached, and the text of what can
            // be read — three round trips for one email would be three chances
            // to disagree about what it says.
            if let Some(raw) = raw_message(account, &hit.id).await {
                let parsed = mime_read::parse(&raw);
                let preview: String = parsed
                    .text
                    .unwrap_or_default()
                    .chars()
                    .take(PREVIEW_CHARS)
                    .collect();
                entry["preview"] = json!(preview);
                entry["attachments"] = json!(
                    parsed
                        .attachments
                        .iter()
                        .map(|part| json!({
                            "name": part.name,
                            "contentType": part.content_type,
                            "size": part.size,
                            "readable": readable(part),
                        }))
                        .collect::<Vec<_>>()
                );
                let mut texts = Vec::new();
                for part in parsed.attachments.iter().filter(|part| readable(part)) {
                    if read_out >= MAX_ATTACHMENTS_READ {
                        break;
                    }
                    if let Some((text, truncated)) = attachment_text(&raw, part) {
                        read_out += 1;
                        texts.push(json!({
                            "name": part.name,
                            "text": text,
                            "truncated": truncated,
                        }));
                    }
                }
                entry["attachmentText"] = json!(texts);
            }
        }
        thread.push(entry);
    }

    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "meetingPrep",
            "meeting": {
                "title": event.summary,
                "startsAt": iso(event.starts_at),
                "endsAt": iso(event.ends_at),
                "allDay": event.all_day,
                "location": event.location,
                "notes": event.description,
                "guests": event.attendees,
                // A sitting of a series, so the model can say "this Tuesday's"
                // rather than talking about the series as though it were one
                // meeting.
                "recurring": event.recurrence.is_some(),
            },
            "thread": thread,
            // What was actually searched for, so an empty thread reads as "no
            // mail matches this title" rather than as "there is nothing".
            "searchedFor": event.summary,
        }
    })))
}

/// `reschedule_event` — moving one meeting to a new time, and changing nothing
/// else about it. Runs only with the asker's own approval (ADR 0047 §1).
///
/// # Errors
/// 422 when no meeting was named or matched, when the new start is missing or
/// malformed, when the end is not after the start, when the meeting is an
/// all-day entry, or when the diary it sits in is not the caller's to edit.
pub async fn execute_reschedule_event(
    account: &Account,
    args: &Value,
) -> Result<Json<Value>, Problem> {
    let event = resolve_meeting(account, args).await?;
    let start_s = string_arg(args, "start").ok_or_else(|| unprocessable("start is required"))?;
    let starts_at = OffsetDateTime::parse(&start_s, &Rfc3339)
        .map_err(|_| unprocessable("start must be an RFC 3339 datetime"))?
        .to_offset(time::UtcOffset::UTC);
    // A move with no end keeps the meeting as long as it already was. Taking
    // `create_event`'s one-hour default here would quietly shorten a workshop.
    let ends_at = match string_arg(args, "end") {
        Some(end) => OffsetDateTime::parse(&end, &Rfc3339)
            .map_err(|_| unprocessable("end must be an RFC 3339 datetime"))?
            .to_offset(time::UtcOffset::UTC),
        None => starts_at + (event.ends_at - event.starts_at),
    };
    if ends_at <= starts_at {
        return Err(unprocessable("end is not after start"));
    }
    if event.all_day {
        return Err(unprocessable(format!(
            "{} is an all-day entry, so it has no time to move — change it in the calendar",
            event.summary
        )));
    }
    // Asked before anything is written, so a diary the caller may only read
    // earns a sentence rather than a store error the client cannot act on.
    if !account
        .acc
        .can_edit_calendar(&event.calendar_id)
        .await
        .map_err(map_store_err)?
    {
        return Err(unprocessable(format!(
            "{} is in a diary you can read but not change",
            event.summary
        )));
    }

    let was = (event.starts_at, event.ends_at);
    match event.recurrence_id {
        // One sitting of a repeating meeting: an override on its own slot, so
        // the rest of the series stays exactly where it is.
        Some(slot) => account
            .acc
            .override_occurrence(
                &event.id,
                slot,
                &OccurrenceOverride {
                    summary: event.summary.clone(),
                    description: event.description.clone(),
                    location: event.location.clone(),
                    starts_at,
                    ends_at,
                    all_day: event.all_day,
                },
            )
            .await
            .map_err(map_store_err)?,
        None => {
            let moved = CalendarEvent {
                starts_at,
                ends_at,
                ..event.clone()
            };
            account
                .acc
                .update_event(&event.id, &moved)
                .await
                .map_err(map_store_err)?;
        }
    }

    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "eventMoved",
            "id": event.id.as_str(),
            "title": event.summary,
            // Both ends of the move, so the room can say what changed rather
            // than only where it landed.
            "wasStartsAt": iso(was.0),
            "wasEndsAt": iso(was.1),
            "startsAt": iso(starts_at),
            "endsAt": iso(ends_at),
            "occurrenceOfSeries": event.recurrence_id.is_some(),
        }
    })))
}

/// The one sitting a name (and possibly a day) means, out of the diaries the
/// caller can see.
///
/// # Errors
/// 422 when nothing was named, when nothing matches, or when several sittings
/// do — the last listing their days, so the next turn can say which.
async fn resolve_meeting(account: &Account, args: &Value) -> Result<CalendarEvent, Problem> {
    let wanted = string_arg(args, "meeting")
        .ok_or_else(|| unprocessable("say which meeting, by its title"))?;
    let (from, to) = match string_arg(args, "on") {
        Some(day) => {
            let day = parse_iso_date(&day).ok_or_else(|| unprocessable("on must be YYYY-MM-DD"))?;
            (
                day.with_time(Time::MIDNIGHT).assume_utc(),
                (day + Duration::days(1))
                    .with_time(Time::MIDNIGHT)
                    .assume_utc(),
            )
        }
        None => {
            let today = OffsetDateTime::now_utc().date();
            (
                (today - Duration::days(LOOK_BACK_DAYS))
                    .with_time(Time::MIDNIGHT)
                    .assume_utc(),
                (today + Duration::days(LOOK_AHEAD_DAYS))
                    .with_time(Time::MIDNIGHT)
                    .assume_utc(),
            )
        }
    };
    let events = account
        .acc
        .events_in_range(from, to)
        .await
        .map_err(map_store_err)?;
    let matched = matching(&wanted, events);
    match matched.len() {
        0 => Err(unprocessable(format!(
            "no meeting of yours in the diary is called {wanted}"
        ))),
        1 => matched.into_iter().next().ok_or_else(Problem::server_error),
        _ => Err(unprocessable(format!(
            "{wanted} is in the diary more than once: {} — say which day",
            matched
                .iter()
                .take(6)
                .map(when)
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

/// The events a title names, by the resolution rule the rest of the agent uses:
/// an exact title wins, and failing that every title containing the words.
///
/// Pure, so what a name matches is testable without a diary.
fn matching(wanted: &str, events: Vec<CalendarEvent>) -> Vec<CalendarEvent> {
    let needle = wanted.trim().to_lowercase();
    let (exact, partial): (Vec<_>, Vec<_>) = events
        .into_iter()
        .filter(|event| event.summary.trim().to_lowercase().contains(&needle))
        .partition(|event| event.summary.trim().to_lowercase() == needle);
    if exact.is_empty() { partial } else { exact }
}

/// One sitting, in the words the "say which day" refusal uses.
fn when(event: &CalendarEvent) -> String {
    let at = event.starts_at;
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        at.year(),
        u8::from(at.month()),
        at.day(),
        at.hour(),
        at.minute()
    )
}

/// The raw bytes of one of the caller's messages — or `None` when it cannot be
/// opened or is too large to parse, which a briefing reports rather than fails
/// on: one unreadable email does not make the meeting unpreparable.
async fn raw_message(account: &Account, id: &str) -> Option<bytes::Bytes> {
    let raw = account
        .acc
        .message_bytes(&MessageId::new(id.to_owned()))
        .await
        .ok()?;
    (raw.len() <= MAX_MESSAGE_BYTES).then_some(raw)
}

/// The text of one attachment, and whether it was cut short. `None` when the
/// bytes turn out not to be text after all — a lossy decode reads like prose
/// and summarises like nonsense, which is the one failure that would make the
/// briefing a lie.
fn attachment_text(raw: &[u8], part: &Attachment) -> Option<(String, bool)> {
    let (bytes, _, _) = mime_read::attachment_bytes(raw, part.index)?;
    let text = String::from_utf8(bytes).ok()?;
    let truncated = text.chars().count() > DEFAULT_TEXT_CHARS;
    Some((text.chars().take(DEFAULT_TEXT_CHARS).collect(), truncated))
}

/// Whether an attachment's own headers say it is text — the same two facts
/// `attachment_read` weighs, asked here so the Agenda agent refuses exactly
/// what the Drive agent refuses.
fn readable(part: &Attachment) -> bool {
    textual_type(&part.content_type.to_lowercase()) || looks_textual(&part.name, None)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use alo_store::{CalendarId, EventId};
    use time::{Date, Month};

    fn at(day: u8, hour: u8) -> OffsetDateTime {
        Date::from_calendar_date(2026, Month::August, day)
            .unwrap()
            .with_hms(hour, 0, 0)
            .unwrap()
            .assume_utc()
    }

    fn event(title: &str, day: u8, hour: u8) -> CalendarEvent {
        CalendarEvent {
            id: EventId::new(format!("e-{title}-{day}")),
            calendar_id: CalendarId::new("cal"),
            summary: title.to_owned(),
            description: None,
            location: None,
            starts_at: at(day, hour),
            ends_at: at(day, hour + 1),
            all_day: false,
            recurrence: None,
            attendees: Vec::new(),
            exdates: Vec::new(),
            recurrence_id: None,
            reminder_minutes: None,
            attendee_status: Vec::new(),
        }
    }

    fn diary() -> Vec<CalendarEvent> {
        vec![
            event("Delaunay review", 17, 10),
            event("Delaunay review prep", 16, 9),
            event("Standup", 17, 9),
            event("Standup", 18, 9),
        ]
    }

    /// An exact title wins over one that merely contains it — so "Delaunay
    /// review" reaches the review and not the prep meeting that shares its
    /// words.
    #[test]
    fn an_exact_title_wins_over_one_that_contains_it() {
        let found = matching("delaunay review", diary());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].summary, "Delaunay review");
        // …and a fragment only one meeting has is that meeting.
        assert_eq!(matching("prep", diary()).len(), 1);
        assert!(matching("hovercraft", diary()).is_empty());
    }

    /// The rule the whole resolution exists for: a repeating meeting is several
    /// sittings, and picking one of them is a guess that moves the wrong
    /// Tuesday.
    #[test]
    fn a_title_in_the_diary_twice_matches_twice() {
        let found = matching("Standup", diary());
        assert_eq!(found.len(), 2);
        // And the days a refusal would name are the days themselves.
        assert_eq!(
            found.iter().map(when).collect::<Vec<_>>(),
            ["2026-08-17 09:00", "2026-08-18 09:00"]
        );
    }

    /// A meeting matched by a word in the middle of its title, whatever the
    /// case or the blanks around it.
    #[test]
    fn a_title_is_matched_case_and_blank_insensitively() {
        assert_eq!(matching("  DELAUNAY REVIEW  ", diary()).len(), 1);
        assert_eq!(matching("review", diary()).len(), 2);
    }

    /// What the briefing will read out in full, and what it will only name.
    /// The same answer `attachment_read` gives, because a person asking the
    /// Agenda agent should not get a different one.
    #[test]
    fn an_attachment_is_read_by_its_type_or_its_name_and_a_pdf_is_neither() {
        let part = |name: &str, content_type: &str| Attachment {
            index: 0,
            name: name.to_owned(),
            content_type: content_type.to_owned(),
            size: 10,
            content_id: None,
            inline: false,
        };
        assert!(readable(&part("agenda.txt", "text/plain")));
        assert!(readable(&part("figures.csv", "text/csv")));
        assert!(readable(&part("notes.md", "application/octet-stream")));
        assert!(!readable(&part("board-pack.pdf", "application/pdf")));
        assert!(!readable(&part("slides.pptx", "application/octet-stream")));
    }
}
