//! A one-to-one with an agent (ADR 0048).
//!
//! Its own file because it is its own room shape, not a variation on
//! [`crate::chat`]'s two: `dm_key` is a pair of **user** ids, and ADR 0141
//! deliberately refuses to make an agent a user. What is here is the whole of
//! the difference — opening one, and finding it again — and every other rule an
//! agent DM has (its members are fixed, it has no name, it is not archived, it
//! is visible only to its own human) is the rule a DM already has, applied
//! through [`ChannelKind::is_direct`] rather than restated.
//!
//! **The room is not a way to see more.** The human is its only member and
//! every turn the agent takes still runs through their account door, so an
//! agent in a one-to-one reaches exactly what its human could already reach.

use crate::account::AccountStore;
use crate::chat::ChannelKind;
use crate::error::{Result, StoreError};
use crate::id::{ChatAgentId, ChatChannelId};

impl AccountStore {
    /// Open the caller's one-to-one with `agent`, creating it once.
    ///
    /// Idempotent by construction, exactly as [`AccountStore::open_dm`] is: the
    /// partial unique index over `(tenant_id, agent_id, created_by)` means two
    /// simultaneous opens still yield one room, and opening it again a week
    /// later returns the conversation rather than a second empty one.
    ///
    /// Created on demand and never in advance — a tenant with a dozen agents
    /// must not find a dozen empty rooms in its sidebar.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if there is no such agent in this tenant,
    /// [`StoreError::Validation`] if it is retired — the same refusal
    /// [`AccountStore::add_agent_to_channel`] gives, because opening a room
    /// with an agent that takes no turns would be a conversation with nobody.
    pub async fn open_agent_dm(&self, agent: &ChatAgentId) -> Result<ChatChannelId> {
        let known = self.agent(agent).await?;
        if known.disabled {
            return Err(StoreError::Validation(format!(
                "@{} is retired and takes no new turns",
                known.handle
            )));
        }
        if let Some(id) = self.agent_dm(agent).await? {
            return Ok(id);
        }

        let id = ChatChannelId::generate();
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let inserted: Option<(String,)> = sqlx::query_as(
            "INSERT INTO chat_channels \
                 (tenant_id, id, kind, visibility, agent_id, created_by) \
             VALUES ($1, $2, 'agent_dm', 'private', $3, $4) \
             ON CONFLICT (tenant_id, agent_id, created_by) WHERE kind = 'agent_dm' \
             DO NOTHING \
             RETURNING id",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(agent.as_str())
        .bind(self.user.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        let Some(_) = inserted else {
            // Another request opened the same conversation between our lookup
            // and our insert; theirs is the room.
            tx.rollback().await.map_err(StoreError::Db)?;
            return self.agent_dm(agent).await?.ok_or(StoreError::NotFound);
        };
        // One human and one agent, each in the table that holds its own kind of
        // participant. A plain member and not an owner: there is nothing here
        // to own — no name to change, nobody to remove.
        sqlx::query(
            "INSERT INTO chat_members (tenant_id, channel_id, user_id, role) \
             VALUES ($1, $2, $3, 'member')",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(self.user.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        sqlx::query(
            "INSERT INTO chat_agent_members (tenant_id, channel_id, agent_id, added_by) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(agent.as_str())
        .bind(self.user.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(id)
    }

    /// The caller's existing one-to-one with `agent`, if they have opened one.
    ///
    /// Scoped to `created_by = the caller`, so this can never hand back a
    /// colleague's conversation with the same agent.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn agent_dm(&self, agent: &ChatAgentId) -> Result<Option<ChatChannelId>> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM chat_channels \
             WHERE tenant_id = $1 AND kind = 'agent_dm' \
               AND agent_id = $2 AND created_by = $3",
        )
        .bind(self.tenant.as_str())
        .bind(agent.as_str())
        .bind(self.user.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(row.map(|r| ChatChannelId::new(r.0)))
    }

    /// The agent a room is the one-to-one with, if it is one.
    ///
    /// This is the trigger's question (ADR 0048: "in an `agent_dm`, every
    /// message from the human is the trigger"), and it is asked of the room
    /// rather than of the words, so there is no handle to type and nothing to
    /// parse. [`StoreError::NotFound`] from [`AccountStore::channel`] is what
    /// keeps a room the caller may not see from answering it at all.
    ///
    /// **A room is not a way past the module switch.** A one-to-one opened
    /// before an admin switched Inventory off for this person stays readable —
    /// its history is theirs — but its counterpart is no longer theirs to see,
    /// so the answer here is `None` and every message in it is an ordinary
    /// message nobody replies to. That is the same sentence
    /// [`AccountStore::agents`] gives about the list, asked of a room.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the room is not the caller's to see.
    pub async fn channel_agent_counterpart(
        &self,
        channel: &ChatChannelId,
    ) -> Result<Option<ChatAgentId>> {
        let room = self.channel(channel).await?;
        if room.kind != ChannelKind::AgentDm {
            return Ok(None);
        }
        let Some(agent) = room.agent else {
            return Ok(None);
        };
        match self.agent(&agent).await {
            Ok(_) => Ok(Some(agent)),
            // The agent gate's own answer for "not yours to see". A room whose
            // counterpart has become invisible is not an error to its owner.
            Err(StoreError::NotFound) => Ok(None),
            Err(other) => Err(other),
        }
    }
}
