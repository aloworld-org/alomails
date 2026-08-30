//! alo Inventory's verbs (ADR 0058) — the Inventory agent over the one
//! command layer.
//!
//! This is the whole of what the Inventory agent may do, and the words a model
//! reads about it. Nothing here reads or writes a record: the executors live
//! beside Inventory's routes in `alo-jmap` (`inventory_intents.rs`), through
//! the asker's tenant-scoped store, and answer with the same figures the
//! `/inventory/*` routes serve — the shelf's, the shortage report's, the order
//! book's and the price list's, never a second sum.
//!
//! Four rules from the module shape the wording, each a mistake it exists to
//! prevent:
//!
//! - **The model states no quantity, no price and no supplier terms.**
//!   `reorder_proposals` takes at most a *narrowing* — one supplier, one place
//!   — and never a number. What to buy is the shortage query's arithmetic over
//!   the tenant's own minimums, their own shelves and their own price list
//!   (`docs/design/inventory.md` § Reorder rules and the shortage query). A
//!   model that believed it was choosing quantities would eventually order
//!   four hundred of something at a price nobody quoted, and a purchase order
//!   is a document that goes to another company.
//! - **A proposal is a draft order, not an order.** What `reorder_proposals`
//!   writes carries no number and has been sent to nobody; somebody presses
//!   send on the purchase-orders screen, which is where the number is drawn
//!   (B5.05a2). The wording says so in the model's own words, so it does not
//!   tell the user their supplier has been contacted.
//! - **A booked delivery is goods on a shelf and a DRAFT bill.**
//!   `receive_delivery` books what a supplier actually sent against the order
//!   we actually placed — the store moves the goods and raises the bill in one
//!   transaction, and that bill is decided on by a person. The wording keeps
//!   the model from telling the user anything has been approved or paid.
//! - **Nothing here predicts anything.** The reads report what is true today —
//!   on the shelf, on order, promised out, under a minimum — and the wording
//!   gives a model no room to add a forecast. "You will need forty next month"
//!   is a claim about the future that a small business would act on, and the
//!   honest version of it needs seasonality and lead-time variance we
//!   deliberately do not build (`docs/design/inventory.md` § The inventory
//!   agent).

use crate::agent_tool::Effect;
use crate::intent::{Arg, Excluded, IntentModule, IntentSpec};

const SUPPLIER_OPT: Arg = Arg::optional(
    "supplier",
    "text",
    "one supplier, by the name the user used; leave out for every supplier",
);

const LOCATION_OPT: Arg = Arg::optional(
    "location",
    "text",
    "one place, by its name or its code; leave out for everywhere",
);

/// The verbs.
pub const INVENTORY_INTENTS: &[IntentSpec] = &[
    // ---- reads ---------------------------------------------------------
    IntentSpec {
        name: "stock_answer",
        purpose: "Where ONE product of the user's stands right now — how much is on each of their shelves, how much is on order from suppliers, how much is promised to customers, what that leaves available, and whether it is under a minimum they set. It orders nothing and reserves nothing: answer with the figures it returns and NOTHING you worked out yourself — never estimate when stock will run out, never predict what will be needed, and never suggest a quantity to buy.",
        effect: Effect::Read,
        args: &[Arg::required(
            "product",
            "text",
            "the product, by the name, SKU or barcode the user used",
        )],
        answers: &[
            "how many {product} do we have left",
            "is {product} in stock",
            "what is on its way for this product",
        ],
        preview: None,
        undo: None,
        routes: &["/inventory/stock"],
    },
    IntentSpec {
        name: "stock_below_minimum",
        purpose: "Everything under the user's OWN minimums — the shortage report exactly as their reorder screen reads it: each product with the shelf it is watched on, the minimum and target they set, what is on hand, on order and promised, how much the arithmetic says to buy, and the supplier who quotes for it with their price. A shortage nobody has quoted us for is reported without a supplier rather than assigned one.",
        effect: Effect::Read,
        args: &[SUPPLIER_OPT, LOCATION_OPT],
        answers: &[
            "what is running low",
            "which products are below minimum",
            "what needs reordering",
        ],
        preview: None,
        undo: None,
        routes: &["/inventory/shortages"],
    },
    IntentSpec {
        name: "open_purchase_orders",
        purpose: "The orders that are not finished with: drafts nobody has sent, sent ones we are waiting on, and part-received ones — each with its supplier, status, expected date, whether it is late, and its totals. Name a status to see just those, finished ones included; \"late\" is computed against today, never guessed.",
        effect: Effect::Read,
        args: &[
            Arg::optional(
                "status",
                "text",
                "one status exactly — \"draft\", \"sent\", \"partially_received\", \"received\" or \"cancelled\"; leave out for everything unfinished",
            ),
            SUPPLIER_OPT,
        ],
        answers: &[
            "what is on order",
            "which purchase orders are open",
            "what are we waiting on from this supplier",
        ],
        preview: None,
        undo: None,
        routes: &["/inventory/purchase-orders"],
    },
    IntentSpec {
        name: "supplier_prices",
        purpose: "What ONE supplier sells us, as the user's own records have it: each product with the price they quoted, its currency, any minimum order quantity, and the lead time that actually applies. It reads the tenant's own price list and nothing else — never a website, never a guess at what somebody would charge.",
        effect: Effect::Read,
        args: &[Arg::required(
            "supplier",
            "text",
            "the supplier, by the name the user used",
        )],
        answers: &[
            "what do we buy from this supplier",
            "what does this supplier charge us",
            "what are their agreed prices",
        ],
        preview: None,
        undo: None,
        routes: &["/inventory/suppliers/{id}/products"],
    },
    IntentSpec {
        name: "recent_moves",
        purpose: "The stock ledger's latest movements, newest first — what moved, from where to where, how much, when and why, receipts and deliveries included. It reads history and records nothing: writing a movement or an adjustment into the ledger is a person's own act in the app.",
        effect: Effect::Read,
        args: &[
            Arg::optional(
                "product",
                "text",
                "one product's history, by the name, SKU or barcode the user used; leave out for everything",
            ),
            LOCATION_OPT,
        ],
        answers: &[
            "what moved recently",
            "what happened in the warehouse",
            "what came in and went out for this product",
        ],
        preview: None,
        undo: None,
        routes: &["/inventory/moves"],
    },
    // ---- writes: propose, then the asker approves ------------------------
    IntentSpec {
        name: "reorder_proposals",
        purpose: "Go through everything the user is under their OWN minimum on and write one DRAFT purchase order per supplier for it. Each draft is SAVED AS A DRAFT in their purchase orders, carries no order number, and has been sent to NOBODY — they open it, change it and press send themselves. You do NOT choose what to buy or what it costs and there is no argument for either: the quantities come from the user's own minimums and shelves, and the prices from their own agreed price list. A shortage nobody has quoted us for is reported back as left out rather than ordered from a supplier you picked.",
        effect: Effect::Write,
        args: &[
            Arg::optional(
                "supplier",
                "text",
                "only what this one supplier sells us; leave out for every supplier",
            ),
            Arg::optional(
                "location",
                "text",
                "only what is short at this one place, by its name or its code; leave out for everywhere",
            ),
        ],
        answers: &[
            "raise the orders for what is running low",
            "draft the reorders",
            "order what we are short of",
        ],
        preview: Some(
            "One DRAFT purchase order per supplier will be written for everything under its minimum — each carries no number and is sent to nobody until you open it and press send.",
        ),
        undo: None,
        routes: &["/inventory/purchase-orders"],
    },
    IntentSpec {
        name: "receive_delivery",
        purpose: "Book the arrival of everything still outstanding on ONE purchase order the user already placed: the goods move into the place they name, the order becomes received, and a DRAFT bill is raised for what arrived — approved by nobody and in no payment run until a person decides on it. It receives the whole of what is still to come; a delivery that differs from the order — short, part or damaged — is booked line by line in the app by the person unpacking it. Never call it for an order the user has not said arrived.",
        effect: Effect::Write,
        args: &[
            Arg::required(
                "order",
                "text",
                "which order arrived — its number exactly as the app shows it, or the supplier's name when only one of their orders is open",
            ),
            Arg::required(
                "location",
                "text",
                "where the goods were put, by the place's name or its code — the one fact a delivery cannot infer",
            ),
            Arg::optional("note", "text", "what the person unpacking it said"),
        ],
        answers: &[
            "the delivery from this supplier arrived",
            "book the goods in",
            "receive that order into the main warehouse",
        ],
        preview: Some(
            "The delivery against order {order} will be booked into {location}: everything still outstanding moves into stock and a DRAFT bill is raised — approved by nobody until a person decides on it.",
        ),
        undo: None,
        routes: &["/inventory/purchase-orders/{id}/receipts"],
    },
];

/// The Inventory routes deliberately without a verb, each with its reason.
pub const INVENTORY_EXCLUDED: &[Excluded] = &[
    Excluded {
        route: "/inventory/suppliers",
        why: "Keeping the supplier book — who we buy from — is configuration, done by a person in the app; supplier_prices answers what one of them sells us.",
    },
    Excluded {
        route: "/inventory/suppliers/{id}",
        why: "Editing a supplier's terms, account and address is configuration, done by a person in the app.",
    },
    Excluded {
        route: "/inventory/suppliers/{id}/archive",
        why: "Ending a supplier relationship is a person's own decision in the app.",
    },
    Excluded {
        route: "/inventory/suppliers/{id}/products/{product_id}",
        why: "Recording what a supplier quotes us is their offer, typed in by a person in the app — a price an agent wrote would be a price nobody agreed.",
    },
    Excluded {
        route: "/inventory/locations",
        why: "Naming the places stock lives is configuration, set by a person in the app.",
    },
    Excluded {
        route: "/inventory/locations/{id}",
        why: "Editing or deleting a place is configuration, set by a person in the app.",
    },
    Excluded {
        route: "/inventory/locations/{id}/archive",
        why: "Closing a place is configuration, set by a person in the app.",
    },
    Excluded {
        route: "/inventory/scan",
        why: "Pointing a camera at a barcode is the scanner screen's own read; stock_answer takes the same barcode as a word.",
    },
    Excluded {
        route: "/inventory/purchase-orders/{id}",
        why: "Editing one order's header and lines is the order screen's own work; open_purchase_orders answers where each order stands.",
    },
    Excluded {
        route: "/inventory/purchase-orders/{id}/cancel",
        why: "Giving up on an order — and accepting a short delivery as final — is a person's own decision in the app.",
    },
    Excluded {
        route: "/inventory/purchase-orders/{id}/send",
        why: "Sending an order draws its number and mails another company; a person presses it.",
    },
    Excluded {
        route: "/inventory/purchase-orders/{id}/print",
        why: "Produces a file.",
    },
    Excluded {
        route: "/inventory/purchase-orders/{id}/pdf",
        why: "Produces a file.",
    },
    Excluded {
        route: "/inventory/sales-orders",
        why: "Promising goods to a customer is the sales-order screen's own work, priced and confirmed by a person.",
    },
    Excluded {
        route: "/inventory/sales-orders/{id}",
        why: "Editing a customer's order is the sales-order screen's own work.",
    },
    Excluded {
        route: "/inventory/sales-orders/{id}/confirm",
        why: "Confirming a customer's order commits stock to them; a person decides it.",
    },
    Excluded {
        route: "/inventory/sales-orders/{id}/cancel",
        why: "Letting a customer down deserves a person's own decision in the app.",
    },
    Excluded {
        route: "/inventory/sales-orders/{id}/deliveries",
        why: "Shipping a customer's goods writes the movement out of stock; the person who packed the box presses it.",
    },
    Excluded {
        route: "/inventory/sales-orders/{id}/invoice",
        why: "Raising the invoice for a delivery is a billing decision, made by a person in the app.",
    },
    Excluded {
        route: "/inventory/sales-orders/{id}/invoices",
        why: "The invoices behind one order are the order screen's own read.",
    },
    Excluded {
        route: "/inventory/order-book",
        why: "The order book is the screen that weighs promises against shelves; stock_answer reports one product's standing.",
    },
    Excluded {
        route: "/inventory/reorder-rules",
        why: "A minimum is the tenant's standing instruction; setting one is configuration, done by a person in the app.",
    },
    Excluded {
        route: "/inventory/reorder-rules/{id}",
        why: "Changing or deleting a minimum is configuration, done by a person in the app.",
    },
    Excluded {
        route: "/inventory/shortages.csv",
        why: "Produces a file.",
    },
    Excluded {
        route: "/inventory/counts",
        why: "A stocktake is done standing in the warehouse, by the person counting.",
    },
    Excluded {
        route: "/inventory/counts/{id}",
        why: "A count sheet is filled in by the person counting, in the app.",
    },
    Excluded {
        route: "/inventory/counts/{id}/lines/{product_id}",
        why: "A counted figure is what somebody actually saw on the shelf; only they may write it.",
    },
    Excluded {
        route: "/inventory/counts/{id}/apply",
        why: "Applying a count adjusts the ledger — the most abusable write in the warehouse; a person does it, in the app.",
    },
    Excluded {
        route: "/inventory/counts/{id}/cancel",
        why: "Abandoning a count is the counter's own act in the app.",
    },
];

/// The Inventory paragraph of the agent's general instructions.
pub const INVENTORY_GUIDANCE: &str = "For an Inventory verb, NEVER invent a quantity, a price, a supplier or a delivery date: the minimums are the tenant's own standing instructions, the quantities follow from what is actually on their shelves, and the prices are what a supplier has already quoted them. Name a product, a supplier, an order or a place with the words the user used and let the verb find it — when more than one matches, the verb says so and asks, which is an answer to repeat rather than a choice to make for them. A draft purchase order reorder_proposals writes is not an order: never tell the user anything has been ordered, sent or confirmed — say the drafts are waiting for them to send. A delivery receive_delivery books puts goods on a shelf and raises a DRAFT bill: never tell the user a bill has been approved or paid — say it is waiting for a person to decide on. Stock figures are true as at the moment they were read: repeat them rather than recomputing anything, add none the verb did not return, and never turn them into a forecast of when something will run out.\n";

/// The module, as the registry reads it.
pub static INVENTORY: IntentModule = IntentModule {
    intents: INVENTORY_INTENTS,
    excluded: INVENTORY_EXCLUDED,
    guidance: INVENTORY_GUIDANCE,
};

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn every_verb_has_a_route_a_purpose_and_a_question_it_answers() {
        for intent in INVENTORY_INTENTS {
            assert!(!intent.routes.is_empty(), "{} names no route", intent.name);
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
        let mut names: Vec<&str> = INVENTORY_INTENTS.iter().map(|i| i.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), INVENTORY_INTENTS.len());
        let doc = INVENTORY.doc();
        for intent in INVENTORY_INTENTS {
            assert_eq!(doc.matches(&format!("- {}:", intent.name)).count(), 1);
        }
        assert!(INVENTORY_GUIDANCE.ends_with('\n'));
    }

    #[test]
    fn an_exclusion_is_a_sentence_not_a_shrug() {
        for excluded in INVENTORY_EXCLUDED {
            assert!(
                excluded.why.ends_with('.'),
                "{}: {}",
                excluded.route,
                excluded.why
            );
            assert!(
                !INVENTORY_INTENTS
                    .iter()
                    .any(|i| i.routes.contains(&excluded.route)),
                "{} is both a verb's route and excluded",
                excluded.route
            );
        }
    }

    #[test]
    fn the_model_is_never_offered_a_quantity_or_a_price_to_choose() {
        // The one mistake this module can make that nothing downstream
        // catches: a quantity or a price the model made up, arriving as an
        // argument on a document that goes to another company.
        let doc = INVENTORY.doc();
        for forbidden in [
            "\"qty\"",
            "\"quantity\"",
            "\"qtyMilli\"",
            "\"price\"",
            "\"unitPriceCents\"",
            "\"amount\"",
            "\"lines\"",
        ] {
            assert!(!doc.contains(forbidden), "{forbidden}");
        }
        assert!(doc.contains("there is no argument for either"));
        assert!(INVENTORY_GUIDANCE.contains("NEVER invent a quantity"));
    }

    #[test]
    fn the_wording_says_a_draft_order_has_been_sent_to_nobody() {
        let reorder = INVENTORY
            .find("reorder_proposals")
            .expect("reorder_proposals is a verb");
        assert!(reorder.purpose.contains("SAVED AS A DRAFT"));
        assert!(reorder.purpose.contains("sent to NOBODY"));
        assert!(reorder.purpose.contains("carries no order number"));
        assert!(INVENTORY_GUIDANCE.contains("is not an order"));
        assert!(INVENTORY_GUIDANCE.contains("waiting for them to send"));
    }

    #[test]
    fn a_booked_delivery_raises_a_draft_bill_and_the_wording_says_so() {
        // The mistake receive_delivery invites: a model telling the user the
        // supplier has been paid because a bill exists. The wording forbids
        // the belief at its source.
        let receive = INVENTORY
            .find("receive_delivery")
            .expect("receive_delivery is a verb");
        assert!(receive.purpose.contains("DRAFT bill"));
        assert!(receive.purpose.contains("approved by nobody"));
        // The whole-delivery bound is stated, so a short or damaged delivery
        // goes to the app rather than through a guessed line set.
        assert!(receive.purpose.contains("booked line by line in the app"));
        assert!(
            receive
                .preview
                .expect("a write has a preview")
                .contains("DRAFT bill")
        );
        assert!(
            INVENTORY_GUIDANCE.contains("never tell the user a bill has been approved or paid")
        );
    }

    #[test]
    fn nothing_here_forecasts_anything() {
        // The design note's deliberate absence, held in the words the model
        // reads: today's figures are the whole answer.
        let doc = INVENTORY.doc();
        assert!(doc.contains("never predict what will be needed"));
        assert!(doc.contains("never estimate when stock will run out"));
        assert!(INVENTORY_GUIDANCE.contains("never turn them into a forecast"));
        for forbidden in ["forecast the", "seasonality", "predicted demand"] {
            assert!(!doc.contains(forbidden), "{forbidden}");
        }
    }

    #[test]
    fn nothing_inventory_offers_sends_ships_adjusts_or_counts() {
        // The writes that reach a counterparty or rewrite the ledger by hand,
        // none of which an agent may propose: sending is a person's act, a
        // customer delivery is the packer's, and an adjustment is the most
        // abusable write in the warehouse.
        for forbidden in [
            "send_purchase_order",
            "deliver_sales_order",
            "adjust_stock",
            "apply_stocktake",
            "create_supplier",
            "set_supplier_price",
        ] {
            assert!(INVENTORY.find(forbidden).is_none());
            assert!(!INVENTORY.doc().contains(forbidden));
        }
    }
}
