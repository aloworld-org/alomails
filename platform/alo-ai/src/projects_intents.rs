//! alo Projects' verbs (ADR 0058) — the Projects agent over the one command
//! layer.
//!
//! This is the whole of what the Projects agent may do, and the words a model
//! reads about it. Nothing here reads or writes a record: the executors live
//! beside Projects' routes in `alo-jmap` (`projects_intents.rs`), through the
//! asker's tenant-scoped store, and answer with the same figures the
//! `/projects/*` routes serve — the portfolio's, the timesheet's and the
//! plan's, never a second sum.
//!
//! Four rules from the module shape the wording, each a mistake it exists to
//! prevent:
//!
//! - **A project is named, never numbered**, like a deal and unlike an
//!   invoice. The name the user said is passed through verbatim and resolved
//!   against the tenant's own boards; a name a model "tidied" resolves to
//!   nothing, or worse, to somebody else's engagement.
//! - **A duration is whole minutes.** An hour and a half is `90`, never
//!   `1.5`, for the reason money is cents: an hour that arrives as a fraction
//!   is an hour somebody has to round before it can be invoiced.
//! - **A logged hour is a suggestion until a human accepts it.** Both writes
//!   here draft *proposed* entries that are in no total, no submitted week and
//!   no invoice until the person whose timesheet it is says the work happened
//!   (ADR 0023, `docs/design/projects.md` § Proposed entries are not hours).
//!   The wording says so in the model's own words, because a model that
//!   believes it is filing a timesheet writes different notes than one that
//!   knows it is suggesting a line.
//! - **A meeting is evidence of an hour, never of a project.** The calendar
//!   draft takes the project from the user's own words and reads only the
//!   *days* from the diary; a model told it may infer the engagement from a
//!   meeting's title will eventually bill one customer for a call with
//!   another.

use crate::agent_tool::Effect;
use crate::intent::{Arg, Excluded, IntentModule, IntentSpec};

const PROJECT_REQ: Arg = Arg::required(
    "project",
    "text",
    "the project's name, exactly as the user says it",
);

/// The verbs.
pub const PROJECTS_INTENTS: &[IntentSpec] = &[
    // ---- reads ---------------------------------------------------------
    IntentSpec {
        name: "active_projects",
        purpose: "The portfolio as it stands: every project this person can see that is not finished — planned, running or on hold, each with its status, its client and budget when it is client work, its hours to date and its open work. Name a status to see just those, finished ones included.",
        effect: Effect::Read,
        args: &[Arg::optional(
            "status",
            "text",
            "one status exactly — \"planned\", \"active\", \"on_hold\", \"completed\" or \"cancelled\"; leave out for everything unfinished",
        )],
        answers: &[
            "which projects are active",
            "what projects do we have",
            "what is running at the moment",
            "which engagements are on hold",
        ],
        preview: None,
        undo: None,
        routes: &["/projects"],
    },
    IntentSpec {
        name: "project_status_summary",
        purpose: "Where one project stands — hours logged, budget used, milestones and open tasks, read through the same store the screens use. Use this to answer how a project is going: the figures are in the timesheet and the plan, not in the search results.",
        effect: Effect::Read,
        args: &[PROJECT_REQ],
        answers: &[
            "how is the website project going",
            "where are we with the relaunch",
            "how much budget is left on X",
        ],
        preview: None,
        undo: None,
        routes: &["/projects/{id}"],
    },
    IntentSpec {
        name: "who_is_on_what",
        purpose: "Who is carrying what across the boards this person can open: each colleague with open tasks, how many of them are overdue, and which projects the work sits on. It counts open tasks, names nobody's hours and reads no timesheet; a task nobody is assigned to is reported as unassigned rather than under an invented name.",
        effect: Effect::Read,
        args: &[Arg::optional(
            "project",
            "text",
            "one project's name, exactly as the user says it, to look at that board alone",
        )],
        answers: &[
            "who is on what",
            "who is working on which project",
            "how is the work spread across the team",
        ],
        preview: None,
        undo: None,
        // The grouping is the store's own reading of the boards' open tasks;
        // no /projects route serves an assignee roll-up, so this verb stands
        // behind none.
        routes: &[],
    },
    IntentSpec {
        name: "time_this_week",
        purpose: "This person's OWN timesheet over a period — this week unless other days are named: each entry with its day, project and minutes, and the week's totals, suggestions counted apart. It reads the asker's hours and nobody else's.",
        effect: Effect::Read,
        args: &[
            Arg::optional(
                "from",
                "date",
                "first day of the period, YYYY-MM-DD; default: Monday of the current week",
            ),
            Arg::optional(
                "to",
                "date",
                "last day, included, YYYY-MM-DD; default: Sunday of the same week",
            ),
            Arg::optional(
                "project",
                "text",
                "one project's name, exactly as the user says it, to see the hours on that board alone",
            ),
        ],
        answers: &[
            "how much time have I logged this week",
            "what is on my timesheet",
            "how many hours did I put on the relaunch last week",
        ],
        preview: None,
        undo: None,
        routes: &["/projects/time"],
    },
    // ---- writes: propose, then the asker approves ------------------------
    IntentSpec {
        name: "log_time",
        purpose: "Suggest ONE timesheet entry for work somebody did on a project. It is SAVED AS A SUGGESTION for the user to accept in their timesheet — it counts towards nothing until they do, and it is never submitted or invoiced on its own. The duration is WHOLE MINUTES — 90 for an hour and a half, never 1.5. Never invent a day, a duration or a project the user did not give you.",
        effect: Effect::Write,
        args: &[
            PROJECT_REQ,
            Arg::required("date", "date", "the day the work was done, YYYY-MM-DD"),
            Arg::required(
                "minutes",
                "integer",
                "how long the work took, in whole minutes",
            ),
            Arg::optional("note", "text", "what was done"),
            Arg::optional("task", "text", "the task on that project it was done under"),
            Arg::optional(
                "billable",
                "boolean",
                "work on a client project is chargeable unless the user says otherwise",
            ),
        ],
        answers: &[
            "log two hours on the relaunch for yesterday",
            "put 90 minutes on X",
            "note the time I spent on the audit",
        ],
        preview: Some(
            "A timesheet entry of {minutes} minutes on \"{project}\" for {date} will be suggested — yours to accept, in no total until you do.",
        ),
        undo: None,
        // What it writes lands in the proposals of the asker's own timesheet,
        // which is where the app shows it for acceptance.
        routes: &["/projects/time/proposals"],
    },
    IntentSpec {
        name: "draft_timesheet_from_calendar",
        purpose: "Suggest one timesheet entry per meeting in the user's OWN Agenda over a range of days — the way somebody fills in a week they forgot to log. Every entry is SAVED AS A SUGGESTION for the user to accept, exactly like log_time, and counts towards nothing until they do. The project is what the USER says it is and is NEVER taken from a meeting's title. A range covers at most 31 days, and all-day entries are left out because a day marked \"Leave\" is not an hour worked. Never call it to find out what somebody did.",
        effect: Effect::Write,
        args: &[
            Arg::required(
                "project",
                "text",
                "the project those meetings were for, exactly as the user says it — never a meeting's title",
            ),
            Arg::required("from", "date", "the first day of the range, YYYY-MM-DD"),
            Arg::optional(
                "to",
                "date",
                "the last day, included, YYYY-MM-DD; the same day as from when it is left out",
            ),
            Arg::optional("billable", "boolean", "as for log_time"),
        ],
        answers: &[
            "fill in my timesheet from my calendar",
            "log last week from my diary",
            "turn my meetings into hours",
        ],
        preview: Some(
            "One timesheet entry per meeting in your own diary from {from} will be suggested on \"{project}\" — each yours to accept, in no total until you do.",
        ),
        undo: None,
        routes: &["/projects/time/proposals"],
    },
];

/// The Projects routes deliberately without a verb, each with its reason.
pub const PROJECTS_EXCLUDED: &[Excluded] = &[
    Excluded {
        route: "/projects/clients/{id}",
        why: "Pricing an engagement — its customer, rate and budget — is configuration, set by a person in the app.",
    },
    Excluded {
        route: "/projects/timer",
        why: "The running clock is a person's own button in the app.",
    },
    Excluded {
        route: "/projects/timer/start",
        why: "Starting the clock is a person's own button in the app.",
    },
    Excluded {
        route: "/projects/timer/stop",
        why: "Stopping the clock writes the hour it measured; the person who worked it presses it.",
    },
    Excluded {
        route: "/projects/time/{id}",
        why: "Editing or deleting an entry is its worker's own act in the app.",
    },
    Excluded {
        route: "/projects/time/{id}/accept",
        why: "Accepting a suggested hour prices it; the person whose timesheet it is decides, one entry at a time, in the app.",
    },
    Excluded {
        route: "/projects/time/{id}/reject",
        why: "Declining a suggested hour is the same person's decision, one entry at a time, in the app.",
    },
    Excluded {
        route: "/projects/weeks",
        why: "The week list is the timesheet screen's own read; time_this_week answers the question.",
    },
    Excluded {
        route: "/projects/weeks/{monday}/submit",
        why: "Handing a week in is its worker's own act in the app.",
    },
    Excluded {
        route: "/projects/weeks/{monday}/withdraw",
        why: "Taking a week back is its worker's own act in the app.",
    },
    Excluded {
        route: "/projects/approvals",
        why: "Deciding colleagues' weeks is the approver's own queue in the app.",
    },
    Excluded {
        route: "/projects/approvals/{id}/approve",
        why: "Approving somebody's week prices their hours; the approver's own act in the app.",
    },
    Excluded {
        route: "/projects/approvals/{id}/reject",
        why: "Turning a week back deserves the approver's own words in the app.",
    },
    Excluded {
        route: "/projects/approvals/{id}/reopen",
        why: "Reopening a decided week is the approver's own act in the app.",
    },
    Excluded {
        route: "/projects/unbilled",
        why: "The hours-to-invoice view feeds a billing decision; a later intent set.",
    },
    Excluded {
        route: "/projects/invoices",
        why: "Raising an invoice from hours is a human act on the unbilled view — the one-way handoff stays a person's.",
    },
    Excluded {
        route: "/projects/reports/profitability",
        why: "The full report is a screen in the app; project_status_summary answers where one project stands.",
    },
    Excluded {
        route: "/projects/reports/profitability.csv",
        why: "Produces a file.",
    },
    Excluded {
        route: "/projects/milestones",
        why: "Planning milestones is done on the timeline in the app; project_status_summary reads where the plan stands.",
    },
    Excluded {
        route: "/projects/milestones/{id}",
        why: "Editing or deleting a milestone is planning, done in the app.",
    },
    Excluded {
        route: "/projects/milestones/{id}/done",
        why: "Declaring a deliverable reached is a person's own call in the app.",
    },
    Excluded {
        route: "/projects/updates",
        why: "A status update is written in its author's own words in the app.",
    },
    Excluded {
        route: "/projects/tasks/{task_id}/milestone",
        why: "Placing a task in the plan is planning, done in the app.",
    },
    Excluded {
        route: "/projects/templates",
        why: "Marking a board reusable is configuration, set by a person in the app.",
    },
    Excluded {
        route: "/projects/templates/{id}",
        why: "Unmarking a template is configuration, set by a person in the app.",
    },
    Excluded {
        route: "/projects/templates/{id}/instantiate",
        why: "Starting an engagement from a template creates a board; a person does it in the app.",
    },
];

/// The Projects paragraph of the agent's general instructions.
pub const PROJECTS_GUIDANCE: &str = "For a Projects verb, pass the project's name through EXACTLY as the user gave it — never invent, complete or reformat one, and never guess which project was meant. If the user did not name one, ANSWER and ask which. A project has no number: it is found by its name, so it is not in the numbered sources. State every duration in whole minutes, and resolve a relative day (yesterday, last Friday) against today's date below. To answer a question about the projects, USE a reading verb first and answer from what it returned, quoting figures as returned and adding none it did not return. A suggestion log_time or the calendar draft writes is not a filed hour: never tell the user something has been logged, submitted or invoiced — say it is waiting for them. When drafting a timesheet from the calendar, the project is still the user's word and never a meeting's: read only the DAYS from the diary.\n";

/// The module, as the registry reads it.
pub static PROJECTS: IntentModule = IntentModule {
    intents: PROJECTS_INTENTS,
    excluded: PROJECTS_EXCLUDED,
    guidance: PROJECTS_GUIDANCE,
};

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    /// The verbs whose figures are the store's own reading, with no
    /// `/projects` route serving that view — the named exceptions to "every
    /// verb stands behind a route".
    const ROUTELESS: &[&str] = &["who_is_on_what"];

    #[test]
    fn every_verb_has_a_route_a_purpose_and_a_question_it_answers() {
        for intent in PROJECTS_INTENTS {
            assert!(
                !intent.routes.is_empty() || ROUTELESS.contains(&intent.name),
                "{} names no route",
                intent.name
            );
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
    }

    #[test]
    fn names_are_unique_and_the_doc_lists_each_once() {
        let mut names: Vec<&str> = PROJECTS_INTENTS.iter().map(|i| i.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), PROJECTS_INTENTS.len());
        let doc = PROJECTS.doc();
        for intent in PROJECTS_INTENTS {
            assert_eq!(doc.matches(&format!("- {}:", intent.name)).count(), 1);
        }
        assert!(PROJECTS_GUIDANCE.ends_with('\n'));
    }

    #[test]
    fn an_exclusion_is_a_sentence_not_a_shrug() {
        for excluded in PROJECTS_EXCLUDED {
            assert!(
                excluded.why.ends_with('.'),
                "{}: {}",
                excluded.route,
                excluded.why
            );
            assert!(
                !PROJECTS_INTENTS
                    .iter()
                    .any(|i| i.routes.contains(&excluded.route)),
                "{} is both a verb's route and excluded",
                excluded.route
            );
        }
    }

    #[test]
    fn the_wording_asks_for_time_the_only_way_we_store_it() {
        let doc = PROJECTS.doc();
        assert!(doc.contains("WHOLE MINUTES"));
        assert!(PROJECTS_GUIDANCE.contains("whole minutes"));
        // No fractional hour is ever asked for, in any wording.
        assert!(!doc.contains("\"hours\""));
        assert!(!doc.contains("decimal"));
    }

    #[test]
    fn a_drafted_hour_is_a_suggestion_and_the_wording_says_so() {
        let doc = PROJECTS.doc();
        // Both writes say it, so a model that believes it is filing a
        // timesheet cannot form that belief here.
        assert_eq!(doc.matches("SAVED AS A SUGGESTION").count(), 2);
        assert!(doc.contains("counts towards nothing until they do"));
        assert!(PROJECTS_GUIDANCE.contains("is not a filed hour"));
    }

    #[test]
    fn the_calendar_draft_says_where_a_project_comes_from_and_what_it_leaves_out() {
        // The one mistake this verb can make that nothing downstream catches:
        // an engagement read off a meeting's title.
        let calendar = PROJECTS
            .find("draft_timesheet_from_calendar")
            .expect("the calendar draft is a verb");
        assert!(
            calendar
                .purpose
                .contains("NEVER taken from a meeting's title")
        );
        assert!(PROJECTS_GUIDANCE.contains("never a meeting's"));
        // A day marked "Leave" is not an hour, and the model is told so rather
        // than left to discover it from the skipped list.
        assert!(calendar.purpose.contains("all-day entries are left out"));
        // The range is bounded, and the bound is a number the model can obey.
        assert!(calendar.purpose.contains("at most 31 days"));
    }

    #[test]
    fn the_team_read_counts_tasks_and_never_reads_a_timesheet() {
        let who = PROJECTS
            .find("who_is_on_what")
            .expect("who_is_on_what is a verb");
        assert!(who.purpose.contains("names nobody's hours"));
        assert!(who.purpose.contains("reads no timesheet"));
        assert!(
            who.purpose
                .contains("unassigned rather than under an invented name")
        );
        // …and the personal read says whose hours it reads.
        let week = PROJECTS
            .find("time_this_week")
            .expect("time_this_week is a verb");
        assert!(week.purpose.contains("OWN timesheet"));
        assert!(week.purpose.contains("nobody else's"));
    }

    #[test]
    fn nothing_projects_offers_submits_approves_invoices_or_deletes() {
        for forbidden in [
            "delete_time",
            "approve_week",
            "submit_week",
            "invoice_hours",
            "accept_time",
            "create_project",
        ] {
            assert!(PROJECTS.find(forbidden).is_none());
            assert!(!PROJECTS.doc().contains(forbidden));
        }
    }
}
