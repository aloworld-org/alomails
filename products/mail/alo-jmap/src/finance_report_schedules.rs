//! Persisted delivery schedules for Finance reports.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::FinReportSchedule;

use crate::billing::{iso, iso_date, map_store_err, parse_body};
use crate::error::Problem;
use crate::finance_expenses::stated_day;
use crate::state::{AppState, authenticate};

fn schedule_json(value: &FinReportSchedule) -> Value {
    json!({
        "id": value.id, "report": value.report, "cadence": value.cadence,
        "format": value.format, "recipient": value.recipient, "active": value.active,
        "nextRunDate": iso_date(value.next_run_date),
        "lastRunAt": value.last_run_at.map(iso), "createdBy": value.created_by,
        "createdAt": iso(value.created_at), "updatedAt": iso(value.updated_at),
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScheduleBody {
    report: String,
    cadence: String,
    recipient: String,
    next_run_date: String,
}

pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_finance()?;
    let rows = state
        .store
        .for_tenant(account.tenant)
        .fin_report_schedules()
        .await
        .map_err(map_store_err)?;
    Ok(Json(
        json!({"schedules": rows.iter().map(schedule_json).collect::<Vec<_>>() }),
    ))
}

pub async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<(StatusCode, Json<Value>), Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_finance()?;
    let request: ScheduleBody = parse_body(&body)?;
    let next = stated_day("nextRunDate", &request.next_run_date)?;
    state
        .store
        .for_tenant(account.tenant.clone())
        .user_by_email(&request.recipient)
        .await
        .map_err(|_| {
            Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                "recipient must be a member of this workspace",
            )
        })?;
    let row = state
        .store
        .for_tenant(account.tenant)
        .create_fin_report_schedule(
            &request.report,
            &request.cadence,
            &request.recipient,
            next,
            &account.user,
        )
        .await
        .map_err(map_store_err)?;
    Ok((
        StatusCode::CREATED,
        Json(json!({"schedule":schedule_json(&row)})),
    ))
}

pub async fn remove(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_finance()?;
    let deleted = state
        .store
        .for_tenant(account.tenant)
        .delete_fin_report_schedule(&id)
        .await
        .map_err(map_store_err)?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(Problem::with(
            StatusCode::NOT_FOUND,
            "report schedule was not found",
        ))
    }
}
