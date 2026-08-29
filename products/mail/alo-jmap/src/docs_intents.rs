//! The executors of alo Docs' verbs (ADR 0058, queue item AB.2) — what runs
//! when the Docs agent uses one of the intents `alo_ai::docs_intents`
//! describes.
//!
//! Every executor runs through the asker's account door and answers with the
//! record view Drive's own routes serve ([`crate::drive::node_json`]) — a
//! document is a Drive node of kind `doc`, and the agent grounds in exactly
//! what a person sees in their Drive. A read returns `{"ok": true,
//! "result": …}` into the turn; a write returns the record it changed, and
//! only ever runs from the asker's approval ([`crate::agent::execute_tool`]
//! holds that, not this module).
//!
//! **No `/docs/` route is adapted here, and that is the design.** Those
//! routes serve the standalone technical-authoring surface (ADR 0015), a
//! different record with its own screens; the module's coverage test below
//! holds every one of them excluded with its reason. The documents the agent
//! works in are reached the way the Docs editor itself reaches them — the
//! node, its blob, its versions — which is what the four older tools in
//! [`crate::agent_docs`] already do; they keep their executors there and are
//! reached from here so the agent has one place to look. What is new here is
//! what AB.2 added: the Docs' own answer to "which documents exist"
//! (`list_documents`) and the one write that starts a document without
//! putting a word in it (`create_document`).

use axum::Json;
use bytes::Bytes;
use serde_json::{Value, json};

use alo_store::{DriveLocation, DriveNode, NewDriveFile};

use crate::agent_args::{string_arg, unprocessable};
use crate::drive::{map_err, node_json};
use crate::error::Problem;
use crate::state::{Account, AppState};

/// How many documents a list read returns — enough for a question, small
/// enough to sit inside the turn's result window.
const MAX_LISTED: i64 = 12;

pub(crate) type Reply = Result<Json<Value>, Problem>;

/// Every read's answer.
fn ok(result: Value) -> Reply {
    Ok(Json(json!({ "ok": true, "result": result })))
}

/// `list_documents` — the caller's own documents, most recently edited first;
/// one folder's documents when a folder is named. An empty list is an answer
/// ("there is no document yet"), never a failure.
pub async fn execute_list_documents(account: &Account, args: &Value) -> Reply {
    let limit = args
        .get("limit")
        .and_then(Value::as_i64)
        .unwrap_or(MAX_LISTED)
        .clamp(1, 20);
    let (folder, documents): (Option<DriveNode>, Vec<DriveNode>) = match string_arg(args, "folder")
    {
        None => (None, account.acc.drive_docs(limit).await.map_err(map_err)?),
        Some(wanted) => {
            let folder = crate::agent_drive::one_folder(account, &wanted).await?;
            let inside = account
                .acc
                .drive_list(&DriveLocation::Personal, Some(&folder.id))
                .await
                .map_err(map_err)?
                .into_iter()
                .filter(|node| node.kind == "doc")
                .collect();
            (Some(folder), inside)
        }
    };
    let shown: Vec<Value> = documents
        .iter()
        .take(usize::try_from(limit).unwrap_or(usize::MAX))
        .map(node_json)
        .collect();
    ok(json!({
        "kind": "docsList",
        "folder": folder.as_ref().map(node_json),
        "documents": shown,
        // Said plainly: what came back is a window, not the whole Drive.
        "shown": shown.len(),
        "total": documents.len(),
    }))
}

/// `create_document` — a write, run on the asker's approval: a new, empty
/// document in the caller's OWN Drive, made the way the editor's own save
/// path stores one — an empty block array as its blob, a `doc` node pointing
/// at it. The destination is checked against the real Drive before anything
/// is written: a folder that is not there is refused with the folders that
/// are, and a sibling of the same name is refused by name rather than made
/// unique.
pub async fn execute_create_document(account: &Account, args: &Value) -> Reply {
    let wanted = string_arg(args, "title")
        .ok_or_else(|| unprocessable("say what the document should be called"))?;
    let title = crate::agent_drive::checked_name(&wanted)?;
    let parent: Option<DriveNode> = match string_arg(args, "folder") {
        None => None,
        Some(wanted) => Some(crate::agent_drive::one_folder(account, &wanted).await?),
    };
    let parent_id = parent.as_ref().map(|folder| folder.id.clone());
    if let Some(clash) = crate::agent_drive::named_in(account, parent_id.as_ref(), &title).await? {
        return Err(unprocessable(format!(
            "there is already a {} in {}",
            clash.name,
            parent
                .as_ref()
                .map_or("the top level of your drive", |f| f.name.as_str())
        )));
    }
    // The editor reads a missing or empty blob as an empty document; storing
    // the empty array outright keeps the node identical to one the editor
    // saved and never opened.
    let bytes: &[u8] = b"[]";
    let blob = account
        .acc
        .put_blob(Bytes::from_static(bytes), Some("application/json"))
        .await
        .map_err(map_err)?;
    let id = account
        .acc
        .drive_create_file(
            &DriveLocation::Personal,
            parent_id.as_ref(),
            &NewDriveFile {
                name: title,
                blob_id: blob.as_str().to_owned(),
                size: i64::try_from(bytes.len()).unwrap_or(0),
                content_type: Some("application/json".to_owned()),
                kind: Some("doc".to_owned()),
                ..NewDriveFile::default()
            },
        )
        .await
        .map_err(map_err)?;
    let record = crate::drive_intents::node_record(account, &id).await?;
    ok(json!({
        "kind": "docCreated",
        "document": record,
        "parent": parent.as_ref().map(|f| json!({ "id": f.id.as_str(), "name": f.name })),
        "changed": true,
    }))
}

/// The module's verbs by name (A4.1c) — Docs' one row in the agent's
/// dispatcher list, `crate::agent::MODULES`. `None` is "not mine": the
/// dispatcher then asks the next module, so two modules never need to know of
/// each other. The four older tools keep their executors in
/// [`crate::agent_docs`] and are reached from here so the agent has one place
/// to look.
pub(crate) fn dispatch<'a>(
    _state: &'a AppState,
    account: &'a Account,
    tool: &'a str,
    args: &'a Value,
) -> Option<crate::agent::Dispatched<'a>> {
    let run: crate::agent::Dispatched<'a> = match tool {
        "list_documents" => Box::pin(execute_list_documents(account, args)),
        "doc_read" => Box::pin(crate::agent_docs::execute_doc_read(account, args)),
        "doc_answer" => Box::pin(crate::agent_docs::execute_doc_answer(account, args)),
        "create_document" => Box::pin(execute_create_document(account, args)),
        "doc_draft_section" => {
            Box::pin(crate::agent_docs::execute_doc_draft_section(account, args))
        }
        "doc_rewrite" => Box::pin(crate::agent_docs::execute_doc_rewrite(account, args)),
        _ => return None,
    };
    Some(run)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use alo_ai::docs_intents::DOCS;

    /// Every `/docs` route the router registers is excluded with a reason —
    /// the standalone authoring surface (ADR 0015) is not the agent's — and
    /// no verb claims a route of it, so the exclusions are the whole story.
    #[test]
    fn every_docs_route_is_excluded_and_no_verb_claims_one() {
        let router = include_str!("server.rs");
        let missing = DOCS.uncovered(router, "/docs");
        assert!(
            missing.is_empty(),
            "routes with neither a verb nor a reason: {missing:?}"
        );
        // The surface is real: the exclusions name routes the app has, so a
        // renamed route cannot leave a stale reason behind.
        let routes = alo_ai::routes_in(router, "/docs");
        for excluded in DOCS.excluded {
            assert!(
                routes.contains(&excluded.route.to_owned()),
                "{} is excluded but is not a route",
                excluded.route
            );
        }
        for intent in DOCS.intents {
            assert!(
                intent.routes.is_empty(),
                "{}: the agent's documents are Drive nodes, not the ADR 0015 record",
                intent.name
            );
        }
    }

    #[test]
    fn every_verb_the_registry_offers_is_dispatched() {
        let dispatch = include_str!("docs_intents.rs");
        for intent in DOCS.intents {
            assert!(
                dispatch.contains(&format!("\"{}\" =>", intent.name)),
                "{} has no executor in the dispatch",
                intent.name
            );
        }
    }

    /// Docs' registration is one row in each list (A4.1c): the agent's
    /// dispatcher names this module once, the registry names it once, and the
    /// two lists are the same length — every moved module has its dispatcher.
    #[test]
    fn the_module_is_one_row_in_each_list() {
        let agent = include_str!("agent.rs");
        assert_eq!(
            agent.matches("docs_intents::").count(),
            1,
            "agent.rs names Docs only in MODULES"
        );
        assert!(agent.contains("crate::docs_intents::dispatch"));
        assert_eq!(
            crate::agent::MODULES.len(),
            alo_ai::MOVED.len(),
            "a moved module without a dispatcher, or the reverse"
        );
    }
}
