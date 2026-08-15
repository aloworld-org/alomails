//! Executing the **Meet** agent's tools (ADR 0034, queue item A3.2) — a
//! meeting after it is over.
//!
//! Its own file rather than more functions in [`crate::agent`], for the reason
//! every product's executors have one: this is Meet's subject matter, and the
//! dispatcher stays a dispatcher.
//!
//! Nothing here touches a call while it is running. alo Meet's media is
//! LiveKit's, deliberately (`alo_store::meet`), and a live in-call agent is a
//! media path nobody has decided on. What this module works from is what a
//! meeting leaves behind in our own database: who was in it, the transcript
//! segments, the messages people sent during it, and the conversation it came
//! out of.
//!
//! Four rules hold the module together, and each is a way the obvious
//! implementation would be wrong:
//!
//! - **A meeting is resolved by title and day, never by id.** The words a
//!   person said become exactly one sitting, and a title that ran twice is a
//!   refusal that lists the days — the same shape [`crate::agent_meeting`] uses
//!   for the diary, and for a sharper reason: minutes go into a room other
//!   people read, so the wrong sitting publishes the wrong record to them.
//! - **Only a meeting that has ended has minutes.** One still running resolves
//!   to a refusal that says so rather than to nothing, because "no such
//!   meeting" would be a lie about a meeting the caller is sitting in.
//! - **The minutes are the asker's own message.** They are posted through
//!   [`alo_store::AccountStore::post_message`], which posts as the caller, so
//!   the room sees a person's minutes rather than a robot's — and the room's
//!   membership check is the caller's own.
//! - **Minutes post a message and do nothing else.** No task is created here
//!   and no calendar entry: the actions in them become work through the Tasks
//!   and Agenda agents' own proposals (ADR 0023), which is what "the ordinary
//!   agent path" means. A second way to put a task on a board is exactly what
//!   A3.2 refuses to build.

use std::collections::HashMap;

use axum::Json;
use serde_json::{Value, json};
use time::OffsetDateTime;

use alo_store::{Meeting, UserId};

use crate::agent_args::{string_arg, unprocessable};
use crate::agent_reads::iso;
use crate::billing::{map_store_err, parse_iso_date};
use crate::error::Problem;
use crate::state::{Account, AppState};

/// How many ended meetings one listing reports, and the ceiling a caller may
/// raise it to. A history is not an answer: naming the last few sittings is
/// what lets the next turn say which one it means.
const DEFAULT_RECENT: i64 = 10;
const MAX_RECENT: i64 = 25;

/// How much of one meeting comes back. A long sitting can carry thousands of
/// transcript segments, and a record that did not stop somewhere would be a
/// denial-of-service on the model's context rather than a source.
const MAX_SEGMENTS: usize = 200;
const MAX_SAID: usize = 50;
const MAX_SINCE: i64 = 20;

/// What the minutes themselves may carry. Long enough for a real set, short
/// enough that nobody can be made to post an essay in a room by approving one
/// button.
const MAX_SUMMARY_CHARS: usize = 2_000;
const MAX_LINE_CHARS: usize = 400;
const MAX_LINES: usize = 20;

/// The headings the posted minutes carry.
///
/// Hardcoded English, as `UNCONFIGURED` and the orchestrator's plan heading
/// already are: every server-side sentence an agent speaks has the same debt,
/// and externalising all of them is one job rather than five (recorded in
/// STATE.md). Everything *around* these three words — the summary, the
/// decisions, the actions — is the model's, written in the language of the
/// meeting.
const MINUTES_HEADING: &str = "Minutes";
const DECISIONS_HEADING: &str = "Decisions";
const ACTIONS_HEADING: &str = "Actions";

/// One meeting, in the fields a listing reports.
fn meeting_json(meeting: &Meeting) -> Value {
    json!({
        "title": meeting.title,
        "startedAt": meeting.started_at.map(iso),
        "endedAt": meeting.ended_at.map(iso),
        // The day, which is the second half of naming a meeting that ran more
        // than once — and the word the next turn passes back as `day`.
        "day": meeting.ended_at.map(|at| at.date().to_string()),
        // Whether there is anywhere to post minutes. A meeting started outside
        // a conversation has no thread, and saying so here is what lets the
        // model avoid proposing minutes that cannot be posted.
        "hasThread": meeting.channel_id.is_some(),
    })
}

/// `meetings_recent` — the ended meetings this person was allowed to see.
///
/// # Errors
/// The store's own failure. There is nothing to refuse: an empty list is the
/// honest answer for somebody who has been in no meetings.
pub async fn execute_meetings_recent(
    account: &Account,
    args: &Value,
) -> Result<Json<Value>, Problem> {
    let limit = args
        .get("limit")
        .and_then(Value::as_i64)
        .unwrap_or(DEFAULT_RECENT)
        .clamp(1, MAX_RECENT);
    let meetings = account
        .acc
        .my_recent_meetings()
        .await
        .map_err(map_store_err)?;
    let total = meetings.len();
    let listed: Vec<Value> = meetings
        .iter()
        .take(usize::try_from(limit).unwrap_or(usize::MAX))
        .map(meeting_json)
        .collect();
    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "meetingsRecent",
            "meetings": listed,
            "truncated": total > usize::try_from(limit).unwrap_or(usize::MAX),
        }
    })))
}

/// The one ended meeting a title means, out of the ones the caller can see.
///
/// # Errors
/// 422 when nothing was named, when the day is not a date, when nothing
/// matches, when several sittings do — the last listing the days — and when
/// the only match is a meeting that has not ended yet.
async fn resolve_meeting(account: &Account, args: &Value) -> Result<Meeting, Problem> {
    let wanted = string_arg(args, "meeting")
        .ok_or_else(|| unprocessable("say which meeting, by its title"))?;
    let day = match string_arg(args, "day") {
        Some(day) => {
            Some(parse_iso_date(&day).ok_or_else(|| unprocessable("day must be YYYY-MM-DD"))?)
        }
        None => None,
    };
    let ended = account
        .acc
        .my_recent_meetings()
        .await
        .map_err(map_store_err)?;
    let mut matched = matching(&wanted, ended);
    if let Some(day) = day {
        matched.retain(|meeting| meeting.ended_at.is_some_and(|at| at.date() == day));
    }
    match matched.len() {
        0 => Err(not_one_meeting(account, &wanted).await),
        1 => matched.into_iter().next().ok_or_else(Problem::server_error),
        _ => Err(unprocessable(format!(
            "{wanted} has run more than once: {} — say which day",
            matched
                .iter()
                .take(6)
                .map(day_of)
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

/// Why no ended meeting matched — which is two different facts, and answering
/// both the same way would be a lie about the one the caller is sitting in.
///
/// A title that names a meeting still running says so; anything else is the
/// ordinary "no meeting of yours". Neither says whether somebody *else's*
/// meeting is called that: the lists both come from the caller's own door.
async fn not_one_meeting(account: &Account, wanted: &str) -> Problem {
    let live = account.acc.my_live_meetings().await.unwrap_or_default();
    if matching(wanted, live).is_empty() {
        unprocessable(format!(
            "no meeting of yours that has ended is called {wanted}"
        ))
    } else {
        unprocessable(format!(
            "{wanted} has not ended yet, so it has no minutes — ask again once it is over"
        ))
    }
}

/// The meetings a title names, by the resolution rule the rest of the agents
/// use: an exact title wins, and failing that every title containing the words.
///
/// Pure, so what a name matches is testable without a database.
fn matching(wanted: &str, meetings: Vec<Meeting>) -> Vec<Meeting> {
    let needle = wanted.trim().to_lowercase();
    let (exact, partial): (Vec<_>, Vec<_>) = meetings
        .into_iter()
        .filter(|meeting| meeting.title.trim().to_lowercase().contains(&needle))
        .partition(|meeting| meeting.title.trim().to_lowercase() == needle);
    if exact.is_empty() { partial } else { exact }
}

/// One sitting, in the words the "say which day" refusal uses.
fn day_of(meeting: &Meeting) -> String {
    match meeting.ended_at {
        Some(at) => format!("{} {:02}:{:02}", at.date(), at.hour(), at.minute()),
        None => "still running".to_owned(),
    }
}

/// Everybody named anywhere in one meeting's record, resolved to the addresses
/// a person would recognise.
///
/// One batch lookup rather than one per segment: a transcript is thousands of
/// lines by a handful of speakers.
async fn addresses(
    account: &Account,
    state: &AppState,
    users: Vec<UserId>,
) -> Result<HashMap<String, String>, Problem> {
    if users.is_empty() {
        return Ok(HashMap::new());
    }
    state
        .store
        .for_tenant(account.tenant.clone())
        .emails_of(&users)
        .await
        .map_err(map_store_err)
}

/// `meeting_record` — one ended meeting, in full.
///
/// # Errors
/// 422 when the meeting was not named, does not resolve to exactly one, or has
/// not ended; the store's own failure otherwise.
pub async fn execute_meeting_record(
    account: &Account,
    args: &Value,
    state: &AppState,
) -> Result<Json<Value>, Problem> {
    let meeting = resolve_meeting(account, args).await?;
    let attended = account
        .acc
        .meeting_participants(&meeting.id)
        .await
        .map_err(map_store_err)?;
    let transcript = account
        .acc
        .meeting_transcript(&meeting.id)
        .await
        .map_err(map_store_err)?;
    let said = account
        .acc
        .meeting_messages(&meeting.id)
        .await
        .map_err(map_store_err)?;

    let mut users: Vec<UserId> = attended.iter().map(|who| who.user.clone()).collect();
    users.extend(transcript.iter().map(|segment| segment.speaker.clone()));
    users.extend(said.iter().map(|message| message.sender.clone()));
    users.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    users.dedup_by(|a, b| a.as_str() == b.as_str());
    let known = addresses(account, state, users).await?;
    let who = |user: &UserId| {
        known
            .get(user.as_str())
            .cloned()
            .unwrap_or_else(|| user.as_str().to_owned())
    };

    // The conversation the meeting came out of, and what has been said in it
    // since — which is how an agent asked twice can see its own first set of
    // minutes instead of posting a second one.
    let (room, posted_since) = match &meeting.channel_id {
        Some(channel) => {
            let name = account
                .acc
                .channel(channel)
                .await
                .map_err(map_store_err)?
                .name;
            let since = meeting.ended_at.unwrap_or(OffsetDateTime::UNIX_EPOCH);
            let messages = account
                .acc
                .messages(channel, None, MAX_SINCE)
                .await
                .map_err(map_store_err)?;
            let after: Vec<Value> = messages
                .iter()
                .rev()
                .filter(|m| m.message.created_at >= since)
                .map(|m| {
                    json!({
                        "author": m.message.author.as_str(),
                        "isAgent": m.message.author_is_agent,
                        "at": iso(m.message.created_at),
                        "body": m.message.body,
                    })
                })
                .collect();
            (name, after)
        }
        None => (None, Vec::new()),
    };

    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "meetingRecord",
            "title": meeting.title,
            "day": meeting.ended_at.map(|at| at.date().to_string()),
            "startedAt": meeting.started_at.map(iso),
            "endedAt": meeting.ended_at.map(iso),
            // Null when the meeting was started outside a conversation: there
            // is then nowhere to post minutes, and the model is told so here
            // rather than finding out at the proposal.
            "room": room,
            "attended": attended
                .iter()
                .map(|person| json!({ "who": who(&person.user), "joinedAt": iso(person.joined_at) }))
                .collect::<Vec<_>>(),
            // Oldest first, both of them: a decision reads forwards.
            "transcript": transcript
                .iter()
                .take(MAX_SEGMENTS)
                .map(|segment| json!({
                    "speaker": who(&segment.speaker),
                    "at": iso(segment.created_at),
                    "text": segment.text,
                }))
                .collect::<Vec<_>>(),
            "transcriptTruncated": transcript.len() > MAX_SEGMENTS,
            "said": said
                .iter()
                .take(MAX_SAID)
                .map(|message| json!({
                    "who": who(&message.sender),
                    "at": iso(message.created_at),
                    "body": message.body,
                }))
                .collect::<Vec<_>>(),
            "postedSince": posted_since,
        }
    })))
}

/// One line of the minutes, checked and trimmed.
fn line_of(value: &Value, what: &str, at: usize) -> Result<String, Problem> {
    let line = value
        .as_str()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .ok_or_else(|| unprocessable(format!("{what} {} is not a sentence", at + 1)))?;
    if line.chars().count() > MAX_LINE_CHARS {
        return Err(unprocessable(format!(
            "{what} {} is longer than {MAX_LINE_CHARS} characters",
            at + 1
        )));
    }
    Ok(line.to_owned())
}

/// The `decisions` argument: a list of sentences, or nothing.
fn decisions_of(args: &Value) -> Result<Vec<String>, Problem> {
    let Some(listed) = args.get("decisions").filter(|v| !v.is_null()) else {
        return Ok(Vec::new());
    };
    let listed = listed
        .as_array()
        .ok_or_else(|| unprocessable("decisions is a list of sentences"))?;
    if listed.len() > MAX_LINES {
        return Err(unprocessable(format!(
            "minutes carry at most {MAX_LINES} decisions"
        )));
    }
    listed
        .iter()
        .enumerate()
        .map(|(at, value)| line_of(value, "decision", at))
        .collect()
}

/// One action, as the minutes render it: what was agreed, who took it, and by
/// when — the last two only when somebody actually said them.
fn action_line(value: &Value, at: usize) -> Result<String, Problem> {
    let what = string_arg(value, "what")
        .ok_or_else(|| unprocessable(format!("action {} says nothing", at + 1)))?;
    if what.chars().count() > MAX_LINE_CHARS {
        return Err(unprocessable(format!(
            "action {} is longer than {MAX_LINE_CHARS} characters",
            at + 1
        )));
    }
    let owner = string_arg(value, "owner");
    let due = match string_arg(value, "due") {
        Some(due) => Some(
            parse_iso_date(&due)
                .ok_or_else(|| unprocessable(format!("action {}'s due is not YYYY-MM-DD", at + 1)))?
                .to_string(),
        ),
        None => None,
    };
    // Nobody is invented: an action nobody took is written down without an
    // owner rather than given to whoever happened to be in the room.
    Ok(match (owner, due) {
        (Some(owner), Some(due)) => format!("{what} — {owner}, by {due}"),
        (Some(owner), None) => format!("{what} — {owner}"),
        (None, Some(due)) => format!("{what} — by {due}"),
        (None, None) => what,
    })
}

/// The `actions` argument, each already rendered as its line.
fn actions_of(args: &Value) -> Result<Vec<String>, Problem> {
    let Some(listed) = args.get("actions").filter(|v| !v.is_null()) else {
        return Ok(Vec::new());
    };
    let listed = listed
        .as_array()
        .ok_or_else(|| unprocessable("actions is a list of {what, owner, due}"))?;
    if listed.len() > MAX_LINES {
        return Err(unprocessable(format!(
            "minutes carry at most {MAX_LINES} actions"
        )));
    }
    listed
        .iter()
        .enumerate()
        .map(|(at, value)| action_line(value, at))
        .collect()
}

/// The message the room sees: a heading naming the meeting and its day, the
/// summary, and the two lists — each section left out when it is empty, so a
/// meeting that decided nothing does not post an empty "Decisions".
fn minutes_body(
    meeting: &Meeting,
    summary: &str,
    decisions: &[String],
    actions: &[String],
) -> String {
    let mut body = match meeting.ended_at {
        Some(at) => format!("{MINUTES_HEADING} — {} ({})\n\n", meeting.title, at.date()),
        None => format!("{MINUTES_HEADING} — {}\n\n", meeting.title),
    };
    body.push_str(summary);
    for (heading, lines) in [(DECISIONS_HEADING, decisions), (ACTIONS_HEADING, actions)] {
        if lines.is_empty() {
            continue;
        }
        // One blank line between sections, whatever the last one ended with —
        // a list already ends in a newline, and adding two more would leave the
        // room reading a gap.
        body.truncate(body.trim_end().len());
        body.push_str(&format!("\n\n{heading}\n"));
        for line in lines {
            body.push_str(&format!("- {line}\n"));
        }
    }
    body.trim_end().to_owned()
}

/// `meeting_minutes` — the record of one ended meeting, posted into the
/// conversation it came out of. Runs only from the asker's own approval
/// (ADR 0047 §1), and the message is theirs.
///
/// # Errors
/// 422 when the meeting was not named, does not resolve to one, has not ended,
/// was not started from a conversation, or when the minutes themselves are
/// missing, over-long or malformed.
pub async fn execute_meeting_minutes(
    account: &Account,
    args: &Value,
) -> Result<Json<Value>, Problem> {
    let meeting = resolve_meeting(account, args).await?;
    let summary =
        string_arg(args, "summary").ok_or_else(|| unprocessable("summary is required"))?;
    if summary.chars().count() > MAX_SUMMARY_CHARS {
        return Err(unprocessable(format!(
            "a summary is at most {MAX_SUMMARY_CHARS} characters"
        )));
    }
    let decisions = decisions_of(args)?;
    let actions = actions_of(args)?;
    // A meeting nobody started from a room has nowhere for its minutes to go,
    // and picking a room for it would put somebody's meeting in front of people
    // who were never in it.
    let Some(channel) = meeting.channel_id.clone() else {
        return Err(unprocessable(format!(
            "{} was not started from a conversation, so there is no thread to post its minutes in",
            meeting.title
        )));
    };
    let body = minutes_body(&meeting, &summary, &decisions, &actions);
    // Posted as the caller, through their own door: the room's membership check
    // is theirs, so minutes cannot reach a conversation the approver is not in.
    let posted = account
        .acc
        .post_message(&channel, &body, None)
        .await
        .map_err(map_store_err)?;
    let room = account
        .acc
        .channel(&channel)
        .await
        .map_err(map_store_err)?
        .name;
    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "meetingMinutes",
            "title": meeting.title,
            "day": meeting.ended_at.map(|at| at.date().to_string()),
            "room": room,
            "seq": posted.seq,
            "decisions": decisions.len(),
            "actions": actions.len(),
            "body": body,
        }
    })))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    use alo_store::{ChatChannelId, MeetingId};
    use time::Duration;

    fn a_meeting(title: &str, days_ago: i64, threaded: bool) -> Meeting {
        let ended = OffsetDateTime::now_utc() - Duration::days(days_ago);
        Meeting {
            id: MeetingId::new(format!("m-{title}-{days_ago}")),
            room: "opaque".to_owned(),
            title: title.to_owned(),
            created_by: UserId::new("u1".to_owned()),
            channel_id: threaded.then(|| ChatChannelId::new("c1".to_owned())),
            event_id: None,
            created_at: ended - Duration::hours(1),
            started_at: Some(ended - Duration::hours(1)),
            ended_at: Some(ended),
        }
    }

    /// The resolution rule, without a database: an exact title wins over one
    /// that merely contains it, so a meeting really called "Standup" is
    /// reachable even though "Standup — design" exists.
    #[test]
    fn an_exact_title_wins_and_a_fragment_matches_what_contains_it() {
        let all = || {
            vec![
                a_meeting("Standup", 1, true),
                a_meeting("Standup — design", 2, true),
                a_meeting("Q3 budget", 3, true),
            ]
        };
        let exact = matching("standup", all());
        assert_eq!(exact.len(), 1);
        assert_eq!(exact[0].title, "Standup");
        assert_eq!(matching("design", all()).len(), 1);
        assert_eq!(matching("budget", all()).len(), 1);
        assert!(matching("hovercraft", all()).is_empty());
    }

    /// Two sittings of one title are a question, and the days are what the
    /// refusal offers — a date is the only thing that tells them apart.
    #[test]
    fn a_title_that_ran_twice_is_told_apart_by_its_day() {
        let twice = vec![
            a_meeting("Weekly review", 7, true),
            a_meeting("Weekly review", 14, true),
        ];
        let matched = matching("weekly review", twice);
        assert_eq!(matched.len(), 2);
        let days: Vec<String> = matched.iter().map(day_of).collect();
        assert_ne!(days[0], days[1], "the refusal has to tell them apart");
    }

    /// The posted message, section by section. An empty list posts no heading:
    /// a meeting that decided nothing must not leave "Decisions" over nothing.
    #[test]
    fn the_minutes_carry_only_the_sections_that_have_something_in_them() {
        let meeting = a_meeting("Q3 budget", 1, true);
        let bare = minutes_body(&meeting, "We went through the figures.", &[], &[]);
        assert!(bare.starts_with("Minutes — Q3 budget ("), "{bare}");
        assert!(bare.contains("We went through the figures."));
        assert!(!bare.contains("Decisions"), "{bare}");
        assert!(!bare.contains("Actions"), "{bare}");

        let full = minutes_body(
            &meeting,
            "We went through the figures.",
            &["Hold the marketing budget flat".to_owned()],
            &["Send the revised sheet — ben@example.test, by 2026-08-20".to_owned()],
        );
        assert!(
            full.contains("\nDecisions\n- Hold the marketing budget flat"),
            "{full}"
        );
        assert!(
            full.contains("\nActions\n- Send the revised sheet — ben@example.test, by 2026-08-20"),
            "{full}"
        );
        // Exactly one blank line between sections — never the gap a list that
        // already ends in a newline would otherwise leave.
        assert!(!full.contains("\n\n\n"), "{full}");
    }

    /// An action is written down with exactly the owner and date somebody gave
    /// it, and with neither when nobody did — inventing either is how minutes
    /// become a way of assigning work nobody agreed to.
    #[test]
    fn an_action_carries_only_the_owner_and_date_it_was_given() {
        assert_eq!(
            action_line(&json!({ "what": "Send the sheet" }), 0).unwrap(),
            "Send the sheet"
        );
        assert_eq!(
            action_line(&json!({ "what": "Send the sheet", "owner": "Ben" }), 0).unwrap(),
            "Send the sheet — Ben"
        );
        assert_eq!(
            action_line(&json!({ "what": "Send the sheet", "due": "2026-08-20" }), 0).unwrap(),
            "Send the sheet — by 2026-08-20"
        );
        assert_eq!(
            action_line(
                &json!({ "what": "Send the sheet", "owner": "Ben", "due": "2026-08-20" }),
                0
            )
            .unwrap(),
            "Send the sheet — Ben, by 2026-08-20"
        );
        // A date that is not one is a refusal naming the action, never a
        // silently dropped deadline.
        let why = action_line(&json!({ "what": "Send it", "due": "next Friday" }), 2)
            .expect_err("a phrase is not a date");
        assert_eq!(why.status, axum::http::StatusCode::UNPROCESSABLE_ENTITY);
        assert!(
            why.detail
                .as_deref()
                .unwrap_or_default()
                .contains("action 3"),
            "{:?}",
            why.detail
        );
        assert!(action_line(&json!({ "owner": "Ben" }), 0).is_err());
    }

    /// The lists are bounded and their contents are sentences — an approved
    /// proposal must not be able to post a wall of text into a room.
    #[test]
    fn the_lists_are_bounded_and_each_line_is_a_sentence() {
        assert!(decisions_of(&json!({})).unwrap().is_empty());
        assert!(
            decisions_of(&json!({ "decisions": null }))
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            decisions_of(&json!({ "decisions": ["  Keep it flat  "] })).unwrap(),
            vec!["Keep it flat".to_owned()]
        );
        assert!(decisions_of(&json!({ "decisions": "one" })).is_err());
        assert!(decisions_of(&json!({ "decisions": [""] })).is_err());
        let many: Vec<Value> = (0..=MAX_LINES)
            .map(|n| json!(format!("decision {n}")))
            .collect();
        assert!(decisions_of(&json!({ "decisions": many })).is_err());
        let long = "x".repeat(MAX_LINE_CHARS + 1);
        assert!(decisions_of(&json!({ "decisions": [long] })).is_err());
        assert!(actions_of(&json!({ "actions": [{ "what": "Do it" }] })).is_ok());
        assert!(actions_of(&json!({ "actions": {} })).is_err());
    }
}
