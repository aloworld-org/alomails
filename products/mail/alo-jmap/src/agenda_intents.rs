//! The executors of alo Agenda's verbs (ADR 0058, queue item AB.5) — what runs
//! when the Agenda agent uses one of the intents `alo_ai::agenda_intents`
//! describes.
//!
//! Every executor runs through the asker's account door: the diaries are the
//! ones [`alo_store::AccountStore::calendars`] lists — owned, or already
//! shared with them — so a colleague's unshared afternoon, and every other
//! tenant's everything, is not among the things that can be named. A read
//! returns `{"ok": true, "result": …}` into the turn; a write returns the
//! record it changed, and only ever runs from the asker's approval
//! ([`crate::agent::execute_tool`] holds that, not this module).
//!
//! What AB.5 adds beside the six kept tools:
//!
//! - `event_lookup` and `colleague_free` — one meeting in full, answered with
//!   the record view the `/calendar/events` routes themselves serve
//!   ([`crate::calendar::event_json`]), and one shared diary's span of time,
//!   resolved through the same [`crate::agent_agenda::shared_diaries`] reach
//!   `find_a_time` looks across — a diary that was never shared is a named
//!   refusal, never an empty (and therefore "free") day.
//! - `cancel_event` and `respond_to_invitation` — the same cancel and RSVP the
//!   calendar's own buttons run ([`crate::calendar::cancel_core`],
//!   [`crate::calendar::rsvp_core`]), no new storage or mail path. A cancel
//!   resolves the meeting by its title, cancels one sitting of a series on its
//!   own, and tells the guests; an answer finds the invitation in the asker's
//!   own mail by the meeting's title, and several invitations that match come
//!   back listed rather than one of them guessed.

use axum::Json;
use serde_json::{Value, json};
use time::format_description::well_known::Rfc3339;
use time::{Duration, OffsetDateTime};

use alo_store::{CalendarEvent, MessageId};

use crate::agent_agenda::{SharedDiaries, resolve_person, shared_diaries};
use crate::agent_args::{string_arg, unprocessable};
use crate::agent_meeting::resolve_meeting;
use crate::agent_reads::{event_json, iso};
use crate::billing::map_store_err;
use crate::error::Problem;
use crate::state::{Account, AppState};

/// How many of the caller's emails a meeting's title is matched against when
/// looking for its invitation — the same sweep `meeting_prep` reads.
const MAX_CANDIDATES: i64 = 10;

pub(crate) type Reply = Result<Json<Value>, Problem>;

/// Every read's answer.
fn ok(result: Value) -> Reply {
    Ok(Json(json!({ "ok": true, "result": result })))
}

/// `event_lookup` — one meeting in full, by its title: the same record
/// `GET /calendar/events/{id}` serves, so guests, replies, recurrence and the
/// reminder are said in the calendar's own vocabulary.
pub async fn execute_event_lookup(account: &Account, args: &Value) -> Reply {
    let event = resolve_meeting(account, args).await?;
    ok(json!({
        "kind": "eventLookup",
        "record": crate::calendar::event_json(&event),
        // Said beside the record, so the model can say "this Tuesday's
        // sitting" rather than treating a series as one meeting.
        "occurrenceOfSeries": event.recurrence_id.is_some(),
    }))
}

/// `colleague_free` — whether ONE colleague already has something over a span,
/// out of the diaries ALREADY shared with the asker.
///
/// # Errors
/// 422 when the person or the span is missing or malformed, and — the rule the
/// verb exists for — when no diary of that person's is shared with the asker:
/// the refusal names the person and says nothing about whether they exist.
pub async fn execute_colleague_free(state: &AppState, account: &Account, args: &Value) -> Reply {
    let wanted = string_arg(args, "person")
        .ok_or_else(|| unprocessable("say which colleague — a name or an email address"))?;
    let start_s = string_arg(args, "start").ok_or_else(|| unprocessable("start is required"))?;
    let start = OffsetDateTime::parse(&start_s, &Rfc3339)
        .map_err(|_| unprocessable("start must be an RFC 3339 datetime"))?;
    let end = match string_arg(args, "end") {
        Some(end) => OffsetDateTime::parse(&end, &Rfc3339)
            .map_err(|_| unprocessable("end must be an RFC 3339 datetime"))?,
        None => start + Duration::hours(1),
    };
    if end <= start {
        return Err(unprocessable("end is not after start"));
    }
    let SharedDiaries {
        owner_of,
        addresses,
        visible,
        ..
    } = shared_diaries(account, state).await?;
    // An unshared diary and a person who does not exist get the same sentence,
    // on purpose — asking cannot tell you which.
    let who = resolve_person(&wanted, &visible).map_err(unprocessable)?;
    let events = account
        .acc
        .events_in_range(start, end)
        .await
        .map_err(map_store_err)?;
    let theirs: Vec<&CalendarEvent> = events
        .iter()
        .filter(|event| {
            owner_of
                .get(event.calendar_id.as_str())
                .is_some_and(|owner| *owner == who)
        })
        .collect();
    // An all-day entry is reported beside the clashes, never as one — "Leave"
    // and "Company offsite" span a day identically, and only one means busy.
    let (all_day, timed): (Vec<&CalendarEvent>, Vec<&CalendarEvent>) =
        theirs.into_iter().partition(|event| event.all_day);
    ok(json!({
        "kind": "colleagueFree",
        "person": wanted,
        "email": addresses.get(who.as_str()).cloned().unwrap_or_default(),
        "start": iso(start),
        "end": iso(end),
        "free": timed.is_empty(),
        "clashes": timed.iter().map(|event| event_json(event)).collect::<Vec<_>>(),
        "allDay": all_day.iter().map(|event| event_json(event)).collect::<Vec<_>>(),
    }))
}

/// `cancel_event` — one meeting taken out of the diary through the calendar's
/// own cancel, guests emailed a `CANCEL` by the same path the delete button
/// uses. One sitting of a series is cancelled on its own (an `EXDATE`); the
/// rest of the series stays. Runs only from the asker's own approval
/// (ADR 0047 §1).
///
/// # Errors
/// 422 when no meeting was named or matched, when several sittings match (the
/// refusal lists their days), or when the meeting sits in a diary the caller
/// can read but not change.
pub async fn execute_cancel_event(state: &AppState, account: &Account, args: &Value) -> Reply {
    let event = resolve_meeting(account, args).await?;
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
    let cancelled = crate::calendar::cancel_core(state, account, &event.id, event.recurrence_id)
        .await?
        .0;
    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "eventCancelled",
            "id": event.id.as_str(),
            "title": event.summary,
            "wasStartsAt": iso(event.starts_at),
            // "occurrence" when one sitting of a series was skipped and the
            // series stays; "series" when the whole meeting is gone.
            "scope": cancelled.get("scope").cloned().unwrap_or(Value::Null),
            // Who is being told, so the room can say so. The mails themselves
            // are best-effort, exactly as they are for the delete button.
            "guestsTold": event.attendees,
        }
    })))
}

/// `respond_to_invitation` — the invitation found in the asker's own mail by
/// the meeting's title, answered through the calendar's own RSVP: the reply is
/// emailed to the organizer and the meeting lands in the diary unless
/// declined. Runs only from the asker's own approval.
///
/// # Errors
/// 422 when the meeting or the answer is missing, when the answer is not
/// accepted/declined/tentative, when no invitation in the caller's mail
/// matches the title, or when several distinct ones do — the refusal lists
/// them, so the next turn can say which.
pub async fn execute_respond_to_invitation(
    state: &AppState,
    account: &Account,
    args: &Value,
) -> Reply {
    let wanted = string_arg(args, "meeting")
        .ok_or_else(|| unprocessable("say which meeting the invitation is for, by its title"))?;
    let response = string_arg(args, "response")
        .map(|answer| answer.trim().to_lowercase())
        .ok_or_else(|| unprocessable("say the user's answer — accepted, declined or tentative"))?;
    if !matches!(response.as_str(), "accepted" | "declined" | "tentative") {
        return Err(unprocessable(
            "response must be accepted, declined or tentative",
        ));
    }

    // The invitations the title names, out of the caller's own mail — the
    // same account-scoped sweep `meeting_prep` reads, so a colleague's
    // correspondence is not among the things that can come back. One
    // invitation re-issued (same UID) is one invitation, newest first from
    // the search; two different meetings that share the words are a question.
    let hits = account
        .acc
        .workspace_search(&wanted, MAX_CANDIDATES)
        .await
        .map_err(map_store_err)?;
    let needle = wanted.trim().to_lowercase();
    let mut invitations: Vec<(CalendarEvent, bytes::Bytes)> = Vec::new();
    for hit in hits.into_iter().filter(|hit| hit.kind == "message") {
        let Some(raw) =
            crate::agent_correspondence::raw_message(account, &MessageId::new(hit.id.clone()))
                .await
        else {
            continue;
        };
        let Some(ics_bytes) = crate::mime_read::calendar_part(&raw) else {
            continue;
        };
        let ics = String::from_utf8_lossy(&ics_bytes);
        if alo_store::ical::method_of(&ics).as_deref() != Some("REQUEST") {
            continue;
        }
        let Some(event) = alo_store::ical::from_ics(&ics, "") else {
            continue;
        };
        if !event.summary.trim().to_lowercase().contains(&needle) {
            continue;
        }
        if !invitations
            .iter()
            .any(|(seen, _)| seen.id.as_str() == event.id.as_str())
        {
            invitations.push((event, raw));
        }
    }
    // An exact title wins over one that merely contains it — the same rule a
    // meeting in the diary is resolved by.
    let exact: Vec<&(CalendarEvent, bytes::Bytes)> = invitations
        .iter()
        .filter(|(event, _)| event.summary.trim().to_lowercase() == needle)
        .collect();
    let matched: Vec<&(CalendarEvent, bytes::Bytes)> = if exact.is_empty() {
        invitations.iter().collect()
    } else {
        exact
    };
    let (event, raw) = match matched.len() {
        0 => {
            return Err(unprocessable(format!(
                "no invitation to {wanted} is in your mail"
            )));
        }
        1 => matched[0],
        _ => {
            return Err(unprocessable(format!(
                "more than one invitation matches {wanted}: {} — say which",
                matched
                    .iter()
                    .take(6)
                    .map(|(event, _)| format!("{} ({})", event.summary, iso(event.starts_at)))
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
    };

    let (invited, added, replied) =
        crate::calendar::rsvp_core(state, account, raw, &response).await?;
    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "invitationAnswered",
            "title": event.summary,
            "startsAt": iso(invited.starts_at),
            "response": response,
            // Whether it now sits in the diary (declining leaves it out), and
            // whether the organizer's reply mail actually went — said plainly,
            // so the room never claims a reply that was not sent.
            "added": added,
            "replied": replied,
        }
    })))
}

/// The module's verbs by name (A4.1c) — Agenda's one row in the agent's
/// dispatcher list, `crate::agent::MODULES`. `None` is "not mine": the
/// dispatcher then asks the next module, so two modules never need to know of
/// each other. The six kept tools keep their executors where they grew up —
/// the diary reads in [`crate::agent_reads`], the cross-diary look in
/// [`crate::agent_agenda`], the one-meeting pair in [`crate::agent_meeting`],
/// and the calendar write in `crate::agent` (Meet's `schedule_meeting` runs
/// that same one, which is what keeps the mechanism single) — and are reached
/// from here so the agent has one place to look.
pub(crate) fn dispatch<'a>(
    state: &'a AppState,
    account: &'a Account,
    tool: &'a str,
    args: &'a Value,
) -> Option<crate::agent::Dispatched<'a>> {
    let run: crate::agent::Dispatched<'a> = match tool {
        "whats_on" => Box::pin(crate::agent_reads::execute_whats_on(account, args)),
        "am_i_free" => Box::pin(crate::agent_reads::execute_am_i_free(account, args)),
        "find_a_time" => Box::pin(crate::agent_agenda::execute_find_a_time(
            account, args, state,
        )),
        "meeting_prep" => Box::pin(crate::agent_meeting::execute_meeting_prep(account, args)),
        "event_lookup" => Box::pin(execute_event_lookup(account, args)),
        "colleague_free" => Box::pin(execute_colleague_free(state, account, args)),
        "create_event" => Box::pin(crate::agent::execute_create_event(account, args)),
        "reschedule_event" => Box::pin(crate::agent_meeting::execute_reschedule_event(
            account, args,
        )),
        "cancel_event" => Box::pin(execute_cancel_event(state, account, args)),
        "respond_to_invitation" => Box::pin(execute_respond_to_invitation(state, account, args)),
        _ => return None,
    };
    Some(run)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use alo_ai::agenda_intents::AGENDA;

    /// Every `/calendar` route the router registers is the verb behind an
    /// intent or excluded with a reason — the coverage ADR 0058 makes
    /// structural.
    #[test]
    fn every_calendar_route_is_a_verb_or_an_exclusion() {
        let router = include_str!("server.rs");
        let missing = AGENDA.uncovered(router, "/calendar");
        assert!(
            missing.is_empty(),
            "routes with neither a verb nor a reason: {missing:?}"
        );
        // …and every named route exists, so an intent cannot claim — and an
        // exclusion cannot excuse — a route the app does not have.
        let routes = alo_ai::routes_in(router, "/calendar");
        for intent in AGENDA.intents {
            for route in intent.routes {
                assert!(
                    routes.contains(&(*route).to_owned()),
                    "{}: {route} is not a route",
                    intent.name
                );
            }
        }
        for excluded in AGENDA.excluded {
            assert!(
                routes.contains(&excluded.route.to_owned()),
                "{} is excused but not registered",
                excluded.route
            );
        }
    }

    #[test]
    fn every_verb_the_registry_offers_is_dispatched() {
        let dispatch = include_str!("agenda_intents.rs");
        for intent in AGENDA.intents {
            assert!(
                dispatch.contains(&format!("\"{}\" =>", intent.name)),
                "{} has no executor in the dispatch",
                intent.name
            );
        }
    }

    /// Agenda's registration is one row in each list (A4.1c): the agent's
    /// dispatcher names this module once, the registry names it once, and the
    /// two lists are the same length — every moved module has its dispatcher.
    #[test]
    fn the_module_is_one_row_in_each_list() {
        let agent = include_str!("agent.rs");
        assert_eq!(
            agent.matches("agenda_intents::").count(),
            1,
            "agent.rs names Agenda only in MODULES"
        );
        assert!(agent.contains("crate::agenda_intents::dispatch"));
        assert_eq!(
            crate::agent::MODULES.len(),
            alo_ai::MOVED.len(),
            "a moved module without a dispatcher, or the reverse"
        );
    }
}
