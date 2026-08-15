//! Chat messages — what is said in a room, and who has read it (alo Chat,
//! ADR 0038, phase 3). The rooms themselves are [`crate::chat`]'s.
//!
//! Every message takes the next **per-channel sequence**, allocated inside the
//! posting transaction from the counter on the channel row (the
//! `mailboxes.uid_next` precedent). That one integer carries ordering,
//! pagination and read state: a page is "everything before seq N", and unread
//! is "seq greater than my cursor". Timestamps are shown, never trusted for
//! order.
//!
//! Reading follows the room's own visibility ([`crate::chat`]: a member sees
//! any room, everyone in the tenant sees a live public channel). **Posting
//! always requires membership** — reading a public room does not make you a
//! participant in it.

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::chat::{CHANNEL_COLUMNS, ChannelRow, ChatChannel, row_to_channel};
use crate::error::{Result, StoreError};
use crate::id::{ChatChannelId, ChatMessageId, UserId};

/// A message is a chat line, not a document — bounded well above anything a
/// person types and well below anything that belongs in Docs.
const MESSAGE_MAX_CHARS: usize = 8_000;
/// The most history one page may ask for.
const MESSAGE_PAGE_MAX: i64 = 200;
/// What a page returns when the caller does not say.
pub const MESSAGE_PAGE_DEFAULT: i64 = 50;

/// Who is speaking: a person, or the room narrating itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageKind {
    /// Someone said this.
    Text,
    /// The room said this about itself (joins, renames, and the like).
    System,
}

impl MessageKind {
    /// The token stored in the `kind` column.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::System => "system",
        }
    }

    fn parse(token: &str) -> Result<Self> {
        match token {
            "text" => Ok(Self::Text),
            "system" => Ok(Self::System),
            other => Err(StoreError::Validation(format!(
                "unknown message kind {other}"
            ))),
        }
    }
}

/// One message as a reader sees it.
#[derive(Debug, Clone)]
pub struct ChatMessage {
    /// Opaque id — what edits and deletes address.
    pub id: ChatMessageId,
    /// The room it belongs to.
    pub channel: ChatChannelId,
    /// Its position in that room: the ordering key, the page cursor, and what
    /// a read cursor is compared against.
    pub seq: i64,
    /// Who said it. A person's user id, or an agent's id when
    /// `author_is_agent` — the two never share a namespace, and a reader must
    /// use the flag rather than guess from the shape of the string.
    pub author: UserId,
    /// Whether the author is an agent rather than a person.
    pub author_is_agent: bool,
    /// On an agent's message, the person whose reach produced it. The room
    /// sees the agent; the record shows who asked.
    pub on_behalf_of: Option<UserId>,
    /// The text — empty when withdrawn (see `deleted_at`).
    pub body: String,
    /// A person's line, or the room's own narration.
    pub kind: MessageKind,
    /// The seq of the message this replies to; `None` in the main feed.
    pub thread_root_seq: Option<i64>,
    /// When it was said.
    pub created_at: OffsetDateTime,
    /// When it was last changed, if it was.
    pub edited_at: Option<OffsetDateTime>,
    /// When it was withdrawn, if it was — the row survives so the sequence
    /// never gains a hole, but the body is gone.
    pub deleted_at: Option<OffsetDateTime>,
}

/// A room with the two numbers a sidebar needs: how much is unread, and when
/// it last had life.
#[derive(Debug, Clone)]
pub struct ChatChannelSummary {
    /// The room itself.
    pub channel: ChatChannel,
    /// Who this one-to-one is with: the other member's address for a DM, and
    /// `@handle` for an agent DM (ADR 0048) — an agent has no address, and its
    /// handle is the name it is unique by in the tenant. `None` for named rooms
    /// or when the directory no longer resolves that member.
    pub counterpart: Option<String>,
    /// Messages after the caller's read cursor, theirs excluded, tombstones
    /// excluded.
    pub unread: i64,
    /// The caller's read cursor (0 = nothing read).
    pub last_read_seq: i64,
    /// The seq of the newest message, if the room has any.
    pub last_seq: Option<i64>,
    /// When that newest message arrived.
    pub last_at: Option<OffsetDateTime>,
    /// The newest surviving message body, for the sidebar preview.
    pub last_body: Option<String>,
}

/// A message as the main feed shows it: the message, plus what is hanging
/// under it.
///
/// The feed carries top-level messages only. A reply belongs to its thread,
/// not to the room's spine — showing both means a conversation is read twice
/// and out of order, which is the failure every threaded chat is judged on.
/// What the feed keeps instead is the count, so a thread announces itself
/// without being unrolled.
#[derive(Debug, Clone)]
pub struct ChatFeedMessage {
    /// The top-level message itself.
    pub message: ChatMessage,
    /// Replies under it, withdrawn ones excluded — a thread whose replies
    /// were all taken back reads as having none, which is the truth.
    pub reply_count: i64,
    /// When the newest surviving reply arrived, for "last reply 5m ago".
    pub last_reply_at: Option<OffsetDateTime>,
    /// Other room members whose read cursor has reached this message.
    /// This is computed by the store so clients never invent delivery state.
    pub read_by: i64,
}

const MESSAGE_COLUMNS: &str = "id, channel_id, seq, author_id, author_kind, on_behalf_of, body, kind, \
     thread_root_seq, created_at, edited_at, deleted_at";

type MessageRow = (
    String,
    String,
    i64,
    String,
    String,
    Option<String>,
    String,
    String,
    Option<i64>,
    OffsetDateTime,
    Option<OffsetDateTime>,
    Option<OffsetDateTime>,
);

fn row_to_message(row: MessageRow) -> Result<ChatMessage> {
    Ok(ChatMessage {
        id: ChatMessageId::new(row.0),
        channel: ChatChannelId::new(row.1),
        seq: row.2,
        author: UserId::new(row.3),
        author_is_agent: row.4 == "agent",
        on_behalf_of: row.5.map(UserId::new),
        body: row.6,
        kind: MessageKind::parse(&row.7)?,
        thread_root_seq: row.8,
        created_at: row.9,
        edited_at: row.10,
        deleted_at: row.11,
    })
}

fn validate_body(body: &str) -> Result<()> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return Err(StoreError::Validation("a message needs words".to_owned()));
    }
    if trimmed.chars().count() > MESSAGE_MAX_CHARS {
        return Err(StoreError::Validation(format!(
            "a message is at most {MESSAGE_MAX_CHARS} characters"
        )));
    }
    Ok(())
}

impl AccountStore {
    /// Say something in a room the caller belongs to, optionally as a reply to
    /// the message at `thread_root_seq`.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the room is not the caller's to see or they
    /// are not a member (reading a public room does not make you one),
    /// [`StoreError::Validation`] for an empty/over-long body, an archived
    /// room, or a reply to something that is not a top-level message here.
    pub async fn post_message(
        &self,
        channel: &ChatChannelId,
        body: &str,
        thread_root_seq: Option<i64>,
    ) -> Result<ChatMessage> {
        // A person posts as themselves, on nobody's behalf.
        let author = self.user.as_str().to_owned();
        self.insert_message(channel, &author, "user", None, body, thread_root_seq)
            .await
    }

    /// The one place a message is written, for a person or an agent alike.
    ///
    /// `author_kind` and `on_behalf_of` are what separate them: an agent posts
    /// under its own id and records the person whose reach produced the turn.
    /// Sharing this body is the point — sequence allocation, the threading
    /// rule and the archived-room refusal must not grow two implementations
    /// that can drift apart.
    ///
    /// Membership is checked against **the caller**, never the author: an
    /// agent posts because a member asked it to, and it is that member's
    /// standing in the room that permits it.
    pub(crate) async fn insert_message(
        &self,
        channel: &ChatChannelId,
        author: &str,
        author_kind: &str,
        on_behalf_of: Option<&str>,
        body: &str,
        thread_root_seq: Option<i64>,
    ) -> Result<ChatMessage> {
        validate_body(body)?;
        let room = self.channel(channel).await?;
        if self.channel_role(channel).await?.is_none() {
            return Err(StoreError::NotFound);
        }
        if room.archived_at.is_some() {
            return Err(StoreError::Validation(
                "this conversation is archived".to_owned(),
            ));
        }

        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        // One level of threading: a reply's root must exist here and be a root
        // itself, so a thread can never grow a thread.
        if let Some(root) = thread_root_seq {
            let ok: Option<(i64,)> = sqlx::query_as(
                "SELECT seq FROM chat_messages \
                 WHERE tenant_id = $1 AND channel_id = $2 AND seq = $3 \
                   AND thread_root_seq IS NULL AND deleted_at IS NULL",
            )
            .bind(self.tenant.as_str())
            .bind(channel.as_str())
            .bind(root)
            .fetch_optional(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
            if ok.is_none() {
                return Err(StoreError::Validation(
                    "that message is not here to reply to".to_owned(),
                ));
            }
        }

        // Allocate this room's next position, locking the channel row for the
        // rest of the transaction — two people posting at once get two seqs.
        let (seq,): (i64,) = sqlx::query_as(
            "UPDATE chat_channels SET next_seq = next_seq + 1 \
             WHERE tenant_id = $1 AND id = $2 RETURNING next_seq - 1",
        )
        .bind(self.tenant.as_str())
        .bind(channel.as_str())
        .fetch_one(&mut *tx)
        .await
        .map_err(StoreError::Db)?;

        let id = ChatMessageId::generate();
        let row: MessageRow = sqlx::query_as(&format!(
            "INSERT INTO chat_messages \
                 (tenant_id, channel_id, id, seq, author_id, author_kind, \
                  on_behalf_of, body, thread_root_seq) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING {MESSAGE_COLUMNS}"
        ))
        .bind(self.tenant.as_str())
        .bind(channel.as_str())
        .bind(id.as_str())
        .bind(seq)
        .bind(author)
        .bind(author_kind)
        .bind(on_behalf_of)
        .bind(body.trim())
        .bind(thread_root_seq)
        .fetch_one(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;
        let message = row_to_message(row)?;
        // Resolved now, while the words are being written, so "is there
        // something here for me?" is later an index lookup and not a text
        // scan. After the commit: a message that was said is said, and a
        // mention that failed to record must not unsay it.
        //
        // Only for a person's words: an agent naming its asker is answering
        // them, not summoning them, and must not badge them for it.
        if author_kind == "user" {
            let _ = self
                .record_mentions(channel, &message.id, message.seq, &message.body)
                .await;
        }
        Ok(message)
    }

    /// A page of a room's main feed, newest first. `before` is a seq cursor:
    /// pass the oldest seq you already have to walk further back.
    ///
    /// **Top-level messages only** — a reply is reached through
    /// [`thread_replies`](Self::thread_replies), and is announced here by its
    /// root's `reply_count`. The cursor still walks the room's own sequence,
    /// so paging stays exact even though the numbers it skips are the replies
    /// living in threads.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the room is not the caller's to see.
    pub async fn messages(
        &self,
        channel: &ChatChannelId,
        before: Option<i64>,
        limit: i64,
    ) -> Result<Vec<ChatFeedMessage>> {
        self.channel(channel).await?;
        let limit = limit.clamp(1, MESSAGE_PAGE_MAX);
        // The message's own columns, then the three the feed adds.
        type FeedRow = (
            String,
            String,
            i64,
            String,
            String,
            Option<String>,
            String,
            String,
            Option<i64>,
            OffsetDateTime,
            Option<OffsetDateTime>,
            Option<OffsetDateTime>,
            i64,
            Option<OffsetDateTime>,
            i64,
        );
        let rows: Vec<FeedRow> = sqlx::query_as(&format!(
            "SELECT {}, \
                 (SELECT count(*) FROM chat_messages r \
                  WHERE r.tenant_id = m.tenant_id AND r.channel_id = m.channel_id \
                    AND r.thread_root_seq = m.seq AND r.deleted_at IS NULL) AS reply_count, \
                 (SELECT max(r.created_at) FROM chat_messages r \
                  WHERE r.tenant_id = m.tenant_id AND r.channel_id = m.channel_id \
                    AND r.thread_root_seq = m.seq AND r.deleted_at IS NULL) AS last_reply_at, \
                 (SELECT count(*) FROM chat_members reader \
                  WHERE reader.tenant_id = m.tenant_id AND reader.channel_id = m.channel_id \
                    AND reader.user_id <> m.author_id AND reader.last_read_seq >= m.seq) AS read_by \
             FROM chat_messages m \
             WHERE m.tenant_id = $1 AND m.channel_id = $2 \
               AND m.thread_root_seq IS NULL \
               AND ($3::bigint IS NULL OR m.seq < $3) \
             ORDER BY m.seq DESC LIMIT $4",
            MESSAGE_COLUMNS
                .split(", ")
                .map(|column| format!("m.{}", column.trim()))
                .collect::<Vec<_>>()
                .join(", ")
        ))
        .bind(self.tenant.as_str())
        .bind(channel.as_str())
        .bind(before)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter()
            .map(|r| {
                let message: MessageRow =
                    (r.0, r.1, r.2, r.3, r.4, r.5, r.6, r.7, r.8, r.9, r.10, r.11);
                Ok(ChatFeedMessage {
                    message: row_to_message(message)?,
                    reply_count: r.12,
                    last_reply_at: r.13,
                    read_by: r.14,
                })
            })
            .collect()
    }

    /// Find messages the caller may read, newest first.
    ///
    /// Visibility is the room's, applied in SQL and identical to
    /// [`channel`](Self::channel)'s rule: a room you are in, or a live public
    /// channel. Search must never be the one place a private room leaks, so
    /// the predicate is written here rather than filtered afterwards — a
    /// post-filter is a leak waiting for someone to forget it.
    ///
    /// Withdrawn messages are excluded: their words are gone, and a hit with
    /// nothing to show is noise. `channel` narrows to one room.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn search_messages(
        &self,
        query: &str,
        channel: Option<&ChatChannelId>,
        limit: i64,
    ) -> Result<Vec<ChatMessage>> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let limit = limit.clamp(1, MESSAGE_PAGE_MAX);
        let rows: Vec<MessageRow> = sqlx::query_as(&format!(
            "SELECT {} FROM chat_messages m              JOIN chat_channels c                ON c.tenant_id = m.tenant_id AND c.id = m.channel_id              WHERE m.tenant_id = $1                AND m.deleted_at IS NULL                AND ($4::text IS NULL OR m.channel_id = $4)                AND to_tsvector('simple', m.body) @@ plainto_tsquery('simple', $2)                AND (                  EXISTS (SELECT 1 FROM chat_members mm                          WHERE mm.tenant_id = c.tenant_id AND mm.channel_id = c.id                            AND mm.user_id = $3)                  OR (c.visibility = 'public' AND c.archived_at IS NULL))              ORDER BY m.created_at DESC LIMIT $5",
            MESSAGE_COLUMNS
                .split(", ")
                .map(|column| format!("m.{}", column.trim()))
                .collect::<Vec<_>>()
                .join(", ")
        ))
        .bind(self.tenant.as_str())
        .bind(query)
        .bind(self.user.as_str())
        .bind(channel.map(ChatChannelId::as_str))
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter().map(row_to_message).collect()
    }

    /// The replies gathered under one top-level message, oldest first.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the room is not the caller's to see.
    pub async fn thread_replies(
        &self,
        channel: &ChatChannelId,
        root_seq: i64,
    ) -> Result<Vec<ChatMessage>> {
        self.channel(channel).await?;
        let rows: Vec<MessageRow> = sqlx::query_as(&format!(
            "SELECT {MESSAGE_COLUMNS} FROM chat_messages \
             WHERE tenant_id = $1 AND channel_id = $2 AND thread_root_seq = $3 \
             ORDER BY seq"
        ))
        .bind(self.tenant.as_str())
        .bind(channel.as_str())
        .bind(root_seq)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter().map(row_to_message).collect()
    }

    /// One message, if its room is the caller's to see.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when it does not exist or its room is not
    /// visible to the caller — deliberately the same answer.
    pub async fn chat_message(&self, id: &ChatMessageId) -> Result<ChatMessage> {
        let row: Option<MessageRow> = sqlx::query_as(&format!(
            "SELECT {MESSAGE_COLUMNS} FROM chat_messages \
             WHERE tenant_id = $1 AND id = $2"
        ))
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        let message = row_to_message(row.ok_or(StoreError::NotFound)?)?;
        // The room decides whether this message exists for the caller at all.
        self.channel(&message.channel).await?;
        Ok(message)
    }

    /// Rewrite one's own message. The sequence, and therefore everyone's read
    /// state, is untouched.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the message is not visible,
    /// [`StoreError::Forbidden`] if it is someone else's,
    /// [`StoreError::Validation`] for a bad body or a withdrawn message.
    pub async fn edit_message(&self, id: &ChatMessageId, body: &str) -> Result<ChatMessage> {
        validate_body(body)?;
        let existing = self.chat_message(id).await?;
        if existing.author.as_str() != self.user.as_str() {
            return Err(StoreError::Forbidden);
        }
        if existing.deleted_at.is_some() {
            return Err(StoreError::Validation(
                "that message was withdrawn".to_owned(),
            ));
        }
        let row: MessageRow = sqlx::query_as(&format!(
            "UPDATE chat_messages SET body = $3, edited_at = now() \
             WHERE tenant_id = $1 AND id = $2 RETURNING {MESSAGE_COLUMNS}"
        ))
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(body.trim())
        .fetch_one(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        let message = row_to_message(row)?;
        // Re-derived, not left alone: editing a message to add a name must
        // reach that person, and editing one out must stop badging them.
        let _ = self
            .record_mentions(&message.channel, &message.id, message.seq, &message.body)
            .await;
        Ok(message)
    }

    /// Withdraw one's own message: the body goes, the row stays as a tombstone
    /// so the room's numbering (and everyone's read state) is unharmed.
    /// Withdrawing twice is not an error.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the message is not visible,
    /// [`StoreError::Forbidden`] if it is someone else's.
    pub async fn delete_message(&self, id: &ChatMessageId) -> Result<()> {
        let existing = self.chat_message(id).await?;
        if existing.author.as_str() != self.user.as_str() {
            return Err(StoreError::Forbidden);
        }
        sqlx::query(
            "UPDATE chat_messages SET body = '', deleted_at = now() \
             WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        // The words are gone, so the mentions in them are too — a badge
        // pointing at an empty tombstone is a promise the room cannot keep.
        sqlx::query("DELETE FROM chat_mentions WHERE tenant_id = $1 AND message_id = $2")
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .execute(&self.pool)
            .await
            .map_err(StoreError::Db)?;
        Ok(())
    }

    /// Move the caller's read cursor forward in a room. Never moves backwards,
    /// and never past what the room has actually said.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the caller is not a member of the room.
    pub async fn mark_read(&self, channel: &ChatChannelId, seq: i64) -> Result<()> {
        let updated = sqlx::query(
            "UPDATE chat_members m SET last_read_seq = GREATEST( \
                 m.last_read_seq, \
                 LEAST($3, (SELECT c.next_seq - 1 FROM chat_channels c \
                            WHERE c.tenant_id = m.tenant_id AND c.id = m.channel_id))) \
             WHERE m.tenant_id = $1 AND m.channel_id = $2 AND m.user_id = $4",
        )
        .bind(self.tenant.as_str())
        .bind(channel.as_str())
        .bind(seq)
        .bind(self.user.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if updated.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// The caller's rooms with what a sidebar needs: unread count and last
    /// activity, liveliest first. Archived rooms sink to the bottom.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn channel_summaries(&self) -> Result<Vec<ChatChannelSummary>> {
        type SummaryRow = (
            String,
            String,
            Option<String>,
            Option<String>,
            String,
            Option<String>,
            String,
            OffsetDateTime,
            Option<OffsetDateTime>,
            i64,
            i64,
            Option<i64>,
            Option<OffsetDateTime>,
            Option<String>,
            Option<String>,
        );
        let rows: Vec<SummaryRow> = sqlx::query_as(&format!(
            "SELECT {} , \
                 m.last_read_seq, \
                 (SELECT count(*) FROM chat_messages x \
                  WHERE x.tenant_id = c.tenant_id AND x.channel_id = c.id \
                    AND x.seq > m.last_read_seq AND x.author_id <> $2 \
                    AND x.deleted_at IS NULL) AS unread, \
                 (SELECT max(x.seq) FROM chat_messages x \
                  WHERE x.tenant_id = c.tenant_id AND x.channel_id = c.id) AS last_seq, \
                 (SELECT max(x.created_at) FROM chat_messages x \
                  WHERE x.tenant_id = c.tenant_id AND x.channel_id = c.id) AS last_at, \
                 (SELECT x.body FROM chat_messages x \
                  WHERE x.tenant_id = c.tenant_id AND x.channel_id = c.id \
                    AND x.deleted_at IS NULL \
                  ORDER BY x.seq DESC LIMIT 1) AS last_body, \
                 CASE WHEN c.kind = 'dm' THEN ( \
                   SELECT u.email FROM chat_members other \
                   JOIN users u ON u.tenant_id = other.tenant_id AND u.id = other.user_id \
                   WHERE other.tenant_id = c.tenant_id AND other.channel_id = c.id \
                     AND other.user_id <> $2 \
                   ORDER BY other.joined_at LIMIT 1 \
                 ) WHEN c.kind = 'agent_dm' THEN ( \
                   SELECT '@' || a.handle FROM chat_agents a \
                   WHERE a.tenant_id = c.tenant_id AND a.id = c.agent_id \
                 ) END AS counterpart \
             FROM chat_channels c \
             JOIN chat_members m \
               ON m.tenant_id = c.tenant_id AND m.channel_id = c.id AND m.user_id = $2 \
             WHERE c.tenant_id = $1 \
             ORDER BY c.archived_at NULLS FIRST, last_at DESC NULLS LAST, c.created_at DESC",
            CHANNEL_COLUMNS
                .split(", ")
                .map(|column| format!("c.{column}"))
                .collect::<Vec<_>>()
                .join(", ")
        ))
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;

        rows.into_iter()
            .map(|r| {
                let channel: ChannelRow = (r.0, r.1, r.2, r.3, r.4, r.5, r.6, r.7, r.8);
                Ok(ChatChannelSummary {
                    channel: row_to_channel(channel)?,
                    last_read_seq: r.9,
                    unread: r.10,
                    last_seq: r.11,
                    last_at: r.12,
                    last_body: r.13,
                    counterpart: r.14,
                })
            })
            .collect()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn a_message_needs_words_and_stays_a_message() {
        assert!(validate_body("hello").is_ok());
        assert!(validate_body("   ").is_err());
        assert!(validate_body("").is_err());
        let long = "x".repeat(MESSAGE_MAX_CHARS + 1);
        assert!(validate_body(&long).is_err());
        assert!(validate_body(&long[..MESSAGE_MAX_CHARS]).is_ok());
    }

    #[test]
    fn message_kinds_round_trip_through_their_column() {
        for kind in [MessageKind::Text, MessageKind::System] {
            assert_eq!(MessageKind::parse(kind.as_str()).unwrap(), kind);
        }
        assert!(MessageKind::parse("shout").is_err());
    }
}
