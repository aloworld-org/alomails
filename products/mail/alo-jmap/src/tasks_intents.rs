//! The executors of alo Tasks' verbs (ADR 0058, queue item AB.4) — what runs
//! when the Tasks agent uses one of the intents `alo_ai::tasks_intents`
//! describes.
//!
//! Every executor runs through the asker's account door, over the reach the
//! older tool executors in [`crate::agent_tasks`] established: the boards are
//! the ones [`alo_store::AccountStore::task_projects`] lists, so a colleague's
//! private board — and every other tenant's everything — is not among the
//! things that can be named. A read returns `{"ok": true, "result": …}` into
//! the turn; a write returns the record it changed, and only ever runs from
//! the asker's approval ([`crate::agent::execute_tool`] holds that, not this
//! module).
//!
//! What AB.4 adds beside the six kept tools:
//!
//! - `board_tasks` and `task_lookup` — one board's open work and one task in
//!   full, both answered with the record views the `/tasks` routes themselves
//!   serve ([`crate::tasks::task_json`], [`crate::tasks::task_record`]), so
//!   the agent grounds in exactly what a person sees on the board.
//! - `complete_task` and `reassign_task` — the same one-move and one-edit the
//!   board's own drag and edit dialog run
//!   ([`alo_store::AccountStore::move_task`],
//!   [`alo_store::AccountStore::update_task`]), no new storage path. Both
//!   resolve the task by its title among the caller's *unfinished* work, and
//!   a title that matches two comes back listing them; a handover carries
//!   every other field across unchanged, and the colleague is named out of
//!   the people already on the visible boards — never out of a directory, so
//!   a name that matches nobody says nothing about who exists.

use axum::Json;
use serde_json::{Value, json};

use alo_store::{Task, TaskEdit, UserId};

use crate::agent_args::{pick, string_arg, unprocessable};
use crate::billing::map_store_err;
use crate::error::Problem;
use crate::state::{Account, AppState};
use crate::tasks::{resolve_emails, task_json, task_record};

/// The most tasks one board read reports — the bound `agent_tasks` uses for
/// its own sweeps.
const MAX_LISTED: usize = 80;

pub(crate) type Reply = Result<Json<Value>, Problem>;

/// Every read's answer.
fn ok(result: Value) -> Reply {
    Ok(Json(json!({ "ok": true, "result": result })))
}

/// `board_tasks` — one board's open tasks, by the board's name, in the order
/// the board itself renders them (column, then position). An empty board is an
/// answer ("nothing is open on it"), never a failure.
pub async fn execute_board_tasks(state: &AppState, account: &Account, args: &Value) -> Reply {
    let wanted =
        string_arg(args, "board").ok_or_else(|| unprocessable("say which board, by its name"))?;
    let all = crate::agent_tasks::boards(account).await?;
    let names: Vec<(&str, usize)> = all
        .iter()
        .enumerate()
        .map(|(at, (_, name))| (name.as_str(), at))
        .collect();
    let at = pick(&wanted, names, "board")?;
    let (id, name) = &all[at];
    let open: Vec<Task> = account
        .acc
        .tasks_in_project(id)
        .await
        .map_err(map_store_err)?
        .into_iter()
        .filter(|task| task.status != "done")
        .collect();
    let ts = state.store.for_tenant(account.tenant.clone());
    let emails = resolve_emails(&ts, &open).await;
    let shown: Vec<Value> = open
        .iter()
        .take(MAX_LISTED)
        .map(|task| task_json(task, &emails))
        .collect();
    ok(json!({
        "kind": "boardTasks",
        "board": name,
        "open": open.len(),
        "tasks": shown,
        // Said plainly, so a cut list reads as cut rather than as the whole
        // board.
        "truncated": open.len() > MAX_LISTED,
    }))
}

/// `task_lookup` — one task in full, by its title, finished or not: the same
/// record `GET /tasks/{id}` serves, with the board's name beside it.
pub async fn execute_task_lookup(state: &AppState, account: &Account, args: &Value) -> Reply {
    let wanted =
        string_arg(args, "task").ok_or_else(|| unprocessable("say which task, by its title"))?;
    // All tasks, not only the unfinished: "where is X" is asked about done
    // work too, and the record says plainly which column it sits in.
    let mut everything: Vec<(Task, String)> = Vec::new();
    for (id, name) in crate::agent_tasks::boards(account).await? {
        for task in account
            .acc
            .tasks_in_project(&id)
            .await
            .map_err(map_store_err)?
        {
            everything.push((task, name.clone()));
        }
    }
    let candidates: Vec<(&str, usize)> = everything
        .iter()
        .enumerate()
        .map(|(at, (task, _))| (task.title.as_str(), at))
        .collect();
    let at = pick(&wanted, candidates, "task")?;
    let (task, board) = &everything[at];
    let record = task_record(state, account, task).await?;
    ok(json!({
        "kind": "taskLookup",
        "board": board,
        "record": record,
    }))
}

/// `complete_task` — one unfinished task moved to its board's done column,
/// through the same store move a board drag runs (`completed_at` set there).
/// Runs only from the asker's own approval (ADR 0047 §1).
pub async fn execute_complete_task(account: &Account, args: &Value) -> Reply {
    let (task, board) = crate::agent_tasks::resolve_task(account, args).await?;
    account
        .acc
        .move_task(&task.id, "done", task.position)
        .await
        .map_err(map_store_err)?;
    let done = account
        .acc
        .task(&task.id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(Problem::server_error)?;
    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "taskCompleted",
            "title": task.title,
            "board": board,
            "was": task.status,
            "completedAt": done.completed_at.map(crate::agent_reads::iso),
        }
    })))
}

/// `reassign_task` — one unfinished task handed to a named colleague, and
/// nothing else about it changed. Runs only from the asker's own approval.
///
/// # Errors
/// 422 when the task or the colleague was not named, when either resolves to
/// nothing or to more than one — the colleague out of the people already on
/// the boards the caller can open, so a name that matches nobody says nothing
/// about who exists in the tenant.
pub async fn execute_reassign_task(state: &AppState, account: &Account, args: &Value) -> Reply {
    let (task, board) = crate::agent_tasks::resolve_task(account, args).await?;
    let wanted = string_arg(args, "to")
        .ok_or_else(|| unprocessable("say who should have it — a name or an email address"))?;
    let ts = state.store.for_tenant(account.tenant.clone());

    let who: UserId = if wanted.trim().eq_ignore_ascii_case("me") {
        account.user.clone()
    } else if let Ok(user) = ts.user_by_email(wanted.trim()).await {
        // An exact address settles it — including a colleague with no task
        // yet, whom the board's own edit dialog could also name.
        user
    } else {
        // A first name is matched against the people already on the boards
        // the caller can open — the asker included — never a directory.
        let mut people: Vec<UserId> = vec![account.user.clone()];
        for (open, _) in crate::agent_tasks::open_tasks(account).await? {
            if let Some(assignee) = &open.assignee {
                let id = UserId::new(assignee.clone());
                if !people.contains(&id) {
                    people.push(id);
                }
            }
        }
        let addresses = ts.emails_of(&people).await.map_err(map_store_err)?;
        let candidates: Vec<(&str, UserId)> = people
            .iter()
            .filter_map(|user| {
                addresses
                    .get(user.as_str())
                    .map(|address| (address.as_str(), user.clone()))
            })
            .collect();
        pick(&wanted, candidates, "colleague on your boards")?
    };

    let from = match &task.assignee {
        Some(user) => ts
            .email_of(&UserId::new(user.clone()))
            .await
            .map_err(map_store_err)?,
        None => None,
    };
    let now = ts.email_of(&who).await.map_err(map_store_err)?;
    // Everything but the owner is carried across unchanged. An edit that took
    // its other fields from the model would let a handover quietly rewrite a
    // title or drop a due date.
    account
        .acc
        .update_task(
            &task.id,
            &TaskEdit {
                title: task.title.clone(),
                description: task.description.clone(),
                assignee: Some(who.as_str().to_owned()),
                due_at: task.due_at,
                priority: task.priority.clone(),
            },
        )
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "taskReassigned",
            "title": task.title,
            "board": board,
            // Null when nobody had it — a handover from an empty chair is
            // still a handover, and the room should say so.
            "from": from,
            "now": now,
        }
    })))
}

/// The module's verbs by name (A4.1c) — Tasks' one row in the agent's
/// dispatcher list, `crate::agent::MODULES`. `None` is "not mine": the
/// dispatcher then asks the next module, so two modules never need to know of
/// each other. The six older tools keep their executors in
/// [`crate::agent_tasks`] (and `create_task`'s beside them there) and are
/// reached from here so the agent has one place to look.
pub(crate) fn dispatch<'a>(
    state: &'a AppState,
    account: &'a Account,
    tool: &'a str,
    args: &'a Value,
) -> Option<crate::agent::Dispatched<'a>> {
    let run: crate::agent::Dispatched<'a> = match tool {
        "my_plate" => Box::pin(crate::agent_tasks::execute_my_plate(account, args)),
        "overdue_by_owner" => Box::pin(crate::agent_tasks::execute_overdue_by_owner(
            account, args, state,
        )),
        "thread_actions" => Box::pin(crate::agent_tasks::execute_thread_actions(account, args)),
        "board_tasks" => Box::pin(execute_board_tasks(state, account, args)),
        "task_lookup" => Box::pin(execute_task_lookup(state, account, args)),
        "create_task" => Box::pin(crate::agent_tasks::execute_create_task(account, args)),
        "set_task_priority" => {
            Box::pin(crate::agent_tasks::execute_set_task_priority(account, args))
        }
        "chase_task" => Box::pin(crate::agent_tasks::execute_chase_task(account, args, state)),
        "capture_actions" => Box::pin(crate::agent_tasks::execute_capture_actions(account, args)),
        "complete_task" => Box::pin(execute_complete_task(account, args)),
        "reassign_task" => Box::pin(execute_reassign_task(state, account, args)),
        _ => return None,
    };
    Some(run)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use alo_ai::tasks_intents::TASKS;

    /// Every `/tasks` route the router registers is the verb behind an intent
    /// or excluded with a reason — the coverage ADR 0058 makes structural.
    #[test]
    fn every_tasks_route_is_a_verb_or_an_exclusion() {
        let router = include_str!("server.rs");
        let missing = TASKS.uncovered(router, "/tasks");
        assert!(
            missing.is_empty(),
            "routes with neither a verb nor a reason: {missing:?}"
        );
        // …and every named route exists, so an intent cannot claim — and an
        // exclusion cannot excuse — a route the app does not have.
        let routes = alo_ai::routes_in(router, "/tasks");
        for intent in TASKS.intents {
            for route in intent.routes {
                assert!(
                    routes.contains(&(*route).to_owned()),
                    "{}: {route} is not a route",
                    intent.name
                );
            }
        }
        for excluded in TASKS.excluded {
            assert!(
                routes.contains(&excluded.route.to_owned()),
                "{} is excused but not registered",
                excluded.route
            );
        }
    }

    #[test]
    fn every_verb_the_registry_offers_is_dispatched() {
        let dispatch = include_str!("tasks_intents.rs");
        for intent in TASKS.intents {
            assert!(
                dispatch.contains(&format!("\"{}\" =>", intent.name)),
                "{} has no executor in the dispatch",
                intent.name
            );
        }
    }

    /// Tasks' registration is one row in each list (A4.1c): the agent's
    /// dispatcher names this module once, the registry names it once, and the
    /// two lists are the same length — every moved module has its dispatcher.
    #[test]
    fn the_module_is_one_row_in_each_list() {
        let agent = include_str!("agent.rs");
        assert_eq!(
            agent.matches("tasks_intents::").count(),
            1,
            "agent.rs names Tasks only in MODULES"
        );
        assert!(agent.contains("crate::tasks_intents::dispatch"));
        assert_eq!(
            crate::agent::MODULES.len(),
            alo_ai::MOVED.len(),
            "a moved module without a dispatcher, or the reverse"
        );
    }
}
