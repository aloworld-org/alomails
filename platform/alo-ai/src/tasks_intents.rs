//! alo Tasks' verbs (ADR 0058, queue item AB.4) — the whole of what the Tasks
//! agent may do, and the words a model reads about it.
//!
//! Nothing here reads or writes a board: the executors live in `alo-jmap`
//! (`tasks_intents.rs`, with the six older tool executors it keeps in
//! `agent_tasks.rs`), through the asker's tenant-scoped store — a board the
//! asker could not open is not among the things that can be named.
//!
//! The rules the hand-written tool set learned, kept because each one is a
//! mistake it exists to prevent:
//!
//! - **A plate is what is open, not what exists.** `my_plate` reports the
//!   caller's own unfinished work — overdue, today, coming up, and the ones
//!   with no date at all, which are the ones a due-date-shaped answer silently
//!   loses.
//! - **You can only reach the boards you can already open.** Every read and
//!   every write runs on the asker's own door, so a colleague's private board
//!   is not among the things that can come back — the shared boards are, and
//!   an answer about who is late has to say that is its reach.
//! - **A chase is posted in the asker's own name.** The comment lands on the
//!   task as a comment from the person who approved it, not from a robot.
//! - **Actions extracted from a conversation are proposed twice.** Approving
//!   `capture_actions` writes them as proposals (ADR 0023), each still
//!   accepted or rejected in the Tasks list; `thread_actions` first, so the
//!   same commitment is not written down twice.
//! - **A task is named, never identified.** The writes take the words the
//!   user said; a title that matches two tasks comes back listing them rather
//!   than picking one, because a mis-aimed write is worse than a question.
//! - **Finishing and handing over are the user's word, not the agent's
//!   guess.** AB.4 adds `complete_task` and `reassign_task`, and both wait
//!   for a tap: closing somebody's work uninvited, or moving it to somebody
//!   else's plate, is exactly what a to-do agent must not do on a hunch.

use crate::agent_tool::Effect;
use crate::intent::{Arg, Excluded, IntentModule, IntentSpec};

const TASK_ARG: Arg = Arg::required("task", "text", "its title, in the user's own words");

/// The verbs.
pub const TASKS_INTENTS: &[IntentSpec] = &[
    // ---- reads ---------------------------------------------------------
    IntentSpec {
        name: "my_plate",
        purpose: "The user's OWN unfinished tasks — the ones already late, the ones due today, the ones coming up, and the ones with no due date at all, which a due-date-shaped answer silently loses. It changes nothing. Use it whenever the user asks what they have to do, what is late, or what to do first, and never answer such a question from the sources: a document that mentions a job is not evidence anybody is still doing it.",
        effect: Effect::Read,
        args: &[Arg::optional(
            "days",
            "integer",
            "how far ahead \"coming up\" reaches, 14 by default, at most 90",
        )],
        answers: &[
            "what have I got on",
            "what is on my plate",
            "what should I do first",
        ],
        preview: None,
        undo: None,
        routes: &["/tasks/today"],
    },
    IntentSpec {
        name: "overdue_by_owner",
        purpose: "The tasks past their due date on the boards this person can already see, grouped by the colleague each one is assigned to. It changes nothing. Read it before chasing anybody, so the chase names a real task and a real date. A colleague's private board is not among them, and neither is work on it — so say the answer is about the boards the user can open rather than sounding like the whole company.",
        effect: Effect::Read,
        args: &[
            Arg::optional(
                "person",
                "text",
                "only this colleague, named the way the user named them — a first name or an email address; everybody when left out",
            ),
            Arg::optional("project", "text", "only this board, by its name"),
        ],
        answers: &["who is late", "which tasks are overdue"],
        preview: None,
        undo: None,
        routes: &["/tasks/due"],
    },
    IntentSpec {
        name: "thread_actions",
        purpose: "A conversation this person can already open, together with the tasks that have ALREADY been written down out of it. It changes nothing. Read it before capture_actions, and treat everything under alreadyCaptured as done: writing the same commitment down twice is the failure this verb exists to prevent.",
        effect: Effect::Read,
        args: &[
            Arg::required("room", "text", "the room's name"),
            Arg::optional(
                "limit",
                "integer",
                "how many recent messages, 30 by default, at most 50",
            ),
        ],
        answers: &["what did we agree in the launch room"],
        preview: None,
        undo: None,
        routes: &[],
    },
    IntentSpec {
        name: "board_tasks",
        purpose: "The open tasks of ONE board this person can already open, by the board's name — each with its column, its owner, its due date and how far its checklist has got. It changes nothing. What \"where are we with the launch board\" means to the people standing at it; for the user's own work across every board, my_plate is the read.",
        effect: Effect::Read,
        args: &[Arg::required("board", "text", "the board, by its name")],
        answers: &[
            "what is open on the launch board",
            "what is left to do on the website board",
        ],
        preview: None,
        undo: None,
        routes: &["/tasks"],
    },
    IntentSpec {
        name: "task_lookup",
        purpose: "ONE task in full, by its title — its notes, its checklist, its comments, who follows it, what blocks it and what has happened to it, finished or not. It changes nothing. Use it when the user asks where a piece of work has got to; a title that matches more than one task comes back listing them: ask which, never guess.",
        effect: Effect::Read,
        args: &[TASK_ARG],
        answers: &[
            "where is the pricing sheet task",
            "what is happening with the venue booking",
        ],
        preview: None,
        undo: None,
        routes: &["/tasks/{id}"],
    },
    // ---- writes: propose, then the asker approves ------------------------
    IntentSpec {
        name: "create_task",
        purpose: "Create ONE to-do for the user, on their own list, active straight away. For several actions out of one conversation use capture_actions instead. Write the title in the user's own words, and never invent a due date they did not give.",
        effect: Effect::Write,
        args: &[
            Arg::required("title", "text", "the to-do, in the user's own words"),
            Arg::optional("due", "date", "\"YYYY-MM-DD\""),
            Arg::optional("notes", "text", "one or two sentences of context"),
        ],
        answers: &["remind me to send the deck"],
        preview: Some("A task \"{title}\" will be added to your own list, active straight away."),
        undo: None,
        routes: &["/tasks"],
    },
    IntentSpec {
        name: "set_task_priority",
        purpose: "Change the priority of ONE task that already exists, and nothing else about it — not its title, its due date, its owner or its column. A title that matches more than one unfinished task comes back listing them: ask which, never guess. Propose this only when the user asked for the priority to change — saying which task matters most is an ANSWER, not a change to their board.",
        effect: Effect::Write,
        args: &[
            TASK_ARG,
            Arg::required(
                "priority",
                "text",
                "one of \"none\", \"low\", \"medium\", \"high\"",
            ),
        ],
        answers: &["make the pricing sheet high priority"],
        preview: Some(
            "The priority of \"{task}\" becomes {priority}; nothing else about it changes.",
        ),
        undo: None,
        routes: &["/tasks/{id}"],
    },
    IntentSpec {
        name: "chase_task",
        purpose: "Post a comment on a task that is late, asking whoever it is assigned to where it has got to. It changes nothing else about the task — it cannot reassign it, move it or close it. The comment appears under the person's OWN name, so write it as they would: short, specific about which task and how late it is, and never rude. Read overdue_by_owner first so the message names a real date.",
        effect: Effect::Write,
        args: &[
            TASK_ARG,
            Arg::required("message", "text", "the comment itself"),
        ],
        answers: &["chase Ben about the pricing sheet"],
        preview: Some("Your comment will be posted on \"{task}\", in your own name."),
        undo: None,
        routes: &["/tasks/{id}/comments"],
    },
    IntentSpec {
        name: "capture_actions",
        purpose: "Write down the actions agreed in a conversation, as PROPOSED tasks the user still accepts or rejects one by one in their task list. Read the room with thread_actions first and leave out anything it lists as already captured. Write each title as the commitment somebody actually made, in their own words, and never invent a deadline nobody gave.",
        effect: Effect::Write,
        args: &[
            Arg::required("room", "text", "the room the actions came out of"),
            Arg::required(
                "tasks",
                "array",
                "the actions, each {\"title\": the commitment (required), \"due\": \"YYYY-MM-DD\" (optional), \"notes\": why — quote the message it came from (optional)}; at most 10",
            ),
        ],
        answers: &["write down what we agreed in the launch room"],
        preview: Some(
            "What {room} agreed will be written down as proposed tasks you still accept one by one in Tasks.",
        ),
        undo: None,
        routes: &["/tasks/propose"],
    },
    IntentSpec {
        name: "complete_task",
        purpose: "Move ONE unfinished task to its board's done column. Propose it only when the user SAID the work is finished — a task that looks done is not one, and finishing somebody's work for them uninvited is exactly what this agent must not do. A title that matches more than one unfinished task comes back listing them: ask which, never guess. A person can drag it back out of done on the board itself.",
        effect: Effect::Write,
        args: &[TASK_ARG],
        answers: &["mark the pricing sheet done", "the deck is finished"],
        preview: Some("\"{task}\" will be moved to done on its board."),
        undo: None,
        routes: &["/tasks/{id}/move"],
    },
    IntentSpec {
        name: "reassign_task",
        purpose: "Hand ONE unfinished task to a named colleague, and change nothing else about it — not its title, its due date, its priority or its column. Name them the way the user did — a first name or an email address; a name that matches nobody, or more than one person, comes back asking rather than guessing, and only the people already working on the boards the user can open can be named. Say in your own sentence who has it after the tap.",
        effect: Effect::Write,
        args: &[
            TASK_ARG,
            Arg::required(
                "to",
                "text",
                "the colleague to hand it to — a first name or an email address; \"me\" for the user themselves",
            ),
        ],
        answers: &[
            "give the pricing sheet to Ben",
            "take the venue booking off my plate and give it to Sam",
        ],
        preview: Some("\"{task}\" will be handed to {to}; nothing else about it changes."),
        undo: None,
        routes: &["/tasks/{id}"],
    },
];

/// The routes deliberately kept from the agent, each with its reason — the
/// other half of the coverage test in `alo-jmap`'s `tasks_intents.rs`.
pub const TASKS_EXCLUDED: &[Excluded] = &[
    Excluded {
        route: "/tasks/projects",
        why: "Boards are made, renamed and archived by people; the agent works on the tasks inside the ones that exist.",
    },
    Excluded {
        route: "/tasks/files",
        why: "A board's file tab lists Drive records; files are the Drive agent's subject.",
    },
    Excluded {
        route: "/tasks/dependencies",
        why: "The board-wide dependency graph is a person's planning screen; task_lookup reports what blocks one task.",
    },
    Excluded {
        route: "/tasks/labels",
        why: "The label taxonomy is the team's shared vocabulary; making and deleting words in it is a human act.",
    },
    Excluded {
        route: "/tasks/labels/{id}",
        why: "Deleting a label strips it from every task at once; nothing here deletes.",
    },
    Excluded {
        route: "/tasks/proposals",
        why: "The waiting proposals are the user's own accept list (ADR 0023); thread_actions reports the captured ones per room.",
    },
    Excluded {
        route: "/tasks/{id}/accept",
        why: "Accepting a captured action is the second yes ADR 0023 requires, and it is the person's own tap in their list.",
    },
    Excluded {
        route: "/tasks/{id}/reject",
        why: "Declining work is the person's own decision; an agent that could reject proposals could silently lose them.",
    },
    Excluded {
        route: "/tasks/{id}/subtasks",
        why: "A checklist is shaped in the task's own screen; a later intent set.",
    },
    Excluded {
        route: "/tasks/{id}/subtasks/{sid}",
        why: "Ticking somebody's checklist is finishing their work in small steps; a later intent set.",
    },
    Excluded {
        route: "/tasks/{id}/followers",
        why: "Following is a person's own subscription; nobody is signed up to notifications by an agent.",
    },
    Excluded {
        route: "/tasks/{id}/attachments",
        why: "Takes an upload; a file arrives on a task by a person putting it there.",
    },
    Excluded {
        route: "/tasks/{id}/attachments/{aid}",
        why: "Removing somebody's attachment is a deletion; nothing here deletes.",
    },
    Excluded {
        route: "/tasks/{id}/attachments/{aid}/download",
        why: "Serves a file; reading an attachment's text is the Drive agent's attachment_read.",
    },
    Excluded {
        route: "/tasks/{id}/labels",
        why: "Labelling is a later intent set; the reads report a task as it stands.",
    },
    Excluded {
        route: "/tasks/{id}/labels/{lid}",
        why: "Labelling is a later intent set; the reads report a task as it stands.",
    },
    Excluded {
        route: "/tasks/{id}/dependencies",
        why: "Declaring what blocks what is a person's planning act; task_lookup reports it.",
    },
    Excluded {
        route: "/tasks/{id}/dependencies/{dep}",
        why: "Undoing a dependency is the same planning act; task_lookup reports what stands.",
    },
];

/// The Tasks paragraph of the agent's general instructions.
pub const TASKS_GUIDANCE: &str = "For a Tasks verb, ask the list rather than the sources: what is on somebody's plate is in their tasks, and a mention of a job in a document is not proof it is still open. Resolve every relative day (today, tomorrow, Friday, next week) against today's date given below and pass a real calendar date — never a phrase. Name a task and a colleague the way the user did; there is no identifier for either that you could know, so never invent one. For create_task and capture_actions, write the title in the user's own words and never invent a due date they did not give you — a task with a deadline nobody set is worse than one with none. Putting the user's work in order is an ANSWER: say what you would do first and why, and only propose set_task_priority when they asked for the priority itself to change. Propose complete_task only when the user said the work is finished, and reassign_task only when they said who should have it — never either on your own reading of the board. You can see only the boards this person can already open, so an answer about who is late is about those boards and has to say so rather than sounding like the whole company. Never say a task has been created, completed, reassigned, chased or reprioritised until the user has approved it.\n";

/// The module, as the registry reads it.
pub static TASKS: IntentModule = IntentModule {
    intents: TASKS_INTENTS,
    excluded: TASKS_EXCLUDED,
    guidance: TASKS_GUIDANCE,
};

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn every_verb_has_a_purpose_a_question_and_a_write_its_preview() {
        for intent in TASKS_INTENTS {
            assert!(
                intent.purpose.ends_with('.'),
                "{} purpose is not a sentence",
                intent.name
            );
            assert!(
                !intent.answers.is_empty(),
                "{} answers nothing",
                intent.name
            );
            if intent.effect == Effect::Write {
                assert!(
                    intent.preview.is_some(),
                    "{} is a write without a preview",
                    intent.name
                );
            }
        }
        for excluded in TASKS_EXCLUDED {
            assert!(
                excluded.route.starts_with("/tasks"),
                "{} is not a Tasks route",
                excluded.route
            );
            assert!(
                excluded.why.ends_with('.'),
                "{} has no sentence for a reason",
                excluded.route
            );
        }
    }

    #[test]
    fn names_are_unique_and_the_doc_lists_each_once() {
        let mut names: Vec<&str> = TASKS_INTENTS.iter().map(|i| i.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), TASKS_INTENTS.len());
        let doc = TASKS.doc();
        for intent in TASKS_INTENTS {
            assert_eq!(doc.matches(&format!("- {}:", intent.name)).count(), 1);
        }
        assert!(TASKS_GUIDANCE.ends_with('\n'));
    }

    /// The five reads answer inside the turn and say so; the six writes wait.
    /// Named explicitly rather than counted, so a seventh write slipped into
    /// the list fails this test instead of passing it.
    #[test]
    fn the_reads_answer_and_every_write_waits_for_a_tap() {
        let find = |name: &str| TASKS.find(name).unwrap_or_else(|| panic!("{name}"));
        for reads in [
            "my_plate",
            "overdue_by_owner",
            "thread_actions",
            "board_tasks",
            "task_lookup",
        ] {
            assert_eq!(find(reads).effect, Effect::Read, "{reads}");
            assert!(find(reads).purpose.contains("changes nothing"), "{reads}");
        }
        for intent in TASKS_INTENTS {
            assert_eq!(
                intent.effect == Effect::Write,
                matches!(
                    intent.name,
                    "create_task"
                        | "set_task_priority"
                        | "chase_task"
                        | "capture_actions"
                        | "complete_task"
                        | "reassign_task"
                ),
                "{} is on the wrong side of the read/write split",
                intent.name
            );
        }
        assert!(TASKS_GUIDANCE.contains("Never say a task has been created"));
    }

    /// A plate that only reported dated work would lose exactly the tasks
    /// nobody has looked at — the ones with no date on them.
    #[test]
    fn a_plate_includes_the_tasks_with_no_date() {
        let plate = TASKS.find("my_plate").unwrap().purpose;
        assert!(plate.contains("no due date at all"), "{plate}");
        assert!(plate.contains("OWN"), "{plate}");
    }

    /// The reach rule, in the words the model reads it in: an answer about who
    /// is late is about the boards this person can open, and says so.
    #[test]
    fn the_reach_is_the_boards_the_asker_can_open() {
        let overdue = TASKS.find("overdue_by_owner").unwrap().purpose;
        assert!(overdue.contains("already see"), "{overdue}");
        assert!(
            overdue.contains("private board is not among them"),
            "{overdue}"
        );
        let board = TASKS.find("board_tasks").unwrap().purpose;
        assert!(board.contains("already open"), "{board}");
        assert!(TASKS_GUIDANCE.contains("boards this person can already open"));
    }

    /// The chase is the asker's own comment, not a robot's, and it can do
    /// nothing else to the task.
    #[test]
    fn a_chase_is_a_comment_in_the_askers_own_name() {
        let chase = TASKS.find("chase_task").unwrap().purpose;
        assert!(chase.contains("OWN name"), "{chase}");
        assert!(chase.contains("cannot reassign it"), "{chase}");
    }

    /// Extraction is proposed twice on purpose (ADR 0023), and the reader that
    /// prevents a second copy is named in the writer's own description.
    #[test]
    fn actions_are_captured_as_proposals_and_never_twice() {
        let capture = TASKS.find("capture_actions").unwrap().purpose;
        assert!(capture.contains("PROPOSED"), "{capture}");
        assert!(capture.contains("accepts or rejects"), "{capture}");
        assert!(capture.contains("thread_actions first"), "{capture}");
        assert!(
            TASKS
                .find("thread_actions")
                .unwrap()
                .purpose
                .contains("alreadyCaptured")
        );
        // …and the person's own accept list stays theirs: the accept and
        // reject routes are excluded, not verbs.
        for route in ["/tasks/{id}/accept", "/tasks/{id}/reject"] {
            assert!(
                TASKS_EXCLUDED
                    .iter()
                    .any(|excluded| excluded.route == route),
                "{route} must stay the person's own tap"
            );
        }
    }

    /// Ordering somebody's work is something the agent *says*, not something
    /// it writes to their board.
    #[test]
    fn putting_work_in_order_is_an_answer_and_not_a_write() {
        assert!(TASKS_GUIDANCE.contains("Putting the user's work in order is an ANSWER"));
        assert!(
            TASKS
                .find("set_task_priority")
                .unwrap()
                .purpose
                .contains("is an ANSWER")
        );
    }

    /// A task is named rather than identified, a title that matches two of
    /// them is a question — and the two verbs AB.4 adds carry the same rule.
    #[test]
    fn a_task_is_named_and_ambiguity_is_a_question() {
        for tool in [
            "task_lookup",
            "set_task_priority",
            "chase_task",
            "complete_task",
            "reassign_task",
        ] {
            let args = TASKS.find(tool).unwrap().args;
            let task = args.iter().find(|arg| arg.name == "task").unwrap();
            assert!(task.purpose.contains("in the user's own words"), "{tool}");
        }
        assert!(TASKS_GUIDANCE.contains("never invent one"));
    }

    /// AB.4's two writes, held to their own sentences: completing is the
    /// user's word and recoverable on the board; a handover changes the owner
    /// and nothing else, out of the people already on the visible boards.
    #[test]
    fn finishing_and_handing_over_are_bounded() {
        let complete = TASKS.find("complete_task").unwrap().purpose;
        assert!(
            complete.contains("user SAID the work is finished"),
            "{complete}"
        );
        assert!(complete.contains("drag it back out of done"), "{complete}");
        let reassign = TASKS.find("reassign_task").unwrap().purpose;
        assert!(reassign.contains("change nothing else"), "{reassign}");
        assert!(
            reassign.contains("matches nobody, or more than one person"),
            "{reassign}"
        );
        assert!(
            reassign.contains("already working on the boards"),
            "{reassign}"
        );
        assert!(TASKS_GUIDANCE.contains("only when the user said the work is finished"));
    }
}
