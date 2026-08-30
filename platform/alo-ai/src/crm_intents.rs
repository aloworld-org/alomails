//! alo CRM's verbs (ADR 0058) — the Sales agent over the one command layer.
//!
//! This is the whole of what the CRM agent may do, and the words a model reads
//! about it. Nothing here reads or writes a record: the executors live beside
//! CRM's routes in `alo-jmap` (`crm_intents.rs`), through the asker's
//! tenant-scoped store, and answer with the same record views the `/crm/*`
//! routes serve.
//!
//! Three rules from the module shape the wording, each a mistake it exists to
//! prevent:
//!
//! - **A deal is named, never numbered.** Unlike an invoice, an opportunity has
//!   no number a person can quote, so a verb resolves the *title* the user said
//!   against the tenant's own deals. The model is told to pass it through
//!   verbatim: a title it "tidied" resolves to nothing, or worse, to the wrong
//!   card.
//! - **Money is integer cents.** A deal's value is asked for in whole cents,
//!   like every other amount in the suite.
//! - **Losing a deal takes a reason and winning one takes none.** The store
//!   demands the first and refuses the second; saying so here turns a `422` a
//!   user would have to read into an argument the model gets right.
//!
//! Nothing here closes a sale on its own account: `move_deal_stage` is the one
//! verb that can win or lose a deal, and like every write it is proposed,
//! previewed and approved by the person who asked before it runs.

use crate::agent_tool::Effect;
use crate::intent::{Arg, Excluded, IntentModule, IntentSpec};

const DEAL_REQ: Arg = Arg::required(
    "deal",
    "text",
    "the deal's title, exactly as the user says it",
);
const COMPANY_OPT: Arg = Arg::optional(
    "company",
    "text",
    "the company's or contact's name, exactly as the user gave it",
);
const PIPELINE_OPT: Arg = Arg::optional(
    "pipeline",
    "text",
    "the board's name — only needed when the tenant has more than one",
);

/// The verbs.
pub const CRM_INTENTS: &[IntentSpec] = &[
    // ---- reads ---------------------------------------------------------
    IntentSpec {
        name: "open_deals",
        purpose: "The opportunities still open, column by column — each with its title, company, owner, value and expected close — with a count and value per stage and per owner.",
        effect: Effect::Read,
        args: &[
            Arg::optional(
                "stage",
                "text",
                "limit to one column of the board, by its name",
            ),
            PIPELINE_OPT,
            Arg::optional("owner", "text", "\"me\" to see only the asker's own deals"),
        ],
        answers: &[
            "which deals are open, and at what stage",
            "what is in the pipeline",
            "what are my open deals",
            "who is working on what",
        ],
        preview: None,
        undo: None,
        routes: &["/crm/deals"],
    },
    IntentSpec {
        name: "deal_lookup",
        purpose: "One deal in full — its stage, value, contact, its history of moves and what was last said and done on it — by its title, or the company's most recent deal when only the company is named.",
        effect: Effect::Read,
        args: &[
            Arg::optional(
                "deal",
                "text",
                "the deal's title, exactly as the user says it",
            ),
            COMPANY_OPT,
        ],
        answers: &[
            "where are we with the {deal} deal",
            "what is the status of {deal}",
            "when did {deal} move stage",
            "what happened on {deal} lately",
        ],
        preview: None,
        undo: None,
        routes: &[
            "/crm/deals/{id}",
            "/crm/deals/{id}/history",
            "/crm/deals/{id}/activities",
        ],
    },
    IntentSpec {
        name: "pipeline_summary",
        purpose: "What the sales board is worth: the open deals and value standing in each column now, and what was won and lost over a period — this year unless another period is named. Figures are the server's, in cents.",
        effect: Effect::Read,
        args: &[
            PIPELINE_OPT,
            Arg::optional(
                "from",
                "date",
                "start of the period, YYYY-MM-DD; default: 1 January this year",
            ),
            Arg::optional(
                "to",
                "date",
                "end of the period, YYYY-MM-DD, inclusive; default: today",
            ),
        ],
        answers: &[
            "how is the pipeline looking",
            "what did we win this year",
            "what is our win rate",
            "how much is on the board",
        ],
        preview: None,
        undo: None,
        routes: &["/crm/reports/pipeline"],
    },
    IntentSpec {
        name: "company_history",
        purpose: "Where we stand with one company or contact: every deal with them — open, won and lost — and the latest notes, calls and moves on each.",
        effect: Effect::Read,
        args: &[Arg::required(
            "company",
            "text",
            "the company's or the contact's name, exactly as the user gave it",
        )],
        answers: &[
            "where are we with {company}",
            "have we spoken to {company} before",
            "what deals have we had with {company}",
            "what have we done with {company} lately",
        ],
        preview: None,
        undo: None,
        routes: &["/crm/deals", "/crm/deals/{id}/activities"],
    },
    // ---- writes: propose, then the asker approves ------------------------
    IntentSpec {
        name: "create_deal",
        purpose: "Raise an opportunity on the sales board. Its value is stated in whole cents, e.g. 500000 for 5000.00. Never invent a value, a close date or a contact the user did not give.",
        effect: Effect::Write,
        args: &[
            Arg::required("title", "text", "what the opportunity is"),
            Arg::optional("company", "text", "the company it is with"),
            Arg::optional("contactName", "text", "the person being spoken to"),
            Arg::optional("contactEmail", "text", "their address"),
            Arg::optional("valueCents", "integer", "what it is worth, in whole cents"),
            Arg::optional("currency", "text", "ISO 4217, e.g. \"EUR\""),
            Arg::optional("expectedClose", "date", "YYYY-MM-DD"),
            Arg::optional(
                "origin",
                "text",
                "where the lead came from, e.g. \"Referral\"",
            ),
            PIPELINE_OPT,
            Arg::optional(
                "stage",
                "text",
                "the column's name; the first column of the board by default",
            ),
            Arg::optional(
                "source",
                "number",
                "the numbered source of an EMAIL this deal comes from — the conversation is then linked to the new deal",
            ),
        ],
        answers: &[
            "raise a deal for {company}",
            "add an opportunity",
            "we might have a new opportunity",
        ],
        preview: Some(
            "A deal \"{title}\" will be raised on the sales board — a card on the board, nothing sent.",
        ),
        undo: None,
        routes: &["/crm/deals"],
    },
    IntentSpec {
        name: "move_deal_stage",
        purpose: "Move a deal to another column of its own board — this is also how a deal is won or lost. Why it was lost is REQUIRED when the column means lost, and refused for any other column.",
        effect: Effect::Write,
        args: &[
            DEAL_REQ,
            Arg::required("stage", "text", "the column to move it to"),
            Arg::optional(
                "reason",
                "text",
                "why it was lost — only when the column means lost",
            ),
        ],
        answers: &[
            "move {deal} to negotiation",
            "we won the {deal} deal",
            "mark {deal} as lost",
        ],
        preview: Some("The deal \"{deal}\" will be moved to \"{stage}\"."),
        undo: None,
        routes: &["/crm/deals/{id}/stage"],
    },
    IntentSpec {
        name: "draft_followup",
        purpose: "Write a follow-up email about a deal and save it to the user's Drafts for them to review and send — it is NEVER sent automatically. Compose the body from the request; do not invent facts about the deal, and never state a recipient — the letter goes to the deal's own contact.",
        effect: Effect::Write,
        args: &[
            DEAL_REQ,
            Arg::required("body", "text", "the whole letter"),
            Arg::optional("subject", "text", "the deal's title by default"),
        ],
        answers: &[
            "chase the {deal} deal",
            "follow up on {deal}",
            "write to them about the offer",
        ],
        preview: Some(
            "A follow-up about \"{deal}\" will be written to the user's Drafts — not sent.",
        ),
        undo: None,
        // The draft lands in the user's Mail Drafts, not through a /crm route —
        // the one verb of this module with no route to stand behind.
        routes: &[],
    },
];

/// The CRM routes deliberately without a verb, each with its reason.
pub const CRM_EXCLUDED: &[Excluded] = &[
    Excluded {
        route: "/crm/pipelines",
        why: "Boards are made and renamed by a person in the app.",
    },
    Excluded {
        route: "/crm/pipelines/{id}",
        why: "Boards are made and renamed by a person in the app.",
    },
    Excluded {
        route: "/crm/pipelines/{id}/archive",
        why: "Boards are made and renamed by a person in the app.",
    },
    Excluded {
        route: "/crm/pipelines/{id}/stages",
        why: "The board's columns are its design — a person's work in the app.",
    },
    Excluded {
        route: "/crm/stages/{id}",
        why: "The board's columns are its design — a person's work in the app.",
    },
    Excluded {
        route: "/crm/stages/{id}/move",
        why: "The board's columns are its design — a person's work in the app.",
    },
    Excluded {
        route: "/crm/stages/{id}/archive",
        why: "The board's columns are its design — a person's work in the app.",
    },
    Excluded {
        route: "/crm/deals/{id}/quote",
        why: "The won-deal handoff raises a Billing draft; offers are the Billing agent's to propose, a later intent set.",
    },
    Excluded {
        route: "/crm/deals/{id}/invoice",
        why: "The won-deal handoff raises a Billing draft; invoices are the Billing agent's to propose, a later intent set.",
    },
    Excluded {
        route: "/crm/deals/{id}/threads",
        why: "Linking a conversation to a deal is a person's decision in the app; a later intent set.",
    },
    Excluded {
        route: "/crm/deals/{id}/threads/{threadId}",
        why: "Linking a conversation to a deal is a person's decision in the app; a later intent set.",
    },
    Excluded {
        route: "/crm/deals/{id}/thread-suggestions",
        why: "Linking a conversation to a deal is a person's decision in the app; a later intent set.",
    },
    Excluded {
        route: "/crm/activities/{id}",
        why: "Deleting a note is its author's own act in the app.",
    },
    Excluded {
        route: "/crm/deals/{id}/next-steps",
        why: "A next step is a real task in the tasks module; the Tasks agent's to read and propose.",
    },
    Excluded {
        route: "/crm/reports/pipeline.csv",
        why: "Produces a file; pipeline_summary answers the question.",
    },
    Excluded {
        route: "/crm/imports/leads/preview",
        why: "Takes a file.",
    },
    Excluded {
        route: "/crm/imports/leads",
        why: "Takes a file.",
    },
];

/// The CRM paragraph of the agent's general instructions.
pub const CRM_GUIDANCE: &str = "For a CRM verb, pass a deal title, a board name or a column name through EXACTLY as the user gave it — never invent, complete or reformat one, and never guess which deal was meant. A deal has no number: it is found by its title, so it is not in the numbered sources — but an EMAIL a deal comes from is, and create_deal takes that source number to link the conversation. To answer a question about deals, companies or the pipeline, USE a reading verb first and answer from what it returned, quoting amounts as returned (amounts are integer cents: 500000 is 5000.00). If a lookup lists several matching deals, ask which was meant rather than picking one.\n";

/// The module, as the registry reads it.
pub static CRM: IntentModule = IntentModule {
    intents: CRM_INTENTS,
    excluded: CRM_EXCLUDED,
    guidance: CRM_GUIDANCE,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_verb_has_a_route_a_purpose_and_a_question_it_answers() {
        for intent in CRM_INTENTS {
            // `draft_followup` writes into Mail's Drafts and has no /crm route
            // to stand behind; every other verb names the route it is the verb
            // of.
            assert!(
                !intent.routes.is_empty() || intent.name == "draft_followup",
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
        let mut names: Vec<&str> = CRM_INTENTS.iter().map(|i| i.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), CRM_INTENTS.len());
        let doc = CRM.doc();
        for intent in CRM_INTENTS {
            assert_eq!(doc.matches(&format!("- {}:", intent.name)).count(), 1);
        }
        assert!(CRM_GUIDANCE.ends_with('\n'));
    }

    #[test]
    fn an_exclusion_is_a_sentence_not_a_shrug() {
        for excluded in CRM_EXCLUDED {
            assert!(
                excluded.why.ends_with('.'),
                "{}: {}",
                excluded.route,
                excluded.why
            );
            assert!(
                !CRM_INTENTS
                    .iter()
                    .any(|i| i.routes.contains(&excluded.route)),
                "{} is both a verb's route and excluded",
                excluded.route
            );
        }
    }

    #[test]
    fn the_wording_states_the_rules_a_model_would_otherwise_get_wrong() {
        let doc = CRM.doc();
        // Money is asked for the only way we store it.
        assert!(doc.contains("valueCents"));
        assert!(doc.contains("whole cents"));
        assert!(!doc.contains("valueEuros"));
        // A lost column demands a reason; every other column refuses one.
        assert!(doc.contains("REQUIRED when the column means lost"));
        // A follow-up is a draft, and its recipient is never the model's to say.
        assert!(doc.contains("NEVER sent automatically"));
        assert!(doc.contains("never state a recipient"));
    }

    #[test]
    fn nothing_crm_offers_deletes_a_record_or_sends_mail() {
        for forbidden in ["delete_deal", "send_followup", "delete_pipeline"] {
            assert!(CRM.find(forbidden).is_none());
            assert!(!CRM.doc().contains(forbidden));
        }
    }
}
