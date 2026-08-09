//! Workspace search HTTP surface (ADR 0029): one authenticated query across the
//! caller's Drive files, visible tasks, and own mail (by full message content).
//! Access is enforced in the store, so this only shapes the response.

use axum::Json;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::error::Problem;
use crate::state::{AppState, authenticate};

#[derive(Deserialize)]
pub struct SearchQuery {
    #[serde(default)]
    q: String,
    #[serde(default)]
    limit: Option<i64>,
}

/// `GET /search?q=&limit=` → `{"hits":[{kind,id,title,space}]}` — files and tasks
/// matching by name/title and mail matching by content, scoped to what the
/// caller can see.
pub async fn search(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<SearchQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let limit = q.limit.unwrap_or(20).clamp(1, 50);
    let hits = account
        .acc
        .workspace_search(&q.q, limit)
        .await
        .map_err(|_| Problem::server_error())?;
    Ok(Json(json!({
        "hits": hits.iter().map(|h| json!({
            "kind": h.kind, "id": h.id, "title": h.title, "space": h.space,
        })).collect::<Vec<_>>(),
    })))
}
