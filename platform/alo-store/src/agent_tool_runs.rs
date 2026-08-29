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
//!
//! Since A8.1 the row is the **action record** of ADR 0058 §6: a person's
//! click and an agent's proposal are the same object. A write keeps the
//! preview a person was shown (or would have been), the record the execution
//! touched, the inverse verb and the arguments that would undo it when the
//! domain has one, and the `chat_proposals` row it settled when it came from
//! one. Who acted was already here — `asked_by` is whose access the run
//! carried (the design's on_behalf_of) and `agent` the agent that acted, so
//! a bare `asked_by` is a person acting for themselves. The execution's
//! result payload is deliberately still absent: the result the record keeps
//! is a *pointer* to the record touched, never a second copy of its content.

use serde_json::Value;
use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::events::valid_event_name;
use crate::id::{ChatAgentId, ChatChannelId, ChatProposalId, ChatToolRunId, UserId};

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
    /// For a write with a preview template: what this would do, rendered with
    /// the resolved arguments — the sentence a proposal card shows.
    pub preview: Option<String>,
    /// The record the execution touched, when it touched exactly one — the
    /// record word the executor's reply uses (`quote`) and the record's id.
    pub record_type: Option<String>,
    pub record_id: Option<String>,
    /// The inverse verb and the arguments that would undo this run, when the
    /// domain has one and the run succeeded touching a record to point it at.
    pub undo_tool: Option<String>,
    pub undo_args: Option<Value>,
    /// The `chat_proposals` row this execution settled, when it came from one.
    pub proposal: Option<ChatProposalId>,
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
    /// The rendered preview, for a write whose registry entry has one.
    pub preview: Option<&'a str>,
    /// The record the execution touched — both halves or neither.
    pub record_type: Option<&'a str>,
    pub record_id: Option<&'a str>,
    /// The inverse verb and its arguments; args never without the verb.
    pub undo_tool: Option<&'a str>,
    pub undo_args: Option<&'a Value>,
    /// The proposal this execution settled, when it came from one.
    pub proposal: Option<&'a ChatProposalId>,
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
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<Value>,
    Option<String>,
    OffsetDateTime,
);

/// The columns both reads return — one string so they can never drift into
/// answering with different shapes.
const SELECT_RUNS: &str = "SELECT id, agent_id, channel_id, asked_by, tool, effect, args, ok, \
            preview, record_type, record_id, undo_tool, undo_args, proposal_id, created_at \
     FROM agent_tool_runs";

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
        preview: row.8,
        record_type: row.9,
        record_id: row.10,
        undo_tool: row.11,
        undo_args: row.12,
        proposal: row.13.map(ChatProposalId::new),
        created_at: row.14,
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
    /// `write`, a preview or undo on a read (a read changes nothing, so there
    /// is nothing to preview and nothing to invert), a half-given record
    /// reference or one outside the event vocabulary, undo arguments without
    /// their verb, or an overlong preview; [`StoreError::Db`] on a database
    /// failure.
    pub async fn record_tool_run(&self, run: &NewAgentToolRun<'_>) -> Result<ChatToolRunId> {
        if !matches!(run.effect, "read" | "write") {
            return Err(StoreError::Validation(format!(
                "unknown tool effect {}",
                run.effect
            )));
        }
        if run.effect == "read" && (run.preview.is_some() || run.undo_tool.is_some()) {
            return Err(StoreError::Validation(
                "a read has no preview and no undo".to_owned(),
            ));
        }
        if let Some(preview) = run.preview
            && (preview.is_empty() || preview.len() > 1000)
        {
            return Err(StoreError::Validation(
                "a preview is one sentence, 1..=1000 bytes".to_owned(),
            ));
        }
        if run.record_type.is_some() != run.record_id.is_some() {
            return Err(StoreError::Validation(
                "a record reference is a type and an id together".to_owned(),
            ));
        }
        if let Some(record_type) = run.record_type
            && !valid_event_name(record_type)
        {
            return Err(StoreError::Validation(
                "record type must be lowercase words joined by '.' or '_'".to_owned(),
            ));
        }
        if let Some(record_id) = run.record_id
            && (record_id.is_empty() || record_id.len() > 128)
        {
            return Err(StoreError::Validation(
                "record id must be 1..=128 bytes".to_owned(),
            ));
        }
        if run.undo_args.is_some() && run.undo_tool.is_none() {
            return Err(StoreError::Validation(
                "undo arguments make no sense without the inverse verb".to_owned(),
            ));
        }
        if let Some(undo_tool) = run.undo_tool
            && !valid_event_name(undo_tool)
        {
            return Err(StoreError::Validation(
                "the inverse verb must be lowercase words joined by '.' or '_'".to_owned(),
            ));
        }
        let id = ChatToolRunId::generate();
        sqlx::query(
            "INSERT INTO agent_tool_runs \
                 (tenant_id, id, agent_id, channel_id, asked_by, tool, effect, args, ok, \
                  preview, record_type, record_id, undo_tool, undo_args, proposal_id) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)",
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
        .bind(run.preview)
        .bind(run.record_type)
        .bind(run.record_id)
        .bind(run.undo_tool)
        .bind(run.undo_args)
        .bind(run.proposal.map(ChatProposalId::as_str))
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
        let rows: Vec<RunRow> = sqlx::query_as(&format!(
            "{SELECT_RUNS} \
             WHERE tenant_id = $1 AND asked_by = $2 \
             ORDER BY created_at DESC, id DESC LIMIT $3"
        ))
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(limit.clamp(1, 200))
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(row_to_run).collect())
    }

    /// One action record, if it is the caller's own — the row the Undo
    /// button reads before it acts (A8.2).
    ///
    /// Scoped to `asked_by = caller` for the same reason the lists are: a run
    /// is an act taken through one person's access, and undoing it is theirs
    /// alone. Another tenant's id, a colleague's run and an id never issued
    /// all get the same [`StoreError::NotFound`] — no oracle.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when no such run is the caller's;
    /// [`StoreError::Db`] on a database failure.
    pub async fn agent_tool_run(&self, id: &ChatToolRunId) -> Result<AgentToolRun> {
        let row: Option<RunRow> = sqlx::query_as(&format!(
            "{SELECT_RUNS} WHERE tenant_id = $1 AND asked_by = $2 AND id = $3"
        ))
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        row.map(row_to_run).ok_or(StoreError::NotFound)
    }

    /// What **one** agent has run for this person, most recent first — the
    /// "what it has done" half of the agent directory (A3.3).
    ///
    /// Scoped to the caller's own runs for the same reason
    /// [`AccountStore::agent_tool_runs`] is: a run is an act taken through one
    /// person's access, and reading which diaries and rooms were opened on
    /// somebody else's behalf would be exactly the leak the access rules exist
    /// to prevent. Two people therefore see different histories for the same
    /// agent, which is the rule the rest of the record already follows.
    ///
    /// **This does not decide whether the agent is the caller's to see** —
    /// [`AccountStore::agent`] does, and every caller here asks it first. An id
    /// of an agent whose module the caller was denied simply matches no row
    /// they could have made a run with, so the worst case is an empty list
    /// rather than a leak; the refusal that says so is the directory's.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn agent_tool_runs_for(
        &self,
        agent: &ChatAgentId,
        limit: i64,
    ) -> Result<Vec<AgentToolRun>> {
        let rows: Vec<RunRow> = sqlx::query_as(&format!(
            "{SELECT_RUNS} \
             WHERE tenant_id = $1 AND asked_by = $2 AND agent_id = $3 \
             ORDER BY created_at DESC, id DESC LIMIT $4"
        ))
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(agent.as_str())
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
