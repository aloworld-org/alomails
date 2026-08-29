//! Goals: multi-step work across agents as an object (ADR 0058 §7,
//! `docs/design/complete-agents.md` §8).
//!
//! An orchestrated run used to live only as room messages: the plan was said,
//! the steps ran, and the first write ended everything — "the rest of this
//! waits until you approve that" had nothing behind it, because nothing kept
//! the rest. A goal is that plan kept: what was asked, which agent is asked
//! what, how far it got, and the one proposal it is waiting behind — so an
//! approval can resume it, a refusal can end it, and a room can see where a
//! piece of multi-step work actually stands.
//!
//! Coordination happens **through this object, never through agents talking**:
//! the steps are fixed when the plan is made, progress only ever moves the
//! cursor forward, and the one approval surface is a column, not a convention.
//!
//! **Only the asker moves a goal.** Every step runs at their reach and every
//! proposal is theirs to decide, so the record of that run is theirs to
//! advance, pause and end. Everyone the room admits may *read* it — the card
//! is part of the conversation — which is the same split proposals have.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::{AgentGoalId, ChatAgentId, ChatChannelId, ChatProposalId, UserId};

/// One step of a goal's plan: an agent, and what it is asked. Fixed at
/// creation — the card must show the plan that was announced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalStep {
    /// The handle of the agent this step asks, without the `@`.
    pub agent: String,
    /// The request for that agent, standing on its own.
    pub ask: String,
}

/// Where a goal stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalStatus {
    /// Steps are running right now (or the process running them died — a
    /// working goal with no turn behind it is stale, not sacred).
    Working,
    /// Stopped at a write, waiting behind its one pending proposal.
    Waiting,
    /// Every step ran.
    Done,
    /// A person ended it: Stop mid-run, or the proposal turned down.
    Stopped,
    /// The run could not continue — budget spent, model unreachable, an
    /// approved step that failed. The note says which.
    Failed,
}

impl GoalStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::Waiting => "waiting",
            Self::Done => "done",
            Self::Stopped => "stopped",
            Self::Failed => "failed",
        }
    }

    fn parse(s: &str) -> Result<Self> {
        match s {
            "working" => Ok(Self::Working),
            "waiting" => Ok(Self::Waiting),
            "done" => Ok(Self::Done),
            "stopped" => Ok(Self::Stopped),
            "failed" => Ok(Self::Failed),
            other => Err(StoreError::Validation(format!(
                "{other} is not a goal status"
            ))),
        }
    }
}

/// How a goal ended — the statuses [`AccountStore::finish_goal`] accepts. A
/// type of its own so "finish it as working" is unrepresentable rather than
/// refused at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalEnd {
    /// Every step ran.
    Done,
    /// A person ended it.
    Stopped,
    /// The run could not continue.
    Failed,
}

impl GoalEnd {
    const fn status(self) -> GoalStatus {
        match self {
            Self::Done => GoalStatus::Done,
            Self::Stopped => GoalStatus::Stopped,
            Self::Failed => GoalStatus::Failed,
        }
    }
}

/// One goal: the plan, its progress, and what it is waiting on.
#[derive(Debug, Clone)]
pub struct AgentGoal {
    pub id: AgentGoalId,
    pub channel: ChatChannelId,
    /// The Ask alo agent that planned it; a resumed run speaks as it.
    pub agent: ChatAgentId,
    /// Whose reach every step runs at — the only person who may move it.
    pub asked_by: UserId,
    /// The goal in the asker's own words.
    pub request: String,
    /// The plan, in order, fixed at creation.
    pub steps: Vec<GoalStep>,
    /// Steps before this index are done; equal to `steps.len()` when finished.
    pub cursor: usize,
    pub status: GoalStatus,
    /// The pending proposal it is waiting behind, exactly while waiting.
    pub proposal: Option<ChatProposalId>,
    /// Why it ended, when it ended early.
    pub note: Option<String>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

type GoalRow = (
    String,
    String,
    String,
    String,
    String,
    Value,
    i32,
    String,
    Option<String>,
    Option<String>,
    OffsetDateTime,
    OffsetDateTime,
);

const GOAL_COLUMNS: &str = "id, channel_id, agent_id, asked_by, request, steps, cursor, \
     status, proposal_id, note, created_at, updated_at";

fn row_to_goal(row: GoalRow) -> Result<AgentGoal> {
    let steps: Vec<GoalStep> = serde_json::from_value(row.5)
        .map_err(|_| StoreError::Validation("a goal's plan could not be read back".to_owned()))?;
    Ok(AgentGoal {
        id: AgentGoalId::new(row.0),
        channel: ChatChannelId::new(row.1),
        agent: ChatAgentId::new(row.2),
        asked_by: UserId::new(row.3),
        request: row.4,
        steps,
        cursor: usize::try_from(row.6).unwrap_or_default(),
        status: GoalStatus::parse(&row.7)?,
        proposal: row.8.map(ChatProposalId::new),
        note: row.9,
        created_at: row.10,
        updated_at: row.11,
    })
}

impl AccountStore {
    /// Record a goal: the plan Ask alo just made, about to start running.
    ///
    /// The caller is the asker — the goal runs at their reach and is theirs to
    /// move from here on.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the room is not the caller's to see;
    /// [`StoreError::Validation`] for an empty request or a plan with no steps.
    pub async fn create_goal(
        &self,
        channel: &ChatChannelId,
        agent: &ChatAgentId,
        request: &str,
        steps: &[GoalStep],
    ) -> Result<AgentGoal> {
        // The room decides whether the caller may put work in it at all.
        self.channel(channel).await?;
        let request = request.trim();
        if request.is_empty() {
            return Err(StoreError::Validation(
                "a goal needs its request".to_owned(),
            ));
        }
        if steps.is_empty() {
            return Err(StoreError::Validation(
                "a goal with no steps is not a plan".to_owned(),
            ));
        }
        let id = AgentGoalId::generate();
        let plan =
            serde_json::to_value(steps).map_err(|e| StoreError::Validation(e.to_string()))?;
        sqlx::query(
            "INSERT INTO agent_goals \
                 (tenant_id, id, channel_id, agent_id, asked_by, request, steps) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(channel.as_str())
        .bind(agent.as_str())
        .bind(self.user.as_str())
        .bind(request)
        .bind(plan)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        self.goal(&id).await
    }

    /// One goal, if its room is the caller's to see.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when it does not exist or its room is not
    /// visible — deliberately the same answer.
    pub async fn goal(&self, id: &AgentGoalId) -> Result<AgentGoal> {
        let found: Option<GoalRow> = sqlx::query_as(&format!(
            "SELECT {GOAL_COLUMNS} FROM agent_goals WHERE tenant_id = $1 AND id = $2"
        ))
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        let row = found.ok_or(StoreError::NotFound)?;
        let goal = row_to_goal(row)?;
        // The room decides whether this goal exists for the caller at all.
        self.channel(&goal.channel).await?;
        Ok(goal)
    }

    /// A room's goals, newest first — what the room's goal card lists.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the room is not the caller's to see.
    pub async fn channel_goals(&self, channel: &ChatChannelId) -> Result<Vec<AgentGoal>> {
        self.channel(channel).await?;
        let rows: Vec<GoalRow> = sqlx::query_as(&format!(
            "SELECT {GOAL_COLUMNS} FROM agent_goals \
             WHERE tenant_id = $1 AND channel_id = $2 \
             ORDER BY created_at DESC, id"
        ))
        .bind(self.tenant.as_str())
        .bind(channel.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter().map(row_to_goal).collect()
    }

    /// A step answered: move the cursor past it and keep going.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] not visible; [`StoreError::Forbidden`] not the
    /// asker; [`StoreError::Validation`] not working, or a cursor that would
    /// move backwards or past the plan.
    pub async fn advance_goal(&self, id: &AgentGoalId, cursor: usize) -> Result<()> {
        let held = self.only_askers(id, GoalStatus::Working).await?;
        if cursor <= held.cursor || cursor > held.steps.len() {
            return Err(StoreError::Validation(
                "a goal only ever moves forward, one plan long".to_owned(),
            ));
        }
        let cursor = i32::try_from(cursor).unwrap_or(i32::MAX);
        let done = sqlx::query(
            "UPDATE agent_goals SET cursor = $3, updated_at = now() \
             WHERE tenant_id = $1 AND id = $2 AND status = 'working' AND cursor < $3",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(cursor)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        moved(done)
    }

    /// A step proposed a change: the goal now waits behind that one proposal.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] not visible; [`StoreError::Forbidden`] not the
    /// asker; [`StoreError::Validation`] when it is not working.
    pub async fn goal_awaits(&self, id: &AgentGoalId, proposal: &ChatProposalId) -> Result<()> {
        self.only_askers(id, GoalStatus::Working).await?;
        let done = sqlx::query(
            "UPDATE agent_goals \
             SET status = 'waiting', proposal_id = $3, updated_at = now() \
             WHERE tenant_id = $1 AND id = $2 AND status = 'working'",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(proposal.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        moved(done)
    }

    /// The goal waiting on this proposal, if the caller has one — what a
    /// settled proposal asks before anything resumes.
    ///
    /// Scoped to the caller's own goals: the decider of a proposal is its
    /// asker, and a goal is its asker's, so anyone else's decision (there is
    /// none — the proposal table refuses them) would find nothing here.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn goal_waiting_on(&self, proposal: &ChatProposalId) -> Result<Option<AgentGoal>> {
        let found: Option<GoalRow> = sqlx::query_as(&format!(
            "SELECT {GOAL_COLUMNS} FROM agent_goals \
             WHERE tenant_id = $1 AND proposal_id = $2 \
               AND status = 'waiting' AND asked_by = $3"
        ))
        .bind(self.tenant.as_str())
        .bind(proposal.as_str())
        .bind(self.user.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        found.map(row_to_goal).transpose()
    }

    /// The waited-on proposal was approved and its step executed: move past
    /// that step and hand the goal back to the run, working again.
    ///
    /// Returns the goal as it now stands; a cursor at the end of the plan
    /// means there is nothing left to run and the caller finishes it done.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] not visible; [`StoreError::Forbidden`] not the
    /// asker; [`StoreError::Validation`] when it is not waiting.
    pub async fn resume_goal(&self, id: &AgentGoalId) -> Result<AgentGoal> {
        self.only_askers(id, GoalStatus::Waiting).await?;
        let done = sqlx::query(
            "UPDATE agent_goals \
             SET status = 'working', proposal_id = NULL, cursor = cursor + 1, \
                 updated_at = now() \
             WHERE tenant_id = $1 AND id = $2 AND status = 'waiting'",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        moved(done)?;
        self.goal(id).await
    }

    /// End a goal, however it ended. The one way out of both `working` and
    /// `waiting`, so a Stop works mid-run and mid-wait alike.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] not visible; [`StoreError::Forbidden`] not the
    /// asker; [`StoreError::Validation`] when it already ended.
    pub async fn finish_goal(
        &self,
        id: &AgentGoalId,
        end: GoalEnd,
        note: Option<&str>,
    ) -> Result<()> {
        let held = self.goal(id).await?;
        if held.asked_by.as_str() != self.user.as_str() {
            return Err(StoreError::Forbidden);
        }
        if !matches!(held.status, GoalStatus::Working | GoalStatus::Waiting) {
            return Err(StoreError::Validation(format!(
                "that goal already ended: {}",
                held.status.as_str()
            )));
        }
        let done = sqlx::query(
            "UPDATE agent_goals \
             SET status = $3, proposal_id = NULL, note = $4, updated_at = now() \
             WHERE tenant_id = $1 AND id = $2 AND status IN ('working', 'waiting')",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(end.status().as_str())
        .bind(note)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        moved(done)
    }

    /// The shared gate of every goal write: visible, the caller's own, and in
    /// the state the transition leaves from — each with its own refusal, so a
    /// colleague hears "not yours" and a finished goal says what it became.
    async fn only_askers(&self, id: &AgentGoalId, from: GoalStatus) -> Result<AgentGoal> {
        let held = self.goal(id).await?;
        if held.asked_by.as_str() != self.user.as_str() {
            return Err(StoreError::Forbidden);
        }
        if held.status != from {
            return Err(StoreError::Validation(format!(
                "that goal is {}, not {}",
                held.status.as_str(),
                from.as_str()
            )));
        }
        Ok(held)
    }
}

/// Every transition is a conditional UPDATE, and "no row moved" is the race it
/// guards against: two writers, one transition, exactly one winner.
fn moved(done: sqlx::postgres::PgQueryResult) -> Result<()> {
    if done.rows_affected() == 0 {
        return Err(StoreError::Validation(
            "that goal moved under you — read it again".to_owned(),
        ));
    }
    Ok(())
}
