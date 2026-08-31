//! Actions an agent has proposed, waiting for a tap (ADR 0023, ADR 0034;
//! `docs/design/chat-agents.md`).
//!
//! A proposal is stored rather than held in the client because a chat
//! proposal is seen by a room, must survive a reload, must be refusable, and
//! must leave a record of who decided. The command palette's approval flow
//! keeps its proposal in React state, which is enough for one person for four
//! seconds and not enough here.
//!
//! **Only the asker may decide.** The proposal was computed through their
//! access, so approving it as anyone else would run their reach on another
//! person's say-so. Everyone in the room can see it, so that refusal is a
//! permission and not a secret — `Forbidden`, never `NotFound`.

use std::collections::HashMap;

use serde_json::Value;
use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::chat_agents::{ChatProposal, ProposalState};
use crate::chat_messages::{ChatMessage, MessageSource};
use crate::error::{Result, StoreError};
use crate::id::{ChatAgentId, ChatChannelId, ChatMessageId, ChatProposalId, UserId};

/// A proposal row plus the channel it belongs to, which the single read needs
/// and the page read already knows.
type ProposalRowWithChannel = (
    String,
    String,
    String,
    String,
    Value,
    String,
    Option<String>,
    OffsetDateTime,
    String,
);

/// Row shape shared by the single read and the page read.
type ProposalRow = (
    String,
    String,
    String,
    String,
    Value,
    String,
    Option<String>,
    OffsetDateTime,
);

fn row_to_proposal(row: ProposalRow, channel: ChatChannelId) -> Result<ChatProposal> {
    Ok(ChatProposal {
        id: ChatProposalId::new(row.0),
        channel,
        message: ChatMessageId::new(row.1),
        asked_by: UserId::new(row.2),
        tool: row.3,
        args: row.4,
        state: ProposalState::parse(&row.5)?,
        decided_by: row.6.map(UserId::new),
        created_at: row.7,
    })
}

impl AccountStore {
    /// Post as an agent, on the caller's behalf.
    ///
    /// The caller is the asker: their membership permits the post, their
    /// account door produced whatever is being said, and their id is recorded
    /// on the message. The agent supplies only a name to say it under.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the room is not the caller's, they are not
    /// a member, or the agent is not in the room;
    /// [`StoreError::Validation`] for an empty body or an archived room.
    pub async fn post_as_agent(
        &self,
        channel: &ChatChannelId,
        agent: &ChatAgentId,
        body: &str,
        thread_root_seq: Option<i64>,
    ) -> Result<ChatMessage> {
        self.post_as_agent_cited(channel, agent, body, thread_root_seq, &[])
            .await
    }

    /// The same, for an answer that cites what it was grounded in.
    ///
    /// Separate rather than a sixth parameter on every call site: most of what
    /// an agent says — a plan, a refusal, a sentence describing a proposal —
    /// cites nothing, and only the answer path has the list to hand.
    ///
    /// # Errors
    /// As [`Self::post_as_agent`].
    pub async fn post_as_agent_cited(
        &self,
        channel: &ChatChannelId,
        agent: &ChatAgentId,
        body: &str,
        thread_root_seq: Option<i64>,
        sources: &[MessageSource],
    ) -> Result<ChatMessage> {
        // An agent speaks only in rooms it has been put in: being defined in
        // the tenant is not permission to appear anywhere in it.
        let present = self
            .channel_agents(channel)
            .await?
            .into_iter()
            .any(|a| a.id.as_str() == agent.as_str());
        if !present {
            return Err(StoreError::NotFound);
        }
        let asker = self.user.as_str().to_owned();
        self.insert_message(
            channel,
            crate::chat_messages::NewMessage {
                author: agent.as_str(),
                author_kind: "agent",
                on_behalf_of: Some(&asker),
                body,
                thread_root_seq,
                sources,
            },
        )
        .await
    }

    /// Record an action an agent has proposed, against the agent's own
    /// message so it renders in place.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the message is not the caller's to see.
    pub async fn propose_action(
        &self,
        message: &ChatMessageId,
        tool: &str,
        args: &Value,
    ) -> Result<ChatProposalId> {
        let held = self.chat_message(message).await?;
        let id = ChatProposalId::generate();
        sqlx::query(
            "INSERT INTO chat_proposals \
                 (tenant_id, id, channel_id, message_id, asked_by, tool, args) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(held.channel.as_str())
        .bind(message.as_str())
        .bind(self.user.as_str())
        .bind(tool)
        .bind(args)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(id)
    }

    /// One proposal, if its room is the caller's to see.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when it does not exist or its room is not
    /// visible — deliberately the same answer.
    pub async fn proposal(&self, id: &ChatProposalId) -> Result<ChatProposal> {
        // The channel comes last so the leading columns are exactly a
        // `ProposalRow` and the two readers share one shape.
        let found: Option<ProposalRowWithChannel> = sqlx::query_as(
            "SELECT id, message_id, asked_by, tool, args, state, decided_by, \
                    created_at, channel_id \
             FROM chat_proposals WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        let found = found.ok_or(StoreError::NotFound)?;
        let channel = ChatChannelId::new(found.8);
        let row: ProposalRow = (
            found.0, found.1, found.2, found.3, found.4, found.5, found.6, found.7,
        );
        // The room decides whether this proposal exists for the caller at all.
        self.channel(&channel).await?;
        row_to_proposal(row, channel)
    }

    /// Approve or turn down a proposal.
    ///
    /// **Only the asker may decide** — see the module note. Deciding happens
    /// once: a proposal already settled is a validation error saying what it
    /// became, never a silent second execution.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if it is not the caller's to see,
    /// [`StoreError::Forbidden`] if they are not the asker,
    /// [`StoreError::Validation`] if it was already decided.
    pub async fn decide_proposal(
        &self,
        id: &ChatProposalId,
        approve: bool,
    ) -> Result<ChatProposal> {
        let held = self.proposal(id).await?;
        if held.asked_by.as_str() != self.user.as_str() {
            return Err(StoreError::Forbidden);
        }
        if held.state != ProposalState::Pending {
            return Err(StoreError::Validation(format!(
                "that was already {}",
                held.state.as_str()
            )));
        }
        let next = if approve {
            ProposalState::Approved
        } else {
            ProposalState::Discarded
        };
        // Conditional on still being pending, so two taps cannot both win.
        let done = sqlx::query(
            "UPDATE chat_proposals \
             SET state = $3, decided_by = $4, decided_at = now() \
             WHERE tenant_id = $1 AND id = $2 AND state = 'pending'",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(next.as_str())
        .bind(self.user.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::Validation(
                "that was already decided".to_owned(),
            ));
        }
        self.proposal(id).await
    }

    /// The proposals on a page of messages, keyed by message id, so a feed
    /// draws its approval cards without a request per message.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn proposals_for_channel(
        &self,
        channel: &ChatChannelId,
        messages: &[ChatMessageId],
    ) -> Result<HashMap<String, ChatProposal>> {
        if messages.is_empty() {
            return Ok(HashMap::new());
        }
        let ids: Vec<String> = messages.iter().map(|m| m.as_str().to_owned()).collect();
        let rows: Vec<ProposalRow> = sqlx::query_as(
            "SELECT id, message_id, asked_by, tool, args, state, decided_by, created_at \
             FROM chat_proposals \
             WHERE tenant_id = $1 AND channel_id = $2 AND message_id = ANY($3)",
        )
        .bind(self.tenant.as_str())
        .bind(channel.as_str())
        .bind(&ids[..])
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;

        let mut out = HashMap::new();
        for row in rows {
            let message = row.1.clone();
            out.insert(message, row_to_proposal(row, channel.clone())?);
        }
        Ok(out)
    }
}
