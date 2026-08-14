//! The `/chat/*` routes that concern agents (ADR 0034 §chat;
//! `docs/design/chat-agents.md`).
//!
//! There is deliberately **no** "run the agent" route. Naming an agent in an
//! ordinary message is the trigger, so the two can never disagree about what
//! was said. What is here is the surface around that: which agents exist,
//! which rooms they are in, and deciding what one has proposed.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::{ChatAgentId, ChatChannelId, ChatProposal, ChatProposalId, StoreError};

use crate::chat_agent::agent_json;
use crate::error::Problem;
use crate::push;
use crate::state::{AppState, authenticate};

/// The store's vocabulary on the wire, unchanged from the rest of chat: a
/// thing you may not see is 404, a thing you may see but not do is 403, and a
/// rule you broke is 422 in the store's own words.
fn map_store_err(e: StoreError) -> Problem {
    match e {
        StoreError::NotFound => Problem::with(StatusCode::NOT_FOUND, "not found"),
        StoreError::Forbidden => Problem::with(
            StatusCode::FORBIDDEN,
            // Said plainly, because the proposal is visible to the whole room:
            // there is no secret here, only a permission.
            "only the person who asked can decide this",
        ),
        StoreError::Conflict(msg) | StoreError::Validation(msg) => {
            Problem::with(StatusCode::UNPROCESSABLE_ENTITY, msg)
        }
        _ => Problem::server_error(),
    }
}

/// The wire shape of a proposal. `askedBy` is what a client compares against
/// its own user id to decide whether the buttons are live.
#[must_use]
pub(crate) fn proposal_json(p: &ChatProposal) -> Value {
    json!({
        "id": p.id.as_str(),
        "message": p.message.as_str(),
        "askedBy": p.asked_by.as_str(),
        "tool": p.tool,
        "args": p.args,
        "state": p.state.as_str(),
        "decidedBy": p.decided_by.as_ref().map(alo_store::UserId::as_str),
    })
}

/// `GET /chat/agents` → the agents this tenant has.
///
/// # Errors
/// 401 unauthenticated.
pub async fn list_agents(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let agents = account.acc.agents().await.map_err(map_store_err)?;
    let records = account.acc.agent_records().await.unwrap_or_default();
    Ok(Json(json!({
        "agents": agents
            .iter()
            .map(|a| agent_json(a, records.get(a.id.as_str())))
            .collect::<Vec<_>>()
    })))
}

#[derive(Deserialize)]
pub struct NewAgentBody {
    handle: String,
    name: String,
    description: Option<String>,
}

/// `POST /chat/agents` `{handle, name, description?}` → define an agent.
///
/// # Errors
/// 422 for a bad or taken handle.
pub async fn create_agent(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<NewAgentBody>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let id = account
        .acc
        .create_agent(&body.handle, &body.name, body.description.as_deref())
        .await
        .map_err(map_store_err)?;
    let agent = account.acc.agent(&id).await.map_err(map_store_err)?;
    Ok(Json(agent_json(&agent, None)))
}

/// `GET /chat/channels/{id}/agents` → the agents in a room.
///
/// # Errors
/// 404 when the room is not the caller's to see.
pub async fn list_channel_agents(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let agents = account
        .acc
        .channel_agents(&ChatChannelId::new(id))
        .await
        .map_err(map_store_err)?;
    let records = account.acc.agent_records().await.unwrap_or_default();
    Ok(Json(json!({
        "agents": agents
            .iter()
            .map(|a| agent_json(a, records.get(a.id.as_str())))
            .collect::<Vec<_>>()
    })))
}

#[derive(Deserialize)]
pub struct AgentMemberBody {
    agent: String,
}

/// `POST /chat/channels/{id}/agents` `{agent}` → put an agent in a room.
///
/// # Errors
/// 404 when the room or agent is not the caller's to see, or they are not a
/// member; 422 when the agent is retired.
pub async fn add_channel_agent(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<AgentMemberBody>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let channel = ChatChannelId::new(id);
    account
        .acc
        .add_agent_to_channel(&channel, &ChatAgentId::new(body.agent))
        .await
        .map_err(map_store_err)?;
    let agents = account
        .acc
        .channel_agents(&channel)
        .await
        .map_err(map_store_err)?;
    let records = account.acc.agent_records().await.unwrap_or_default();
    notify(&state, &account, &channel).await;
    Ok(Json(json!({
        "agents": agents
            .iter()
            .map(|a| agent_json(a, records.get(a.id.as_str())))
            .collect::<Vec<_>>()
    })))
}

/// `DELETE /chat/channels/{id}/agents/{agent}` → take an agent out of a room.
/// Its past messages stay: a room's history does not change because somebody
/// left it.
///
/// # Errors
/// 404 when the room is not the caller's, or they are not a member.
pub async fn remove_channel_agent(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, agent)): Path<(String, String)>,
) -> Result<StatusCode, Problem> {
    let account = authenticate(&state, &headers).await?;
    let channel = ChatChannelId::new(id);
    account
        .acc
        .remove_agent_from_channel(&channel, &ChatAgentId::new(agent))
        .await
        .map_err(map_store_err)?;
    notify(&state, &account, &channel).await;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct DecisionBody {
    /// `true` approves and runs it; `false` turns it down.
    approve: bool,
}

/// `POST /chat/proposals/{id}` `{approve}` → decide a proposed action.
///
/// **Only the asker may decide** — the proposal was computed through their
/// access, so approving it as anyone else would run their reach on another
/// person's say-so. Anyone in the room may *see* it, so refusing is a plain
/// 403 with the reason said, not a 404.
///
/// Approving **runs it**, here, in the same request. Recording a decision the
/// client then has to follow up on would let the two drift: a dropped
/// connection between them leaves a proposal marked approved that never
/// happened, which is precisely the disagreement this table exists to prevent.
///
/// The run goes through [`crate::agent::execute_tool`] — the same allowlist and
/// the same dispatcher the command palette uses — so a tool cannot behave one
/// way when approved in chat and another when approved elsewhere.
///
/// Marking comes first and is conditional on still being pending, so two taps
/// cannot both reach the executor.
///
/// # Errors
/// 404 not visible, 403 not the asker, 422 already decided, plus whatever the
/// executor raises for arguments that no longer name something real.
pub async fn decide_proposal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<DecisionBody>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let decided = account
        .acc
        .decide_proposal(&ChatProposalId::new(id), body.approve)
        .await
        .map_err(map_store_err)?;

    let mut result = Value::Null;
    if body.approve {
        // Runs as the asker, because `account` IS the asker: `decide_proposal`
        // has already refused anybody else.
        // The agent whose message carried the proposal, so the run joins the
        // rest of that agent's record (ADR 0047 §4). Best-effort: a message
        // that has gone since must not stop the act the asker just approved.
        let author = account.acc.chat_message(&decided.message).await.ok();
        let agent = author
            .filter(|message| message.author_is_agent)
            .map(|message| ChatAgentId::new(message.author.as_str().to_owned()));
        let run = crate::agent::ToolRun {
            approval: crate::agent::Approval::Asker,
            agent: agent.as_ref(),
            channel: Some(&decided.channel),
        };
        match crate::agent::execute_tool(&state, &account, &decided.tool, &decided.args, &run).await
        {
            Ok(Json(done)) => result = done,
            Err(problem) => {
                // The record says approved and the act failed. Say so rather
                // than swallowing it: the room saw the tap, and a silent
                // failure here is how someone believes a task exists that
                // does not.
                notify(&state, &account, &decided.channel).await;
                return Err(problem);
            }
        }
    }
    // The whole room watched it pending; the whole room sees it settled.
    notify(&state, &account, &decided.channel).await;
    let mut value = proposal_json(&decided);
    if let Some(object) = value.as_object_mut() {
        object.insert("result".to_owned(), result);
    }
    Ok(Json(value))
}

/// Tell a room its shape changed. Best-effort: a write that succeeded is never
/// reported as failed because a notification could not be delivered.
async fn notify(state: &AppState, account: &crate::state::Account, channel: &ChatChannelId) {
    let users: Vec<alo_store::UserId> = account
        .acc
        .channel_members(channel)
        .await
        .map(|m| m.into_iter().map(|m| m.user).collect())
        .unwrap_or_default();
    push::notify_chat(state, &account.tenant, &users).await;
}

/// `GET /chat/channels/{id}/turns` → the agent turns running in this room
/// right now, so a reader can see that something is happening rather than
/// staring at a room that looks idle while a model thinks.
///
/// # Errors
/// 404 when the room is not the caller's to see.
pub async fn list_turns(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let channel = ChatChannelId::new(id);
    // The room decides whether any of this exists for the caller.
    account.acc.channel(&channel).await.map_err(map_store_err)?;
    let running = state.turns.in_room(&account.tenant, &channel);
    Ok(Json(json!({
        "turns": running
            .iter()
            .map(|t| crate::chat_turns::turn_json(t, account.user.as_str()))
            .collect::<Vec<_>>()
    })))
}

/// `POST /chat/channels/{id}/turns/{turn}/stop` → stop a running turn.
///
/// Only the person who asked may stop it, for the same reason only they may
/// approve what it proposes: the turn is running with their access, not the
/// room's.
///
/// A stop that finds nothing still answers 204. The turn may have finished a
/// moment ago or be running on another process; either way what the caller
/// wanted — that turn not continuing — is now true, and a 404 would invite a
/// client to retry something already settled.
///
/// # Errors
/// 404 when the room is not the caller's to see.
pub async fn stop_turn(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, turn)): Path<(String, String)>,
) -> Result<StatusCode, Problem> {
    let account = authenticate(&state, &headers).await?;
    let channel = ChatChannelId::new(id);
    account.acc.channel(&channel).await.map_err(map_store_err)?;
    let _ = state
        .turns
        .stop(&account.tenant, &channel, &turn, account.user.as_str());
    notify(&state, &account, &channel).await;
    Ok(StatusCode::NO_CONTENT)
}
