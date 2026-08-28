//! What an agent remembers, and where it may remember it (ADR 0057 §6,
//! `docs/design/complete-agents.md` §6, queue item A6.1).
//!
//! **The channel is the consent boundary.** What was shared in a channel may
//! be remembered by the agents in it, whoever wrote it, and used in that
//! channel and nowhere else; a one-to-one with an agent feeds only that
//! person's memory. The two scopes are exclusive rows, not a search radius —
//! there is no cross-channel pool for a wide query to fall into.
//!
//! A memory is a **fact, never a transcript**: one short standalone sentence
//! with the message it came from. The length cap here is what makes that a
//! property of the store rather than a hope about the extractor.
//!
//! Learning is switched per channel (room settings, `chat_channels.
//! agent_memory`), defaulting to the workspace's own default
//! (`agent_memory_defaults`), defaulting to ON. An explicit "remember that …"
//! is stored whatever the switches say — a person asking by name is the
//! consent the switch exists to approximate.

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::chat::MemberRole;
use crate::error::{Result, StoreError};
use crate::id::{AgentMemoryId, ChatAgentId, ChatChannelId, ChatMessageId, UserId};

/// The longest fact the store will hold. Long enough for "Northstar Foods
/// invoices are net 30 and go to their Rotterdam office", far too short for a
/// transcript.
pub const MEMORY_FACT_MAX: usize = 400;

/// How many memories one agent keeps per room (or per person). At the cap the
/// oldest is dropped for the newest — an agent's memory of a busy year should
/// be the recent facts, not the first two hundred.
pub const MEMORIES_PER_SCOPE: i64 = 200;

/// How a fact came to be remembered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryLearnedFrom {
    /// Extracted at the end of a turn from what the turn read.
    Turn,
    /// A person said "remember that …" — works even where learning is off.
    Explicit,
}

impl MemoryLearnedFrom {
    /// The token stored in the `learned_from` column.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Turn => "turn",
            Self::Explicit => "explicit",
        }
    }

    fn parse(token: &str) -> Result<Self> {
        match token {
            "turn" => Ok(Self::Turn),
            "explicit" => Ok(Self::Explicit),
            other => Err(StoreError::Validation(format!(
                "unknown memory provenance {other}"
            ))),
        }
    }
}

/// One fact an agent remembers.
#[derive(Debug, Clone)]
pub struct AgentMemory {
    pub id: AgentMemoryId,
    /// The agent that remembers it — a memory is per agent, never shared.
    pub agent: ChatAgentId,
    /// The room it belongs to, for a channel memory.
    pub channel: Option<ChatChannelId>,
    /// The person it belongs to, for a one-to-one memory.
    pub user: Option<UserId>,
    /// The fact itself, one short standalone sentence.
    pub fact: String,
    /// The message it came from, when there is one.
    pub source_msg: Option<ChatMessageId>,
    pub learned_from: MemoryLearnedFrom,
    pub created_at: OffsetDateTime,
}

type MemoryRow = (
    String,
    String,
    Option<String>,
    Option<String>,
    String,
    Option<String>,
    String,
    OffsetDateTime,
);

const MEMORY_COLUMNS: &str =
    "id, agent_id, channel_id, user_id, fact, source_msg, learned_from, created_at";

fn row_to_memory(row: MemoryRow) -> Result<AgentMemory> {
    Ok(AgentMemory {
        id: AgentMemoryId::new(row.0),
        agent: ChatAgentId::new(row.1),
        channel: row.2.map(ChatChannelId::new),
        user: row.3.map(UserId::new),
        fact: row.4,
        source_msg: row.5.map(ChatMessageId::new),
        learned_from: MemoryLearnedFrom::parse(&row.6)?,
        created_at: row.7,
    })
}

/// One fact, trimmed and held under [`MEMORY_FACT_MAX`] — the "never a
/// transcript" rule, enforced where the row is made.
fn validate_fact(fact: &str) -> Result<&str> {
    let fact = fact.trim();
    if fact.is_empty() {
        return Err(StoreError::Validation(
            "there is nothing to remember".to_owned(),
        ));
    }
    if fact.chars().count() > MEMORY_FACT_MAX {
        return Err(StoreError::Validation(format!(
            "a memory is one short fact — at most {MEMORY_FACT_MAX} characters, not a transcript"
        )));
    }
    Ok(fact)
}

impl AccountStore {
    /// Remember one fact for an agent **in a room** — usable in that room and
    /// nowhere else.
    ///
    /// The caller must be a member of the room (the person whose words the
    /// turn ran on always is), and the agent must be theirs to see — the same
    /// module gate every other agent surface asks. A one-to-one with an agent
    /// is refused here by design: what is said there feeds the person's own
    /// memory ([`AccountStore::remember_for_me`]), never a room's.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] for a room or agent that is not the caller's
    /// to see; [`StoreError::Validation`] for an empty or transcript-length
    /// fact, or an agent one-to-one.
    pub async fn remember_in_channel(
        &self,
        agent: &ChatAgentId,
        channel: &ChatChannelId,
        fact: &str,
        source_msg: Option<&ChatMessageId>,
        learned_from: MemoryLearnedFrom,
    ) -> Result<AgentMemoryId> {
        let fact = validate_fact(fact)?;
        let room = self.channel(channel).await?;
        if room.kind == crate::chat::ChannelKind::AgentDm {
            return Err(StoreError::Validation(
                "a one-to-one with an agent feeds that person's memory, not a room's".to_owned(),
            ));
        }
        if self.channel_role(channel).await?.is_none() {
            return Err(StoreError::NotFound);
        }
        self.agent(agent).await?;
        let id = AgentMemoryId::generate();
        sqlx::query(
            "INSERT INTO agent_memories \
               (tenant_id, id, agent_id, scope, channel_id, fact, source_msg, learned_from) \
             VALUES ($1, $2, $3, 'channel', $4, $5, $6, $7)",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(agent.as_str())
        .bind(channel.as_str())
        .bind(fact)
        .bind(source_msg.map(ChatMessageId::as_str))
        .bind(learned_from.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        self.trim_scope(agent, Some(channel), None).await?;
        Ok(id)
    }

    /// Remember one fact for an agent **about the caller** — what a one-to-one
    /// with the agent feeds, usable only for this person.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] for an agent that is not the caller's to see;
    /// [`StoreError::Validation`] for an empty or transcript-length fact.
    pub async fn remember_for_me(
        &self,
        agent: &ChatAgentId,
        fact: &str,
        source_msg: Option<&ChatMessageId>,
        learned_from: MemoryLearnedFrom,
    ) -> Result<AgentMemoryId> {
        let fact = validate_fact(fact)?;
        self.agent(agent).await?;
        let id = AgentMemoryId::generate();
        sqlx::query(
            "INSERT INTO agent_memories \
               (tenant_id, id, agent_id, scope, user_id, fact, source_msg, learned_from) \
             VALUES ($1, $2, $3, 'person', $4, $5, $6, $7)",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(agent.as_str())
        .bind(self.user.as_str())
        .bind(fact)
        .bind(source_msg.map(ChatMessageId::as_str))
        .bind(learned_from.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        self.trim_scope(agent, None, Some(&self.user)).await?;
        Ok(id)
    }

    /// Drop the oldest rows of one scope beyond [`MEMORIES_PER_SCOPE`].
    async fn trim_scope(
        &self,
        agent: &ChatAgentId,
        channel: Option<&ChatChannelId>,
        user: Option<&UserId>,
    ) -> Result<()> {
        sqlx::query(
            "DELETE FROM agent_memories WHERE tenant_id = $1 AND id IN ( \
               SELECT id FROM agent_memories \
                WHERE tenant_id = $1 AND agent_id = $2 \
                  AND channel_id IS NOT DISTINCT FROM $3 \
                  AND user_id IS NOT DISTINCT FROM $4 \
                ORDER BY created_at DESC, id DESC OFFSET $5)",
        )
        .bind(self.tenant.as_str())
        .bind(agent.as_str())
        .bind(channel.map(ChatChannelId::as_str))
        .bind(user.map(UserId::as_str))
        .bind(MEMORIES_PER_SCOPE)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }

    /// What one agent remembers in one room, newest first — readable by
    /// everyone who can read the room, exactly like the messages the memories
    /// came from.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the room is not the caller's to see.
    pub async fn channel_memories(
        &self,
        agent: &ChatAgentId,
        channel: &ChatChannelId,
    ) -> Result<Vec<AgentMemory>> {
        self.channel(channel).await?;
        let rows: Vec<MemoryRow> = sqlx::query_as(&format!(
            "SELECT {MEMORY_COLUMNS} FROM agent_memories \
             WHERE tenant_id = $1 AND agent_id = $2 AND scope = 'channel' \
               AND channel_id = $3 \
             ORDER BY created_at DESC, id DESC"
        ))
        .bind(self.tenant.as_str())
        .bind(agent.as_str())
        .bind(channel.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter().map(row_to_memory).collect()
    }

    /// What one agent remembers about the caller, newest first. Structurally
    /// the caller's own: the query is bound to their id, so there is no
    /// argument with which to read a colleague's.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn my_memories(&self, agent: &ChatAgentId) -> Result<Vec<AgentMemory>> {
        let rows: Vec<MemoryRow> = sqlx::query_as(&format!(
            "SELECT {MEMORY_COLUMNS} FROM agent_memories \
             WHERE tenant_id = $1 AND agent_id = $2 AND scope = 'person' \
               AND user_id = $3 \
             ORDER BY created_at DESC, id DESC"
        ))
        .bind(self.tenant.as_str())
        .bind(agent.as_str())
        .bind(self.user.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter().map(row_to_memory).collect()
    }

    /// Set the room's learning switch: `Some(true)` on, `Some(false)` off,
    /// `None` back to the workspace default.
    ///
    /// A named room's switch is its owner's (room settings, like the name and
    /// the topic); in a one-to-one either side may flip it — there is no owner
    /// there, and both people are the whole membership.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the room is not the caller's;
    /// [`StoreError::Forbidden`] for a member changing a named room's switch.
    pub async fn set_channel_memory(
        &self,
        channel: &ChatChannelId,
        enabled: Option<bool>,
    ) -> Result<()> {
        let room = self.channel(channel).await?;
        match self.channel_role(channel).await? {
            Some(MemberRole::Owner) => {}
            Some(MemberRole::Member) if room.kind.is_direct() => {}
            Some(MemberRole::Member) => return Err(StoreError::Forbidden),
            None => return Err(StoreError::NotFound),
        }
        sqlx::query(
            "UPDATE chat_channels SET agent_memory = $3, updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(channel.as_str())
        .bind(enabled)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }

    /// The room's own switch, unresolved: `None` means "follow the workspace
    /// default".
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the room is not the caller's to see.
    pub async fn channel_memory_setting(&self, channel: &ChatChannelId) -> Result<Option<bool>> {
        self.channel(channel).await?;
        let row: Option<(Option<bool>,)> = sqlx::query_as(
            "SELECT agent_memory FROM chat_channels WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(channel.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(row.and_then(|(setting,)| setting))
    }

    /// Whether this room's memory is on: the room's switch, else the
    /// workspace default, else ON.
    ///
    /// Gates **learning** at the end of a turn, and gates **retrieval** the
    /// same way (A6.2): a room switched off hides its memories from later
    /// turns rather than deleting them (30-day deletion is A6.3) — the rows
    /// stay, and flipping the switch back on surfaces them again. An explicit
    /// "remember that …" does not come through here — a person asking by name
    /// is the consent the switch approximates.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the room is not the caller's to see.
    pub async fn memory_enabled(&self, channel: &ChatChannelId) -> Result<bool> {
        let setting = self.channel_memory_setting(channel).await?;
        if let Some(chosen) = setting {
            return Ok(chosen);
        }
        self.workspace_memory_default().await
    }

    /// The workspace's learning default — what every room that never chose for
    /// itself follows. ON where no admin ever touched it.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn workspace_memory_default(&self) -> Result<bool> {
        let row: Option<(bool,)> =
            sqlx::query_as("SELECT enabled FROM agent_memory_defaults WHERE tenant_id = $1")
                .bind(self.tenant.as_str())
                .fetch_optional(&self.pool)
                .await
                .map_err(StoreError::Db)?;
        Ok(row.is_none_or(|(enabled,)| enabled))
    }

    /// Set the workspace's learning default. The route gates this on admin
    /// (`/admin/*`), the same way the AI provider settings are gated.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn set_workspace_memory_default(&self, enabled: bool) -> Result<()> {
        sqlx::query(
            "INSERT INTO agent_memory_defaults (tenant_id, enabled) VALUES ($1, $2) \
             ON CONFLICT (tenant_id) DO UPDATE SET enabled = $2, updated_at = now()",
        )
        .bind(self.tenant.as_str())
        .bind(enabled)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn a_fact_is_trimmed_and_never_a_transcript() {
        assert_eq!(validate_fact("  net 30  ").unwrap(), "net 30");
        assert!(validate_fact("   ").is_err());
        assert!(validate_fact(&"a".repeat(MEMORY_FACT_MAX)).is_ok());
        assert!(validate_fact(&"a".repeat(MEMORY_FACT_MAX + 1)).is_err());
    }

    #[test]
    fn provenance_tokens_round_trip() {
        for from in [MemoryLearnedFrom::Turn, MemoryLearnedFrom::Explicit] {
            assert_eq!(MemoryLearnedFrom::parse(from.as_str()).unwrap(), from);
        }
        assert!(MemoryLearnedFrom::parse("guessed").is_err());
    }
}
