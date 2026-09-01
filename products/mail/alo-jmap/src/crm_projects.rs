//! HTTP contract for the durable Sales ↔ Projects relationship.
//! Creation is an explicit confirmed POST and delegates the atomic, idempotent
//! conversion to the account-scoped store.

use alo_store::{BillingCustomerId, CrmDealId, DealProject, NewProjectClient, ProjectId};
use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::billing::{iso, map_store_err, parse_body};
use crate::error::Problem;
use crate::state::{AppState, authenticate};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateProjectBody {
    name: String,
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    customer_id: Option<String>,
}

fn relationship_json(link: &DealProject) -> Value {
    json!({
        "dealId": link.deal_id.as_str(),
        "dealTitle": link.deal_title,
        "projectId": link.project_id.as_str(),
        "projectName": link.project_name,
        "createdBy": link.created_by,
        "createdAt": iso(link.created_at),
    })
}

/// `GET /crm/deals/{id}/project` returns the linked project or `null`.
pub async fn deal_project(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let link = account
        .acc
        .crm_deal_project(&CrmDealId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(
        json!({"project": link.as_ref().map(relationship_json)}),
    ))
}

/// `POST /crm/deals/{id}/project` confirms conversion of a won deal.
pub async fn create_deal_project(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<(StatusCode, Json<Value>), Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: CreateProjectBody = parse_body(&body)?;
    let color = req
        .color
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let client = req
        .customer_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| NewProjectClient::for_customer(BillingCustomerId::new(value)));
    let (link, created) = account
        .acc
        .create_project_from_won_deal(&CrmDealId::new(id), &req.name, color, client.as_ref())
        .await
        .map_err(map_store_err)?;
    let status = if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((
        status,
        Json(json!({"project": relationship_json(&link), "created": created})),
    ))
}

/// `GET /projects/{id}/deal` returns the originating Sales deal or `null`.
pub async fn project_deal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let link = account
        .acc
        .crm_project_deal(&ProjectId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({"deal": link.as_ref().map(relationship_json)})))
}
