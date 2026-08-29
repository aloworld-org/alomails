//! Ask alo's **plan**: which product agent is asked what, in what order
//! (ADR 0034 "Ask alo orchestrates above them", A3.1).
//!
//! Ask alo used to be the widest agent rather than the one above the others: it
//! was offered every product's tools and answered out of one generic prompt, so
//! "orchestrates" meant "has everything". That is a bigger blast radius for one
//! turn and a worse answer, because a question about stock was decided by a
//! model reading sixty-eight tool descriptions instead of by the agent whose
//! whole prompt is stock.
//!
//! So the first call of an Ask alo turn chooses **agents, not tools**. It sees
//! the handles this person can actually see and nothing else — no tool list, no
//! records — and returns either a sentence of its own or a short list of steps.
//! Each step is then an ordinary product-agent turn, with that agent's prompt,
//! that agent's grounding and that agent's scope at the execution boundary.
//!
//! Three bounds, all here rather than in the prompt, because the model is the
//! untrusted party:
//!
//! - **At most [`MAX_PLAN_STEPS`] steps.** A run costs a model call per step and
//!   speaks in somebody's room each time; an injected or confused plan cannot
//!   turn one question into twenty.
//! - **Only a handle from the roster.** A step naming an agent this person
//!   cannot see is dropped here, and the roster the caller builds is already
//!   module-gated — so a plan cannot reach round the switch that hid an agent.
//! - **A plan with nothing left is not a plan.** If every step was dropped the
//!   parse fails, and the caller falls back to answering as an ordinary agent
//!   rather than posting an empty plan into a room.

use serde::Deserialize;

use alo_store::AgentProduct;

use crate::agent::extract_json;
use crate::agent_product::headline;
use crate::{AiConfig, ChatMessage, InferenceError, chat};

/// How many steps one orchestrated run may have (A3.1, widened by A8.3).
///
/// Four, the goal shape's width (ADR 0058 §7): look the record up, act on it,
/// tell the people it concerns, put the follow-through in the diary — four
/// products, one step each. It stays at the run's handoff budget, never above
/// it, so a maximal plan still fits its own run; beyond this a request is
/// really several requests, and saying so is a better answer than a run
/// nobody can follow.
pub const MAX_PLAN_STEPS: usize = 4;

/// An agent the plan may route to — one row of the roster.
///
/// The handle is what the model names and what the caller resolves back to an
/// agent record; the product decides the sentence describing it, so the
/// description a plan reads and the prompt that agent will be given come from
/// the same table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlanAgent<'a> {
    /// Its handle, without the `@`.
    pub handle: &'a str,
    /// What it is the agent of.
    pub product: AgentProduct,
}

/// What the planner is asked: the words, the roster, and the date.
///
/// Deliberately **no sources**. The planner routes; it does not answer from the
/// workspace, and grounding it would invite it to answer out of a search
/// snippet instead of asking the agent that owns the record — the failure ADR
/// 0034 names by name.
#[derive(Debug, Clone, Copy)]
pub struct PlanAsk<'a> {
    /// The request, in the asker's own words.
    pub request: &'a str,
    /// The agents this person can see, which is the whole menu.
    pub agents: &'a [PlanAgent<'a>],
    /// The caller's current date, so a step can carry a resolved "tomorrow".
    pub today: &'a str,
}

/// One step of a plan: an agent, and what to ask it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanStep {
    /// The handle, as it appears in the roster — never as the model spelled it.
    pub agent: String,
    /// The request for that agent, standing on its own.
    pub ask: String,
}

/// What the planner decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentPlan {
    /// Nothing to route: Ask alo says this itself.
    Answer(String),
    /// Ask these agents, in this order.
    Steps(Vec<PlanStep>),
}

/// The opening of the planner's system prompt.
const PLAN_HEAD: &str = "You are alo, the assistant above this person's whole workspace. You do NOT do the work yourself and you have no access to their records: every product has its own agent, and your job is to decide which of them is asked what, and in what order.\n\n\
You reply with a SINGLE JSON object and nothing else, in one of exactly two shapes:\n\
1) {\"kind\":\"answer\",\"answer\":\"<text>\"} — only when no agent below is needed: a greeting, a question about what you can do, or a request none of them covers. NEVER answer a question about this person's mail, files, calendar, tasks, customers, stock, figures or website this way. You cannot see any of it; the agents can.\n\
2) {\"kind\":\"plan\",\"steps\":[{\"agent\":\"<handle>\",\"ask\":\"<what to ask that agent>\"}]} — the agents to ask, in the order they should be asked.\n";

/// The rules on a plan's shape, and the output contract.
const PLAN_RULES: &str = "Rules for a plan:\n\
- Use the FEWEST steps that answer the request. One agent is the usual answer; two or three only when the request genuinely has that many parts.\n\
- Only name a handle from the list above. A handle that is not on it is dropped and its step never runs.\n\
- Each \"ask\" must stand on its own, in the person's own words and their language: the agent reading it sees neither the other steps nor their results, so \"and then file it\" tells it nothing.\n\
- Put a step that CHANGES something last. Everything a change depends on has to be looked up before it, and the run stops at the first change to wait for the person to approve it.\n\
- Resolve any relative date (today, tomorrow, next Friday) against the current date below before you put it in a step.\n\n\
Output ONLY the JSON object — no markdown, no code fences, no preamble.";

/// The roster, one line per agent, as the model reads it.
fn roster(agents: &[PlanAgent<'_>]) -> String {
    let mut out = String::new();
    for agent in agents {
        out.push_str(&format!(
            "- @{}: {}{}\n",
            agent.handle,
            headline(agent.product),
            registry_asks(agent.product)
        ));
    }
    out
}

/// How many of a product's example questions the roster quotes — enough to
/// route by, short enough that one product cannot crowd the list.
const ROSTER_ASKS: usize = 12;

/// The questions a product's verbs answer (ADR 0058), quoted after its
/// headline so the planner routes on what the agent can actually do. Empty for
/// a product that has not moved to intents yet.
fn registry_asks(product: AgentProduct) -> String {
    let asks: Vec<String> = crate::agent_product::tool_sets(product)
        .iter()
        .filter_map(|set| set.intents())
        .flat_map(|module| module.intents.iter())
        .flat_map(|intent| intent.answers.iter())
        .take(ROSTER_ASKS)
        .map(|ask| format!("\"{ask}\""))
        .collect();
    if asks.is_empty() {
        String::new()
    } else {
        format!(" Ask it for: {}.", asks.join(", "))
    }
}

/// The whole planner system prompt for one roster.
#[must_use]
pub fn plan_system_prompt(agents: &[PlanAgent<'_>], max_steps: usize) -> String {
    format!(
        "{PLAN_HEAD}\nAt most {max_steps} steps.\n\nThe agents you may ask, and nobody else:\n{}\n{PLAN_RULES}",
        roster(agents)
    )
}

/// The chat messages for one planning call. Pure and exported so the prompt is
/// testable without a backend.
#[must_use]
pub fn plan_messages(ask: &PlanAsk<'_>) -> Vec<ChatMessage> {
    vec![
        ChatMessage {
            role: "system".to_owned(),
            content: plan_system_prompt(ask.agents, MAX_PLAN_STEPS),
        },
        ChatMessage {
            role: "user".to_owned(),
            content: format!(
                "Today's date is {}.\nRequest: {}",
                ask.today.trim(),
                ask.request.trim()
            ),
        },
    ]
}

/// Read the planner's reply against the roster it was given.
///
/// The roster is a parameter rather than a courtesy check afterwards: a handle
/// the model invented, or one belonging to an agent this person's module
/// switches hide, must not survive parsing at all. What comes back carries the
/// roster's own spelling of the handle, so the caller resolves it by equality.
///
/// # Errors
/// [`InferenceError::Empty`] when there is no valid envelope, when an answer is
/// blank, or when a plan has no step naming an agent that exists — the caller
/// then takes an ordinary turn rather than posting an empty plan.
pub fn parse_plan(text: &str, agents: &[PlanAgent<'_>]) -> Result<AgentPlan, InferenceError> {
    #[derive(Deserialize)]
    struct Step {
        #[serde(default)]
        agent: String,
        #[serde(default)]
        ask: String,
    }
    #[derive(Deserialize)]
    struct Envelope {
        kind: String,
        #[serde(default)]
        answer: Option<String>,
        #[serde(default)]
        steps: Vec<Step>,
    }
    let json = extract_json(text).ok_or(InferenceError::Empty)?;
    let env: Envelope = serde_json::from_str(json).map_err(|_| InferenceError::Empty)?;
    match env.kind.as_str() {
        "answer" => {
            let answer = env.answer.unwrap_or_default().trim().to_owned();
            if answer.is_empty() {
                return Err(InferenceError::Empty);
            }
            Ok(AgentPlan::Answer(answer))
        }
        "plan" => {
            let steps: Vec<PlanStep> = env
                .steps
                .into_iter()
                .filter_map(|step| {
                    let ask = step.ask.trim().to_owned();
                    let named = step.agent.trim().trim_start_matches('@');
                    let known = agents
                        .iter()
                        .find(|agent| agent.handle.eq_ignore_ascii_case(named))?;
                    (!ask.is_empty()).then(|| PlanStep {
                        agent: known.handle.to_owned(),
                        ask,
                    })
                })
                .take(MAX_PLAN_STEPS)
                .collect();
            if steps.is_empty() {
                return Err(InferenceError::Empty);
            }
            Ok(AgentPlan::Steps(steps))
        }
        _ => Err(InferenceError::Empty),
    }
}

/// Ask the model for a plan.
///
/// # Errors
/// [`InferenceError`] for disabled/unconfigured/unreachable/backend/empty. Every
/// one of them is a reason for the caller to take an ordinary turn instead: a
/// workspace whose planner cannot be reached should still have an assistant.
pub async fn run_planner(
    config: &AiConfig,
    ask: &PlanAsk<'_>,
) -> Result<AgentPlan, InferenceError> {
    let text = chat(config, &plan_messages(ask), 0.2).await?;
    parse_plan(&text, ask.agents)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    /// ADR 0058: a moved product's roster line quotes the questions its verbs
    /// answer, so "what did we quote X" is routed to Billing by the registry.
    #[test]
    fn the_roster_says_what_a_moved_product_answers() {
        let prompt = plan_system_prompt(
            &[
                PlanAgent {
                    handle: "billing",
                    product: AgentProduct::Billing,
                },
                PlanAgent {
                    handle: "crm",
                    product: AgentProduct::Crm,
                },
                PlanAgent {
                    handle: "projects",
                    product: AgentProduct::Projects,
                },
                PlanAgent {
                    handle: "inventory",
                    product: AgentProduct::Inventory,
                },
                PlanAgent {
                    handle: "people",
                    product: AgentProduct::Hr,
                },
                PlanAgent {
                    handle: "agenda",
                    product: AgentProduct::Agenda,
                },
            ],
            MAX_PLAN_STEPS,
        );
        assert!(prompt.contains("- @billing:"));
        assert!(prompt.contains("\"what did we quote X\""), "{prompt}");
        assert!(prompt.contains("\"which quotes are open\""));
        // Sales moved at AA.1, so its line carries its verbs' questions too.
        let crm_line = prompt
            .lines()
            .find(|line| line.starts_with("- @crm:"))
            .unwrap_or_default();
        assert!(crm_line.contains("Ask it for:"), "{crm_line}");
        assert!(
            crm_line.contains("\"which deals are open, and at what stage\""),
            "{crm_line}"
        );
        // …and Projects at AA.3.
        let projects_line = prompt
            .lines()
            .find(|line| line.starts_with("- @projects:"))
            .unwrap_or_default();
        assert!(projects_line.contains("Ask it for:"), "{projects_line}");
        assert!(
            projects_line.contains("\"which projects are active\""),
            "{projects_line}"
        );
        // …and Inventory at AA.4.
        let inventory_line = prompt
            .lines()
            .find(|line| line.starts_with("- @inventory:"))
            .unwrap_or_default();
        assert!(inventory_line.contains("Ask it for:"), "{inventory_line}");
        assert!(
            inventory_line.contains("\"which products are below minimum\""),
            "{inventory_line}"
        );
        // …and HR at AA.5.
        let people_line = prompt
            .lines()
            .find(|line| line.starts_with("- @people:"))
            .unwrap_or_default();
        assert!(people_line.contains("Ask it for:"), "{people_line}");
        assert!(
            people_line.contains("\"who is off this week\""),
            "{people_line}"
        );
        // …and Agenda at AB.5 — the last module to move, so no roster line is
        // without its hints any more.
        let agenda_line = prompt
            .lines()
            .find(|line| line.starts_with("- @agenda:"))
            .unwrap_or_default();
        assert!(agenda_line.contains("Ask it for:"), "{agenda_line}");
        assert!(
            agenda_line.contains("\"what have I got on Thursday\""),
            "{agenda_line}"
        );
    }

    const ROSTER: [PlanAgent<'static>; 3] = [
        PlanAgent {
            handle: "mail",
            product: AgentProduct::Mail,
        },
        PlanAgent {
            handle: "tasks",
            product: AgentProduct::Tasks,
        },
        PlanAgent {
            handle: "inventory",
            product: AgentProduct::Inventory,
        },
    ];

    fn plan(text: &str) -> Result<AgentPlan, InferenceError> {
        parse_plan(text, &ROSTER)
    }

    #[test]
    fn the_prompt_offers_the_roster_and_nobody_else() {
        let msgs = plan_messages(&PlanAsk {
            request: "are we in contact with ABC?",
            agents: &ROSTER,
            today: "2026-08-15",
        });
        assert_eq!(msgs.len(), 2);
        let system = &msgs[0].content;
        assert!(system.contains("- @mail: You are the alo Mail agent"));
        assert!(system.contains("- @inventory: You are the alo Inventory agent"));
        // An agent nobody put on the roster is not describable to the model.
        assert!(!system.contains("@finance"));
        assert!(system.contains("At most 4 steps"));
        assert!(system.ends_with("no preamble."));
        assert!(msgs[1].content.contains("are we in contact with ABC?"));
        assert!(msgs[1].content.contains("2026-08-15"));
    }

    /// The planner is given **no sources**, so it cannot answer a question
    /// about the workspace out of a search snippet — it has to route it.
    #[test]
    fn the_planner_is_told_it_cannot_see_the_records() {
        let system = plan_system_prompt(&ROSTER, MAX_PLAN_STEPS);
        assert!(system.contains("no access to their records"));
        assert!(system.contains("NEVER answer a question about this person's"));
        assert!(!system.contains("Sources:"));
    }

    #[test]
    fn parses_a_single_step_plan() {
        let text = r#"{"kind":"plan","steps":[{"agent":"@mail","ask":"are we in contact with ABC Supplies?"}]}"#;
        assert_eq!(
            plan(text).unwrap(),
            AgentPlan::Steps(vec![PlanStep {
                agent: "mail".to_owned(),
                ask: "are we in contact with ABC Supplies?".to_owned(),
            }])
        );
    }

    #[test]
    fn parses_an_answer_and_tolerates_fences() {
        let text =
            "```json\n{\"kind\":\"answer\",\"answer\":\"I can ask the agents for you.\"}\n```";
        assert_eq!(
            plan(text).unwrap(),
            AgentPlan::Answer("I can ask the agents for you.".to_owned())
        );
    }

    /// **A handle nobody offered is dropped**, whatever the model spelled it
    /// as. The roster is already module-gated by the caller, so this is what
    /// keeps a plan from reaching round a switch that hid an agent.
    #[test]
    fn a_step_naming_an_agent_off_the_roster_is_dropped() {
        let text = r#"{"kind":"plan","steps":[
            {"agent":"@finance","ask":"what did we spend?"},
            {"agent":"mail","ask":"who last replied?"}]}"#;
        assert_eq!(
            plan(text).unwrap(),
            AgentPlan::Steps(vec![PlanStep {
                agent: "mail".to_owned(),
                ask: "who last replied?".to_owned(),
            }])
        );
        // And a plan of nothing but strangers is no plan at all.
        let none = r#"{"kind":"plan","steps":[{"agent":"payroll","ask":"pay everyone"}]}"#;
        assert!(plan(none).is_err());
    }

    /// The handle that comes back is the **roster's** spelling, so the caller
    /// resolves it by equality rather than by re-parsing what the model wrote.
    #[test]
    fn the_handle_comes_back_as_the_roster_spells_it() {
        let text = r#"{"kind":"plan","steps":[{"agent":"@MAIL","ask":"anything from ABC?"}]}"#;
        match plan(text).unwrap() {
            AgentPlan::Steps(steps) => assert_eq!(steps[0].agent, "mail"),
            other => panic!("expected steps, got {other:?}"),
        }
    }

    /// The bound is enforced here, not asked for in the prompt.
    #[test]
    fn a_plan_is_cut_to_the_step_budget() {
        let text = r#"{"kind":"plan","steps":[
            {"agent":"mail","ask":"one"},
            {"agent":"tasks","ask":"two"},
            {"agent":"inventory","ask":"three"},
            {"agent":"mail","ask":"four"},
            {"agent":"tasks","ask":"five"}]}"#;
        match plan(text).unwrap() {
            AgentPlan::Steps(steps) => {
                assert_eq!(steps.len(), MAX_PLAN_STEPS);
                assert_eq!(steps[3].ask, "four");
            }
            other => panic!("expected steps, got {other:?}"),
        }
    }

    #[test]
    fn a_step_with_nothing_to_ask_is_dropped() {
        let text = r#"{"kind":"plan","steps":[
            {"agent":"mail","ask":"   "},
            {"agent":"tasks","ask":"add a follow-up"}]}"#;
        match plan(text).unwrap() {
            AgentPlan::Steps(steps) => {
                assert_eq!(steps.len(), 1);
                assert_eq!(steps[0].agent, "tasks");
            }
            other => panic!("expected steps, got {other:?}"),
        }
    }

    #[test]
    fn rejects_garbage_empty_and_the_wrong_envelope() {
        assert!(plan("no json here").is_err());
        assert!(plan(r#"{"kind":"answer","answer":"  "}"#).is_err());
        assert!(plan(r#"{"kind":"plan","steps":[]}"#).is_err());
        assert!(plan(r#"{"kind":"plan"}"#).is_err());
        // The single-agent envelope is not a plan: an Ask alo turn that
        // returned one would otherwise be read as a step naming no agent.
        assert!(plan(r#"{"kind":"action","action":{"tool":"create_task"}}"#).is_err());
    }

    /// An empty roster is a workspace with no product agents at all. The prompt
    /// still renders, and every plan against it is rejected — so the caller
    /// falls back rather than posting steps nobody can take.
    #[test]
    fn a_roster_of_nobody_can_be_planned_against_and_routes_nowhere() {
        let system = plan_system_prompt(&[], MAX_PLAN_STEPS);
        assert!(system.contains("The agents you may ask"));
        assert!(
            parse_plan(
                r#"{"kind":"plan","steps":[{"agent":"mail","ask":"hi"}]}"#,
                &[]
            )
            .is_err()
        );
        assert_eq!(
            parse_plan(r#"{"kind":"answer","answer":"Hello."}"#, &[]).unwrap(),
            AgentPlan::Answer("Hello.".to_owned())
        );
    }
}
