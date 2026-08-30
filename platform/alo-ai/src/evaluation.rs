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
//! **A question names no record** (A10.3). The 2026-08-30 real-model run could
//! not score 41 of its own asks, because the registry wrote them as templates
//! with the subject spelled out — "what did we quote X", "what is Ben's
//! address". An agent answers those honestly ("I could not find a customer
//! called X") and the verb is proved by nothing. So a subject is now a `{arg}`
//! hole over the intent's own argument, every verb owes the set one question
//! with no hole in it at all ([`EvalCase::askable`], [`NEEDS_A_SUBJECT`]), and
//! a run fills what holes remain ([`EvalCase::asked`]) rather than asking
//! after a record nobody has.
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

use crate::agent_product::{MOVED, intent_spec};
use crate::agent_tool::Effect;
use crate::intent::{IntentSpec, holes, render_preview};

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
    /// The question, in the intent's own words — with `{arg}` holes where the
    /// asker has to supply a record ([`subjects`](Self::subjects)).
    pub ask: &'static str,
}

impl EvalCase {
    /// The arguments this question makes the asker supply — its `{arg}` holes,
    /// empty for a question a run puts word for word.
    #[must_use]
    pub fn subjects(&self) -> Vec<&'static str> {
        holes(self.ask)
    }

    /// Whether a run can put this question the only way the verb can be
    /// asked: no holes, or holes over arguments the verb cannot run without.
    /// A hole over an *optional* argument is a narrowing the asker could have
    /// left out — so the intent owes the evaluation the unnarrowed question
    /// too, which is what [`a question a run can put`] holds it to.
    ///
    /// [`a question a run can put`]: NEEDS_A_SUBJECT
    #[must_use]
    pub fn askable(&self) -> bool {
        let Some(spec) = intent_spec(self.verb) else {
            return self.subjects().is_empty();
        };
        self.subjects().iter().all(|hole| {
            spec.args
                .iter()
                .any(|arg| arg.name == *hole && arg.required)
        })
    }

    /// The question as a run puts it: every hole filled with a placeholder of
    /// its argument's kind, the same values [`placeholder_args`] hands the
    /// verb — so the question and the call name the same thing. The owner's
    /// real-model run substitutes records its own tenant holds instead.
    #[must_use]
    pub fn asked(&self) -> String {
        match intent_spec(self.verb) {
            Some(spec) => render_preview(self.ask, &every_placeholder(spec)),
            None => self.ask.to_owned(),
        }
    }
}

/// The verbs that cannot be asked without naming a record, with the reason:
/// each is a lookup of ONE record whose identifying arguments are all optional
/// only because *either* of them identifies it (a quote by its number or by
/// its customer). There is no unnarrowed question for them — "show me the
/// invoice" names nothing to show — so they are the declared exception to
/// [`EvalCase::askable`], listed here rather than left as a silent gap.
pub const NEEDS_A_SUBJECT: &[&str] = &["quote_lookup", "invoice_lookup", "deal_lookup"];

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

/// One question per verb of `product`, in registry order — the
/// [`askable`](EvalCase::askable) one where the verb has one, else its first,
/// which by [`NEEDS_A_SUBJECT`] is a lookup whose subject the run fills.
///
/// This is what the scripted run walks: a verb is asked once, and it is asked
/// the question it can actually be scored on rather than whichever question
/// the module happened to list first.
#[must_use]
pub fn one_per_verb(product: AgentProduct) -> Vec<EvalCase> {
    let mut chosen: Vec<EvalCase> = Vec::new();
    for case in evaluation_set()
        .into_iter()
        .filter(|case| case.product == product)
    {
        match chosen.iter_mut().find(|seen| seen.verb == case.verb) {
            Some(seen) => {
                if !seen.askable() && case.askable() {
                    *seen = case;
                }
            }
            None => chosen.push(case),
        }
    }
    chosen
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

/// A placeholder for **every** argument, required or not — what fills the
/// `{arg}` holes of a question ([`EvalCase::asked`]). The call itself still
/// carries [`placeholder_args`]' required-only object: a question may name the
/// customer an optional argument narrows to, without the run narrowing the
/// call.
#[must_use]
fn every_placeholder(spec: &IntentSpec) -> Value {
    let mut args = Map::new();
    for arg in spec.args {
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
        assert_eq!(set.len(), 373, "the evaluation set census moved");
    }

    /// A10.3, the rule: an ask names no record. Whatever the asker supplies is
    /// a `{arg}` hole over one of the intent's OWN arguments — a hole naming
    /// something the verb does not take would be a question nothing can fill.
    #[test]
    fn every_hole_names_an_argument_of_its_own_intent() {
        for (_, module) in MOVED {
            for intent in module.intents {
                for ask in intent.answers {
                    for hole in holes(ask) {
                        assert!(
                            intent.args.iter().any(|arg| arg.name == hole),
                            "{}'s {ask:?} holes {hole:?}, which is not one of its arguments",
                            intent.name
                        );
                    }
                }
            }
        }
    }

    /// A10.3, the fix: every verb has one question a run can put — no holes,
    /// or holes over arguments it cannot run without. The three lookups that
    /// have no unnarrowed question say so in [`NEEDS_A_SUBJECT`], and that
    /// list may not shelter a verb that simply forgot its plain question.
    #[test]
    fn every_verb_has_a_question_a_run_can_put() {
        let set = evaluation_set();
        for (_, module) in MOVED {
            for intent in module.intents {
                let askable = set
                    .iter()
                    .any(|case| case.verb == intent.name && case.askable());
                let excepted = NEEDS_A_SUBJECT.contains(&intent.name);
                assert!(
                    askable != excepted,
                    "{}: askable={askable}, excepted={excepted} — a verb is one or the other",
                    intent.name
                );
                if excepted {
                    assert!(
                        intent.args.iter().all(|arg| !arg.required),
                        "{} is excepted, but has a required argument it could hole instead",
                        intent.name
                    );
                }
            }
        }
        // Every verb of every product is asked exactly once, and the question
        // chosen is the one the run can put.
        for (product, module) in MOVED {
            let chosen = one_per_verb(*product);
            assert_eq!(chosen.len(), module.intents.len());
            for case in &chosen {
                assert!(
                    case.askable() || NEEDS_A_SUBJECT.contains(&case.verb),
                    "{} is asked {:?}, which no run can put",
                    case.verb,
                    case.ask
                );
                assert!(
                    !case.asked().contains('{'),
                    "{} is asked with a hole left in it: {}",
                    case.verb,
                    case.asked()
                );
            }
        }
    }

    /// The regression the 2026-08-30 run found: an ask that names a customer,
    /// a colleague or a document number only its writer knows cannot be
    /// scored. Nothing mechanical can tell "the launch dinner" from a record
    /// a tenant holds, but a capital letter is what every one of the 41 had —
    /// `X`, `INV-2026-00042`, `Ben`, `ABC Supplies` — so a capitalised word
    /// outside this handful is refused, and the subject becomes a hole.
    #[test]
    fn no_ask_names_a_record() {
        // Days, months, the first person, and the two proper words the asks
        // legitimately carry: an abbreviation and a language.
        const ALLOWED: &[&str] = &[
            "I",
            "Monday",
            "Tuesday",
            "Wednesday",
            "Thursday",
            "Friday",
            "Saturday",
            "Sunday",
            "January",
            "February",
            "March",
            "April",
            "May",
            "June",
            "July",
            "August",
            "September",
            "October",
            "November",
            "December",
            "VAT",
            "French",
        ];
        for case in evaluation_set() {
            for word in case
                .ask
                .split(|c: char| !c.is_alphanumeric() && c != '\'' && c != '-')
                .filter(|word| word.chars().any(char::is_uppercase))
            {
                let bare = word.trim_end_matches("'s");
                assert!(
                    ALLOWED.contains(&bare),
                    "{} asks {:?}, which names {bare:?} — hole it instead",
                    case.verb,
                    case.ask
                );
            }
        }
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
