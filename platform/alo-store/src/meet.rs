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

use sha2::{Digest, Sha256};
use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::{ChatChannelId, EventId, MeetingId, UserId};
use crate::id::{TenantId, generate_token};
use crate::store::Store;

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

/// A durable text message sent while a meeting is running.
#[derive(Debug, Clone)]
pub struct MeetingMessage {
    pub id: String,
    pub sender: UserId,
    pub recipient: Option<UserId>,
    pub body: String,
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct MeetingMessageAttachment {
    pub id: String,
    pub message_id: String,
    pub file_name: String,
    pub content_type: String,
    pub size: i64,
    pub data: Option<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct MeetingMessageReaction {
    pub message_id: String,
    pub user: UserId,
    pub emoji: String,
}

#[derive(Debug, Clone)]
pub struct MeetingTranscriptSegment {
    pub id: String,
    pub speaker: UserId,
    pub text: String,
    pub final_segment: bool,
    pub created_at: OffsetDateTime,
}

/// A consent-gated recording of one meeting.
#[derive(Debug, Clone)]
pub struct MeetingRecording {
    pub id: String,
    pub requested_by: UserId,
    pub egress_id: Option<String>,
    pub status: String,
    pub file_path: Option<String>,
    pub requested_at: OffsetDateTime,
    pub started_at: Option<OffsetDateTime>,
    pub stopped_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone)]
pub struct MeetingRecordingConsent {
    pub user: UserId,
    pub consented_at: OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct MeetingWorkspace {
    pub state: serde_json::Value,
    pub revision: i64,
    pub updated_at: OffsetDateTime,
}

/// What a meeting is attached to when it is made.
#[derive(Debug, Clone, Default)]
pub struct NewMeeting {
    pub title: String,
    pub channel_id: Option<ChatChannelId>,
    pub event_id: Option<EventId>,
}

/// A guest invitation returned to its host. The raw token is present only at creation.
#[derive(Debug, Clone)]
pub struct MeetingGuestInvitationCreated {
    pub id: String,
    pub token: String,
    pub expires_at: OffsetDateTime,
}

/// A guest's lobby state. Public resolution never exposes the tenant or media room.
#[derive(Debug, Clone)]
pub struct MeetingGuest {
    pub id: String,
    pub tenant: TenantId,
    pub meeting_id: MeetingId,
    pub room: String,
    pub meeting_title: String,
    pub guest_email: String,
    pub guest_name: String,
    pub expires_at: OffsetDateTime,
    pub requested_at: Option<OffsetDateTime>,
    pub admitted_at: Option<OffsetDateTime>,
    pub denied_at: Option<OffsetDateTime>,
}

fn token_hash(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
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

fn to_recording(
    row: (
        String,
        String,
        Option<String>,
        String,
        Option<String>,
        OffsetDateTime,
        Option<OffsetDateTime>,
        Option<OffsetDateTime>,
    ),
) -> MeetingRecording {
    MeetingRecording {
        id: row.0,
        requested_by: UserId::new(row.1),
        egress_id: row.2,
        status: row.3,
        file_path: row.4,
        requested_at: row.5,
        started_at: row.6,
        stopped_at: row.7,
    }
}

const COLUMNS: &str =
    "id, room, title, created_by, channel_id, event_id, created_at, started_at, ended_at";

impl AccountStore {
    /// Shared agenda, polls, and notes for a meeting. The meeting visibility
    /// check is the permission boundary for every read and write.
    pub async fn meeting_workspace(&self, id: &MeetingId) -> Result<MeetingWorkspace> {
        self.meeting(id).await?;
        sqlx::query("INSERT INTO meeting_workspaces (tenant_id, meeting_id) VALUES ($1,$2) ON CONFLICT DO NOTHING")
            .bind(self.tenant.as_str()).bind(id.as_str()).execute(&self.pool).await.map_err(StoreError::Db)?;
        let row: (serde_json::Value, i64, OffsetDateTime) = sqlx::query_as(
            "SELECT state, revision, updated_at FROM meeting_workspaces WHERE tenant_id=$1 AND meeting_id=$2",
        ).bind(self.tenant.as_str()).bind(id.as_str()).fetch_one(&self.pool).await.map_err(StoreError::Db)?;
        Ok(MeetingWorkspace {
            state: row.0,
            revision: row.1,
            updated_at: row.2,
        })
    }

    /// Replace shared meeting state only when the caller edited the revision
    /// they actually read. This prevents one participant's note from silently
    /// erasing somebody else's poll vote.
    pub async fn put_meeting_workspace(
        &self,
        id: &MeetingId,
        revision: i64,
        state: &serde_json::Value,
    ) -> Result<MeetingWorkspace> {
        self.meeting(id).await?;
        let row: Option<(serde_json::Value, i64, OffsetDateTime)> = sqlx::query_as(
            "UPDATE meeting_workspaces SET state=$4, revision=revision+1, updated_at=now() WHERE tenant_id=$1 AND meeting_id=$2 AND revision=$3 RETURNING state,revision,updated_at",
        ).bind(self.tenant.as_str()).bind(id.as_str()).bind(revision).bind(state).fetch_optional(&self.pool).await.map_err(StoreError::Db)?;
        row.map(|r| MeetingWorkspace {
            state: r.0,
            revision: r.1,
            updated_at: r.2,
        })
        .ok_or_else(|| {
            StoreError::Conflict("meeting workspace changed; reload and try again".to_owned())
        })
    }

    /// Record only the caller's vote. The caller cannot manufacture votes for
    /// another participant; optimistic retries preserve simultaneous voters.
    pub async fn vote_meeting_poll(
        &self,
        id: &MeetingId,
        poll_id: &str,
        option: usize,
    ) -> Result<MeetingWorkspace> {
        for _ in 0..3 {
            let current = self.meeting_workspace(id).await?;
            let mut state = current.state.clone();
            let polls = state
                .get_mut("polls")
                .and_then(serde_json::Value::as_array_mut)
                .ok_or_else(|| StoreError::Validation("meeting polls are invalid".to_owned()))?;
            let poll = polls
                .iter_mut()
                .find(|poll| poll.get("id").and_then(serde_json::Value::as_str) == Some(poll_id))
                .ok_or(StoreError::NotFound)?;
            let option_count = poll
                .get("options")
                .and_then(serde_json::Value::as_array)
                .map(Vec::len)
                .unwrap_or(0);
            if option >= option_count {
                return Err(StoreError::Validation("poll option is invalid".to_owned()));
            }
            let votes = poll
                .get_mut("votes")
                .and_then(serde_json::Value::as_object_mut)
                .ok_or_else(|| {
                    StoreError::Validation("meeting poll votes are invalid".to_owned())
                })?;
            votes.insert(self.user.as_str().to_owned(), serde_json::json!(option));
            match self
                .put_meeting_workspace(id, current.revision, &state)
                .await
            {
                Ok(saved) => return Ok(saved),
                Err(StoreError::Conflict(_)) => continue,
                Err(error) => return Err(error),
            }
        }
        Err(StoreError::Conflict(
            "meeting workspace is busy; try again".to_owned(),
        ))
    }

    /// Ask everyone currently present to consent to a recording.
    pub async fn request_meeting_recording(
        &self,
        meeting_id: &MeetingId,
    ) -> Result<MeetingRecording> {
        let meeting = self.meeting(meeting_id).await?;
        self.require_meeting_host(&meeting)?;
        if meeting.ended_at.is_some() {
            return Err(StoreError::Validation(
                "a live meeting is required".to_owned(),
            ));
        }
        let active: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM meeting_recordings WHERE tenant_id=$1 AND meeting_id=$2 AND status IN ('pending','recording'))")
            .bind(self.tenant.as_str()).bind(meeting_id.as_str()).fetch_one(&self.pool).await.map_err(StoreError::Db)?;
        if active {
            return Err(StoreError::Conflict(
                "this meeting already has an active recording request".to_owned(),
            ));
        }
        let id = generate_token();
        let row: (String,String,Option<String>,String,Option<String>,OffsetDateTime,Option<OffsetDateTime>,Option<OffsetDateTime>) = sqlx::query_as(
            "INSERT INTO meeting_recordings (tenant_id,meeting_id,id,requested_by) VALUES ($1,$2,$3,$4) RETURNING id,requested_by,egress_id,status,file_path,requested_at,started_at,stopped_at"
        ).bind(self.tenant.as_str()).bind(meeting_id.as_str()).bind(&id).bind(self.user.as_str()).fetch_one(&self.pool).await.map_err(StoreError::Db)?;
        Ok(to_recording(row))
    }

    pub async fn current_meeting_recording(
        &self,
        meeting_id: &MeetingId,
    ) -> Result<Option<MeetingRecording>> {
        self.meeting(meeting_id).await?;
        let row = sqlx::query_as("SELECT id,requested_by,egress_id,status,file_path,requested_at,started_at,stopped_at FROM meeting_recordings WHERE tenant_id=$1 AND meeting_id=$2 ORDER BY requested_at DESC LIMIT 1")
            .bind(self.tenant.as_str()).bind(meeting_id.as_str()).fetch_optional(&self.pool).await.map_err(StoreError::Db)?;
        Ok(row.map(to_recording))
    }

    /// Keep explicit evidence that this participant accepted this recording.
    pub async fn consent_to_meeting_recording(
        &self,
        meeting_id: &MeetingId,
        recording_id: &str,
    ) -> Result<MeetingRecordingConsent> {
        let meeting = self.meeting(meeting_id).await?;
        if meeting.ended_at.is_some() {
            return Err(StoreError::Validation(
                "a live meeting is required".to_owned(),
            ));
        }
        let present: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM meeting_participants WHERE tenant_id=$1 AND meeting_id=$2 AND user_id=$3)")
            .bind(self.tenant.as_str()).bind(meeting_id.as_str()).bind(self.user.as_str()).fetch_one(&self.pool).await.map_err(StoreError::Db)?;
        if !present {
            return Err(StoreError::Forbidden);
        }
        let row: Option<(String,OffsetDateTime)> = sqlx::query_as("INSERT INTO meeting_recording_consents (tenant_id,meeting_id,recording_id,user_id) SELECT $1,$2,$3,$4 WHERE EXISTS(SELECT 1 FROM meeting_recordings WHERE tenant_id=$1 AND meeting_id=$2 AND id=$3 AND status='pending') ON CONFLICT (tenant_id,meeting_id,recording_id,user_id) DO UPDATE SET consented_at=meeting_recording_consents.consented_at RETURNING user_id,consented_at")
            .bind(self.tenant.as_str()).bind(meeting_id.as_str()).bind(recording_id).bind(self.user.as_str()).fetch_optional(&self.pool).await.map_err(StoreError::Db)?;
        row.map(|(user, consented_at)| MeetingRecordingConsent {
            user: UserId::new(user),
            consented_at,
        })
        .ok_or(StoreError::NotFound)
    }

    pub async fn meeting_recording_consents(
        &self,
        meeting_id: &MeetingId,
        recording_id: &str,
    ) -> Result<Vec<MeetingRecordingConsent>> {
        self.meeting(meeting_id).await?;
        let rows: Vec<(String,OffsetDateTime)> = sqlx::query_as("SELECT user_id,consented_at FROM meeting_recording_consents WHERE tenant_id=$1 AND meeting_id=$2 AND recording_id=$3 ORDER BY consented_at")
            .bind(self.tenant.as_str()).bind(meeting_id.as_str()).bind(recording_id).fetch_all(&self.pool).await.map_err(StoreError::Db)?;
        Ok(rows
            .into_iter()
            .map(|(user, consented_at)| MeetingRecordingConsent {
                user: UserId::new(user),
                consented_at,
            })
            .collect())
    }

    pub async fn mark_meeting_recording_started(
        &self,
        meeting_id: &MeetingId,
        recording_id: &str,
        egress_id: &str,
        file_path: &str,
    ) -> Result<MeetingRecording> {
        let meeting = self.meeting(meeting_id).await?;
        self.require_meeting_host(&meeting)?;
        let row = sqlx::query_as("UPDATE meeting_recordings SET status='recording',egress_id=$4,file_path=$5,started_at=now() WHERE tenant_id=$1 AND meeting_id=$2 AND id=$3 AND status='pending' RETURNING id,requested_by,egress_id,status,file_path,requested_at,started_at,stopped_at")
            .bind(self.tenant.as_str()).bind(meeting_id.as_str()).bind(recording_id).bind(egress_id).bind(file_path).fetch_optional(&self.pool).await.map_err(StoreError::Db)?;
        row.map(to_recording).ok_or(StoreError::Conflict(
            "the recording is no longer waiting to start".to_owned(),
        ))
    }

    pub async fn mark_meeting_recording_stopped(
        &self,
        meeting_id: &MeetingId,
        recording_id: &str,
    ) -> Result<MeetingRecording> {
        let meeting = self.meeting(meeting_id).await?;
        self.require_meeting_host(&meeting)?;
        let row = sqlx::query_as("UPDATE meeting_recordings SET status='completed',stopped_at=now() WHERE tenant_id=$1 AND meeting_id=$2 AND id=$3 AND status='recording' RETURNING id,requested_by,egress_id,status,file_path,requested_at,started_at,stopped_at")
            .bind(self.tenant.as_str()).bind(meeting_id.as_str()).bind(recording_id).fetch_optional(&self.pool).await.map_err(StoreError::Db)?;
        row.map(to_recording).ok_or(StoreError::Conflict(
            "the recording is not running".to_owned(),
        ))
    }

    /// Store a meeting message before it is broadcast through the media engine.
    pub async fn post_meeting_message(
        &self,
        meeting_id: &MeetingId,
        body: &str,
        recipient: Option<&UserId>,
    ) -> Result<MeetingMessage> {
        let meeting = self.meeting(meeting_id).await?;
        if meeting.ended_at.is_some() || body.trim().is_empty() || body.len() > 10_000 {
            return Err(StoreError::Validation(
                "a live meeting and a message are required".to_owned(),
            ));
        }
        if let Some(person) = recipient {
            let allowed: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM meeting_participants WHERE tenant_id=$1 AND meeting_id=$2 AND user_id=$3)",
            )
            .bind(self.tenant.as_str())
            .bind(meeting_id.as_str())
            .bind(person.as_str())
            .fetch_one(&self.pool)
            .await
            .map_err(StoreError::Db)?;
            if !allowed {
                return Err(StoreError::Forbidden);
            }
        }
        let id = generate_token();
        let row: (String, String, Option<String>, String, OffsetDateTime) = sqlx::query_as(
            "INSERT INTO meeting_messages (tenant_id,meeting_id,id,sender_id,recipient_id,body) VALUES ($1,$2,$3,$4,$5,$6) RETURNING id,sender_id,recipient_id,body,created_at",
        )
        .bind(self.tenant.as_str())
        .bind(meeting_id.as_str())
        .bind(&id)
        .bind(self.user.as_str())
        .bind(recipient.map(UserId::as_str))
        .bind(body.trim())
        .fetch_one(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(MeetingMessage {
            id: row.0,
            sender: UserId::new(row.1),
            recipient: row.2.map(UserId::new),
            body: row.3,
            created_at: row.4,
        })
    }

    /// Messages visible to the caller: public, addressed to them, or sent by them.
    pub async fn meeting_messages(&self, meeting_id: &MeetingId) -> Result<Vec<MeetingMessage>> {
        self.meeting(meeting_id).await?;
        let rows: Vec<(String, String, Option<String>, String, OffsetDateTime)> = sqlx::query_as(
            "SELECT id,sender_id,recipient_id,body,created_at FROM meeting_messages WHERE tenant_id=$1 AND meeting_id=$2 AND (recipient_id IS NULL OR recipient_id=$3 OR sender_id=$3) ORDER BY created_at,id LIMIT 1000",
        )
        .bind(self.tenant.as_str())
        .bind(meeting_id.as_str())
        .bind(self.user.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows
            .into_iter()
            .map(|row| MeetingMessage {
                id: row.0,
                sender: UserId::new(row.1),
                recipient: row.2.map(UserId::new),
                body: row.3,
                created_at: row.4,
            })
            .collect())
    }

    pub async fn attach_to_meeting_message(
        &self,
        meeting_id: &MeetingId,
        message_id: &str,
        file_name: &str,
        content_type: &str,
        data: Vec<u8>,
    ) -> Result<MeetingMessageAttachment> {
        self.meeting(meeting_id).await?;
        if data.is_empty()
            || data.len() > 10 * 1024 * 1024
            || file_name.trim().is_empty()
            || !(content_type.starts_with("image/") || content_type == "application/pdf")
        {
            return Err(StoreError::Validation(
                "only images and PDFs up to 10 MB are supported".to_owned(),
            ));
        }
        let owns: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM meeting_messages WHERE tenant_id=$1 AND meeting_id=$2 AND id=$3 AND sender_id=$4)")
            .bind(self.tenant.as_str()).bind(meeting_id.as_str()).bind(message_id).bind(self.user.as_str()).fetch_one(&self.pool).await.map_err(StoreError::Db)?;
        if !owns {
            return Err(StoreError::Forbidden);
        }
        let id = generate_token();
        sqlx::query("INSERT INTO meeting_message_attachments (tenant_id,meeting_id,message_id,id,file_name,content_type,data) VALUES ($1,$2,$3,$4,$5,$6,$7)")
            .bind(self.tenant.as_str()).bind(meeting_id.as_str()).bind(message_id).bind(&id).bind(file_name.trim()).bind(content_type).bind(&data).execute(&self.pool).await.map_err(StoreError::Db)?;
        Ok(MeetingMessageAttachment {
            id,
            message_id: message_id.to_owned(),
            file_name: file_name.trim().to_owned(),
            content_type: content_type.to_owned(),
            size: data.len() as i64,
            data: None,
        })
    }

    pub async fn meeting_message_attachments(
        &self,
        meeting_id: &MeetingId,
    ) -> Result<Vec<MeetingMessageAttachment>> {
        self.meeting(meeting_id).await?;
        let rows: Vec<(String,String,String,String,i64)> = sqlx::query_as("SELECT a.id,a.message_id,a.file_name,a.content_type,octet_length(a.data) FROM meeting_message_attachments a JOIN meeting_messages m ON m.tenant_id=a.tenant_id AND m.meeting_id=a.meeting_id AND m.id=a.message_id WHERE a.tenant_id=$1 AND a.meeting_id=$2 AND (m.recipient_id IS NULL OR m.recipient_id=$3 OR m.sender_id=$3) ORDER BY a.created_at,a.id")
            .bind(self.tenant.as_str()).bind(meeting_id.as_str()).bind(self.user.as_str()).fetch_all(&self.pool).await.map_err(StoreError::Db)?;
        Ok(rows
            .into_iter()
            .map(|r| MeetingMessageAttachment {
                id: r.0,
                message_id: r.1,
                file_name: r.2,
                content_type: r.3,
                size: r.4,
                data: None,
            })
            .collect())
    }

    pub async fn meeting_message_attachment(
        &self,
        meeting_id: &MeetingId,
        message_id: &str,
        attachment_id: &str,
    ) -> Result<MeetingMessageAttachment> {
        self.meeting(meeting_id).await?;
        let row: Option<(String,String,String,String,i64,Vec<u8>)> = sqlx::query_as("SELECT a.id,a.message_id,a.file_name,a.content_type,octet_length(a.data),a.data FROM meeting_message_attachments a JOIN meeting_messages m ON m.tenant_id=a.tenant_id AND m.meeting_id=a.meeting_id AND m.id=a.message_id WHERE a.tenant_id=$1 AND a.meeting_id=$2 AND a.message_id=$3 AND a.id=$4 AND (m.recipient_id IS NULL OR m.recipient_id=$5 OR m.sender_id=$5)")
            .bind(self.tenant.as_str()).bind(meeting_id.as_str()).bind(message_id).bind(attachment_id).bind(self.user.as_str()).fetch_optional(&self.pool).await.map_err(StoreError::Db)?;
        row.map(|r| MeetingMessageAttachment {
            id: r.0,
            message_id: r.1,
            file_name: r.2,
            content_type: r.3,
            size: r.4,
            data: Some(r.5),
        })
        .ok_or(StoreError::NotFound)
    }

    /// Add a reaction to a message visible to the caller. Repeating it is idempotent.
    pub async fn react_to_meeting_message(
        &self,
        meeting_id: &MeetingId,
        message_id: &str,
        emoji: &str,
    ) -> Result<()> {
        self.meeting(meeting_id).await?;
        if emoji.is_empty() || emoji.chars().count() > 8 {
            return Err(StoreError::Validation(
                "a short emoji reaction is required".to_owned(),
            ));
        }
        let visible: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM meeting_messages WHERE tenant_id=$1 AND meeting_id=$2 AND id=$3 AND (recipient_id IS NULL OR recipient_id=$4 OR sender_id=$4))")
            .bind(self.tenant.as_str()).bind(meeting_id.as_str()).bind(message_id).bind(self.user.as_str()).fetch_one(&self.pool).await.map_err(StoreError::Db)?;
        if !visible {
            return Err(StoreError::NotFound);
        }
        sqlx::query("INSERT INTO meeting_message_reactions (tenant_id,meeting_id,message_id,user_id,emoji) VALUES ($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING")
            .bind(self.tenant.as_str()).bind(meeting_id.as_str()).bind(message_id).bind(self.user.as_str()).bind(emoji).execute(&self.pool).await.map_err(StoreError::Db)?;
        Ok(())
    }

    pub async fn meeting_message_reactions(
        &self,
        meeting_id: &MeetingId,
    ) -> Result<Vec<MeetingMessageReaction>> {
        self.meeting(meeting_id).await?;
        let rows: Vec<(String,String,String)> = sqlx::query_as("SELECT r.message_id,r.user_id,r.emoji FROM meeting_message_reactions r JOIN meeting_messages m ON m.tenant_id=r.tenant_id AND m.meeting_id=r.meeting_id AND m.id=r.message_id WHERE r.tenant_id=$1 AND r.meeting_id=$2 AND (m.recipient_id IS NULL OR m.recipient_id=$3 OR m.sender_id=$3) ORDER BY r.created_at")
            .bind(self.tenant.as_str()).bind(meeting_id.as_str()).bind(self.user.as_str()).fetch_all(&self.pool).await.map_err(StoreError::Db)?;
        Ok(rows
            .into_iter()
            .map(|r| MeetingMessageReaction {
                message_id: r.0,
                user: UserId::new(r.1),
                emoji: r.2,
            })
            .collect())
    }

    /// Store this participant's own caption segment. Speech recognition sends
    /// refinements under one id before marking the phrase final.
    pub async fn put_meeting_transcript_segment(
        &self,
        meeting_id: &MeetingId,
        id: &str,
        text: &str,
        final_segment: bool,
    ) -> Result<MeetingTranscriptSegment> {
        self.meeting(meeting_id).await?;
        let id = id.trim();
        let text = text.trim();
        if id.is_empty()
            || id.chars().count() > 200
            || text.is_empty()
            || text.chars().count() > 8_000
        {
            return Err(StoreError::Validation(
                "a transcript segment id and text are required".to_owned(),
            ));
        }
        let row: (String, String, String, bool, OffsetDateTime) = sqlx::query_as(
            "INSERT INTO meeting_transcript_segments (tenant_id,meeting_id,id,speaker_id,text,final) VALUES ($1,$2,$3,$4,$5,$6) ON CONFLICT (tenant_id,meeting_id,id) DO UPDATE SET text=EXCLUDED.text,final=EXCLUDED.final WHERE meeting_transcript_segments.speaker_id=EXCLUDED.speaker_id RETURNING id,speaker_id,text,final,created_at"
        ).bind(self.tenant.as_str()).bind(meeting_id.as_str()).bind(id).bind(self.user.as_str()).bind(text).bind(final_segment).fetch_one(&self.pool).await.map_err(StoreError::Db)?;
        Ok(MeetingTranscriptSegment {
            id: row.0,
            speaker: UserId::new(row.1),
            text: row.2,
            final_segment: row.3,
            created_at: row.4,
        })
    }

    pub async fn meeting_transcript(
        &self,
        meeting_id: &MeetingId,
    ) -> Result<Vec<MeetingTranscriptSegment>> {
        self.meeting(meeting_id).await?;
        let rows: Vec<(String, String, String, bool, OffsetDateTime)> = sqlx::query_as(
            "SELECT id,speaker_id,text,final,created_at FROM meeting_transcript_segments WHERE tenant_id=$1 AND meeting_id=$2 ORDER BY created_at,id"
        ).bind(self.tenant.as_str()).bind(meeting_id.as_str()).fetch_all(&self.pool).await.map_err(StoreError::Db)?;
        Ok(rows
            .into_iter()
            .map(|row| MeetingTranscriptSegment {
                id: row.0,
                speaker: UserId::new(row.1),
                text: row.2,
                final_segment: row.3,
                created_at: row.4,
            })
            .collect())
    }

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
                .post_message(
                    channel,
                    &format!("__meeting__:{}", meeting.id.as_str()),
                    None,
                )
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
    fn require_meeting_host(&self, meeting: &Meeting) -> Result<()> {
        if meeting.created_by.as_str() == self.user.as_str() {
            Ok(())
        } else {
            Err(StoreError::Forbidden)
        }
    }

    /// Create a single-use, independently revocable guest link.
    pub async fn create_meeting_guest_invitation(
        &self,
        id: &MeetingId,
        email: &str,
        name: &str,
        expires_at_epoch: i64,
    ) -> Result<MeetingGuestInvitationCreated> {
        let meeting = self.meeting(id).await?;
        self.require_meeting_host(&meeting)?;
        let expires_at = OffsetDateTime::from_unix_timestamp(expires_at_epoch)
            .map_err(|_| StoreError::Validation("invalid guest invitation expiry".to_owned()))?;
        if expires_at <= OffsetDateTime::now_utc()
            || email.trim().is_empty()
            || name.trim().is_empty()
        {
            return Err(StoreError::Validation(
                "guest name, email, and future expiry are required".to_owned(),
            ));
        }
        let invitation_id = generate_token();
        let token = format!("{}{}", generate_token(), generate_token());
        sqlx::query("INSERT INTO meeting_guest_invitations (tenant_id,id,meeting_id,token_hash,guest_email,guest_name,expires_at) VALUES ($1,$2,$3,$4,$5,$6,$7)")
            .bind(self.tenant.as_str()).bind(&invitation_id).bind(id.as_str()).bind(token_hash(&token)).bind(email.trim()).bind(name.trim()).bind(expires_at)
            .execute(&self.pool).await.map_err(StoreError::Db)?;
        Ok(MeetingGuestInvitationCreated {
            id: invitation_id,
            token,
            expires_at,
        })
    }

    /// Admit a waiting guest. Only the meeting creator can moderate the lobby.
    pub async fn admit_meeting_guest(
        &self,
        meeting_id: &MeetingId,
        invitation_id: &str,
    ) -> Result<()> {
        let meeting = self.meeting(meeting_id).await?;
        self.require_meeting_host(&meeting)?;
        let changed = sqlx::query("UPDATE meeting_guest_invitations SET admitted_at=now(), denied_at=NULL WHERE tenant_id=$1 AND meeting_id=$2 AND id=$3 AND revoked_at IS NULL AND expires_at>now()")
            .bind(self.tenant.as_str()).bind(meeting_id.as_str()).bind(invitation_id).execute(&self.pool).await.map_err(StoreError::Db)?.rows_affected();
        if changed == 0 {
            Err(StoreError::NotFound)
        } else {
            Ok(())
        }
    }

    /// Revoke a guest link immediately. Only the host may do so.
    pub async fn revoke_meeting_guest(
        &self,
        meeting_id: &MeetingId,
        invitation_id: &str,
    ) -> Result<()> {
        let meeting = self.meeting(meeting_id).await?;
        self.require_meeting_host(&meeting)?;
        let changed = sqlx::query("UPDATE meeting_guest_invitations SET revoked_at=now() WHERE tenant_id=$1 AND meeting_id=$2 AND id=$3")
            .bind(self.tenant.as_str()).bind(meeting_id.as_str()).bind(invitation_id).execute(&self.pool).await.map_err(StoreError::Db)?.rows_affected();
        if changed == 0 {
            Err(StoreError::NotFound)
        } else {
            Ok(())
        }
    }
}

impl Store {
    async fn meeting_guest_by_token(&self, token: &str) -> Result<Option<MeetingGuest>> {
        type Row = (
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            OffsetDateTime,
            Option<OffsetDateTime>,
            Option<OffsetDateTime>,
            Option<OffsetDateTime>,
        );
        let row: Option<Row> = sqlx::query_as("SELECT i.id,i.tenant_id,i.meeting_id,m.room,m.title,i.guest_email,i.guest_name,i.expires_at,i.requested_at,i.admitted_at,i.denied_at FROM meeting_guest_invitations i JOIN meetings m ON m.tenant_id=i.tenant_id AND m.id=i.meeting_id WHERE i.token_hash=$1 AND i.expires_at>now() AND i.revoked_at IS NULL AND m.ended_at IS NULL")
            .bind(token_hash(token)).fetch_optional(self.pool()).await.map_err(StoreError::Db)?;
        Ok(row.map(|r| MeetingGuest {
            id: r.0,
            tenant: TenantId::new(r.1),
            meeting_id: MeetingId::new(r.2),
            room: r.3,
            meeting_title: r.4,
            guest_email: r.5,
            guest_name: r.6,
            expires_at: r.7,
            requested_at: r.8,
            admitted_at: r.9,
            denied_at: r.10,
        }))
    }

    /// Resolve an active guest link without revealing expired or revoked rows.
    pub async fn resolve_meeting_guest(&self, token: &str) -> Result<Option<MeetingGuest>> {
        self.meeting_guest_by_token(token).await
    }

    /// Put a valid guest into the lobby. Idempotent across browser retries.
    pub async fn request_meeting_guest_admission(
        &self,
        token: &str,
    ) -> Result<Option<MeetingGuest>> {
        let Some(_guest) = self.meeting_guest_by_token(token).await? else {
            return Ok(None);
        };
        sqlx::query("UPDATE meeting_guest_invitations SET requested_at=COALESCE(requested_at,now()) WHERE token_hash=$1")
            .bind(token_hash(token)).execute(self.pool()).await.map_err(StoreError::Db)?;
        self.meeting_guest_by_token(token).await
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

    /// Recently ended meetings this person was allowed to see.
    ///
    /// Visibility deliberately reuses [`AccountStore::meeting`]'s channel
    /// rule instead of turning history into a tenant-wide activity feed.
    pub async fn my_recent_meetings(&self) -> Result<Vec<Meeting>> {
        let rows: Vec<MeetingRow> = sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM meetings \
             WHERE tenant_id = $1 AND ended_at IS NOT NULL \
             ORDER BY ended_at DESC LIMIT 50"
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
