//! The executors of alo Meet's verbs (ADR 0058, AC.2) — what runs when the
//! Meet agent uses one of the intents `alo_ai::meet_intents` describes.
//!
//! Every executor runs through the asker's account door. The three verbs the
//! old tool set already had keep their executors in [`crate::agent_meet`] —
//! the record of a sitting is that module's subject matter — and are
//! dispatched from here so the agent has one place to look. What this module
//! itself executes is the *before*: the meetings ahead in the asker's own
//! diary, one of them looked up with its notes, and a new one scheduled.
//!
//! Two seams are deliberate reuse rather than new reach:
//!
//! - **A meeting ahead is resolved exactly as the Agenda agent resolves one**
//!   ([`crate::agent_meeting::resolve_meeting`]): title in the asker's own
//!   words, day when the title matches twice, the refusal listing the days.
//!   Two resolution rules for one diary would drift, and the drift would be a
//!   lookup answering about a different sitting than a reschedule moves.
//! - **Scheduling is the Agenda module's own calendar write**
//!   ([`crate::agent::execute_create_event`]), run as the asker once they
//!   approve — AC.2's "Agenda's intent called as the asker". The event lands
//!   in their personal calendar, invites nobody, and is the same record the
//!   calendar's own button makes; there is no second way to put something in
//!   a diary.

use serde_json::{Value, json};
use time::OffsetDateTime;

use crate::agent_args::{string_arg, unprocessable};
use crate::agent_reads::iso;
use crate::billing::map_store_err;
use crate::error::Problem;
use crate::state::{Account, AppState};

/// How far ahead `upcoming_meetings` looks, and the ceiling a caller may raise
/// it to — the same horizon [`crate::agent_meeting`] resolves a named meeting
/// in, so what the listing shows and what a lookup can name agree.
const DEFAULT_AHEAD_DAYS: i64 = 14;
const MAX_AHEAD_DAYS: i64 = 60;

/// How many entries one listing reports. A diary is not an answer: naming the
/// next few sittings is what lets the next turn say which one it means.
const MAX_LISTED: usize = 25;

type Reply = Result<axum::Json<Value>, Problem>;

fn ok(result: Value) -> Reply {
    Ok(axum::Json(json!({ "ok": true, "result": result })))
}

/// One diary entry, in the fields a listing reports — the day repeated on its
/// own because it is the word the next turn passes back as `day`.
fn entry_json(event: &alo_store::CalendarEvent) -> Value {
    json!({
        "title": event.summary,
        "day": event.starts_at.date().to_string(),
        "startsAt": iso(event.starts_at),
        "endsAt": iso(event.ends_at),
        "allDay": event.all_day,
        "location": event.location,
        // A sitting of a series, so the model can say "this Tuesday's" rather
        // than talking about the series as though it were one meeting.
        "recurring": event.recurrence.is_some() || event.recurrence_id.is_some(),
    })
}

/// `upcoming_meetings` — the diary ahead, counted from the record.
///
/// # Errors
/// The store's own failure. There is nothing to refuse: an empty list is the
/// honest answer for an empty diary.
pub async fn execute_upcoming_meetings(account: &Account, args: &Value) -> Reply {
    let days = args
        .get("days")
        .and_then(Value::as_i64)
        .unwrap_or(DEFAULT_AHEAD_DAYS)
        .clamp(1, MAX_AHEAD_DAYS);
    let now = OffsetDateTime::now_utc();
    let events = account
        .acc
        .events_in_range(now, now + time::Duration::days(days))
        .await
        .map_err(map_store_err)?;
    ok(json!({
        "kind": "upcomingMeetings",
        "daysAhead": days,
        "total": events.len(),
        "meetings": events.iter().take(MAX_LISTED).map(entry_json).collect::<Vec<_>>(),
        "truncated": events.len() > MAX_LISTED,
    }))
}

/// `meeting_lookup` — one meeting in the asker's diary, with its notes, and
/// whether a sitting of it has already happened and left a record.
///
/// # Errors
/// 422 when no meeting was named, the day is not a date, nothing matches, or
/// several sittings do — the last listing the days.
pub async fn execute_meeting_lookup(account: &Account, args: &Value) -> Reply {
    let wanted = string_arg(args, "meeting")
        .ok_or_else(|| unprocessable("say which meeting, by its title"))?;
    if let Some(day) = string_arg(args, "day") {
        // Checked here so the refusal names this module's own argument; the
        // shared resolver would say "on".
        crate::billing::parse_iso_date(&day)
            .ok_or_else(|| unprocessable("day must be YYYY-MM-DD"))?;
    }
    // This module's verbs say `day`, as the record reads do; the shared
    // resolver grew up under the Agenda agent and reads `on`. Translated here,
    // so the two modules keep one resolution rule without one of them renaming
    // an argument its models already learned.
    let resolved = crate::agent_meeting::resolve_meeting(
        account,
        &json!({ "meeting": wanted, "on": args.get("day").cloned().unwrap_or(Value::Null) }),
    )
    .await?;
    // Whether a sitting of it already ran: the meeting record on this event,
    // if anybody started one. Null is the ordinary answer for a meeting still
    // ahead — the model is told here rather than finding out at meeting_record.
    let record = account
        .acc
        .meeting_for_event(&resolved.id)
        .await
        .map_err(map_store_err)?;
    ok(json!({
        "kind": "meetingLookup",
        "title": resolved.summary,
        "day": resolved.starts_at.date().to_string(),
        "startsAt": iso(resolved.starts_at),
        "endsAt": iso(resolved.ends_at),
        "allDay": resolved.all_day,
        "location": resolved.location,
        "notes": resolved.description,
        "guests": resolved.attendees,
        "recurring": resolved.recurrence.is_some() || resolved.recurrence_id.is_some(),
        "record": record.map(|meeting| json!({
            "live": meeting.ended_at.is_none(),
            "endedAt": meeting.ended_at.map(iso),
        })),
    }))
}

/// The module's verbs by name (A4.1c) — Meet's one row in the agent's
/// dispatcher list, `crate::agent::MODULES`. `None` is "not mine": the
/// dispatcher then asks the next module. The three verbs the old tool set
/// already had keep their executors in [`crate::agent_meet`];
/// `schedule_meeting` runs the Agenda module's own calendar write as the
/// asker, which is the verb's whole definition.
pub(crate) fn dispatch<'a>(
    state: &'a AppState,
    account: &'a Account,
    tool: &'a str,
    args: &'a Value,
) -> Option<crate::agent::Dispatched<'a>> {
    let run: crate::agent::Dispatched<'a> = match tool {
        "meetings_recent" => Box::pin(crate::agent_meet::execute_meetings_recent(account, args)),
        "meeting_record" => Box::pin(crate::agent_meet::execute_meeting_record(
            account, args, state,
        )),
        "upcoming_meetings" => Box::pin(execute_upcoming_meetings(account, args)),
        "meeting_lookup" => Box::pin(execute_meeting_lookup(account, args)),
        "meeting_minutes" => Box::pin(crate::agent_meet::execute_meeting_minutes(account, args)),
        "schedule_meeting" => Box::pin(crate::agent::execute_create_event(account, args)),
        _ => return None,
    };
    Some(run)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use alo_ai::meet_intents::MEET;

    /// Every `/meet` route the router registers is the adapter of a verb or
    /// excluded with a reason — the coverage ADR 0058 makes structural. The
    /// prefix is `/meet` rather than `/meet/` so the root route (start a call,
    /// list the live ones) is covered too.
    #[test]
    fn every_meet_route_is_a_verb_or_an_exclusion() {
        let router = include_str!("server.rs");
        let missing = MEET.uncovered(router, "/meet");
        assert!(
            missing.is_empty(),
            "routes with neither a verb nor a reason: {missing:?}"
        );
        // …and every verb's route exists, so an intent cannot name a route
        // the app does not have.
        let routes = alo_ai::routes_in(router, "/meet");
        for intent in MEET.intents {
            for route in intent.routes {
                assert!(
                    routes.contains(&(*route).to_owned()),
                    "{}: {route} is not a route",
                    intent.name
                );
            }
        }
    }

    #[test]
    fn every_verb_the_registry_offers_is_dispatched() {
        let dispatch = include_str!("meet_intents.rs");
        for intent in MEET.intents {
            assert!(
                dispatch.contains(&format!("\"{}\" =>", intent.name)),
                "{} has no executor in the dispatch",
                intent.name
            );
        }
    }

    /// Meet's registration is one row in each list (A4.1c): the agent's
    /// dispatcher names this module once, and the two lists are the same
    /// length — every moved module has its dispatcher.
    #[test]
    fn the_module_is_one_row_in_each_list() {
        let agent = include_str!("agent.rs");
        assert_eq!(
            agent.matches("meet_intents::").count(),
            1,
            "agent.rs names Meet only in MODULES"
        );
        assert!(agent.contains("crate::meet_intents::dispatch"));
        assert_eq!(
            crate::agent::MODULES.len(),
            alo_ai::MOVED.len(),
            "a moved module without a dispatcher, or the reverse"
        );
    }

    /// AC.2's seam, held structurally: scheduling dispatches to the Agenda
    /// module's own calendar write — the shared executor, not a copy of it —
    /// so a diary entry is made in exactly one place whichever agent proposed
    /// it.
    #[test]
    fn scheduling_runs_the_agenda_write_and_not_a_copy() {
        let dispatch = include_str!("meet_intents.rs");
        assert!(
            dispatch
                .contains("\"schedule_meeting\" => Box::pin(crate::agent::execute_create_event("),
            "schedule_meeting does not run the shared calendar write"
        );
    }
}
