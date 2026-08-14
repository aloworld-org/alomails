//! Agents as chat participants (ADR 0034 §chat, ADR 0038;
//! `docs/design/chat-agents.md`).
//!
//! An agent has an **identity** and no **authority**. It posts under its own
//! name, and every turn it takes runs through the account door of the person
//! who asked it — this module is reached from an [`AccountStore`], so there is
//! no other door available. There is no agent credential anywhere in this
//! type: an agent cannot authenticate, cannot be a caller, and cannot see one
//! thing more than the human who summoned it.
//!
//! That is ADR 0034's "an agent cannot widen access" made structural rather
//! than promised. It also bounds prompt injection: a hostile message in a
//! channel can only ever reach as far as the person who triggered the turn
//! could already reach.

use serde_json::Value;
use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::{ChatAgentId, ChatChannelId, ChatMessageId, ChatProposalId, UserId};

/// A handle is what people type after `@`; it must read like one.
const HANDLE_MAX: usize = 32;

/// An agent that can be named in a conversation.
#[derive(Debug, Clone)]
pub struct ChatAgent {
    /// Opaque id. What `chat_messages.author_id` holds for its messages.
    pub id: ChatAgentId,
    /// Typed after `@`, lowercase, unique in the tenant.
    pub handle: String,
    /// Shown in the feed beside its messages.
    pub name: String,
    /// One line: what asking it is good for.
    pub description: Option<String>,
    /// Retired agents keep their past messages but take no new turns.
    pub disabled: bool,
}

/// What state a proposal is in. A proposal is only ever decided once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProposalState {
    /// Waiting for the asker's tap.
    Pending,
    /// Approved and executed.
    Approved,
    /// Turned down.
    Discarded,
    /// Aged out without a decision.
    Expired,
}

impl ProposalState {
    /// The token stored in the `state` column.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Approved => "approved",
            Self::Discarded => "discarded",
            Self::Expired => "expired",
        }
    }

    pub(crate) fn parse(token: &str) -> Result<Self> {
        match token {
            "pending" => Ok(Self::Pending),
            "approved" => Ok(Self::Approved),
            "discarded" => Ok(Self::Discarded),
            "expired" => Ok(Self::Expired),
            other => Err(StoreError::Validation(format!(
                "unknown proposal state {other}"
            ))),
        }
    }
}

/// An action an agent has proposed, waiting for a tap.
#[derive(Debug, Clone)]
pub struct ChatProposal {
    pub id: ChatProposalId,
    pub channel: ChatChannelId,
    /// The agent's message carrying it, so it renders in place.
    pub message: ChatMessageId,
    /// The person whose words caused it — and the only person who may
    /// approve it.
    pub asked_by: UserId,
    pub tool: String,
    pub args: Value,
    pub state: ProposalState,
    pub decided_by: Option<UserId>,
    pub created_at: OffsetDateTime,
}

fn validate_handle(handle: &str) -> Result<String> {
    let handle = handle.trim().trim_start_matches('@').to_lowercase();
    if handle.is_empty() {
        return Err(StoreError::Validation("an agent needs a handle".to_owned()));
    }
    if handle.chars().count() > HANDLE_MAX {
        return Err(StoreError::Validation(format!(
            "a handle is at most {HANDLE_MAX} characters"
        )));
    }
    // The same characters `parse_handles` will accept after an '@'; a handle
    // nobody can type is not a handle.
    if !handle
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    {
        return Err(StoreError::Validation(
            "a handle uses letters, digits, dot, dash or underscore".to_owned(),
        ));
    }
    Ok(handle)
}

type AgentRow = (
    String,
    String,
    String,
    Option<String>,
    Option<OffsetDateTime>,
);

fn row_to_agent(row: AgentRow) -> ChatAgent {
    ChatAgent {
        id: ChatAgentId::new(row.0),
        handle: row.1,
        name: row.2,
        description: row.3,
        disabled: row.4.is_some(),
    }
}

impl AccountStore {
    /// Define an agent for this tenant.
    ///
    /// # Errors
    /// [`StoreError::Validation`] for a bad handle,
    /// [`StoreError::Conflict`] if the handle is taken.
    pub async fn create_agent(
        &self,
        handle: &str,
        name: &str,
        description: Option<&str>,
    ) -> Result<ChatAgentId> {
        let handle = validate_handle(handle)?;
        let name = name.trim();
        if name.is_empty() {
            return Err(StoreError::Validation("an agent needs a name".to_owned()));
        }
        let id = ChatAgentId::generate();
        let done = sqlx::query(
            "INSERT INTO chat_agents (tenant_id, id, handle, name, description) \
             VALUES ($1, $2, $3, $4, $5) ON CONFLICT DO NOTHING",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(&handle)
        .bind(name)
        .bind(description.map(str::trim).filter(|d| !d.is_empty()))
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::Conflict(format!("@{handle} is already taken")));
        }
        Ok(id)
    }

    /// Every agent this tenant has, retired ones included — the composer's
    /// `@` list filters, the member sheet shows.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn agents(&self) -> Result<Vec<ChatAgent>> {
        let rows: Vec<AgentRow> = sqlx::query_as(
            "SELECT id, handle, name, description, disabled_at FROM chat_agents \
             WHERE tenant_id = $1 ORDER BY handle",
        )
        .bind(self.tenant.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(row_to_agent).collect())
    }

    /// One agent of this tenant.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if there is no such agent here.
    pub async fn agent(&self, id: &ChatAgentId) -> Result<ChatAgent> {
        let row: Option<AgentRow> = sqlx::query_as(
            "SELECT id, handle, name, description, disabled_at FROM chat_agents \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        row.map(row_to_agent).ok_or(StoreError::NotFound)
    }

    /// The agents in a room, so a mention can be resolved against them.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the room is not the caller's to see.
    pub async fn channel_agents(&self, channel: &ChatChannelId) -> Result<Vec<ChatAgent>> {
        self.channel(channel).await?;
        let rows: Vec<AgentRow> = sqlx::query_as(
            "SELECT a.id, a.handle, a.name, a.description, a.disabled_at \
             FROM chat_agents a \
             JOIN chat_agent_members m \
               ON m.tenant_id = a.tenant_id AND m.agent_id = a.id \
             WHERE a.tenant_id = $1 AND m.channel_id = $2 \
             ORDER BY a.handle",
        )
        .bind(self.tenant.as_str())
        .bind(channel.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(row_to_agent).collect())
    }

    /// Put an agent in a room. Membership is a member's business, the same as
    /// inviting a person.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the room or agent is not the caller's to
    /// see, or they are not a member of it.
    pub async fn add_agent_to_channel(
        &self,
        channel: &ChatChannelId,
        agent: &ChatAgentId,
    ) -> Result<()> {
        self.channel(channel).await?;
        if self.channel_role(channel).await?.is_none() {
            return Err(StoreError::NotFound);
        }
        let known = self.agent(agent).await?;
        if known.disabled {
            return Err(StoreError::Validation(format!(
                "@{} is retired and takes no new turns",
                known.handle
            )));
        }
        sqlx::query(
            "INSERT INTO chat_agent_members (tenant_id, channel_id, agent_id, added_by) \
             VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING",
        )
        .bind(self.tenant.as_str())
        .bind(channel.as_str())
        .bind(agent.as_str())
        .bind(self.user.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }

    /// Take an agent out of a room. Its past messages stay: a room's history
    /// does not change because somebody left.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the room is not the caller's, or they are
    /// not a member.
    pub async fn remove_agent_from_channel(
        &self,
        channel: &ChatChannelId,
        agent: &ChatAgentId,
    ) -> Result<()> {
        self.channel(channel).await?;
        if self.channel_role(channel).await?.is_none() {
            return Err(StoreError::NotFound);
        }
        sqlx::query(
            "DELETE FROM chat_agent_members \
             WHERE tenant_id = $1 AND channel_id = $2 AND agent_id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(channel.as_str())
        .bind(agent.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }
}

impl AccountStore {
    /// Colleagues whose address matches `query` — for choosing someone to
    /// start a conversation with.
    ///
    /// A search and not a listing: see
    /// [`find_people`](crate::identity::find_people). Never includes the
    /// caller, and never reaches outside their tenant.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn find_people(&self, query: &str, limit: i64) -> Result<Vec<(UserId, String)>> {
        crate::identity::find_people(
            &self.pool,
            self.tenant.as_str(),
            query,
            self.user.as_str(),
            limit,
        )
        .await
    }
}

/// What an agent has actually done — the difference between a feature and a
/// colleague.
#[derive(Debug, Clone, Default)]
pub struct AgentRecord {
    /// Times it has answered.
    pub answers: i64,
    /// Actions it proposed that someone approved and that therefore ran.
    pub actions: i64,
    /// Reading tools it ran **for the caller**, inside a turn and with nobody's
    /// approval (ADR 0047 §4).
    ///
    /// Counted from `agent_tool_runs` rather than from proposals, because a
    /// read no longer makes one. Without this, eleven of the thirty-three
    /// tools would run leaving nothing behind and this record — the one
    /// surface that says what an agent has done — would quietly under-report a
    /// third of its work.
    pub reads: i64,
    /// When it last said anything.
    pub last_at: Option<OffsetDateTime>,
}

impl AccountStore {
    /// What each agent has done, keyed by agent id.
    ///
    /// **Counted only within rooms the caller can see** — a member of one
    /// channel must not learn from a tally that an agent is busy in a private
    /// room they were never in. Aggregates leak too, just more slowly, and the
    /// rule everywhere else in chat is that you see what you could already
    /// read.
    ///
    /// One query for every agent rather than one each: this is drawn beside a
    /// list.
    ///
    /// The reads are a second query rather than another join, because they are
    /// counted over a different population: what an agent *said* is scoped by
    /// the rooms the caller can see, while what it *ran* is scoped to the
    /// caller's own runs — a read happens through one person's access and a
    /// colleague must not learn from a tally which diaries were opened for
    /// them. Folding both into one `GROUP BY` would multiply the two counts
    /// together.
    ///
    /// An agent that has only ever read still appears here: its row comes from
    /// the reads alone, so eleven tools' worth of work is not invisible until
    /// the agent happens to also speak.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn agent_records(&self) -> Result<std::collections::HashMap<String, AgentRecord>> {
        let rows: Vec<(String, i64, i64, Option<OffsetDateTime>)> = sqlx::query_as(
            "SELECT m.author_id, \
                    count(*) FILTER (WHERE m.deleted_at IS NULL) AS answers, \
                    count(p.id) FILTER (WHERE p.state = 'approved') AS actions, \
                    max(m.created_at) AS last_at \
             FROM chat_messages m \
             JOIN chat_channels c \
               ON c.tenant_id = m.tenant_id AND c.id = m.channel_id \
             LEFT JOIN chat_proposals p \
               ON p.tenant_id = m.tenant_id AND p.message_id = m.id \
             WHERE m.tenant_id = $1 AND m.author_kind = 'agent' \
               AND ( \
                 EXISTS (SELECT 1 FROM chat_members mm \
                         WHERE mm.tenant_id = c.tenant_id AND mm.channel_id = c.id \
                           AND mm.user_id = $2) \
                 OR (c.visibility = 'public' AND c.archived_at IS NULL)) \
             GROUP BY m.author_id",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        let mut records: std::collections::HashMap<String, AgentRecord> = rows
            .into_iter()
            .map(|(agent, answers, actions, last_at)| {
                (
                    agent,
                    AgentRecord {
                        answers,
                        actions,
                        reads: 0,
                        last_at,
                    },
                )
            })
            .collect();
        for (agent, reads) in self.agent_read_counts().await? {
            records.entry(agent).or_default().reads = reads;
        }
        Ok(records)
    }
}
