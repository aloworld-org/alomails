//! The **Insights** tool set of the agent (ADR 0034, queue item A2.4) — the
//! names alo Insights contributes to its own agent, and the words that tell a
//! model what they take.
//!
//! The same seam every product before it uses ([`crate::agent_docs`]): a tool
//! list carrying each tool's effect, a description block, and a paragraph of
//! guidance. Nothing here reads a figure or writes a board — the reading tools
//! are executed inside the turn and the write only from an approval, both by
//! `alo-jmap`'s `agent_insights` over the same query engine
//! (`alo_store::insight_query`) that `POST /insights/eval` uses, against the
//! caller's own tenant-scoped store.
//!
//! Four rules shape the wording below, and each is a way a figure could
//! otherwise quietly lie:
//!
//! - **The model fills in a form; it never writes a query.** A question about
//!   the numbers is a [`ChartSpec`](../../alo_store/insight_spec/struct.ChartSpec.html)
//!   over a closed catalog, and every word in one is an enum variant the server
//!   validates. That is why `insight_catalog` comes first: the vocabulary is
//!   looked up, never remembered, so an agent cannot name a measure this build
//!   does not have. The same design ADR 0037 settled for ask-to-chart, reached
//!   from a room.
//! - **A figure is repeated, never recomputed.** Every value that comes back is
//!   a whole number in a declared unit — cents in a named currency, a count, a
//!   rate in basis points — and the guidance says to report it as it arrived. A
//!   percentage a model worked out in its head is a figure nobody can check
//!   against the books, and money the store deliberately kept in separate
//!   currencies must not be added up in a sentence.
//! - **A change is what moved, not why.** `insight_change` answers with the
//!   figure before, the figure now and the difference, biggest movement first.
//!   The reason is not in the numbers, so the guidance forbids offering one the
//!   sources do not carry — an invented cause is the failure this wave names,
//!   not a partial success.
//! - **A report waits for a tap.** `insight_report` builds a board somebody's
//!   colleagues will read, so it is declared a write (ADR 0047 §1) and the only
//!   path that runs it is an approval the asker themselves gave.

use crate::agent_tool::AgentTool;

/// The Insights tools, each declaring whether it reads or writes (ADR 0047 §1).
///
/// The jmap layer validates an approved tool against the union of this list and
/// every other product's ([`crate::is_agent_tool`]) and owns the execution of
/// each.
pub const INSIGHTS_TOOLS: &[AgentTool] = &[
    AgentTool::read("insight_catalog"),
    AgentTool::read("insight_answer"),
    AgentTool::read("insight_change"),
    AgentTool::write("insight_report"),
];

/// The description of each Insights tool, spliced into the agent's system
/// prompt.
///
/// The catalog itself is deliberately **not** here: it is thousands of
/// characters, it belongs to the store, and a copy in a prompt would be the one
/// thing that could name a measure the validator does not accept. The model
/// looks it up instead, which is what the first tool is for.
///
/// Every line ends with a newline so the block concatenates into the list above
/// it without the caller knowing how many tools Insights has.
pub const INSIGHTS_TOOL_DOC: &str = "\
- insight_catalog: read what this workspace can measure — its datasets, the measures each one offers, the breakdowns and filters each measure allows, and the shape a chart specification takes. It changes nothing and takes no arguments. args: {}. Call it FIRST, before you name a dataset, a measure, a breakdown or a filter anywhere else: those names are a closed list, and one that is not on it is refused rather than guessed at.\n\
- insight_answer: answer from the numbers — it evaluates ONE chart specification against this company's own records and returns the buckets and their values. It reads; it changes nothing. args: {\"spec\": object (a chart specification in the shape insight_catalog describes, REQUIRED)}. Prefer the smallest specification that answers the question: no breakdown at all for a single figure, and no filters, sort or limit unless the question asks for them.\n\
- insight_change: explain a change — it evaluates the same specification over two periods and returns what moved, biggest movement first, each with the figure before, the figure now and the difference. It reads; it changes nothing. args: {\"spec\": object (the chart specification, carrying the LATER period, REQUIRED), \"against\": object (the earlier period on its own, in the same shape as a specification's period, REQUIRED)}. The specification must break down by a category or not at all — a date breakdown has different buckets in each period and nothing to compare, and is refused.\n\
- insight_report: propose a report — a named board of saved charts the user can open in Insights. It only proposes: no board and no chart is saved until the user approves it. args: {\"name\": string (what the report is called, in the user's own language, REQUIRED), \"charts\": [{\"title\": string (the caption on that chart, REQUIRED), \"spec\": object (a chart specification, REQUIRED)}] (REQUIRED, 1 to 8)}. Every chart is validated and answered before anything is saved, so one that cannot be answered refuses the whole report by name rather than pinning a broken tile. Say what the report will contain before you propose it.\n";

/// The Insights paragraph of the agent's general instructions.
///
/// Kept apart from the per-tool lines above because it says what the *product*
/// is, not what a tool takes: it is the sentence that stops a model reporting a
/// figure it worked out itself, and the one that stops a change coming with an
/// invented cause.
pub const INSIGHTS_GUIDANCE: &str = "You answer from this company's own figures and from nothing else. ALWAYS look the vocabulary up with insight_catalog before naming a dataset, a measure, a breakdown or a filter — they are a closed list, and a name that is not on it is refused. Say which measure, which dataset and which period every figure came from, because a number without its question is not an answer. Figures arrive as whole numbers — cents in a named currency, a count, or a rate in basis points — and you repeat them exactly as they arrived: never add them up, scale them, convert a currency or work out a percentage yourself, because a figure nobody can check against the books is worse than no figure. When an answer comes back in more than one currency, keep them apart; they were kept apart for a reason. For how much was billed, measure net on billing.documents; for money that actually arrived, amount on billing.payments; for money still owed, outstanding on billing.receivables. Say what moved and by how much, and never offer a REASON the figures do not carry — the numbers say what changed, not why. When a question is about something the catalog does not hold, say so plainly rather than answering from anything else, and never say a report exists until the user has approved it.\n";

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn every_insights_tool_is_described_to_the_model() {
        for tool in INSIGHTS_TOOLS {
            assert!(
                INSIGHTS_TOOL_DOC.contains(&format!("- {}:", tool.name)),
                "{} has no description in the prompt",
                tool.name
            );
        }
        // …and nothing is described that cannot be executed.
        let described = INSIGHTS_TOOL_DOC.matches("\n- ").count() + 1;
        assert_eq!(described, INSIGHTS_TOOLS.len());
    }

    #[test]
    fn the_block_concatenates_cleanly_into_a_list() {
        assert!(INSIGHTS_TOOL_DOC.ends_with('\n'));
        assert!(INSIGHTS_TOOL_DOC.starts_with("- "));
        assert!(INSIGHTS_GUIDANCE.ends_with('\n'));
    }

    fn line(name: &str) -> String {
        INSIGHTS_TOOL_DOC
            .lines()
            .find(|line| line.starts_with(&format!("- {name}:")))
            .expect("the tool is described")
            .to_owned()
    }

    /// The three reads answer inside the turn; only the board waits. Declared,
    /// not derived from the names — and the reads say plainly that they change
    /// nothing, because a question about the numbers answered with a button is
    /// the bug ADR 0047 was written about.
    #[test]
    fn the_reads_answer_and_only_the_report_waits() {
        for reads in ["insight_catalog", "insight_answer", "insight_change"] {
            assert!(crate::is_read_tool(reads), "{reads}");
            assert!(line(reads).contains("changes nothing"), "{reads}");
        }
        assert!(!crate::is_read_tool("insight_report"));
        assert!(line("insight_report").contains("It only proposes"));
        assert!(INSIGHTS_GUIDANCE.contains("never say a report exists"));
    }

    /// The rule the whole tool set rests on: the vocabulary is looked up, never
    /// remembered. A model naming a measure from memory is how a question gets
    /// answered with a chart of something else.
    #[test]
    fn the_vocabulary_is_looked_up_rather_than_remembered() {
        assert!(INSIGHTS_GUIDANCE.contains("ALWAYS look the vocabulary up with insight_catalog"));
        assert!(line("insight_catalog").contains("Call it FIRST"));
        assert!(line("insight_catalog").contains("closed list"));
        // The catalog is never copied into the prompt: a second copy of the
        // vocabulary is the only thing that could name a measure the validator
        // refuses. The named datasets below are guidance about which measure
        // answers which question, not a menu — the menu comes from the tool.
        assert!(!INSIGHTS_TOOL_DOC.contains("billing.documents"));
    }

    /// A figure is repeated, not recomputed, and currencies the store kept
    /// apart stay apart in the sentence too.
    #[test]
    fn no_figure_is_worked_out_in_the_answer() {
        assert!(INSIGHTS_GUIDANCE.contains("repeat them exactly as they arrived"));
        assert!(INSIGHTS_GUIDANCE.contains("never add them up"));
        assert!(INSIGHTS_GUIDANCE.contains("work out a percentage yourself"));
        assert!(INSIGHTS_GUIDANCE.contains("keep them apart"));
    }

    /// What `insight_change` may and may not say, where the model reads it.
    #[test]
    fn a_change_is_what_moved_and_never_why() {
        assert!(line("insight_change").contains("biggest movement first"));
        assert!(line("insight_change").contains("nothing to compare"));
        assert!(INSIGHTS_GUIDANCE.contains("never offer a REASON the figures do not carry"));
    }

    /// Nothing in this set changes a figure, a document or a book: the one
    /// write pins questions somebody already asked, and every answer is read
    /// out of records another product owns.
    #[test]
    fn nothing_here_edits_the_records_the_figures_come_from() {
        for tool in INSIGHTS_TOOLS {
            assert!(
                !tool.name.contains("delete") && !tool.name.contains("edit"),
                "{} would let a report change what it reports on",
                tool.name
            );
        }
        assert!(line("insight_report").contains("named board of saved charts"));
    }
}
