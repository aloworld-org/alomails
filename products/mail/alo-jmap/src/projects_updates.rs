//! Project status updates HTTP surface.

use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::{ProjectId, ProjectUpdate, ProjectUpdateAttachment, ProjectUpdateState};

use crate::billing::{iso, map_store_err, parse_body};
use crate::error::Problem;
use crate::state::{AppState, authenticate};

fn update_json(update: &ProjectUpdate) -> Value {
    json!({
        "id": update.id.as_str(),
        "projectId": update.project_id.as_str(),
        "state": update.state.as_str(),
        "body": update.body,
        "authorId": update.author_id,
        "authorEmail": update.author_email,
        "createdAt": iso(update.created_at),
        "attachments": update.attachments,
    })
}

#[derive(Deserialize)]
pub struct UpdatesQuery {
    #[serde(default, rename = "projectId")]
    project_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NewUpdateBody {
    #[serde(default)]
    project_id: Option<String>,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    attachments: Vec<ProjectUpdateAttachment>,
}

fn required(name: &str, value: Option<&str>) -> Result<String, Problem> {
    value
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| {
            Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("{name} is required"),
            )
        })
}

pub async fn list_updates(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<UpdatesQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let project = ProjectId::new(required("projectId", query.project_id.as_deref())?);
    let updates = account
        .acc
        .project_updates(&project)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "updates": updates.iter().map(update_json).collect::<Vec<_>>()
    })))
}

pub async fn create_update(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<(StatusCode, Json<Value>), Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: NewUpdateBody = parse_body(&body)?;
    let project = ProjectId::new(required("projectId", req.project_id.as_deref())?);
    if req.attachments.len() > 8
        || req.attachments.iter().any(|attachment| {
            attachment.blob_id.trim().is_empty()
                || attachment.filename.trim().is_empty()
                || attachment.size < 0
        })
    {
        return Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "up to 8 valid attachments are allowed",
        ));
    }
    let state = ProjectUpdateState::parse(&required("state", req.state.as_deref())?)
        .map_err(map_store_err)?;
    let update = account
        .acc
        .create_project_update(
            &project,
            state,
            &required("body", req.body.as_deref())?,
            &req.attachments,
        )
        .await
        .map_err(map_store_err)?;
    Ok((
        StatusCode::CREATED,
        Json(json!({ "update": update_json(&update) })),
    ))
}
