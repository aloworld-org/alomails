//! The **Meet** agent's tool set (ADR 0034) — a meeting after the fact.
//!
//! Meet was the last product whose agent had no tools at all: it answered from
//! its grounding, which is empty for it, and proposed nothing. A3.2 gives it
//! the two reads that let it say what actually happened in a sitting and the
//! one write that puts the record of it where the people who were there will
//! see it.
//!
//! **This agent is deliberately not a live participant.** It joins nothing,
//! hears nothing and says nothing while a meeting is running — the in-call
//! agent is a media path, it is not decided, and it is not this. Everything
//! here happens once the meeting is over, from what the meeting left behind.
//!
//! Five rules shape the wording, and each is a way the obvious version would be
//! wrong:
//!
//! - **Minutes are written from the meeting, not from its title.** "Q3 budget"
//!   is not evidence that a budget was agreed. `meeting_record` returns the
//!   transcript, the messages sent during the sitting and who was actually
//!   there, and the minutes are written out of those or not at all.
//! - **A meeting is named, never identified.** The model passes the title the
//!   user said and, when it must, the day; a title that ran twice this month
//!   comes back listing the days rather than picking the nearer one — writing a
//!   meeting's minutes into the wrong room publishes them to the wrong people.
//! - **The minutes are the asker's own message.** They land in the meeting's
//!   conversation under the name of the person who approved them, so the model
//!   writes them as that person would.
//! - **Actions become tasks through the Tasks agent, and follow-ups through the
//!   Agenda agent.** `meeting_minutes` posts a message and does nothing else:
//!   there is no `create_task` here and no `create_event`, so a meeting cannot
//!   become a second way of putting work on somebody's board. Whoever wants the
//!   actions written down asks the Tasks agent to capture them out of that room
//!   afterwards, and each one is still accepted or rejected in the task list
//!   (ADR 0023).
//! - **Minutes are written once.** The record includes what has been posted in
//!   the conversation since the meeting ended, so an agent asked twice can see
//!   its own first answer and say so instead of posting a second set.

use crate::agent_tool::AgentTool;

/// What the Meet agent may do (ADR 0047 §1 declares the effect beside the
/// name).
///
/// The two lookups run inside the turn and their answer lands in the room with
/// no tap; the one tool that changes something — a message posted into a
/// conversation other people read — waits for one.
pub const MEET_TOOLS: &[AgentTool] = &[
    AgentTool::read("meetings_recent"),
    AgentTool::read("meeting_record"),
    AgentTool::write("meeting_minutes"),
];

/// What each Meet tool takes, in the words the model reads.
///
/// Whether a tool reads or writes is **not** written here: it is declared in
/// [`MEET_TOOLS`] and rendered into the prompt from there (ADR 0047 §1), so
/// prose and behaviour cannot drift apart.
pub const MEET_TOOL_DOC: &str = "\
- meetings_recent: read the meetings this person was in that have already ENDED — the title, when each one ran, and whether it came out of a conversation. It changes nothing. args: {\"limit\": integer (how many, optional, 10 by default, at most 25)}. Use this whenever the user says \"the last meeting\" or names one you have not seen: there is no identifier for a meeting that you could know, so a meeting is named by its title and, when a title ran more than once, by its day.\n\
- meeting_record: read ONE ended meeting in full — who attended, what was said in it (the live transcript and the messages sent during it), and what has been posted in its conversation SINCE it finished. It changes nothing. args: {\"meeting\": string (its title, in the user's own words, required), \"day\": string in \"YYYY-MM-DD\" (the day it ran, optional — needed only when the title ran more than once)}. Read this before writing anything about a meeting: a title is not evidence of what was decided. Read postedSince too — minutes that are already in the room must not be written a second time.\n\
- meeting_minutes: post the minutes of ONE ended meeting into the conversation the meeting came out of — a summary, the decisions, and the actions people agreed. args: {\"meeting\": string (its title, required), \"day\": string in \"YYYY-MM-DD\" (optional), \"summary\": string (required, a short paragraph in the language of the meeting), \"decisions\": array of strings (optional, at most 20), \"actions\": array of {\"what\": string (required), \"owner\": string (whoever agreed to it, named the way the meeting named them, optional), \"due\": string in \"YYYY-MM-DD\" (optional)} (optional, at most 20)}. The message appears under the person's OWN name, so write it as they would. Every line comes from meeting_record: never write down a decision nobody made, an owner nobody volunteered or a deadline nobody gave. This posts the minutes and NOTHING else — it creates no tasks and no calendar entries. To turn the actions into to-dos, ask the Tasks agent to write down what was agreed in that room; for a follow-up sitting, ask the Agenda agent.\n";

/// The rules that keep a Meet proposal honest, appended to the system prompt.
pub const MEET_GUIDANCE: &str = "For a Meet tool, read the meeting before you say anything about it: what happened in a sitting is in its transcript and its messages, and its title is not evidence of anything. Name a meeting the way the user did and add the day when a title ran more than once; there is no identifier for one that you could know, so never invent one. Resolve every relative day (today, yesterday, Monday) against today's date given below and pass a real calendar date — never a phrase. You can see only the meetings this person was allowed to see, so an answer about what was decided is about those and has to say so rather than sounding like the whole company. A meeting that is still running has no minutes yet: say so, and offer to write them once it has ended. Minutes are a record, not a summary of the title — quote what people actually agreed, attribute a decision only to somebody who made it, and leave out anything the record does not contain. The actions in minutes become to-dos and diary entries through the Tasks and Agenda agents, whose proposals the user accepts one at a time: say that is what you would do next, and never claim a task, an event or a reminder has been created here. Never say the minutes have been posted until the user has approved it.\n";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_named_tool_is_described() {
        for tool in MEET_TOOLS {
            assert!(
                MEET_TOOL_DOC.contains(&format!("- {}:", tool.name)),
                "{} is offered to the model with no description",
                tool.name
            );
        }
    }

    /// Reading a meeting must never be able to change one. The single write is
    /// named rather than counted, so a second one slipped into the list fails
    /// this test instead of passing it.
    #[test]
    fn only_the_one_declared_write_can_change_anything() {
        for tool in MEET_TOOLS {
            assert_eq!(
                tool.is_read(),
                !matches!(tool.name, "meeting_minutes"),
                "{} is on the wrong side of the read/write split",
                tool.name
            );
        }
    }

    /// Every read says, where the model reads it, that it changes nothing — so
    /// a turn never offers a button for an answer.
    #[test]
    fn the_reads_say_they_change_nothing() {
        for tool in MEET_TOOLS.iter().filter(|tool| tool.is_read()) {
            assert!(
                line(tool.name).contains("changes nothing"),
                "{} does not say it changes nothing",
                tool.name
            );
        }
    }

    /// A meeting is named and dated, and a title that ran twice is a question —
    /// the same rule the Agenda agent's `reschedule_event` obeys, for the same
    /// reason.
    #[test]
    fn a_meeting_is_named_and_never_identified() {
        assert!(line("meetings_recent").contains("no identifier"));
        assert!(line("meeting_record").contains("in the user's own words"));
        assert!(line("meeting_record").contains("day"));
        assert!(MEET_GUIDANCE.contains("never invent one"));
    }

    /// The item's whole point: minutes are written out of the record, and the
    /// reader that prevents a second copy is named in the writer's own
    /// description.
    #[test]
    fn minutes_come_out_of_the_record_and_are_never_written_twice() {
        assert!(line("meeting_minutes").contains("Every line comes from meeting_record"));
        assert!(line("meeting_record").contains("postedSince"));
        assert!(line("meeting_record").contains("second time"));
    }

    /// A3.2's ordinary-agent-path rule, in the words the model reads it in:
    /// minutes post a message, and the actions in them become work through the
    /// Tasks and Agenda agents rather than through a second mechanism in Meet.
    #[test]
    fn actions_become_tasks_through_the_tasks_agent_and_not_here() {
        let minutes = line("meeting_minutes");
        assert!(minutes.contains("creates no tasks and no calendar entries"));
        assert!(minutes.contains("ask the Tasks agent"));
        assert!(minutes.contains("ask the Agenda agent"));
        assert!(MEET_GUIDANCE.contains("accepts one at a time"));
    }

    /// The minutes are somebody's own message in a room other people read, so
    /// they are written as that person would write them.
    #[test]
    fn the_minutes_are_the_askers_own_message() {
        assert!(line("meeting_minutes").contains("OWN name"));
        assert!(MEET_GUIDANCE.contains("Never say the minutes have been posted"));
    }

    fn line(name: &str) -> &'static str {
        MEET_TOOL_DOC
            .lines()
            .find(|line| line.starts_with(&format!("- {name}:")))
            .unwrap_or_else(|| panic!("{name} is described"))
    }
}
