//! The **Tasks** agent's tool set (ADR 0034) — reading a to-do list, and
//! changing what is on it.
//!
//! It began as one tool. `create_task` had sat in `crate::agent`'s
//! undifferentiated "core" list since before agents had products, which meant
//! the Tasks agent could add to somebody's list and could not read it: asked
//! "what have I got on today?" it answered from whatever the workspace search
//! happened to match, and asked "who is late?" it had nothing to look at at
//! all. A2.7 gives it the three reads that make the answers real and the three
//! writes that let it act on them.
//!
//! Six rules shape the wording, and each is a way the obvious version would be
//! wrong:
//!
//! - **A plate is what is open, not what exists.** `my_plate` reports the
//!   caller's own unfinished work — overdue, today, coming up, and the ones
//!   with no date at all, which are the ones a due-date-shaped answer silently
//!   loses.
//! - **Lateness is a fact; urgency is a judgement.** The reads return the due
//!   date, the priority, what blocks a task and how far its checklist has got,
//!   and the model puts them in order in its answer. Nothing here decides for
//!   the person what matters most, and `set_task_priority` changes the stored
//!   priority only when they asked for it.
//! - **You can only chase somebody about work you can already see.** Both
//!   `overdue_by_owner` and `chase_task` run on the asker's own door, so a
//!   colleague's private board is not among the things that can come back —
//!   the shared boards are.
//! - **A chase is posted in the asker's own name.** The comment lands on the
//!   task as a comment from the person who approved it, not from a robot, so
//!   the model writes the words the way they would.
//! - **Actions extracted from a conversation are proposed twice.** Approving
//!   `capture_actions` does not put tasks on a board: it writes them as
//!   proposals (ADR 0023), and each one is still accepted or rejected in the
//!   Tasks list. Reading the room first with `thread_actions` shows what was
//!   already captured from it, so the same commitment is not written down
//!   twice.
//! - **A task is named, never identified.** The writes take the words the user
//!   said; a title that matches two open tasks comes back listing them rather
//!   than picking one, for the reason a mis-aimed reschedule is worse than a
//!   question.

use crate::agent_tool::AgentTool;

/// What the Tasks agent may do (ADR 0047 §1 declares the effect beside the
/// name).
///
/// The three lookups run inside the turn and their answer lands in the room
/// with no tap; the four that change something — a new task, a changed
/// priority, a comment chasing somebody, a set of actions written down out of
/// a conversation — wait for one.
pub const TASKS_TOOLS: &[AgentTool] = &[
    AgentTool::read("my_plate"),
    AgentTool::read("overdue_by_owner"),
    AgentTool::read("thread_actions"),
    AgentTool::write("create_task"),
    AgentTool::write("set_task_priority"),
    AgentTool::write("chase_task"),
    AgentTool::write("capture_actions"),
];

/// What each Tasks tool takes, in the words the model reads.
///
/// Whether a tool reads or writes is **not** written here: it is declared in
/// [`TASKS_TOOLS`] and rendered into the prompt from there (ADR 0047 §1), so
/// prose and behaviour cannot drift apart.
pub const TASKS_TOOL_DOC: &str = "\
- my_plate: read the user's OWN unfinished tasks — the ones already late, the ones due today, the ones coming up, and the ones with no due date at all. It changes nothing. args: {\"days\": integer (how far ahead \"coming up\" reaches, optional, 14 by default, at most 90)}. Use this whenever the user asks what they have to do, what is on their plate, what is late, or what to do first. Never answer such a question from the sources: a document that mentions a job is not evidence anybody is still doing it.\n\
- overdue_by_owner: read the tasks that are past their due date on the boards this person can already see, grouped by the colleague each one is assigned to. It changes nothing. args: {\"person\": string (only this colleague, named the way the user named them — a first name or an email address; optional, everybody when left out), \"project\": string (only this board, by its name; optional)}. Use this before chasing anybody, so the chase names a real task and a real date. It sees only the boards the user can open: a colleague's private board is not among them, and neither is work on it.\n\
- thread_actions: read a conversation this person can already open, together with the tasks that have ALREADY been written down out of it. It changes nothing. args: {\"room\": string (the room's name, required), \"limit\": integer (how many recent messages, optional, 30 by default, at most 50)}. Use this before capture_actions, and treat everything under alreadyCaptured as done: writing the same commitment down twice is the failure this tool exists to prevent.\n\
- create_task: create ONE to-do for the user, on their own list, active straight away. args: {\"title\": string (required), \"due\": string in \"YYYY-MM-DD\" (optional), \"notes\": string (optional)}. For several actions out of one conversation use capture_actions instead.\n\
- set_task_priority: change the priority of ONE task that already exists, and nothing else about it — not its title, its due date, its owner or its column. args: {\"task\": string (its title, in the user's own words, required), \"priority\": one of \"none\", \"low\", \"medium\", \"high\" (required)}. A title that matches more than one unfinished task comes back listing them: ask which, never guess. Propose this only when the user asked for the priority to change — saying which task matters most is an ANSWER, not a change to their board.\n\
- chase_task: post a comment on a task that is late, asking whoever it is assigned to where it has got to. It changes nothing else about the task, and it cannot reassign it, move it or close it. args: {\"task\": string (its title, in the user's own words, required), \"message\": string (the comment itself, required)}. The comment appears under the person's OWN name, so write it as they would write it: short, specific about which task and how late it is, and never rude. Read overdue_by_owner first so the message names a real date.\n\
- capture_actions: write down the actions agreed in a conversation, as PROPOSED tasks the user still accepts or rejects one by one in their task list. args: {\"room\": string (the room they came out of, required), \"tasks\": array of {\"title\": string (required), \"due\": string in \"YYYY-MM-DD\" (optional), \"notes\": string (optional, why — quote the message it came from)} (required, at most 10)}. Read the room with thread_actions first and leave out anything it lists as already captured. Write each title as the commitment somebody actually made, in their own words, and never invent a deadline nobody gave.\n";

/// The rules that keep a Tasks proposal honest, appended to the system prompt.
pub const TASKS_GUIDANCE: &str = "For a Tasks tool, ask the list rather than the sources: what is on somebody's plate is in their tasks, and a mention of a job in a document is not proof it is still open. Resolve every relative day (today, tomorrow, Friday, next week) against today's date given below and pass a real calendar date — never a phrase. Name a task and a colleague the way the user did; there is no identifier for either that you could know, so never invent one. For create_task and capture_actions, write the title in the user's own words and never invent a due date they did not give you — a task with a deadline nobody set is worse than one with none. Putting the user's work in order is an ANSWER: say what you would do first and why, and only propose set_task_priority when they asked for the priority itself to change. You can see only the boards this person can already open, so an answer about who is late is about those boards and has to say so rather than sounding like the whole company. Never say a task has been created, chased or reprioritised until the user has approved it.\n";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_named_tool_is_described() {
        for tool in TASKS_TOOLS {
            assert!(
                TASKS_TOOL_DOC.contains(&format!("- {}:", tool.name)),
                "{} is offered to the model with no description",
                tool.name
            );
        }
    }

    /// Reading a to-do list must never be able to change one. The four writes
    /// are named explicitly rather than counted, so a fifth one slipped into
    /// the list fails this test instead of passing it.
    #[test]
    fn only_the_four_declared_writes_can_change_a_list() {
        for tool in TASKS_TOOLS {
            assert_eq!(
                tool.is_read(),
                !matches!(
                    tool.name,
                    "create_task" | "set_task_priority" | "chase_task" | "capture_actions"
                ),
                "{} is on the wrong side of the read/write split",
                tool.name
            );
        }
    }

    /// Every read says, where the model reads it, that it changes nothing — so
    /// a turn never offers a button for an answer.
    #[test]
    fn the_reads_say_they_change_nothing() {
        for tool in TASKS_TOOLS.iter().filter(|tool| tool.is_read()) {
            assert!(
                line(tool.name).contains("changes nothing"),
                "{} does not say it changes nothing",
                tool.name
            );
        }
    }

    /// A plate that only reported dated work would lose exactly the tasks
    /// nobody has looked at — the ones with no date on them.
    #[test]
    fn a_plate_includes_the_tasks_with_no_date() {
        assert!(line("my_plate").contains("no due date at all"));
        assert!(line("my_plate").contains("OWN"));
    }

    /// A2.7's reach rule, in the words the model reads it in: an answer about
    /// who is late is about the boards this person can open, and says so.
    #[test]
    fn chasing_reaches_only_the_boards_the_asker_can_open() {
        assert!(line("overdue_by_owner").contains("already see"));
        assert!(line("overdue_by_owner").contains("private board is not among them"));
        assert!(TASKS_GUIDANCE.contains("boards this person can already open"));
    }

    /// The chase is the asker's own comment, not a robot's, and it can do
    /// nothing else to the task.
    #[test]
    fn a_chase_is_a_comment_in_the_askers_own_name() {
        assert!(line("chase_task").contains("OWN name"));
        assert!(line("chase_task").contains("cannot reassign it"));
    }

    /// Extraction is proposed twice on purpose (ADR 0023), and the reader that
    /// prevents a second copy is named in the writer's own description.
    #[test]
    fn actions_are_captured_as_proposals_and_never_twice() {
        assert!(line("capture_actions").contains("PROPOSED"));
        assert!(line("capture_actions").contains("accepts or rejects"));
        assert!(line("capture_actions").contains("thread_actions first"));
        assert!(line("thread_actions").contains("alreadyCaptured"));
    }

    /// Ordering somebody's work is something the agent *says*, not something it
    /// writes to their board.
    #[test]
    fn putting_work_in_order_is_an_answer_and_not_a_write() {
        assert!(TASKS_GUIDANCE.contains("Putting the user's work in order is an ANSWER"));
        assert!(line("set_task_priority").contains("is an ANSWER"));
    }

    /// A task, like a meeting, is named rather than identified — and a title
    /// that matches two of them is a question.
    #[test]
    fn a_task_is_named_and_ambiguity_is_a_question() {
        for tool in ["set_task_priority", "chase_task"] {
            assert!(line(tool).contains("in the user's own words"), "{tool}");
        }
        assert!(line("set_task_priority").contains("never guess"));
        assert!(TASKS_GUIDANCE.contains("never invent one"));
    }

    fn line(name: &str) -> &'static str {
        TASKS_TOOL_DOC
            .lines()
            .find(|line| line.starts_with(&format!("- {name}:")))
            .unwrap_or_else(|| panic!("{name} is described"))
    }
}
