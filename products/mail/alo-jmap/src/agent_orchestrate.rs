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
//! * **The plan is the run's delegation, not a second machinery** (A5.2,
//!   ADR 0057 §3). Every step is [`crate::agent_turn::delegate_turn`] at
//!   depth 1, on ONE [`RunEnv`] budget held across the whole run: a step
//!   spends a handoff slot exactly as a handoff does, a step's own handoffs
//!   and reads draw down the same counters, and a plan the budget refuses to
//!   continue says so in the room rather than quietly running wider than a
//!   product agent's run ever could.
//! * **The plan is visible.** It goes into the room as a message before any of
//!   it runs, so somebody watching knows what is about to happen and can stop
//!   it. A run whose plan only appears afterwards is a run nobody could have
//!   stopped.
//! * **One approval surface.** A run stops at the first step that wants to
//!   change something: that step's proposal is the only thing to approve, and
//!   the rest of the plan waits behind it. Two pending proposals from one
//!   question would be two buttons whose order matters and which nothing
//!   enforces.
//! * **The plan is a goal record** (A8.3, ADR 0058 §7). The run keeps its
//!   progress on an `agent_goals` row: which step it is on, and — when a step
//!   proposed — the one proposal it waits behind. That row is what makes
//!   "the rest of this waits until you approve that" true rather than polite:
//!   approving the proposal hands the goal back to [`resume_goal`] and the
//!   remaining steps run; turning it down ends the goal. Coordination happens
//!   through the object, never through agents talking.
//! * **Stop actually stops.** The flag [`crate::chat_turns`] hands out is read
//!   between every step and again after every model call, which is what makes
//!   stopping a *run* different from declining to post one answer — and a
//!   resumed segment registers a turn of its own, so Stop reaches it too.
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
use alo_store::{
    AgentGoal, AgentGoalId, AgentProduct, ChatAgent, ChatAgentId, ChatChannelId, GoalEnd, GoalStep,
    UserId,
};

use crate::agent_turn::{OUT_OF_HANDOFFS, RunEnv, TurnResult, delegate_turn};
use crate::chat_agent::{Spoken, UNCONFIGURED, UNREACHABLE};
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
    // The plan becomes a goal record before any of it runs (A8.3): the object
    // an approval later resumes, and the card a room reads progress off.
    // Best-effort — a workspace whose bookkeeping insert failed still gets its
    // run, exactly as it did before goals existed; there is just nothing to
    // resume.
    let plan: Vec<GoalStep> = steps
        .iter()
        .map(|step| GoalStep {
            agent: step.agent.clone(),
            ask: step.ask.clone(),
        })
        .collect();
    let goal = account
        .acc
        .create_goal(run.channel, &run.alo.id, run.question, &plan)
        .await
        .ok();
    // ONE budget for the whole run (A5.2): the steps spend its handoff slots,
    // and whatever a step reads or hands off further comes out of the same
    // counters — the plan is the delegation, so it is bounded like one.
    let env = RunEnv::new(state, account, run.config);
    Orchestrated::Ran(
        run_plan(
            &voice,
            run,
            &env,
            &roster,
            &steps,
            goal.as_ref().map(|g| &g.id),
        )
        .await,
    )
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

/// Post the plan, then run its steps from the top.
async fn run_plan(
    voice: &Voice<'_>,
    run: &Run<'_>,
    env: &RunEnv<'_>,
    roster: &[ChatAgent],
    steps: &[PlanStep],
    goal: Option<&AgentGoalId>,
) -> Option<Spoken> {
    // The plan goes in the room **before** any of it runs. That is what makes
    // it a plan rather than a summary, and what gives somebody watching the
    // chance to stop it.
    voice.say(&run.alo.id, &render_plan(steps)).await?;
    run_steps(voice, run, env, roster, steps, 0, goal).await
}

/// End the goal, when there is one. Best-effort by design: the goal is the
/// run's record, and a record that could not be written must never stop the
/// run being spoken in the room.
async fn goal_ends(voice: &Voice<'_>, goal: Option<&AgentGoalId>, end: GoalEnd, note: &str) {
    if let Some(goal) = goal {
        let note = (!note.is_empty()).then_some(note);
        let _ = voice.account.acc.finish_goal(goal, end, note).await;
    }
}

/// Take each step from `from` until one of them changes something, one of them
/// fails, the budget refuses another, or somebody presses Stop — keeping the
/// goal record in step with what the room sees (A8.3). Called with `from = 0`
/// by a fresh plan and with the goal's own cursor by [`resume_goal`].
async fn run_steps(
    voice: &Voice<'_>,
    run: &Run<'_>,
    env: &RunEnv<'_>,
    roster: &[ChatAgent],
    steps: &[PlanStep],
    from: usize,
    goal: Option<&AgentGoalId>,
) -> Option<Spoken> {
    let acc = &voice.account.acc;
    let total = steps.len();
    for (done, step) in steps.iter().enumerate().skip(from) {
        if run.is_stopped() {
            goal_ends(voice, goal, GoalEnd::Stopped, "").await;
            voice.say(&run.alo.id, &stopped_after(done, total)).await;
            return Some(Spoken::Stopped);
        }
        // A step is a handoff and spends a handoff slot (A5.2). A full plan
        // always fits — the planner is capped at the budget — so this only
        // bites when an earlier step's own handoffs spent the rest, and then
        // the room is told rather than the bound being quietly exceeded.
        if !env.take_handoff() {
            goal_ends(voice, goal, GoalEnd::Failed, "the run's budget was spent").await;
            voice.say(&run.alo.id, OUT_OF_HANDOFFS).await;
            return Some(Spoken::Excused);
        }
        let Some(delegate) = roster.iter().find(|agent| agent.handle == step.agent) else {
            // A resumed goal re-reads the roster, so an agent whose module was
            // switched off while the goal waited lands here rather than being
            // reached around the gate; on a fresh plan it is unreachable (the
            // plan was parsed against this roster) but still said, because a
            // silent `continue` would be a step that vanished.
            goal_ends(voice, goal, GoalEnd::Failed, "an agent could not be asked").await;
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
            goal_ends(voice, goal, GoalEnd::Failed, "an agent could not be asked").await;
            voice
                .say(&run.alo.id, &could_not_ask(&delegate.handle))
                .await;
            return Some(Spoken::Excused);
        }
        // The step is a delegate turn at depth 1 — the same mechanism a
        // product agent's handoff travels by, offered the same roster, so a
        // step may hand off once more and every spend lands on the run's one
        // budget. Nothing here is special-cased for orchestration: the step
        // gets the same grounding, the same reads-answer/writes-propose split
        // and the same boundary it would get if the person had typed
        // `@mail …` themselves.
        let took = delegate_turn(
            env,
            delegate,
            &step.ask,
            run.today,
            Some(voice.channel),
            roster,
            1,
        )
        .await;
        // Stopped while this step was thinking: the model call cannot be
        // un-made, but its words can be kept out of the room, and the room is
        // told the run ended rather than left waiting on a plan.
        if run.is_stopped() {
            goal_ends(voice, goal, GoalEnd::Stopped, "").await;
            voice.say(&run.alo.id, &stopped_after(done, total)).await;
            return Some(Spoken::Stopped);
        }
        match took {
            Ok(TurnResult::Answer(answer)) => {
                voice.say(&delegate.id, &answer).await?;
                if let Some(goal) = goal {
                    let _ = acc.advance_goal(goal, done + 1).await;
                }
            }
            Ok(TurnResult::Propose { action, say }) => {
                // **The one approval surface.** The step that wants a change is
                // where the run stops: its proposal is the only thing pending,
                // everything after it waits behind that one tap — and the goal
                // records which tap, so the approval knows what to resume.
                let said = voice.say(&delegate.id, &say).await?;
                let card = voice
                    .account
                    .acc
                    .propose_action(&said, &action.tool, &action.args)
                    .await
                    .ok()?;
                if let Some(goal) = goal {
                    let _ = acc.goal_awaits(goal, &card).await;
                }
                if done + 1 < total {
                    voice.say(&run.alo.id, WAITING_ON_APPROVAL).await;
                }
                return Some(Spoken::Proposed);
            }
            // A step's own delegate proposed (A5.2): the proposal is already
            // in the room under that delegate's id, and it is the same one
            // surface — the run stops behind it exactly as it does for a
            // step's own write, and the goal waits on it the same way.
            Ok(TurnResult::DelegateProposed(card)) => {
                if let Some(goal) = goal {
                    let _ = acc.goal_awaits(goal, &card).await;
                }
                if done + 1 < total {
                    voice.say(&run.alo.id, WAITING_ON_APPROVAL).await;
                }
                return Some(Spoken::Proposed);
            }
            Err(InferenceError::Disabled | InferenceError::NotConfigured) => {
                goal_ends(voice, goal, GoalEnd::Failed, "no AI provider is configured").await;
                voice.say(&run.alo.id, UNCONFIGURED).await;
                return Some(Spoken::Excused);
            }
            Err(error) => {
                // The room is told nothing about why — `UNREACHABLE` says so
                // deliberately — but an operator must be able to find out.
                // Before this line the reason was discarded, and a run that
                // stopped here looked the same whether the provider was
                // rate-limiting, timing out, or refusing the request.
                tracing::warn!(
                    step = done + 1,
                    of = total,
                    delegate = %delegate.handle,
                    %error,
                    "an orchestrated step could not reach the model"
                );
                goal_ends(
                    voice,
                    goal,
                    GoalEnd::Failed,
                    "the model could not be reached",
                )
                .await;
                voice.say(&run.alo.id, UNREACHABLE).await;
                return Some(Spoken::Excused);
            }
        }
    }
    goal_ends(voice, goal, GoalEnd::Done, "").await;
    Some(Spoken::Answered)
}

/// Said when an approval hands a goal back and there is more plan to run —
/// so the room knows the tap did more than settle a card.
fn carrying_on(done: usize, total: usize) -> String {
    format!("Carrying on — step {} of {total}.", done + 1)
}

/// Carry a goal on after the proposal it waited behind was approved and
/// executed (A8.3).
///
/// The remaining steps run through the very machinery the original segment
/// used — [`run_steps`], the same roster gate, a fresh [`RunEnv`] budget (an
/// approval is a person back in the loop, and the leash starts over) — and the
/// segment registers a turn of its own, so the room sees Ask alo thinking and
/// Stop reaches a resumed run exactly as it reaches a fresh one.
///
/// Spawned, not awaited: the approval's HTTP response must not wait on a model.
/// Every early return below ends the goal with its reason; the one silent path
/// is `resume_goal` itself refusing, which means the goal moved under us —
/// somebody stopped it between the tap and here — and their ending stands.
pub(crate) fn resume_goal(state: &AppState, account: &Account, goal: AgentGoal) {
    let state = state.clone();
    let account = account.clone();
    tokio::spawn(async move {
        let acc = &account.acc;
        let Ok(goal) = acc.resume_goal(&goal.id).await else {
            return;
        };
        // The approved step was the last: there is nothing left to run.
        if goal.cursor >= goal.steps.len() {
            let _ = acc.finish_goal(&goal.id, GoalEnd::Done, None).await;
            return;
        }
        let Ok(alo) = acc.agent(&goal.agent).await else {
            let _ = acc
                .finish_goal(&goal.id, GoalEnd::Failed, Some("its agent is gone"))
                .await;
            return;
        };
        let voice = Voice::new(&state, &account, &goal.channel).await;
        let config = match acc.default_ai_config().await {
            Ok(Some(row)) => AiConfig {
                base_url: row.base_url,
                model: row.model,
                api_key: row.api_key,
                enabled: row.enabled,
            },
            _ => {
                goal_ends(
                    &voice,
                    Some(&goal.id),
                    GoalEnd::Failed,
                    "no AI provider is configured",
                )
                .await;
                voice.say(&alo.id, UNCONFIGURED).await;
                return;
            }
        };
        let today = crate::chat_agent::today_for(acc).await;
        // A turn of its own: visible in the room's turn list, stoppable by the
        // asker — who is the caller, because only the asker decides a proposal.
        let (turn, stopped) = state.turns.begin(
            &account.tenant,
            &goal.channel,
            alo.id.as_str(),
            &alo.handle,
            acc.user().as_str(),
        );
        let run = Run {
            channel: &goal.channel,
            alo: &alo,
            question: &goal.request,
            today: &today,
            config: &config,
            stopped: &stopped,
        };
        let roster = roster(&account, &alo).await;
        let steps: Vec<PlanStep> = goal
            .steps
            .iter()
            .map(|step| PlanStep {
                agent: step.agent.clone(),
                ask: step.ask.clone(),
            })
            .collect();
        let env = RunEnv::new(&state, &account, &config);
        if voice
            .say(&alo.id, &carrying_on(goal.cursor, steps.len()))
            .await
            .is_some()
        {
            run_steps(
                &voice,
                &run,
                &env,
                &roster,
                &steps,
                goal.cursor,
                Some(&goal.id),
            )
            .await;
        }
        state.turns.end(&account.tenant, &goal.channel, &turn);
    });
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
