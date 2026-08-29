//! Drafting a week of timesheet suggestions from the caller's own Agenda
//! (ADR 0034, ADR 0035 wave B3.10b) — the acting half of
//! [`alo_ai::agent_projects`]'s `draft_timesheet_from_calendar`.
//!
//! Its own module rather than a third executor in [`crate::agent_projects`],
//! because it is a different kind of thing: the two tools there each touch one
//! record, and this one turns a *period of somebody's diary* into a set of
//! proposals. The deciding is pure ([`plan_drafts`]) and the writing is a loop
//! over what it decided, so the rules below are testable without a calendar, a
//! store or a model.
//!
//! Four rules, each of them a mistake it exists to prevent:
//!
//! - **A meeting is evidence of an hour, never of a project.** The engagement
//!   comes from the user's own words and is resolved among the boards they can
//!   see; only the *days* are read from the diary. A project inferred from a
//!   meeting's title would eventually charge one customer for a call with
//!   another, and nothing downstream would catch it.
//! - **Every entry is a proposal, all the way down.** Each lands `proposed`
//!   ([`alo_store::NewTimeEntry::proposed`]) with no rate, in no total and in no
//!   submitted week until the person whose timesheet it is accepts it
//!   (ADR 0023). Drafting twenty of them at once is exactly why: a batch a
//!   machine could file would be a batch nobody reads.
//! - **Running it twice drafts nothing twice.** Each entry remembers the
//!   occurrence it came from (`source_kind = "event"`, [`source_id`]), and an
//!   occurrence already drafted — accepted, pending or even long since
//!   invoiced — is skipped and said so. A tool that doubles somebody's Tuesday
//!   every time they ask is a tool they use once.
//! - **What was left out is part of the answer.** All-day entries, meetings in a
//!   week already submitted, a diary emptier than the user expected: each comes
//!   back as a skipped line with a machine-readable reason the client writes
//!   words for. Silence would read as "your calendar had nothing", which is a
//!   different and often wrong statement.
//!
//! The result carries figures and reason codes only — never a sentence. A
//! sentence composed here would be a user-facing string authored in the server
//! in one language, which is a bug in a European product (CLAUDE.md).
//!
//! **Days are UTC days**, as everywhere else on this surface: an event is filed
//! under the day it *starts* in UTC, and the range's own bounds are UTC
//! midnights. The whole suite's "today" is UTC (B3.09a, B3.10a) and one tool
//! inventing a second answer would be worse than the shared, stated one.

use std::collections::HashSet;

use axum::Json;
use serde_json::{Value, json};
use time::{Date, Duration, OffsetDateTime};

use alo_store::{
    CalendarEvent, MINUTES_MAX, NewTimeEntry, ProjectId, TimeEntry, week_start as monday_of,
};

use crate::agent_args::{string_arg, unprocessable};
use crate::agent_projects::resolve_project;
use crate::billing::{iso_date, map_store_err, parse_iso_date};
use crate::error::Problem;
use crate::state::Account;

/// The longest range a single call may cover, in days, both ends included.
///
/// A month is the period a person actually forgets to log; a year is a request
/// that would write hundreds of proposals nobody will read through. The model
/// is told the same number ([`alo_ai::agent_projects`]) so a wider ask is
/// narrowed in the conversation rather than refused after the fact.
const MAX_RANGE_DAYS: i64 = 31;

/// The most entries one call will draft. Beyond it the rest of the diary is
/// returned as skipped lines carrying [`REASON_LIMIT`] — never silently
/// dropped, because a truncated answer that looks complete is the one failure
/// a person cannot see.
const MAX_DRAFTS: usize = 50;

/// The source kind every drafted entry carries, and the marker the de-duplicating
/// read matches on.
const SOURCE_EVENT: &str = "event";

/// Left out: an all-day entry. A day marked "Leave" or "Berlin" is not an hour
/// worked, and it has no duration to log anyway.
const REASON_ALL_DAY: &str = "allDay";

/// Left out: this occurrence already has an entry drafted from it.
const REASON_ALREADY: &str = "alreadyDrafted";

/// Left out: the meeting has no length (it ends when it starts).
const REASON_NO_DURATION: &str = "noDuration";

/// Left out: the meeting is longer than a day, which no single entry can hold
/// ([`alo_store::MINUTES_MAX`]) — a multi-day block is a period, not a sitting.
const REASON_TOO_LONG: &str = "tooLong";

/// Left out: its week is submitted or approved, so no hour may be added to it —
/// the store's own rule, applied before writing rather than discovered halfway
/// through a batch.
const REASON_WEEK_LOCKED: &str = "weekLocked";

/// Left out: [`MAX_DRAFTS`] were already drafted in this call.
const REASON_LIMIT: &str = "limitReached";

/// Left out: it starts outside the asked-for range. `events_in_range` answers
/// with everything *overlapping* the window, so a meeting that began the
/// evening before reaches this code; it belongs to that earlier day's
/// timesheet, not to this range's.
const REASON_OUTSIDE: &str = "outsideRange";

/// One entry this call will write, decided but not yet written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft {
    /// The occurrence it came from, in [`source_id`]'s spelling.
    pub source_id: String,
    /// The meeting's title, which becomes the entry's note.
    pub summary: String,
    /// The UTC day it is filed under.
    pub work_date: Date,
    /// When the meeting began — provenance only, never a period boundary.
    pub starts_at: OffsetDateTime,
    /// How long it ran, in whole minutes.
    pub minutes: i64,
    /// Whether it overlaps the meeting drafted before it. Flagged, never
    /// resolved: two calls at once is a real thing that happens to people, and
    /// which of them was the work is theirs to say.
    pub overlaps: bool,
}

/// One meeting this call will not write an entry for, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skipped {
    /// The meeting's title, so the user recognises what was left out.
    pub summary: String,
    /// The UTC day it starts on.
    pub day: Date,
    /// One of the `REASON_*` codes. Machine-readable on purpose: the words are
    /// the client's, in the reader's language.
    pub reason: &'static str,
}

/// What a call decided to do with a period of the diary.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    /// The entries to write, earliest first.
    pub drafts: Vec<Draft>,
    /// Everything else that was in the window, with its reason.
    pub skipped: Vec<Skipped>,
}

/// `draft_timesheet_from_calendar` — one proposed entry per meeting in the
/// caller's own Agenda over a range of days.
///
/// The order is: resolve the project, read the range, decide the whole plan,
/// then write it. Deciding before writing is what makes a partial batch
/// impossible to *mean* something different from a whole one — every refusal a
/// meeting can earn is known before the first row exists.
///
/// # Errors
/// `422` when the project cannot be resolved to exactly one board, when a bound
/// is missing or is not a plain `YYYY-MM-DD`, when the range runs backwards or
/// is longer than [`MAX_RANGE_DAYS`]; the store's own `404`/`409`/`422`
/// otherwise. A store refusal partway through leaves the proposals already
/// written in place — they are suggestions in nobody's total, and the user
/// rejects them in their timesheet like any other.
pub async fn execute_draft_timesheet_from_calendar(
    account: &Account,
    args: &Value,
) -> Result<Json<Value>, Problem> {
    let project = resolve_project(account, args).await?;
    let (from, to) = range(args)?;
    let billable = args
        .get("billable")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let project_id = ProjectId::new(project.id.as_str());

    let events = account
        .acc
        .events_in_range(midnight(from), midnight(to) + Duration::days(1))
        .await
        .map_err(map_store_err)?;
    // Already drafted: every entry of the caller's own in this range that
    // remembers an event, whatever state it reached since — an accepted or even
    // invoiced hour must not be suggested a second time.
    let existing = account
        .acc
        .time_entries(from, to, None)
        .await
        .map_err(map_store_err)?;
    // Locked weeks, read once for the range rather than discovered per row:
    // the store refuses an hour in a submitted week, and a batch that stopped
    // at the first refusal would leave the rest of the diary unexplained.
    let weeks = account
        .acc
        .timesheet_weeks(from, to)
        .await
        .map_err(map_store_err)?;
    let locked: HashSet<Date> = weeks
        .iter()
        .filter(|week| week.status.is_locked())
        .map(|week| week.week_start)
        .collect();

    let plan = plan_drafts(&events, from, to, &drafted_sources(&existing), &locked);

    let mut written = Vec::with_capacity(plan.drafts.len());
    for draft in &plan.drafts {
        let new = NewTimeEntry {
            billable,
            note: draft.summary.clone(),
            started_at: Some(draft.starts_at),
            // The whole point of the tool: suggestions, in no total until the
            // person whose week it is accepts each one.
            proposed: true,
            source_kind: Some(SOURCE_EVENT.to_owned()),
            source_id: Some(draft.source_id.clone()),
            ..NewTimeEntry::worked(project_id.clone(), draft.work_date, draft.minutes)
        };
        let entry = account.acc.log_time(&new).await.map_err(map_store_err)?;
        written.push((draft, entry));
    }

    crate::billing_intents::ok(json!({
        "kind": "timesheetDraft",
        "id": project.id.as_str(),
        "title": project.name,
        "from": iso_date(from),
        "to": iso_date(to),
        "drafted": written
                .iter()
                .map(|(draft, entry)| json!({
                    "id": entry.id.as_str(),
                    "workDate": iso_date(entry.work_date),
                    "minutes": entry.minutes,
                    "note": entry.note,
                    "overlaps": draft.overlaps,
                }))
                .collect::<Vec<_>>(),
            // The batch's own figures, so the receipt states them rather than
            // adding up the list it just printed.
            "minutes": written.iter().map(|(_, entry)| entry.minutes).sum::<i64>(),
            "overlaps": plan.drafts.iter().filter(|d| d.overlaps).count(),
            "billable": billable,
            "skipped": plan
                .skipped
                .iter()
                .map(|skipped| json!({
                    "summary": skipped.summary,
                    "day": iso_date(skipped.day),
                    "reason": skipped.reason,
                }))
                .collect::<Vec<_>>(),
    }))
}

/// Which occurrences already have an entry of the caller's in this period.
///
/// Any state counts, and that is the point: a proposal still waiting, an hour
/// accepted last week and an hour already on an invoice all mean the same thing
/// here — this meeting has been dealt with.
fn drafted_sources(existing: &[TimeEntry]) -> HashSet<String> {
    existing
        .iter()
        .filter(|entry| entry.source_kind.as_deref() == Some(SOURCE_EVENT))
        .filter_map(|entry| entry.source_id.clone())
        .collect()
}

/// The whole decision, as pure code: which meetings become entries, which do
/// not, and which of the entries sit on top of one another.
///
/// Events arrive earliest-first from the store and are re-sorted here anyway,
/// because the overlap flag is only meaningful in time order and this function
/// is called by tests that build their own lists.
#[must_use]
pub fn plan_drafts(
    events: &[CalendarEvent],
    from: Date,
    to: Date,
    already: &HashSet<String>,
    locked_weeks: &HashSet<Date>,
) -> Plan {
    let mut ordered: Vec<&CalendarEvent> = events.iter().collect();
    ordered.sort_by_key(|event| (event.starts_at, event.id.as_str().to_owned()));

    let mut plan = Plan::default();
    // The end of the last meeting drafted, which is what "overlaps" is about.
    let mut previous_end: Option<OffsetDateTime> = None;
    // Occurrences dealt with earlier in this same call: a diary can hold the
    // same occurrence twice only through a bug, but a batch that wrote it twice
    // would be a worse one.
    let mut seen: HashSet<String> = HashSet::new();

    for event in ordered {
        let day = event.starts_at.date();
        let source = source_id(event);
        let skip = |reason| Skipped {
            summary: event.summary.clone(),
            day,
            reason,
        };
        if day < from || day > to {
            plan.skipped.push(skip(REASON_OUTSIDE));
            continue;
        }
        if event.all_day {
            plan.skipped.push(skip(REASON_ALL_DAY));
            continue;
        }
        let minutes = (event.ends_at - event.starts_at).whole_minutes();
        if minutes < 1 {
            plan.skipped.push(skip(REASON_NO_DURATION));
            continue;
        }
        if minutes > MINUTES_MAX {
            plan.skipped.push(skip(REASON_TOO_LONG));
            continue;
        }
        // Before the week lock, deliberately: a meeting that is already in the
        // timesheet is *there*, and saying "that week is submitted" about an
        // hour the person submitted themselves would send them looking for a
        // problem that does not exist.
        if already.contains(&source) || !seen.insert(source.clone()) {
            plan.skipped.push(skip(REASON_ALREADY));
            continue;
        }
        if locked_weeks.contains(&monday_of(day)) {
            plan.skipped.push(skip(REASON_WEEK_LOCKED));
            continue;
        }
        if plan.drafts.len() >= MAX_DRAFTS {
            plan.skipped.push(skip(REASON_LIMIT));
            continue;
        }
        plan.drafts.push(Draft {
            source_id: source,
            summary: event.summary.clone(),
            work_date: day,
            starts_at: event.starts_at,
            minutes,
            overlaps: previous_end.is_some_and(|end| event.starts_at < end),
        });
        previous_end = Some(event.ends_at.max(previous_end.unwrap_or(event.ends_at)));
    }
    plan
}

/// The stable handle of one *occurrence* of an event.
///
/// A one-off is its own id. Every occurrence of a recurring series shares that
/// id, so the slot it fills is appended — without it, the weekly stand-up would
/// be drafted once and then reported as already done for the rest of the month.
/// The slot is the `RECURRENCE-ID` when the store expanded one, and the start
/// otherwise, which is the same instant for an untouched occurrence.
fn source_id(event: &CalendarEvent) -> String {
    if event.recurrence.is_none() && event.recurrence_id.is_none() {
        return event.id.as_str().to_owned();
    }
    let slot = event.recurrence_id.unwrap_or(event.starts_at);
    format!("{}@{}", event.id.as_str(), stamp(slot))
}

/// A UTC instant in the fixed, sortable spelling the source handle uses.
/// Independent of any locale, and short enough to stay inside the store's
/// source-id bound.
fn stamp(at: OffsetDateTime) -> String {
    at.format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| at.unix_timestamp().to_string())
}

/// The UTC midnight that opens a day.
fn midnight(day: Date) -> OffsetDateTime {
    day.midnight().assume_utc()
}

/// The range a call covers: `from`, and `to` — which defaults to the same day,
/// because "draft yesterday from my calendar" is the commonest ask and should
/// not require the same date twice.
///
/// # Errors
/// `422` when a bound is missing or is not a plain `YYYY-MM-DD`, when the range
/// runs backwards, or when it is longer than [`MAX_RANGE_DAYS`].
fn range(args: &Value) -> Result<(Date, Date), Problem> {
    let from = day_arg(args, &["from", "date", "day", "start"])?
        .ok_or_else(|| unprocessable("the first day of the range is required"))?;
    let to = day_arg(args, &["to", "until", "end"])?.unwrap_or(from);
    if to < from {
        return Err(unprocessable(
            "the last day of the range must not be before its first",
        ));
    }
    let days = (to - from).whole_days() + 1;
    if days > MAX_RANGE_DAYS {
        return Err(unprocessable(format!(
            "a calendar draft covers at most {MAX_RANGE_DAYS} days at a time, and this asks for \
             {days} — draft one month at a time"
        )));
    }
    Ok((from, to))
}

/// The first of several spellings of a day, read as a plain `YYYY-MM-DD`.
///
/// Several spellings because a model reaches for whichever word the user used;
/// one meaning, because a date that is not exactly a day is refused rather than
/// truncated (`crate::billing::parse_iso_date` argues that at length).
fn day_arg(args: &Value, keys: &[&str]) -> Result<Option<Date>, Problem> {
    for key in keys {
        let Some(stated) = string_arg(args, key) else {
            continue;
        };
        return parse_iso_date(&stated)
            .map(Some)
            .ok_or_else(|| unprocessable(format!("{key} must be a day written YYYY-MM-DD")));
    }
    Ok(None)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    use alo_store::{CalendarId, EventId};

    fn day(iso: &str) -> Date {
        parse_iso_date(iso).expect("a plain day")
    }

    fn at(iso: &str) -> OffsetDateTime {
        OffsetDateTime::parse(iso, &time::format_description::well_known::Rfc3339)
            .expect("an RFC 3339 instant")
    }

    /// A plain timed meeting.
    fn meeting(id: &str, summary: &str, starts: &str, ends: &str) -> CalendarEvent {
        CalendarEvent {
            id: EventId::new(id),
            calendar_id: CalendarId::new("cal"),
            summary: summary.to_owned(),
            description: None,
            location: None,
            starts_at: at(starts),
            ends_at: at(ends),
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

    fn week(monday: &str) -> HashSet<Date> {
        HashSet::from([day(monday)])
    }

    #[test]
    fn a_meeting_becomes_one_entry_on_the_day_it_started() {
        let events = [meeting(
            "e1",
            "Kickoff with Hansen",
            "2026-08-03T09:00:00Z",
            "2026-08-03T10:30:00Z",
        )];
        let plan = plan_drafts(
            &events,
            day("2026-08-03"),
            day("2026-08-07"),
            &HashSet::new(),
            &HashSet::new(),
        );
        assert_eq!(plan.skipped, Vec::new());
        assert_eq!(plan.drafts.len(), 1);
        let draft = &plan.drafts[0];
        assert_eq!(draft.minutes, 90, "an hour and a half, in whole minutes");
        assert_eq!(draft.work_date, day("2026-08-03"));
        assert_eq!(draft.summary, "Kickoff with Hansen");
        assert_eq!(draft.source_id, "e1");
        assert!(!draft.overlaps);
    }

    #[test]
    fn what_is_left_out_says_why_rather_than_vanishing() {
        let mut all_day = meeting(
            "hol",
            "Leave",
            "2026-08-04T00:00:00Z",
            "2026-08-05T00:00:00Z",
        );
        all_day.all_day = true;
        let events = [
            all_day,
            meeting("z", "Zero", "2026-08-04T09:00:00Z", "2026-08-04T09:00:00Z"),
            meeting(
                "long",
                "Offsite",
                "2026-08-04T08:00:00Z",
                "2026-08-06T18:00:00Z",
            ),
            meeting(
                "before",
                "Late call yesterday",
                "2026-08-02T22:00:00Z",
                "2026-08-03T01:00:00Z",
            ),
            meeting(
                "ok",
                "Review",
                "2026-08-04T11:00:00Z",
                "2026-08-04T12:00:00Z",
            ),
        ];
        let plan = plan_drafts(
            &events,
            day("2026-08-03"),
            day("2026-08-07"),
            &HashSet::new(),
            &HashSet::new(),
        );
        assert_eq!(plan.drafts.len(), 1, "only the real meeting");
        assert_eq!(plan.drafts[0].summary, "Review");
        let reasons: Vec<&str> = plan.skipped.iter().map(|s| s.reason).collect();
        assert!(reasons.contains(&REASON_ALL_DAY));
        assert!(reasons.contains(&REASON_NO_DURATION));
        assert!(reasons.contains(&REASON_TOO_LONG));
        // The meeting that ran over midnight belongs to the day it started on,
        // which is outside the asked-for range.
        assert!(reasons.contains(&REASON_OUTSIDE));
        // Each skipped line names what was left out, so a person can see it.
        assert!(plan.skipped.iter().all(|s| !s.summary.is_empty()));
    }

    #[test]
    fn a_meeting_already_drafted_is_never_drafted_twice() {
        let events = [
            meeting(
                "e1",
                "Kickoff",
                "2026-08-03T09:00:00Z",
                "2026-08-03T10:00:00Z",
            ),
            meeting(
                "e2",
                "Review",
                "2026-08-04T09:00:00Z",
                "2026-08-04T10:00:00Z",
            ),
        ];
        let already = HashSet::from(["e1".to_owned()]);
        let plan = plan_drafts(
            &events,
            day("2026-08-03"),
            day("2026-08-07"),
            &already,
            &HashSet::new(),
        );
        assert_eq!(plan.drafts.len(), 1);
        assert_eq!(plan.drafts[0].source_id, "e2");
        assert_eq!(plan.skipped.len(), 1);
        assert_eq!(plan.skipped[0].reason, REASON_ALREADY);
        // Running the whole thing again drafts nothing at all — the second run
        // is where a doubled Tuesday would come from.
        let both = HashSet::from(["e1".to_owned(), "e2".to_owned()]);
        let again = plan_drafts(
            &events,
            day("2026-08-03"),
            day("2026-08-07"),
            &both,
            &HashSet::new(),
        );
        assert!(again.drafts.is_empty());
        assert_eq!(again.skipped.len(), 2);
    }

    #[test]
    fn every_occurrence_of_a_series_is_its_own_meeting() {
        let mut first = meeting(
            "series",
            "Stand-up",
            "2026-08-03T09:00:00Z",
            "2026-08-03T09:15:00Z",
        );
        first.recurrence = Some("FREQ=WEEKLY".to_owned());
        let mut second = first.clone();
        second.starts_at = at("2026-08-10T09:00:00Z");
        second.ends_at = at("2026-08-10T09:15:00Z");
        second.recurrence_id = Some(at("2026-08-10T09:00:00Z"));
        let plan = plan_drafts(
            &[first, second],
            day("2026-08-03"),
            day("2026-08-14"),
            &HashSet::new(),
            &HashSet::new(),
        );
        // Two entries, not one: the occurrences share an id, so the slot is what
        // tells them apart.
        assert_eq!(plan.drafts.len(), 2);
        assert_ne!(plan.drafts[0].source_id, plan.drafts[1].source_id);
        assert!(plan.drafts[0].source_id.starts_with("series@"));
        // …and the handle is stable, so a second run skips them both.
        let already: HashSet<String> = plan.drafts.iter().map(|d| d.source_id.clone()).collect();
        let again = plan_drafts(
            &[meeting(
                "other",
                "Ad hoc",
                "2026-08-05T09:00:00Z",
                "2026-08-05T10:00:00Z",
            )],
            day("2026-08-03"),
            day("2026-08-14"),
            &already,
            &HashSet::new(),
        );
        assert_eq!(again.drafts.len(), 1, "a one-off is unaffected");
    }

    #[test]
    fn meetings_on_top_of_one_another_are_flagged_and_not_resolved() {
        let events = [
            meeting(
                "a",
                "Design call",
                "2026-08-03T09:00:00Z",
                "2026-08-03T10:00:00Z",
            ),
            meeting(
                "b",
                "Client call",
                "2026-08-03T09:30:00Z",
                "2026-08-03T10:30:00Z",
            ),
            meeting("c", "Retro", "2026-08-03T11:00:00Z", "2026-08-03T11:30:00Z"),
        ];
        let plan = plan_drafts(
            &events,
            day("2026-08-03"),
            day("2026-08-03"),
            &HashSet::new(),
            &HashSet::new(),
        );
        // All three are drafted — double-booking is a real thing that happens to
        // people, and which of the two was the work is theirs to say.
        assert_eq!(plan.drafts.len(), 3);
        assert!(!plan.drafts[0].overlaps);
        assert!(plan.drafts[1].overlaps, "the second sits on the first");
        assert!(!plan.drafts[2].overlaps, "the third is clear of both");
    }

    #[test]
    fn a_submitted_week_takes_no_new_hours_and_says_so_before_writing_any() {
        let events = [
            // The week of Monday 27 July, which this person has submitted…
            meeting(
                "a",
                "Last week",
                "2026-07-30T09:00:00Z",
                "2026-07-30T10:00:00Z",
            ),
            // …and the open one after it.
            meeting(
                "b",
                "This week",
                "2026-08-04T09:00:00Z",
                "2026-08-04T10:00:00Z",
            ),
        ];
        let plan = plan_drafts(
            &events,
            day("2026-07-27"),
            day("2026-08-07"),
            &HashSet::new(),
            &week("2026-07-27"),
        );
        assert_eq!(plan.drafts.len(), 1);
        assert_eq!(plan.drafts[0].summary, "This week");
        assert_eq!(plan.skipped.len(), 1);
        assert_eq!(plan.skipped[0].reason, REASON_WEEK_LOCKED);

        // An hour already in that submitted week reads as what it is — already
        // there — rather than as the lock, which would send the person looking
        // for a problem they do not have.
        let already = HashSet::from(["a".to_owned()]);
        let plan = plan_drafts(
            &events,
            day("2026-07-27"),
            day("2026-08-07"),
            &already,
            &week("2026-07-27"),
        );
        assert_eq!(plan.skipped[0].reason, REASON_ALREADY);
    }

    #[test]
    fn a_diary_bigger_than_the_batch_reports_the_rest_rather_than_dropping_it() {
        let events: Vec<CalendarEvent> = (0..MAX_DRAFTS + 5)
            .map(|i| {
                let hour = i % 24;
                let day_of_month = 1 + i / 24;
                meeting(
                    &format!("e{i}"),
                    "Call",
                    &format!("2026-08-{day_of_month:02}T{hour:02}:00:00Z"),
                    &format!("2026-08-{day_of_month:02}T{hour:02}:30:00Z"),
                )
            })
            .collect();
        let plan = plan_drafts(
            &events,
            day("2026-08-01"),
            day("2026-08-31"),
            &HashSet::new(),
            &HashSet::new(),
        );
        assert_eq!(plan.drafts.len(), MAX_DRAFTS);
        assert_eq!(plan.skipped.len(), 5);
        assert!(plan.skipped.iter().all(|s| s.reason == REASON_LIMIT));
    }

    #[test]
    fn a_range_is_one_day_unless_a_second_is_stated_and_is_never_a_year() {
        let one = range(&json!({ "from": "2026-08-03" })).unwrap();
        assert_eq!(one, (day("2026-08-03"), day("2026-08-03")));
        let week = range(&json!({ "from": "2026-08-03", "to": "2026-08-09" })).unwrap();
        assert_eq!(week, (day("2026-08-03"), day("2026-08-09")));
        // The spellings a model reaches for all mean the same range.
        assert_eq!(
            range(&json!({ "date": "2026-08-03", "until": "2026-08-09" })).unwrap(),
            week
        );
        // A missing start is a refusal, never "today": a range nobody stated is
        // a batch of hours nobody asked for.
        let problem = range(&json!({ "to": "2026-08-09" })).expect_err("refused");
        assert_eq!(problem.status, axum::http::StatusCode::UNPROCESSABLE_ENTITY);
        // Backwards is a refusal…
        assert!(range(&json!({ "from": "2026-08-09", "to": "2026-08-03" })).is_err());
        // …and so is a range longer than a month, with the number in the words.
        let problem = range(&json!({ "from": "2026-01-01", "to": "2026-12-31" }))
            .expect_err("a year is refused");
        let detail = problem.detail.unwrap_or_default();
        assert!(detail.contains("at most 31 days"), "{detail}");
        // Exactly the bound is allowed: a month is the period people forget.
        assert!(range(&json!({ "from": "2026-08-01", "to": "2026-08-31" })).is_ok());
        for bad in ["yesterday", "03/08/2026", "2026-08-03T09:00:00Z"] {
            assert!(
                range(&json!({ "from": bad })).is_err(),
                "{bad} is not a day"
            );
        }
    }
}
