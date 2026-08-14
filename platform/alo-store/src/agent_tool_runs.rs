//! What an agent has actually run — every tool, read or write (ADR 0047 §4).
//!
//! An agent's record used to be countable from `chat_proposals`, because a
//! tool ran only when somebody approved one. ADR 0047 lets a reading tool run
//! inside the turn with no tap, and eleven of the thirty-three tools are
//! reads. Left there, a third of what an agent does would leave nothing behind
//! and the surface that shows its record would quietly under-report it.
//!
//! So **both** paths write here, not only the new one — one log rather than
//! two half-histories a reader has to know how to join.
//!
//! Written through the [`AccountStore`], like everything else an agent does:
//! the row records the person whose access the tool ran through, because an
//! agent has no access of its own (see `chat_agents`). Recording is
//! deliberately **best-effort at the call site** — an audit row that could fail
//! a read the caller was entitled to would trade a working product for a
//! logline — but a failure to write one is never silently turned into success
//! here: this returns its error and lets the caller decide.

use serde_json::Value;
use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::{ChatAgentId, ChatChannelId, ChatToolRunId, UserId};

/// One tool run, as it is recorded.
#[derive(Debug, Clone)]
pub struct AgentToolRun {
    pub id: ChatToolRunId,
    /// The agent that ran it; `None` for the workspace assistant reached from
    /// the command palette, which is not a row in `chat_agents`.
    pub agent: Option<ChatAgentId>,
    /// The room it happened in; `None` outside chat.
    pub channel: Option<ChatChannelId>,
    /// Whose access it ran through.
    pub asked_by: UserId,
    pub tool: String,
    /// `"read"` (ran inside the turn) or `"write"` (ran from an approval).
    pub effect: String,
    pub args: Value,
    /// Whether it did what it was asked.
    pub ok: bool,
    pub created_at: OffsetDateTime,
}

/// What to record about one run. A struct rather than eight positional
/// arguments, because `record_tool_run(a, b, true, false)` is a bug waiting to
/// be written.
#[derive(Debug, Clone)]
pub struct NewAgentToolRun<'a> {
    pub agent: Option<&'a ChatAgentId>,
    pub channel: Option<&'a ChatChannelId>,
    pub tool: &'a str,
    pub effect: &'a str,
    pub args: &'a Value,
    pub ok: bool,
}

type RunRow = (
    String,
    Option<String>,
    Option<String>,
    String,
    String,
    String,
    Value,
    bool,
    OffsetDateTime,
);

fn row_to_run(row: RunRow) -> AgentToolRun {
    AgentToolRun {
        id: ChatToolRunId::new(row.0),
        agent: row.1.map(ChatAgentId::new),
        channel: row.2.map(ChatChannelId::new),
        asked_by: UserId::new(row.3),
        tool: row.4,
        effect: row.5,
        args: row.6,
        ok: row.7,
        created_at: row.8,
    }
}

impl AccountStore {
    /// Record that a tool ran (or was refused).
    ///
    /// The asker is always the caller: this store handle *is* the person whose
    /// access produced the run, so there is no parameter for it and therefore
    /// no way to attribute a run to somebody else.
    ///
    /// # Errors
    /// [`StoreError::Validation`] for an effect that is neither `read` nor
    /// `write`; [`StoreError::Db`] on a database failure.
    pub async fn record_tool_run(&self, run: &NewAgentToolRun<'_>) -> Result<ChatToolRunId> {
        if !matches!(run.effect, "read" | "write") {
            return Err(StoreError::Validation(format!(
                "unknown tool effect {}",
                run.effect
            )));
        }
        let id = ChatToolRunId::generate();
        sqlx::query(
            "INSERT INTO agent_tool_runs \
                 (tenant_id, id, agent_id, channel_id, asked_by, tool, effect, args, ok) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(run.agent.map(ChatAgentId::as_str))
        .bind(run.channel.map(ChatChannelId::as_str))
        .bind(self.user.as_str())
        .bind(run.tool)
        .bind(run.effect)
        .bind(run.args)
        .bind(run.ok)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(id)
    }

    /// What this person's agents have run, most recent first.
    ///
    /// **Only the caller's own runs.** A run is an act taken through one
    /// person's access, and a colleague reading which diaries and rooms were
    /// looked at on somebody else's behalf would learn from the log exactly
    /// what the access rules exist to withhold. The tenant-wide view belongs
    /// to an admin surface with its own gate, not to this door.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn agent_tool_runs(&self, limit: i64) -> Result<Vec<AgentToolRun>> {
        let rows: Vec<RunRow> = sqlx::query_as(
            "SELECT id, agent_id, channel_id, asked_by, tool, effect, args, ok, created_at \
             FROM agent_tool_runs \
             WHERE tenant_id = $1 AND asked_by = $2 \
             ORDER BY created_at DESC, id DESC LIMIT $3",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(limit.clamp(1, 200))
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(row_to_run).collect())
    }

    /// How many reads each agent has run for this person, keyed by agent id —
    /// the reads half of [`crate::AgentRecord`].
    ///
    /// Counted over the caller's **own** runs, for the same reason the answers
    /// and actions are counted only over rooms they can see: a tally is a leak
    /// too, just a slower one.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub(crate) async fn agent_read_counts(&self) -> Result<std::collections::HashMap<String, i64>> {
        let rows: Vec<(String, i64)> = sqlx::query_as(
            "SELECT agent_id, count(*) FROM agent_tool_runs \
             WHERE tenant_id = $1 AND asked_by = $2 AND agent_id IS NOT NULL \
               AND effect = 'read' AND ok \
             GROUP BY agent_id",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().collect())
    }
}
