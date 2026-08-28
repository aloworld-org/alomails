//! The **Projects** tool set of the agent (ADR 0034, ADR 0035 wave B3.10) — the
//! names alo Projects contributes to the one agent, and the words that tell a
//! model what they take.
//!
//! The third product on the seam the Billing and CRM tool sets opened (both
//! since moved to intents): a product agent is a tool set plus a
//! paragraph, not a second system. Nothing here reads, writes or decides
//! anything — the proposal is parsed by [`crate::agent`], and an approved
//! proposal is executed by the jmap layer against the caller's tenant-scoped
//! store.
//!
//! Three rules shape the wording below, and each of them is a mistake it exists
//! to prevent:
//!
//! - **A project is named, never numbered**, like a deal and unlike an invoice.
//!   The name the user said is passed through verbatim and resolved against the
//!   tenant's own boards; a name a model "tidied" resolves to nothing, or worse,
//!   to somebody else's engagement.
//! - **A duration is whole minutes.** An hour and a half is `90`, never `1.5`,
//!   for the reason money is cents: an hour that arrives as a fraction is an
//!   hour somebody has to round before it can be invoiced.
//! - **A logged hour is a suggestion until a human accepts it.** `log_time`
//!   writes a *proposed* entry that is in no total, no submitted week and no
//!   invoice until the person whose timesheet it is says the work happened
//!   (ADR 0023, `docs/design/projects.md` § Proposed entries are not hours).
//!   The description says so in the model's own words, because a model that
//!   believes it is filing a timesheet writes different notes than one that
//!   knows it is suggesting a line.
//! - **A meeting is evidence of an hour, never of a project.** The calendar
//!   draft (B3.10b) takes the project from the user's own words and reads only
//!   the *days* from the diary; a model told it may infer the engagement from a
//!   meeting's title will eventually bill one customer for a call with another.
//!
//! ADR 0035 wave B3.10 ends here: three tools, two of which write nothing but a
//! suggestion, and one that writes nothing at all.
//!
//! Nothing here invoices anybody: the hours→invoice handoff (B3.06) is a human
//! act on the unbilled view, and no tool in this list can reach it.

use crate::agent_tool::AgentTool;

/// The Projects tools, each declaring whether it reads or writes (ADR 0047 §1).
///
/// The jmap layer validates an approved tool against the union of this list and
/// every other product's ([`crate::is_agent_tool`]) and owns the execution of
/// each.
pub const PROJECTS_TOOLS: &[AgentTool] = &[
    AgentTool::write("log_time"),
    AgentTool::read("project_status_summary"),
    AgentTool::write("draft_timesheet_from_calendar"),
];

/// The description of each Projects tool, spliced into the agent's system
/// prompt after the CRM tools ([`crate::agent`]).
///
/// Every line ends with a newline so the block concatenates into the list above
/// it without the caller knowing how many tools Projects has.
pub const PROJECTS_TOOL_DOC: &str = "\
- log_time: suggest a timesheet entry for work somebody did on a project. It is SAVED AS A SUGGESTION for the user to accept in their timesheet — it counts towards nothing until they do, and it is never submitted or invoiced on its own. args: {\"project\": string (the project's name, exactly as the user says it, required), \"date\": string in \"YYYY-MM-DD\" (the day the work was done, required), \"minutes\": integer (WHOLE MINUTES — 90 for an hour and a half, never 1.5, required), \"note\": string (what was done, optional), \"task\": string (the task on that project it was done under, optional), \"billable\": boolean (optional; work on a client project is chargeable unless the user says otherwise)}. Never invent a day, a duration or a project the user did not give you.\n\
- project_status_summary: report where one project stands — hours logged, budget used, milestones, and open tasks. args: {\"project\": string (the project's name, exactly as the user says it, required)}. Use this instead of answering from the sources when the user asks how a project is going: the figures are in the timesheet and the plan, not in the search results.\n\
- draft_timesheet_from_calendar: suggest one timesheet entry per meeting in the user's OWN Agenda over a range of days — the way somebody fills in a week they forgot to log. Every entry is SAVED AS A SUGGESTION for the user to accept, exactly like log_time, and counts towards nothing until they do. args: {\"project\": string (the project those meetings were for, exactly as the user says it, required — the project is what the USER says it is and is never taken from a meeting's title), \"from\": string in \"YYYY-MM-DD\" (the first day of the range, required), \"to\": string in \"YYYY-MM-DD\" (the last day, included; optional — the same day as from when it is left out), \"billable\": boolean (optional; as for log_time)}. A range covers at most 31 days, and all-day entries are left out because a day marked \"Leave\" is not an hour worked. Propose this when the user asks to fill in a timesheet from their calendar or diary; never call it to find out what somebody did.\n";

/// The Projects paragraph of the agent's general instructions.
///
/// Kept apart from the per-tool lines above because it says what the *product*
/// is, not what a tool takes: it is the sentence that stops a model rewriting a
/// project's name on its way to the store, and the one that tells it what to do
/// when the user's words name no project at all.
pub const PROJECTS_GUIDANCE: &str = "For a projects tool, pass the project's name through EXACTLY as the user gave it — never invent, complete or reformat one, and never guess which project was meant. If the user did not name one, ANSWER and ask which. A project has no number: it is found by its name, so it is not in the numbered sources. State every duration in whole minutes, and resolve a relative day (yesterday, last Friday) against today's date below. When drafting a timesheet from the calendar, the project is still the user's word and never a meeting's: read only the DAYS from the diary.\n";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_projects_tool_is_described_to_the_model() {
        for tool in PROJECTS_TOOLS {
            assert!(
                PROJECTS_TOOL_DOC.contains(&format!("- {}:", tool.name)),
                "{} has no description in the prompt",
                tool.name
            );
        }
        // …and nothing is described that cannot be executed.
        let described = PROJECTS_TOOL_DOC.matches("\n- ").count() + 1;
        assert_eq!(described, PROJECTS_TOOLS.len());
    }

    #[test]
    fn the_block_concatenates_cleanly_into_a_list() {
        assert!(PROJECTS_TOOL_DOC.ends_with('\n'));
        assert!(PROJECTS_TOOL_DOC.starts_with("- "));
        assert!(PROJECTS_GUIDANCE.ends_with('\n'));
    }

    #[test]
    fn the_wording_asks_for_time_the_only_way_we_store_it() {
        assert!(PROJECTS_TOOL_DOC.contains("WHOLE MINUTES"));
        assert!(PROJECTS_GUIDANCE.contains("whole minutes"));
        // No fractional hour is ever asked for, in any wording.
        assert!(!PROJECTS_TOOL_DOC.contains("\"hours\""));
        assert!(!PROJECTS_TOOL_DOC.contains("decimal"));
    }

    #[test]
    fn the_wording_states_the_two_rules_a_model_would_otherwise_get_wrong() {
        // A logged hour is a suggestion, not a filed timesheet line.
        assert!(PROJECTS_TOOL_DOC.contains("SAVED AS A SUGGESTION"));
        assert!(PROJECTS_TOOL_DOC.contains("counts towards nothing until they do"));
        // The summary is a read — declared in the registry and rendered into
        // the prompt from there (ADR 0047 §1), so it now runs inside the turn
        // and grounds the answer instead of being offered as a button.
        assert!(crate::is_read_tool("project_status_summary"));
        assert!(!crate::is_read_tool("log_time"), "a logged hour is a write");
    }

    #[test]
    fn the_calendar_draft_says_where_a_project_comes_from_and_what_it_leaves_out() {
        // The one mistake this tool can make that nothing downstream catches:
        // an engagement read off a meeting's title.
        assert!(PROJECTS_TOOL_DOC.contains("never taken from a meeting's title"));
        assert!(PROJECTS_GUIDANCE.contains("never a meeting's"));
        // A day marked "Leave" is not an hour, and the model is told so rather
        // than left to discover it from the skipped list.
        assert!(PROJECTS_TOOL_DOC.contains("all-day entries are left out"));
        // A suggestion, on this tool as much as on log_time.
        let calendar = PROJECTS_TOOL_DOC
            .split("- draft_timesheet_from_calendar:")
            .nth(1)
            .unwrap_or_default();
        assert!(!calendar.is_empty(), "the calendar tool is described");
        assert!(calendar.contains("SAVED AS A SUGGESTION"));
        // The range is bounded, and the bound is a number the model can obey.
        assert!(calendar.contains("at most 31 days"));
    }

    #[test]
    fn nothing_projects_offers_invoices_approves_or_deletes() {
        for forbidden in [
            "delete_time",
            "approve_week",
            "submit_week",
            "invoice_hours",
        ] {
            assert!(!PROJECTS_TOOL_DOC.contains(forbidden));
            assert!(crate::agent_tool::find_tool(PROJECTS_TOOLS, forbidden).is_none());
        }
    }
}
