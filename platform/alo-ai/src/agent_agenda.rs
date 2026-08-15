//! The **Agenda** agent's tool set (ADR 0034) — reading a diary, and writing
//! to one.
//!
//! `create_event` joined these in A1.2. It had sat in `crate::agent`'s
//! undifferentiated "core" list since before agents had products, which meant
//! the one tool that writes a diary lived apart from the two that read it, and
//! the Agenda agent would have been offered the reads without the write.
//!
//! The agent could already `create_event`, and could read nothing. It would
//! book a meeting over an existing one and answer "what have I got on
//! Thursday?" from search results — from whatever documents happened to
//! mention Thursday, rather than from the diary. Both tools here are reads,
//! and both exist so an answer about time comes from the calendar.
//!
//! Two rules shape the wording, and each is a mistake it exists to prevent:
//!
//! - **A day is a date, never a phrase.** "Thursday" and "next week" are
//!   resolved by the model against today's date, which the prompt supplies,
//!   and arrive here as `YYYY-MM-DD`. A tool that accepted "next Thursday"
//!   would have to guess a week boundary and a timezone, and would guess
//!   differently from the person asking.
//! - **Busy is not the same as unavailable.** `am_i_free` reports what
//!   overlaps a window; it does not decide whether the person may be
//!   interrupted. A meeting they would happily leave and one they would not
//!   look identical in a database, so the tool reports the clash and lets a
//!   human read it.
//!
//! A2.6 took the agent past the caller's own diary, and added three more rules
//! of the same kind:
//!
//! - **A diary you cannot see is never reported as free.** `find_a_time` looks
//!   only at the diaries already shared with the person asking, and a colleague
//!   whose calendar is not among them comes back in `couldNotCheck` with the
//!   reason. Treating an unreadable diary as an empty one is how an agent
//!   books a meeting over somebody's afternoon and calls it a free slot.
//! - **A meeting is named, and a day disambiguates it.** Neither of the two
//!   tools that act on one existing meeting takes an identifier: the model
//!   passes the words the user said, and a title that matches several sittings
//!   comes back listing their days rather than picking the next one.
//! - **Moving a meeting keeps its length.** A reschedule with no end given
//!   lands the meeting where it was asked for, as long as it already was — the
//!   hour-long default `create_event` uses would silently shorten a
//!   ninety-minute workshop.

use crate::agent_tool::AgentTool;

/// The Agenda tools, each declaring whether it reads or writes (ADR 0047 §1).
///
/// The four lookups run inside the turn and their answer lands in the room with
/// no tap; the two that change a diary — putting a new meeting in one, moving
/// one that is already there — wait for one.
pub const AGENDA_TOOLS: &[AgentTool] = &[
    AgentTool::read("whats_on"),
    AgentTool::read("am_i_free"),
    AgentTool::read("find_a_time"),
    AgentTool::read("meeting_prep"),
    AgentTool::write("create_event"),
    AgentTool::write("reschedule_event"),
];

/// What each Agenda tool takes, in the words the model reads.
///
/// Whether a tool reads or writes is **not** written here: it is declared in
/// [`AGENDA_TOOLS`] and rendered into the prompt from there (ADR 0047 §1), so
/// prose and behaviour cannot drift apart.
pub const AGENDA_TOOL_DOC: &str = "\
- whats_on: read what is in the user's own calendar over a range of days. args: {\"from\": string in \"YYYY-MM-DD\" (the first day, required), \"to\": string in \"YYYY-MM-DD\" (the last day, included; optional — the same day as from when left out)}. Use this whenever the user asks what they have on, what their day or week looks like, or when something is. Never answer such a question from the sources: what is in the diary is in the diary, and a document that mentions a meeting is not evidence it is still happening. A range covers at most 31 days.\n\
- am_i_free: check whether anything already overlaps a specific span of time. args: {\"start\": string RFC 3339 datetime e.g. \"2026-08-13T14:00:00Z\" (required), \"end\": string RFC 3339 (optional; one hour after start when left out)}. Use this BEFORE create_event whenever the user asks to book something at a particular time, so a new meeting is not proposed on top of one they already have. It reports what clashes; it does not decide whether they can be interrupted.\n\
- find_a_time: find the free slots several people share, over a range of days. args: {\"people\": array of strings (the colleagues to include, each named the way the user named them — a first name or an email address; optional, just the user themselves when left out), \"from\": string in \"YYYY-MM-DD\" (the first day to look at, required), \"to\": string in \"YYYY-MM-DD\" (the last day, included; optional — the same day as from when left out), \"minutes\": integer (how long the meeting needs to be, optional, 30 by default, at most 480), \"earliest\": string \"HH:MM\" and \"latest\": string \"HH:MM\" (the working window to look inside, optional, 09:00 to 17:00 by default — both are UTC, so convert the user's working day into UTC yourself using the timezone given below)}. It changes nothing and books nothing. It looks only at diaries already shared with the person asking: a colleague whose calendar is not shared comes back under couldNotCheck, and the slots are then free for the others only — say so, by name, rather than presenting them as free for everybody. A range covers at most 31 days.\n\
- meeting_prep: gather what a meeting already in the diary is about — the meeting itself, and the emails and attachments that go with it. It changes nothing. args: {\"meeting\": string (its title, in the user's own words, required), \"on\": string in \"YYYY-MM-DD\" (which day's sitting, optional — needed when the same meeting is in the diary more than once)}. Use this before writing an agenda, a briefing or a set of talking points, and write them from what comes back rather than from the meeting's title. A title that matches several sittings comes back listing their days: ask which one, never guess.\n\
- create_event: schedule a calendar event. args: {\"title\": string (required), \"start\": string RFC 3339 datetime e.g. \"2026-08-07T14:00:00Z\" (required), \"end\": string RFC 3339 (optional; defaults to one hour after start), \"location\": string (optional), \"notes\": string (optional)}.\n\
- reschedule_event: propose MOVING a meeting that is already in a diary to a new time. It changes nothing else about it — not its title, its guests, its place or its notes — and it cannot cancel one. args: {\"meeting\": string (its title, in the user's own words, required), \"on\": string in \"YYYY-MM-DD\" (which day's sitting is being moved, optional — needed when the same meeting is in the diary more than once), \"start\": string RFC 3339 (the new start, required), \"end\": string RFC 3339 (optional; the meeting keeps its current length when left out)}. One sitting of a repeating meeting is moved on its own and the rest of the series stays where it is. Check the new time with find_a_time or am_i_free BEFORE proposing it, and never say a meeting has been moved until the user has approved it.\n";

/// The rules that keep an Agenda proposal honest, appended to the system prompt.
pub const AGENDA_GUIDANCE: &str = "For an Agenda tool, resolve every relative day (today, tomorrow, Thursday, next week) against today's date given below and pass a real calendar date — never a phrase, and never a day you were not given enough to work out. If the user's meaning is ambiguous (\"Friday\" when it is already Friday), ANSWER and ask which they mean rather than choosing. Ask what is on the calendar with whats_on rather than inferring it from the sources; a document that mentions a meeting is not proof it is still in the diary. Name a meeting and a colleague the way the user did — there is no identifier for either that you could know, so never invent one. You can see only the diaries that have been shared with this person: a colleague find_a_time could not check is NOT free, and an answer that leaves them out has to say whose diary was not read. Never say a meeting has been moved, or that everyone is free, until the tool has actually said so.\n";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_named_tool_is_described() {
        for tool in AGENDA_TOOLS {
            assert!(
                AGENDA_TOOL_DOC.contains(&format!("- {}:", tool.name)),
                "{} is offered to the model with no description",
                tool.name
            );
        }
    }

    /// Reading a diary must never be able to change one: putting a meeting in
    /// a diary and moving one already in it are the only tools here that may,
    /// and both wait for the user's tap (ADR 0047). Naming them explicitly
    /// rather than counting writes, so a third write slipped into this list
    /// fails the test instead of passing it.
    #[test]
    fn nothing_but_creating_and_moving_a_meeting_can_change_a_diary() {
        for tool in AGENDA_TOOLS {
            assert_eq!(
                tool.is_read(),
                !matches!(tool.name, "create_event" | "reschedule_event"),
                "{} is on the wrong side of the read/write split",
                tool.name
            );
        }
    }

    /// A2.6's first rule, in the words the model reads it in: the tool that
    /// looks across diaries says plainly that an unreadable one is not a free
    /// one, and the guidance forbids passing off a partial answer as a whole.
    #[test]
    fn a_diary_that_could_not_be_read_is_never_reported_as_free() {
        assert!(line("find_a_time").contains("couldNotCheck"));
        assert!(line("find_a_time").contains("only at diaries already shared"));
        assert!(AGENDA_GUIDANCE.contains("could not check is NOT free"));
    }

    /// Moving a meeting is not re-creating one: the description says what it
    /// leaves alone, and says the length survives a start-only move.
    #[test]
    fn moving_a_meeting_changes_its_time_and_nothing_else() {
        assert!(line("reschedule_event").contains("changes nothing else"));
        assert!(line("reschedule_event").contains("cannot cancel"));
        assert!(line("reschedule_event").contains("keeps its current length"));
        // One sitting moves; the series does not follow it.
        assert!(line("reschedule_event").contains("rest of the series stays"));
    }

    /// The two tools that act on an existing meeting take the user's own words
    /// and a day, never an id — and a title that matches several sittings is a
    /// question rather than a guess.
    #[test]
    fn a_meeting_is_named_and_a_day_disambiguates_it() {
        for tool in ["meeting_prep", "reschedule_event"] {
            assert!(line(tool).contains("in the user's own words"), "{tool}");
            assert!(line(tool).contains("more than once"), "{tool}");
        }
        assert!(line("meeting_prep").contains("never guess"));
        assert!(AGENDA_GUIDANCE.contains("never invent one"));
    }

    /// Prep is written from what the tool returns, not from the meeting's
    /// title — the same rule the Drive agent's `file_read` carries.
    #[test]
    fn a_briefing_is_written_from_the_meeting_and_not_from_its_name() {
        assert!(line("meeting_prep").contains("rather than from the meeting's title"));
    }

    /// Every tool that only reads says so where the model reads it, so a turn
    /// never offers a button for an answer.
    #[test]
    fn the_reads_say_they_change_nothing() {
        for tool in AGENDA_TOOLS.iter().filter(|tool| tool.is_read()) {
            if tool.name == "whats_on" || tool.name == "am_i_free" {
                continue; // worded in their own terms, before ADR 0047's phrase
            }
            assert!(
                line(tool.name).contains("changes nothing"),
                "{} does not say it changes nothing",
                tool.name
            );
        }
    }

    fn line(name: &str) -> &'static str {
        AGENDA_TOOL_DOC
            .lines()
            .find(|line| line.starts_with(&format!("- {name}:")))
            .unwrap_or_else(|| panic!("{name} is described"))
    }

    /// The whole reason the tool takes a date rather than a word.
    #[test]
    fn the_model_is_told_to_resolve_days_itself() {
        assert!(AGENDA_GUIDANCE.contains("never a phrase"));
        assert!(AGENDA_TOOL_DOC.contains("YYYY-MM-DD"));
    }

    /// Booking over an existing meeting is the failure this set exists to
    /// prevent; the model has to be told when to check.
    #[test]
    fn the_model_is_told_to_check_before_booking() {
        assert!(AGENDA_TOOL_DOC.contains("BEFORE create_event"));
    }
}
