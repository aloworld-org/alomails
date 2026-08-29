//! The goal on the wire (ADR 0058 §7, A8.3): a room's goals with their
//! progress, and what a settled proposal does to the goal waiting on it.
//!
//! There is deliberately no "create a goal" route and no "advance a goal"
//! route. A goal is made by Ask alo planning and moved by its steps running —
//! the object records the run, it is not a second way to have one. What the
//! wire carries is the reading (the room's card) and the one human verb that
//! is not already a chat surface: deciding the proposal a goal waits behind,
//! which is `POST /chat/proposals/{id}` and lands here through
//! [`proposal_settled`]. Stop is the turn stop while a segment runs, and
//! turning the proposal down while it waits.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use serde_json::{Value, json};

use alo_store::{AgentGoal, ChatChannelId, ChatProposal, GoalEnd, GoalStatus};

use crate::chat_agent_routes::map_store_err;
use crate::error::Problem;
use crate::state::{Account, AppState, authenticate};

/// What became of a settled proposal, as the goal behind it cares: did the
/// step happen, was it refused, or did it fail on execution.
pub(crate) enum ProposalOutcome {
    /// Approved and executed: the goal resumes past the step that proposed.
    Ran,
    /// Turned down: the goal ends, as the room was promised — "turn it down
    /// and I'll leave it there".
    Declined,
    /// Approved but the execution failed: continuing would build the rest of
    /// the plan on a step that did not happen.
    FailedToRun,
}

/// Move the goal waiting on a settled proposal, if there is one.
///
/// Called after every proposal decision — most proposals have no goal behind
/// them and this is one indexed read saying so. Best-effort throughout: the
/// proposal's own outcome was already decided and answered, and a goal
/// bookkeeping failure must not turn an executed action into an error.
pub(crate) async fn proposal_settled(
    state: &AppState,
    account: &Account,
    proposal: &ChatProposal,
    outcome: ProposalOutcome,
) {
    let Ok(Some(goal)) = account.acc.goal_waiting_on(&proposal.id).await else {
        return;
    };
    match outcome {
        ProposalOutcome::Ran => crate::agent_orchestrate::resume_goal(state, account, goal),
        ProposalOutcome::Declined => {
            let _ = account
                .acc
                .finish_goal(
                    &goal.id,
                    GoalEnd::Stopped,
                    Some("the proposal was turned down"),
                )
                .await;
        }
        ProposalOutcome::FailedToRun => {
            let _ = account
                .acc
                .finish_goal(&goal.id, GoalEnd::Failed, Some("the approved step failed"))
                .await;
        }
    }
}

/// `GET /chat/channels/{id}/goals` → the room's goals, newest first, each with
/// its plan and where it stands — the card's data source.
///
/// # Errors
/// 404 when the room is not the caller's to see.
pub async fn list_channel_goals(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let goals = account
        .acc
        .channel_goals(&ChatChannelId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "goals": goals.iter().map(goal_json).collect::<Vec<_>>()
    })))
}

/// One step's state as the card reads it, derived from the goal rather than
/// stored per step — the cursor and the status cannot disagree with a copy
/// that does not exist.
fn step_state(goal: &AgentGoal, index: usize) -> &'static str {
    if index < goal.cursor {
        return "done";
    }
    if index == goal.cursor {
        match goal.status {
            GoalStatus::Waiting => return "waiting",
            GoalStatus::Working => return "working",
            GoalStatus::Done | GoalStatus::Stopped | GoalStatus::Failed => {}
        }
    }
    "pending"
}

/// The wire shape of a goal. `askedBy` is what a client compares against its
/// own user id — the same convention proposals use.
fn goal_json(goal: &AgentGoal) -> Value {
    let rfc3339 = |at: time::OffsetDateTime| {
        at.format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default()
    };
    json!({
        "id": goal.id.as_str(),
        "request": goal.request,
        "agent": goal.agent.as_str(),
        "askedBy": goal.asked_by.as_str(),
        "status": goal.status.as_str(),
        "cursor": goal.cursor,
        "steps": goal
            .steps
            .iter()
            .enumerate()
            .map(|(index, step)| json!({
                "agent": step.agent,
                "ask": step.ask,
                "state": step_state(goal, index),
            }))
            .collect::<Vec<_>>(),
        "proposal": goal.proposal.as_ref().map(alo_store::ChatProposalId::as_str),
        "note": goal.note,
        "createdAt": rfc3339(goal.created_at),
        "updatedAt": rfc3339(goal.updated_at),
    })
}
