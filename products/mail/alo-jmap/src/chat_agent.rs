//! An agent taking a turn in a conversation (ADR 0034 §chat, ADR 0038;
//! `docs/design/chat-agents.md`).
//!
//! Naming an agent in a message is the whole trigger. There is no separate
//! "ask the agent" route, because the trigger *is* the message — a second
//! endpoint would let the two disagree about what was actually said.
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
use alo_store::{AgentRecord, ChatAgent, ChatChannelId, parse_handles};

use crate::agent_turn::{Turn, TurnContext, TurnResult, take_turn as run_turn};
use crate::push;
use crate::state::{Account, AppState};

/// How much of the workspace grounds one chat turn. Matches the command
/// palette's budget: the same question deserves the same evidence wherever it
/// is asked.
const CHAT_SOURCES: i64 = 8;

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
enum Spoken {
    /// It answered; nothing waits on anybody.
    Answered,
    /// It proposed something, and only the asker may approve it.
    Proposed,
    /// It could not answer, and said so once.
    Excused,
}

/// Run one agent turn and post its result into the room.
///
/// Best-effort throughout: a turn that fails leaves the asker's own message
/// untouched, because their words were already said and are not conditional on
/// a model being reachable.
async fn take_turn(
    state: &AppState,
    account: &Account,
    channel: &ChatChannelId,
    agent: &ChatAgent,
    question: &str,
    stopped: &std::sync::atomic::AtomicBool,
) -> Option<Spoken> {
    let acc = &account.acc;
    // Access-scoped retrieval — the only thing this turn may ever see, and it
    // is the asker's access, never the agent's.
    let hits = acc
        .workspace_search_terms(question, CHAT_SOURCES)
        .await
        .unwrap_or_default();
    let ground: Vec<WorkspaceSource> = hits
        .iter()
        .enumerate()
        .map(|(i, h)| WorkspaceSource {
            index: i + 1,
            kind: h.kind.clone(),
            title: h.title.clone(),
            detail: String::new(),
        })
        .collect();

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
    // The turn, with its reading tools run inside it (ADR 0047): asking what is
    // in stock comes back as the figure, and only a change comes back as
    // something to approve.
    let turn = Turn {
        request: question,
        sources: &ground,
        today: &today,
        folders: &[],
        context: TurnContext::in_room(&agent.id, channel),
    };
    let decided = run_turn(state, account, &config, &turn).await;
    // Stopped while it was thinking: the call cannot be un-made, but its words
    // can be kept out of the room, which is what someone pressing Stop wants.
    if stopped.load(std::sync::atomic::Ordering::SeqCst) {
        return None;
    }
    match decided {
        Ok(TurnResult::Answer(answer)) => {
            acc.post_as_agent(channel, &agent.id, &answer, None)
                .await
                .ok()?;
            Some(Spoken::Answered)
        }
        Ok(TurnResult::Propose { action, say }) => {
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
const UNCONFIGURED: &str = "I can't answer yet — no AI provider is set up for this workspace. An admin can add one in Settings.";

/// Said when the provider could not be reached. Deliberately says nothing
/// about why: the reason is an operator's business, not a room's.
const UNREACHABLE: &str = "I couldn't reach the model just now. Try me again in a moment.";

/// Answer every agent named in a message, in the background.
///
/// Spawned rather than awaited: the asker's message is already stored and
/// already delivered, and nothing about it should wait on inference. The reply
/// reaches the room over the push stream the client is already holding open.
pub(crate) fn answer_if_named(
    state: &AppState,
    account: &Account,
    channel: &ChatChannelId,
    body: &str,
) {
    let state = state.clone();
    let account = account.clone();
    let tenant = account.tenant.clone();
    let channel = channel.clone();
    let body = body.to_owned();
    tokio::spawn(async move {
        let acc = account.acc.clone();
        let Ok(present) = acc.channel_agents(&channel).await else {
            return;
        };
        for agent in named_agents(&body, &present) {
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
            let spoke = take_turn(&state, &account, &channel, &agent, &body, &stopped).await;
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
