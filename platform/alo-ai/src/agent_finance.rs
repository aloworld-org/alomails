//! The **Finance** tool set of the agent (ADR 0034, ADR 0035 wave B4.14) — the
//! names alo Finance contributes to the one agent, and the words that tell a
//! model what they take.
//!
//! The fourth product on the seam [`crate::agent_billing`] opened: a tool list
//! and a paragraph, in the product's own module. Nothing here reads, writes or
//! decides anything — the proposal is parsed by [`crate::agent`], and an
//! approved proposal is executed by the jmap layer against the caller's
//! tenant-scoped store.
//!
//! Two rules shape the wording below, and each is a mistake it exists to
//! prevent:
//!
//! - **The model does not classify anything.** `categorise_transactions` asks
//!   the *store* to suggest categories, from the words this person has already
//!   agreed to for the same merchant. A model that believes it is choosing the
//!   category will start passing one in, and a cost booked to a word somebody
//!   invented is a wrong P&L nobody can see. The description therefore names no
//!   category argument at all — there is none to pass.
//! - **A suggestion is not a classification.** What the tool writes lands in a
//!   column no report reads, waiting for the claimant to accept each one
//!   (`docs/design/finance.md` § The finance agent). The description says so in
//!   the model's own words, so it does not tell the user their expenses have
//!   been sorted out.
//!
//! B4.14b added the two **answer** tools, and each carries a rule of its own:
//!
//! - **`vat_summary` states its period or it does not run.** Both days are
//!   required, exactly as `GET /finance/reports/vat` requires them: a VAT figure
//!   printed under a heading nobody asked for is the one number that gets copied
//!   into a year-end and argued about a year later.
//! - **`flag_anomalies` names entries, never people.** An anomaly is a fact
//!   about a document; an agent that summarised whose spending looks odd would be
//!   a profiling feature nobody asked for. The description says the tool cannot
//!   answer a question about a person, so a model does not try to make it one.

use crate::agent_tool::AgentTool;

/// The Finance tools, each declaring whether it reads or writes (ADR 0047 §1).
///
/// The jmap layer validates an approved tool against the union of this list and
/// every other product's ([`crate::is_agent_tool`]) and owns the execution of
/// each.
pub const FINANCE_TOOLS: &[AgentTool] = &[
    AgentTool::write("categorise_transactions"),
    AgentTool::read("vat_summary"),
    AgentTool::read("flag_anomalies"),
];

/// The description of each Finance tool, spliced into the agent's system prompt
/// after the Projects tools ([`crate::agent`]).
///
/// Every line ends with a newline so the block concatenates into the list above
/// it without the caller knowing how many tools Finance has.
pub const FINANCE_TOOL_DOC: &str = "\
- categorise_transactions: go through the user's OWN expense claims that have no category yet and SUGGEST one for each, from the categories they have already used for the same merchant. Every suggestion is SAVED AS A SUGGESTION on its claim for the user to accept or decline — nothing is classified, booked or reported until they accept it, one claim at a time. args: {\"from\": string in \"YYYY-MM-DD\" (the first day of the period of claims to look at, optional — the last three months when left out), \"to\": string in \"YYYY-MM-DD\" (the last day, included, optional — today when left out)}. You do NOT choose the categories and there is no argument for one: the suggestion comes from what this person has agreed to before, and a claim from a merchant they have never classified gets no suggestion rather than a guess. Propose this when the user asks to sort out, categorise or tidy up their expenses.\n\
- vat_summary: read the VAT figures the user's own books carry for a period — tax charged on sales per rate, tax paid on purchases per rate, and the net payable between them. It files nothing with any tax authority. args: {\"from\": string in \"YYYY-MM-DD\" (the first day of the period, REQUIRED), \"to\": string in \"YYYY-MM-DD\" (the last day, included, REQUIRED)}. Both days are required and neither has a default: work out the days the user meant from today's date below and state them. Use this instead of answering from the sources when the user asks what VAT they owe or for a quarter's figures — the figures are in their books, not in the search results.\n\
- flag_anomalies: read the user's own journal over a period and name what is worth a second look in it — the same amount booked twice to the same counterparty within a week, an amount far outside what its account usually moves, a monthly cost that skipped a month. It marks nothing as reviewed and accuses nobody. args: {\"from\": string in \"YYYY-MM-DD\" (the first day, optional — the last twelve months when left out), \"to\": string in \"YYYY-MM-DD\" (the last day, included, optional — today when left out)}. Every finding names the ENTRIES behind it and never a person: this tool cannot say who spent what, so never propose it for a question about somebody's spending. Use it when the user asks to check, review or look over their books.\n";

/// The Finance paragraph of the agent's general instructions.
///
/// Kept apart from the per-tool line above because it says what the *product*
/// is, not what a tool takes: it is the sentence that stops a model inventing a
/// chart of accounts, a category or a figure on its way to somebody's books.
pub const FINANCE_GUIDANCE: &str = "For a finance tool, NEVER invent a category, an account or an amount: the categories are the tenant's own words in their own language, the figures are in their books, and anything you compose yourself would be a number in somebody's accounts that nobody can trace. Resolve a relative period (this month, last quarter) against today's date below into plain YYYY-MM-DD days. A suggestion a finance tool writes is not a decision: never tell the user something has been classified, booked or filed — say it is waiting for them. A finance tool that ANSWERS returns figures and the entries behind them: those figures are the answer, so repeat them rather than recomputing anything, add none the tool did not return, and report what it found as something worth looking at rather than as a verdict about anybody.\n";

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn every_finance_tool_is_described_to_the_model() {
        for tool in FINANCE_TOOLS {
            assert!(
                FINANCE_TOOL_DOC.contains(&format!("- {}:", tool.name)),
                "{} has no description in the prompt",
                tool.name
            );
        }
        // …and nothing is described that cannot be executed.
        let described = FINANCE_TOOL_DOC.matches("\n- ").count() + 1;
        assert_eq!(described, FINANCE_TOOLS.len());
    }

    #[test]
    fn the_block_concatenates_cleanly_into_a_list() {
        assert!(FINANCE_TOOL_DOC.ends_with('\n'));
        assert!(FINANCE_TOOL_DOC.starts_with("- "));
        assert!(FINANCE_GUIDANCE.ends_with('\n'));
    }

    #[test]
    fn the_model_is_never_offered_a_category_to_choose() {
        // The one mistake this tool set can make that nothing downstream
        // catches: a category the model made up, arriving as an argument.
        assert!(!FINANCE_TOOL_DOC.contains("\"category\""));
        assert!(FINANCE_TOOL_DOC.contains("there is no argument for one"));
        assert!(FINANCE_GUIDANCE.contains("NEVER invent a category"));
    }

    #[test]
    fn the_wording_says_a_suggestion_is_not_a_classification() {
        assert!(FINANCE_TOOL_DOC.contains("SAVED AS A SUGGESTION"));
        assert!(FINANCE_TOOL_DOC.contains("nothing is classified"));
        assert!(FINANCE_GUIDANCE.contains("is not a decision"));
    }

    #[test]
    fn each_answer_tool_says_it_changes_nothing() {
        // The one thing a reading tool must not be mistaken for is a writing
        // one: a model that believes `vat_summary` files a return will tell
        // somebody their VAT is filed.
        //
        // That both of them only read is now declared in the registry and
        // rendered into the prompt from there (ADR 0047 §1), so it is asserted
        // where it is *decided* rather than in a sentence per tool that the
        // code could not check.
        for tool in ["vat_summary", "flag_anomalies"] {
            assert!(crate::is_read_tool(tool), "{tool} is a read");
        }
        // What stays here is what is true of *these* tools and of no others:
        // the specific act each one must not be mistaken for.
        assert!(FINANCE_TOOL_DOC.contains("files nothing with any tax authority"));
        assert!(FINANCE_TOOL_DOC.contains("marks nothing as reviewed and accuses nobody"));
        assert!(FINANCE_GUIDANCE.contains("repeat them rather than recomputing"));
    }

    #[test]
    fn the_vat_period_has_no_default_to_fall_back_on() {
        let line = FINANCE_TOOL_DOC
            .lines()
            .find(|line| line.starts_with("- vat_summary:"))
            .expect("vat_summary is described");
        // Twice REQUIRED, once more in prose: a figure under a period nobody
        // asked for is the one number that must never be guessed.
        assert_eq!(line.matches("REQUIRED").count(), 2);
        assert!(line.contains("Both days are required"));
        assert!(!line.contains("when left out"), "no default may be offered");
    }

    #[test]
    fn the_anomaly_tool_is_told_it_cannot_answer_about_a_person() {
        let line = FINANCE_TOOL_DOC
            .lines()
            .find(|line| line.starts_with("- flag_anomalies:"))
            .expect("flag_anomalies is described");
        assert!(line.contains("names the ENTRIES behind it and never a person"));
        assert!(line.contains("never propose it for a question about somebody's spending"));
        // And no word that turns a question into a verdict.
        for forbidden in ["fraud", "risk", "score", "suspicious"] {
            assert!(!FINANCE_TOOL_DOC.contains(forbidden), "{forbidden}");
        }
    }

    #[test]
    fn nothing_finance_offers_posts_pays_or_deletes() {
        for forbidden in [
            "post_entry",
            "approve_expense",
            "pay_invoice",
            "delete_expense",
            "close_period",
        ] {
            assert!(!FINANCE_TOOL_DOC.contains(forbidden));
            assert!(crate::agent_tool::find_tool(FINANCE_TOOLS, forbidden).is_none());
        }
    }
}
