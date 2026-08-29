//! Executing `find_a_time` — the Agenda agent looking for a slot **across
//! several diaries** (ADR 0034, queue item A2.6).
//!
//! Its own file rather than another function in [`crate::agent_reads`], because
//! it answers a different question with a different reach. Everything in
//! `agent_reads` is about the caller's own record; this one is about several
//! people's, and the whole of its difficulty is in what the asker is allowed to
//! see of somebody else's day.
//!
//! Three rules hold it together, and each is a way the obvious implementation
//! would be wrong:
//!
//! - **A diary that was not read is not a free diary.** The only calendars this
//!   looks at are the ones already shared with the person asking
//!   ([`alo_store::AccountStore::calendars`] lists exactly those, owned and
//!   granted). A colleague whose calendar is not among them is reported in
//!   `couldNotCheck`, with the reason, and never counted as having nothing on.
//!   Silently treating an unreadable diary as an empty one is how an agent
//!   proposes a meeting over somebody's afternoon and calls it free.
//! - **A colleague is named, and the names that exist are the diaries you can
//!   see.** No user id is ever asked of the model, and no directory is
//!   searched: the candidates are the owners of the calendars the asker can
//!   already open, labelled with their addresses through
//!   [`alo_store::TenantStore::emails_of`]. So a name that resolves to nobody
//!   gets the same answer whether that person is a colleague whose diary is
//!   private or somebody who does not exist — asking cannot tell you which.
//! - **The working window is UTC, and says so.** The store speaks instants and
//!   this workspace has no timezone database; the prompt already tells the
//!   model which zone the asker is in, so converting "nine to five" into UTC is
//!   the model's job and passing an hour through unconverted would be the
//!   silent-wrong-answer version of the same bug.
//!
//! An all-day entry does not block a slot, for the reason `am_i_free` gives:
//! "Leave" and "Company offsite" span a day identically and only one of them
//! means busy. They are reported beside the slots rather than counted against
//! them.

use std::collections::{HashMap, HashSet};

use axum::Json;
use serde_json::{Value, json};
use time::{Date, Duration, OffsetDateTime, Time};

use alo_store::UserId;

use crate::agent_args::{string_arg, unprocessable};
use crate::agent_reads::{MAX_DAYS, iso};
use crate::billing::{map_store_err, parse_iso_date};
use crate::error::Problem;
use crate::state::{Account, AppState};

/// The shortest meeting worth looking for, and the longest. A five-minute slot
/// is noise in an answer; a day-long one is not a gap in a working day.
const MIN_MINUTES: i64 = 5;
const MAX_MINUTES: i64 = 480;

/// How many slots come back. Enough to choose from, few enough to read out.
const DEFAULT_SLOTS: i64 = 5;
const MAX_SLOTS: i64 = 20;

/// The working window looked inside when the model does not state one.
const DEFAULT_EARLIEST: (u8, u8) = (9, 0);
const DEFAULT_LATEST: (u8, u8) = (17, 0);

/// A half-open span of time — a meeting somebody already has, or a gap between
/// two of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Span {
    from: OffsetDateTime,
    to: OffsetDateTime,
}

/// `find_a_time` — the slots a set of people share, over a range of days.
///
/// # Errors
/// 422 when the days are missing, malformed, backwards or span too long; when
/// the working window is not a window; or when `people` is not a list of names.
/// The store's own failure otherwise.
pub async fn execute_find_a_time(
    account: &Account,
    args: &Value,
    state: &AppState,
) -> Result<Json<Value>, Problem> {
    let (from, to) = stated_range(args)?;
    let minutes = args
        .get("minutes")
        .and_then(Value::as_i64)
        .unwrap_or(30)
        .clamp(MIN_MINUTES, MAX_MINUTES);
    let earliest = stated_time(args, "earliest", DEFAULT_EARLIEST)?;
    let latest = stated_time(args, "latest", DEFAULT_LATEST)?;
    if latest <= earliest {
        return Err(unprocessable("latest is not after earliest"));
    }
    let wanted = stated_people(args)?;
    let limit = args
        .get("limit")
        .and_then(Value::as_i64)
        .unwrap_or(DEFAULT_SLOTS)
        .clamp(1, MAX_SLOTS);

    let SharedDiaries {
        owner_of,
        addresses,
        mine,
        visible,
    } = shared_diaries(account, state).await?;

    // Who is actually in the answer: the asker, plus every named colleague
    // whose diary they can see. The rest are reported, not assumed free.
    let mut people = vec![json!({
        "person": "you",
        "email": mine,
        "diaryVisible": true,
    })];
    let mut could_not_check = Vec::new();
    let mut participants: HashSet<UserId> = HashSet::from([account.user.clone()]);
    for name in &wanted {
        match resolve_person(name, &visible) {
            Ok(found) if found == account.user => {}
            Ok(found) => {
                people.push(json!({
                    "person": name,
                    "email": addresses.get(found.as_str()).cloned().unwrap_or_default(),
                    "diaryVisible": true,
                }));
                participants.insert(found);
            }
            Err(reason) => could_not_check.push(json!({ "person": name, "reason": reason })),
        }
    }

    // One read over the whole range, then split by whose calendar each event is
    // on. The events of a diary nobody asked about are dropped here rather than
    // fetched separately: `events_in_range` already answers only for calendars
    // the asker may see.
    let start = from.with_time(Time::MIDNIGHT).assume_utc();
    let end = (to + Duration::days(1))
        .with_time(Time::MIDNIGHT)
        .assume_utc();
    let events = account
        .acc
        .events_in_range(start, end)
        .await
        .map_err(map_store_err)?;
    let mut busy = Vec::new();
    let mut all_day = Vec::new();
    for event in &events {
        let owner = owner_of.get(event.calendar_id.as_str());
        if !owner.is_some_and(|owner| participants.contains(owner)) {
            continue;
        }
        if event.all_day {
            all_day.push(json!({
                "title": event.summary,
                "startsAt": iso(event.starts_at),
                "endsAt": iso(event.ends_at),
                "whose": whose(owner, &addresses, &account.user),
            }));
        } else {
            busy.push(Span {
                from: event.starts_at,
                to: event.ends_at,
            });
        }
    }

    let least = Duration::minutes(minutes);
    let mut slots = Vec::new();
    let mut day = from;
    while day <= to && i64::try_from(slots.len()).unwrap_or(i64::MAX) < limit {
        let window = Span {
            from: day.with_time(earliest).assume_utc(),
            to: day.with_time(latest).assume_utc(),
        };
        for gap in free_gaps(window, &busy, least) {
            if i64::try_from(slots.len()).unwrap_or(i64::MAX) >= limit {
                break;
            }
            slots.push(json!({
                "start": iso(gap.from),
                "end": iso(gap.from + least),
                // How far the gap runs, so the model can offer a later start
                // inside the same free stretch without asking again.
                "freeUntil": iso(gap.to),
            }));
        }
        day += Duration::days(1);
    }

    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "agendaSlots",
            "from": from.to_string(),
            "to": to.to_string(),
            "minutes": minutes,
            "earliest": hhmm(earliest),
            "latest": hhmm(latest),
            "people": people,
            "couldNotCheck": could_not_check,
            // Said outright rather than left to a reader comparing two lists:
            // an answer that skipped somebody's diary is not an answer about
            // everybody, and the model is told to say whose it skipped.
            "complete": could_not_check.is_empty(),
            "slots": slots,
            // Beside the slots, never against them: an all-day entry is as
            // often "Company offsite" as it is "Leave".
            "allDay": all_day,
        }
    })))
}

/// The diaries the asker can already open, gathered once.
///
/// Shared with the `colleague_free` executor in [`crate::agenda_intents`], so
/// "which diaries exist for this person" has exactly one answer — two readings
/// of it would eventually disagree about whose afternoon is visible.
pub(crate) struct SharedDiaries {
    /// Calendar id → the diary's owner.
    pub owner_of: HashMap<String, UserId>,
    /// Owner id → email address, the asker included.
    pub addresses: HashMap<String, String>,
    /// The asker's own address.
    pub mine: String,
    /// `(address, owner)` pairs, sorted by address — what a name is resolved
    /// against, and nothing else is.
    pub visible: Vec<(String, UserId)>,
}

/// Every calendar the asker can open, owned or shared with them — the whole
/// of what the cross-diary tools are able to look at.
pub(crate) async fn shared_diaries(
    account: &Account,
    state: &AppState,
) -> Result<SharedDiaries, Problem> {
    let calendars = account.acc.calendars().await.map_err(map_store_err)?;
    let mut owner_of: HashMap<String, UserId> = HashMap::new();
    for calendar in &calendars {
        owner_of.insert(
            calendar.id.as_str().to_owned(),
            UserId::new(calendar.owner.clone()),
        );
    }
    let others: Vec<UserId> = owner_of
        .values()
        .filter(|owner| **owner != account.user)
        .cloned()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    // Labels for the diaries that are already readable — never a directory
    // lookup, so a name that matches nothing says nothing about who exists.
    let mut addresses = state
        .store
        .for_tenant(account.tenant.clone())
        .emails_of(&others)
        .await
        .map_err(map_store_err)?;
    let mine = state
        .store
        .for_tenant(account.tenant.clone())
        .email_of(&account.user)
        .await
        .map_err(map_store_err)?
        .unwrap_or_default();
    addresses.insert(account.user.as_str().to_owned(), mine.clone());
    // The asker is among the diaries that can be named: a person who says
    // "Ben, Marta and me" has named themselves, and answering that their own
    // diary is not shared with them would be a nonsense.
    let mut visible: Vec<(String, UserId)> = others
        .iter()
        .chain(std::iter::once(&account.user))
        .filter_map(|owner| {
            addresses
                .get(owner.as_str())
                .map(|email| (email.clone(), owner.clone()))
        })
        .collect();
    visible.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(SharedDiaries {
        owner_of,
        addresses,
        mine,
        visible,
    })
}

/// A bound of the working window, in the `HH:MM` the model passed it as — the
/// answer says the window it actually looked inside, and says it in the same
/// vocabulary the argument used.
fn hhmm(at: Time) -> String {
    format!("{:02}:{:02}", at.hour(), at.minute())
}

/// Whose diary an event sits in, in the words the answer uses.
fn whose(owner: Option<&UserId>, addresses: &HashMap<String, String>, me: &UserId) -> String {
    match owner {
        Some(owner) if owner == me => "you".to_owned(),
        Some(owner) => addresses.get(owner.as_str()).cloned().unwrap_or_default(),
        None => String::new(),
    }
}

/// The range of days asked about, held to the same month `whats_on` is.
fn stated_range(args: &Value) -> Result<(Date, Date), Problem> {
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
    Ok((from, to))
}

/// An `HH:MM` bound of the working window, in UTC, or the default.
fn stated_time(args: &Value, key: &str, fallback: (u8, u8)) -> Result<Time, Problem> {
    let Some(stated) = string_arg(args, key) else {
        // The two defaults are constants of this file and are hours of a real
        // day; a failure here would be this code being wrong about itself.
        return Time::from_hms(fallback.0, fallback.1, 0).map_err(|_| Problem::server_error());
    };
    let (hours, minutes) = stated
        .split_once(':')
        .ok_or_else(|| unprocessable(format!("{key} must be a time like 09:00")))?;
    let hours: u8 = hours
        .trim()
        .parse()
        .map_err(|_| unprocessable(format!("{key} must be a time like 09:00")))?;
    let minutes: u8 = minutes
        .trim()
        .parse()
        .map_err(|_| unprocessable(format!("{key} must be a time like 09:00")))?;
    Time::from_hms(hours, minutes, 0)
        .map_err(|_| unprocessable(format!("{key} is not a time of day")))
}

/// The colleagues the model named, as they said them. A `people` that is not a
/// list of names is a 422 rather than an empty answer: silently reading it as
/// "just me" would answer a different question from the one asked.
fn stated_people(args: &Value) -> Result<Vec<String>, Problem> {
    let Some(stated) = args.get("people") else {
        return Ok(Vec::new());
    };
    if stated.is_null() {
        return Ok(Vec::new());
    }
    let listed = stated
        .as_array()
        .ok_or_else(|| unprocessable("people must be a list of names"))?;
    let mut out = Vec::new();
    for person in listed {
        let name = person
            .as_str()
            .ok_or_else(|| unprocessable("people must be a list of names"))?
            .trim();
        if !name.is_empty() && !out.iter().any(|had: &String| had == name) {
            out.push(name.to_owned());
        }
    }
    Ok(out)
}

/// The colleague a name means, out of the diaries the asker can already open.
///
/// The address wins over its local part and the local part over a fragment, so
/// `ben@…` reaches Ben even when Bennett's diary is shared too; two matches is
/// a question that lists them, never a guess — the wrong diary would be read as
/// the right one, and the meeting would land on the wrong person's afternoon.
pub(crate) fn resolve_person(wanted: &str, visible: &[(String, UserId)]) -> Result<UserId, String> {
    let needle = wanted.trim().to_lowercase();
    if needle.is_empty() {
        return Err("which colleague was meant is required".to_owned());
    }
    let local = |email: &str| email.split('@').next().unwrap_or(email).to_lowercase();
    let by = |rule: &dyn Fn(&str) -> bool| {
        visible
            .iter()
            .filter(|(email, _)| rule(email))
            .collect::<Vec<_>>()
    };
    let mut found = by(&|email: &str| email.to_lowercase() == needle);
    if found.is_empty() {
        found = by(&|email: &str| local(email) == needle);
    }
    if found.is_empty() {
        found = by(&|email: &str| email.to_lowercase().contains(&needle));
    }
    match found.len() {
        0 => Err(format!("no diary of {wanted}'s is shared with you")),
        1 => Ok(found[0].1.clone()),
        _ => Err(format!(
            "more than one shared diary matches {wanted}: {} — say which",
            found
                .iter()
                .map(|(email, _)| email.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

/// The stretches of `window` nothing in `busy` overlaps, longest-first order
/// being irrelevant — they come back in time order, which is how a slot is
/// offered.
///
/// Pure, and the whole of the arithmetic this tool does: what a person is told
/// is free is testable here without a database, a model or a calendar.
fn free_gaps(window: Span, busy: &[Span], least: Duration) -> Vec<Span> {
    let mut blocks: Vec<Span> = busy
        .iter()
        .filter(|span| span.to > window.from && span.from < window.to)
        .copied()
        .collect();
    blocks.sort_by_key(|span| span.from);
    let mut out = Vec::new();
    let mut cursor = window.from;
    for block in blocks {
        if block.from > cursor && block.from - cursor >= least {
            out.push(Span {
                from: cursor,
                to: block.from,
            });
        }
        // Overlapping meetings do not each push the cursor back: a meeting
        // inside another one would otherwise re-open the time it sits in.
        if block.to > cursor {
            cursor = block.to;
        }
    }
    if window.to > cursor && window.to - cursor >= least {
        out.push(Span {
            from: cursor,
            to: window.to,
        });
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use time::Month;

    fn span(from: OffsetDateTime, to: OffsetDateTime) -> Span {
        Span { from, to }
    }

    /// A time on the one Monday every test in this module is about.
    fn at(hour: u8, minute: u8) -> OffsetDateTime {
        Date::from_calendar_date(2026, Month::August, 17)
            .unwrap()
            .with_hms(hour, minute, 0)
            .unwrap()
            .assume_utc()
    }

    fn day() -> Span {
        span(at(9, 0), at(17, 0))
    }

    /// The ordinary shape: two meetings, three gaps, and the short one between
    /// them left out because nobody wants a meeting they cannot hold.
    #[test]
    fn the_gaps_between_meetings_are_what_is_free() {
        let busy = [span(at(10, 0), at(11, 0)), span(at(11, 15), at(12, 0))];
        let gaps = free_gaps(day(), &busy, Duration::minutes(30));
        assert_eq!(
            gaps,
            vec![span(at(9, 0), at(10, 0)), span(at(12, 0), at(17, 0)),],
            "the quarter of an hour at 11:00 is not half an hour"
        );
        // Asked for something that fits, it is offered.
        assert_eq!(free_gaps(day(), &busy, Duration::minutes(15)).len(), 3);
    }

    /// A meeting inside another meeting must not re-open the time it sits in —
    /// the failure a naive cursor produces, and the one that would offer a slot
    /// in the middle of somebody's afternoon.
    #[test]
    fn a_meeting_inside_another_one_does_not_open_a_gap() {
        let busy = [span(at(9, 0), at(16, 0)), span(at(10, 0), at(11, 0))];
        assert_eq!(
            free_gaps(day(), &busy, Duration::minutes(30)),
            vec![span(at(16, 0), at(17, 0))]
        );
    }

    /// A meeting that starts before the working day and ends inside it eats
    /// only the part that overlaps; one entirely outside is ignored.
    #[test]
    fn a_meeting_is_only_ever_counted_where_it_overlaps_the_window() {
        let busy = [span(at(7, 0), at(9, 30)), span(at(18, 0), at(19, 0))];
        assert_eq!(
            free_gaps(day(), &busy, Duration::minutes(60)),
            vec![span(at(9, 30), at(17, 0))]
        );
    }

    /// A full day is one gap, and a day with nothing free is no gap at all —
    /// which is an empty answer, never a slot offered anyway.
    #[test]
    fn an_empty_day_is_one_gap_and_a_full_one_is_none() {
        assert_eq!(free_gaps(day(), &[], Duration::minutes(30)), vec![day()]);
        assert!(free_gaps(day(), &[day()], Duration::minutes(30)).is_empty());
    }

    fn diaries() -> Vec<(String, UserId)> {
        vec![
            ("ben@example.test".to_owned(), UserId::new("u-ben")),
            ("bennett@example.test".to_owned(), UserId::new("u-bennett")),
            ("marta@other.test".to_owned(), UserId::new("u-marta")),
        ]
    }

    /// A whole address beats its local part, a local part beats a fragment, and
    /// a fragment two people share is a question that names them.
    #[test]
    fn a_colleague_is_resolved_out_of_the_diaries_you_can_see() {
        assert_eq!(
            resolve_person("ben@example.test", &diaries()).unwrap(),
            UserId::new("u-ben")
        );
        assert_eq!(
            resolve_person("  BEN ", &diaries()).unwrap(),
            UserId::new("u-ben"),
            "the local part is exact even though bennett@ contains it"
        );
        assert_eq!(
            resolve_person("marta", &diaries()).unwrap(),
            UserId::new("u-marta")
        );
        // With no Ben of their own, "ben" is Bennett and nobody else — a
        // fragment only one diary contains is that diary.
        assert_eq!(
            resolve_person("ben", &[diaries()[1].clone(), diaries()[2].clone()]).unwrap(),
            UserId::new("u-bennett")
        );
        // Two diaries that share the fragment is a question that names them,
        // never the first of them.
        let both = vec![
            ("bennett@example.test".to_owned(), UserId::new("u-bennett")),
            ("benito@example.test".to_owned(), UserId::new("u-benito")),
        ];
        let why = resolve_person("ben", &both).unwrap_err();
        assert!(why.contains("bennett@example.test"), "{why}");
        assert!(why.contains("benito@example.test"), "{why}");
        let why = resolve_person("exampl", &diaries()).unwrap_err();
        assert!(why.contains("more than one shared diary"), "{why}");
    }

    /// The rule the whole module exists for: a diary that is not shared is not
    /// a diary that is free, and the refusal says nothing about whether that
    /// person exists at all.
    #[test]
    fn an_unshared_diary_is_a_refusal_that_reveals_nothing() {
        let why = resolve_person("paula", &diaries()).unwrap_err();
        assert_eq!(why, "no diary of paula's is shared with you");
        // Word for word the same answer for somebody who is in no tenant at
        // all — which is what makes asking useless as a way to find out.
        assert_eq!(
            resolve_person("nobody-at-all", &diaries()).unwrap_err(),
            "no diary of nobody-at-all's is shared with you"
        );
        assert!(resolve_person("   ", &diaries()).is_err());
    }

    /// `people` is a list of names or it is a mistake — never quietly "just
    /// me", which would answer a narrower question than the one asked.
    #[test]
    fn people_is_a_list_of_names_or_a_refusal() {
        assert_eq!(stated_people(&json!({})).unwrap(), Vec::<String>::new());
        assert_eq!(
            stated_people(&json!({ "people": null })).unwrap(),
            Vec::<String>::new()
        );
        assert_eq!(
            stated_people(&json!({ "people": [" Ben ", "marta", "Ben"] })).unwrap(),
            vec!["Ben".to_owned(), "marta".to_owned()],
            "trimmed, and a name said twice is one person"
        );
        assert!(stated_people(&json!({ "people": "Ben" })).is_err());
        assert!(stated_people(&json!({ "people": [7] })).is_err());
    }

    /// The working window is a window, and a bound that is not a time of day
    /// is refused rather than rounded into one.
    #[test]
    fn the_working_window_is_read_as_hours_and_minutes() {
        let args = json!({ "earliest": "08:30", "latest": "18:00" });
        assert_eq!(
            stated_time(&args, "earliest", DEFAULT_EARLIEST).unwrap(),
            Time::from_hms(8, 30, 0).unwrap()
        );
        assert_eq!(
            stated_time(&json!({}), "latest", DEFAULT_LATEST).unwrap(),
            Time::from_hms(17, 0, 0).unwrap()
        );
        for bad in ["nine", "0900", "25:00", "09:61"] {
            assert!(
                stated_time(&json!({ "earliest": bad }), "earliest", DEFAULT_EARLIEST).is_err(),
                "{bad}"
            );
        }
    }

    /// The same month-long ceiling `whats_on` has, from the same constant.
    #[test]
    fn a_range_is_a_month_at_most_and_never_backwards() {
        let (from, to) = stated_range(&json!({ "from": "2026-08-17" })).unwrap();
        assert_eq!(from, to, "one day when no last day is stated");
        assert!(stated_range(&json!({ "to": "2026-08-17" })).is_err());
        assert!(
            stated_range(&json!({ "from": "2026-08-17", "to": "2026-08-16" })).is_err(),
            "backwards"
        );
        assert!(stated_range(&json!({ "from": "2026-08-01", "to": "2026-09-30" })).is_err());
        assert!(stated_range(&json!({ "from": "the 17th" })).is_err());
    }
}
