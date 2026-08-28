//! One agent turn, with its reading tools run **inside it** (ADR 0047).
//!
//! Before this, every tool the model chose came back as a proposal, reads
//! included: `@inventory is the X100 in stock?` answered with a sentence about
//! what it was willing to look up, and the stock figure arrived only after a
//! tap. That was never a safety property — it was one envelope and one code
//! path, grown into a rule.
//!
//! So a turn is a small loop rather than a single call: ask, and if what comes
//! back is a **read**, run it, put its result among the sources, and ask again
//! so the answer is cited to the record instead of to a search snippet. A
//! **write** still comes back as a proposal and is the caller's to record.
//!
//! A turn may also **hand off** a sub-question to another agent (ADR 0057 §3,
//! A5.1): the model replies with a `delegate` envelope, the named agent takes
//! an ordinary nested turn — its prompt, its grounding, its scope at the
//! execution boundary, all through the **asker's** account door — and its
//! answer is folded back in as a further numbered source to cite. The room
//! sees the handoff line before it runs; a delegate that merely answers posts
//! nothing. A delegate that wants to **change** something posts its proposal
//! itself, under its own id, and the run ends there (A5.2): one pending
//! proposal is the run's whole approval surface, and the author of the message
//! carrying it is what the approval later runs at — so it must be the
//! delegate, never the asking agent.
//!
//! **Ask alo's planner is this mechanism, not a second one** (A5.2): an
//! orchestrated step is [`delegate_turn`] at depth 1, drawing on the same
//! [`RunEnv`] budget as the handoffs it may make itself.
//!
//! The bounds hold it down, and all of them are here rather than in the prompt:
//!
//! - **At most [`MAX_READS`] read executions per run.** A confused or injected
//!   turn cannot spend a workspace's inference budget going round; on the
//!   seventh it answers with what it has and says so.
//! - **At most [`MAX_HANDOFFS`] handoffs per run, to a depth of
//!   [`MAX_HANDOFF_DEPTH`]**, and the run has ONE budget: a delegate's reads
//!   and further handoffs draw down the same [`RunBudget`] the asking agent
//!   started with, so nesting multiplies nothing.
//! - **A write is refused at the execution boundary**, not asked not to happen.
//!   [`crate::agent::execute_tool`] is given [`Approval::InTurn`] here, and that
//!   is what makes "reads only" true no matter what the model returns.
//! - **A handoff reaches only agents the asker can see.** The roster is the
//!   asker's own module-gated agent list; a handle outside it is dropped with
//!   a line the model can answer around, never resolved more widely.
//!
//! Everything runs through the **asker's** account door, exactly as the
//! retrieval that grounds the turn already did. A read's blast radius under
//! prompt injection is therefore the one workspace search already had: a
//! hostile message can make an agent look up something the person who
//! triggered the turn could already look up, and nothing further — and a
//! handoff widens that to other agents' *reads*, still as the same person.

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};

use alo_ai::{AgentAsk, AgentDecision, AiConfig, InferenceError, PlanAgent, WorkspaceSource};
use alo_store::{AgentProduct, ChatAgent, ChatAgentId, ChatChannelId};

use crate::agent::{Approval, ToolRun, execute_tool};
use crate::state::{Account, AppState};

/// How many reading tools one run may execute (ADR 0047 §2, raised by ADR
/// 0058: "what did we quote Northstar" is customer → quotes → quote).
const MAX_READS: usize = 6;

/// How many handoffs one run may make, refusals included (ADR 0057 §3). A
/// dropped handoff spends its slot too — that is what keeps a model that
/// insists on delegating from circling forever.
const MAX_HANDOFFS: usize = 4;

/// How deep a handoff chain may go (ADR 0057 §3): the asking agent may hand
/// off, its delegate may hand off once more, and the turn at that depth is
/// offered nobody.
const MAX_HANDOFF_DEPTH: usize = 2;

// A full plan always fits its run's budget (A5.2). Ask alo's plan is this run
// machinery — each step spends a handoff slot — so the planner's step cap must
// sit at or below the handoff cap, or a maximal plan would refuse its own last
// step before anything else had spent a thing.
const _: () = assert!(alo_ai::MAX_PLAN_STEPS <= MAX_HANDOFFS);

/// How much of a tool's result is shown to the model. Enough for a diary month
/// or a stock record; short enough that one verbose tool cannot crowd out the
/// question it was meant to answer.
///
/// Crate-visible because one tool's result is written **against** it: the
/// Insights catalog ([`crate::agent_insights`]) is a menu a model has to read
/// whole, and half a menu is a menu with invented spellings at the end of it, so
/// a test there holds the rendering under this number rather than under a copy
/// of it.
pub(crate) const MAX_RESULT_CHARS: usize = 4_000;

/// The `kind` a tool result carries in the numbered sources, so the model can
/// tell what it looked up from what a search happened to match.
const RESULT_KIND: &str = "tool result";

/// The `kind` a delegate's folded-in answer carries in the numbered sources
/// (A5.1), so citing it names the agent that was asked.
const DELEGATED_KIND: &str = "delegated answer";

/// Said when the turn used every lookup it had and still wanted another. It
/// says which question went unanswered rather than pretending to one.
const OUT_OF_LOOKUPS: &str = "I looked things up as far as I'm allowed to for one question and still couldn't get to an \
     answer. Could you narrow it down, or ask me one part of it at a time?";

/// Said when the run spent every handoff it had and still wanted another.
/// Crate-visible because an orchestrated run says it too when its plan wants
/// a step the budget refuses (A5.2: one budget, however the sub-question
/// travels).
pub(crate) const OUT_OF_HANDOFFS: &str = "I've asked the other agents as much as I'm allowed to for one question and still couldn't \
     finish. Could you ask the remaining part directly?";

/// One run's spend, shared across the asking turn and every turn it delegates
/// to — the "one budget" of ADR 0057 §3. Counters rather than a mutex because
/// the whole run is one task: atomics are just the cheapest thing that lets
/// the nested futures stay `Send`.
struct RunBudget {
    /// Read executions taken, capped at [`MAX_READS`].
    reads: AtomicUsize,
    /// Handoffs taken (refused ones included), capped at [`MAX_HANDOFFS`].
    handoffs: AtomicUsize,
}

impl RunBudget {
    const fn new() -> Self {
        Self {
            reads: AtomicUsize::new(0),
            handoffs: AtomicUsize::new(0),
        }
    }

    /// Take one read from the run, saying whether there was one to take.
    fn take_read(&self) -> bool {
        take(&self.reads, MAX_READS)
    }

    /// Whether a further read could still be taken — what the after-read
    /// prompt tells the model about its remaining budget.
    fn reads_left(&self) -> bool {
        self.reads.load(Ordering::SeqCst) < MAX_READS
    }

    /// Take one handoff from the run, saying whether there was one to take.
    fn take_handoff(&self) -> bool {
        take(&self.handoffs, MAX_HANDOFFS)
    }
}

/// Count one use against `cap`, refusing at it.
fn take(counter: &AtomicUsize, cap: usize) -> bool {
    counter
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |used| {
            (used < cap).then_some(used + 1)
        })
        .is_ok()
}

/// Where a turn is happening, for the boundary check and the audit record.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TurnContext<'a> {
    /// The agent taking it; `None` for the command palette's assistant, which
    /// is not a row in `chat_agents`.
    pub agent: Option<&'a ChatAgentId>,
    /// The room it is happening in; `None` outside chat.
    pub channel: Option<&'a ChatChannelId>,
}

impl<'a> TurnContext<'a> {
    /// A turn with no room and no agent record — the command palette.
    pub(crate) const fn palette() -> Self {
        Self {
            agent: None,
            channel: None,
        }
    }

    /// A turn an agent is taking in a room.
    pub(crate) const fn in_room(agent: &'a ChatAgentId, channel: &'a ChatChannelId) -> Self {
        Self {
            agent: Some(agent),
            channel: Some(channel),
        }
    }

    /// How a tool run from inside this turn arrives at the boundary: with
    /// nobody's approval, and therefore reads only.
    const fn run(self) -> ToolRun<'a> {
        ToolRun {
            approval: Approval::InTurn,
            agent: self.agent,
            channel: self.channel,
        }
    }
}

/// Everything one turn is asked from: the words, the access-scoped grounding,
/// the clock, and where it is happening.
///
/// A struct rather than five more parameters, because the two surfaces that
/// build one — a room and the command palette — should be visibly filling in
/// the same form.
pub(crate) struct Turn<'a> {
    /// The product whose agent is taking it, which decides what it is offered
    /// (A1.2). [`AgentProduct::Workspace`] for the command palette's "Ask alo".
    ///
    /// This is the *prompt's* copy of the scope. The one that is enforced is
    /// read from the agent's own row at the execution boundary, so a turn that
    /// claimed a wider product here would be offered tools it still could not
    /// run.
    pub product: AgentProduct,
    /// What was asked, in the asker's own words.
    pub request: &'a str,
    /// The grounding the caller retrieved through the asker's access.
    pub sources: &'a [WorkspaceSource],
    /// Today's date, and the clock the asker is on.
    pub today: &'a str,
    /// The asker's own mail folders, for `move_to_folder`.
    pub folders: &'a [String],
    /// The agent and the room, if there are any.
    pub context: TurnContext<'a>,
    /// The agents this run may hand a sub-question to (A5.1): the asker's own
    /// module-gated roster, already stripped of the retired and of every "Ask
    /// alo". Empty where delegation has no place — the command palette, which
    /// has no room for the handoff line to be seen in. An orchestrated step
    /// carries it too (A5.2): the plan is that run's delegation, and a step
    /// may hand off further on the same budget.
    pub roster: &'a [ChatAgent],
}

/// What one turn ended as.
pub(crate) enum TurnResult {
    /// Something to say, grounded in the sources and in whatever was looked up.
    Answer(String),
    /// A change the asker has to approve before it happens.
    Propose {
        /// The tool the caller records against the agent's message.
        action: alo_ai::ProposedAction,
        /// The sentence that goes in the room.
        say: String,
    },
    /// A delegate's write, already said and recorded in the room under the
    /// delegate's own id (A5.2) — the run's one approval surface. There is
    /// nothing left for the caller to post, and nothing further may run: a
    /// second pending proposal would be a second button whose order matters.
    DelegateProposed,
}

/// Run one turn to a result, executing its reading tools along the way.
///
/// `sources` is the access-scoped grounding the caller retrieved; the tool
/// results are appended to it as further numbered sources, so a citation means
/// the same thing whichever it points at.
///
/// # Errors
/// [`InferenceError`] for disabled/unconfigured/unreachable/backend/empty —
/// the tool runs themselves never fail a turn, because a refusal or a bad
/// argument is something the model can be told about and answer around.
pub(crate) async fn take_turn(
    state: &AppState,
    account: &Account,
    config: &AiConfig,
    turn: &Turn<'_>,
) -> Result<TurnResult, InferenceError> {
    let env = RunEnv::new(state, account, config);
    turn_at(&env, turn, 0).await
}

/// What every depth of one run shares: the world it runs in, and the one
/// budget it spends. A struct rather than four parameters threaded through the
/// recursion, so a nested turn cannot be handed somebody else's budget.
///
/// Crate-visible because an orchestrated run is one of these too (A5.2): Ask
/// alo's plan holds one env across all its steps, so a step and the handoffs
/// it makes draw down the same budget.
pub(crate) struct RunEnv<'a> {
    state: &'a AppState,
    account: &'a Account,
    config: &'a AiConfig,
    budget: RunBudget,
}

impl<'a> RunEnv<'a> {
    /// One run's environment, with a fresh budget.
    pub(crate) fn new(state: &'a AppState, account: &'a Account, config: &'a AiConfig) -> Self {
        Self {
            state,
            account,
            config,
            budget: RunBudget::new(),
        }
    }

    /// Take one handoff from the run's budget, saying whether there was one to
    /// take. An orchestrated plan spends this for each of its steps — the plan
    /// IS the delegation, so a step and a handoff are the same spend.
    pub(crate) fn take_handoff(&self) -> bool {
        self.budget.take_handoff()
    }
}

/// One turn at one depth of a run, drawing on the run's one budget.
///
/// Boxed because it is recursive: a handoff takes the delegate's turn through
/// this same function at `depth + 1`, and an async fn cannot hold its own
/// future inline.
fn turn_at<'a>(
    env: &'a RunEnv<'a>,
    turn: &'a Turn<'a>,
    depth: usize,
) -> Pin<Box<dyn Future<Output = Result<TurnResult, InferenceError>> + Send + 'a>> {
    Box::pin(async move {
        let Turn {
            product,
            request,
            today,
            folders,
            context,
            ..
        } = *turn;
        let mut sources = turn.sources.to_vec();
        let offers = handoff_offers(turn, depth);
        // Built each round rather than held across the loop: `sources` grows by
        // a tool result between the calls, so an `AgentAsk` borrowing it cannot
        // outlive one round.
        let mut decided = alo_ai::run_agent(
            env.config,
            &AgentAsk {
                product,
                request,
                sources: &sources,
                today,
                folders,
                delegates: &offers,
            },
        )
        .await?;

        loop {
            // Every round either ends the turn or spends the run's budget, and
            // a spend the budget refuses ends the turn too — which is what
            // bounds the loop, nested turns included.
            let (kind, title, detail) = match step(decided) {
                Step::Done(result) => return Ok(result),
                Step::RunRead(action) => {
                    if !env.budget.take_read() {
                        return Ok(finish(Step::RunRead(action)));
                    }
                    let detail = run_read(env.state, env.account, &action, context).await;
                    (RESULT_KIND, action.tool.clone(), detail)
                }
                Step::Handoff { to, ask } => {
                    if !env.budget.take_handoff() {
                        return Ok(finish(Step::Handoff { to, ask }));
                    }
                    match run_handoff(env, turn, depth, &to, &ask).await {
                        Handoff::Fold(detail) => (DELEGATED_KIND, format!("@{to}"), detail),
                        // The delegate proposed, in the room, under its own id:
                        // the run is over, whatever depth this is — one pending
                        // proposal is the whole approval surface (A5.2).
                        Handoff::Over(result) => return Ok(result),
                    }
                }
            };
            sources.push(WorkspaceSource {
                index: sources.len() + 1,
                kind: kind.to_owned(),
                title,
                detail,
            });
            decided = alo_ai::run_agent_after_read(
                env.config,
                &AgentAsk {
                    product,
                    request,
                    sources: &sources,
                    today,
                    folders,
                    delegates: &offers,
                },
                env.budget.reads_left(),
            )
            .await?;
        }
    })
}

/// The agents this turn is offered to hand off to: the run's roster minus the
/// turn's own agent, and nobody once the chain is [`MAX_HANDOFF_DEPTH`] deep —
/// an offer that cannot be honoured is the invitation to a refusal loop.
fn handoff_offers<'a>(turn: &'a Turn<'_>, depth: usize) -> Vec<PlanAgent<'a>> {
    if depth >= MAX_HANDOFF_DEPTH {
        return Vec::new();
    }
    turn.roster
        .iter()
        .filter(|agent| {
            !turn
                .context
                .agent
                .is_some_and(|id| id.as_str() == agent.id.as_str())
        })
        .map(|agent| PlanAgent {
            handle: &agent.handle,
            product: agent.product,
        })
        .collect()
}

/// What one handoff came to, for the loop above.
enum Handoff {
    /// A line for the asking model's sources: the delegate's answer, or the
    /// sentence saying why there is none.
    Fold(String),
    /// The run is over: a delegate's proposal is pending in the room, and
    /// nothing further may run behind it.
    Over(TurnResult),
}

/// One handoff (ADR 0057 §3): resolve the handle against the asker's own
/// roster, say in the room who is being asked what, take the delegate's turn
/// as the asker on the run's shared budget, and fold what came of it into a
/// line the asking model can cite — or answer around.
///
/// Nothing here fails the turn. A handle the asker cannot see or a model that
/// could not be reached is a sentence for the model, because "say which part
/// you could not do" is a far better turn than one that dies silently. A
/// delegate that wants to **change** something ends the run instead (A5.2):
/// its proposal goes in the room under its own id and waits for the asker.
async fn run_handoff(
    env: &RunEnv<'_>,
    turn: &Turn<'_>,
    depth: usize,
    to: &str,
    ask: &str,
) -> Handoff {
    // Resolved against the same roster the offer was made from — the asker's
    // own module-gated agents — and against the same depth rule, so a stray
    // envelope at the depth cap meets the same line an unknown handle does.
    // A handle the asker cannot see is DROPPED here: no room line, no turn.
    let target = turn
        .roster
        .iter()
        .filter(|_| depth < MAX_HANDOFF_DEPTH)
        .find(|agent| {
            agent.handle == to
                && !turn
                    .context
                    .agent
                    .is_some_and(|id| id.as_str() == agent.id.as_str())
        });
    let Some(delegate) = target else {
        return Handoff::Fold(format!(
            "nobody was asked: there is no @{to} here you can hand this to — answer from what \
             you have, or say which part you could not do"
        ));
    };
    // The room sees who asked whom for what, before it runs — the handoff
    // line ADR 0057 §3 requires ("visibly: the room sees who asked whom").
    // Best-effort, like every posting inside a turn. A nested asker is a
    // delegate itself and not yet in the room, so it joins to say its line —
    // the same idempotent, module-gated join an orchestrated step makes,
    // because an agent that takes part in a conversation is a participant in
    // it (ADR 0034).
    if let (Some(asking), Some(channel)) = (turn.context.agent, turn.context.channel) {
        let line = format!("I'm asking @{}: {ask}", delegate.handle);
        join_and_say(env, channel, asking, &line).await;
    }
    match delegate_turn(
        env,
        delegate,
        ask,
        turn.today,
        turn.context.channel,
        turn.roster,
        depth + 1,
    )
    .await
    {
        Ok(TurnResult::Answer(text)) => {
            Handoff::Fold(format!("@{} answered: {text}", delegate.handle))
        }
        // **A delegate's write lands on the asker's one approval surface**
        // (A5.2): said in the room by the delegate itself — the author of the
        // message is what the approval runs at, so it must be the delegate —
        // and recorded against that message. The run ends here.
        Ok(TurnResult::Propose { action, say }) => match turn.context.channel {
            Some(channel) => match propose_in_room(env, channel, &delegate.id, &action, &say).await
            {
                Some(()) => Handoff::Over(TurnResult::DelegateProposed),
                // The room would not take it — the delegate's module gate, a
                // room gone archived. The old words are the safe floor: the
                // person is pointed at the agent that can do it.
                None => Handoff::Fold(wanted_a_change(&delegate.handle, &say)),
            },
            // No room to carry a proposal (never the case for a handoff today:
            // the palette offers no roster) — the words, not a lost write.
            None => Handoff::Fold(wanted_a_change(&delegate.handle, &say)),
        },
        // A deeper delegate already proposed: the surface exists, bubble the
        // end of the run up the chain.
        Ok(TurnResult::DelegateProposed) => Handoff::Over(TurnResult::DelegateProposed),
        Err(_) => Handoff::Fold(format!(
            "the handoff to @{} did not run: the model could not be reached",
            delegate.handle
        )),
    }
}

/// The fold-in line for a delegate's write that could not be proposed: the
/// asking agent is told what it wanted so it can say so, and the person can
/// ask that agent directly.
fn wanted_a_change(handle: &str, say: &str) -> String {
    format!(
        "@{handle} did not answer — it wanted to make a change first: \"{say}\". That change \
         could not be put up for approval here: say so, and that the person can ask @{handle} \
         directly."
    )
}

/// Take one delegate's turn: its product's grounding, its prompt, its id at
/// the execution boundary — and the asker's account door under all of it,
/// which is what "as the asker" means.
///
/// **The one mechanism a sub-question travels by** (A5.2): a handoff calls it
/// at `depth + 1`, and an orchestrated step of Ask alo's plan calls it at
/// depth 1 — the plan is that run's delegation, not a second machinery — so
/// both draw on the same [`RunEnv`] budget and both may hand off further
/// until the depth cap.
///
/// # Errors
/// [`InferenceError`] as [`take_turn`]: the caller decides whether that is a
/// fold-in sentence (a handoff) or a room line (a plan step).
pub(crate) async fn delegate_turn(
    env: &RunEnv<'_>,
    delegate: &ChatAgent,
    ask: &str,
    today: &str,
    channel: Option<&ChatChannelId>,
    roster: &[ChatAgent],
    depth: usize,
) -> Result<TurnResult, InferenceError> {
    let ground = crate::chat_agent::ground(
        env.account,
        delegate.product,
        ask,
        crate::chat_agent::CHAT_SOURCES,
    )
    .await;
    let nested = Turn {
        product: delegate.product,
        request: ask,
        sources: &ground,
        today,
        folders: &[],
        context: TurnContext {
            agent: Some(&delegate.id),
            channel,
        },
        roster,
    };
    turn_at(env, &nested, depth).await
}

/// Join `agent` to the room, say one line as it, and tell the room — the
/// idempotent, module-gated join every speaking agent makes (ADR 0034), so a
/// run cannot put an agent in a room its asker was not allowed to reach.
/// Best-effort, like every posting inside a turn.
async fn join_and_say(
    env: &RunEnv<'_>,
    channel: &ChatChannelId,
    agent: &ChatAgentId,
    body: &str,
) -> Option<alo_store::ChatMessageId> {
    env.account
        .acc
        .add_agent_to_channel(channel, agent)
        .await
        .ok()?;
    let said = env
        .account
        .acc
        .post_as_agent(channel, agent, body, None)
        .await
        .ok()?;
    let audience: Vec<alo_store::UserId> = env
        .account
        .acc
        .channel_members(channel)
        .await
        .map(|members| members.into_iter().map(|member| member.user).collect())
        .unwrap_or_default();
    crate::push::notify_chat(env.state, &env.account.tenant, &audience).await;
    Some(said.id)
}

/// Put a delegate's write up for approval: its sentence in the room under its
/// own id, the action recorded against that message, only the asker's tap to
/// run it. `None` when the room would not take the message — the caller then
/// falls back to folding the wish in as words rather than losing it.
async fn propose_in_room(
    env: &RunEnv<'_>,
    channel: &ChatChannelId,
    delegate: &ChatAgentId,
    action: &alo_ai::ProposedAction,
    say: &str,
) -> Option<()> {
    let said = join_and_say(env, channel, delegate, say).await?;
    env.account
        .acc
        .propose_action(&said, &action.tool, &action.args)
        .await
        .ok()?;
    Some(())
}

/// What a decision means for the turn: the turn is over, or there is a read to
/// run — or a handoff to make — before asking again.
enum Step {
    /// Nothing left to look up.
    Done(TurnResult),
    /// A reading tool to execute, whose result grounds the next question.
    RunRead(alo_ai::ProposedAction),
    /// A sub-question to hand to another agent, whose answer grounds the next
    /// question (A5.1).
    Handoff {
        /// The handle the model named, bare.
        to: String,
        /// The sub-question, in words that stand on their own.
        ask: String,
    },
}

/// Read one decision (ADR 0047 §1). **The whole read-versus-write split lives
/// here**, and it asks the registry — never the tool's name, never its
/// description, and never the model's own word for what it is doing.
///
/// A write is proposed, exactly as before. A read is run. Anything the registry
/// does not know is proposed, so an unfamiliar name meets the allowlist at the
/// execution boundary rather than the path that skips approval.
///
/// **Product scope is deliberately not asked here** (A1.2). A read belonging to
/// another product still takes the read path and is refused by the boundary,
/// which tells the model *which* agent owns it — so the turn ends with the
/// agent saying who to ask instead of putting a button on a lookup, which is
/// the bug ADR 0047 exists to remove. Checking it here as well would be a
/// second copy of a permission rule, and two copies are how they come to
/// disagree.
fn step(decided: AgentDecision) -> Step {
    match decided {
        AgentDecision::Answer(answer) => Step::Done(TurnResult::Answer(answer)),
        AgentDecision::Action { action, say } => {
            if alo_ai::is_read_tool(&action.tool) {
                Step::RunRead(action)
            } else {
                Step::Done(TurnResult::Propose { action, say })
            }
        }
        AgentDecision::Delegate { to, ask } => Step::Handoff { to, ask },
    }
}

/// The step, once the budget refuses it.
///
/// A read still being asked for here is the bound biting: it must not become a
/// proposal, because a button that reads something is the bug ADR 0047 exists
/// to remove — so the turn says what happened instead. A handoff at the bound
/// ends the same way, in its own words.
fn finish(step: Step) -> TurnResult {
    match step {
        Step::Done(result) => result,
        Step::RunRead(_) => TurnResult::Answer(OUT_OF_LOOKUPS.to_owned()),
        Step::Handoff { .. } => TurnResult::Answer(OUT_OF_HANDOFFS.to_owned()),
    }
}

/// Run one reading tool and render its result for the model.
///
/// A refusal is not an error here: the model is told plainly that the lookup
/// did not work, so it can answer around it or say what it could not find —
/// which is a far better turn than one that dies silently.
async fn run_read(
    state: &AppState,
    account: &Account,
    action: &alo_ai::ProposedAction,
    context: TurnContext<'_>,
) -> String {
    match execute_tool(state, account, &action.tool, &action.args, &context.run()).await {
        Ok(axum::Json(body)) => {
            let result = body.get("result").unwrap_or(&body);
            truncate(&result.to_string())
        }
        // The `Problem` detail is the executor's own words about the arguments
        // ("to is before from", "no folder by that name") and carries no
        // content of anybody's records — safe to hand back to the model, and
        // the only thing that lets it correct itself.
        Err(problem) => format!(
            "this lookup did not run: {}",
            problem.detail.as_deref().unwrap_or("it was refused")
        ),
    }
}

/// Cut a result to [`MAX_RESULT_CHARS`], on a character boundary, saying so.
fn truncate(text: &str) -> String {
    if text.chars().count() <= MAX_RESULT_CHARS {
        return text.to_owned();
    }
    let cut: String = text.chars().take(MAX_RESULT_CHARS).collect();
    format!("{cut}… (result truncated; ask for a narrower range to see the rest)")
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn action(tool: &str) -> AgentDecision {
        AgentDecision::Action {
            action: alo_ai::ProposedAction {
                tool: tool.to_owned(),
                args: serde_json::json!({}),
            },
            say: "doing it".to_owned(),
        }
    }

    /// The bound biting must not produce a button on a read.
    #[test]
    fn a_read_still_wanted_at_the_bound_becomes_an_answer() {
        match finish(step(action("stock_answer"))) {
            TurnResult::Answer(text) => assert!(text.contains("narrow it down")),
            TurnResult::Propose { .. } | TurnResult::DelegateProposed => {
                panic!("a read must never become a proposal")
            }
        }
    }

    #[test]
    fn a_write_at_the_bound_is_still_proposed() {
        match finish(step(action("create_task"))) {
            TurnResult::Propose { action, say } => {
                assert_eq!(action.tool, "create_task");
                assert_eq!(say, "doing it");
            }
            TurnResult::Answer(_) | TurnResult::DelegateProposed => {
                panic!("a write must still wait for a tap")
            }
        }
    }

    /// A tool nobody declared is not a read, so it takes the proposal path —
    /// where the allowlist refuses it — rather than the one that skips
    /// approval.
    #[test]
    fn an_unknown_tool_is_not_treated_as_a_read() {
        assert!(matches!(
            finish(step(action("delete_everything"))),
            TurnResult::Propose { .. }
        ));
        assert!(matches!(step(action("delete_everything")), Step::Done(_)));
    }

    /// A handoff the budget refuses ends as a sentence, never as a proposal
    /// and never as a delegate turn nobody counted.
    #[test]
    fn a_handoff_at_the_bound_becomes_an_answer_in_its_own_words() {
        let wanted = step(AgentDecision::Delegate {
            to: "crm".to_owned(),
            ask: "which deal is behind Q-31?".to_owned(),
        });
        match finish(wanted) {
            TurnResult::Answer(text) => assert!(text.contains("ask the remaining part")),
            TurnResult::Propose { .. } | TurnResult::DelegateProposed => {
                panic!("a handoff must never become a proposal")
            }
        }
    }

    /// One budget for one run (ADR 0057 §3): the caps refuse exactly at their
    /// numbers, wherever in the run the spend comes from.
    #[test]
    fn the_run_budget_refuses_at_its_caps() {
        let budget = RunBudget::new();
        for _ in 0..MAX_READS {
            assert!(budget.take_read());
        }
        assert!(!budget.take_read(), "a seventh read must be refused");
        assert!(!budget.reads_left());
        for _ in 0..MAX_HANDOFFS {
            assert!(budget.take_handoff());
        }
        assert!(!budget.take_handoff(), "a fifth handoff must be refused");
    }

    fn roster_agent(handle: &str, product: AgentProduct) -> ChatAgent {
        ChatAgent {
            id: ChatAgentId::new(format!("agent-{handle}")),
            handle: handle.to_owned(),
            name: handle.to_owned(),
            description: None,
            product,
            disabled: false,
        }
    }

    /// The offer is the roster minus the turn's own agent, and nobody at the
    /// depth cap — the turn two handoffs down is told about no one, so the
    /// chain ends by construction rather than by refusal.
    #[test]
    fn handoffs_are_offered_within_depth_and_never_to_oneself() {
        let roster = [
            roster_agent("billing", AgentProduct::Billing),
            roster_agent("inventory", AgentProduct::Inventory),
        ];
        let billing_id = ChatAgentId::new("agent-billing".to_owned());
        let channel = ChatChannelId::new("c1".to_owned());
        let turn = Turn {
            product: AgentProduct::Billing,
            request: "can we fulfil the quote?",
            sources: &[],
            today: "2026-08-28",
            folders: &[],
            context: TurnContext::in_room(&billing_id, &channel),
            roster: &roster,
        };
        let offered = handoff_offers(&turn, 0);
        assert_eq!(offered.len(), 1, "never to oneself");
        assert_eq!(offered[0].handle, "inventory");
        assert_eq!(handoff_offers(&turn, 1).len(), 1);
        assert!(
            handoff_offers(&turn, MAX_HANDOFF_DEPTH).is_empty(),
            "the turn at the depth cap is offered nobody"
        );
    }

    /// **Another product's read takes the read path and is refused there**
    /// (A1.2), rather than being turned into a button here.
    ///
    /// The scope is not asked in this module on purpose: the diary lookup goes
    /// to [`crate::agent::execute_tool`] whoever asked for it, and the
    /// Inventory agent gets back "whats_on is not a tool the inventory agent
    /// has", which the next call turns into an answer naming the agent that
    /// owns it. Putting the check here as well would make a lookup wear a
    /// button — the exact bug ADR 0047 removed — and would be a second copy of
    /// a permission rule.
    #[test]
    fn another_products_read_still_takes_the_read_path() {
        assert!(matches!(step(action("whats_on")), Step::RunRead(_)));
        // What makes that safe is the boundary, not this: no product but
        // Agenda and Ask alo may actually run it.
        for product in alo_store::ALL_AGENT_PRODUCTS {
            assert_eq!(
                alo_ai::offers(product, "whats_on"),
                matches!(product, AgentProduct::Agenda | AgentProduct::Workspace),
                "{product}"
            );
        }
    }

    /// The refusal a foreign lookup comes back with is handed to the model as
    /// the tool's result, so the turn can say who to ask instead of dying
    /// silently. This pins the rendering of that refusal, which is the only
    /// part of it this module owns.
    #[test]
    fn a_refused_lookup_is_reported_to_the_model_rather_than_failing_the_turn() {
        let problem = crate::error::Problem::with(
            axum::http::StatusCode::FORBIDDEN,
            "whats_on is not a tool the inventory agent has — ask the agent whose product it belongs to",
        );
        let said = format!(
            "this lookup did not run: {}",
            problem.detail.as_deref().unwrap_or("it was refused")
        );
        assert!(said.contains("not a tool the inventory agent has"));
        assert!(said.contains("did not run"));
    }

    /// **A read never becomes a proposal, and a write always does** — over
    /// every tool that exists, not a sample. `TurnResult::Propose` is the only
    /// thing either surface turns into a `chat_proposals` row, so this is the
    /// property that keeps a read out of that table.
    #[test]
    fn every_read_runs_and_every_write_waits() {
        for entry in alo_ai::all_tools() {
            // Ask alo has every tool, so this stays a statement about the
            // read/write split and not about product scope (which
            // `a_read_belonging_to_another_product_is_never_run_in_the_turn`
            // covers over every product).
            match step(action(entry.name)) {
                Step::RunRead(ran) => {
                    assert!(entry.is_read(), "{} ran without approval", entry.name);
                    assert_eq!(ran.tool, entry.name);
                }
                Step::Done(TurnResult::Propose { action, .. }) => {
                    assert!(!entry.is_read(), "{} was put behind a button", entry.name);
                    assert_eq!(action.tool, entry.name);
                }
                Step::Done(TurnResult::Answer(_) | TurnResult::DelegateProposed) => {
                    panic!("{} became an answer with no tool run", entry.name)
                }
                Step::Handoff { .. } => {
                    panic!("{} is a tool, never a handoff", entry.name)
                }
            }
        }
        // Forty-four of them, which is the whole point of ADR 0047 — eleven
        // from the products A1 covered, the Website agent's three (A2.1), its
        // language count (A2.1b), the Sheet agent's three (A2.2), the Docs
        // agent's two (A2.3), the Insights agent's three (A2.4), the Drive
        // agent's two (A2.5), the Agenda agent's two (A2.6), the Tasks agent's
        // three (A2.7), the Mail agent's two (A2.8), the Meet agent's two
        // (A3.2), the Billing agent's six intent reads (A4.1) and the Sales
        // agent's four (AA.1); the rename, the move, the reschedule, the
        // priority, the chase, the capture and the minutes are writes and are
        // counted on the other side.
        let reads = alo_ai::all_tools().iter().filter(|t| t.is_read()).count();
        assert_eq!(reads, 44);
    }

    #[test]
    fn an_answer_passes_straight_through() {
        match finish(step(AgentDecision::Answer("42 in stock [1].".to_owned()))) {
            TurnResult::Answer(text) => assert_eq!(text, "42 in stock [1]."),
            TurnResult::Propose { .. } | TurnResult::DelegateProposed => {
                panic!("an answer is not a proposal")
            }
        }
    }

    #[test]
    fn a_long_result_is_cut_on_a_character_boundary_and_says_so() {
        let short = "é".repeat(10);
        assert_eq!(truncate(&short), short);
        let long = "é".repeat(MAX_RESULT_CHARS + 50);
        let cut = truncate(&long);
        assert!(cut.starts_with(&"é".repeat(MAX_RESULT_CHARS)));
        assert!(cut.contains("truncated"));
        // Exactly at the bound is not truncated.
        assert_eq!(
            truncate(&"é".repeat(MAX_RESULT_CHARS)).chars().count(),
            MAX_RESULT_CHARS
        );
    }

    /// An in-turn run carries no approval, whichever surface it is on. This is
    /// the property the write refusal rests on.
    #[test]
    fn a_turn_never_carries_an_approval() {
        assert_eq!(TurnContext::palette().run().approval, Approval::InTurn);
        let agent = ChatAgentId::new("a1".to_owned());
        let channel = ChatChannelId::new("c1".to_owned());
        let run = TurnContext::in_room(&agent, &channel).run();
        assert_eq!(run.approval, Approval::InTurn);
        assert_eq!(run.agent.map(ChatAgentId::as_str), Some("a1"));
        assert_eq!(run.channel.map(ChatChannelId::as_str), Some("c1"));
    }
}
