//! The **billing** tool set of the agent (ADR 0034, ADR 0035 wave B1.25) — the
//! names alo Billing contributes to the one agent, and the words that tell a
//! model what they take.
//!
//! ADR 0034's shape is "one framework, many thin agents": a product agent is a
//! product-scoped tool set plus its description, not a second system. This
//! module is billing's, and it is deliberately *only* text and names — nothing
//! here reads, writes or decides anything. The proposal is parsed by
//! [`crate::agent`], and an approved proposal is executed by the jmap layer
//! against the caller's tenant-scoped store, which is the only place a billing
//! record is ever touched.
//!
//! Two rules shape the wording below, both from the constitution rather than
//! from the model:
//!
//! - **Money is integer cents.** A price is asked for in whole cents and a VAT
//!   rate in basis points, so nothing a model writes has to be rounded on the
//!   way in. A quantity may be fractional (1.5 hours) and is converted exactly,
//!   without floating point, at the execution boundary.
//! - **Nothing here issues, sends or numbers anything.** The three tools raise a
//!   *draft* invoice, accept a quote into a *draft* invoice, and write a mail
//!   *draft*. Assigning a legal number and putting mail on the wire stay
//!   deliberate human acts.

use crate::agent_tool::AgentTool;

/// The billing tools, each declaring whether it reads or writes (ADR 0047 §1).
///
/// The jmap layer validates an approved tool against the union of this list and
/// the core one ([`crate::is_agent_tool`]) and owns the execution of each.
pub const BILLING_TOOLS: &[AgentTool] = &[
    AgentTool::write("create_invoice_draft"),
    AgentTool::write("quote_to_invoice"),
    AgentTool::write("draft_payment_reminder"),
];

/// The description of each billing tool, spliced into the agent's system prompt
/// after the core tools ([`crate::agent`]).
///
/// Every line ends with a newline so the block concatenates into the list above
/// it without the caller knowing how many tools billing has.
pub const BILLING_TOOL_DOC: &str = "\
- create_invoice_draft: raise a DRAFT invoice for a customer. A draft carries no number, is not sent, and is not owed by anyone until the user issues it themselves. args: {\"customer\": string (the customer's name, required), \"lines\": array (required, at least one), \"reference\": string (the customer's own order/PO reference, optional), \"note\": string (a note printed under the lines, optional)}. Each line is EITHER {\"product\": string (a name from the tenant's price list), \"quantity\": number (in units, may be fractional, default 1)} — its unit, price and VAT rate then come from the price list — OR {\"description\": string, \"quantity\": number, \"unitPriceCents\": integer (the price of ONE unit in whole cents, e.g. 12000 for 120.00), \"vatRateBp\": integer (the VAT rate in basis points, e.g. 2100 for 21%), \"unit\": string (optional, e.g. \"hour\")}. Never state both a product and a price on one line, never invent a price or a VAT rate the user did not give you, and never write a total: the server computes every total from the lines.\n\
- quote_to_invoice: accept a quote the customer has agreed to. This closes the quote as accepted and raises a draft invoice carrying a copy of its lines. args: {\"quote\": string (the quote number exactly as the user says it, e.g. \"QUO-2026-00001\", required)}.\n\
- draft_payment_reminder: write a reminder about an unpaid invoice to the customer and save it to the user's Drafts for them to review and send — it is NEVER sent automatically. args: {\"invoice\": string (the invoice number, e.g. \"INV-2026-00042\", required), \"note\": string (one extra sentence to add to the reminder, optional)}.\n";

/// The billing paragraph of the agent's general instructions.
///
/// Kept apart from the per-tool lines above because it says what the *product*
/// is, not what a tool takes: it is the sentence that stops a model inventing a
/// document number, and the one that tells it what to do when the user's words
/// do not name a document at all.
pub const BILLING_GUIDANCE: &str = "For a billing tool, pass a customer, product, invoice or quote name or number through EXACTLY as the user gave it — never invent, complete or reformat a document number, and never guess which customer was meant. If the user did not name one, ANSWER and ask which. Documents are not in the numbered sources: a billing tool is proposed from what the user said, not from a source number.\n";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_billing_tool_is_described_to_the_model() {
        for tool in BILLING_TOOLS {
            assert!(
                BILLING_TOOL_DOC.contains(&format!("- {}:", tool.name)),
                "{} has no description in the prompt",
                tool.name
            );
        }
        // …and nothing is described that cannot be executed.
        let described = BILLING_TOOL_DOC.matches("\n- ").count() + 1;
        assert_eq!(described, BILLING_TOOLS.len());
    }

    #[test]
    fn the_block_concatenates_cleanly_into_a_list() {
        assert!(BILLING_TOOL_DOC.ends_with('\n'));
        assert!(BILLING_TOOL_DOC.starts_with("- "));
        assert!(BILLING_GUIDANCE.ends_with('\n'));
    }

    #[test]
    fn the_wording_asks_for_money_the_only_way_we_store_it() {
        // Integer cents and basis points, never a decimal amount.
        assert!(BILLING_TOOL_DOC.contains("unitPriceCents"));
        assert!(BILLING_TOOL_DOC.contains("whole cents"));
        assert!(BILLING_TOOL_DOC.contains("basis points"));
        assert!(BILLING_TOOL_DOC.contains("never write a total"));
    }

    #[test]
    fn nothing_the_billing_tools_offer_issues_sends_or_numbers() {
        // The three tools are draft-only by name and by description; a model
        // must not be able to read an issue or a send out of this block.
        assert!(BILLING_TOOL_DOC.contains("DRAFT invoice"));
        assert!(BILLING_TOOL_DOC.contains("NEVER sent automatically"));
        for forbidden in ["issue_invoice", "send_invoice", "record_payment"] {
            assert!(!BILLING_TOOL_DOC.contains(forbidden));
            assert!(crate::agent_tool::find_tool(BILLING_TOOLS, forbidden).is_none());
        }
    }
}
