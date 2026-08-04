//! The AI inference endpoint (ADR 0011): a thin, authenticated, tenant-scoped
//! bridge to `alo-ai`. It loads the tenant's operator-set config and calls
//! the configured OpenAI-compatible backend. Draft text and completions are
//! never logged (law #1); errors carry a coarse machine code, never a backend
//! body.

use alo_ai::{AiConfig, InferenceError};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::{Json, body::Bytes};
use serde_json::{Value, json};

use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// Cap the draft we send for improvement (bytes) — a sane bound independent of
/// the JMAP request ceiling. Also applied as a per-route body limit in
/// `server.rs` so an oversized upload is rejected before it is buffered, not
/// after (the router-wide limit is the much larger blob-upload ceiling).
pub const MAX_IMPROVE_BYTES: usize = 64 * 1024;

/// Cap the thread text we send for summarization (bytes) — larger than a single
/// draft since it is a whole conversation, but still bounded.
pub const MAX_SUMMARIZE_BYTES: usize = 256 * 1024;

/// Load the tenant's default AI backend config, or a 503 problem if none is set.
async fn tenant_ai_config(account: &crate::state::Account) -> Result<AiConfig, Problem> {
    let row = account
        .acc
        .default_ai_config()
        .await
        .map_err(|_| Problem::server_error())?
        .ok_or_else(|| ai_problem(&InferenceError::NotConfigured))?;
    Ok(AiConfig {
        base_url: row.base_url,
        model: row.model,
        api_key: row.api_key,
        enabled: row.enabled,
    })
}

/// `POST /ai/summarize` — `{"text": "<thread>"}` → `{"summary": "..."}`. The
/// reading pane calls this when a conversation opens; degrades the same way as
/// `/ai/improve` (503 when AI is off, 502 on a backend failure).
pub async fn summarize(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    if body.len() > MAX_SUMMARIZE_BYTES {
        return Err(Problem::with(
            StatusCode::PAYLOAD_TOO_LARGE,
            "text too large",
        ));
    }
    let request: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let text = request.get("text").and_then(Value::as_str).unwrap_or("");
    if text.trim().is_empty() {
        return Err(Problem::with(StatusCode::BAD_REQUEST, "text required"));
    }
    let config = tenant_ai_config(&account).await?;
    let summary = alo_ai::summarize(&config, text)
        .await
        .map_err(|e| ai_problem(&e))?;
    Ok(Json(json!({ "summary": summary })))
}

/// `POST /ai/replies` — `{"text": "<thread>"}` → `{"replies": ["...", ...]}`.
/// Suggests up to three short replies for the open conversation; degrades like
/// the other AI endpoints (503 when AI is off, 502 on a backend failure).
pub async fn replies(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    if body.len() > MAX_SUMMARIZE_BYTES {
        return Err(Problem::with(
            StatusCode::PAYLOAD_TOO_LARGE,
            "text too large",
        ));
    }
    let request: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let text = request.get("text").and_then(Value::as_str).unwrap_or("");
    if text.trim().is_empty() {
        return Err(Problem::with(StatusCode::BAD_REQUEST, "text required"));
    }
    let config = tenant_ai_config(&account).await?;
    let replies = alo_ai::suggest_replies(&config, text)
        .await
        .map_err(|e| ai_problem(&e))?;
    Ok(Json(json!({ "replies": replies })))
}

/// `POST /ai/extract-tasks` — `{"text": "<email>"}` → `{"tasks": [{"title": "..."}]}`.
/// Reads an email's text and returns candidate action items (titles only) for the
/// propose-then-approve flow (ADR 0024). Never creates a task itself — the client
/// feeds these to `/tasks/propose`. Degrades like the other AI endpoints (503 when
/// AI is off, 502 on a backend failure).
pub async fn extract_tasks(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    if body.len() > MAX_SUMMARIZE_BYTES {
        return Err(Problem::with(
            StatusCode::PAYLOAD_TOO_LARGE,
            "text too large",
        ));
    }
    let request: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let text = request.get("text").and_then(Value::as_str).unwrap_or("");
    if text.trim().is_empty() {
        return Err(Problem::with(StatusCode::BAD_REQUEST, "text required"));
    }
    let config = tenant_ai_config(&account).await?;
    let tasks = alo_ai::extract_tasks(&config, text)
        .await
        .map_err(|e| ai_problem(&e))?;
    Ok(Json(json!({ "tasks": tasks })))
}

/// `POST /ai/improve` — `{"text": "..."}` → `{"text": "improved"}`.
///
/// Soft-degrading by contract: if AI is disabled/unconfigured the caller gets a
/// 503 (the UI hides the control when the session says AI is off, so this is a
/// fallback); a backend failure is a 502. Neither blocks the user's own action.
pub async fn improve(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    if body.len() > MAX_IMPROVE_BYTES {
        return Err(Problem::with(
            StatusCode::PAYLOAD_TOO_LARGE,
            "text too large",
        ));
    }
    let request: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let text = request.get("text").and_then(Value::as_str).unwrap_or("");
    if text.trim().is_empty() {
        return Err(Problem::with(StatusCode::BAD_REQUEST, "text required"));
    }

    let config = tenant_ai_config(&account).await?;
    let improved = alo_ai::improve(&config, text)
        .await
        .map_err(|e| ai_problem(&e))?;
    Ok(Json(json!({ "text": improved })))
}

/// Map an inference error to a client problem with a coarse, safe code.
fn ai_problem(err: &InferenceError) -> Problem {
    match err {
        InferenceError::Disabled | InferenceError::NotConfigured => {
            Problem::with(StatusCode::SERVICE_UNAVAILABLE, "ai-unavailable")
        }
        InferenceError::Backend(_) | InferenceError::Transport | InferenceError::Empty => {
            Problem::with(StatusCode::BAD_GATEWAY, "ai-backend")
        }
    }
}
