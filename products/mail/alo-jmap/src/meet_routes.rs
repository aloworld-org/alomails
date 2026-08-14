//! The Meet API — start a meeting, join one, see who is in it, end it.
//!
//! alo owns the record; the media engine owns the media. Every route here is
//! about the record, and exactly one of them touches the engine: `join` mints
//! a token, and only after the store has already decided the caller may be in
//! the room. That ordering is the security property — the token is minted from
//! an answer, never from a request.

use axum::Json;
use axum::body::{Body, Bytes};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, header};
use axum::response::Response;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashSet;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use alo_store::UserId;
use alo_store::{ChatChannelId, EventId, Meeting, MeetingId, NewMeeting};

use crate::billing::map_store_err;
use crate::error::Problem;
use crate::state::{AppState, authenticate};

fn iso(at: OffsetDateTime) -> String {
    at.format(&Rfc3339).unwrap_or_default()
}

/// The wire shape of a meeting.
///
/// The engine's room name is deliberately absent: a client gets it inside a
/// join token or not at all, so a room name cannot be shared as a way in.
fn meeting_json(m: &Meeting) -> Value {
    json!({
        "id": m.id.as_str(),
        "title": m.title,
        "createdBy": m.created_by.as_str(),
        "channel": m.channel_id.as_ref().map(alo_store::ChatChannelId::as_str),
        "event": m.event_id.as_ref().map(alo_store::EventId::as_str),
        "createdAt": iso(m.created_at),
        "startedAt": m.started_at.map(iso),
        "endedAt": m.ended_at.map(iso),
        "live": m.ended_at.is_none(),
    })
}

#[derive(Deserialize)]
pub struct StartMeeting {
    #[serde(default)]
    title: String,
    #[serde(default)]
    channel: Option<String>,
    #[serde(default)]
    event: Option<String>,
}

#[derive(Deserialize)]
pub struct PutWorkspace {
    revision: i64,
    state: Value,
}

#[derive(Deserialize)]
pub struct VotePoll {
    poll: String,
    option: usize,
}

fn workspace_json(workspace: alo_store::MeetingWorkspace) -> Value {
    json!({ "state": workspace.state, "revision": workspace.revision, "updatedAt": iso(workspace.updated_at) })
}

fn validate_workspace(state: &Value) -> Result<(), Problem> {
    let agenda = state
        .get("agenda")
        .and_then(Value::as_array)
        .ok_or_else(Problem::not_json)?;
    let polls = state
        .get("polls")
        .and_then(Value::as_array)
        .ok_or_else(Problem::not_json)?;
    let notes = state
        .get("notes")
        .and_then(Value::as_str)
        .ok_or_else(Problem::not_json)?;
    if agenda.len() > 50 || polls.len() > 20 || notes.chars().count() > 50_000 {
        return Err(Problem::with(
            axum::http::StatusCode::PAYLOAD_TOO_LARGE,
            "meeting workspace is too large",
        ));
    }
    if agenda.iter().any(|item| {
        item.get("text")
            .and_then(Value::as_str)
            .is_none_or(|text| text.trim().is_empty() || text.chars().count() > 500)
    }) {
        return Err(Problem::with(
            axum::http::StatusCode::BAD_REQUEST,
            "agenda item is invalid",
        ));
    }
    for poll in polls {
        let question = poll.get("question").and_then(Value::as_str).unwrap_or("");
        let options = poll
            .get("options")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if question.trim().is_empty()
            || question.chars().count() > 500
            || !(2..=10).contains(&options.len())
            || options.iter().any(|option| {
                option
                    .as_str()
                    .is_none_or(|text| text.trim().is_empty() || text.chars().count() > 200)
            })
        {
            return Err(Problem::with(
                axum::http::StatusCode::BAD_REQUEST,
                "poll is invalid",
            ));
        }
    }
    Ok(())
}

pub async fn workspace(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let workspace = account
        .acc
        .meeting_workspace(&MeetingId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(workspace_json(workspace)))
}

pub async fn put_workspace(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<PutWorkspace>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let meeting_id = MeetingId::new(id);
    let meeting = account
        .acc
        .meeting(&meeting_id)
        .await
        .map_err(map_store_err)?;
    if meeting.created_by.as_str() != account.user.as_str() {
        return Err(Problem::with(
            axum::http::StatusCode::FORBIDDEN,
            "only the host can edit meeting tools",
        ));
    }
    validate_workspace(&body.state)?;
    let saved = account
        .acc
        .put_meeting_workspace(&meeting_id, body.revision, &body.state)
        .await
        .map_err(map_store_err)?;
    Ok(Json(workspace_json(saved)))
}

pub async fn vote_poll(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<VotePoll>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let saved = account
        .acc
        .vote_meeting_poll(&MeetingId::new(id), &body.poll, body.option)
        .await
        .map_err(map_store_err)?;
    Ok(Json(workspace_json(saved)))
}

/// `POST /meet` — start a meeting, optionally attached to a room or an event.
///
/// # Errors
/// 401 unauthenticated; 404 when the channel is not the caller's to see.
pub async fn start(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<StartMeeting>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let meeting = account
        .acc
        .create_meeting(&NewMeeting {
            title: body.title,
            channel_id: body.channel.map(ChatChannelId::new),
            event_id: body.event.map(EventId::new),
        })
        .await
        .map_err(map_store_err)?;
    Ok(Json(meeting_json(&meeting)))
}

/// `GET /meet/{id}` — one meeting, if it is the caller's to see.
///
/// # Errors
/// 404 when it does not exist or is not theirs.
pub async fn get(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let meeting = account
        .acc
        .meeting(&MeetingId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(meeting_json(&meeting)))
}

/// `GET /meet/channels/{id}` — meetings still running in a room.
///
/// # Errors
/// 404 when the room is not the caller's to see.
pub async fn in_channel(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let live = account
        .acc
        .live_meetings_in(&ChatChannelId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "meetings": live.iter().map(meeting_json).collect::<Vec<_>>()
    })))
}

/// `GET /meet` — everything live that the caller can walk into.
///
/// # Errors
/// 401 unauthenticated.
pub async fn mine(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let live = account
        .acc
        .my_live_meetings()
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "meetings": live.iter().map(meeting_json).collect::<Vec<_>>()
    })))
}

/// `GET /meet/history` — recently ended meetings visible to the caller.
pub async fn history(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let meetings = account
        .acc
        .my_recent_meetings()
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "meetings": meetings.iter().map(meeting_json).collect::<Vec<_>>()
    })))
}

/// `GET /meet/events/{id}` — the meeting on a calendar event, if any.
///
/// Answers `null` rather than 404 when there is none: "this invitation has no
/// meeting" is an ordinary state an agenda asks about constantly, not an error.
///
/// # Errors
/// 401 unauthenticated.
pub async fn for_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let found = account
        .acc
        .meeting_for_event(&EventId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "meeting": found.as_ref().map(meeting_json)
    })))
}

/// `POST /meet/{id}/join` — record attendance and mint a token for the engine.
///
/// The store decides first. Only a meeting the caller may be in produces a
/// token, and the token is for that meeting's room alone.
///
/// # Errors
/// 404 when the meeting is not the caller's or is over; 503 when no media
/// engine is configured — which is a deployment fact, not a bad request.
pub async fn join(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let meeting = account
        .acc
        .join_meeting(&MeetingId::new(id))
        .await
        .map_err(map_store_err)?;

    let Some(media) = state.media.as_ref() else {
        // Everything above still happened: the meeting exists and attendance
        // is recorded. Only the media is unavailable, and saying so plainly
        // beats a 500 that suggests the request was wrong.
        return Err(Problem::with(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "meetings are not configured on this deployment",
        ));
    };

    // What other participants see: the local part, never the address. A
    // participant list in a third party's logs must not be a list of who works
    // at a customer.
    // The directory answers through the tenant door, as chat's own name
    // resolution does. Best-effort: a name we cannot look up becomes
    // "someone", which is a poorer label but never a failed join.
    let display = state
        .store
        .for_tenant(account.tenant.clone())
        .emails_of(std::slice::from_ref(&account.user))
        .await
        .unwrap_or_default()
        .get(account.user.as_str())
        .and_then(|e| e.split('@').next())
        .unwrap_or("someone")
        .to_owned();
    let token = crate::meet_token::mint(
        &media.api_key,
        &media.api_secret,
        &meeting.room,
        account.user.as_str(),
        &display,
        OffsetDateTime::now_utc().unix_timestamp(),
    )
    .ok_or_else(|| {
        Problem::with(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "meetings are not configured on this deployment",
        )
    })?;

    Ok(Json(json!({
        "meeting": meeting_json(&meeting),
        "url": media.url,
        "token": token,
    })))
}

/// `POST /meet/{id}/end` — declare it over.
///
/// # Errors
/// 404 when the meeting is not the caller's to see.
pub async fn end(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .end_meeting(&MeetingId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "ok": true })))
}

/// `GET /meet/{id}/participants` — who has been in it.
///
/// # Errors
/// 404 when the meeting is not the caller's to see.
pub async fn participants(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let people = account
        .acc
        .meeting_participants(&MeetingId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "participants": people
            .iter()
            .map(|p| json!({ "user": p.user.as_str(), "joinedAt": iso(p.joined_at) }))
            .collect::<Vec<_>>()
    })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModerateParticipant {
    action: String,
    participant: String,
    #[serde(default)]
    track_sid: Option<String>,
}

/// `POST /meet/{id}/moderate` — let the host mute or remove a participant.
///
/// The browser never receives a room-admin token. alo resolves the opaque room,
/// verifies the caller created the meeting, then makes one narrowly scoped
/// RoomService call on their behalf.
pub async fn moderate(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<ModerateParticipant>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let meeting = account
        .acc
        .meeting(&MeetingId::new(id))
        .await
        .map_err(map_store_err)?;
    if meeting.created_by.as_str() != account.user.as_str() {
        return Err(Problem::with(
            axum::http::StatusCode::FORBIDDEN,
            "only the meeting host can moderate participants",
        ));
    }
    if body.participant.trim().is_empty() || body.participant == account.user.as_str() {
        return Err(Problem::with(
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            "choose another participant",
        ));
    }
    let Some(media) = state.media.as_ref() else {
        return Err(Problem::with(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "meetings are not configured on this deployment",
        ));
    };
    let token = crate::meet_token::mint_room_admin(
        &media.api_key,
        &media.api_secret,
        &meeting.room,
        OffsetDateTime::now_utc().unix_timestamp(),
    )
    .ok_or_else(Problem::server_error)?;
    let (method, payload) = match body.action.as_str() {
        "mute" => {
            let track_sid = body
                .track_sid
                .filter(|sid| !sid.trim().is_empty())
                .ok_or_else(|| {
                    Problem::with(
                        axum::http::StatusCode::UNPROCESSABLE_ENTITY,
                        "a microphone track is required",
                    )
                })?;
            (
                "MutePublishedTrack",
                json!({ "room": meeting.room, "identity": body.participant, "track_sid": track_sid, "muted": true }),
            )
        }
        "remove" => (
            "RemoveParticipant",
            json!({ "room": meeting.room, "identity": body.participant }),
        ),
        _ => {
            return Err(Problem::with(
                axum::http::StatusCode::UNPROCESSABLE_ENTITY,
                "moderation action must be mute or remove",
            ));
        }
    };
    let host = media
        .url
        .replacen("wss://", "https://", 1)
        .replacen("ws://", "http://", 1)
        .trim_end_matches('/')
        .to_owned();
    let response = reqwest::Client::new()
        .post(format!("{host}/twirp/livekit.RoomService/{method}"))
        .bearer_auth(token)
        .json(&payload)
        .send()
        .await
        .map_err(|_| {
            Problem::with(
                axum::http::StatusCode::BAD_GATEWAY,
                "the meeting server could not apply that action",
            )
        })?;
    if !response.status().is_success() {
        return Err(Problem::with(
            axum::http::StatusCode::BAD_GATEWAY,
            "the meeting server could not apply that action",
        ));
    }
    Ok(Json(json!({ "ok": true })))
}

fn recording_json(
    recording: &alo_store::MeetingRecording,
    consents: &[alo_store::MeetingRecordingConsent],
) -> Value {
    json!({
        "id": recording.id,
        "requestedBy": recording.requested_by.as_str(),
        "status": recording.status,
        "filePath": recording.file_path,
        "requestedAt": iso(recording.requested_at),
        "startedAt": recording.started_at.map(iso),
        "stoppedAt": recording.stopped_at.map(iso),
        "consents": consents.iter().map(|consent| json!({"user": consent.user.as_str(), "consentedAt": iso(consent.consented_at)})).collect::<Vec<_>>(),
    })
}

async fn livekit_post(
    media: &crate::state::MediaEngine,
    service: &str,
    method: &str,
    token: String,
    payload: Value,
) -> Result<Value, Problem> {
    let host = media
        .url
        .replacen("wss://", "https://", 1)
        .replacen("ws://", "http://", 1)
        .trim_end_matches('/')
        .to_owned();
    let response = reqwest::Client::new()
        .post(format!("{host}/twirp/livekit.{service}/{method}"))
        .bearer_auth(token)
        .json(&payload)
        .send()
        .await
        .map_err(|_| {
            Problem::with(
                axum::http::StatusCode::BAD_GATEWAY,
                "the meeting server did not respond",
            )
        })?;
    if !response.status().is_success() {
        return Err(Problem::with(
            axum::http::StatusCode::BAD_GATEWAY,
            "the meeting server could not complete that request",
        ));
    }
    response.json().await.map_err(|_| {
        Problem::with(
            axum::http::StatusCode::BAD_GATEWAY,
            "the meeting server returned an invalid response",
        )
    })
}

/// Begin the consent phase. Requesting is also the host's explicit consent.
pub async fn request_recording(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let meeting_id = MeetingId::new(id);
    let recording = account
        .acc
        .request_meeting_recording(&meeting_id)
        .await
        .map_err(map_store_err)?;
    let _ = account
        .acc
        .consent_to_meeting_recording(&meeting_id, &recording.id)
        .await
        .map_err(map_store_err)?;
    let consents = account
        .acc
        .meeting_recording_consents(&meeting_id, &recording.id)
        .await
        .map_err(map_store_err)?;
    Ok(Json(recording_json(&recording, &consents)))
}

pub async fn current_recording(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let meeting_id = MeetingId::new(id);
    let Some(recording) = account
        .acc
        .current_meeting_recording(&meeting_id)
        .await
        .map_err(map_store_err)?
    else {
        return Ok(Json(json!({"recording": null})));
    };
    let consents = account
        .acc
        .meeting_recording_consents(&meeting_id, &recording.id)
        .await
        .map_err(map_store_err)?;
    Ok(Json(
        json!({"recording": recording_json(&recording, &consents)}),
    ))
}

pub async fn consent_recording(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((meeting, recording)): Path<(String, String)>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let meeting_id = MeetingId::new(meeting);
    account
        .acc
        .consent_to_meeting_recording(&meeting_id, &recording)
        .await
        .map_err(map_store_err)?;
    let current = account
        .acc
        .current_meeting_recording(&meeting_id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(Problem::not_found)?;
    let consents = account
        .acc
        .meeting_recording_consents(&meeting_id, &recording)
        .await
        .map_err(map_store_err)?;
    Ok(Json(recording_json(&current, &consents)))
}

pub async fn start_recording(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((meeting, recording)): Path<(String, String)>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let meeting_id = MeetingId::new(meeting);
    let record = account
        .acc
        .current_meeting_recording(&meeting_id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(Problem::not_found)?;
    if record.id != recording || record.status != "pending" {
        return Err(Problem::with(
            axum::http::StatusCode::CONFLICT,
            "the recording is not waiting to start",
        ));
    }
    let meeting = account
        .acc
        .meeting(&meeting_id)
        .await
        .map_err(map_store_err)?;
    if meeting.created_by.as_str() != account.user.as_str() {
        return Err(Problem::with(
            axum::http::StatusCode::FORBIDDEN,
            "only the meeting host can start recording",
        ));
    }
    let Some(media) = state.media.as_ref() else {
        return Err(Problem::with(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "meetings are not configured on this deployment",
        ));
    };
    let admin = crate::meet_token::mint_room_admin(
        &media.api_key,
        &media.api_secret,
        &meeting.room,
        OffsetDateTime::now_utc().unix_timestamp(),
    )
    .ok_or_else(Problem::server_error)?;
    let live = livekit_post(
        media,
        "RoomService",
        "ListParticipants",
        admin,
        json!({"room": meeting.room}),
    )
    .await?;
    let attended: HashSet<String> = account
        .acc
        .meeting_participants(&meeting_id)
        .await
        .map_err(map_store_err)?
        .into_iter()
        .map(|p| p.user.as_str().to_owned())
        .collect();
    let current: HashSet<String> = live["participants"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| p["identity"].as_str())
        .filter(|identity| attended.contains(*identity))
        .map(ToOwned::to_owned)
        .collect();
    let consented: HashSet<String> = account
        .acc
        .meeting_recording_consents(&meeting_id, &recording)
        .await
        .map_err(map_store_err)?
        .into_iter()
        .map(|c| c.user.as_str().to_owned())
        .collect();
    let missing = current.difference(&consented).count();
    if missing > 0 {
        return Err(Problem::with(
            axum::http::StatusCode::CONFLICT,
            "everyone currently in the meeting must consent before recording starts",
        )
        .with_extra(json!({"missingConsents": missing})));
    }
    let token = crate::meet_token::mint_room_record(
        &media.api_key,
        &media.api_secret,
        OffsetDateTime::now_utc().unix_timestamp(),
    )
    .ok_or_else(Problem::server_error)?;
    let file_path = format!("meet/{}/{}.mp4", meeting_id.as_str(), recording);
    let started = livekit_post(media, "Egress", "StartRoomCompositeEgress", token, json!({"room_name": meeting.room, "layout": "speaker", "file_outputs": [{"filepath": file_path}]})).await?;
    let egress_id = started["egress_id"]
        .as_str()
        .filter(|id| !id.is_empty())
        .ok_or_else(|| {
            Problem::with(
                axum::http::StatusCode::BAD_GATEWAY,
                "the recording service returned no recording id",
            )
        })?;
    let updated = account
        .acc
        .mark_meeting_recording_started(&meeting_id, &recording, egress_id, &file_path)
        .await
        .map_err(map_store_err)?;
    let consents = account
        .acc
        .meeting_recording_consents(&meeting_id, &recording)
        .await
        .map_err(map_store_err)?;
    Ok(Json(recording_json(&updated, &consents)))
}

pub async fn stop_recording(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((meeting, recording)): Path<(String, String)>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let meeting_id = MeetingId::new(meeting);
    let current = account
        .acc
        .current_meeting_recording(&meeting_id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(Problem::not_found)?;
    if current.id != recording || current.status != "recording" {
        return Err(Problem::with(
            axum::http::StatusCode::CONFLICT,
            "the recording is not running",
        ));
    }
    let meeting = account
        .acc
        .meeting(&meeting_id)
        .await
        .map_err(map_store_err)?;
    if meeting.created_by.as_str() != account.user.as_str() {
        return Err(Problem::with(
            axum::http::StatusCode::FORBIDDEN,
            "only the meeting host can stop recording",
        ));
    }
    let Some(media) = state.media.as_ref() else {
        return Err(Problem::with(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "meetings are not configured on this deployment",
        ));
    };
    let token = crate::meet_token::mint_room_record(
        &media.api_key,
        &media.api_secret,
        OffsetDateTime::now_utc().unix_timestamp(),
    )
    .ok_or_else(Problem::server_error)?;
    let egress_id = current
        .egress_id
        .as_deref()
        .ok_or_else(Problem::server_error)?;
    livekit_post(
        media,
        "Egress",
        "StopEgress",
        token,
        json!({"egress_id": egress_id}),
    )
    .await?;
    let stopped = account
        .acc
        .mark_meeting_recording_stopped(&meeting_id, &recording)
        .await
        .map_err(map_store_err)?;
    let consents = account
        .acc
        .meeting_recording_consents(&meeting_id, &recording)
        .await
        .map_err(map_store_err)?;
    Ok(Json(recording_json(&stopped, &consents)))
}

fn attachment_json(meeting_id: &str, attachment: &alo_store::MeetingMessageAttachment) -> Value {
    json!({ "id": attachment.id, "name": attachment.file_name, "contentType": attachment.content_type, "size": attachment.size,
        "url": format!("/api/meet/{meeting_id}/messages/{}/attachments/{}", attachment.message_id, attachment.id) })
}

fn message_json(
    message: &alo_store::MeetingMessage,
    attachments: &[alo_store::MeetingMessageAttachment],
    reactions: &[alo_store::MeetingMessageReaction],
    meeting_id: &str,
) -> Value {
    json!({
        "id": message.id,
        "sender": message.sender.as_str(),
        "recipient": message.recipient.as_ref().map(UserId::as_str),
        "body": message.body,
        "createdAt": iso(message.created_at),
        "attachments": attachments.iter().filter(|a| a.message_id == message.id).map(|a| attachment_json(meeting_id, a)).collect::<Vec<_>>(),
        "reactions": reactions.iter().filter(|r| r.message_id == message.id).map(|r| json!({"emoji":r.emoji,"actor":r.user.as_str()})).collect::<Vec<_>>(),
    })
}

/// `GET /meet/{id}/messages` — durable messages visible to this participant.
pub async fn messages(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let meeting_id = MeetingId::new(id);
    let messages = account
        .acc
        .meeting_messages(&meeting_id)
        .await
        .map_err(map_store_err)?;
    let attachments = account
        .acc
        .meeting_message_attachments(&meeting_id)
        .await
        .map_err(map_store_err)?;
    let reactions = account
        .acc
        .meeting_message_reactions(&meeting_id)
        .await
        .map_err(map_store_err)?;
    Ok(Json(
        json!({ "messages": messages.iter().map(|m| message_json(m, &attachments, &reactions, meeting_id.as_str())).collect::<Vec<_>>() }),
    ))
}

#[derive(Deserialize)]
pub struct PostMeetingMessage {
    body: String,
    #[serde(default)]
    recipient: Option<String>,
}

/// `POST /meet/{id}/messages` — persist before real-time broadcast.
pub async fn post_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<PostMeetingMessage>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let recipient = body.recipient.map(UserId::new);
    let meeting_id = MeetingId::new(id);
    let message = account
        .acc
        .post_meeting_message(&meeting_id, &body.body, recipient.as_ref())
        .await
        .map_err(map_store_err)?;
    Ok(Json(message_json(&message, &[], &[], meeting_id.as_str())))
}

#[derive(Deserialize)]
pub struct PostReaction {
    emoji: String,
}

pub async fn react(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((meeting, message)): Path<(String, String)>,
    Json(body): Json<PostReaction>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .react_to_meeting_message(&MeetingId::new(meeting), &message, &body.emoji)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({"ok":true})))
}

fn transcript_json(segment: &alo_store::MeetingTranscriptSegment) -> Value {
    json!({
        "id": segment.id,
        "speaker": segment.speaker.as_str(),
        "text": segment.text,
        "final": segment.final_segment,
        "createdAt": iso(segment.created_at),
    })
}

pub async fn transcript(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let segments = account
        .acc
        .meeting_transcript(&MeetingId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(
        json!({ "segments": segments.iter().map(transcript_json).collect::<Vec<_>>() }),
    ))
}

#[derive(Deserialize)]
pub struct PutTranscriptSegment {
    id: String,
    text: String,
    #[serde(rename = "final", default)]
    final_segment: bool,
}

pub async fn put_transcript_segment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<PutTranscriptSegment>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let segment = account
        .acc
        .put_meeting_transcript_segment(
            &MeetingId::new(id),
            &body.id,
            &body.text,
            body.final_segment,
        )
        .await
        .map_err(map_store_err)?;
    Ok(Json(transcript_json(&segment)))
}

#[derive(Deserialize)]
pub struct AttachmentName {
    name: String,
}

pub async fn upload_attachment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((meeting, message)): Path<(String, String)>,
    Query(query): Query<AttachmentName>,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream");
    let attachment = account
        .acc
        .attach_to_meeting_message(
            &MeetingId::new(meeting.clone()),
            &message,
            &query.name,
            content_type,
            body.to_vec(),
        )
        .await
        .map_err(map_store_err)?;
    Ok(Json(attachment_json(&meeting, &attachment)))
}

pub async fn download_attachment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((meeting, message, attachment)): Path<(String, String, String)>,
) -> Result<Response, Problem> {
    let account = authenticate(&state, &headers).await?;
    let file = account
        .acc
        .meeting_message_attachment(&MeetingId::new(meeting), &message, &attachment)
        .await
        .map_err(map_store_err)?;
    let mut response = Response::new(Body::from(file.data.unwrap_or_default()));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&file.content_type)
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!(
            "attachment; filename=\"{}\"",
            file.file_name.replace(['\"', '\\'], "_")
        ))
        .unwrap_or_else(|_| HeaderValue::from_static("attachment")),
    );
    Ok(response)
}
