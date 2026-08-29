//! Executing the agent's **reading** tools for Agenda, Chat and Contacts
//! (ADR 0034).
//!
//! Together in one file because they are one idea: each answers a question
//! from the record instead of from the search snippets, and none of them
//! changes anything. Every one runs on the asker's own account door, so an
//! agent sees exactly what the person who asked could see — a diary they
//! cannot open, a room they are not in and an address book that is not theirs
//! do not exist here.

use axum::Json;
use serde_json::{Value, json};
use time::format_description::well_known::Rfc3339;
use time::{Duration, OffsetDateTime, Time};

use alo_store::ChatChannelId;

use crate::agent_args::{string_arg, unprocessable};
use crate::billing::{map_store_err, parse_iso_date};
use crate::error::Problem;
use crate::state::Account;

/// At most a month of diary in one answer: a year of events is not a summary,
/// it is a denial-of-service on the model's context.
///
/// Shared with [`crate::agent_agenda`], which looks at the same diaries over
/// the same kind of range: two limits would mean two answers to "how far ahead
/// may an agent look", and the looser one would be the real one.
pub(crate) const MAX_DAYS: i64 = 31;

pub(crate) fn iso(at: OffsetDateTime) -> String {
    at.format(&Rfc3339).unwrap_or_default()
}

/// The wire shape of one event. Enough to say what it is and when, and
/// nothing more — an agent answering "what's on Thursday" does not need every
/// attendee's status.
/// Shared with the Agenda agent's `colleague_free` executor
/// (`crate::agenda_intents`), which reports a shared diary's clashes in the
/// same words the asker's own are reported in.
pub(crate) fn event_json(event: &alo_store::CalendarEvent) -> Value {
    json!({
        "title": event.summary,
        "startsAt": iso(event.starts_at),
        "endsAt": iso(event.ends_at),
        "allDay": event.all_day,
        "location": event.location,
    })
}

/// `whats_on` — the caller's own diary over a range of days.
///
/// # Errors
/// 422 when the dates are missing, malformed, backwards, or span too long.
pub async fn execute_whats_on(account: &Account, args: &Value) -> Result<Json<Value>, Problem> {
    let from_s = string_arg(args, "from").ok_or_else(|| unprocessable("from is required"))?;
    let from = parse_iso_date(&from_s).ok_or_else(|| unprocessable("from must be YYYY-MM-DD"))?;
    let to = match string_arg(args, "to") {
        Some(to) => parse_iso_date(&to).ok_or_else(|| unprocessable("to must be YYYY-MM-DD"))?,
        None => from,
    };
    if to < from {
        return Err(unprocessable("to is before from"));
    }
    if (to - from).whole_days() >= MAX_DAYS {
        return Err(unprocessable("a range covers at most 31 days"));
    }
    // The whole of both end days, in UTC. A diary question is about days, and
    // a range that stopped at midnight-minus-one would silently drop the last.
    let start = from.with_time(Time::MIDNIGHT).assume_utc();
    let end = (to + Duration::days(1))
        .with_time(Time::MIDNIGHT)
        .assume_utc();
    let events = account
        .acc
        .events_in_range(start, end)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "agendaDays",
            "from": from_s,
            "to": to.to_string(),
            "events": events.iter().map(event_json).collect::<Vec<_>>(),
        }
    })))
}

/// `am_i_free` — what already overlaps a span of time.
///
/// Reports the clash; does not judge it. A meeting somebody would happily
/// leave and one they would not look identical in a database.
///
/// # Errors
/// 422 when the instants are missing, malformed or backwards.
pub async fn execute_am_i_free(account: &Account, args: &Value) -> Result<Json<Value>, Problem> {
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
    let events = account
        .acc
        .events_in_range(start, end)
        .await
        .map_err(map_store_err)?;
    // An all-day entry is not a reason to refuse a meeting: "Leave" and
    // "Company offsite" both span the day, and only one of them means busy.
    // They are reported separately rather than counted as a clash.
    let (all_day, timed): (Vec<_>, Vec<_>) = events.iter().partition(|e| e.all_day);
    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "agendaFree",
            "start": iso(start),
            "end": iso(end),
            "free": timed.is_empty(),
            "clashes": timed.iter().map(|e| event_json(e)).collect::<Vec<_>>(),
            "allDay": all_day.iter().map(|e| event_json(e)).collect::<Vec<_>>(),
        }
    })))
}

/// Find one channel by the name the user said. Returns `None` when no room of
/// that name is the caller's to read — which is indistinguishable, on purpose,
/// from there being no such room.
///
/// Shared with [`crate::agent_tasks`], whose two thread tools name a room the
/// same way: two resolutions would eventually disagree about which room "the
/// launch" is, and the looser one would be the real one.
pub(crate) async fn room_named(
    account: &Account,
    name: &str,
) -> Result<Option<ChatChannelId>, Problem> {
    let wanted = name.trim().trim_start_matches('#').to_lowercase();
    let channels = account.acc.channels().await.map_err(map_store_err)?;
    Ok(channels
        .into_iter()
        .find(|c| c.name.as_deref().map(str::to_lowercase) == Some(wanted.clone()))
        .map(|c| c.id))
}

/// `catch_up_room` — the recent messages of one conversation.
///
/// # Errors
/// 422 when no room was named.
pub async fn execute_catch_up_room(
    account: &Account,
    args: &Value,
) -> Result<Json<Value>, Problem> {
    let room = string_arg(args, "room").ok_or_else(|| unprocessable("room is required"))?;
    let limit = args
        .get("limit")
        .and_then(Value::as_i64)
        .unwrap_or(30)
        .clamp(1, 50);
    let Some(id) = room_named(account, &room).await? else {
        // Not an error: the model was told to say so rather than guess, and a
        // 404 here would tell the caller a private room exists.
        return Ok(Json(json!({
            "ok": true,
            "result": { "kind": "chatCatchUp", "room": room, "found": false, "messages": [] }
        })));
    };
    let messages = account
        .acc
        .messages(&id, None, limit)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "chatCatchUp",
            "room": room,
            "found": true,
            // Oldest first: a summary reads forwards even though the store
            // answers newest-first.
            "messages": messages
                .iter()
                .rev()
                .map(|m| json!({
                    // The room sees the agent; the record shows who asked.
                    // Both matter to a summary, so both are reported.
                    "author": m.message.author.as_str(),
                    "isAgent": m.message.author_is_agent,
                    "at": iso(m.message.created_at),
                    "body": m.message.body,
                }))
                .collect::<Vec<_>>(),
        }
    })))
}

/// `find_in_chat` — messages matching words, across rooms the caller can read.
///
/// # Errors
/// 422 when no query was given.
pub async fn execute_find_in_chat(account: &Account, args: &Value) -> Result<Json<Value>, Problem> {
    let query = string_arg(args, "query").ok_or_else(|| unprocessable("query is required"))?;
    let limit = args
        .get("limit")
        .and_then(Value::as_i64)
        .unwrap_or(10)
        .clamp(1, 25);
    let room = match string_arg(args, "room") {
        Some(name) => room_named(account, &name).await?,
        None => None,
    };
    let found = account
        .acc
        .search_messages(&query, room.as_ref(), limit)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "chatMatches",
            "query": query,
            "messages": found
                .iter()
                .map(|m| json!({
                    "author": m.author.as_str(),
                    "isAgent": m.author_is_agent,
                    "at": iso(m.created_at),
                    "body": m.body,
                }))
                .collect::<Vec<_>>(),
        }
    })))
}

/// `find_contact` — people in the caller's own address book.
///
/// # Errors
/// 422 when no query was given.
pub async fn execute_find_contact(account: &Account, args: &Value) -> Result<Json<Value>, Problem> {
    let query = string_arg(args, "query").ok_or_else(|| unprocessable("query is required"))?;
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Err(unprocessable("query is required"));
    }
    let limit = args
        .get("limit")
        .and_then(Value::as_i64)
        .unwrap_or(5)
        .clamp(1, 10);
    // The address book is a person's own and small; filtering here keeps the
    // matching rule in one readable place rather than in SQL nobody revisits.
    let matched: Vec<_> = account
        .acc
        .contacts()
        .await
        .map_err(map_store_err)?
        .into_iter()
        .filter(|c| {
            c.display_name.to_lowercase().contains(&query)
                || c.emails
                    .iter()
                    .any(|e| e.value.to_lowercase().contains(&query))
                || c.organization
                    .as_deref()
                    .is_some_and(|o| o.to_lowercase().contains(&query))
        })
        .take(usize::try_from(limit).unwrap_or(5))
        .collect();
    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "contacts",
            "query": query,
            // Several matches are reported, never resolved: two people called
            // Ben is ordinary, and choosing one puts the wrong address in
            // whatever gets written next.
            "people": matched
                .iter()
                .map(|c| json!({
                    "name": c.display_name,
                    "emails": c.emails.iter().map(|e| e.value.clone()).collect::<Vec<_>>(),
                    "phones": c.phones.iter().map(|p| p.value.clone()).collect::<Vec<_>>(),
                    "organization": c.organization,
                }))
                .collect::<Vec<_>>(),
        }
    })))
}
