//! The caller's own action record on the wire, and its Undo (ADR 0058 §6,
//! queue item A8.2).
//!
//! `GET /ai/actions` lists what has been done through this person's access —
//! their own taps and every agent run that carried their reach — in exactly
//! the shape the agent directory reports a single agent's runs, because the
//! two are one record read through two doors. `POST /ai/actions/{id}/undo`
//! takes one of those actions back: it runs the **inverse verb the registry
//! declared**, with the arguments the action row kept, through the same
//! execution boundary as everything else — so the undo is itself an intent
//! execution, leaves its own action row, emits its own event, and refuses
//! cleanly when the record is already gone. A person's click and an agent's
//! proposal are the same object, so one button undoes both.
//!
//! **Only the caller's own actions.** The row was written through their
//! access and the undo runs through it again; another tenant's id, a
//! colleague's run and an id never issued all answer the same 404.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use serde_json::{Value, json};

use alo_store::ChatToolRunId;

use crate::agent::{ToolRun, execute_tool};
use crate::agent_directory::run_json;
use crate::chat_agent_routes::map_store_err;
use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// How many actions one page answers — the same window the directory shows
/// per agent, here across all of them.
const RECENT_ACTIONS: i64 = 50;

/// `GET /ai/actions` → `{"actions":[…]}` — what has been done through this
/// person's access, most recent first, each row saying what it would do
/// (`preview`), what it touched (`record`) and whether it can be taken back
/// (`undoable`).
///
/// # Errors
/// 401 unauthenticated.
pub async fn list_actions(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let runs = account
        .acc
        .agent_tool_runs(RECENT_ACTIONS)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "actions": runs.iter().map(run_json).collect::<Vec<_>>(),
    })))
}

/// `POST /ai/actions/{id}/undo` → `{"ok":true,"result":…}` — take one action
/// back with its declared inverse.
///
/// The person pressing the button is the actor: the undo runs as their own
/// tap (no agent, no proposal), exactly like an execution from the command
/// palette, and leaves its own action row saying so. Undoing twice is not
/// special-cased — the second run reaches the executor and is refused there,
/// because the record it names is already gone, which is the honest answer.
///
/// # Errors
/// 404 when the action is not the caller's own; 422 when it has no inverse
/// (a read, a failed run, or a verb whose purpose says it cannot be undone);
/// otherwise whatever the inverse verb's executor raises.
pub async fn undo_action(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let action = account
        .acc
        .agent_tool_run(&ChatToolRunId::new(id))
        .await
        .map_err(map_store_err)?;
    let (Some(undo_tool), Some(undo_args)) = (action.undo_tool.as_deref(), &action.undo_args)
    else {
        return Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "that action has no undo",
        ));
    };
    // The button IS the approval, and it is the caller's own — the same
    // semantics as a palette tap, through the same boundary: allowlist,
    // audit row, event.
    execute_tool(&state, &account, undo_tool, undo_args, &ToolRun::approved()).await
}
