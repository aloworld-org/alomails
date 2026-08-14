//! The **CRM** tool set of the agent (ADR 0034, ADR 0035 wave B2.10) — the
//! names alo CRM contributes to the one agent, and the words that tell a model
//! what they take.
//!
//! The second product to use the seam [`crate::agent_billing`] opened: a
//! product agent is a tool set plus a paragraph, not a second system. This
//! module is deliberately *only* text and names — nothing here reads, writes or
//! decides anything. The proposal is parsed by [`crate::agent`], and an
//! approved proposal is executed by the jmap layer against the caller's
//! tenant-scoped store.
//!
//! Three rules shape the wording below, and each of them is a mistake it exists
//! to prevent:
//!
//! - **A deal is named, never numbered.** Unlike an invoice, an opportunity has
//!   no number a person can quote, so a tool resolves the *title* the user said
//!   against the tenant's own deals. The model is told to pass it through
//!   verbatim: a title it "tidied" resolves to nothing, or worse, to the wrong
//!   card.
//! - **Money is integer cents.** A deal's value is asked for in whole cents,
//!   like every other amount in the suite, so nothing a model writes has to be
//!   rounded on the way in.
//! - **Losing a deal takes a reason and winning one takes none.** The store
//!   demands the first and refuses the second; saying so here turns a `422` a
//!   user would have to read into an argument the model gets right.
//!
//! Nothing here closes a sale on its own account: `move_deal_stage` is the one
//! tool that can win or lose a deal, and it does so only after the user reads
//! the proposal and approves it.

use crate::agent_tool::AgentTool;

/// The CRM tools, each declaring whether it reads or writes (ADR 0047 §1).
///
/// The jmap layer validates an approved tool against the union of this list and
/// every other product's ([`crate::is_agent_tool`]) and owns the execution of
/// each.
pub const CRM_TOOLS: &[AgentTool] = &[
    AgentTool::write("create_deal"),
    AgentTool::write("move_deal_stage"),
    AgentTool::write("draft_followup"),
];

/// The description of each CRM tool, spliced into the agent's system prompt
/// after the billing tools ([`crate::agent`]).
///
/// Every line ends with a newline so the block concatenates into the list above
/// it without the caller knowing how many tools CRM has.
pub const CRM_TOOL_DOC: &str = "\
- create_deal: raise an opportunity on the sales board. args: {\"title\": string (what the opportunity is, required), \"company\": string (the company it is with, optional), \"contactName\": string (the person being spoken to, optional), \"contactEmail\": string (their address, optional), \"valueCents\": integer (what it is worth, in whole cents, e.g. 500000 for 5000.00, optional), \"currency\": string (ISO 4217, e.g. \"EUR\", optional), \"expectedClose\": string in \"YYYY-MM-DD\" (optional), \"origin\": string (where the lead came from, e.g. \"Referral\", optional), \"pipeline\": string (the board's name — only needed when the tenant has more than one), \"stage\": string (the column's name; the first column of the board by default), \"source\": number (the numbered source of an EMAIL this deal comes from — the conversation is then linked to the new deal, optional)}. Never invent a value or a close date the user did not give you.\n\
- move_deal_stage: move a deal to another column of its own board — this is also how a deal is won or lost. args: {\"deal\": string (the deal's title, exactly as the user says it, required), \"stage\": string (the column to move it to, required), \"reason\": string (why it was lost — REQUIRED when the column means lost, and refused for any other column)}.\n\
- draft_followup: write a follow-up email to a deal's contact and save it to the user's Drafts for them to review and send — it is NEVER sent automatically. args: {\"deal\": string (the deal's title, required), \"body\": string (the whole letter, required), \"subject\": string (optional; the deal's title by default)}. Compose the body from the request; do not invent facts about the deal, and never state a recipient — the letter goes to the deal's own contact.\n";

/// The CRM paragraph of the agent's general instructions.
///
/// Kept apart from the per-tool lines above because it says what the *product*
/// is, not what a tool takes: it is the sentence that stops a model renaming a
/// deal on its way to the store, and the one that tells it what to do when the
/// user's words name no deal at all.
pub const CRM_GUIDANCE: &str = "For a CRM tool, pass a deal title, a board name or a column name through EXACTLY as the user gave it — never invent, complete or reformat one, and never guess which deal was meant. If the user did not name one, ANSWER and ask which. A deal has no number: it is found by its title, so it is not in the numbered sources — but an EMAIL a deal comes from is, and create_deal takes that source number to link the conversation.\n";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_crm_tool_is_described_to_the_model() {
        for tool in CRM_TOOLS {
            assert!(
                CRM_TOOL_DOC.contains(&format!("- {}:", tool.name)),
                "{} has no description in the prompt",
                tool.name
            );
        }
        // …and nothing is described that cannot be executed.
        let described = CRM_TOOL_DOC.matches("\n- ").count() + 1;
        assert_eq!(described, CRM_TOOLS.len());
    }

    #[test]
    fn the_block_concatenates_cleanly_into_a_list() {
        assert!(CRM_TOOL_DOC.ends_with('\n'));
        assert!(CRM_TOOL_DOC.starts_with("- "));
        assert!(CRM_GUIDANCE.ends_with('\n'));
    }

    #[test]
    fn the_wording_asks_for_money_the_only_way_we_store_it() {
        assert!(CRM_TOOL_DOC.contains("valueCents"));
        assert!(CRM_TOOL_DOC.contains("whole cents"));
        // No decimal amount is ever asked for, in any wording.
        assert!(!CRM_TOOL_DOC.contains("valueEuros"));
        assert!(!CRM_TOOL_DOC.contains("amount in euros"));
    }

    #[test]
    fn the_wording_states_the_two_rules_a_model_would_otherwise_get_wrong() {
        // A lost column demands a reason; every other column refuses one.
        assert!(CRM_TOOL_DOC.contains("REQUIRED when the column means lost"));
        // A follow-up is a draft, and its recipient is never the model's to say.
        assert!(CRM_TOOL_DOC.contains("NEVER sent automatically"));
        assert!(CRM_TOOL_DOC.contains("never state a recipient"));
    }

    #[test]
    fn nothing_crm_offers_deletes_a_record_or_sends_mail() {
        for forbidden in ["delete_deal", "send_followup", "delete_pipeline"] {
            assert!(!CRM_TOOL_DOC.contains(forbidden));
            assert!(crate::agent_tool::find_tool(CRM_TOOLS, forbidden).is_none());
        }
    }
}
