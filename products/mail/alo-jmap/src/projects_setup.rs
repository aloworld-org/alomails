//! Reviewed optional setup around one delivery project.

use alo_store::{KickoffPlan, ProjectId, ProjectSetup, ProjectSetupPlan};
use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::billing::{iso, map_store_err, parse_body, parse_rfc3339};
use crate::error::Problem;
use crate::state::{AppState, authenticate};

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct SetupBody {
    #[serde(default)]
    create_files_space: bool,
    #[serde(default)]
    create_chat_room: bool,
    #[serde(default)]
    kickoff: Option<KickoffBody>,
    #[serde(default)]
    starter_tasks: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct KickoffBody {
    starts_at: String,
    ends_at: String,
    #[serde(default)]
    timezone: Option<String>,
}

fn setup_json(setup: &ProjectSetup) -> Value {
    json!({
        "projectId": setup.project_id.as_str(),
        "spaceId": setup.space_id,
        "chatChannelId": setup.chat_channel_id,
        "kickoffEventId": setup.kickoff_event_id,
        "starterTaskIds": setup.starter_task_ids,
        "createdBy": setup.created_by,
        "createdAt": iso(setup.created_at),
        "updatedAt": iso(setup.updated_at),
    })
}

pub async fn get_setup(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let setup = account
        .acc
        .project_setup(&ProjectId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "setup": setup.as_ref().map(setup_json) })))
}

pub async fn create_setup(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let request: SetupBody = parse_body(&body)?;
    let kickoff = request
        .kickoff
        .map(|kickoff| -> Result<KickoffPlan, Problem> {
            let starts_at = parse_rfc3339(&kickoff.starts_at).ok_or_else(|| {
                Problem::with(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "kickoff startsAt must be RFC 3339",
                )
            })?;
            let ends_at = parse_rfc3339(&kickoff.ends_at).ok_or_else(|| {
                Problem::with(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "kickoff endsAt must be RFC 3339",
                )
            })?;
            Ok(KickoffPlan {
                starts_at,
                ends_at,
                timezone: kickoff.timezone,
            })
        })
        .transpose()?;
    let setup = account
        .acc
        .setup_project(
            &ProjectId::new(id),
            &ProjectSetupPlan {
                create_files_space: request.create_files_space,
                create_chat_room: request.create_chat_room,
                kickoff,
                starter_tasks: request.starter_tasks,
            },
        )
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "setup": setup_json(&setup) })))
}
