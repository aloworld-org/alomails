//! Meetings — the record alo keeps beside the media it deliberately does not
//! run.
//!
//! LiveKit is the WebRTC engine (`docs/alo-product-description.md` § what we
//! build vs. integrate: video/WebRTC internals are explicitly not ours). It is
//! a sealed container that knows an opaque room name and a signed token, and
//! nothing else. Every fact that makes a meeting belong to somebody — the
//! tenant, the title, the chat room or calendar event it came from, who
//! attended — lives here.
//!
//! Two rules follow from that seam, and both are enforced rather than
//! documented:
//!
//! - **The engine never learns a tenant's words.** The room name it is told is
//!   generated, never derived from the title. A room called
//!   "q3-budget-acme-renewal" would put a customer's name in a third party's
//!   logs, and a meeting's title is exactly the kind of thing that names a
//!   customer.
//! - **Attendance is ours.** It is written when somebody takes a token rather
//!   than read back from the engine afterwards, because engines are swappable
//!   and attendance is evidence.

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::{ChatChannelId, EventId, MeetingId, UserId};

/// A meeting as the workspace knows it.
#[derive(Debug, Clone)]
pub struct Meeting {
    pub id: MeetingId,
    /// The opaque name the media engine is told. Never derived from `title`.
    pub room: String,
    pub title: String,
    pub created_by: UserId,
    /// The chat room it belongs to, if it was started from one.
    pub channel_id: Option<ChatChannelId>,
    /// The calendar event it belongs to, if it was scheduled.
    pub event_id: Option<EventId>,
    pub created_at: OffsetDateTime,
    /// When the first person actually joined. `None` means nobody did.
    pub started_at: Option<OffsetDateTime>,
    pub ended_at: Option<OffsetDateTime>,
}

/// Somebody who was in a meeting.
#[derive(Debug, Clone)]
pub struct MeetingParticipant {
    pub user: UserId,
    pub joined_at: OffsetDateTime,
}

/// What a meeting is attached to when it is made.
#[derive(Debug, Clone, Default)]
pub struct NewMeeting {
    pub title: String,
    pub channel_id: Option<ChatChannelId>,
    pub event_id: Option<EventId>,
}

type MeetingRow = (
    String,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    OffsetDateTime,
    Option<OffsetDateTime>,
    Option<OffsetDateTime>,
);

fn to_meeting(row: MeetingRow) -> Meeting {
    Meeting {
        id: MeetingId::new(row.0),
        room: row.1,
        title: row.2,
        created_by: UserId::new(row.3),
        channel_id: row.4.map(ChatChannelId::new),
        event_id: row.5.map(EventId::new),
        created_at: row.6,
        started_at: row.7,
        ended_at: row.8,
    }
}

const COLUMNS: &str =
    "id, room, title, created_by, channel_id, event_id, created_at, started_at, ended_at";

impl AccountStore {
    /// Start a meeting.
    ///
    /// When it belongs to a chat room, the caller must be able to read that
    /// room — otherwise a meeting is a way to attach yourself to a
    /// conversation you were never in.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the channel is not the caller's to see;
    /// [`StoreError::Db`] on a database failure.
    pub async fn create_meeting(&self, new: &NewMeeting) -> Result<Meeting> {
        if let Some(channel) = &new.channel_id {
            // Reuses the room's own visibility rule rather than restating it.
            self.channel(channel).await?;
        }
        let id = MeetingId::generate();
        // Opaque, and unrelated to the title: the engine must not be told what
        // this meeting is about.
        let room = format!("m-{}", MeetingId::generate().as_str());
        let row: MeetingRow = sqlx::query_as(&format!(
            "INSERT INTO meetings (tenant_id, id, room, title, created_by, channel_id, event_id) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING {COLUMNS}"
        ))
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(&room)
        .bind(new.title.trim())
        .bind(self.user.as_str())
        .bind(new.channel_id.as_ref().map(ChatChannelId::as_str))
        .bind(new.event_id.as_ref().map(EventId::as_str))
        .fetch_one(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        let meeting = to_meeting(row);

        // A meeting announces itself in the room it belongs to. This is the
        // seam Teams leaves open: there, a call happens somewhere else and the
        // conversation it concerns never learns it happened. Here the room is
        // told, everyone in it can join from where they already are, and the
        // transcript has a place to come back to.
        //
        // Best-effort: a meeting that exists but was not announced is a small
        // loss, and refusing to start a call because a message failed would be
        // a large one.
        if let Some(channel) = &meeting.channel_id {
            let _ = self
                .post_message(channel, &format!("__meeting__:{}", meeting.id.as_str()), None)
                .await;
        }
        Ok(meeting)
    }

    /// One meeting, if it is the caller's to see.
    ///
    /// A meeting attached to a chat room is visible to that room's readers; one
    /// attached to nothing is visible to whoever made it. There is no third
    /// case, because a meeting nobody can place is a meeting nobody should
    /// find.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when it does not exist or is not the caller's.
    pub async fn meeting(&self, id: &MeetingId) -> Result<Meeting> {
        let row: Option<MeetingRow> = sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM meetings WHERE tenant_id = $1 AND id = $2"
        ))
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        let meeting = row.map(to_meeting).ok_or(StoreError::NotFound)?;
        match &meeting.channel_id {
            Some(channel) => {
                self.channel(channel).await?;
            }
            None if meeting.created_by.as_str() != self.user.as_str() => {
                return Err(StoreError::NotFound);
            }
            None => {}
        }
        Ok(meeting)
    }

    /// The meetings still running in a room the caller can read.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the room is not the caller's;
    /// [`StoreError::Db`] on a database failure.
    pub async fn live_meetings_in(&self, channel: &ChatChannelId) -> Result<Vec<Meeting>> {
        self.channel(channel).await?;
        let rows: Vec<MeetingRow> = sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM meetings \
             WHERE tenant_id = $1 AND channel_id = $2 AND ended_at IS NULL \
             ORDER BY created_at DESC"
        ))
        .bind(self.tenant.as_str())
        .bind(channel.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(to_meeting).collect())
    }

    /// Record that somebody joined, and start the meeting if they are first.
    ///
    /// Idempotent: pressing join twice is one attendance, not two, and does not
    /// move the moment it started.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the meeting is not the caller's to see;
    /// [`StoreError::Db`] on a database failure.
    pub async fn join_meeting(&self, id: &MeetingId) -> Result<Meeting> {
        let meeting = self.meeting(id).await?;
        if meeting.ended_at.is_some() {
            // A meeting that is over cannot be rejoined. The engine would
            // happily make a new room of the same name; the record says no.
            return Err(StoreError::NotFound);
        }
        sqlx::query(
            "INSERT INTO meeting_participants (tenant_id, meeting_id, user_id) \
             VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(self.user.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        let row: MeetingRow = sqlx::query_as(&format!(
            "UPDATE meetings SET started_at = COALESCE(started_at, now()) \
             WHERE tenant_id = $1 AND id = $2 RETURNING {COLUMNS}"
        ))
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(to_meeting(row))
    }

    /// Declare a meeting over. Idempotent; the first ending is the one kept.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when it is not the caller's to see.
    pub async fn end_meeting(&self, id: &MeetingId) -> Result<()> {
        self.meeting(id).await?;
        sqlx::query(
            "UPDATE meetings SET ended_at = COALESCE(ended_at, now()) \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }

    /// Who has been in a meeting.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when it is not the caller's to see.
    pub async fn meeting_participants(&self, id: &MeetingId) -> Result<Vec<MeetingParticipant>> {
        self.meeting(id).await?;
        let rows: Vec<(String, OffsetDateTime)> = sqlx::query_as(
            "SELECT user_id, joined_at FROM meeting_participants \
             WHERE tenant_id = $1 AND meeting_id = $2 ORDER BY joined_at",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows
            .into_iter()
            .map(|(user, joined_at)| MeetingParticipant {
                user: UserId::new(user),
                joined_at,
            })
            .collect())
    }
}

impl AccountStore {
    /// The meeting attached to a calendar event, if there is one.
    ///
    /// An event has at most one: a second meeting on the same invitation is
    /// two links in one place, and half the attendees end up in the wrong
    /// call. The route that creates one asks this first.
    ///
    /// Ended meetings are ignored — a recurring weekly that finished last
    /// Tuesday should not hand out last Tuesday's room.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn meeting_for_event(&self, event: &EventId) -> Result<Option<Meeting>> {
        let row: Option<MeetingRow> = sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM meetings \
             WHERE tenant_id = $1 AND event_id = $2 AND ended_at IS NULL \
             ORDER BY created_at DESC LIMIT 1"
        ))
        .bind(self.tenant.as_str())
        .bind(event.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        // The event's own visibility is the gate: a meeting on somebody's
        // invitation is theirs to see exactly when the invitation is.
        match row {
            Some(row) => {
                let meeting = to_meeting(row);
                if self
                    .event(event)
                    .await
                    .map_err(|_| StoreError::NotFound)?
                    .is_none()
                {
                    return Ok(None);
                }
                Ok(Some(meeting))
            }
            None => Ok(None),
        }
    }
}

impl AccountStore {
    /// Meetings this person can currently walk into: everything live that is
    /// theirs to see.
    ///
    /// A meeting in a room they read, or one they started themselves. The room
    /// membership check happens per row rather than in the query, because the
    /// rule about who can see a channel lives in one place and should not be
    /// restated in SQL that will drift from it.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn my_live_meetings(&self) -> Result<Vec<Meeting>> {
        let rows: Vec<MeetingRow> = sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM meetings \
             WHERE tenant_id = $1 AND ended_at IS NULL \
             ORDER BY created_at DESC LIMIT 100"
        ))
        .bind(self.tenant.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        let mut mine = Vec::new();
        for row in rows {
            let meeting = to_meeting(row);
            let visible = match &meeting.channel_id {
                Some(channel) => self.channel(channel).await.is_ok(),
                None => meeting.created_by.as_str() == self.user.as_str(),
            };
            if visible {
                mine.push(meeting);
            }
        }
        Ok(mine)
    }
}
