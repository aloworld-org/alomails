//! The **Inventory** tool set of the agent (ADR 0034, ADR 0035 wave B5.10) — the
//! names alo Inventory contributes to the one agent, and the words that tell a
//! model what they take.
//!
//! The fifth product on the seam [`crate::agent_billing`] opened: a tool list
//! and a paragraph, in the product's own module. Nothing here reads, writes or
//! decides anything — the proposal is parsed by [`crate::agent`], and an
//! approved proposal is executed by the jmap layer against the caller's
//! tenant-scoped store.
//!
//! Three rules shape the wording below, and each is a mistake it exists to
//! prevent:
//!
//! - **The model states no quantity, no price and no supplier terms.**
//!   `reorder_proposals` takes at most a *narrowing* — one supplier, one place —
//!   and never a number. What to buy is the shortage query's arithmetic over the
//!   tenant's own minimums, their own shelves and their own price list
//!   (`docs/design/inventory.md` § Reorder rules and the shortage query). A model
//!   that believed it was choosing quantities would eventually order four hundred
//!   of something at a price nobody quoted, and a purchase order is a document
//!   that goes to another company.
//! - **A proposal is a draft order, not an order.** What the tool writes carries
//!   no number and has been sent to nobody; somebody presses send on the
//!   purchase-orders screen, which is where the number is drawn (B5.05a2). The
//!   description says so in the model's own words, so it does not tell the user
//!   their supplier has been contacted.
//! - **Nothing here predicts anything.** `stock_answer` reports what is true
//!   today — on the shelf, on order, promised out — and the description gives a
//!   model no room to add a forecast to it. "You will need forty next month" is a
//!   claim about the future that a small business would act on, and the honest
//!   version of it needs seasonality and lead-time variance we deliberately do
//!   not build (`docs/design/inventory.md` § The inventory agent).

/// The Inventory tools the agent may propose, by name.
///
/// The jmap layer validates an approved tool against the union of this list and
/// every other product's ([`crate::is_agent_tool`]) and owns the execution of
/// each.
pub const INVENTORY_TOOLS: &[&str] = &["reorder_proposals", "stock_answer"];

/// The description of each Inventory tool, spliced into the agent's system
/// prompt after the Finance tools ([`crate::agent`]).
///
/// Every line ends with a newline so the block concatenates into the list above
/// it without the caller knowing how many tools Inventory has.
pub const INVENTORY_TOOL_DOC: &str = "\
- reorder_proposals: go through everything the user is under their OWN minimum on and write one DRAFT purchase order per supplier for it. Each draft is SAVED AS A DRAFT in their purchase orders, carries no order number, and has been sent to NOBODY — they open it, change it and press send themselves. args: {\"supplier\": string (only what this one supplier sells us, optional — every supplier when left out), \"location\": string (only what is short at this one place, by its name or its code, optional — everywhere when left out)}. You do NOT choose what to buy or what it costs and there is no argument for either: the quantities come from the user's own minimums and shelves, and the prices from their own agreed price list. A shortage nobody has quoted us for is reported back as left out rather than ordered from a supplier you picked. Propose this when the user asks what needs reordering, what is running low, or to raise the orders for it.\n\
- stock_answer: read where ONE product of the user's stands right now — how much is on each of their shelves, how much is on order from suppliers, how much is promised to customers, what that leaves available, and whether it is under a minimum they set. It only READS: it writes nothing, orders nothing and reserves nothing. args: {\"product\": string (the product, by the name, SKU or barcode the user used, REQUIRED)}. Answer with the figures it returns and NOTHING you worked out yourself: never estimate when stock will run out, never predict what will be needed, and never suggest a quantity to buy — the tool reports today, and reorder_proposals is what turns a shortage into a document. Propose this when the user asks how many of something is left, whether there is enough, or what is on its way.\n";

/// The Inventory paragraph of the agent's general instructions.
///
/// Kept apart from the per-tool line above because it says what the *product*
/// is, not what a tool takes: it is the sentence that stops a model inventing a
/// quantity, a price or a delivery date on its way to another company's inbox.
pub const INVENTORY_GUIDANCE: &str = "For an inventory tool, NEVER invent a quantity, a price, a supplier or a delivery date: the minimums are the tenant's own standing instructions, the quantities follow from what is actually on their shelves, and the prices are what a supplier has already quoted them. Name a product, a supplier or a place with the words the user used and let the tool find it — when more than one matches, the tool says so and asks, which is an answer to repeat rather than a choice to make for them. A draft purchase order an inventory tool writes is not an order: never tell the user anything has been ordered, sent or confirmed — say the draft is waiting for them to send. Stock figures are true as at the moment they were read: repeat them rather than recomputing anything, add none the tool did not return, and never turn them into a forecast of when something will run out.\n";

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn every_inventory_tool_is_described_to_the_model() {
        for tool in INVENTORY_TOOLS {
            assert!(
                INVENTORY_TOOL_DOC.contains(&format!("- {tool}:")),
                "{tool} has no description in the prompt"
            );
        }
        // …and nothing is described that cannot be executed.
        let described = INVENTORY_TOOL_DOC.matches("\n- ").count() + 1;
        assert_eq!(described, INVENTORY_TOOLS.len());
    }

    #[test]
    fn the_block_concatenates_cleanly_into_a_list() {
        assert!(INVENTORY_TOOL_DOC.ends_with('\n'));
        assert!(INVENTORY_TOOL_DOC.starts_with("- "));
        assert!(INVENTORY_GUIDANCE.ends_with('\n'));
    }

    #[test]
    fn the_model_is_never_offered_a_quantity_or_a_price_to_choose() {
        // The one mistake this tool set can make that nothing downstream
        // catches: a quantity or a price the model made up, arriving as an
        // argument on a document that goes to another company.
        for forbidden in [
            "\"qty\"",
            "\"quantity\"",
            "\"qtyMilli\"",
            "\"price\"",
            "\"unitPriceCents\"",
            "\"amount\"",
        ] {
            assert!(!INVENTORY_TOOL_DOC.contains(forbidden), "{forbidden}");
        }
        assert!(INVENTORY_TOOL_DOC.contains("there is no argument for either"));
        assert!(INVENTORY_GUIDANCE.contains("NEVER invent a quantity"));
    }

    #[test]
    fn the_wording_says_a_draft_order_has_been_sent_to_nobody() {
        assert!(INVENTORY_TOOL_DOC.contains("SAVED AS A DRAFT"));
        assert!(INVENTORY_TOOL_DOC.contains("sent to NOBODY"));
        assert!(INVENTORY_TOOL_DOC.contains("carries no order number"));
        assert!(INVENTORY_GUIDANCE.contains("is not an order"));
        assert!(INVENTORY_GUIDANCE.contains("waiting for them to send"));
    }

    #[test]
    fn the_reading_tool_says_it_changes_nothing() {
        let line = INVENTORY_TOOL_DOC
            .lines()
            .find(|line| line.starts_with("- stock_answer:"))
            .expect("stock_answer is described");
        assert!(line.contains("only READS"), "{line}");
        assert!(line.contains("writes nothing"), "{line}");
        // A reservation is the thing this answer is most likely to be mistaken
        // for: the store deliberately has none (`inv_reorder`'s header argues
        // it), so the model must not imply one was made.
        assert!(line.contains("reserves nothing"), "{line}");
    }

    #[test]
    fn nothing_here_forecasts_anything() {
        // The design note's deliberate absence, held in the words the model
        // reads: today's figures are the whole answer.
        assert!(INVENTORY_TOOL_DOC.contains("never predict what will be needed"));
        assert!(INVENTORY_TOOL_DOC.contains("never estimate when stock will run out"));
        assert!(INVENTORY_GUIDANCE.contains("never turn them into a forecast"));
        for forbidden in ["forecast the", "seasonality", "predicted demand"] {
            assert!(!INVENTORY_TOOL_DOC.contains(forbidden), "{forbidden}");
        }
    }

    #[test]
    fn nothing_inventory_offers_sends_receives_ships_or_adjusts() {
        // The writes that move real goods or reach a counterparty, none of
        // which an agent may propose: sending is a person's act, and an
        // adjustment is the most abusable write in the warehouse.
        for forbidden in [
            "send_purchase_order",
            "receive_purchase_order",
            "deliver_sales_order",
            "adjust_stock",
            "apply_stocktake",
        ] {
            assert!(!INVENTORY_TOOL_DOC.contains(forbidden));
            assert!(!INVENTORY_TOOLS.contains(&forbidden));
        }
    }
}
