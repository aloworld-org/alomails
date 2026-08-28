//! The executors of alo Drive's verbs (ADR 0058, queue item AB.1) — what runs
//! when the Drive agent uses one of the intents `alo_ai::drive_intents`
//! describes.
//!
//! Every executor runs through the asker's account door and answers with the
//! **record view** Drive's own routes serve ([`crate::drive::node_json`]), so
//! an agent grounds in exactly what a person sees in their Drive — there is no
//! second summary of a file. A read returns `{"ok": true, "result": …}` into
//! the turn; a write returns the record it changed, and only ever runs from
//! the asker's approval ([`crate::agent::execute_tool`] holds that, not this
//! module).
//!
//! **A route is an adapter of the same verb** (ADR 0058, A4.1b). The shared
//! cores below do the verb's work on resolved inputs, and both callers run
//! them: the `/drive/` handler with the id from its path, the executor after
//! resolving a name. The coverage test at the bottom asserts the call in each
//! handler's source, so a route and its verb cannot quietly drift apart.
//!
//! The five older tools keep their executors where they were — the file ones
//! in [`crate::agent_drive`], `attachment_read` in
//! [`crate::agent_attachments`] — and are reached from here so the agent has
//! one place to look. What is new here is what AB.1 added: the Drive's own
//! answer to "which files do we have" (`recent_files`), one folder's contents
//! (`list_folder`), the Spaces as "shared with me" (`shared_with_me`), and the
//! one write that grows the tree without touching a file (`create_folder`).

use axum::Json;
use serde_json::{Value, json};

use alo_store::{DriveLocation, DriveNode, DriveNodeId};

use crate::agent_args::{string_arg, unprocessable};
use crate::drive::{map_err, node_json};
use crate::error::Problem;
use crate::state::{Account, AppState};

/// How many nodes a list read returns — enough for a question, small enough
/// to sit inside the turn's result window.
const MAX_LISTED: usize = 12;

/// How many Spaces `shared_with_me` walks. A tenant with more has a bigger
/// question than one tool answer, and the count says so honestly.
const MAX_SPACES: usize = 8;

pub(crate) type Reply = Result<Json<Value>, Problem>;

/// Every read's answer.
fn ok(result: Value) -> Reply {
    Ok(Json(json!({ "ok": true, "result": result })))
}

// ---- the shared cores: what a route and its verb both run ----------------
//
// Each returns exactly what its route answers today, so the handler stays a
// thin adapter and the executor builds its agent-facing extras on top.

/// The node list `GET /drive/list` serves: a folder's live contents in a
/// location the caller can read (the location root on `None`), folders first.
pub(crate) async fn node_list(
    account: &Account,
    loc: &DriveLocation,
    parent: Option<&DriveNodeId>,
) -> Result<Vec<Value>, Problem> {
    Ok(account
        .acc
        .drive_list(loc, parent)
        .await
        .map_err(map_err)?
        .iter()
        .map(node_json)
        .collect())
}

/// The single node `GET /drive/nodes/{id}` serves.
pub(crate) async fn node_record(account: &Account, id: &DriveNodeId) -> Result<Value, Problem> {
    let node = account
        .acc
        .drive_node(id)
        .await
        .map_err(map_err)?
        .ok_or_else(|| Problem::with(axum::http::StatusCode::NOT_FOUND, "no such node"))?;
    Ok(node_json(&node))
}

/// `POST /drive/folders`' act: a new folder in a location the caller can
/// write. Answers the new folder's id.
pub(crate) async fn create_folder(
    account: &Account,
    loc: &DriveLocation,
    parent: Option<&DriveNodeId>,
    name: &str,
) -> Result<DriveNodeId, Problem> {
    account
        .acc
        .drive_create_folder(loc, parent, name)
        .await
        .map_err(map_err)
}

/// `PUT /drive/nodes/{id}`'s act: the node's new name.
pub(crate) async fn rename_node(
    account: &Account,
    id: &DriveNodeId,
    name: &str,
) -> Result<(), Problem> {
    account.acc.drive_rename(id, name).await.map_err(map_err)
}

/// `POST /drive/nodes/{id}/move`'s act: the node into another folder —
/// re-scoping access when the location changes (ADR 0027), which is why the
/// executor only ever passes [`DriveLocation::Personal`].
pub(crate) async fn move_node(
    account: &Account,
    id: &DriveNodeId,
    loc: &DriveLocation,
    parent: Option<&DriveNodeId>,
) -> Result<(), Problem> {
    account
        .acc
        .drive_move(id, loc, parent)
        .await
        .map_err(map_err)
}

// ---- the executors on top of the cores -----------------------------------

/// `recent_files` — the caller's own files, newest work first.
pub async fn execute_recent_files(account: &Account, args: &Value) -> Reply {
    let limit = args
        .get("limit")
        .and_then(Value::as_i64)
        .unwrap_or(MAX_LISTED as i64)
        .clamp(1, 20);
    let files = account.acc.drive_recent(limit).await.map_err(map_err)?;
    ok(json!({
        "kind": "driveRecentFiles",
        "files": files.iter().map(node_json).collect::<Vec<_>>(),
        // Said plainly: what came back is a window, not the whole Drive.
        "shown": files.len(),
        "limit": limit,
    }))
}

/// `list_folder` — one folder of the caller's own Drive, by name; the top
/// level when none is named.
pub async fn execute_list_folder(account: &Account, args: &Value) -> Reply {
    let folder: Option<DriveNode> = match string_arg(args, "folder") {
        None => None,
        Some(wanted) => Some(crate::agent_drive::one_folder(account, &wanted).await?),
    };
    let nodes = node_list(
        account,
        &DriveLocation::Personal,
        folder.as_ref().map(|f| &f.id),
    )
    .await?;
    let listed: Vec<Value> = nodes.iter().take(MAX_LISTED).cloned().collect();
    ok(json!({
        "kind": "driveFolder",
        "folder": folder.as_ref().map(node_json),
        "nodes": listed,
        "nodeCount": nodes.len(),
    }))
}

/// `shared_with_me` — the Spaces the caller belongs to, each with what its
/// files area holds. A Space is how files are shared in alo, so no Spaces is
/// an answer ("nothing is shared with you"), never a failure.
pub async fn execute_shared_with_me(account: &Account, args: &Value) -> Reply {
    let all: Vec<alo_store::Space> = account
        .acc
        .spaces()
        .await
        .map_err(map_err)?
        .into_iter()
        .filter(|space| !space.archived)
        .collect();
    let wanted = string_arg(args, "space");
    let spaces: Vec<&alo_store::Space> = match &wanted {
        None => all.iter().collect(),
        Some(name) => {
            let name = name.trim().to_lowercase();
            let matching: Vec<&alo_store::Space> = all
                .iter()
                .filter(|space| space.name.to_lowercase().contains(&name))
                .collect();
            if matching.is_empty() {
                return Err(unprocessable(format!(
                    "you are in no space called \"{}\"{}",
                    wanted.as_deref().unwrap_or_default().trim(),
                    if all.is_empty() {
                        String::new()
                    } else {
                        format!(
                            " — you have: {}",
                            all.iter()
                                .map(|space| space.name.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    }
                )));
            }
            matching
        }
    };
    let mut entries: Vec<Value> = Vec::new();
    for space in spaces.iter().take(MAX_SPACES) {
        let nodes = node_list(account, &DriveLocation::Space(space.id.clone()), None).await?;
        entries.push(json!({
            "id": space.id.as_str(),
            "name": space.name,
            "myRole": space.my_role.as_str(),
            "nodes": nodes.iter().take(MAX_LISTED).cloned().collect::<Vec<_>>(),
            "nodeCount": nodes.len(),
        }));
    }
    ok(json!({
        "kind": "driveShared",
        "spaces": entries,
        "spaceCount": spaces.len(),
    }))
}

/// `create_folder` — a write, run on the asker's approval: a new folder in the
/// caller's OWN Drive. The destination is checked against the real Drive
/// before anything is written: a parent that is not there is refused with the
/// folders that are, and a sibling of the same name is refused by name rather
/// than made unique.
pub async fn execute_create_folder(account: &Account, args: &Value) -> Reply {
    let wanted = string_arg(args, "name")
        .ok_or_else(|| unprocessable("say what the folder should be called"))?;
    let name = crate::agent_drive::checked_name(&wanted)?;
    let parent: Option<DriveNode> = match string_arg(args, "folder") {
        None => None,
        Some(wanted) => Some(crate::agent_drive::one_folder(account, &wanted).await?),
    };
    let parent_id = parent.as_ref().map(|folder| folder.id.clone());
    if let Some(clash) = crate::agent_drive::named_in(account, parent_id.as_ref(), &name).await? {
        return Err(unprocessable(format!(
            "there is already a {} in {}",
            clash.name,
            parent
                .as_ref()
                .map_or("the top level of your drive", |f| f.name.as_str())
        )));
    }
    let id = create_folder(account, &DriveLocation::Personal, parent_id.as_ref(), &name).await?;
    let record = node_record(account, &id).await?;
    ok(json!({
        "kind": "driveFolderCreated",
        "folder": record,
        "parent": parent.as_ref().map(|f| json!({ "id": f.id.as_str(), "name": f.name })),
        "changed": true,
    }))
}

/// The module's verbs by name (A4.1c) — Drive's one row in the agent's
/// dispatcher list, `crate::agent::MODULES`. `None` is "not mine": the
/// dispatcher then asks the next module, so two modules never need to know of
/// each other. The five older tools keep their executors in
/// [`crate::agent_drive`] and [`crate::agent_attachments`], and are reached
/// from here so the agent has one place to look.
pub(crate) fn dispatch<'a>(
    _state: &'a AppState,
    account: &'a Account,
    tool: &'a str,
    args: &'a Value,
) -> Option<crate::agent::Dispatched<'a>> {
    let run: crate::agent::Dispatched<'a> = match tool {
        "recent_files" => Box::pin(execute_recent_files(account, args)),
        "list_folder" => Box::pin(execute_list_folder(account, args)),
        "shared_with_me" => Box::pin(execute_shared_with_me(account, args)),
        "find_file" => Box::pin(crate::agent_drive::execute_find_file(account, args)),
        "file_read" => Box::pin(crate::agent_drive::execute_file_read(account, args)),
        "attachment_read" => Box::pin(crate::agent_attachments::execute_attachment_read(
            account, args,
        )),
        "create_folder" => Box::pin(execute_create_folder(account, args)),
        "file_rename" => Box::pin(crate::agent_drive::execute_file_rename(account, args)),
        "file_move" => Box::pin(crate::agent_drive::execute_file_move(account, args)),
        _ => return None,
    };
    Some(run)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use alo_ai::drive_intents::DRIVE;

    /// Every `/drive/` route the router registers is the adapter of a verb or
    /// excluded with a reason — the coverage ADR 0058 makes structural.
    #[test]
    fn every_drive_route_is_a_verb_or_an_exclusion() {
        let router = include_str!("server.rs");
        let missing = DRIVE.uncovered(router, "/drive/");
        assert!(
            missing.is_empty(),
            "routes with neither a verb nor a reason: {missing:?}"
        );
        // …and every verb's route exists, so an intent cannot name a route the
        // app does not have.
        let routes = alo_ai::routes_in(router, "/drive/");
        for intent in DRIVE.intents {
            for route in intent.routes {
                assert!(
                    routes.contains(&(*route).to_owned()),
                    "{}: {route} is not a route",
                    intent.name
                );
            }
        }
    }

    #[test]
    fn every_verb_the_registry_offers_is_dispatched() {
        let dispatch = include_str!("drive_intents.rs");
        for intent in DRIVE.intents {
            assert!(
                dispatch.contains(&format!("\"{}\" =>", intent.name)),
                "{} has no executor in the dispatch",
                intent.name
            );
        }
    }

    /// Drive's registration is one row in each list (A4.1c): the agent's
    /// dispatcher names this module once, the registry names it once, and the
    /// two lists are the same length — every moved module has its dispatcher.
    #[test]
    fn the_module_is_one_row_in_each_list() {
        let agent = include_str!("agent.rs");
        assert_eq!(
            agent.matches("drive_intents::").count(),
            1,
            "agent.rs names Drive only in MODULES"
        );
        assert!(agent.contains("crate::drive_intents::dispatch"));
        assert_eq!(
            crate::agent::MODULES.len(),
            alo_ai::MOVED.len(),
            "a moved module without a dispatcher, or the reverse"
        );
    }

    /// The call each verb's route handlers must contain (A4.1b): the shared
    /// core the verb's executor runs, qualified so a store function whose name
    /// merely ends the same way cannot satisfy it.
    const SHARED_CORES: &[(&str, &str)] = &[
        ("recent_files", "drive_intents::node_list("),
        ("list_folder", "drive_intents::node_list("),
        ("shared_with_me", "drive_intents::node_list("),
        ("find_file", "drive_intents::node_list("),
        ("file_read", "drive_intents::node_record("),
        ("create_folder", "drive_intents::create_folder("),
        ("file_rename", "drive_intents::rename_node("),
        ("file_move", "drive_intents::move_node("),
    ];

    /// The `module::handler` pairs a router source registers on `route`.
    fn handlers_of(router: &str, route: &str) -> Vec<(String, String)> {
        let literal = format!("\"{route}\"");
        let mut found = Vec::new();
        let mut rest = router;
        while let Some(at) = rest.find(&literal) {
            let after = &rest[at + literal.len()..];
            let segment = &after[..after.find(".route").unwrap_or(after.len())];
            for token in segment.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == ':')) {
                if let Some((module, handler)) = token.split_once("::")
                    && !module.is_empty()
                    && !handler.is_empty()
                    && !handler.contains(':')
                {
                    found.push((module.to_owned(), handler.to_owned()));
                }
            }
            rest = after;
        }
        found
    }

    /// The body of `async fn name(…)` in `source` — the signature excluded, so
    /// a handler whose own name contains the core's cannot match itself.
    fn body_of<'a>(source: &'a str, name: &str) -> &'a str {
        let Some(start) = source.find(&format!("async fn {name}(")) else {
            return "";
        };
        let after = &source[start..];
        let Some(open) = after.find('{') else {
            return "";
        };
        let body = &after[open + 1..];
        &body[..body.find("\npub async fn ").unwrap_or(body.len())]
    }

    /// A4.1b's claim, asserted structurally: for every verb that adapts a
    /// route, at least one handler registered on it **calls** the shared core
    /// the executor runs — the call, not just the route's name in a list.
    /// (`attachment_read` adapts none: it reads mail, not a `/drive/` route.)
    #[test]
    fn every_verbs_route_handler_calls_the_executors_core() {
        let server = include_str!("server.rs");
        let drive = include_str!("drive.rs");
        for intent in DRIVE.intents {
            if intent.routes.is_empty() {
                continue;
            }
            let (_, call) = SHARED_CORES
                .iter()
                .find(|(verb, _)| *verb == intent.name)
                .unwrap_or_else(|| panic!("{} names no shared core", intent.name));
            let handlers: Vec<(String, String)> = intent
                .routes
                .iter()
                .flat_map(|route| handlers_of(server, route))
                .collect();
            assert!(
                !handlers.is_empty(),
                "{}: no handler registered on {:?}",
                intent.name,
                intent.routes
            );
            let adapted = handlers.iter().any(|(module, handler)| {
                module == "drive" && body_of(drive, handler).contains(call)
            });
            assert!(
                adapted,
                "{}: none of {:?} calls {call}…) — the route and the verb have drifted apart",
                intent.name, handlers
            );
        }
    }
}
