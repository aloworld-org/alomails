//! Ask alo running a plan across the product agents (ADR 0034, A3.1).
//!
//! Before this, "Ask alo orchestrates above them" was a sentence in an ADR and
//! nothing in the code: Ask alo was simply the agent with every product's tools,
//! so a question about stock was decided by a model reading sixty-eight tool
//! descriptions instead of by the agent whose whole prompt is stock. This module
//! is the layer that makes the sentence true, and it is a layer **over**
//! [`crate::agent_turn`] rather than a second copy of it — every step is an
//! ordinary product-agent turn, with that agent's prompt, that agent's
//! grounding, and that agent's scope at the execution boundary.
//!
//! Four properties, each of which is why this file exists rather than a branch
//! in [`crate::chat_agent`]:
//!
//! * **It routes, it does not widen.** The plan chooses handles from a roster
//!   the asker can see, and each step runs under the delegate's own
//!   [`ChatAgentId`] — so `execute_tool` reads *its* product off *its* row.
//!   Ask alo's own scope is never what a delegated lookup runs at, which would
//!   quietly undo A1.2.
//! * **The plan is visible.** It goes into the room as a message before any of
//!   it runs, so somebody watching knows what is about to happen and can stop
//!   it. A run whose plan only appears afterwards is a run nobody could have
//!   stopped.
//! * **One approval surface.** A run stops at the first step that wants to
//!   change something: that step's proposal is the only thing to approve, and
//!   the rest of the plan waits behind it. Two pending proposals from one
//!   question would be two buttons whose order matters and which nothing
//!   enforces.
//! * **Stop actually stops.** The flag [`crate::chat_turns`] hands out is read
//!   between every step and again after every model call, which is what makes
//!   stopping a *run* different from declining to post one answer.
//!
//! **Each step speaks as its own agent, and joins the room to do it.** An agent
//! that takes part in a conversation is in that conversation (ADR 0034: agents
//! are first-class participants), and the alternative is worse than untidy:
//! chat's proposal approval reads the scope off the **author** of the message
//! carrying the proposal, so a delegated write posted under Ask alo's name would
//! execute at Ask alo's scope. Joining is idempotent and goes through
//! `add_agent_to_channel`, which re-checks the module gate — so a run cannot put
//! an agent in a room its asker was not allowed to reach.

use std::sync::atomic::{AtomicBool, Ordering};

use alo_ai::{AgentPlan, AiConfig, InferenceError, PlanAgent, PlanAsk, PlanStep};
use alo_store::{AgentProduct, ChatAgent, ChatAgentId, ChatChannelId, UserId};

use crate::agent_turn::{Turn, TurnContext, TurnResult, take_turn as run_turn};
use crate::chat_agent::{CHAT_SOURCES, Spoken, UNCONFIGURED, UNREACHABLE, ground};
use crate::push;
use crate::state::{Account, AppState};

/// What an orchestration attempt came to.
pub(crate) enum Orchestrated {
    /// It ran. `None` when it was stopped before it had said anything at all.
    Ran(Option<Spoken>),
    /// There was nothing to route to, or the planner could not be reached: the
    /// caller takes the ordinary single-agent turn instead, so a workspace
    /// whose planner is unavailable still has an assistant.
    NotRouted,
}

/// Introduces the plan in the room. The steps follow, one numbered line each.
const PLAN_HEADING: &str = "Here's how I'll do that:";

/// Said when a run is stopped part-way. It says how far it got, because the
/// room can already see the plan and the difference between "stopped" and
/// "still going" is the only thing that is not visible.
fn stopped_after(done: usize, total: usize) -> String {
    format!("Stopped — I did {done} of {total} steps.")
}

/// Said when a step wants to change something and there is more plan behind it.
const WAITING_ON_APPROVAL: &str =
    "The rest of this waits until you approve that. Turn it down and I'll leave it there.";

/// Said when an agent the plan named could not be brought into this room.
fn could_not_ask(handle: &str) -> String {
    format!("I couldn't ask @{handle} in this room, so I've stopped here.")
}

/// Everything one orchestrated run is carried out with.
///
/// A struct rather than six more parameters threaded through four functions:
/// they travel together from the moment the turn starts, and a positional list
/// of six borrows is how one run ends up carrying another's Stop flag.
pub(crate) struct Run<'a> {
    /// The room it is happening in.
    pub channel: &'a ChatChannelId,
    /// The Ask alo agent that was asked, which speaks for the run itself.
    pub alo: &'a ChatAgent,
    /// What was asked, in the asker's own words.
    pub question: &'a str,
    /// Today's date and the asker's clock, exactly as a single turn gets it.
    pub today: &'a str,
    /// The tenant's model.
    pub config: &'a AiConfig,
    /// Tripped by `POST /chat/channels/{id}/turns/{turn}/stop`
    /// ([`crate::chat_turns`]).
    pub stopped: &'a AtomicBool,
}

impl Run<'_> {
    /// Whether somebody has pressed Stop since the last time we looked.
    fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::SeqCst)
    }
}

/// Run one Ask alo turn as a plan across the product agents.
///
/// Returns [`Orchestrated::NotRouted`] when there is nothing to orchestrate —
/// no other agent this person can see, or a planner that gave nothing usable —
/// and the caller then takes an ordinary turn.
pub(crate) async fn orchestrate(
    state: &AppState,
    account: &Account,
    run: &Run<'_>,
) -> Orchestrated {
    let roster = roster(account, run.alo).await;
    if roster.is_empty() {
        return Orchestrated::NotRouted;
    }
    let offered: Vec<PlanAgent<'_>> = roster
        .iter()
        .map(|agent| PlanAgent {
            handle: &agent.handle,
            product: agent.product,
        })
        .collect();
    let planned = alo_ai::run_planner(
        run.config,
        &PlanAsk {
            request: run.question,
            agents: &offered,
            today: run.today,
        },
    )
    .await;
    // Stopped while the plan was being made: nothing has been said yet, so
    // nothing is said now. This is the one place a stop leaves the room silent,
    // and it is silent because the room never saw a plan to begin with.
    if run.is_stopped() {
        return Orchestrated::Ran(None);
    }
    let voice = Voice::new(state, account, run.channel).await;
    let steps = match planned {
        Ok(AgentPlan::Steps(steps)) => steps,
        // Nothing to route: Ask alo says it itself, which is the right answer to
        // "hello" and to "what can you do?".
        Ok(AgentPlan::Answer(text)) => {
            return match voice.say(&run.alo.id, &text).await {
                Some(_) => Orchestrated::Ran(Some(Spoken::Answered)),
                None => Orchestrated::Ran(None),
            };
        }
        // A planner that is switched off or unreachable must not leave a
        // mention unanswered: the ordinary turn says so in the caller, in the
        // same words every other agent uses.
        Err(_) => return Orchestrated::NotRouted,
    };
    Orchestrated::Ran(run_plan(&voice, run, &roster, &steps).await)
}

/// The agents this run may route to: the ones this person can see, minus the
/// retired, minus every "Ask alo" (which would be this run again).
///
/// Read through [`alo_store::AccountStore::agents`], so the module gate is the
/// same one the agent list itself obeys: a module an admin switched off for
/// this person has no agent here, and therefore no step can name it.
///
/// Crate-visible because a product agent's handoffs ([`crate::agent_turn`],
/// A5.1) route over this same list: one roster, one gate, however a
/// sub-question travels.
pub(crate) async fn roster(account: &Account, alo: &ChatAgent) -> Vec<ChatAgent> {
    account
        .acc
        .agents()
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|agent| {
            !agent.disabled
                && agent.product != AgentProduct::Workspace
                && agent.id.as_str() != alo.id.as_str()
        })
        .collect()
}

/// Post the plan, then take each step until one of them changes something, one
/// of them fails, or somebody presses Stop.
async fn run_plan(
    voice: &Voice<'_>,
    run: &Run<'_>,
    roster: &[ChatAgent],
    steps: &[PlanStep],
) -> Option<Spoken> {
    // The plan goes in the room **before** any of it runs. That is what makes
    // it a plan rather than a summary, and what gives somebody watching the
    // chance to stop it.
    voice.say(&run.alo.id, &render_plan(steps)).await?;

    let total = steps.len();
    for (done, step) in steps.iter().enumerate() {
        if run.is_stopped() {
            voice.say(&run.alo.id, &stopped_after(done, total)).await;
            return Some(Spoken::Stopped);
        }
        let Some(delegate) = roster.iter().find(|agent| agent.handle == step.agent) else {
            // Unreachable: the plan was parsed against this roster. Stated
            // anyway, because a silent `continue` here would be a step that
            // vanished without the room being told.
            voice.say(&run.alo.id, &could_not_ask(&step.agent)).await;
            return Some(Spoken::Excused);
        };
        // It answers under its own name, so an approval later runs at its
        // scope and not at Ask alo's — see the module note.
        if voice
            .account
            .acc
            .add_agent_to_channel(voice.channel, &delegate.id)
            .await
            .is_err()
        {
            voice
                .say(&run.alo.id, &could_not_ask(&delegate.handle))
                .await;
            return Some(Spoken::Excused);
        }
        let took = take_step(voice, run, delegate, &step.ask).await;
        // Stopped while this step was thinking: the model call cannot be
        // un-made, but its words can be kept out of the room, and the room is
        // told the run ended rather than left waiting on a plan.
        if run.is_stopped() {
            voice.say(&run.alo.id, &stopped_after(done, total)).await;
            return Some(Spoken::Stopped);
        }
        match took {
            Ok(TurnResult::Answer(answer)) => {
                voice.say(&delegate.id, &answer).await?;
            }
            Ok(TurnResult::Propose { action, say }) => {
                // **The one approval surface.** The step that wants a change is
                // where the run stops: its proposal is the only thing pending,
                // and everything after it waits behind that one tap.
                let said = voice.say(&delegate.id, &say).await?;
                voice
                    .account
                    .acc
                    .propose_action(&said, &action.tool, &action.args)
                    .await
                    .ok()?;
                if done + 1 < total {
                    voice.say(&run.alo.id, WAITING_ON_APPROVAL).await;
                }
                return Some(Spoken::Proposed);
            }
            Err(InferenceError::Disabled | InferenceError::NotConfigured) => {
                voice.say(&run.alo.id, UNCONFIGURED).await;
                return Some(Spoken::Excused);
            }
            Err(_) => {
                voice.say(&run.alo.id, UNREACHABLE).await;
                return Some(Spoken::Excused);
            }
        }
    }
    Some(Spoken::Answered)
}

/// One step: an ordinary product-agent turn, grounded in that product and run
/// under that agent's id.
///
/// Nothing here is special-cased for orchestration — that is the point. A step
/// gets the same read budget, the same reads-answer/writes-propose split and the
/// same boundary it would get if the person had typed `@mail …` themselves.
async fn take_step(
    voice: &Voice<'_>,
    run: &Run<'_>,
    delegate: &ChatAgent,
    ask: &str,
) -> Result<TurnResult, InferenceError> {
    let sources = ground(voice.account, delegate.product, ask, CHAT_SOURCES).await;
    let turn = Turn {
        product: delegate.product,
        request: ask,
        sources: &sources,
        today: run.today,
        folders: &[],
        context: TurnContext::in_room(&delegate.id, voice.channel),
        // A step does not hand off further (A5.1): the plan above it is the
        // delegation, and A5.2 makes the two one mechanism.
        roster: &[],
    };
    run_turn(voice.state, voice.account, run.config, &turn).await
}

/// The plan as the room reads it: one numbered line per step, naming the agent
/// and what it is being asked.
fn render_plan(steps: &[PlanStep]) -> String {
    let mut out = String::from(PLAN_HEADING);
    for (n, step) in steps.iter().enumerate() {
        out.push_str(&format!("\n{}. @{} — {}", n + 1, step.agent, step.ask));
    }
    out
}

/// The room, and everybody to tell when something is said in it.
///
/// The membership is read once per run rather than once per message: a run
/// posts up to five times, and the room does not change between them in any way
/// that matters.
struct Voice<'a> {
    state: &'a AppState,
    account: &'a Account,
    channel: &'a ChatChannelId,
    audience: Vec<UserId>,
}

impl<'a> Voice<'a> {
    async fn new(state: &'a AppState, account: &'a Account, channel: &'a ChatChannelId) -> Self {
        let audience = account
            .acc
            .channel_members(channel)
            .await
            .map(|members| members.into_iter().map(|member| member.user).collect())
            .unwrap_or_default();
        Self {
            state,
            account,
            channel,
            audience,
        }
    }

    /// Say one line in the room as `agent`, and tell the room it is there.
    ///
    /// The push after **each** message is what makes a plan watchable: without
    /// it the whole run would arrive at once at the end, and the Stop it exists
    /// to enable would have had nothing to interrupt.
    async fn say(&self, agent: &ChatAgentId, body: &str) -> Option<alo_store::ChatMessageId> {
        let said = self
            .account
            .acc
            .post_as_agent(self.channel, agent, body, None)
            .await
            .ok()?;
        push::notify_chat(self.state, &self.account.tenant, &self.audience).await;
        Some(said.id)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn step(agent: &str, ask: &str) -> PlanStep {
        PlanStep {
            agent: agent.to_owned(),
            ask: ask.to_owned(),
        }
    }

    /// The room reads the plan before any of it happens, and every line names
    /// the agent it will be asked of — which is what makes a Stop meaningful.
    #[test]
    fn the_plan_is_rendered_as_numbered_steps_naming_their_agents() {
        let rendered = render_plan(&[
            step("mail", "are we in contact with ABC Supplies?"),
            step("tasks", "add a follow-up for Friday"),
        ]);
        assert!(rendered.starts_with(PLAN_HEADING));
        assert!(rendered.contains("\n1. @mail — are we in contact with ABC Supplies?"));
        assert!(rendered.contains("\n2. @tasks — add a follow-up for Friday"));
        // One step is still a plan: the room should see what is being asked of
        // whom even when there is only one of it.
        let one = render_plan(&[step("inventory", "is the X100 in stock?")]);
        assert!(one.contains("\n1. @inventory — is the X100 in stock?"));
        assert!(!one.contains("\n2."));
    }

    #[test]
    fn a_stop_says_how_far_it_got() {
        assert_eq!(stopped_after(0, 3), "Stopped — I did 0 of 3 steps.");
        assert_eq!(stopped_after(2, 3), "Stopped — I did 2 of 3 steps.");
    }
}
