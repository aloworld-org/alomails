//! alo Finance's verbs (ADR 0058) — the Finance agent over the one command
//! layer.
//!
//! This is the whole of what the Finance agent may do, and the words a model
//! reads about it. Nothing here reads or writes a record: the executors live
//! beside Finance's routes in `alo-jmap` (`finance_intents.rs`), through the
//! asker's tenant-scoped store, and answer with the same figures the
//! `/finance/*` routes serve — the journal's, never a second sum.
//!
//! Five rules from the module shape the wording, each a mistake it exists to
//! prevent:
//!
//! - **The model does not classify anything.** `categorise_transactions` asks
//!   the *store* to suggest categories, from the words this person has already
//!   agreed to for the same merchant. The description names no category
//!   argument at all — there is none to pass, and a cost booked to a word
//!   somebody invented is a wrong P&L nobody can see.
//! - **`vat_summary` states its period or it does not run.** Both days are
//!   required, exactly as `GET /finance/reports/vat` requires them: a VAT
//!   figure printed under a heading nobody asked for is the one number that
//!   gets copied into a year-end and argued about a year later.
//! - **`flag_anomalies` names entries, never people.** An anomaly is a fact
//!   about a document; the tool cannot answer a question about a person, and
//!   the wording says so, so a model does not try to make it one.
//! - **Approving an expense claim is a decision about money.** It is the one
//!   write here that changes what the books say, and like every write it is
//!   proposed, previewed and approved by the person who asked before it runs
//!   — and the executors gate it exactly as the approvals inbox is gated.
//! - **A books figure that is short says so** (A10.2). `ledger_summary` reads
//!   the journal, and the journal has only held documents since the day the
//!   document paths were wired to post them, so on an older tenant it answered
//!   `0.00` where Billing answered in full. Its reply now carries the period's
//!   documents beside the entries, and `post_missing_documents` — the one write
//!   here that puts a *past* event in the books rather than making a new one —
//!   closes the difference. It takes no amount, no account and no date to book
//!   on: every posting is the store's own rule applied to a document that
//!   already exists.

use crate::agent_tool::Effect;
use crate::intent::{Arg, Excluded, IntentModule, IntentSpec};

const FROM_OPT: Arg = Arg::optional(
    "from",
    "date",
    "start of the period, YYYY-MM-DD; default: 1 January this year",
);
const TO_OPT: Arg = Arg::optional(
    "to",
    "date",
    "end of the period, YYYY-MM-DD, inclusive; default: today",
);

/// The verbs.
pub const FINANCE_INTENTS: &[IntentSpec] = &[
    // ---- reads ---------------------------------------------------------
    IntentSpec {
        name: "ledger_summary",
        purpose: "What the books say was invoiced, paid and still outstanding — read from the receivables account of the tenant's own journal, entry by entry, with its closing balance as what is open. This year unless another period is named. Figures are the server's, in cents.",
        effect: Effect::Read,
        args: &[FROM_OPT, TO_OPT],
        answers: &[
            "how much have we invoiced this year, and how much is unpaid",
            "how much came in this quarter",
            "what is still outstanding",
            "how are the books looking",
        ],
        preview: None,
        undo: None,
        // The receivables ledger is the journal's own view; no /finance route
        // serves that drill-down yet, so this verb stands behind none.
        routes: &[],
    },
    IntentSpec {
        name: "vat_summary",
        purpose: "The VAT figures the tenant's books carry for a period — tax charged on sales per rate, tax paid on purchases per rate, and the net payable between them. It files nothing with any tax authority. Both days are REQUIRED and neither has a default: work out the days the user meant from today's date and state them.",
        effect: Effect::Read,
        args: &[
            Arg::required("from", "date", "the first day of the period, YYYY-MM-DD"),
            Arg::required(
                "to",
                "date",
                "the last day of the period, included, YYYY-MM-DD",
            ),
        ],
        answers: &[
            "what VAT do we owe",
            "the quarter's VAT figures",
            "how much tax did we charge",
        ],
        preview: None,
        undo: None,
        routes: &["/finance/reports/vat"],
    },
    IntentSpec {
        name: "flag_anomalies",
        purpose: "Read the tenant's journal over a period — the last twelve months unless another period is named — and name what is worth a second look: the same amount booked twice to the same counterparty within a week, an amount far outside what its account usually moves, a monthly cost that skipped a month. It marks nothing as reviewed and accuses nobody: every finding names the entries behind it and never a person, so it cannot answer a question about somebody's spending.",
        effect: Effect::Read,
        args: &[
            Arg::optional(
                "from",
                "date",
                "the first day, YYYY-MM-DD; default: twelve months back",
            ),
            TO_OPT,
        ],
        answers: &[
            "check my books",
            "look over the journal",
            "anything odd in the accounts",
        ],
        preview: None,
        undo: None,
        // The scan is the store's own reading of the journal; no /finance
        // route serves it, so this verb stands behind none.
        routes: &[],
    },
    IntentSpec {
        name: "unmatched_bank_lines",
        purpose: "The imported bank lines not yet matched to any document — money that arrived or left the account that the books cannot yet explain, oldest first.",
        effect: Effect::Read,
        args: &[],
        answers: &[
            "which bank lines are unmatched",
            "what has not been reconciled",
            "what arrived that we cannot place",
        ],
        preview: None,
        undo: None,
        routes: &["/finance/bank/lines"],
    },
    IntentSpec {
        name: "expenses_awaiting",
        purpose: "The expense claims waiting on the company — handed in and awaiting a decision, or, when waiting is \"reimbursement\", approved and not yet paid back. Each names its claimant, merchant and amount.",
        effect: Effect::Read,
        args: &[Arg::optional(
            "waiting",
            "text",
            "\"approval\" (the default) or \"reimbursement\"",
        )],
        answers: &[
            "which expenses are waiting for approval",
            "what claims are open",
            "who is still owed a reimbursement",
        ],
        preview: None,
        undo: None,
        routes: &[
            "/finance/expenses/pending",
            "/finance/expenses/reimbursable",
        ],
    },
    IntentSpec {
        name: "account_balance",
        purpose: "One account of the tenant's chart, by its code or its name — what it is, and its balance: the whole journal's to date, or its movement over a stated period.",
        effect: Effect::Read,
        args: &[
            Arg::required(
                "account",
                "text",
                "the account's code or name, exactly as the chart shows it",
            ),
            Arg::optional(
                "from",
                "date",
                "start of the period, YYYY-MM-DD; leave out for the balance to date",
            ),
            TO_OPT,
        ],
        answers: &[
            "what is the balance of account 6100",
            "how much is on the bank account",
            "what did we spend on marketing this year",
        ],
        preview: None,
        undo: None,
        routes: &["/finance/accounts", "/finance/accounts/{id}"],
    },
    // ---- writes: propose, then the asker approves ------------------------
    IntentSpec {
        name: "categorise_transactions",
        purpose: "Go through the user's OWN expense claims that have no category yet and SUGGEST one for each, from the categories they have already used for the same merchant — the last three months unless another period is named. Every suggestion is SAVED AS A SUGGESTION on its claim for the user to accept or decline: nothing is classified, booked or reported until they accept it, one claim at a time. You do NOT choose the categories and there is no argument for one; a claim from a merchant they have never classified gets no suggestion rather than a guess.",
        effect: Effect::Write,
        args: &[
            Arg::optional(
                "from",
                "date",
                "the first day of the period of claims to look at, YYYY-MM-DD; default: three months back",
            ),
            TO_OPT,
        ],
        answers: &[
            "sort out my expenses",
            "categorise my claims",
            "tidy up my expenses",
        ],
        preview: Some(
            "Categories will be suggested for your unclassified expense claims — each saved as a suggestion for you to accept, nothing booked.",
        ),
        undo: None,
        // What it writes is a suggestion on the claimant's own claims; no
        // /finance route runs the suggesting, so this verb stands behind none.
        routes: &[],
    },
    IntentSpec {
        name: "approve_expense",
        purpose: "Approve ONE expense claim from the approvals queue — the cost becomes the company's, and when the claimant's own money paid, so does the debt to them. Name the claim by its merchant exactly as expenses_awaiting shows it; never approve a claim you have not seen in that queue.",
        effect: Effect::Write,
        args: &[
            Arg::required(
                "merchant",
                "text",
                "the merchant on the waiting claim, exactly as the queue shows it",
            ),
            Arg::optional(
                "claimant",
                "text",
                "the claimant's email, when two people have a claim from the same merchant",
            ),
            Arg::optional(
                "spentOn",
                "date",
                "the day of the purchase, YYYY-MM-DD, when one person has two claims from the same merchant",
            ),
            Arg::optional("note", "text", "one sentence for the claimant"),
        ],
        answers: &[
            "approve the claim from {claimant}",
            "approve that expense",
            "sign off the travel claim",
        ],
        preview: Some("The expense claim from \"{merchant}\" will be approved."),
        undo: None,
        routes: &["/finance/expenses/{id}/approve"],
    },
    IntentSpec {
        name: "post_missing_documents",
        purpose: "Put into the books the invoices and credit notes that were already issued but were never posted to the journal — each at its own issue date, by the same rules an issue made today uses, together with the payments recorded against them. Documents raised before the books began recording documents are why Finance can report less than Billing for the same period; ledger_summary says how many there are. It is safe to run twice: a document already in the books is left alone. It invents nothing — a document in a closed period, or one with no usable exchange rate, is refused and named rather than booked. There is NO argument for an amount, an account or a date to book on.",
        effect: Effect::Write,
        args: &[
            Arg::optional(
                "from",
                "date",
                "only documents issued on or after this day, YYYY-MM-DD; default: however far back the records go",
            ),
            TO_OPT,
        ],
        answers: &[
            "are all our invoices in the books",
            "the books show less than the invoices we raised",
            "put the documents that are missing from the books into them",
        ],
        preview: Some(
            "Every issued document the books do not hold will be posted to the journal at its own date, with the payments recorded against it. Nothing already in the books changes.",
        ),
        // Un-booking a journal entry is not a verb this product has, and it
        // should not be: a correction to a posted document is a reversal an
        // accountant writes, not an undo of a repair.
        undo: None,
        // The repair walks the documents and calls the store's posting doors;
        // no /finance route serves it, so this verb stands behind none.
        routes: &[],
    },
];

/// The Finance routes deliberately without a verb, each with its reason.
pub const FINANCE_EXCLUDED: &[Excluded] = &[
    Excluded {
        route: "/finance/expenses",
        why: "A claim is typed by the person who spent the money, receipt in hand, in the app.",
    },
    Excluded {
        route: "/finance/expenses/{id}",
        why: "Editing or deleting a claim is its claimant's own act in the app.",
    },
    Excluded {
        route: "/finance/expenses/{id}/submit",
        why: "Handing a claim in is its claimant's own act in the app.",
    },
    Excluded {
        route: "/finance/expenses/{id}/withdraw",
        why: "Taking a claim back is its claimant's own act in the app.",
    },
    Excluded {
        route: "/finance/expenses/{id}/category/accept",
        why: "Accepting a suggested category is the claimant's decision, one claim at a time, in the app.",
    },
    Excluded {
        route: "/finance/expenses/{id}/category/decline",
        why: "Declining a suggested category is the claimant's decision, one claim at a time, in the app.",
    },
    Excluded {
        route: "/finance/expenses/{id}/reject",
        why: "Turning a claim back deserves the approver's own words in the app; the agent only approves.",
    },
    Excluded {
        route: "/finance/expenses/{id}/reimburse",
        why: "Recording a repayment books money out of the bank; the payer does it in the app, payment date in hand.",
    },
    Excluded {
        route: "/finance/receipts",
        why: "Takes a file.",
    },
    Excluded {
        route: "/finance/mileage/rates",
        why: "The tenant's mileage rates are configuration, set by a person in the app.",
    },
    Excluded {
        route: "/finance/mileage",
        why: "A mileage claim is typed by the person who drove, in the app.",
    },
    Excluded {
        route: "/finance/mileage/{id}",
        why: "Deleting a mileage claim is its claimant's own act in the app.",
    },
    Excluded {
        route: "/finance/imports/bank/preview",
        why: "Takes a file.",
    },
    Excluded {
        route: "/finance/imports/bank",
        why: "Takes a file.",
    },
    Excluded {
        route: "/finance/bank/statements",
        why: "The list of imports; the lines are the work, and unmatched_bank_lines reads them.",
    },
    Excluded {
        route: "/finance/bank/suggestions",
        why: "Reconciliation is decided line by line in the app; a later intent set.",
    },
    Excluded {
        route: "/finance/bank/lines/{id}/match",
        why: "Matching money to a document changes the books; a person's decision in the app, a later intent set.",
    },
    Excluded {
        route: "/finance/bank/lines/{id}/unmatch",
        why: "Unmatching changes the books; a person's decision in the app, a later intent set.",
    },
    Excluded {
        route: "/finance/bank/lines/{id}/ignore",
        why: "Setting a bank line aside is a person's decision in the app.",
    },
    Excluded {
        route: "/finance/bank/lines/{id}/unignore",
        why: "Bringing a bank line back is a person's decision in the app.",
    },
    Excluded {
        route: "/finance/periods",
        why: "Accounting periods are made by the accountant in the app.",
    },
    Excluded {
        route: "/finance/periods/{id}/close",
        why: "Closing a period locks the books; the accountant's own act in the app.",
    },
    Excluded {
        route: "/finance/periods/{id}/reopen",
        why: "Reopening a period unlocks the books; the accountant's own act in the app.",
    },
    Excluded {
        route: "/finance/reports/pl",
        why: "The full statement is a screen in the app; a later intent set.",
    },
    Excluded {
        route: "/finance/reports/pl.csv",
        why: "Produces a file.",
    },
    Excluded {
        route: "/finance/reports/balance",
        why: "The full statement is a screen in the app; a later intent set.",
    },
    Excluded {
        route: "/finance/reports/balance.csv",
        why: "Produces a file.",
    },
    Excluded {
        route: "/finance/reports/aged",
        why: "The full ageing is a screen in the app; ledger_summary answers what is outstanding.",
    },
    Excluded {
        route: "/finance/reports/aged.csv",
        why: "Produces a file.",
    },
    Excluded {
        route: "/finance/reports/vat.csv",
        why: "Produces a file; vat_summary answers the question.",
    },
];

/// The Finance paragraph of the agent's general instructions.
pub const FINANCE_GUIDANCE: &str = "For a Finance verb, NEVER invent a category, an account or an amount: the categories are the tenant's own words in their own language, the figures are in their books, and anything you compose yourself would be a number in somebody's accounts that nobody can trace. Resolve a relative period (this month, last quarter) against today's date below into plain YYYY-MM-DD days. To answer a question about the books, USE a reading verb first and answer from what it returned, quoting amounts as returned (amounts are integer cents: 500000 is 5000.00) and adding none it did not return. A suggestion categorise_transactions writes is not a decision: never tell the user something has been classified, booked or filed — say it is waiting for them. What flag_anomalies finds names entries and never a person: report it as something worth looking at rather than as a verdict about anybody, and never propose it for a question about somebody's spending. When a reading's \"documents\" block says issued documents are not in the books, NEVER report the journal's figure as the whole truth: say the books' figure, then say how many documents are missing and what they come to, and offer post_missing_documents — a Finance figure that is quietly short is how one agent comes to contradict another about the same period.\n";

/// The module, as the registry reads it.
pub static FINANCE: IntentModule = IntentModule {
    intents: FINANCE_INTENTS,
    excluded: FINANCE_EXCLUDED,
    guidance: FINANCE_GUIDANCE,
};

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    /// The verbs whose figures are the journal's or the store's own reading,
    /// with no `/finance` route serving that view yet — the named exceptions
    /// to "every verb stands behind a route".
    const ROUTELESS: &[&str] = &[
        "ledger_summary",
        "flag_anomalies",
        "categorise_transactions",
        "post_missing_documents",
    ];

    #[test]
    fn every_verb_has_a_route_a_purpose_and_a_question_it_answers() {
        for intent in FINANCE_INTENTS {
            assert!(
                !intent.routes.is_empty() || ROUTELESS.contains(&intent.name),
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
        let mut names: Vec<&str> = FINANCE_INTENTS.iter().map(|i| i.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), FINANCE_INTENTS.len());
        let doc = FINANCE.doc();
        for intent in FINANCE_INTENTS {
            assert_eq!(doc.matches(&format!("- {}:", intent.name)).count(), 1);
        }
        assert!(FINANCE_GUIDANCE.ends_with('\n'));
    }

    #[test]
    fn an_exclusion_is_a_sentence_not_a_shrug() {
        for excluded in FINANCE_EXCLUDED {
            assert!(
                excluded.why.ends_with('.'),
                "{}: {}",
                excluded.route,
                excluded.why
            );
            assert!(
                !FINANCE_INTENTS
                    .iter()
                    .any(|i| i.routes.contains(&excluded.route)),
                "{} is both a verb's route and excluded",
                excluded.route
            );
        }
    }

    #[test]
    fn the_model_is_never_offered_a_category_to_choose() {
        // The one mistake this module can make that nothing downstream
        // catches: a category the model made up, arriving as an argument.
        let doc = FINANCE.doc();
        assert!(!doc.contains("\"category\""));
        assert!(doc.contains("there is no argument for one"));
        assert!(FINANCE_GUIDANCE.contains("NEVER invent a category"));
        // …and the wording says a suggestion is not a classification.
        assert!(doc.contains("SAVED AS A SUGGESTION"));
        assert!(doc.contains("nothing is classified"));
        assert!(FINANCE_GUIDANCE.contains("is not a decision"));
    }

    #[test]
    fn the_vat_period_has_no_default_to_fall_back_on() {
        let vat = FINANCE.find("vat_summary").expect("vat_summary is a verb");
        for arg in vat.args {
            assert!(arg.required, "{} must be required", arg.name);
        }
        assert!(vat.purpose.contains("REQUIRED"));
        assert!(vat.purpose.contains("files nothing with any tax authority"));
    }

    #[test]
    fn the_anomaly_verb_is_told_it_cannot_answer_about_a_person() {
        let scan = FINANCE
            .find("flag_anomalies")
            .expect("flag_anomalies is a verb");
        assert!(
            scan.purpose
                .contains("names the entries behind it and never a person")
        );
        assert!(
            scan.purpose
                .contains("marks nothing as reviewed and accuses nobody")
        );
        // And no word that turns a question into a verdict.
        let doc = FINANCE.doc();
        for forbidden in ["fraud", "risk", "score", "suspicious"] {
            assert!(!doc.contains(forbidden), "{forbidden}");
        }
    }

    #[test]
    fn nothing_finance_offers_posts_pays_rejects_or_deletes() {
        for forbidden in [
            "post_entry",
            "pay_invoice",
            "delete_expense",
            "reject_expense",
            "reimburse_expense",
            "close_period",
        ] {
            assert!(FINANCE.find(forbidden).is_none());
            assert!(!FINANCE.doc().contains(forbidden));
        }
    }
}
