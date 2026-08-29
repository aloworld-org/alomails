//! Channel memory, on the turn's two paths and on the wire (ADR 0057 §6,
//! `docs/design/complete-agents.md` §6, queue item A6.1).
//!
//! Two ways a fact gets remembered, one place each:
//!
//! - **Explicitly**: a message to an agent that says *"remember that …"* is an
//!   instruction, not a question — the fact is stored verbatim and the agent
//!   confirms, with no model call at all. It works whatever the learning
//!   switches say, because a person asking by name IS the consent the
//!   switches approximate.
//! - **At the end of a turn**: after an agent answers in a room, the exchange
//!   and what the turn read go to one extraction call, and whatever short
//!   facts come back are stored — only where the room's switch (or the
//!   workspace default it falls back to) says learning is on.
//!
//! Scope follows the room: a channel feeds that channel's memory, a
//! one-to-one with an agent feeds only that person's — the store refuses the
//! combination that would cross them.
//!
//! **Retrieval reads the same scope back and nothing wider** (A6.2,
//! [`remembered`]): a turn in a room is grounded in that agent's memories of
//! that room; a turn in the agent's own one-to-one is grounded in what it
//! remembers about the asker; and there is no third case — a delegate taking
//! a turn inside somebody else's one-to-one reads nothing, because the only
//! memories in scope there belong to the room's own counterpart. A room
//! switched off hides its memories from retrieval rather than deleting them
//! (the design's "off hides") — until the switch has been off thirty days,
//! when the store's background sweep deletes what it hides. **Deletion
//! follows the source** (A6.3) on the other three paths too, synchronously in
//! the store: a withdrawn message takes the facts learned from it, an
//! archived room takes its channel memories, and an agent removed from a room
//! takes what it learned there.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use serde_json::{Value, json};
use time::format_description::well_known::Rfc3339;

use alo_ai::{AiConfig, WorkspaceSource};
use alo_store::{
    AgentMemory, AgentMemoryId, ChatAgent, ChatAgentId, ChatChannelId, ChatMessageId, MemberRole,
    MemoryLearnedFrom, StoreError,
};

use crate::chat_agent_routes::map_store_err;
use crate::error::Problem;
use crate::state::{Account, AppState, authenticate};

/// Said in the room when a fact was stored. Hardcoded English like every
/// room-posted agent string — the one standing i18n debt, tracked as one item.
const REMEMBERED: &str = "Noted — I'll remember that.";

/// The instruction prefix. Deliberately a fixed phrase rather than a model's
/// judgement: "remember that …" must work with no provider configured, and a
/// phrase the docs can state exactly is one a person can rely on.
const REMEMBER_PREFIX: &str = "remember that";

/// The fact in an explicit "remember that …" message, if that is what this is.
///
/// Leading agent mentions are stripped (the mention is how the message reached
/// the agent at all; in a one-to-one there is none), then the prefix is
/// matched case-insensitively at the start — a "remember that" buried
/// mid-sentence is conversation, not an instruction. What follows the prefix
/// is the fact, verbatim.
pub(crate) fn explicit_fact(body: &str) -> Option<&str> {
    let mut text = body.trim_start();
    while let Some(rest) = text.strip_prefix('@') {
        let end = rest.find(char::is_whitespace)?;
        text =
            rest[end..].trim_start_matches(|c: char| c.is_whitespace() || matches!(c, ',' | ':'));
    }
    let head = text.get(..REMEMBER_PREFIX.len())?;
    if !head.eq_ignore_ascii_case(REMEMBER_PREFIX) {
        return None;
    }
    let rest = &text[REMEMBER_PREFIX.len()..];
    // The phrase ends at a word boundary: "remember thatcher…" is not it.
    if rest
        .chars()
        .next()
        .is_some_and(|c| !c.is_whitespace() && !matches!(c, ',' | ':'))
    {
        return None;
    }
    let fact = rest
        .trim_start_matches(|c: char| c.is_whitespace() || matches!(c, ',' | ':'))
        .trim_end();
    (!fact.is_empty()).then_some(fact)
}

/// Store an explicit fact and confirm in the room, as the agent.
///
/// A one-to-one with the agent feeds the asker's own memory; anywhere else
/// feeds the room's. A fact the store refuses (transcript-length) is answered
/// with the store's own words — they name the rule and echo nobody's records.
pub(crate) async fn remember_explicit(
    account: &Account,
    channel: &ChatChannelId,
    agent: &ChatAgent,
    fact: &str,
    source: &ChatMessageId,
) -> Option<crate::chat_agent::Spoken> {
    let acc = &account.acc;
    let stored = match acc.channel_agent_counterpart(channel).await.ok()? {
        Some(_) => {
            acc.remember_for_me(&agent.id, fact, Some(source), MemoryLearnedFrom::Explicit)
                .await
        }
        None => {
            acc.remember_in_channel(
                &agent.id,
                channel,
                fact,
                Some(source),
                MemoryLearnedFrom::Explicit,
            )
            .await
        }
    };
    let say = match stored {
        Ok(_) => REMEMBERED.to_owned(),
        Err(StoreError::Validation(rule)) => format!("I couldn't remember that: {rule}"),
        Err(_) => return None,
    };
    acc.post_as_agent(channel, &agent.id, &say, None)
        .await
        .ok()?;
    Some(crate::chat_agent::Spoken::Answered)
}

/// How many memories ground one turn: the newest of the scope, not all two
/// hundred it may hold. Matches the grounding budget
/// ([`crate::chat_agent::CHAT_SOURCES`]) — what an agent remembers should
/// inform a turn, not crowd out the question.
const MEMORY_SOURCES: usize = 8;

/// What one turn remembers, as further numbered sources (A6.2).
///
/// The scope is the turn's and nothing wider: a room turn reads `agent`'s
/// memories **of that room**; a turn in `agent`'s own one-to-one reads what it
/// remembers **about the asker** (the store binds that read to the caller's
/// id, so there is no argument with which to reach a colleague's); and a turn
/// some other agent takes inside a one-to-one — a delegate handed a
/// sub-question there — reads nothing, because no memory in that room is its.
/// A room whose memory switch resolves off surfaces nothing: off hides.
///
/// Best-effort by design, like the grounding it joins: a turn that cannot
/// read its memories still answers, from the sources it has.
pub(crate) async fn remembered(
    account: &Account,
    agent: &ChatAgentId,
    channel: &ChatChannelId,
    from: usize,
) -> Vec<WorkspaceSource> {
    let acc = &account.acc;
    if !acc.memory_enabled(channel).await.unwrap_or(false) {
        return Vec::new();
    }
    let rows = match acc.channel_agent_counterpart(channel).await {
        Ok(Some(counterpart)) if counterpart.as_str() == agent.as_str() => {
            acc.my_memories(agent).await
        }
        Ok(Some(_)) => return Vec::new(),
        Ok(None) => acc.channel_memories(agent, channel).await,
        Err(_) => return Vec::new(),
    }
    .unwrap_or_default();
    rows.into_iter()
        .take(MEMORY_SOURCES)
        .enumerate()
        .map(|(i, memory)| WorkspaceSource {
            index: from + i + 1,
            kind: "remembered".to_owned(),
            title: memory.fact,
            detail: String::new(),
        })
        .collect()
}

/// One answered exchange, as the extractor is shown it.
pub(crate) struct Exchange<'a> {
    /// What the person asked, in their own words.
    pub question: &'a str,
    /// What the agent answered in the room.
    pub answer: &'a str,
    /// What the turn read to get there — grounding, tool results, folded-in
    /// delegate answers.
    pub read: &'a [WorkspaceSource],
    /// The asker's message — what the stored facts cite as their source.
    pub source: &'a ChatMessageId,
}

/// Learn from one answered turn, where the room's switch allows it.
///
/// Best-effort by design: learning is a side effect of an answer that has
/// already been said, and no failure here — switch lookup, extraction,
/// storage — may surface in the room or undo the turn.
pub(crate) async fn learn_from_turn(
    account: &Account,
    config: &AiConfig,
    channel: &ChatChannelId,
    agent: &ChatAgent,
    exchange: &Exchange<'_>,
) {
    let acc = &account.acc;
    if !acc.memory_enabled(channel).await.unwrap_or(false) {
        return;
    }
    let Ok(facts) =
        alo_ai::extract_memories(config, exchange.question, exchange.answer, exchange.read).await
    else {
        return;
    };
    let counterpart = acc.channel_agent_counterpart(channel).await.ok().flatten();
    for fact in facts {
        let _ = match counterpart {
            Some(_) => {
                acc.remember_for_me(
                    &agent.id,
                    &fact,
                    Some(exchange.source),
                    MemoryLearnedFrom::Turn,
                )
                .await
            }
            None => {
                acc.remember_in_channel(
                    &agent.id,
                    channel,
                    &fact,
                    Some(exchange.source),
                    MemoryLearnedFrom::Turn,
                )
                .await
            }
        };
    }
}

/// The wire shape of one remembered fact (A6.4). `canForget` is answered here,
/// where the rule lives, so the screen offers only buttons the server will
/// honour: the store's [`alo_store::AccountStore::forget_memory`] makes the
/// same judgement when the button is pressed.
fn memory_json(memory: &AgentMemory, forgets_any: bool, caller: &str) -> Value {
    let mine = memory
        .source_author
        .as_ref()
        .is_some_and(|author| author.as_str() == caller);
    json!({
        "id": memory.id.as_str(),
        "fact": memory.fact,
        "learnedFrom": memory.learned_from.as_str(),
        "createdAt": memory.created_at.format(&Rfc3339).unwrap_or_default(),
        "canForget": forgets_any || mine,
    })
}

/// `GET /chat/channels/{id}/agents/{agent}/memories` — what this agent
/// remembers here, for the **What I remember** panel (A6.4).
///
/// Readable by everyone who can read the room, exactly like the messages the
/// facts were learned from. The room decides the scope: the agent's own
/// one-to-one surfaces what it remembers about the caller, any other room its
/// channel memories — and another agent's one-to-one holds nothing of this
/// agent's, so the list is empty there. Shown whatever the learning switch
/// says: the switch hides memories from *turns* (A6.2), and this panel is how
/// a person sees what a switched-off room is still holding.
pub async fn agent_memories(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, agent)): Path<(String, String)>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let acc = &account.acc;
    let channel = ChatChannelId::new(id);
    let agent = ChatAgentId::new(agent);
    let counterpart = acc
        .channel_agent_counterpart(&channel)
        .await
        .map_err(map_store_err)?;
    let rows = match &counterpart {
        Some(mine) if mine.as_str() == agent.as_str() => acc.my_memories(&agent).await,
        Some(_) => Ok(Vec::new()),
        None => acc.channel_memories(&agent, &channel).await,
    }
    .map_err(map_store_err)?;
    // Who forgets ANY fact here: the person a one-to-one's memories are about,
    // a named room's owner, either side of a direct room. Everyone else only
    // the facts learned from their own words.
    let forgets_any = if counterpart.is_some() {
        true
    } else {
        let room = acc.channel(&channel).await.map_err(map_store_err)?;
        match acc.channel_role(&channel).await.map_err(map_store_err)? {
            Some(MemberRole::Owner) => true,
            Some(MemberRole::Member) => room.kind.is_direct(),
            None => false,
        }
    };
    let caller = acc.user().as_str();
    Ok(Json(json!({
        "memories": rows
            .iter()
            .map(|memory| memory_json(memory, forgets_any, caller))
            .collect::<Vec<_>>(),
    })))
}

/// `DELETE /chat/memories/{id}` — forget one fact by hand (A6.4). The store
/// holds the rule: the room's owner (either side of a one-to-one), or the
/// author of the message the fact was learned from. 204 when forgotten.
pub async fn forget_memory(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .forget_memory(&AgentMemoryId::new(id))
        .await
        .map_err(|e| match e {
            StoreError::Forbidden => Problem::with(
                StatusCode::FORBIDDEN,
                "only the room's owner, or the person whose words taught it, can forget this",
            ),
            other => map_store_err(other),
        })?;
    Ok(StatusCode::NO_CONTENT)
}

/// The wire shape of a room's memory switch: what it resolves to, the room's
/// own override (null = follows the default), and the default it would follow.
async fn switch_json(account: &Account, channel: &ChatChannelId) -> Result<Value, Problem> {
    let setting = account
        .acc
        .channel_memory_setting(channel)
        .await
        .map_err(map_store_err)?;
    let default = account
        .acc
        .workspace_memory_default()
        .await
        .map_err(map_store_err)?;
    Ok(json!({
        "enabled": setting.unwrap_or(default),
        "override": setting,
        "workspaceDefault": default,
    }))
}

/// `GET /chat/channels/{id}/memory` — the room's learning switch, resolved.
/// Any member may read it: the switch decides what happens to their own words.
pub async fn channel_memory(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let id = ChatChannelId::new(id);
    Ok(Json(switch_json(&account, &id).await?))
}

/// `POST /chat/channels/{id}/memory` — set the room's switch. Body
/// `{ "enabled": true | false | null }`; null returns the room to the
/// workspace default. Owners only in a named room; either side in a
/// one-to-one.
pub async fn set_channel_memory(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let v: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let enabled = match v.get("enabled") {
        Some(Value::Bool(chosen)) => Some(*chosen),
        Some(Value::Null) => None,
        _ => {
            return Err(Problem::with(
                axum::http::StatusCode::UNPROCESSABLE_ENTITY,
                "enabled must be true, false, or null (follow the workspace default)",
            ));
        }
    };
    let id = ChatChannelId::new(id);
    account
        .acc
        .set_channel_memory(&id, enabled)
        .await
        .map_err(map_store_err)?;
    Ok(Json(switch_json(&account, &id).await?))
}

/// `GET /admin/agent-memory` — the workspace's learning default (admin, like
/// every `/admin/*` route).
pub async fn memory_default(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_admin()?;
    let enabled = account
        .acc
        .workspace_memory_default()
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "enabled": enabled })))
}

/// `POST /admin/agent-memory` — set the workspace default. Body
/// `{ "enabled": true | false }`. Rooms that chose for themselves keep their
/// choice; every other room follows.
pub async fn set_memory_default(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_admin()?;
    let v: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let Some(enabled) = v.get("enabled").and_then(Value::as_bool) else {
        return Err(Problem::with(
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            "enabled must be true or false",
        ));
    };
    account
        .acc
        .set_workspace_memory_default(enabled)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "enabled": enabled })))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    /// The phrase is an instruction only at the start of what was said to the
    /// agent — after its mention, in any case, with or without a separator.
    #[test]
    fn the_instruction_is_recognised_at_the_start_and_only_there() {
        assert_eq!(
            explicit_fact("@billing remember that Northstar invoices are net 30"),
            Some("Northstar invoices are net 30")
        );
        assert_eq!(
            explicit_fact("Remember that: the demo is Fridays"),
            Some("the demo is Fridays"),
            "no mention in a one-to-one, and any case"
        );
        assert_eq!(
            explicit_fact("@billing @inventory remember that the X100 ships from Ghent"),
            Some("the X100 ships from Ghent"),
            "every named agent gets the same instruction"
        );
        assert_eq!(
            explicit_fact("please remember that we close in August"),
            None,
            "mid-sentence is conversation, not an instruction"
        );
        assert_eq!(explicit_fact("@billing remember that   "), None);
        assert_eq!(explicit_fact("@billing"), None);
        assert_eq!(
            explicit_fact("@billing remembering the old days"),
            None,
            "the prefix is a phrase, not a stem"
        );
        assert_eq!(
            explicit_fact("remember thatcher resigned in 1990"),
            None,
            "the phrase ends at a word boundary"
        );
    }
}
