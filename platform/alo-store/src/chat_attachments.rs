//! Sharing a file in a conversation (alo Chat, ADR 0038).
//!
//! A chat attachment is a **pointer to a Drive node, never a copy**. The file
//! keeps living in Drive, with one set of permissions, one version history and
//! one place it can be deleted from. Copying the bytes into a chat table would
//! fork all three, and the second copy is the one that outlives its permission
//! grant. This follows alo Finance's receipt pointer (ADR 0035) and alo Base's
//! node reference (ADR 0032).
//!
//! Access is checked **twice, on purpose**:
//!
//! * **On the way in** — you may only share a file you can already open, so a
//!   room cannot be used to launder a file id into somewhere it does not
//!   belong.
//! * **On the way out** — every pointer is re-resolved through Drive's own
//!   access check, and one that no longer resolves is dropped from the
//!   payload. Access changes after the words are said: a Space is unshared,
//!   a file is trashed, someone leaves a team. Because an attachment shows the
//!   file's *name*, a single check at write time would leave that name on
//!   display in a room long after the reader lost the right to it.
//!
//! That second check is what alo Base does (`base()` re-opens through
//! `drive_require_read`), and what alo Finance does not need to do — finance
//! returns only an opaque node id, which discloses nothing on its own.

use std::collections::HashMap;

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::{ChatChannelId, ChatMessageId, DriveNodeId};

/// The most files one message may carry. A conversation is not a folder; past
/// a handful, what is wanted is a Drive link to the folder itself.
pub const ATTACHMENTS_MAX: usize = 10;

/// A shared file as a reader sees it: the pointer, plus what Drive says about
/// it *right now*.
///
/// The name and size are read live rather than stored, so a renamed file shows
/// its current name and a file the reader may no longer open never appears at
/// all.
#[derive(Debug, Clone)]
pub struct ChatAttachment {
    /// The Drive node. What a client opens with
    /// `GET /drive/nodes/{id}/download`.
    pub node: DriveNodeId,
    /// Drive's current name for it.
    pub name: String,
    /// Bytes, as Drive reports them.
    pub size: i64,
    /// What it is, when Drive knows.
    pub content_type: Option<String>,
    /// Whether Drive has it in the trash. Still shown — a colleague saying
    /// "I trashed that" is information — but shown as trashed.
    pub trashed: bool,
    /// When it was shared here.
    pub shared_at: OffsetDateTime,
}

/// What Drive says about a node right now. `None` in the resolve map means
/// the node no longer resolves *for this reader* — deleted, or no longer
/// theirs — which are the same answer here.
struct NodeFacts {
    name: String,
    size: i64,
    content_type: Option<String>,
    trashed: bool,
}

impl AccountStore {
    /// Record `nodes` as the files shared on `message`.
    ///
    /// Every node is checked against Drive's own read gate first: you may only
    /// share a file you can already open. Duplicates collapse, and the order
    /// given is the order kept.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the message is not the caller's to see or a
    /// node is not theirs to open — deliberately the same answer, so neither
    /// can be used to test whether the other exists.
    /// [`StoreError::Validation`] past [`ATTACHMENTS_MAX`].
    pub async fn attach_files(
        &self,
        message: &ChatMessageId,
        nodes: &[DriveNodeId],
    ) -> Result<Vec<DriveNodeId>> {
        if nodes.is_empty() {
            return Ok(Vec::new());
        }
        if nodes.len() > ATTACHMENTS_MAX {
            return Err(StoreError::Validation(format!(
                "a message carries at most {ATTACHMENTS_MAX} files"
            )));
        }
        let held = self.chat_message(message).await?;

        let mut kept: Vec<DriveNodeId> = Vec::new();
        for node in nodes {
            if kept.iter().any(|k| k.as_str() == node.as_str()) {
                continue;
            }
            // The gate Drive publishes for exactly this: NotFound when the
            // caller cannot read the node's location.
            self.drive_require_read(node).await?;
            kept.push(node.clone());
        }

        for (position, node) in kept.iter().enumerate() {
            let position = i32::try_from(position).unwrap_or(i32::MAX);
            sqlx::query(
                "INSERT INTO chat_attachments \
                     (tenant_id, channel_id, message_id, node_id, position) \
                 VALUES ($1, $2, $3, $4, $5) \
                 ON CONFLICT (tenant_id, message_id, node_id) DO NOTHING",
            )
            .bind(self.tenant.as_str())
            .bind(held.channel.as_str())
            .bind(message.as_str())
            .bind(node.as_str())
            .bind(position)
            .execute(&self.pool)
            .await
            .map_err(StoreError::Db)?;
        }
        Ok(kept)
    }

    /// The files shared on a page of messages, keyed by message id.
    ///
    /// Every pointer is re-resolved through Drive's access check, and any the
    /// caller may no longer open is dropped rather than shown as a name they
    /// have no right to. Messages sharing nothing are absent from the map.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn attachments_for_channel(
        &self,
        channel: &ChatChannelId,
        messages: &[ChatMessageId],
    ) -> Result<HashMap<String, Vec<ChatAttachment>>> {
        if messages.is_empty() {
            return Ok(HashMap::new());
        }
        let ids: Vec<String> = messages.iter().map(|m| m.as_str().to_owned()).collect();
        let rows: Vec<(String, String, OffsetDateTime)> = sqlx::query_as(
            "SELECT message_id, node_id, created_at FROM chat_attachments \
             WHERE tenant_id = $1 AND channel_id = $2 AND message_id = ANY($3) \
             ORDER BY message_id, position, created_at",
        )
        .bind(self.tenant.as_str())
        .bind(channel.as_str())
        .bind(&ids[..])
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;

        // One resolve per distinct file, not per row: the same spec attached
        // in three messages is one lookup.
        let mut seen: HashMap<String, Option<NodeFacts>> = HashMap::new();
        let mut out: HashMap<String, Vec<ChatAttachment>> = HashMap::new();
        for (message_id, node_id, shared_at) in rows {
            if !seen.contains_key(&node_id) {
                let node = DriveNodeId::new(node_id.clone());
                // `drive_node` answers None both for "gone" and for "not
                // yours" — which is the answer we want either way.
                let resolved = self.drive_node(&node).await.ok().flatten();
                seen.insert(
                    node_id.clone(),
                    resolved.map(|n| NodeFacts {
                        name: n.name,
                        size: n.size,
                        content_type: n.content_type,
                        trashed: n.trashed,
                    }),
                );
            }
            let Some(Some(facts)) = seen.get(&node_id) else {
                continue; // no longer resolvable for this reader: not shown
            };
            out.entry(message_id).or_default().push(ChatAttachment {
                node: DriveNodeId::new(node_id),
                name: facts.name.clone(),
                size: facts.size,
                content_type: facts.content_type.clone(),
                trashed: facts.trashed,
                shared_at,
            });
        }
        Ok(out)
    }

    /// Say something and share files with it, refusing before a word is said
    /// if any of them is not the caller's to share.
    ///
    /// The order matters: checking the files first means a rejected share
    /// leaves no message behind. Attaching after posting would answer "you
    /// cannot share that" to someone whose words are already in the room.
    ///
    /// # Errors
    /// Everything [`post_message`](Self::post_message) can raise, plus
    /// [`StoreError::NotFound`] for a file the caller cannot open and
    /// [`StoreError::Validation`] past [`ATTACHMENTS_MAX`].
    pub async fn post_message_with_files(
        &self,
        channel: &ChatChannelId,
        body: &str,
        thread_root_seq: Option<i64>,
        files: &[DriveNodeId],
    ) -> Result<crate::chat_messages::ChatMessage> {
        if files.len() > ATTACHMENTS_MAX {
            return Err(StoreError::Validation(format!(
                "a message carries at most {ATTACHMENTS_MAX} files"
            )));
        }
        for node in files {
            self.drive_require_read(node).await?;
        }
        let message = self.post_message(channel, body, thread_root_seq).await?;
        self.attach_files(&message.id, files).await?;
        Ok(message)
    }

    /// The files shared on one message, for the answer to posting it.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the message is not the caller's to see.
    pub async fn message_attachments(&self, id: &ChatMessageId) -> Result<Vec<ChatAttachment>> {
        let message = self.chat_message(id).await?;
        let mut found = self
            .attachments_for_channel(&message.channel, std::slice::from_ref(id))
            .await?;
        Ok(found.remove(id.as_str()).unwrap_or_default())
    }
}
