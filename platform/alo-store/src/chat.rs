//! Chat — channels, DMs and their membership (alo Chat, ADR 0038), reached
//! through the account door like [`crate::tasks`] and [`crate::sites`].
//!
//! **Membership is the permission** (`docs/design/chat.md`): a room the caller
//! may not see answers [`StoreError::NotFound`], never `Forbidden`, so a
//! private room's existence is never disclosed. A *public* channel is visible
//! to — and joinable by — every user of the tenant; a *private* channel and
//! every DM are visible only to their members. Nothing here is addressable
//! across tenants: chat has no global surface at all.
//!
//! Messages, the per-channel sequence and the read cursor's movement land in
//! the next phase; this module owns the rooms and who is in them.

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::{ChatAgentId, ChatChannelId, UserId};

/// A channel name is a human label (`#general`), bounded for sanity.
const CHANNEL_NAME_MAX_CHARS: usize = 80;
/// A topic is one line describing the room, never a document.
const CHANNEL_TOPIC_MAX_CHARS: usize = 300;

/// What a room is: a named channel, the pair a DM is, or the one-to-one a
/// person has with an agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelKind {
    /// A named room: `#general`, `#sales`.
    Channel,
    /// A direct conversation between exactly two people.
    Dm,
    /// A direct conversation between one person and one agent (ADR 0048).
    ///
    /// Not a [`Self::Dm`] with something else in it: `dm_key` is a pair of
    /// **user** ids and an agent is deliberately not a user, so this is its own
    /// kind and every query that switches on `kind` refuses it rather than
    /// misreading it as two humans.
    AgentDm,
}

impl ChannelKind {
    /// The token stored in the `kind` column.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Channel => "channel",
            Self::Dm => "dm",
            Self::AgentDm => "agent_dm",
        }
    }

    /// Whether this is a one-to-one — with a person or with an agent.
    ///
    /// The rules a DM has because it is a one-to-one (its members are fixed
    /// when it is opened, it has no name to change, it is not archived) are the
    /// same rules an agent DM has, and asking this once is what keeps the two
    /// from drifting apart a rule at a time.
    #[must_use]
    pub fn is_direct(self) -> bool {
        matches!(self, Self::Dm | Self::AgentDm)
    }

    fn parse(token: &str) -> Result<Self> {
        match token {
            "channel" => Ok(Self::Channel),
            "dm" => Ok(Self::Dm),
            "agent_dm" => Ok(Self::AgentDm),
            other => Err(StoreError::Validation(format!(
                "unknown channel kind {other}"
            ))),
        }
    }
}

/// Who may see a named channel. A DM is always [`ChannelVisibility::Private`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelVisibility {
    /// Any user of the tenant may see and join it.
    Public,
    /// Only members may see it at all.
    Private,
}

impl ChannelVisibility {
    /// The token stored in the `visibility` column.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Private => "private",
        }
    }

    fn parse(token: &str) -> Result<Self> {
        match token {
            "public" => Ok(Self::Public),
            "private" => Ok(Self::Private),
            other => Err(StoreError::Validation(format!(
                "unknown channel visibility {other}"
            ))),
        }
    }
}

/// What a member may do to the room itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberRole {
    /// May rename, retopic, archive, and remove other members.
    Owner,
    /// May read and post.
    Member,
}

impl MemberRole {
    /// The token stored in the `role` column.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Member => "member",
        }
    }

    fn parse(token: &str) -> Result<Self> {
        match token {
            "owner" => Ok(Self::Owner),
            "member" => Ok(Self::Member),
            other => Err(StoreError::Validation(format!(
                "unknown member role {other}"
            ))),
        }
    }
}

/// One room as the caller sees it.
#[derive(Debug, Clone)]
pub struct ChatChannel {
    /// Opaque id.
    pub id: ChatChannelId,
    /// Named room or DM.
    pub kind: ChannelKind,
    /// The `#name` of a named room; `None` for a DM.
    pub name: Option<String>,
    /// One line describing the room.
    pub topic: Option<String>,
    /// Public or private (a DM is always private).
    pub visibility: ChannelVisibility,
    /// The agent this room is the one-to-one **with** (ADR 0048); `None` for
    /// every other kind. Never a member id and never a user id: it is what
    /// makes an [`ChannelKind::AgentDm`] identifiable at all.
    pub agent: Option<ChatAgentId>,
    /// Who created it.
    pub created_by: UserId,
    /// When it was created.
    pub created_at: OffsetDateTime,
    /// Set when the room was archived; `None` while it is live.
    pub archived_at: Option<OffsetDateTime>,
}

/// One person in a room.
#[derive(Debug, Clone)]
pub struct ChatMember {
    /// Who.
    pub user: UserId,
    /// What they may do to the room.
    pub role: MemberRole,
    /// When they joined.
    pub joined_at: OffsetDateTime,
    /// The last per-channel sequence they have seen (0 = nothing).
    pub last_read_seq: i64,
    /// Whether they muted the room's notifications.
    pub muted: bool,
}

/// The columns every channel read selects, in [`row_to_channel`]'s order.
pub(crate) const CHANNEL_COLUMNS: &str =
    "id, kind, name, topic, visibility, agent_id, created_by, created_at, archived_at";

pub(crate) type ChannelRow = (
    String,
    String,
    Option<String>,
    Option<String>,
    String,
    Option<String>,
    String,
    OffsetDateTime,
    Option<OffsetDateTime>,
);

pub(crate) fn row_to_channel(row: ChannelRow) -> Result<ChatChannel> {
    Ok(ChatChannel {
        id: ChatChannelId::new(row.0),
        kind: ChannelKind::parse(&row.1)?,
        name: row.2,
        topic: row.3,
        visibility: ChannelVisibility::parse(&row.4)?,
        agent: row.5.map(ChatAgentId::new),
        created_by: UserId::new(row.6),
        created_at: row.7,
        archived_at: row.8,
    })
}

/// The DM key for a pair: both ids sorted and joined, so either person opening
/// the conversation lands in the same room. Ids are URL-safe base64, so `:`
/// cannot occur inside one and is a safe separator.
fn dm_key(a: &str, b: &str) -> String {
    if a <= b {
        format!("{a}:{b}")
    } else {
        format!("{b}:{a}")
    }
}

fn validate_channel_name(name: &str) -> Result<()> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(StoreError::Validation("a channel needs a name".to_owned()));
    }
    if trimmed.chars().count() > CHANNEL_NAME_MAX_CHARS {
        return Err(StoreError::Validation(format!(
            "a channel name is at most {CHANNEL_NAME_MAX_CHARS} characters"
        )));
    }
    Ok(())
}

fn validate_topic(topic: Option<&str>) -> Result<()> {
    if let Some(text) = topic
        && text.chars().count() > CHANNEL_TOPIC_MAX_CHARS
    {
        return Err(StoreError::Validation(format!(
            "a topic is at most {CHANNEL_TOPIC_MAX_CHARS} characters"
        )));
    }
    Ok(())
}

/// A duplicate live `#name` in the tenant, told apart from any other database
/// error by the unique index it violated.
fn map_name_taken(err: sqlx::Error) -> StoreError {
    if let sqlx::Error::Database(ref db) = err
        && db.constraint() == Some("chat_channels_name")
    {
        return StoreError::Conflict("a channel with that name already exists".to_owned());
    }
    StoreError::Db(err)
}

impl AccountStore {
    /// Create a named channel and put the caller in it as its owner.
    ///
    /// # Errors
    /// [`StoreError::Validation`] for an empty or over-long name/topic,
    /// [`StoreError::Conflict`] if the tenant already has a live channel of
    /// that name, [`StoreError::Db`] on failure.
    pub async fn create_channel(
        &self,
        name: &str,
        topic: Option<&str>,
        visibility: ChannelVisibility,
    ) -> Result<ChatChannelId> {
        validate_channel_name(name)?;
        validate_topic(topic)?;
        let id = ChatChannelId::generate();
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        sqlx::query(
            "INSERT INTO chat_channels \
                 (tenant_id, id, kind, name, topic, visibility, created_by) \
             VALUES ($1, $2, 'channel', $3, $4, $5, $6)",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(name.trim())
        .bind(topic.map(str::trim).filter(|t| !t.is_empty()))
        .bind(visibility.as_str())
        .bind(self.user.as_str())
        .execute(&mut *tx)
        .await
        .map_err(map_name_taken)?;
        sqlx::query(
            "INSERT INTO chat_members (tenant_id, channel_id, user_id, role) \
             VALUES ($1, $2, $3, 'owner')",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(self.user.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(id)
    }

    /// Open the DM between the caller and `other`, creating it once.
    ///
    /// Idempotent by construction: the pair's key carries a unique index, so
    /// two simultaneous opens still yield one room.
    ///
    /// # Errors
    /// [`StoreError::Validation`] for a DM with oneself,
    /// [`StoreError::NotFound`] if `other` is not a user of this tenant.
    pub async fn open_dm(&self, other: &UserId) -> Result<ChatChannelId> {
        if other.as_str() == self.user.as_str() {
            return Err(StoreError::Validation(
                "a direct message needs someone else".to_owned(),
            ));
        }
        // A DM partner must be a real user of this tenant — the account door
        // scopes the lookup, so a foreign id is simply absent.
        let exists: Option<(String,)> =
            sqlx::query_as("SELECT id FROM users WHERE tenant_id = $1 AND id = $2")
                .bind(self.tenant.as_str())
                .bind(other.as_str())
                .fetch_optional(&self.pool)
                .await
                .map_err(StoreError::Db)?;
        if exists.is_none() {
            return Err(StoreError::NotFound);
        }

        let key = dm_key(self.user.as_str(), other.as_str());
        if let Some(id) = self.dm_by_key(&key).await? {
            return Ok(id);
        }

        let id = ChatChannelId::generate();
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let inserted: Option<(String,)> = sqlx::query_as(
            "INSERT INTO chat_channels \
                 (tenant_id, id, kind, visibility, dm_key, created_by) \
             VALUES ($1, $2, 'dm', 'private', $3, $4) \
             ON CONFLICT (tenant_id, dm_key) WHERE dm_key IS NOT NULL DO NOTHING \
             RETURNING id",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(&key)
        .bind(self.user.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        let Some(_) = inserted else {
            // Someone opened the same conversation between our check and our
            // insert; theirs is the room.
            tx.rollback().await.map_err(StoreError::Db)?;
            return self.dm_by_key(&key).await?.ok_or(StoreError::NotFound);
        };
        for member in [self.user.as_str(), other.as_str()] {
            sqlx::query(
                "INSERT INTO chat_members (tenant_id, channel_id, user_id, role) \
                 VALUES ($1, $2, $3, 'member')",
            )
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .bind(member)
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        }
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(id)
    }

    async fn dm_by_key(&self, key: &str) -> Result<Option<ChatChannelId>> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT id FROM chat_channels WHERE tenant_id = $1 AND dm_key = $2")
                .bind(self.tenant.as_str())
                .bind(key)
                .fetch_optional(&self.pool)
                .await
                .map_err(StoreError::Db)?;
        Ok(row.map(|r| ChatChannelId::new(r.0)))
    }

    /// The rooms the caller is a member of, newest activity first (creation
    /// order until messages arrive), archived ones last.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn channels(&self) -> Result<Vec<ChatChannel>> {
        let rows: Vec<ChannelRow> = sqlx::query_as(&format!(
            "SELECT {CHANNEL_COLUMNS} FROM chat_channels c \
             WHERE c.tenant_id = $1 AND EXISTS ( \
                 SELECT 1 FROM chat_members m \
                 WHERE m.tenant_id = c.tenant_id AND m.channel_id = c.id \
                   AND m.user_id = $2) \
             ORDER BY c.archived_at NULLS FIRST, c.created_at DESC"
        ))
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter().map(row_to_channel).collect()
    }

    /// The live public channels of the tenant the caller has **not** joined —
    /// the "browse channels" list.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn joinable_channels(&self) -> Result<Vec<ChatChannel>> {
        let rows: Vec<ChannelRow> = sqlx::query_as(&format!(
            "SELECT {CHANNEL_COLUMNS} FROM chat_channels c \
             WHERE c.tenant_id = $1 AND c.kind = 'channel' \
               AND c.visibility = 'public' AND c.archived_at IS NULL \
               AND NOT EXISTS ( \
                 SELECT 1 FROM chat_members m \
                 WHERE m.tenant_id = c.tenant_id AND m.channel_id = c.id \
                   AND m.user_id = $2) \
             ORDER BY lower(c.name)"
        ))
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter().map(row_to_channel).collect()
    }

    /// One room, if the caller may see it: a member sees any room, a
    /// non-member sees only live public channels.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when it does not exist or is not the caller's
    /// to see — the two are deliberately indistinguishable.
    pub async fn channel(&self, id: &ChatChannelId) -> Result<ChatChannel> {
        let row: Option<ChannelRow> = sqlx::query_as(&format!(
            "SELECT {CHANNEL_COLUMNS} FROM chat_channels c \
             WHERE c.tenant_id = $1 AND c.id = $2 AND ( \
                 EXISTS (SELECT 1 FROM chat_members m \
                         WHERE m.tenant_id = c.tenant_id AND m.channel_id = c.id \
                           AND m.user_id = $3) \
                 OR (c.visibility = 'public' AND c.archived_at IS NULL))"
        ))
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(self.user.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        row_to_channel(row.ok_or(StoreError::NotFound)?)
    }

    /// Whether the caller is a member of `id` (and what they may do).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn channel_role(&self, id: &ChatChannelId) -> Result<Option<MemberRole>> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT role FROM chat_members \
             WHERE tenant_id = $1 AND channel_id = $2 AND user_id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(self.user.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        row.map(|r| MemberRole::parse(&r.0)).transpose()
    }

    /// The people in a room the caller can see.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the room is not the caller's to see.
    pub async fn channel_members(&self, id: &ChatChannelId) -> Result<Vec<ChatMember>> {
        self.channel(id).await?;
        let rows: Vec<(String, String, OffsetDateTime, i64, bool)> = sqlx::query_as(
            "SELECT user_id, role, joined_at, last_read_seq, muted FROM chat_members \
             WHERE tenant_id = $1 AND channel_id = $2 ORDER BY joined_at",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter()
            .map(|r| {
                Ok(ChatMember {
                    user: UserId::new(r.0),
                    role: MemberRole::parse(&r.1)?,
                    joined_at: r.2,
                    last_read_seq: r.3,
                    muted: r.4,
                })
            })
            .collect()
    }

    /// Join a live public channel. Joining twice is not an error.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if there is no such joinable channel — a
    /// private room simply does not exist as far as an outsider is told.
    pub async fn join_channel(&self, id: &ChatChannelId) -> Result<()> {
        let joinable: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM chat_channels \
             WHERE tenant_id = $1 AND id = $2 AND kind = 'channel' \
               AND visibility = 'public' AND archived_at IS NULL",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if joinable.is_none() {
            return Err(StoreError::NotFound);
        }
        self.insert_member(id, self.user.as_str(), MemberRole::Member)
            .await
    }

    /// Add someone to a room the caller belongs to. Adding twice is not an
    /// error; the added person is always a plain member.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the caller is not a member of the room or
    /// the added user is not of this tenant; [`StoreError::Validation`] for a
    /// DM or an agent DM, whose two participants are fixed when it is opened —
    /// a one-to-one must not become a channel by accretion.
    pub async fn add_member(&self, id: &ChatChannelId, user: &UserId) -> Result<()> {
        let channel = self.channel(id).await?;
        if self.channel_role(id).await?.is_none() {
            return Err(StoreError::NotFound);
        }
        if channel.kind.is_direct() {
            return Err(StoreError::Validation(
                "a direct message has exactly two people".to_owned(),
            ));
        }
        let exists: Option<(String,)> =
            sqlx::query_as("SELECT id FROM users WHERE tenant_id = $1 AND id = $2")
                .bind(self.tenant.as_str())
                .bind(user.as_str())
                .fetch_optional(&self.pool)
                .await
                .map_err(StoreError::Db)?;
        if exists.is_none() {
            return Err(StoreError::NotFound);
        }
        self.insert_member(id, user.as_str(), MemberRole::Member)
            .await
    }

    async fn insert_member(&self, id: &ChatChannelId, user: &str, role: MemberRole) -> Result<()> {
        sqlx::query(
            "INSERT INTO chat_members (tenant_id, channel_id, user_id, role) \
             VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(user)
        .bind(role.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }

    /// Leave a room, or (as its owner) remove someone else. Removing a person
    /// who is not there is not an error.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the caller is not a member;
    /// [`StoreError::Forbidden`] when removing someone else without being an
    /// owner; [`StoreError::Validation`] for a DM, which is left by hiding it,
    /// never by emptying it.
    pub async fn remove_member(&self, id: &ChatChannelId, user: &UserId) -> Result<()> {
        let channel = self.channel(id).await?;
        let role = self.channel_role(id).await?.ok_or(StoreError::NotFound)?;
        if channel.kind.is_direct() {
            return Err(StoreError::Validation(
                "a direct message keeps both people".to_owned(),
            ));
        }
        if user.as_str() != self.user.as_str() && role != MemberRole::Owner {
            return Err(StoreError::Forbidden);
        }
        sqlx::query(
            "DELETE FROM chat_members \
             WHERE tenant_id = $1 AND channel_id = $2 AND user_id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(user.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }

    /// Rename and/or retopic a named channel. `None` leaves a field alone.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the caller is not a member,
    /// [`StoreError::Forbidden`] if not an owner, [`StoreError::Validation`]
    /// for a bad name/topic or a DM, [`StoreError::Conflict`] if the name is
    /// taken by another live channel.
    pub async fn rename_channel(
        &self,
        id: &ChatChannelId,
        name: Option<&str>,
        topic: Option<&str>,
    ) -> Result<()> {
        let channel = self.channel(id).await?;
        self.require_owner(id).await?;
        if channel.kind.is_direct() {
            return Err(StoreError::Validation(
                "a direct message has no name to change".to_owned(),
            ));
        }
        if let Some(new_name) = name {
            validate_channel_name(new_name)?;
        }
        validate_topic(topic)?;
        sqlx::query(
            "UPDATE chat_channels \
             SET name = COALESCE($3, name), topic = COALESCE($4, topic), \
                 updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(name.map(str::trim))
        .bind(topic.map(str::trim))
        .execute(&self.pool)
        .await
        .map_err(map_name_taken)?;
        Ok(())
    }

    /// Archive a channel: it leaves the lists and frees its name, and its
    /// history stays readable to its members. Archiving twice is not an error.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the caller is not a member,
    /// [`StoreError::Forbidden`] if not an owner, [`StoreError::Validation`]
    /// for a DM.
    pub async fn archive_channel(&self, id: &ChatChannelId) -> Result<()> {
        let channel = self.channel(id).await?;
        self.require_owner(id).await?;
        if channel.kind.is_direct() {
            return Err(StoreError::Validation(
                "a direct message is not archived".to_owned(),
            ));
        }
        sqlx::query(
            "UPDATE chat_channels SET archived_at = now(), updated_at = now() \
             WHERE tenant_id = $1 AND id = $2 AND archived_at IS NULL",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        // The room's history stays readable; what its agents learned there
        // does not (A6.3) — an archived room takes no further turns, so its
        // memories have no surface left to be right on.
        self.forget_channel_memories(id).await?;
        Ok(())
    }

    async fn require_owner(&self, id: &ChatChannelId) -> Result<()> {
        match self.channel_role(id).await? {
            Some(MemberRole::Owner) => Ok(()),
            Some(MemberRole::Member) => Err(StoreError::Forbidden),
            None => Err(StoreError::NotFound),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn a_dm_key_is_the_same_from_either_side() {
        assert_eq!(dm_key("alice", "bob"), dm_key("bob", "alice"));
        assert_eq!(dm_key("alice", "bob"), "alice:bob");
        // Distinct pairs never collide.
        assert_ne!(dm_key("a", "bc"), dm_key("ab", "c"));
    }

    #[test]
    fn channel_names_are_present_and_bounded() {
        assert!(validate_channel_name("general").is_ok());
        assert!(validate_channel_name("  ").is_err());
        assert!(validate_channel_name("").is_err());
        let long = "x".repeat(CHANNEL_NAME_MAX_CHARS + 1);
        assert!(validate_channel_name(&long).is_err());
        assert!(validate_channel_name(&long[..CHANNEL_NAME_MAX_CHARS]).is_ok());
    }

    #[test]
    fn topics_are_optional_and_bounded() {
        assert!(validate_topic(None).is_ok());
        assert!(validate_topic(Some("what we ship this week")).is_ok());
        assert!(validate_topic(Some(&"x".repeat(CHANNEL_TOPIC_MAX_CHARS + 1))).is_err());
    }

    #[test]
    fn tokens_round_trip_through_their_columns() {
        for kind in [ChannelKind::Channel, ChannelKind::Dm, ChannelKind::AgentDm] {
            assert_eq!(ChannelKind::parse(kind.as_str()).unwrap(), kind);
        }
        for visibility in [ChannelVisibility::Public, ChannelVisibility::Private] {
            assert_eq!(
                ChannelVisibility::parse(visibility.as_str()).unwrap(),
                visibility
            );
        }
        for role in [MemberRole::Owner, MemberRole::Member] {
            assert_eq!(MemberRole::parse(role.as_str()).unwrap(), role);
        }
        assert!(ChannelKind::parse("broadcast").is_err());
        // A one-to-one is a one-to-one whoever the counterpart is: the rules
        // add_member, rename and archive apply hang off this, so a new kind
        // cannot quietly acquire a channel's permissions.
        assert!(ChannelKind::Dm.is_direct());
        assert!(ChannelKind::AgentDm.is_direct());
        assert!(!ChannelKind::Channel.is_direct());
        assert!(ChannelVisibility::parse("secret").is_err());
        assert!(MemberRole::parse("admin").is_err());
    }
}
