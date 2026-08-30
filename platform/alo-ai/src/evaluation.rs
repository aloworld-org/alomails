//! The evaluation set (ADR 0057, A4.7) — every question the agents are
//! expected to answer, derived from the registry rather than kept by hand.
//!
//! The 2026-08-28 run asked seventeen questions and six agents said "I could
//! not find it" to questions their own records answer. That run is the
//! standing baseline ([`STANDING`]); the set *grows* from every intent's
//! [`answers`](crate::intent::IntentSpec::answers) — the module's own account
//! of which questions each verb is the answer to — so a verb cannot be added
//! without adding the question that proves it ([`evaluation_set`]).
//!
//! Two runs read this set:
//!
//! - the **scripted run** (`alo-jmap`'s `agent_evaluation_http` suite), which
//!   drives every case through a real room, router and store with the
//!   scripted model — the wave's exit gate, answers recorded verbatim;
//! - the **real-model run**, which the owner performs with the tenant's own
//!   provider after each wave (never from a test — no live model is ever
//!   called by a suite).

use alo_store::{AgentProduct, default_handle};
use serde_json::{Map, Value};

use crate::agent_product::MOVED;
use crate::agent_tool::Effect;
use crate::intent::IntentSpec;

/// One question of the evaluation: who is asked, what, and which verb the
/// registry says is the answer.
#[derive(Debug, Clone, Copy)]
pub struct EvalCase {
    /// The product whose agent is asked.
    pub product: AgentProduct,
    /// The handle it is asked by (`@billing …`).
    pub handle: &'static str,
    /// The verb the registry declares as this question's answer.
    pub verb: &'static str,
    /// Reads answer, writes propose — what the run must see happen.
    pub effect: Effect,
    /// The question, in the intent's own words.
    pub ask: &'static str,
}

/// The grown set: every moved module, every intent, every `answers` entry —
/// in registry order, so the run and the transcript are stable across builds.
#[must_use]
pub fn evaluation_set() -> Vec<EvalCase> {
    MOVED
        .iter()
        .flat_map(|(product, module)| {
            module.intents.iter().flat_map(|intent| {
                intent.answers.iter().map(|ask| EvalCase {
                    product: *product,
                    handle: default_handle(*product),
                    verb: intent.name,
                    effect: intent.effect,
                    ask,
                })
            })
        })
        .collect()
}

/// The seventeen questions of 2026-08-28 (`docs/autonomy/agents/STATE.md`),
/// verbatim — the run that found six agents unable to answer from their own
/// records, kept as the regression baseline the real-model run always asks
/// first. `alo` is Ask alo, [`AgentProduct::Workspace`].
pub const STANDING: &[(&str, &str)] = &[
    (
        "billing",
        "which quotes are open right now, and what are they worth?",
    ),
    (
        "mail",
        "are we in contact with anyone at axongroup.com? Who wrote last?",
    ),
    ("agenda", "what is in my diary this week?"),
    ("tasks", "what is on my plate, and is anything overdue?"),
    ("chat", "what has been said in this room so far?"),
    ("drive", "which files do we have in Drive?"),
    ("sheets", "which spreadsheets exist, and what is in them?"),
    ("docs", "which documents exist?"),
    ("crm", "which deals are open, and at what stage?"),
    ("projects", "which projects are active?"),
    (
        "finance",
        "how much have we invoiced this year, and how much is unpaid?",
    ),
    ("inventory", "is Managed hosting in stock?"),
    ("hr", "who is on leave this month?"),
    ("insights", "how much did we invoice in August 2026?"),
    (
        "meet",
        "were any meetings recorded recently, and what was decided?",
    ),
    ("sites", "do we have a website, and is it published?"),
    (
        "alo",
        "what did we quote Northstar Foods, and has it been sent?",
    ),
];

/// The required arguments of an intent, each filled with a placeholder of its
/// declared kind — what the scripted run hands a verb so the executor is
/// reached with arguments *shaped* right, on a workspace that holds no
/// records for them to name.
///
/// Optional arguments are left out on purpose: the run exercises each verb's
/// defaults, not its widest call.
#[must_use]
pub fn placeholder_args(spec: &IntentSpec) -> Value {
    let mut args = Map::new();
    for arg in spec.args.iter().filter(|arg| arg.required) {
        if let Some(value) = placeholder(arg.kind) {
            args.insert(arg.name.to_owned(), value);
        }
    }
    Value::Object(args)
}

/// A value of the declared kind, or `None` for a kind word the registry does
/// not use — refused in a test rather than repaired, so a new kind word is a
/// visible decision here and not a silently unfillable argument.
#[must_use]
fn placeholder(kind: &str) -> Option<Value> {
    match kind {
        "text" => Some(Value::from("Northstar")),
        "date" => Some(Value::from("2026-08-28")),
        "integer" | "number" => Some(Value::from(1)),
        "boolean" => Some(Value::from(true)),
        "array" => Some(Value::from(vec!["Northstar"])),
        "object" => Some(Value::Object(Map::new())),
        _ => None,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use alo_store::ALL_AGENT_PRODUCTS;

    use crate::agent_product::offers;

    /// The property A4.7 exists for: every verb of every moved module is in
    /// the set with at least one question, because an intent with an empty
    /// `answers` would be a capability the evaluation never asks about.
    #[test]
    fn every_verb_contributes_at_least_one_question() {
        let set = evaluation_set();
        for (_, module) in MOVED {
            for intent in module.intents {
                assert!(
                    !intent.answers.is_empty(),
                    "{} declares no question it answers",
                    intent.name
                );
                assert!(
                    set.iter().any(|case| case.verb == intent.name),
                    "{} is not in the evaluation set",
                    intent.name
                );
            }
        }
        // Grown, never trimmed: one case per `answers` entry, in registry
        // order. The census is pinned like the registry's own (137 tools), so
        // a shrink is a visible change to this line and not a quiet loss of
        // coverage.
        let entries: usize = MOVED
            .iter()
            .flat_map(|(_, module)| module.intents.iter())
            .map(|intent| intent.answers.len())
            .sum();
        assert_eq!(set.len(), entries);
        assert_eq!(set.len(), 372, "the evaluation set census moved");
    }

    /// A case is askable as written: its ask has words, its handle is its
    /// product's, and the product it addresses actually offers its verb —
    /// the registry link that turns a question into a tool run.
    #[test]
    fn every_case_is_asked_of_an_agent_that_offers_its_verb() {
        for case in evaluation_set() {
            assert!(!case.ask.trim().is_empty(), "{} asks nothing", case.verb);
            assert_eq!(case.handle, default_handle(case.product));
            assert!(
                offers(case.product, case.verb),
                "{} is asked of {}, which does not offer it",
                case.verb,
                case.handle
            );
        }
    }

    /// Every argument of every intent can be filled: a kind word
    /// [`placeholder`] does not know would leave a required argument silently
    /// missing from the scripted run's calls.
    #[test]
    fn a_placeholder_exists_for_every_declared_kind() {
        for (_, module) in MOVED {
            for intent in module.intents {
                for arg in intent.args {
                    assert!(
                        placeholder(arg.kind).is_some(),
                        "{}'s {} has kind {:?}, which has no placeholder",
                        intent.name,
                        arg.name,
                        arg.kind
                    );
                }
                let filled = placeholder_args(intent);
                for arg in intent.args.iter().filter(|arg| arg.required) {
                    assert!(
                        filled.get(arg.name).is_some(),
                        "{}'s required {} was not filled",
                        intent.name,
                        arg.name
                    );
                }
            }
        }
        assert!(placeholder("blob").is_none(), "unknown kinds are refused");
    }

    /// The baseline is intact: seventeen questions, each addressed to a
    /// handle some product actually seeds — so the owner's real-model run
    /// can always start from the same seventeen the 2026-08-28 finding is
    /// written in.
    #[test]
    fn the_standing_seventeen_address_real_handles() {
        assert_eq!(STANDING.len(), 17);
        for (handle, ask) in STANDING {
            assert!(
                ALL_AGENT_PRODUCTS
                    .iter()
                    .any(|product| default_handle(*product) == *handle),
                "@{handle} is nobody's default handle"
            );
            assert!(!ask.trim().is_empty());
        }
        // The six that said "I could not find it" now have a read whose
        // question the set carries — the reason wave A4 was built. One
        // representative assertion per formerly-silent agent.
        let set = evaluation_set();
        for verb in [
            "open_quotes",     // billing: no read existed at all
            "recent_files",    // drive: beyond find_file
            "list_documents",  // docs
            "open_deals",      // crm
            "active_projects", // projects
            "ledger_summary",  // finance
        ] {
            assert!(
                set.iter()
                    .any(|case| case.verb == verb && matches!(case.effect, Effect::Read)),
                "{verb} has no read question in the set"
            );
        }
    }
}
