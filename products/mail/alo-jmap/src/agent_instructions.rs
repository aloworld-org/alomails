//! Standing instructions on the wire and on the clock (ADR 0057,
//! `docs/design/complete-agents.md` §7, queue item A7.1).
//!
//! A person asks once, in advance: `POST /chat/channels/{id}/instructions`
//! stands the ask up in the room — the agent to run it, the words to run,
//! and the trigger (a schedule, or a module event the intent registry
//! names). `GET` lists the room's cards; `DELETE /chat/instructions/{id}` is
//! Cancel, for the author and the room's owner. None of the three is an
//! agent verb: an agent must not commission itself, and Cancel is a person's
//! brake on one.
//!
//! **Each firing is a turn with the author as asker** — [`run_due`], on the
//! same background sweeper that submits scheduled mail. The claimed
//! instruction's text runs verbatim through [`crate::chat_agent::take_turn`]
//! under the author's own account door: reads post into the room, writes
//! propose to the author (the proposal's `asked_by` is the account that
//! proposed it, which here IS the author). A firing with no AI provider
//! configured is skipped quietly rather than posting the "not configured"
//! excuse into the room on a clock.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use serde_json::{Value, json};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use alo_store::{
    AgentInstruction, AgentInstructionId, AgentProduct, ChatAgentId, ChatChannelId, DueInstruction,
    InstructionTrigger, MemberRole, StoreError, UserId,
};

use crate::chat_agent_routes::map_store_err;
use crate::error::Problem;
use crate::push;
use crate::state::{Account, AppState, authenticate};

/// How many firings one sweep takes on. The sweeper ticks every half minute,
/// so a backlog beyond this is caught up within minutes rather than starving
/// the tick that found it.
const FIRINGS_PER_SWEEP: i64 = 50;

fn instruction_json(row: &AgentInstruction, owner_like: bool, caller: &str) -> Value {
    let trigger = match &row.trigger {
        InstructionTrigger::Schedule { every_minutes } => {
            json!({ "kind": "schedule", "everyMinutes": every_minutes })
        }
        InstructionTrigger::Event { kind } => json!({ "kind": "event", "event": kind }),
    };
    json!({
        "id": row.id.as_str(),
        "agentId": row.agent.as_str(),
        "agentHandle": row.agent_handle,
        "text": row.text,
        "trigger": trigger,
        "nextRun": row.next_run.map(|at| at.format(&Rfc3339).unwrap_or_default()),
        "lastFiredAt": row.last_fired_at.map(|at| at.format(&Rfc3339).unwrap_or_default()),
        // The card says paused — set when the author left the room.
        "paused": row.paused_at.is_some(),
        "author": row.author_email,
        "createdAt": row.created_at.format(&Rfc3339).unwrap_or_default(),
        // Answered server-side so the card offers only the Cancel the server
        // will honour: the author's own, or the room owner's.
        "canCancel": owner_like || row.author.as_str() == caller,
    })
}

/// Whether this caller cancels ANY instruction in this room: its owner, or
/// either side of a direct room. Everyone else cancels only their own.
async fn owner_like(account: &Account, channel: &ChatChannelId) -> Result<bool, Problem> {
    let room = account.acc.channel(channel).await.map_err(map_store_err)?;
    Ok(
        match account
            .acc
            .channel_role(channel)
            .await
            .map_err(map_store_err)?
        {
            Some(MemberRole::Owner) => true,
            Some(MemberRole::Member) => room.kind.is_direct(),
            None => false,
        },
    )
}

/// The trigger a request body asks for, plus the schedule's first firing.
fn parse_trigger(body: &Value) -> Result<(InstructionTrigger, Option<OffsetDateTime>), Problem> {
    let trigger = body
        .get("trigger")
        .ok_or_else(|| Problem::with(StatusCode::UNPROCESSABLE_ENTITY, "trigger is required"))?;
    match trigger.get("kind").and_then(Value::as_str) {
        Some("schedule") => {
            let minutes = trigger
                .get("everyMinutes")
                .and_then(Value::as_i64)
                .and_then(|m| i32::try_from(m).ok())
                .ok_or_else(|| {
                    Problem::with(
                        StatusCode::UNPROCESSABLE_ENTITY,
                        "a schedule needs everyMinutes, a whole number of minutes",
                    )
                })?;
            let first_at = match trigger.get("firstAt") {
                None | Some(Value::Null) => None,
                Some(Value::String(at)) => {
                    Some(OffsetDateTime::parse(at, &Rfc3339).map_err(|_| {
                        Problem::with(
                            StatusCode::UNPROCESSABLE_ENTITY,
                            "firstAt must be an RFC 3339 instant",
                        )
                    })?)
                }
                Some(_) => {
                    return Err(Problem::with(
                        StatusCode::UNPROCESSABLE_ENTITY,
                        "firstAt must be an RFC 3339 instant",
                    ));
                }
            };
            Ok((
                InstructionTrigger::Schedule {
                    every_minutes: minutes,
                },
                first_at,
            ))
        }
        Some("event") => {
            let kind = trigger
                .get("event")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|kind| !kind.is_empty())
                .ok_or_else(|| {
                    Problem::with(
                        StatusCode::UNPROCESSABLE_ENTITY,
                        "an event trigger needs event, the verb's name",
                    )
                })?;
            // A module event the intent registry names (design §7): the
            // registry is the vocabulary, and a verb nobody registered would
            // be a trigger that can never fire. Ask alo's set is every
            // module's, so one question covers the whole registry.
            if !alo_ai::offers(AgentProduct::Workspace, kind) {
                return Err(Problem::with(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    format!("the intent registry names no verb '{kind}'"),
                ));
            }
            Ok((
                InstructionTrigger::Event {
                    kind: kind.to_owned(),
                },
                None,
            ))
        }
        _ => Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "trigger.kind must be 'schedule' or 'event'",
        )),
    }
}

/// `POST /chat/channels/{id}/instructions` — stand an instruction up in the
/// room, this caller as author. Body: `{ "agentId": …, "text": …, "trigger":
/// { "kind": "schedule", "everyMinutes": …, "firstAt"?: … } | { "kind":
/// "event", "event": … } }`.
pub async fn create_instruction(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let v: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let agent = v
        .get("agentId")
        .and_then(Value::as_str)
        .filter(|agent| !agent.is_empty())
        .ok_or_else(|| Problem::with(StatusCode::UNPROCESSABLE_ENTITY, "agentId is required"))?;
    let text = v.get("text").and_then(Value::as_str).unwrap_or_default();
    let (trigger, first_at) = parse_trigger(&v)?;
    let channel = ChatChannelId::new(id);
    let made = account
        .acc
        .create_agent_instruction(
            &ChatAgentId::new(agent.to_owned()),
            &channel,
            text,
            &trigger,
            first_at,
        )
        .await
        .map_err(map_store_err)?;
    let owner_like = owner_like(&account, &channel).await?;
    Ok(Json(instruction_json(
        &made,
        owner_like,
        account.acc.user().as_str(),
    )))
}

/// `GET /chat/channels/{id}/instructions` — the room's cards, in the order
/// they were made. Readable by everyone who can read the room.
pub async fn list_instructions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let channel = ChatChannelId::new(id);
    let rows = account
        .acc
        .channel_instructions(&channel)
        .await
        .map_err(map_store_err)?;
    let owner_like = owner_like(&account, &channel).await?;
    let caller = account.acc.user().as_str();
    Ok(Json(json!({
        "instructions": rows
            .iter()
            .map(|row| instruction_json(row, owner_like, caller))
            .collect::<Vec<_>>(),
    })))
}

/// `DELETE /chat/instructions/{id}` — Cancel, for the author and the room's
/// owner. 204 when cancelled.
pub async fn cancel_instruction(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .cancel_agent_instruction(&AgentInstructionId::new(id))
        .await
        .map_err(|e| match e {
            StoreError::Forbidden => Problem::with(
                StatusCode::FORBIDDEN,
                "only the author, or the room's owner, can cancel a standing instruction",
            ),
            other => map_store_err(other),
        })?;
    Ok(StatusCode::NO_CONTENT)
}

/// Fire every instruction that has come due — called on the scheduled-mail
/// sweeper's tick, the same claim-then-act shape as scheduled sends. Each
/// claimed row runs one ordinary agent turn as its author; a firing that
/// cannot run (author's view lost the agent, no provider) is skipped, its
/// claim already stamped, and the next due moment tries again.
pub async fn run_due(state: &AppState) {
    let due = match state.store.claim_due_instructions(FIRINGS_PER_SWEEP).await {
        Ok(due) => due,
        Err(error) => {
            tracing::warn!(%error, "instruction sweep: could not claim due instructions");
            return;
        }
    };
    for firing in due {
        fire(state, &firing).await;
    }
}

/// One firing: the author's account door, the room's own agent, one turn.
async fn fire(state: &AppState, due: &DueInstruction) {
    let acc = state
        .store
        .for_account(due.tenant.clone(), due.author.clone());
    let facts = acc.access_facts().await.unwrap_or_default();
    let account = Account {
        tenant: due.tenant.clone(),
        user: due.author.clone(),
        acc,
        is_admin: facts.is_admin,
        roles: facts.roles,
        denied_modules: facts.denied_modules,
        delegated: None,
    };
    // Through the author's own view of the room (the module gate, A1.5): an
    // author since denied the agent's module has no agent here to run.
    let Ok(present) = account.acc.channel_agents(&due.channel).await else {
        return;
    };
    let Some(agent) = present
        .into_iter()
        .find(|agent| !agent.disabled && agent.id.as_str() == due.agent.as_str())
    else {
        tracing::debug!("instruction firing skipped: the author no longer sees the agent");
        return;
    };
    // No provider: skip quietly. A mention gets the "not configured" excuse
    // because a person is waiting; a clock is not, and posting the excuse
    // hourly would make an unconfigured workspace look haunted.
    match account.acc.default_ai_config().await {
        Ok(Some(provider)) if provider.enabled => {}
        _ => {
            tracing::debug!("instruction firing skipped: no AI provider configured");
            return;
        }
    }
    let (turn, stopped) = state.turns.begin(
        &due.tenant,
        &due.channel,
        agent.id.as_str(),
        &agent.handle,
        due.author.as_str(),
    );
    let spoke = crate::chat_agent::take_turn(
        state,
        &account,
        &due.channel,
        &agent,
        &due.text,
        None,
        &stopped,
    )
    .await;
    state.turns.end(&due.tenant, &due.channel, &turn);
    if spoke.is_some() {
        let users: Vec<UserId> = account
            .acc
            .channel_members(&due.channel)
            .await
            .map(|members| members.into_iter().map(|member| member.user).collect())
            .unwrap_or_default();
        push::notify_chat(state, &due.tenant, &users).await;
    }
}
