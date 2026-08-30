//! alo Billing's verbs (ADR 0058) — the reference module of the one command
//! layer.
//!
//! This is the whole of what the Billing agent may do, and the words a model
//! reads about it. Nothing here reads or writes a record: the executors live
//! beside Billing's routes in `alo-jmap` (`billing_intents.rs`), through the
//! asker's tenant-scoped store, and a route and an intent call the same
//! functions.
//!
//! Two rules from the constitution shape the wording:
//!
//! - **Money is integer cents.** A price is asked for in whole cents and a VAT
//!   rate in basis points; a quantity may be fractional and is converted
//!   exactly at the boundary.
//! - **Numbering and sending are proposals.** `send_quote`, `issue_invoice`
//!   and `record_payment` are writes: previewed, then approved by the person
//!   who asked. Nothing here puts mail on the wire — a reminder is a draft.
//!
//! Every Billing route is either the adapter of a verb below or listed as
//! excluded with its reason; the coverage test in `alo-jmap` holds that.

use crate::agent_tool::Effect;
use crate::intent::{Arg, Excluded, IntentModule, IntentSpec};

const CUSTOMER_OPT: Arg = Arg::optional("customer", "text", "limit to one customer, by name");
const CUSTOMER_REQ: Arg = Arg::required(
    "customer",
    "text",
    "the customer's name, exactly as the user gave it",
);
const QUOTE_OPT: Arg = Arg::optional(
    "quote",
    "text",
    "the quote number exactly as the user says it, e.g. \"QUO-2026-00001\"",
);
const INVOICE_OPT: Arg = Arg::optional(
    "invoice",
    "text",
    "the invoice number exactly as the user says it, e.g. \"INV-2026-00042\"",
);
const INVOICE_REQ: Arg = Arg::required(
    "invoice",
    "text",
    "the invoice number exactly as the user says it, e.g. \"INV-2026-00042\"",
);

/// The verbs.
pub const BILLING_INTENTS: &[IntentSpec] = &[
    // ---- reads ---------------------------------------------------------
    IntentSpec {
        name: "open_quotes",
        purpose: "The offers that are open — sent and not yet answered — newest first, each with its number, customer, validity and what it is worth; plus how many drafts are not sent yet.",
        effect: Effect::Read,
        args: &[CUSTOMER_OPT],
        answers: &[
            "which quotes are open",
            "what have we offered lately",
            "what is open with X",
            "how many drafts are waiting",
        ],
        preview: None,
        undo: None,
        routes: &["/billing/quotes"],
    },
    IntentSpec {
        name: "quote_lookup",
        purpose: "One offer in full — lines, totals, status, dates — by its number, or the customer's most recent offer when only the customer is named.",
        effect: Effect::Read,
        args: &[QUOTE_OPT, CUSTOMER_OPT],
        answers: &[
            "what did we quote X",
            "show me quote QUO-2026-00031",
            "what is in the offer for X",
            "has the quote for X been sent",
        ],
        preview: None,
        undo: None,
        routes: &["/billing/quotes/{id}"],
    },
    IntentSpec {
        name: "customer_lookup",
        purpose: "A customer's record — address, VAT id, terms, currency — with their open offers and unpaid invoices. Several customers matching the name are listed for the user to choose.",
        effect: Effect::Read,
        args: &[CUSTOMER_REQ],
        answers: &[
            "where are we with X",
            "what are X's payment terms",
            "do we have a customer called X",
            "what does X owe us",
        ],
        preview: None,
        undo: None,
        routes: &["/billing/customers", "/billing/customers/{id}"],
    },
    IntentSpec {
        name: "unpaid_invoices",
        purpose: "Issued invoices not yet paid in full, newest first, each with what is outstanding and whether it is overdue.",
        effect: Effect::Read,
        args: &[CUSTOMER_OPT],
        answers: &[
            "which invoices are unpaid",
            "what is overdue",
            "who owes us money",
            "how much is outstanding",
        ],
        preview: None,
        undo: None,
        routes: &["/billing/invoices"],
    },
    IntentSpec {
        name: "invoice_lookup",
        purpose: "One invoice in full — lines, totals, payments, what is outstanding — by its number, or the customer's most recent invoice when only the customer is named.",
        effect: Effect::Read,
        args: &[INVOICE_OPT, CUSTOMER_OPT],
        answers: &[
            "show me invoice INV-2026-00042",
            "what did we invoice X last",
            "is invoice INV-2026-00042 paid",
        ],
        preview: None,
        undo: None,
        routes: &["/billing/invoices/{id}"],
    },
    IntentSpec {
        name: "billing_totals",
        purpose: "What was invoiced, paid and is still outstanding over a period, with VAT — this year unless another period is named. Figures are the server's, in cents.",
        effect: Effect::Read,
        args: &[
            Arg::optional(
                "period",
                "text",
                "this-month, last-month, this-year or last-year",
            ),
            Arg::optional("from", "date", "start of a custom period, YYYY-MM-DD"),
            Arg::optional(
                "to",
                "date",
                "end of a custom period, YYYY-MM-DD, inclusive",
            ),
        ],
        answers: &[
            "how much did we invoice this year",
            "what did we bill last month",
            "how much is unpaid",
            "what was our turnover in August",
        ],
        preview: None,
        undo: None,
        // Summed over the same record views `/billing/invoices` serves — the
        // by-rate VAT report answers a different question (see the exclusions).
        routes: &["/billing/invoices"],
    },
    // ---- writes: propose, then the asker approves ------------------------
    IntentSpec {
        name: "create_invoice_draft",
        purpose: "Raise a DRAFT invoice for a customer. A draft carries no number, is not sent, and is not owed by anyone until the user issues it. Each line is EITHER {\"product\": text (a name from the tenant's price list), \"quantity\": number (units, may be fractional, default 1)} — its unit, price and VAT rate then come from the price list — OR {\"description\": text, \"quantity\": number, \"unitPriceCents\": integer (the price of ONE unit in whole cents, e.g. 12000 for 120.00), \"vatRateBp\": integer (the VAT rate in basis points, e.g. 2100 for 21%), \"unit\": text (optional, e.g. \"hour\")}. Never state both a product and a price on one line, never invent a price or a VAT rate the user did not give, and never write a total: the server computes every total from the lines.",
        effect: Effect::Write,
        args: &[
            CUSTOMER_REQ,
            Arg::required(
                "lines",
                "array",
                "at least one line, in the shape described",
            ),
            Arg::optional("reference", "text", "the customer's own order/PO reference"),
            Arg::optional("note", "text", "a note printed under the lines"),
        ],
        answers: &[
            "invoice X for …",
            "raise an invoice",
            "bill X for the consulting",
        ],
        preview: Some(
            "A draft invoice for {customer} will be raised — unnumbered, unsent, for the user to issue.",
        ),
        undo: Some("discard_invoice_draft"),
        routes: &["/billing/invoices"],
    },
    IntentSpec {
        name: "quote_to_invoice",
        purpose: "Accept an offer the customer has agreed to: the offer closes as accepted and a DRAFT invoice is raised carrying a copy of its lines.",
        effect: Effect::Write,
        args: &[Arg::required(
            "quote",
            "text",
            "the quote number exactly as the user says it, e.g. \"QUO-2026-00001\"",
        )],
        answers: &[
            "make that quote an invoice",
            "X accepted the offer",
            "convert the quote",
        ],
        preview: Some(
            "Quote {quote} will be closed as accepted and a draft invoice raised from its lines.",
        ),
        undo: None,
        routes: &["/billing/quotes/{id}/accept"],
    },
    IntentSpec {
        name: "draft_payment_reminder",
        purpose: "Write a reminder about an unpaid invoice to the customer and save it to the user's Drafts for them to review and send — it is NEVER sent automatically.",
        effect: Effect::Write,
        args: &[
            INVOICE_REQ,
            Arg::optional("note", "text", "one extra sentence to add to the reminder"),
        ],
        answers: &[
            "chase invoice INV-…",
            "remind X about their invoice",
            "chase everyone overdue",
        ],
        preview: Some(
            "A reminder about invoice {invoice} will be written to the user's Drafts — not sent.",
        ),
        undo: None,
        routes: &["/billing/invoices/{id}/reminder"],
    },
    IntentSpec {
        name: "send_quote",
        purpose: "Send an offer: it gets its number, its validity clock starts, and its lines and design are frozen. Named by its number, or by the customer for their newest draft.",
        effect: Effect::Write,
        args: &[QUOTE_OPT, CUSTOMER_OPT],
        answers: &[
            "send the quote",
            "send X their offer",
            "mark the quote as sent",
        ],
        preview: Some(
            "The offer for {customer} will be numbered and marked sent; its lines and design will be frozen.",
        ),
        undo: None,
        routes: &["/billing/quotes/{id}/send"],
    },
    IntentSpec {
        name: "issue_invoice",
        purpose: "Issue a draft invoice: it gets its legal number and its due date, and becomes owed. An issued invoice cannot be un-issued; a mistake is corrected with a credit note. Named by the customer for their newest draft.",
        effect: Effect::Write,
        args: &[CUSTOMER_REQ],
        answers: &[
            "issue the invoice for X",
            "finalise X's invoice",
            "number the invoice",
        ],
        preview: Some(
            "The draft invoice for {customer} will be issued — numbered, dated, and owed.",
        ),
        undo: None,
        routes: &["/billing/invoices/{id}/issue"],
    },
    IntentSpec {
        name: "record_payment",
        purpose: "Record that an invoice was paid, in whole or in part. The amount defaults to what is outstanding.",
        effect: Effect::Write,
        args: &[
            INVOICE_REQ,
            Arg::optional(
                "amountCents",
                "integer",
                "the amount received, in whole cents; default: everything outstanding",
            ),
            Arg::optional("paidOn", "date", "YYYY-MM-DD; default: today"),
            Arg::optional(
                "method",
                "text",
                "transfer, card, cash …; default: transfer",
            ),
            Arg::optional("reference", "text", "the bank reference"),
        ],
        answers: &[
            "X paid invoice INV-…",
            "record the payment",
            "mark INV-… as paid",
        ],
        preview: Some(
            "A payment of {amountCents} cents will be recorded against invoice {invoice}.",
        ),
        undo: Some("delete_payment"),
        routes: &["/billing/invoices/{id}/payments"],
    },
    // ---- the inverse verbs (A8.2): what the Undo button runs --------------
    IntentSpec {
        name: "discard_invoice_draft",
        purpose: "Discard a DRAFT invoice — its lines go with it. A draft never carried a number, so nothing is missing from the series afterwards; an issued invoice cannot be discarded and says so. Named by its id (from an action's undo), or by the customer for their newest draft.",
        effect: Effect::Write,
        args: &[
            Arg::optional(
                "invoice",
                "text",
                "the draft's id, exactly as an action record or lookup returned it",
            ),
            CUSTOMER_OPT,
        ],
        answers: &[
            "discard the draft invoice",
            "throw away the draft for X",
            "undo the invoice draft",
        ],
        preview: Some(
            "The draft invoice will be discarded — it never carried a number, so the series keeps no gap.",
        ),
        undo: None,
        routes: &["/billing/invoices/{id}"],
    },
    IntentSpec {
        name: "delete_payment",
        purpose: "Take back a payment recorded wrongly: the newest payment on an invoice is removed, its ledger entry reversed, and the invoice owed again. Named by the payment's id (from an action's undo), or by the invoice for its newest payment.",
        effect: Effect::Write,
        args: &[
            Arg::optional(
                "payment",
                "text",
                "the payment's id, exactly as an action record returned it",
            ),
            INVOICE_OPT,
        ],
        answers: &[
            "remove the payment on INV-…",
            "that payment was a mistake",
            "undo the payment",
        ],
        preview: Some(
            "The payment will be removed and its invoice owed again — the ledger keeps the reversal, nothing is edited.",
        ),
        undo: None,
        routes: &["/billing/invoices/{id}/payments/{payment_id}"],
    },
];

/// The Billing routes deliberately without a verb, each with its reason.
pub const BILLING_EXCLUDED: &[Excluded] = &[
    Excluded {
        route: "/billing/customers/{id}/archive",
        why: "Archiving a customer is a person's decision in the app; a later intent set.",
    },
    Excluded {
        route: "/billing/products",
        why: "The price list is kept by a person in the app; a later intent set.",
    },
    Excluded {
        route: "/billing/products/{id}",
        why: "The price list is kept by a person in the app; a later intent set.",
    },
    Excluded {
        route: "/billing/products/{id}/archive",
        why: "The price list is kept by a person in the app; a later intent set.",
    },
    Excluded {
        route: "/billing/price-connections",
        why: "Supplier and customer catalogue connections are configured by a person in the app; a later intent set.",
    },
    Excluded {
        route: "/billing/price-connections/{id}",
        why: "Pausing or disconnecting a catalogue connection is a person's decision in the app; a later intent set.",
    },
    Excluded {
        route: "/billing/price-connections/{id}/sync",
        why: "A person starts an exceptional catalogue sync in the app; scheduled synchronization remains automatic.",
    },
    Excluded {
        route: "/billing/invoices/{id}/void",
        why: "Voiding and crediting are corrections a person makes deliberately; a later intent set.",
    },
    Excluded {
        route: "/billing/invoices/{id}/credit-note",
        why: "Voiding and crediting are corrections a person makes deliberately; a later intent set.",
    },
    Excluded {
        route: "/billing/invoices/{id}/print",
        why: "Serves a file; an agent answers from the record instead.",
    },
    Excluded {
        route: "/billing/invoices/{id}/pdf",
        why: "Serves a file; an agent answers from the record instead.",
    },
    Excluded {
        route: "/billing/invoices/{id}/facturx.xml",
        why: "Serves a file; an agent answers from the record instead.",
    },
    Excluded {
        route: "/billing/invoices/{id}/xrechnung.xml",
        why: "Serves a file; an agent answers from the record instead.",
    },
    Excluded {
        route: "/billing/invoices/{id}/send",
        why: "Composes a mail draft with the PDF; the agent's draft_payment_reminder covers chasing, sending an invoice is a later intent.",
    },
    Excluded {
        route: "/billing/quotes/{id}/email-draft",
        why: "Composes a customer mail draft with the finalized quotation PDF; customer delivery remains a deliberate action in the app.",
    },
    Excluded {
        route: "/billing/bills/import",
        why: "Takes a file.",
    },
    Excluded {
        route: "/billing/bills",
        why: "Supplier bills are a later intent set.",
    },
    Excluded {
        route: "/billing/bills/{id}",
        why: "Supplier bills are a later intent set.",
    },
    Excluded {
        route: "/billing/bills/{id}/approve",
        why: "Supplier bills are a later intent set.",
    },
    Excluded {
        route: "/billing/bills/{id}/reject",
        why: "Supplier bills are a later intent set.",
    },
    Excluded {
        route: "/billing/bills/sepa.xml",
        why: "Produces a bank file.",
    },
    Excluded {
        route: "/billing/quotes/{id}/decline",
        why: "Closing an offer as declined or expired is a later intent set.",
    },
    Excluded {
        route: "/billing/quotes/{id}/expire",
        why: "Closing an offer as declined or expired is a later intent set.",
    },
    Excluded {
        route: "/billing/quotes/{id}/print",
        why: "Serves a file; an agent answers from the record instead.",
    },
    Excluded {
        route: "/billing/quotes/{id}/pdf",
        why: "Serves a file; an agent answers from the record instead.",
    },
    Excluded {
        route: "/billing/quotes/{id}/design",
        why: "The quotation studio's layout is a person's design work.",
    },
    Excluded {
        route: "/billing/schedules/run",
        why: "Recurring invoices are a later intent set.",
    },
    Excluded {
        route: "/billing/schedules",
        why: "Recurring invoices are a later intent set.",
    },
    Excluded {
        route: "/billing/schedules/{id}",
        why: "Recurring invoices are a later intent set.",
    },
    Excluded {
        route: "/billing/schedules/{id}/pause",
        why: "Recurring invoices are a later intent set.",
    },
    Excluded {
        route: "/billing/schedules/{id}/resume",
        why: "Recurring invoices are a later intent set.",
    },
    Excluded {
        route: "/billing/settings",
        why: "The issuer's identity and bank details are a person's configuration.",
    },
    Excluded {
        route: "/billing/fx/rates",
        why: "Exchange rates are a person's configuration.",
    },
    Excluded {
        route: "/billing/fx/rates/import",
        why: "Takes a file.",
    },
    Excluded {
        route: "/billing/reports/vat",
        why: "The by-rate VAT summary is the accountant's screen; billing_totals answers period questions from the same invoice records.",
    },
    Excluded {
        route: "/billing/reports/vat.csv",
        why: "Produces a file; billing_totals answers the question.",
    },
];

/// The Billing paragraph of the agent's general instructions.
pub const BILLING_GUIDANCE: &str = "For a billing verb, pass a customer, product, invoice or quote name or number through EXACTLY as the user gave it — never invent, complete or reformat a document number, and never guess which customer was meant. To answer a question about offers, invoices, customers or money, USE a reading verb first and answer from what it returned, quoting numbers and amounts as returned (amounts are integer cents: 24900 is 249.00). If a lookup lists several matching customers, ask which was meant rather than picking one. A write is proposed from what the user said, not from a source number.\n";

/// The module, as the registry reads it.
pub static BILLING: IntentModule = IntentModule {
    intents: BILLING_INTENTS,
    excluded: BILLING_EXCLUDED,
    guidance: BILLING_GUIDANCE,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_verb_has_a_route_a_purpose_and_a_question_it_answers() {
        for intent in BILLING_INTENTS {
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
        let mut names: Vec<&str> = BILLING_INTENTS.iter().map(|i| i.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), BILLING_INTENTS.len());
        let doc = BILLING.doc();
        for intent in BILLING_INTENTS {
            assert_eq!(doc.matches(&format!("- {}:", intent.name)).count(), 1);
        }
        assert!(BILLING_GUIDANCE.ends_with('\n'));
    }

    #[test]
    fn an_exclusion_is_a_sentence_not_a_shrug() {
        for excluded in BILLING_EXCLUDED {
            assert!(
                excluded.why.ends_with('.'),
                "{}: {}",
                excluded.route,
                excluded.why
            );
            assert!(
                !BILLING_INTENTS
                    .iter()
                    .any(|i| i.routes.contains(&excluded.route)),
                "{} is both a verb's route and excluded",
                excluded.route
            );
        }
    }
}
