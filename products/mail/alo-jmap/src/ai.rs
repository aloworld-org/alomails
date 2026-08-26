//! The AI inference endpoint (ADR 0011): a thin, authenticated, tenant-scoped
//! bridge to `alo-ai`. It loads the tenant's operator-set config and calls
//! the configured OpenAI-compatible backend. Draft text and completions are
//! never logged (law #1); errors carry a coarse machine code, never a backend
//! body.

use alo_ai::{AiConfig, InferenceError, WorkspaceSource};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::{Json, body::Bytes};
use serde_json::{Value, json};

use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// Cap the question sent to "ask your workspace" (bytes). A question, not a
/// document — kept small.
pub const MAX_ASK_BYTES: usize = 4 * 1024;

/// How many retrieved items to ground the answer on. Enough to cover the
/// question without overrunning the model's context.
const ASK_SOURCES: i64 = 8;

/// Cap the draft we send for improvement (bytes) — a sane bound independent of
/// the JMAP request ceiling. Also applied as a per-route body limit in
/// `server.rs` so an oversized upload is rejected before it is buffered, not
/// after (the router-wide limit is the much larger blob-upload ceiling).
pub const MAX_IMPROVE_BYTES: usize = 64 * 1024;

/// Cap the thread text we send for summarization (bytes) — larger than a single
/// draft since it is a whole conversation, but still bounded.
pub const MAX_SUMMARIZE_BYTES: usize = 256 * 1024;

/// An encoded phone photo or screenshot of a price list. Eight MiB of source
/// image becomes roughly eleven MiB as base64 JSON; cap before parsing.
pub const MAX_PRICE_IMAGE_BYTES: usize = 12 * 1024 * 1024;

/// Load the tenant's default AI backend config, or a 503 problem if none is set.
pub(crate) async fn tenant_ai_config(account: &crate::state::Account) -> Result<AiConfig, Problem> {
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

/// Translate a short caption into one explicitly selected language.
pub async fn translate(
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
    let text = request
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let language = request
        .get("language")
        .and_then(Value::as_str)
        .unwrap_or("");
    let language_name = match language {
        "en" => "English",
        "fr" => "French",
        "nl" => "Dutch",
        _ => {
            return Err(Problem::with(
                StatusCode::BAD_REQUEST,
                "supported language required",
            ));
        }
    };
    if text.is_empty() {
        return Err(Problem::with(StatusCode::BAD_REQUEST, "text required"));
    }
    let config = tenant_ai_config(&account).await?;
    let translated = alo_ai::translate_text(&config, text, language_name)
        .await
        .map_err(|e| ai_problem(&e))?;
    Ok(Json(json!({ "text": translated })))
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

/// `POST /ai/extract-price-list` proposes structured rows from one image. It
/// never creates products: the Billing review screen is the only writer.
pub async fn extract_price_list(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    if body.len() > MAX_PRICE_IMAGE_BYTES {
        return Err(Problem::with(StatusCode::PAYLOAD_TOO_LARGE, "image too large"));
    }
    let request: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let data_url = request.get("dataUrl").and_then(Value::as_str).unwrap_or("");
    let supported = ["data:image/jpeg;base64,", "data:image/png;base64,", "data:image/webp;base64,"];
    if !supported.iter().any(|prefix| data_url.starts_with(prefix)) {
        return Err(Problem::with(StatusCode::BAD_REQUEST, "JPEG, PNG or WebP image required"));
    }
    let config = tenant_ai_config(&account).await?;
    let rows = alo_ai::extract_price_list_image(&config, data_url)
        .await
        .map_err(|error| ai_problem(&error))?;
    Ok(Json(json!({ "rows": rows })))
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

/// `POST /ai/ask` — `{"q": "<question>"}` → `{"answer": "..."|null, "reason":
/// null|"unconfigured"|"unreachable", "sources": [{kind,id,title,space}]}`.
///
/// The cross-workspace, source-cited assistant (ADR 0029 §1). Retrieval runs
/// first and is **always** returned, scoped to exactly what the caller can see
/// (their files, Spaces, tasks, and mailbox — the same `workspace_search`
/// predicates). Only then is the model asked to answer *from those sources* and
/// cite them. Access is never widened by AI. If no model is configured or the
/// backend is unreachable, the matches are still returned (`answer: null` with a
/// `reason`) — the search half degrades gracefully without the AI half.
pub async fn ask(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    if body.len() > MAX_ASK_BYTES {
        return Err(Problem::with(
            StatusCode::PAYLOAD_TOO_LARGE,
            "question too large",
        ));
    }
    let request: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let question = request
        .get("q")
        .or_else(|| request.get("question"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_owned();
    if question.is_empty() {
        return Err(Problem::with(StatusCode::BAD_REQUEST, "q required"));
    }

    // Access-scoped retrieval — the only thing the AI may ever see. Keyword-aware
    // so a natural-language question matches on its content words.
    let hits = account
        .acc
        .workspace_search_terms(&question, ASK_SOURCES)
        .await
        .map_err(|_| Problem::server_error())?;
    let sources_json: Vec<Value> = hits
        .iter()
        .map(|h| json!({ "kind": h.kind, "id": h.id, "title": h.title, "space": h.space }))
        .collect();

    // Nothing matched → no model call; the UI shows "no matches".
    if hits.is_empty() {
        return Ok(Json(
            json!({ "answer": Value::Null, "reason": Value::Null, "sources": sources_json }),
        ));
    }

    let ground: Vec<WorkspaceSource> = hits
        .iter()
        .enumerate()
        .map(|(i, h)| WorkspaceSource {
            index: i + 1,
            kind: h.kind.clone(),
            title: h.title.clone(),
            detail: String::new(),
        })
        .collect();

    // Model half — degrade to sources-only if AI is off or unreachable.
    let Some(row) = account
        .acc
        .default_ai_config()
        .await
        .map_err(|_| Problem::server_error())?
    else {
        return Ok(Json(
            json!({ "answer": Value::Null, "reason": "unconfigured", "sources": sources_json }),
        ));
    };
    let config = AiConfig {
        base_url: row.base_url,
        model: row.model,
        api_key: row.api_key,
        enabled: row.enabled,
    };
    match alo_ai::ask_workspace(&config, &question, &ground).await {
        Ok(answer) => Ok(Json(
            json!({ "answer": answer, "reason": Value::Null, "sources": sources_json }),
        )),
        Err(InferenceError::Disabled | InferenceError::NotConfigured) => Ok(Json(
            json!({ "answer": Value::Null, "reason": "unconfigured", "sources": sources_json }),
        )),
        Err(_) => Ok(Json(
            json!({ "answer": Value::Null, "reason": "unreachable", "sources": sources_json }),
        )),
    }
}

/// `POST /ai/compose` — `{"instruction": "...", "context": "<current doc md>"}`
/// → `{"proposal": "..."}`. Document AI (ADR 0029 §3): returns *proposed* text
/// for the alo Doc editor to show; the editor only writes it on the user's
/// approval, never silently. Degrades like the other AI endpoints (503 when AI
/// is off, 502 on a backend failure).
pub async fn compose(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    if body.len() > MAX_SUMMARIZE_BYTES {
        return Err(Problem::with(
            StatusCode::PAYLOAD_TOO_LARGE,
            "context too large",
        ));
    }
    let request: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let instruction = request
        .get("instruction")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if instruction.is_empty() {
        return Err(Problem::with(
            StatusCode::BAD_REQUEST,
            "instruction required",
        ));
    }
    let context = request.get("context").and_then(Value::as_str).unwrap_or("");
    let config = tenant_ai_config(&account).await?;
    let proposal = alo_ai::compose_doc(&config, instruction, context)
        .await
        .map_err(|e| ai_problem(&e))?;
    Ok(Json(json!({ "proposal": proposal })))
}

/// Map an inference error to a client problem with a coarse, safe code.
pub(crate) fn ai_problem(err: &InferenceError) -> Problem {
    match err {
        InferenceError::Disabled | InferenceError::NotConfigured => {
            Problem::with(StatusCode::SERVICE_UNAVAILABLE, "ai-unavailable")
        }
        InferenceError::Backend(_) | InferenceError::Transport | InferenceError::Empty => {
            Problem::with(StatusCode::BAD_GATEWAY, "ai-backend")
        }
    }
}
