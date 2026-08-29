//! Executing the **Tasks** agent's kept tools (ADR 0034, queue item A2.7) —
//! adding to somebody's list, what is on their plate, who is late, and the
//! two ways a conversation turns into work.
//!
//! Its own file rather than more functions in [`crate::agent`], because these
//! are a product's tools rather than the assistant's: `create_task` sat in
//! the dispatcher's own module from before agents had products until AB.4
//! moved it home, and everything A2.7 added is here. The verbs AB.4 adds —
//! one board's open work, one task in full, completing and handing over —
//! execute in [`crate::tasks_intents`], which is also the module's one
//! dispatch row; the helpers they share (the board reach, the title
//! resolver) are this file's.
//!
//! Five rules hold the module together, and each is a way the obvious
//! implementation would be wrong:
//!
//! - **Overdue means due before today, everywhere.** Not "due before now" —
//!   that makes a task due today late at 00:01 and turns every morning into a
//!   chase. The same rule decides the `overdue` bucket of a plate, the groups
//!   of [`execute_overdue_by_owner`], and whether [`execute_chase_task`] will
//!   chase at all.
//! - **A plate is what is unfinished, not what is dated.** The tasks with no
//!   due date are the ones a due-date-shaped query silently loses, and they are
//!   usually the ones nobody has looked at. They come back in their own bucket.
//! - **Mine is not the same as assigned to me.** A task the agent itself made
//!   lands on the caller's personal board with no assignee at all
//!   (`create_task` sets none), so a plate that filtered on assignee would hide
//!   exactly the tasks this agent created. Ownership here is *assigned to me*
//!   **or** *unassigned on my own board* — and the assigned half is read
//!   through [`alo_store::AccountStore::my_plate`], which reaches a task
//!   somebody assigned to the caller on a board the caller cannot open.
//! - **You can chase somebody only about work you can already see.** Every read
//!   and every write here runs on the asker's own account door: the boards are
//!   the ones [`alo_store::AccountStore::task_projects`] lists (theirs and the
//!   team's), the comment goes on through
//!   [`alo_store::AccountStore::add_task_comment`], which refuses an invisible
//!   task, and a colleague is named out of the assignees already on those
//!   boards rather than out of a directory.
//! - **Writing down what a room agreed is proposed twice.** Approving
//!   `capture_actions` writes `state = 'proposed'` rows (ADR 0023), so each
//!   action is still accepted or rejected one at a time in the task list — and
//!   the room they came out of is recorded on every row, so `thread_actions`
//!   can say what has already been captured and the same commitment is not
//!   written down twice.

use std::collections::HashMap;

use axum::Json;
use serde_json::{Value, json};
use time::{Date, Duration, OffsetDateTime, Time};

use alo_store::{ChatChannelId, NewTask, ProjectId, Task, TaskEdit, UserId};

use crate::agent_args::{pick, string_arg, unprocessable};
use crate::agent_reads::{iso, room_named};
use crate::billing::{map_store_err, parse_iso_date};
use crate::error::Problem;
use crate::state::{Account, AppState};

/// How far ahead "coming up" reaches when the model does not say, and the most
/// it may ask for. Beyond that a plate stops being a plate.
const DEFAULT_HORIZON_DAYS: i64 = 14;
const MAX_HORIZON_DAYS: i64 = 90;

/// The most boards one answer sweeps, and the most tasks it reports out of
/// them. A workspace with two hundred boards is a real thing; a model's context
/// is not.
const MAX_BOARDS: usize = 25;
const MAX_TASKS: usize = 80;

/// The most actions one approval may write down, and the longest chase.
const MAX_CAPTURED: usize = 10;
const MAX_COMMENT_CHARS: usize = 2_000;

/// The priorities a task may carry — the same four the board offers.
const PRIORITIES: [&str; 4] = ["none", "low", "medium", "high"];

/// Midnight this morning, UTC: the instant that separates *late* from *due
/// today* everywhere in this module.
fn today_start(today: Date) -> OffsetDateTime {
    today.with_time(Time::MIDNIGHT).assume_utc()
}

/// One task and the board it sits on, as the model reads it.
///
/// No id: a task has no identifier the person asking ever saw, and the two
/// writes here take the title they used instead. A board the caller cannot open
/// is `null` rather than absent, so a task assigned to them on somebody's
/// private list is reported as theirs without naming where it lives.
fn task_json(task: &Task, board: Option<&str>, today: Date) -> Value {
    let mut out = json!({
        "title": task.title,
        "board": board,
        "column": task.status,
        "priority": task.priority,
        "due": task.due_at.map(iso),
        "checklist": format!("{}/{}", task.subtask_done, task.subtask_total),
    });
    if let Some(days) = days_late(task, today) {
        out["daysLate"] = json!(days);
    }
    out
}

/// How many whole days a task is past its due date, or `None` when it is not
/// late. Days rather than hours, because that is the unit a chase is written in.
fn days_late(task: &Task, today: Date) -> Option<i64> {
    let due = task.due_at?;
    (due < today_start(today)).then(|| (today_start(today) - due).whole_days().max(1))
}

/// Every board the caller can open, newest-first-bounded, as `(id, name)` —
/// the reach this module and the intent executors in
/// [`crate::tasks_intents`] share.
pub(crate) async fn boards(account: &Account) -> Result<Vec<(ProjectId, String)>, Problem> {
    Ok(account
        .acc
        .task_projects()
        .await
        .map_err(map_store_err)?
        .into_iter()
        .take(MAX_BOARDS)
        .map(|project| (project.id, project.name))
        .collect())
}

/// Every unfinished task on the boards the caller can open, each with its
/// board's name — the reach both reads and both writes share.
///
/// "Unfinished" is the `done` column excluded, not `completed_at`: a task
/// dragged out of `done` is open again, and the timestamp survives the drag.
pub(crate) async fn open_tasks(account: &Account) -> Result<Vec<(Task, String)>, Problem> {
    let mut out = Vec::new();
    for (id, name) in boards(account).await? {
        let tasks = account
            .acc
            .tasks_in_project(&id)
            .await
            .map_err(map_store_err)?;
        for task in tasks.into_iter().filter(|task| task.status != "done") {
            out.push((task, name.clone()));
        }
    }
    Ok(out)
}

/// `my_plate` — the caller's own unfinished work, in the order a day is read.
///
/// # Errors
/// 422 when `days` is not a whole number of days; the store's own failure
/// otherwise.
pub async fn execute_my_plate(account: &Account, args: &Value) -> Result<Json<Value>, Problem> {
    let days = match args.get("days") {
        None | Some(Value::Null) => DEFAULT_HORIZON_DAYS,
        Some(Value::Number(number)) => number
            .as_i64()
            .ok_or_else(|| unprocessable("days must be a whole number of days"))?
            .clamp(1, MAX_HORIZON_DAYS),
        Some(_) => return Err(unprocessable("days must be a whole number of days")),
    };
    let today = OffsetDateTime::now_utc().date();
    let horizon = (today + Duration::days(days + 1))
        .with_time(Time::MIDNIGHT)
        .assume_utc();

    // Two reaches, unioned: the boards the caller can open, and — for the dated
    // half — every task assigned to them wherever it lives, including a board
    // they cannot open at all.
    let mine_personal = account
        .acc
        .ensure_personal_project()
        .await
        .map_err(map_store_err)?;
    let mut named: HashMap<String, String> = HashMap::new();
    let mut plate: Vec<Task> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for (task, board) in open_tasks(account).await? {
        let mine = task.assignee.as_deref() == Some(account.user.as_str())
            || (task.assignee.is_none() && task.project_id == mine_personal);
        if !mine {
            continue;
        }
        named.insert(task.id.as_str().to_owned(), board);
        seen.push(task.id.as_str().to_owned());
        plate.push(task);
    }
    for task in account
        .acc
        .my_plate(horizon)
        .await
        .map_err(map_store_err)?
        .into_iter()
        .filter(|task| task.status != "done")
    {
        if !seen.iter().any(|id| id == task.id.as_str()) {
            seen.push(task.id.as_str().to_owned());
            plate.push(task);
        }
    }

    let start = today_start(today);
    let tomorrow = start + Duration::days(1);
    let mut overdue = Vec::new();
    let mut due_today = Vec::new();
    let mut coming_up = Vec::new();
    let mut later = Vec::new();
    let mut no_date = Vec::new();
    plate.sort_by_key(|task| (task.due_at, task.title.clone()));
    for task in plate.iter().take(MAX_TASKS) {
        let board = named.get(task.id.as_str()).map(String::as_str);
        let entry = task_json(task, board, today);
        match task.due_at {
            None => no_date.push(entry),
            Some(due) if due < start => overdue.push(entry),
            Some(due) if due < tomorrow => due_today.push(entry),
            Some(due) if due < horizon => coming_up.push(entry),
            Some(_) => later.push(entry),
        }
    }

    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "myPlate",
            "today": today.to_string(),
            "horizonDays": days,
            "overdue": overdue,
            "dueToday": due_today,
            "comingUp": coming_up,
            "later": later,
            "noDate": no_date,
            // Said plainly, so a plate that was cut reads as cut rather than as
            // the whole of somebody's work.
            "truncated": plate.len() > MAX_TASKS,
        }
    })))
}

/// `overdue_by_owner` — who is late, out of the boards the caller can open.
///
/// # Errors
/// 422 when the named board or the named colleague resolves to nothing, or to
/// more than one thing; the store's own failure otherwise.
pub async fn execute_overdue_by_owner(
    account: &Account,
    args: &Value,
    state: &AppState,
) -> Result<Json<Value>, Problem> {
    let today = OffsetDateTime::now_utc().date();
    let all = boards(account).await?;
    let boards_read = all.len();
    let mut tasks: Vec<(Task, String)> = open_tasks(account)
        .await?
        .into_iter()
        .filter(|(task, _)| days_late(task, today).is_some())
        .collect();

    if let Some(wanted) = string_arg(args, "project") {
        let names: Vec<(&str, String)> = all
            .iter()
            .map(|(_, name)| (name.as_str(), name.clone()))
            .collect();
        let board = pick(&wanted, names, "board")?;
        tasks.retain(|(_, name)| *name == board);
    }

    // The people who can be named are the assignees already on those boards —
    // never a directory, so a name that matches nobody says nothing about who
    // exists (the rule `find_a_time` follows for diaries).
    let owners: Vec<UserId> = {
        let mut out: Vec<UserId> = Vec::new();
        for (task, _) in &tasks {
            if let Some(assignee) = &task.assignee {
                let id = UserId::new(assignee.clone());
                if !out.contains(&id) {
                    out.push(id);
                }
            }
        }
        out
    };
    let addresses = state
        .store
        .for_tenant(account.tenant.clone())
        .emails_of(&owners)
        .await
        .map_err(map_store_err)?;
    let label = |user: &str| {
        addresses
            .get(user)
            .cloned()
            .unwrap_or_else(|| user.to_owned())
    };

    if let Some(wanted) = string_arg(args, "person") {
        let candidates: Vec<(&str, String)> = owners
            .iter()
            .map(|user| {
                (
                    addresses.get(user.as_str()).map_or("", String::as_str),
                    user,
                )
            })
            .filter(|(address, _)| !address.is_empty())
            .map(|(address, user)| (address, user.as_str().to_owned()))
            .collect();
        let who = pick(&wanted, candidates, "colleague with late work")?;
        tasks.retain(|(task, _)| task.assignee.as_deref() == Some(who.as_str()));
    }

    tasks.sort_by_key(|(task, _)| (task.assignee.clone(), task.due_at));
    let mut people: Vec<Value> = Vec::new();
    let mut order: Vec<Option<String>> = Vec::new();
    for (task, board) in tasks.iter().take(MAX_TASKS) {
        let key = task.assignee.clone();
        let entry = task_json(task, Some(board.as_str()), today);
        match order.iter().position(|seen| *seen == key) {
            Some(at) => {
                if let Some(list) = people[at]["tasks"].as_array_mut() {
                    list.push(entry);
                }
            }
            None => {
                order.push(key.clone());
                people.push(json!({
                    // Nobody's name is invented: an unassigned task says so.
                    "who": key.as_deref().map(label),
                    "tasks": [entry],
                }));
            }
        }
    }

    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "overdueByOwner",
            "today": today.to_string(),
            "people": people,
            // What was actually looked at, so "nobody is late" reads as "nobody
            // on the boards you can open" rather than as a statement about the
            // whole company.
            "boardsRead": boards_read,
            "truncated": tasks.len() > MAX_TASKS,
        }
    })))
}

/// `thread_actions` — a conversation, and what has already been written down
/// out of it.
///
/// # Errors
/// 422 when no room was named; the store's own failure otherwise. A room the
/// caller cannot read comes back `found: false` rather than as a 404 — a
/// refusal would tell them a private room exists.
pub async fn execute_thread_actions(
    account: &Account,
    args: &Value,
) -> Result<Json<Value>, Problem> {
    let room = string_arg(args, "room").ok_or_else(|| unprocessable("room is required"))?;
    let limit = args
        .get("limit")
        .and_then(Value::as_i64)
        .unwrap_or(30)
        .clamp(1, 50);
    let Some(id) = room_named(account, &room).await? else {
        return Ok(Json(json!({
            "ok": true,
            "result": {
                "kind": "threadActions", "room": room, "found": false,
                "messages": [], "alreadyCaptured": [],
            }
        })));
    };
    let messages = account
        .acc
        .messages(&id, None, limit)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "threadActions",
            "room": room,
            "found": true,
            // Oldest first: a decision reads forwards.
            "messages": messages
                .iter()
                .rev()
                .map(|m| json!({
                    "author": m.message.author.as_str(),
                    "isAgent": m.message.author_is_agent,
                    "at": iso(m.message.created_at),
                    "body": m.message.body,
                }))
                .collect::<Vec<_>>(),
            "alreadyCaptured": captured_from(account, &id).await?,
        }
    })))
}

/// What has already been written down out of one room — both the actions that
/// were accepted onto a board and the ones still waiting in the caller's
/// proposals.
///
/// Both halves matter: `tasks_for_source` sees only `active` rows, so a
/// capture that has not been accepted yet would be invisible to it, and the
/// same conversation would be captured twice in a row.
async fn captured_from(account: &Account, channel: &ChatChannelId) -> Result<Vec<Value>, Problem> {
    let mut out = Vec::new();
    let accepted = account
        .acc
        .tasks_for_source("chat", channel.as_str())
        .await
        .map_err(map_store_err)?;
    let proposed = account
        .acc
        .task_proposals()
        .await
        .map_err(map_store_err)?
        .into_iter()
        .filter(|task| {
            task.source_kind.as_deref() == Some("chat")
                && task.source_id.as_deref() == Some(channel.as_str())
        });
    for task in accepted.into_iter().chain(proposed).take(MAX_TASKS) {
        out.push(json!({
            "title": task.title,
            "due": task.due_at.map(iso),
            // `proposed` is one the user has not accepted yet — it is still
            // captured, and proposing it again would be the duplicate.
            "state": task.state,
        }));
    }
    Ok(out)
}

/// The one unfinished task a title means, out of the boards the caller can
/// open.
///
/// # Errors
/// 422 when nothing was named, when nothing matches, or when several tasks do
/// — the last listing them, so the next turn can say which.
pub(crate) async fn resolve_task(
    account: &Account,
    args: &Value,
) -> Result<(Task, String), Problem> {
    let wanted =
        string_arg(args, "task").ok_or_else(|| unprocessable("say which task, by its title"))?;
    let open = open_tasks(account).await?;
    let candidates: Vec<(&str, usize)> = open
        .iter()
        .enumerate()
        .map(|(at, (task, _))| (task.title.as_str(), at))
        .collect();
    let at = pick(&wanted, candidates, "unfinished task")?;
    open.into_iter().nth(at).ok_or_else(Problem::server_error)
}

/// `create_task` — one to-do on the caller's personal project, active straight
/// away. Reuses the same tenant-scoped `create_task` the `/tasks` route uses —
/// no new storage path. Runs only from the asker's own approval (ADR 0047 §1).
///
/// # Errors
/// 422 when no title was given; the store's own failure otherwise.
pub async fn execute_create_task(account: &Account, args: &Value) -> Result<Json<Value>, Problem> {
    let title = string_arg(args, "title").unwrap_or_default();
    if title.is_empty() {
        return Err(unprocessable("title required"));
    }
    let description = string_arg(args, "notes");
    let due_at = args
        .get("due")
        .and_then(Value::as_str)
        .and_then(crate::agent::parse_due);

    let project = account
        .acc
        .ensure_personal_project()
        .await
        .map_err(map_store_err)?;
    let new = NewTask {
        title,
        description,
        status: None,
        assignee: None,
        due_at,
        priority: None,
        // Active — the user approved it (not a "proposed" suggestion).
        state: None,
        source_kind: None,
        source_id: None,
    };
    let id = account
        .acc
        .create_task(&project, &new)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "ok": true,
        "result": { "kind": "task", "id": id.as_str(), "title": new.title }
    })))
}

/// `set_task_priority` — one task's priority, and nothing else about it. Runs
/// only from the asker's own approval (ADR 0047 §1).
///
/// # Errors
/// 422 when the task was not named or does not resolve to exactly one, or when
/// the priority is not one of the four the board offers.
pub async fn execute_set_task_priority(
    account: &Account,
    args: &Value,
) -> Result<Json<Value>, Problem> {
    let (task, board) = resolve_task(account, args).await?;
    let priority =
        string_arg(args, "priority").ok_or_else(|| unprocessable("priority is required"))?;
    let priority = priority.to_lowercase();
    if !PRIORITIES.contains(&priority.as_str()) {
        return Err(unprocessable(format!(
            "priority is one of {} — not {priority}",
            PRIORITIES.join(", ")
        )));
    }
    // Everything but the priority is carried across unchanged. An edit that
    // took its other fields from the model would let a reprioritisation quietly
    // rewrite a title or drop a due date.
    account
        .acc
        .update_task(
            &task.id,
            &TaskEdit {
                title: task.title.clone(),
                description: task.description.clone(),
                assignee: task.assignee.clone(),
                due_at: task.due_at,
                priority: priority.clone(),
            },
        )
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "taskPriority",
            "title": task.title,
            "board": board,
            "was": task.priority,
            "now": priority,
        }
    })))
}

/// `chase_task` — a comment on a late task, asking its owner where it has got
/// to. Runs only from the asker's own approval, and the comment is theirs.
///
/// # Errors
/// 422 when the task was not named or does not resolve to one, when it is not
/// actually late, or when the message is missing or too long.
pub async fn execute_chase_task(
    account: &Account,
    args: &Value,
    state: &AppState,
) -> Result<Json<Value>, Problem> {
    let (task, board) = resolve_task(account, args).await?;
    let message =
        string_arg(args, "message").ok_or_else(|| unprocessable("message is required"))?;
    if message.chars().count() > MAX_COMMENT_CHARS {
        return Err(unprocessable(format!(
            "a chase is at most {MAX_COMMENT_CHARS} characters"
        )));
    }
    let today = OffsetDateTime::now_utc().date();
    // Chasing somebody about work that is not late is the mistake this refusal
    // exists to prevent — it is the one an agent makes when it reads "soon" as
    // "overdue".
    let Some(late) = days_late(&task, today) else {
        return Err(unprocessable(match task.due_at {
            Some(due) => format!("{} is not late — it is due on {}", task.title, due.date()),
            None => format!("{} has no due date, so nobody is late with it", task.title),
        }));
    };
    account
        .acc
        .add_task_comment(&task.id, &message)
        .await
        .map_err(map_store_err)?;
    let owner = match &task.assignee {
        Some(user) => state
            .store
            .for_tenant(account.tenant.clone())
            .email_of(&UserId::new(user.clone()))
            .await
            .map_err(map_store_err)?,
        None => None,
    };
    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "taskChased",
            "title": task.title,
            "board": board,
            // Null when nobody has it: the comment is still worth leaving, and
            // the room should be told there is no owner to chase.
            "owner": owner,
            "due": task.due_at.map(iso),
            "daysLate": late,
            "comment": message,
        }
    })))
}

/// `capture_actions` — what a room agreed, written down as proposals the user
/// still accepts one at a time (ADR 0023). Runs only from the asker's own
/// approval.
///
/// # Errors
/// 422 when no room was named or the room is not one the caller can read, when
/// `tasks` is missing, empty, too long or not a list of titled actions, or when
/// a due date is not a date.
pub async fn execute_capture_actions(
    account: &Account,
    args: &Value,
) -> Result<Json<Value>, Problem> {
    let room = string_arg(args, "room").ok_or_else(|| unprocessable("room is required"))?;
    let Some(channel) = room_named(account, &room).await? else {
        return Err(unprocessable(format!("no room of yours is called {room}")));
    };
    let wanted = args
        .get("tasks")
        .and_then(Value::as_array)
        .ok_or_else(|| unprocessable("tasks is a list of actions"))?;
    if wanted.is_empty() {
        return Err(unprocessable("tasks is a list of actions"));
    }
    if wanted.len() > MAX_CAPTURED {
        return Err(unprocessable(format!(
            "at most {MAX_CAPTURED} actions at a time — {} were given",
            wanted.len()
        )));
    }
    // Every action is validated before any of them is written: half a
    // conversation captured is worse than none, because the half that failed is
    // the half nobody notices is missing.
    let mut new: Vec<NewTask> = Vec::new();
    for (at, action) in wanted.iter().enumerate() {
        let title = string_arg(action, "title")
            .ok_or_else(|| unprocessable(format!("action {} has no title", at + 1)))?;
        let due_at = match string_arg(action, "due") {
            Some(day) => Some(
                parse_iso_date(&day)
                    .ok_or_else(|| {
                        unprocessable(format!("action {}: due must be YYYY-MM-DD", at + 1))
                    })?
                    .with_time(Time::MIDNIGHT)
                    .assume_utc(),
            ),
            None => None,
        };
        new.push(NewTask {
            title,
            description: string_arg(action, "notes"),
            status: None,
            assignee: None,
            due_at,
            priority: None,
            // ADR 0023: an action the agent read out of a conversation is a
            // suggestion until its owner says otherwise, whatever the approval
            // that ran this tool said.
            state: Some("proposed".to_owned()),
            // The room it came out of, so `thread_actions` can say this has
            // already been captured.
            source_kind: Some("chat".to_owned()),
            source_id: Some(channel.as_str().to_owned()),
        });
    }
    let project = account
        .acc
        .ensure_personal_project()
        .await
        .map_err(map_store_err)?;
    let mut written = Vec::new();
    for task in &new {
        account
            .acc
            .create_task(&project, task)
            .await
            .map_err(map_store_err)?;
        written.push(json!({ "title": task.title, "due": task.due_at.map(iso) }));
    }
    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "actionsCaptured",
            "room": room,
            "captured": written.len(),
            "tasks": written,
            // Said in the result, because the room's next sentence should be
            // "they are waiting in your tasks" rather than "done".
            "state": "proposed",
        }
    })))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use alo_store::TaskId;
    use time::Month;

    fn day(d: u8) -> Date {
        Date::from_calendar_date(2026, Month::August, d).unwrap()
    }

    fn task(title: &str, due: Option<Date>) -> Task {
        Task {
            id: TaskId::new(format!("task-{title}")),
            project_id: ProjectId::new("proj"),
            title: title.to_owned(),
            description: None,
            status: "todo".to_owned(),
            position: 1.0,
            assignee: None,
            due_at: due.map(|d| d.with_time(Time::MIDNIGHT).assume_utc()),
            priority: "none".to_owned(),
            state: "active".to_owned(),
            source_kind: None,
            source_id: None,
            created_by: "u".to_owned(),
            created_at: OffsetDateTime::now_utc(),
            updated_at: OffsetDateTime::now_utc(),
            completed_at: None,
            subtask_done: 1,
            subtask_total: 3,
            comment_count: 0,
        }
    }

    /// The rule the whole module shares: a task due **today** is not late, and
    /// one due yesterday is late by a day.
    #[test]
    fn late_means_due_before_today_and_never_earlier_in_the_same_day() {
        let today = day(20);
        assert_eq!(days_late(&task("t", Some(day(20))), today), None);
        assert_eq!(days_late(&task("t", Some(day(19))), today), Some(1));
        assert_eq!(days_late(&task("t", Some(day(13))), today), Some(7));
        assert_eq!(days_late(&task("t", Some(day(21))), today), None);
        // …and a task nobody dated is not late, which is a different thing from
        // being on time.
        assert_eq!(days_late(&task("t", None), today), None);
    }

    /// What the model is shown of one task: enough to put it in order, and no
    /// identifier it could point at a record with.
    #[test]
    fn a_task_is_reported_without_an_id_and_with_its_lateness() {
        let shown = task_json(
            &task("Send the deck", Some(day(18))),
            Some("Sales"),
            day(20),
        );
        assert_eq!(shown["title"], json!("Send the deck"));
        assert_eq!(shown["board"], json!("Sales"));
        assert_eq!(shown["checklist"], json!("1/3"));
        assert_eq!(shown["daysLate"], json!(2));
        assert!(shown.get("id").is_none(), "{shown}");
        // A board the asker cannot open is null rather than absent, and a task
        // that is not late carries no lateness at all.
        let shown = task_json(&task("Later", Some(day(25))), None, day(20));
        assert_eq!(shown["board"], Value::Null);
        assert!(shown.get("daysLate").is_none(), "{shown}");
    }

    /// The four the board offers, and nothing else — the check that keeps a
    /// model's invented word ("urgent") out of the column.
    #[test]
    fn only_the_boards_own_priorities_exist() {
        assert!(PRIORITIES.contains(&"high"));
        assert!(!PRIORITIES.contains(&"urgent"));
        assert_eq!(PRIORITIES.len(), 4);
    }
}
