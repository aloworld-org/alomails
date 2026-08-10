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
//! B4.14b adds the two **answer** tools (`vat_summary`, `flag_anomalies`) to
//! the list below; this slice is the drafting one.

/// The Finance tools the agent may propose, by name.
///
/// The jmap layer validates an approved tool against the union of this list and
/// every other product's ([`crate::is_agent_tool`]) and owns the execution of
/// each.
pub const FINANCE_TOOLS: &[&str] = &["categorise_transactions"];

/// The description of each Finance tool, spliced into the agent's system prompt
/// after the Projects tools ([`crate::agent`]).
///
/// Every line ends with a newline so the block concatenates into the list above
/// it without the caller knowing how many tools Finance has.
pub const FINANCE_TOOL_DOC: &str = "\
- categorise_transactions: go through the user's OWN expense claims that have no category yet and SUGGEST one for each, from the categories they have already used for the same merchant. Every suggestion is SAVED AS A SUGGESTION on its claim for the user to accept or decline — nothing is classified, booked or reported until they accept it, one claim at a time. args: {\"from\": string in \"YYYY-MM-DD\" (the first day of the period of claims to look at, optional — the last three months when left out), \"to\": string in \"YYYY-MM-DD\" (the last day, included, optional — today when left out)}. You do NOT choose the categories and there is no argument for one: the suggestion comes from what this person has agreed to before, and a claim from a merchant they have never classified gets no suggestion rather than a guess. Propose this when the user asks to sort out, categorise or tidy up their expenses.\n";

/// The Finance paragraph of the agent's general instructions.
///
/// Kept apart from the per-tool line above because it says what the *product*
/// is, not what a tool takes: it is the sentence that stops a model inventing a
/// chart of accounts, a category or a figure on its way to somebody's books.
pub const FINANCE_GUIDANCE: &str = "For a finance tool, NEVER invent a category, an account or an amount: the categories are the tenant's own words in their own language, the figures are in their books, and anything you compose yourself would be a number in somebody's accounts that nobody can trace. Resolve a relative period (this month, last quarter) against today's date below into plain YYYY-MM-DD days. A suggestion a finance tool writes is not a decision: never tell the user something has been classified, booked or filed — say it is waiting for them.\n";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_finance_tool_is_described_to_the_model() {
        for tool in FINANCE_TOOLS {
            assert!(
                FINANCE_TOOL_DOC.contains(&format!("- {tool}:")),
                "{tool} has no description in the prompt"
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
    fn nothing_finance_offers_posts_pays_or_deletes() {
        for forbidden in [
            "post_entry",
            "approve_expense",
            "pay_invoice",
            "delete_expense",
            "close_period",
        ] {
            assert!(!FINANCE_TOOL_DOC.contains(forbidden));
            assert!(!FINANCE_TOOLS.contains(&forbidden));
        }
    }
}
