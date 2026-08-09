//! Reacting to a message (alo Chat, ADR 0038).
//!
//! A reaction is one person, one message, one emoji — and the primary key says
//! so. Reacting twice with the same emoji is a toggle back off, enforced by the
//! table rather than by the application counting and hoping.
//!
//! Counts are derived, never stored. A tally is one cheap aggregate and cannot
//! disagree with the rows it came from; a stored counter is a second source of
//! truth that drifts the first time a delete races an insert.
//!
//! Visibility follows the message's room, exactly as reading does. **Reacting
//! requires membership** — the same rule as posting, for the same reason: a
//! reaction is a contribution to the conversation, and reading a public room
//! does not make you a participant in it.

use std::collections::HashMap;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::{ChatChannelId, ChatMessageId, UserId};

/// The reactions a person may leave.
///
/// A fixed set rather than free text, because the field is displayed to
/// everyone in the room: arbitrary strings would make it a second, unmoderated
/// message body. It lives here rather than in a `CHECK` constraint so that
/// growing the set is a release, not a migration on every tenant's database.
///
/// The order is the order a picker should offer them in — agreement first,
/// because that is what most reactions are.
pub const REACTIONS: [&str; 10] = ["👍", "🎉", "❤️", "😄", "👀", "🙏", "✅", "😮", "😢", "👎"];

/// One emoji on one message, with how many people chose it.
#[derive(Debug, Clone)]
pub struct ReactionTally {
    /// Which reaction, always one of [`REACTIONS`].
    pub emoji: String,
    /// How many people have left it.
    pub count: i64,
    /// Whether the caller is one of them — what makes the chip a toggle
    /// rather than a counter.
    pub mine: bool,
}

fn validate_emoji(emoji: &str) -> Result<()> {
    if REACTIONS.contains(&emoji) {
        return Ok(());
    }
    Err(StoreError::Validation(format!(
        "{emoji} is not a reaction that can be left here"
    )))
}

impl AccountStore {
    /// Add the caller's reaction to a message, or take it back if it is
    /// already there. Returns `true` when it is now set.
    ///
    /// Idempotent in both directions: reacting twice leaves it off, and
    /// un-reacting something never reacted to is not an error.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the message is not the caller's to see, or
    /// they are not a member of its room; [`StoreError::Validation`] for an
    /// emoji outside [`REACTIONS`], a withdrawn message, or an archived room.
    pub async fn toggle_reaction(&self, id: &ChatMessageId, emoji: &str) -> Result<bool> {
        validate_emoji(emoji)?;
        // Visibility first, then membership — the same order posting uses, so
        // a non-member of a private room learns nothing beyond "not found".
        let message = self.chat_message(id).await?;
        if self.channel_role(&message.channel).await?.is_none() {
            return Err(StoreError::NotFound);
        }
        if message.deleted_at.is_some() {
            return Err(StoreError::Validation(
                "that message was withdrawn".to_owned(),
            ));
        }
        let room = self.channel(&message.channel).await?;
        if room.archived_at.is_some() {
            return Err(StoreError::Validation(
                "this conversation is archived".to_owned(),
            ));
        }

        // One statement decides which way the toggle goes: the delete reports
        // whether a row was there, so there is no read-then-write window for a
        // double click to slip through.
        let removed = sqlx::query(
            "DELETE FROM chat_reactions \
             WHERE tenant_id = $1 AND message_id = $2 AND user_id = $3 AND emoji = $4",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(self.user.as_str())
        .bind(emoji)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?
        .rows_affected();
        if removed > 0 {
            return Ok(false);
        }

        sqlx::query(
            "INSERT INTO chat_reactions (tenant_id, channel_id, message_id, user_id, emoji) \
             VALUES ($1, $2, $3, $4, $5) ON CONFLICT DO NOTHING",
        )
        .bind(self.tenant.as_str())
        .bind(message.channel.as_str())
        .bind(id.as_str())
        .bind(self.user.as_str())
        .bind(emoji)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(true)
    }

    /// Tallies for one message, in [`REACTIONS`] order.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the message is not the caller's to see.
    pub async fn message_reactions(&self, id: &ChatMessageId) -> Result<Vec<ReactionTally>> {
        let message = self.chat_message(id).await?;
        let mut tallies = self
            .reactions_for_channel(&message.channel, std::slice::from_ref(id))
            .await?;
        Ok(tallies.remove(id.as_str()).unwrap_or_default())
    }

    /// Tallies for a whole page of messages, keyed by message id.
    ///
    /// One query for the page rather than one per message: a page of fifty
    /// messages should cost one aggregate, not fifty round trips. Messages
    /// with no reactions are simply absent from the map.
    ///
    /// The room is *not* re-checked here — callers reach this with messages
    /// they already read from a room they were allowed to read, and the query
    /// is bounded by both the tenant and that channel.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn reactions_for_channel(
        &self,
        channel: &ChatChannelId,
        messages: &[ChatMessageId],
    ) -> Result<HashMap<String, Vec<ReactionTally>>> {
        if messages.is_empty() {
            return Ok(HashMap::new());
        }
        let ids: Vec<String> = messages.iter().map(|m| m.as_str().to_owned()).collect();
        let rows: Vec<(String, String, i64, bool)> = sqlx::query_as(
            "SELECT message_id, emoji, count(*) AS n, \
                    bool_or(user_id = $4) AS mine \
             FROM chat_reactions \
             WHERE tenant_id = $1 AND channel_id = $2 AND message_id = ANY($3) \
             GROUP BY message_id, emoji",
        )
        .bind(self.tenant.as_str())
        .bind(channel.as_str())
        .bind(&ids[..])
        .bind(self.user.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;

        let mut out: HashMap<String, Vec<ReactionTally>> = HashMap::new();
        for (message_id, emoji, count, mine) in rows {
            out.entry(message_id)
                .or_default()
                .push(ReactionTally { emoji, count, mine });
        }
        // A stable order, and the same one the picker offers, so a chip never
        // moves under a cursor because someone else reacted.
        let rank = |emoji: &str| {
            REACTIONS
                .iter()
                .position(|r| *r == emoji)
                .unwrap_or(REACTIONS.len())
        };
        for tallies in out.values_mut() {
            tallies.sort_by_key(|t| rank(&t.emoji));
        }
        Ok(out)
    }

    /// Who left a particular reaction — for the "Anna, Ben and 3 others"
    /// tooltip a chip carries.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the message is not the caller's to see.
    pub async fn reaction_users(&self, id: &ChatMessageId, emoji: &str) -> Result<Vec<UserId>> {
        validate_emoji(emoji)?;
        self.chat_message(id).await?;
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT user_id FROM chat_reactions \
             WHERE tenant_id = $1 AND message_id = $2 AND emoji = $3 \
             ORDER BY created_at",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(emoji)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(|r| UserId::new(r.0)).collect())
    }
}
