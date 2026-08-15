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

use crate::agent_tool::AgentTool;

/// The Agenda tools, each declaring whether it reads or writes (ADR 0047 §1).
///
/// The two lookups run inside the turn and their answer lands in the room with
/// no tap; `create_event` changes a diary and waits for one.
pub const AGENDA_TOOLS: &[AgentTool] = &[
    AgentTool::read("whats_on"),
    AgentTool::read("am_i_free"),
    AgentTool::write("create_event"),
];

/// What each Agenda tool takes, in the words the model reads.
///
/// Whether a tool reads or writes is **not** written here: it is declared in
/// [`AGENDA_TOOLS`] and rendered into the prompt from there (ADR 0047 §1), so
/// prose and behaviour cannot drift apart.
pub const AGENDA_TOOL_DOC: &str = "\
- whats_on: read what is in the user's own calendar over a range of days. args: {\"from\": string in \"YYYY-MM-DD\" (the first day, required), \"to\": string in \"YYYY-MM-DD\" (the last day, included; optional — the same day as from when left out)}. Use this whenever the user asks what they have on, what their day or week looks like, or when something is. Never answer such a question from the sources: what is in the diary is in the diary, and a document that mentions a meeting is not evidence it is still happening. A range covers at most 31 days.\n\
- am_i_free: check whether anything already overlaps a specific span of time. args: {\"start\": string RFC 3339 datetime e.g. \"2026-08-13T14:00:00Z\" (required), \"end\": string RFC 3339 (optional; one hour after start when left out)}. Use this BEFORE create_event whenever the user asks to book something at a particular time, so a new meeting is not proposed on top of one they already have. It reports what clashes; it does not decide whether they can be interrupted.\n\
- create_event: schedule a calendar event. args: {\"title\": string (required), \"start\": string RFC 3339 datetime e.g. \"2026-08-07T14:00:00Z\" (required), \"end\": string RFC 3339 (optional; defaults to one hour after start), \"location\": string (optional), \"notes\": string (optional)}.\n";

/// The rules that keep an Agenda proposal honest, appended to the system prompt.
pub const AGENDA_GUIDANCE: &str = "For an Agenda tool, resolve every relative day (today, tomorrow, Thursday, next week) against today's date given below and pass a real calendar date — never a phrase, and never a day you were not given enough to work out. If the user's meaning is ambiguous (\"Friday\" when it is already Friday), ANSWER and ask which they mean rather than choosing. Ask what is on the calendar with whats_on rather than inferring it from the sources; a document that mentions a meeting is not proof it is still in the diary.\n";

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

    /// Reading a diary must never be able to change one: `create_event` is the
    /// only tool here that may, and it waits for the user's tap (ADR 0047).
    /// Naming it explicitly rather than counting writes, so a second write
    /// slipped into this list fails the test instead of passing it.
    #[test]
    fn nothing_but_create_event_can_change_a_diary() {
        for tool in AGENDA_TOOLS {
            assert_eq!(
                tool.is_read(),
                tool.name != "create_event",
                "{} is on the wrong side of the read/write split",
                tool.name
            );
        }
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
