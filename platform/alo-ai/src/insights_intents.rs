//! alo Insights' verbs (ADR 0058, AC.3) — the figures, what moved, the boards
//! they are pinned to, and nothing that could quietly lie.
//!
//! This is the whole of what the Insights agent may do, and the words a model
//! reads about it. The executors live beside Insights' routes in `alo-jmap`
//! (`insights_intents.rs`), through the asker's tenant-scoped store and the
//! same query engine (`alo_store::insight_query`) the boards themselves draw
//! from.
//!
//! The four rules the old tool set was written around hold unchanged — each is
//! a way a figure could otherwise quietly lie:
//!
//! - **The model fills in a form; it never writes a query.** A question about
//!   the numbers is a chart specification over a closed catalog, and every
//!   word in one is an enum variant the server validates. That is why
//!   `insight_catalog` comes first: the vocabulary is looked up, never
//!   remembered, so an agent cannot name a measure this build does not have.
//! - **A figure is repeated, never recomputed.** Every value that comes back
//!   is a whole number in a declared unit — cents in a named currency, a
//!   count, a rate in basis points — and the guidance says to report it as it
//!   arrived.
//! - **A change is what moved, not why.** `insight_change` answers with the
//!   figure before, the figure now and the difference, biggest movement
//!   first; the reason is not in the numbers.
//! - **A board waits for a tap.** `insight_report` builds a board somebody's
//!   colleagues will read and `pin_chart` adds to one, so both are writes
//!   (ADR 0047 §1) and the only path that runs either is an approval the
//!   asker themselves gave.
//!
//! What AC.3 adds is the boards as a *subject*: `dashboard_tiles` reads what
//! is already pinned — the boards by name and the question each tile asks —
//! and `pin_chart` puts one more answered question on a board that exists,
//! where `insight_report` builds a whole new one.

use crate::agent_tool::Effect;
use crate::intent::{Arg, Excluded, IntentModule, IntentSpec};

const SPEC_REQ: Arg = Arg::required(
    "spec",
    "object",
    "a chart specification in the shape insight_catalog describes — every name in it comes from that closed list",
);

/// The verbs.
pub const INSIGHTS_INTENTS: &[IntentSpec] = &[
    // ---- reads ---------------------------------------------------------
    IntentSpec {
        name: "insight_catalog",
        purpose: "What this workspace can measure — its datasets, the measures each one offers, the breakdowns and filters each measure allows, and the shape a chart specification takes. It changes nothing and takes no arguments. Call it FIRST, before you name a dataset, a measure, a breakdown or a filter anywhere else: those names are a closed list, and one that is not on it is refused rather than guessed at.",
        effect: Effect::Read,
        args: &[],
        answers: &[
            "what can you measure",
            "what figures do we have",
            "what kinds of charts can I ask for",
        ],
        preview: None,
        undo: None,
        // The vocabulary is rendered from the catalog enums themselves — no
        // `/insights/` route serves it, which is why this verb adapts none.
        routes: &[],
    },
    IntentSpec {
        name: "insight_answer",
        purpose: "The figures ONE chart specification asks for, evaluated against this company's own records — the buckets and their values, each a whole number in a declared unit. It reads; it changes nothing. Prefer the smallest specification that answers the question: no breakdown at all for a single figure, and no filters, sort or limit unless the question asks for them.",
        effect: Effect::Read,
        args: &[SPEC_REQ],
        answers: &[
            "how much have we billed this year",
            "revenue by month",
            "how many deals are open",
        ],
        preview: None,
        undo: None,
        routes: &["/insights/eval"],
    },
    IntentSpec {
        name: "insight_change",
        purpose: "The same specification evaluated over two periods, and what moved — biggest movement first, each with the figure before, the figure now and the difference. It reads; it changes nothing. The specification must break down by a category or not at all: a date breakdown has different buckets in each period and nothing to compare, and is refused.",
        effect: Effect::Read,
        args: &[
            Arg::required(
                "spec",
                "object",
                "the chart specification, carrying the LATER period",
            ),
            Arg::required(
                "against",
                "object",
                "the earlier period on its own, in the same shape as a specification's period",
            ),
        ],
        answers: &[
            "what changed since last quarter",
            "which customers moved the most",
            "how does this month compare to May",
        ],
        preview: None,
        undo: None,
        // Two evaluations of the route insight_answer already adapts — this
        // verb adds no route of its own.
        routes: &[],
    },
    IntentSpec {
        name: "dashboard_tiles",
        purpose: "The boards this workspace already has, each with its pinned charts as a list — the tile's caption and the question it asks (dataset, measure, period), read from the stored board. It changes nothing. Name a board to read just that one; a workspace that has never opened Insights honestly has no boards yet.",
        effect: Effect::Read,
        args: &[Arg::optional(
            "board",
            "text",
            "one board's name, as its tab shows it — leave it out to list every board",
        )],
        answers: &[
            "what is on the dashboard",
            "what charts are on the sales board",
            "what reports do we have",
        ],
        preview: None,
        undo: None,
        routes: &["/insights/dashboards", "/insights/dashboards/{id}"],
    },
    // ---- writes: propose, then the asker approves ------------------------
    IntentSpec {
        name: "insight_report",
        purpose: "Propose a report — a NEW named board of saved charts the user can open in Insights. It only proposes: no board and no chart is saved until the user approves it. Every chart is validated and answered before anything is saved, so one that cannot be answered refuses the whole report by name rather than pinning a broken tile. Say what the report will contain before you propose it; to add one chart to a board that already exists, use pin_chart instead.",
        effect: Effect::Write,
        args: &[
            Arg::required(
                "name",
                "text",
                "what the report is called, in the user's own language",
            ),
            Arg::required(
                "charts",
                "array",
                "each {\"title\": text (the caption on that chart), \"spec\": object (a chart specification)}, 1 to 8",
            ),
        ],
        answers: &[
            "build me a revenue report",
            "make a board of our key numbers",
            "put together a sales overview",
        ],
        preview: Some(
            "A new board called {name} will be created in Insights, its charts answered before anything is saved.",
        ),
        undo: None,
        routes: &["/insights/dashboards"],
    },
    IntentSpec {
        name: "pin_chart",
        purpose: "Pin ONE more chart to a board that already exists, named the way its tab shows it. It only proposes: nothing is pinned until the user approves. The chart is validated and answered first, so an approved pin cannot leave a broken tile on a board colleagues read; a board nobody has made yet is a refusal that names the boards there are — building a new one is insight_report's job.",
        effect: Effect::Write,
        args: &[
            Arg::required(
                "board",
                "text",
                "the board's name, as its tab shows it — there is no identifier for a board that you could know",
            ),
            Arg::required("title", "text", "the caption on the new chart"),
            SPEC_REQ,
        ],
        answers: &[
            "pin that to the dashboard",
            "add this chart to the sales board",
            "put revenue by month on the overview",
        ],
        preview: Some("{title} will be pinned to the {board} board."),
        undo: None,
        routes: &["/insights/dashboards/{id}/tiles"],
    },
];

/// The Insights routes deliberately without a verb, each with its reason.
pub const INSIGHTS_EXCLUDED: &[Excluded] = &[
    Excluded {
        route: "/insights/tiles/{id}",
        why: "Retitling, respecifying or deleting a pinned chart changes a board colleagues already read; that is done on the board, where the person sees the chart they are changing — the agent pins new questions and removes nothing.",
    },
    Excluded {
        route: "/insights/tiles/{id}/move",
        why: "Arranging a board is a drag the person does while looking at it; a layout is visual and not a sentence an agent should write.",
    },
    Excluded {
        route: "/insights/tiles/{id}/data",
        why: "Serves the screen's tiles their figures as they render; the agent asks its own question through insight_answer.",
    },
    Excluded {
        route: "/insights/gallery",
        why: "The gallery of prebuilt boards is the screen's own picker; the agent builds from the catalog, chart by chart, so what it proposes is what was asked for.",
    },
    Excluded {
        route: "/insights/ask",
        why: "The screen's own ask-a-chart box, which runs its own model turn; the agent already is a conversation, and its questions go through insight_answer.",
    },
];

/// The Insights paragraph of the agent's general instructions.
///
/// It says what the *product* is, not what a tool takes: it is the sentence
/// that stops a model reporting a figure it worked out itself, and the one
/// that stops a change coming with an invented cause.
pub const INSIGHTS_GUIDANCE: &str = "You answer from this company's own figures and from nothing else. ALWAYS look the vocabulary up with insight_catalog before naming a dataset, a measure, a breakdown or a filter — they are a closed list, and a name that is not on it is refused. Say which measure, which dataset and which period every figure came from, because a number without its question is not an answer. Figures arrive as whole numbers — cents in a named currency, a count, or a rate in basis points — and you repeat them exactly as they arrived: never add them up, scale them, convert a currency or work out a percentage yourself, because a figure nobody can check against the books is worse than no figure. When an answer comes back in more than one currency, keep them apart; they were kept apart for a reason. For how much was billed, measure net on billing.documents; for money that actually arrived, amount on billing.payments; for money still owed, outstanding on billing.receivables. Say what moved and by how much, and never offer a REASON the figures do not carry — the numbers say what changed, not why. Name a board the way its tab does and never invent an identifier for one. When a question is about something the catalog does not hold, say so plainly rather than answering from anything else. Both writes wait for the asker's approval: never say a report exists or a chart has been pinned until they approve.\n";

/// The module, as the registry reads it.
pub static INSIGHTS: IntentModule = IntentModule {
    intents: INSIGHTS_INTENTS,
    excluded: INSIGHTS_EXCLUDED,
    guidance: INSIGHTS_GUIDANCE,
};

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// The verbs whose surface is not a route of this module — the catalog is
    /// rendered from the product's own enums, and a change is two evaluations
    /// of the route insight_answer already adapts. Named, so a new verb with
    /// an empty route list fails the test instead of joining them silently.
    const ROUTELESS: &[&str] = &["insight_catalog", "insight_change"];

    #[test]
    fn every_verb_has_a_route_a_purpose_and_a_question_it_answers() {
        for intent in INSIGHTS_INTENTS {
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
        let mut names: Vec<&str> = INSIGHTS_INTENTS.iter().map(|i| i.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), INSIGHTS_INTENTS.len());
        let doc = INSIGHTS.doc();
        for intent in INSIGHTS_INTENTS {
            assert_eq!(doc.matches(&format!("- {}:", intent.name)).count(), 1);
        }
        assert!(INSIGHTS_GUIDANCE.ends_with('\n'));
    }

    #[test]
    fn an_exclusion_is_a_sentence_not_a_shrug() {
        for excluded in INSIGHTS_EXCLUDED {
            assert!(
                excluded.why.ends_with('.'),
                "{}: {}",
                excluded.route,
                excluded.why
            );
            assert!(
                !INSIGHTS_INTENTS
                    .iter()
                    .any(|i| i.routes.contains(&excluded.route)),
                "{} is both a verb's route and excluded",
                excluded.route
            );
        }
    }

    /// Every read says, where the model reads it, that it changes nothing — so
    /// a turn never offers a button for an answer.
    #[test]
    fn the_reads_say_they_change_nothing() {
        for intent in INSIGHTS_INTENTS.iter().filter(|i| i.effect == Effect::Read) {
            assert!(
                intent.purpose.contains("changes nothing"),
                "{} does not say it changes nothing",
                intent.name
            );
        }
    }

    /// The rule the whole module rests on: the vocabulary is looked up, never
    /// remembered. A model naming a measure from memory is how a question gets
    /// answered with a chart of something else — and the catalog itself is
    /// never copied into the prompt, because a second copy of the vocabulary
    /// is the one thing that could name a measure the validator refuses.
    #[test]
    fn the_vocabulary_is_looked_up_rather_than_remembered() {
        assert!(INSIGHTS_GUIDANCE.contains("ALWAYS look the vocabulary up with insight_catalog"));
        let catalog = INSIGHTS.find("insight_catalog").unwrap();
        assert!(catalog.purpose.contains("Call it FIRST"));
        assert!(catalog.purpose.contains("closed list"));
        assert!(!INSIGHTS.doc().contains("billing.documents"));
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
        let change = INSIGHTS.find("insight_change").unwrap();
        assert!(change.purpose.contains("biggest movement first"));
        assert!(change.purpose.contains("nothing to compare"));
        assert!(INSIGHTS_GUIDANCE.contains("never offer a REASON the figures do not carry"));
    }

    /// The two writes write **questions** — a board, a tile — and wait for the
    /// asker's tap; nothing in this set can touch a figure, a document or a
    /// book the figures are read from.
    #[test]
    fn the_writes_pin_questions_and_wait_for_approval() {
        let report = INSIGHTS.find("insight_report").unwrap();
        assert_eq!(report.effect, Effect::Write);
        assert!(report.purpose.contains("It only proposes"));
        assert!(report.purpose.contains("validated and answered before"));
        let pin = INSIGHTS.find("pin_chart").unwrap();
        assert_eq!(pin.effect, Effect::Write);
        assert!(pin.purpose.contains("board that already exists"));
        assert!(pin.purpose.contains("validated and answered first"));
        assert!(INSIGHTS_GUIDANCE.contains("never say a report exists or a chart has been pinned"));
        for intent in INSIGHTS_INTENTS {
            assert!(
                !intent.name.contains("delete") && !intent.name.contains("edit"),
                "{} would let a report change what it reports on",
                intent.name
            );
        }
    }

    /// A board is named the way a meeting is: in the user's words, never by an
    /// identifier the model could not know.
    #[test]
    fn a_board_is_named_and_never_identified() {
        let pin = INSIGHTS.find("pin_chart").unwrap();
        assert!(
            pin.args
                .iter()
                .any(|arg| arg.name == "board" && arg.purpose.contains("no identifier")),
            "pin_chart does not take the board's name in the user's own words"
        );
        assert!(INSIGHTS_GUIDANCE.contains("never invent an identifier"));
    }
}
