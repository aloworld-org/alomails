//! An agent taking a turn in a conversation (ADR 0034 §chat, ADR 0038;
//! `docs/design/chat-agents.md`).
//!
//! Saying something to an agent is the whole trigger. There is no separate
//! "ask the agent" route, because the trigger *is* the message — a second
//! endpoint would let the two disagree about what was actually said. In a
//! channel that means naming it; in a one-to-one with it (ADR 0048) every
//! message is addressed to it already, so no handle is needed.
//!
//! The turn runs **after** the message is stored and **off the request**. A
//! model call takes seconds, and making someone wait for it before their own
//! words appear would be the worst kind of slow. Chat already has a live
//! stream, so the agent's reply arrives the same way any other message does.
//!
//! Everything inside a turn runs through the **asker's** account door: the
//! retrieval that grounds it, and any action it proposes. The agent supplies a
//! name to answer under and nothing else. That is what makes a hostile
//! instruction in a channel harmless beyond the reach of whoever triggered it.

use serde_json::{Value, json};

use alo_ai::{AiConfig, InferenceError, WorkspaceSource};
use alo_store::{AgentRecord, ChatAgent, ChatChannelId, ChatMessageId, parse_handles};

use crate::agent_turn::{Turn, TurnContext, TurnResult, take_turn as run_turn};
use crate::push;
use crate::state::{Account, AppState};

/// How much of the workspace grounds one chat turn. Matches the command
/// palette's budget: the same question deserves the same evidence wherever it
/// is asked.
///
/// Shared with [`crate::agent_orchestrate`], so a step of an Ask alo plan is
/// grounded exactly as the same question typed at the agent directly would be.
pub(crate) const CHAT_SOURCES: i64 = 8;

/// The agents named in `body`, out of those present in the room.
///
/// Reuses the store's own handle parser, so `@alo` in chat means exactly what
/// `@person` means: a handle at a word boundary, an inline address is not a
/// mention, and trailing punctuation is not part of a name. One parser, one
/// set of surprises.
#[must_use]
pub(crate) fn named_agents(body: &str, present: &[ChatAgent]) -> Vec<ChatAgent> {
    let handles = parse_handles(body);
    present
        .iter()
        .filter(|agent| !agent.disabled && handles.contains(&agent.handle))
        .cloned()
        .collect()
}

/// What the agent decided, already said in the room.
pub(crate) enum Spoken {
    /// It answered; nothing waits on anybody.
    Answered,
    /// It proposed something, and only the asker may approve it.
    Proposed,
    /// It could not answer, and said so once.
    Excused,
    /// A multi-step run was stopped part-way and said so
    /// ([`crate::agent_orchestrate`]). A single turn stopped while it was
    /// thinking says nothing at all instead — there is nothing half-done to
    /// account for.
    Stopped,
}

/// The grounding for one turn: whatever the asker's own access turns up in that
/// product, as numbered sources with no bodies.
///
/// The only thing a turn may ever see, and it is the **asker's** access, never
/// the agent's. Scoped to the product too (A1.3): the Inventory agent is not
/// grounded in eight of the asker's emails, and reaches stock through its own
/// reading tool. Shared with [`crate::agent_orchestrate`] so a delegated step
/// grounds the same way.
pub(crate) async fn ground(
    account: &Account,
    product: alo_store::AgentProduct,
    question: &str,
    limit: i64,
) -> Vec<WorkspaceSource> {
    account
        .acc
        .agent_ground(product, question, limit)
        .await
        .unwrap_or_default()
        .iter()
        .enumerate()
        .map(|(i, h)| WorkspaceSource {
            index: i + 1,
            kind: h.kind.clone(),
            title: h.title.clone(),
            detail: String::new(),
        })
        .collect()
}

/// Run one agent turn and post its result into the room.
///
/// Best-effort throughout: a turn that fails leaves the asker's own message
/// untouched, because their words were already said and are not conditional on
/// a model being reachable.
///
/// `asked` is the message that triggered the turn, when one did — a standing
/// instruction's firing (A7.1) has none, and a turn with no message behind it
/// learns nothing: memory rests on a person's words in the room, and a firing
/// is the author's past words replayed, not new consent to remember from.
pub(crate) async fn take_turn(
    state: &AppState,
    account: &Account,
    channel: &ChatChannelId,
    agent: &ChatAgent,
    question: &str,
    asked: Option<&ChatMessageId>,
    stopped: &std::sync::atomic::AtomicBool,
) -> Option<Spoken> {
    let acc = &account.acc;
    let config = match acc.default_ai_config().await {
        Ok(Some(row)) => AiConfig {
            base_url: row.base_url,
            model: row.model,
            api_key: row.api_key,
            enabled: row.enabled,
        },
        // No provider configured: say so once, in the room, as the agent. A
        // model nobody set up must not make chat look broken, and must not
        // leave a mention hanging with no reply at all.
        _ => {
            let _ = acc
                .post_as_agent(channel, &agent.id, UNCONFIGURED, None)
                .await;
            return Some(Spoken::Excused);
        }
    };

    // A room turn has no browser to ask, so it uses what the person's own
    // sessions have already told us. Unknown stays unknown: the prompt then
    // makes the model declare which hour it assumed.
    let today = {
        let date = time::OffsetDateTime::now_utc().date().to_string();
        match acc.user_timezone().await.unwrap_or_default() {
            Some(zone) => format!(
                "{date}, and the person asking is in the {zone} timezone. Every datetime                  you produce must be an instant that means the time THEY said on THEIR clock."
            ),
            None => format!(
                "{date}. The person's timezone is unknown, so any datetime you produce is                  read as UTC — say which hour you assumed in your `say` line."
            ),
        }
    };
    // **Ask alo orchestrates rather than owns** (ADR 0034, A3.1): it routes the
    // question to the product agents and runs their answers as a visible plan,
    // each step under its own agent's scope. It falls back to the ordinary turn
    // below when there is nobody to route to or the planner cannot be reached —
    // a workspace with one agent in it should still have an assistant.
    if agent.product == alo_store::AgentProduct::Workspace {
        let run = crate::agent_orchestrate::Run {
            channel,
            alo: agent,
            question,
            today: &today,
            config: &config,
            stopped,
        };
        match crate::agent_orchestrate::orchestrate(state, account, &run).await {
            crate::agent_orchestrate::Orchestrated::Ran(spoke) => return spoke,
            crate::agent_orchestrate::Orchestrated::NotRouted => {}
        }
    }

    let mut ground = ground(account, agent.product, question, CHAT_SOURCES).await;
    // What this agent remembers here joins the numbered sources (A6.2): the
    // room's own memories in a room, what it remembers about the asker in its
    // one-to-one — the same scope the learning below feeds, read back.
    let recalled =
        crate::chat_agent_memory::remembered(account, &agent.id, channel, ground.len()).await;
    ground.extend(recalled);
    // Who this agent may hand a sub-question to (A5.1): the same module-gated
    // roster orchestration routes over, which is what keeps a handoff inside
    // what the asker can see. Ask alo gets none here — its delegation path is
    // the planner above, and this turn is only its fallback.
    let roster = if agent.product == alo_store::AgentProduct::Workspace {
        Vec::new()
    } else {
        crate::agent_orchestrate::roster(account, agent).await
    };
    // The turn, with its reading tools run inside it (ADR 0047): asking what is
    // in stock comes back as the figure, and only a change comes back as
    // something to approve.
    let turn = Turn {
        // The agent's own product (A1.2): it is offered its product's tools
        // and told it is that product's agent. The refusal of every other
        // product's tools happens at the execution boundary, which reads this
        // same value off the agent's row rather than taking it from here.
        product: agent.product,
        request: question,
        sources: &ground,
        today: &today,
        folders: &[],
        context: TurnContext::in_room(&agent.id, channel),
        roster: &roster,
    };
    let decided = run_turn(state, account, &config, &turn).await;
    // Stopped while it was thinking: the call cannot be un-made, but its words
    // can be kept out of the room, which is what someone pressing Stop wants.
    if stopped.load(std::sync::atomic::Ordering::SeqCst) {
        return None;
    }
    match decided {
        Ok((TurnResult::Answer(answer), read)) => {
            acc.post_as_agent(channel, &agent.id, &answer, None)
                .await
                .ok()?;
            // The answer is already said; whether the exchange taught the room
            // anything is a side effect behind the room's own switch (A6.1),
            // and never a reason the answer could fail.
            if let Some(asked) = asked {
                crate::chat_agent_memory::learn_from_turn(
                    account,
                    &config,
                    channel,
                    agent,
                    &crate::chat_agent_memory::Exchange {
                        question,
                        answer: &answer,
                        read: &read,
                        source: asked,
                    },
                )
                .await;
            }
            Some(Spoken::Answered)
        }
        Ok((TurnResult::Propose { action, say }, _)) => {
            // The sentence goes in the room so everyone can read what was
            // proposed; the action is recorded against that message, and only
            // the asker's tap can run it.
            let said = acc
                .post_as_agent(channel, &agent.id, &say, None)
                .await
                .ok()?;
            acc.propose_action(&said.id, &action.tool, &action.args)
                .await
                .ok()?;
            Some(Spoken::Proposed)
        }
        // A delegate of this run proposed (A5.2): its sentence and its
        // proposal are already in the room under its own id — the run's one
        // approval surface — and this agent has nothing left to say.
        Ok((TurnResult::DelegateProposed, _)) => Some(Spoken::Proposed),
        Err(InferenceError::Disabled | InferenceError::NotConfigured) => {
            let _ = acc
                .post_as_agent(channel, &agent.id, UNCONFIGURED, None)
                .await;
            Some(Spoken::Excused)
        }
        Err(_) => {
            let _ = acc
                .post_as_agent(channel, &agent.id, UNREACHABLE, None)
                .await;
            Some(Spoken::Excused)
        }
    }
}

/// Said in the room when no model is configured. Plain, and not an apology:
/// it names what is missing and who can fix it.
pub(crate) const UNCONFIGURED: &str = "I can't answer yet — no AI provider is set up for this workspace. An admin can add one in Settings.";

/// Said when the provider could not be reached. Deliberately says nothing
/// about why: the reason is an operator's business, not a room's.
pub(crate) const UNREACHABLE: &str =
    "I couldn't reach the model just now. Try me again in a moment.";

/// The agents a message is addressed to, which depends on the room it was said
/// in.
///
/// In a one-to-one with an agent there is nobody else it could be for, so the
/// room's own counterpart answers whatever was typed — ADR 0048's "every
/// message from the human is the trigger". Everywhere else a handle is what
/// addresses an agent, and a message naming none is not a question for one.
///
/// A retired agent answers in neither: `named_agents` already filters it out,
/// and an agent DM whose counterpart has since been switched off simply stays
/// readable, which is the rule `add_agent_to_channel` applies too.
async fn asked_agents(
    account: &Account,
    channel: &ChatChannelId,
    body: &str,
    present: &[ChatAgent],
) -> Vec<ChatAgent> {
    match account.acc.channel_agent_counterpart(channel).await {
        Ok(Some(counterpart)) => present
            .iter()
            .find(|agent| !agent.disabled && agent.id.as_str() == counterpart.as_str())
            .cloned()
            .into_iter()
            .collect(),
        // Not a one-to-one — or a room this caller cannot see, in which case
        // they have nothing to be answered anyway.
        _ => named_agents(body, present),
    }
}

/// Answer every agent a message was addressed to, in the background.
///
/// Only ever called with a **person's** message: an agent's own words are
/// posted through `post_as_agent`, which does not come this way. That is what
/// keeps two agents from being arranged into a loop, and an agent from
/// answering itself.
///
/// Spawned rather than awaited: the asker's message is already stored and
/// already delivered, and nothing about it should wait on inference. The reply
/// reaches the room over the push stream the client is already holding open.
pub(crate) fn answer_if_asked(
    state: &AppState,
    account: &Account,
    channel: &ChatChannelId,
    body: &str,
    message: &ChatMessageId,
) {
    let state = state.clone();
    let account = account.clone();
    let tenant = account.tenant.clone();
    let channel = channel.clone();
    let body = body.to_owned();
    let message = message.clone();
    tokio::spawn(async move {
        let acc = account.acc.clone();
        let Ok(present) = acc.channel_agents(&channel).await else {
            return;
        };
        for agent in asked_agents(&account, &channel, &body, &present).await {
            // "Remember that …" is an instruction, not a question (A6.1): the
            // fact is stored and confirmed with no model call, and no turn to
            // register — there is nothing to think about and nothing to stop.
            if let Some(fact) = crate::chat_agent_memory::explicit_fact(&body) {
                let spoke = crate::chat_agent_memory::remember_explicit(
                    &account, &channel, &agent, fact, &message,
                )
                .await;
                if spoke.is_some() {
                    let users: Vec<alo_store::UserId> = acc
                        .channel_members(&channel)
                        .await
                        .map(|m| m.into_iter().map(|m| m.user).collect())
                        .unwrap_or_default();
                    push::notify_chat(&state, &tenant, &users).await;
                }
                continue;
            }
            // Registered before the call so the room can say who is thinking,
            // and forgotten afterwards however it ended.
            let (id, stopped) = state.turns.begin(
                &tenant,
                &channel,
                agent.id.as_str(),
                &agent.handle,
                acc.user().as_str(),
            );
            push::notify_chat(&state, &tenant, &[acc.user().clone()]).await;
            let spoke = take_turn(
                &state,
                &account,
                &channel,
                &agent,
                &body,
                Some(&message),
                &stopped,
            )
            .await;
            state.turns.end(&tenant, &channel, &id);
            if spoke.is_some() {
                // Tell the room its shape changed, exactly as a person's
                // message does.
                let users: Vec<alo_store::UserId> = acc
                    .channel_members(&channel)
                    .await
                    .map(|m| m.into_iter().map(|m| m.user).collect())
                    .unwrap_or_default();
                push::notify_chat(&state, &tenant, &users).await;
            }
        }
    });
}

/// The wire shape of an agent, with what it has actually done.
///
/// The record is counted only over rooms the caller can see, so two people
/// can legitimately be shown different numbers for the same agent — that is
/// the same rule the rest of chat follows, not an inconsistency.
#[must_use]
pub(crate) fn agent_json(a: &ChatAgent, record: Option<&AgentRecord>) -> Value {
    json!({
        "id": a.id.as_str(),
        "handle": a.handle,
        "name": a.name,
        "description": a.description,
        // What it is the agent *of* (ADR 0034, A1.2) — the same word the
        // rail uses for the module, so a client can put an agent beside its
        // product without a second mapping.
        "product": a.product.as_str(),
        "disabled": a.disabled,
        "answers": record.map_or(0, |r| r.answers),
        "actions": record.map_or(0, |r| r.actions),
        // What it looked up for this person, with nobody's approval (ADR 0047
        // §4). Without it a third of an agent's work would be invisible here.
        "reads": record.map_or(0, |r| r.reads),
        "lastAt": record.and_then(|r| r.last_at).map(|at| {
            at.format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default()
        }),
    })
}
