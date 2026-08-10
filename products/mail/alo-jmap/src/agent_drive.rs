//! Executing alo Drive's agent tools (ADR 0034).
//!
//! One tool, `find_file`, and it only reads. It runs against the caller's own
//! account door, so the agent finds exactly what the person who asked could
//! have found themselves — the same rule that governs everything else an agent
//! does on somebody's behalf.

use axum::Json;
use serde_json::{Value, json};

use crate::agent_args::{string_arg, unprocessable};
use crate::billing::map_store_err;
use crate::error::Problem;
use crate::state::Account;

/// `find_file` — files in the caller's Drive matching what they called it.
///
/// # Errors
/// 400 when no query was given; 500 on a store failure.
pub async fn execute_find_file(account: &Account, args: &Value) -> Result<Json<Value>, Problem> {
    // The model was told to ask which file rather than search for something
    // plausible; if it did neither, say so plainly instead of returning the
    // first twenty files in the drive.
    let query = string_arg(args, "query").ok_or_else(|| unprocessable("query is required"))?;
    let query = query.trim();
    if query.is_empty() {
        return Err(unprocessable("query is required"));
    }
    let limit = args.get("limit").and_then(Value::as_i64).unwrap_or(10);
    let found = account
        .acc
        .drive_find(query, limit)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "driveFiles",
            "query": query,
            "files": found
                .iter()
                .map(|node| json!({
                    "id": node.id.as_str(),
                    "name": node.name,
                    "kind": node.kind,
                    "size": node.size,
                    "contentType": node.content_type,
                    "updatedAt": node.updated_at
                        .format(&time::format_description::well_known::Rfc3339)
                        .unwrap_or_default(),
                }))
                .collect::<Vec<_>>()
        }
    })))
}
