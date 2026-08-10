//! The Meet API — start a meeting, join one, see who is in it, end it.
//!
//! alo owns the record; the media engine owns the media. Every route here is
//! about the record, and exactly one of them touches the engine: `join` mints
//! a token, and only after the store has already decided the caller may be in
//! the room. That ordering is the security property — the token is minted from
//! an answer, never from a request.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use serde::Deserialize;
use serde_json::{Value, json};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

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
