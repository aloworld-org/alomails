//! The executors of alo Sheets' verbs (ADR 0058, queue item AB.3) — what runs
//! when the Sheets agent uses one of the intents `alo_ai::sheets_intents`
//! describes.
//!
//! Every executor runs through the asker's account door and answers with the
//! record view Drive's own routes serve ([`crate::drive::node_json`]) — a
//! spreadsheet is a Drive node of kind `sheet`, and the agent grounds in
//! exactly what a person sees in their Drive. A read returns `{"ok": true,
//! "result": …}` into the turn; a write returns the record it changed, and
//! only ever runs from the asker's approval ([`crate::agent::execute_tool`]
//! holds that, not this module).
//!
//! **No route is adapted here, because there is none.** alo Sheets has no
//! route surface of its own: the editor loads and saves a workbook through
//! Drive's routes, and the five older tools in [`crate::agent_sheets`] reach
//! the snapshot the same way — the node, its blob, its versions. They keep
//! their executors there and are reached from here so the agent has one place
//! to look; the coverage test below holds the router to registering no
//! `/sheets` route at all, so the module's empty exclusion list stays the
//! whole story. What is new here is what AB.3 adds: the Sheets' own answer to
//! "which spreadsheets exist" (`list_spreadsheets`).

use axum::Json;
use serde_json::{Value, json};

use alo_store::{DriveLocation, DriveNode};

use crate::agent_args::string_arg;
use crate::drive::{map_err, node_json};
use crate::error::Problem;
use crate::state::{Account, AppState};

/// How many spreadsheets a list read returns — enough for a question, small
/// enough to sit inside the turn's result window.
const MAX_LISTED: i64 = 12;

pub(crate) type Reply = Result<Json<Value>, Problem>;

/// Every read's answer.
fn ok(result: Value) -> Reply {
    Ok(Json(json!({ "ok": true, "result": result })))
}

/// `list_spreadsheets` — the caller's own spreadsheets, most recently edited
/// first; one folder's spreadsheets when a folder is named. An empty list is
/// an answer ("there is no spreadsheet yet"), never a failure.
pub async fn execute_list_spreadsheets(account: &Account, args: &Value) -> Reply {
    let limit = args
        .get("limit")
        .and_then(Value::as_i64)
        .unwrap_or(MAX_LISTED)
        .clamp(1, 20);
    let (folder, spreadsheets): (Option<DriveNode>, Vec<DriveNode>) =
        match string_arg(args, "folder") {
            None => (
                None,
                account.acc.drive_sheets(limit).await.map_err(map_err)?,
            ),
            Some(wanted) => {
                let folder = crate::agent_drive::one_folder(account, &wanted).await?;
                let inside = account
                    .acc
                    .drive_list(&DriveLocation::Personal, Some(&folder.id))
                    .await
                    .map_err(map_err)?
                    .into_iter()
                    .filter(|node| node.kind == "sheet")
                    .collect();
                (Some(folder), inside)
            }
        };
    let shown: Vec<Value> = spreadsheets
        .iter()
        .take(usize::try_from(limit).unwrap_or(usize::MAX))
        .map(node_json)
        .collect();
    ok(json!({
        "kind": "sheetsList",
        "folder": folder.as_ref().map(node_json),
        "spreadsheets": shown,
        // Said plainly: what came back is a window, not the whole Drive.
        "shown": shown.len(),
        "total": spreadsheets.len(),
    }))
}

/// The module's verbs by name (A4.1c) — Sheets' one row in the agent's
/// dispatcher list, `crate::agent::MODULES`. `None` is "not mine": the
/// dispatcher then asks the next module, so two modules never need to know of
/// each other. The five older tools keep their executors in
/// [`crate::agent_sheets`] and are reached from here so the agent has one
/// place to look.
pub(crate) fn dispatch<'a>(
    _state: &'a AppState,
    account: &'a Account,
    tool: &'a str,
    args: &'a Value,
) -> Option<crate::agent::Dispatched<'a>> {
    let run: crate::agent::Dispatched<'a> = match tool {
        "list_spreadsheets" => Box::pin(execute_list_spreadsheets(account, args)),
        "sheet_read" => Box::pin(crate::agent_sheets::execute_sheet_read(account, args)),
        "sheet_answer" => Box::pin(crate::agent_sheets::execute_sheet_answer(account, args)),
        "sheet_formula_explain" => Box::pin(crate::agent_sheets::execute_sheet_formula_explain(
            account, args,
        )),
        "sheet_write_formula" => Box::pin(crate::agent_sheets::execute_sheet_write_formula(
            account, args,
        )),
        "sheet_clean_column" => Box::pin(crate::agent_sheets::execute_sheet_clean_column(
            account, args,
        )),
        _ => return None,
    };
    Some(run)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use alo_ai::sheets_intents::SHEETS;

    /// The router registers no `/sheets` route — a workbook is a Drive node
    /// and the editor saves through Drive's own routes — so the module's empty
    /// exclusion list is the whole story, and stays honest: the day a
    /// `/sheets` route lands, this test demands its verb or its reason.
    #[test]
    fn there_is_no_sheets_route_to_cover_and_no_verb_claims_one() {
        let router = include_str!("server.rs");
        assert!(
            alo_ai::routes_in(router, "/sheets").is_empty(),
            "a /sheets route exists now — give it a verb or an exclusion"
        );
        assert!(SHEETS.uncovered(router, "/sheets").is_empty());
        assert!(SHEETS.excluded.is_empty());
        for intent in SHEETS.intents {
            assert!(
                intent.routes.is_empty(),
                "{}: the agent's spreadsheets are Drive nodes, not a route's record",
                intent.name
            );
        }
    }

    #[test]
    fn every_verb_the_registry_offers_is_dispatched() {
        let dispatch = include_str!("sheets_intents.rs");
        for intent in SHEETS.intents {
            assert!(
                dispatch.contains(&format!("\"{}\" =>", intent.name)),
                "{} has no executor in the dispatch",
                intent.name
            );
        }
    }

    /// Sheets' registration is one row in each list (A4.1c): the agent's
    /// dispatcher names this module once, the registry names it once, and the
    /// two lists are the same length — every moved module has its dispatcher.
    #[test]
    fn the_module_is_one_row_in_each_list() {
        let agent = include_str!("agent.rs");
        assert_eq!(
            agent.matches("sheets_intents::").count(),
            1,
            "agent.rs names Sheets only in MODULES"
        );
        assert!(agent.contains("crate::sheets_intents::dispatch"));
        assert_eq!(
            crate::agent::MODULES.len(),
            alo_ai::MOVED.len(),
            "a moved module without a dispatcher, or the reverse"
        );
    }
}
