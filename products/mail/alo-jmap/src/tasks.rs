//! Tasks HTTP surface (ADR 0021–0023). Authenticated, tenant/user-scoped through
//! the account door — every handler resolves the caller with [`authenticate`]
//! and touches only tasks on projects visible to them.
//!
//! The API returns tasks as plain rows; the client groups them into a board
//! (by status) or a flat list (ADR 0022), so there is no server-side "board
//! shape" to keep in sync. Moving a card is one endpoint (`/move`) shared by the
//! board drag and the list status-change. AI-created tasks arrive via `/propose`
//! as `proposed` and are only ever surfaced as work after `/accept` (ADR 0023).

use std::collections::HashMap;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use alo_store::{
    AccountStore, AttachmentId, BlobId, CommentId, LabelId, NewTask, ProjectId, StoreError,
    SubtaskId, Task, TaskEdit, TaskId, TaskLabel, TenantStore, UserId,
};

use crate::error::Problem;
use crate::state::{Account, AppState, authenticate};

// ---- JSON shaping -----------------------------------------------------------

fn iso(t: OffsetDateTime) -> String {
    t.format(&Rfc3339).unwrap_or_default()
}

/// A task as JSON. `emails` maps assignee user ids to display addresses.
///
/// `pub(crate)` because a task is one shape everywhere it appears — including
/// the next steps of a CRM deal (`crate::crm_next_steps`), which reads the same
/// rows through ADR 0021's source link.
pub(crate) fn task_json(t: &Task, emails: &HashMap<String, String>) -> Value {
    json!({
        "id": t.id.as_str(),
        "projectId": t.project_id.as_str(),
        "title": t.title,
        "description": t.description,
        "status": t.status,
        "position": t.position,
        "assigneeId": t.assignee,
        "assignee": t.assignee.as_deref().and_then(|u| emails.get(u)).cloned(),
        "dueAt": t.due_at.map(iso),
        "priority": t.priority,
        "state": t.state,
        "sourceKind": t.source_kind,
        "sourceId": t.source_id,
        "subtaskDone": t.subtask_done,
        "subtaskTotal": t.subtask_total,
        "commentCount": t.comment_count,
        "completedAt": t.completed_at.map(iso),
        "createdAt": iso(t.created_at),
    })
}

/// Resolve a set of user ids to their email addresses (deduped, best-effort).
pub(crate) async fn resolve_emails(ts: &TenantStore, tasks: &[Task]) -> HashMap<String, String> {
    let mut ids: Vec<String> = tasks.iter().filter_map(|t| t.assignee.clone()).collect();
    ids.sort();
    ids.dedup();
    let mut map = HashMap::new();
    for id in ids {
        if let Ok(Some(email)) = ts.email_of(&UserId::new(id.clone())).await {
            map.insert(id, email);
        }
    }
    map
}

fn label_json(l: &TaskLabel) -> Value {
    json!({ "id": l.id.as_str(), "name": l.name, "color": l.color })
}

async fn tasks_response(ts: &TenantStore, acc: &AccountStore, tasks: Vec<Task>) -> Value {
    let emails = resolve_emails(ts, &tasks).await;
    let ids: Vec<String> = tasks.iter().map(|t| t.id.as_str().to_owned()).collect();
    let labels = acc.labels_for_task_ids(&ids).await.unwrap_or_default();
    let out: Vec<Value> = tasks
        .iter()
        .map(|t| {
            let mut j = task_json(t, &emails);
            let ls: Vec<Value> = labels
                .get(t.id.as_str())
                .map(|v| v.iter().map(label_json).collect())
                .unwrap_or_default();
            if let Some(obj) = j.as_object_mut() {
                obj.insert("labels".to_owned(), Value::Array(ls));
            }
            j
        })
        .collect();
    json!({ "tasks": out })
}

fn parse_time(s: &str) -> Result<OffsetDateTime, Problem> {
    OffsetDateTime::parse(s, &Rfc3339)
        .map(|t| t.to_offset(time::UtcOffset::UTC))
        .map_err(|_| {
            Problem::with(
                StatusCode::BAD_REQUEST,
                "invalid date/time (expected RFC 3339)",
            )
        })
}

fn map_store_err(e: StoreError) -> Problem {
    match e {
        StoreError::NotFound => Problem::with(StatusCode::NOT_FOUND, "not found"),
        StoreError::Conflict(msg) => Problem::with(StatusCode::CONFLICT, &msg),
        _ => Problem::server_error(),
    }
}

// ---- projects ---------------------------------------------------------------

/// `GET /tasks/projects` → `{"projects":[...]}` — the caller's visible projects.
pub async fn list_projects(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let projects = account
        .acc
        .task_projects()
        .await
        .map_err(|_| Problem::server_error())?;
    let out: Vec<Value> = projects
        .iter()
        .map(|p| {
            json!({
                "id": p.id.as_str(), "name": p.name, "kind": p.kind, "color": p.color,
            })
        })
        .collect();
    Ok(Json(json!({ "projects": out })))
}

#[derive(Deserialize)]
struct ProjectBody {
    name: String,
    #[serde(default)]
    color: Option<String>,
}

/// `POST /tasks/projects` → the created team project.
pub async fn create_project(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: ProjectBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let name = req.name.trim();
    if name.is_empty() {
        return Err(Problem::with(
            StatusCode::BAD_REQUEST,
            "a project name is required",
        ));
    }
    let color = req
        .color
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty());
    let id = account
        .acc
        .create_task_project(name, color)
        .await
        .map_err(|_| Problem::server_error())?;
    Ok(Json(
        json!({ "id": id.as_str(), "name": name, "kind": "team", "color": color }),
    ))
}

// ---- tasks ------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ProjectQuery {
    project: String,
}

/// `GET /tasks?project=` → `{"tasks":[...]}` — the active tasks on a project
/// (the client groups them into board columns or a list).
pub async fn list_tasks(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ProjectQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let tasks = account
        .acc
        .tasks_in_project(&ProjectId::new(q.project))
        .await
        .map_err(|_| Problem::server_error())?;
    let ts = state.store.for_tenant(account.tenant.clone());
    Ok(Json(tasks_response(&ts, &account.acc, tasks).await))
}

#[derive(Deserialize)]
struct TaskBody {
    #[serde(default, rename = "projectId")]
    project_id: Option<String>,
    title: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    assignee: Option<String>,
    #[serde(default, rename = "dueAt")]
    due_at: Option<String>,
    #[serde(default)]
    priority: Option<String>,
    #[serde(default, rename = "sourceKind")]
    source_kind: Option<String>,
    #[serde(default, rename = "sourceId")]
    source_id: Option<String>,
}

/// Resolves an assignee email/id to a user id in the caller's tenant, or `None`.
pub(crate) async fn resolve_assignee(
    state: &AppState,
    account: &Account,
    assignee: &Option<String>,
) -> Option<String> {
    let raw = assignee.as_deref()?.trim();
    if raw.is_empty() {
        return None;
    }
    let ts = state.store.for_tenant(account.tenant.clone());
    // An email resolves to its user id; a bare user id is kept as-is.
    match ts.user_by_email(raw).await {
        Ok(uid) => Some(uid.as_str().to_owned()),
        Err(_) => Some(raw.to_owned()),
    }
}

async fn build_new_task(
    state: &AppState,
    account: &Account,
    req: TaskBody,
    proposed: bool,
) -> Result<NewTask, Problem> {
    let title = req.title.trim().to_owned();
    if title.is_empty() {
        return Err(Problem::with(
            StatusCode::BAD_REQUEST,
            "a title is required",
        ));
    }
    let due_at = match &req.due_at {
        Some(s) if !s.is_empty() => Some(parse_time(s)?),
        _ => None,
    };
    let assignee = resolve_assignee(state, account, &req.assignee).await;
    Ok(NewTask {
        title,
        description: req
            .description
            .map(|d| d.trim().to_owned())
            .filter(|d| !d.is_empty()),
        status: req.status.filter(|s| !s.is_empty()),
        assignee,
        due_at,
        priority: req.priority.filter(|p| !p.is_empty()),
        state: proposed.then(|| "proposed".to_owned()),
        source_kind: req.source_kind,
        source_id: req.source_id,
    })
}

/// `POST /tasks` → the created task (on `projectId`, else the personal project).
pub async fn create_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: TaskBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let project = match req.project_id.as_deref().filter(|p| !p.is_empty()) {
        Some(p) => ProjectId::new(p.to_owned()),
        None => account
            .acc
            .ensure_personal_project()
            .await
            .map_err(|_| Problem::server_error())?,
    };
    let new = build_new_task(&state, &account, req, false).await?;
    let id = account
        .acc
        .create_task(&project, &new)
        .await
        .map_err(map_store_err)?;
    match account
        .acc
        .task(&id)
        .await
        .map_err(|_| Problem::server_error())?
    {
        Some(t) => {
            let ts = state.store.for_tenant(account.tenant.clone());
            let emails = resolve_emails(&ts, std::slice::from_ref(&t)).await;
            Ok(Json(task_json(&t, &emails)))
        }
        None => Err(Problem::server_error()),
    }
}

/// `GET /tasks/:id` → the task with its subtasks, comments, and activity.
pub async fn get_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let tid = TaskId::new(id);
    let Some(task) = account
        .acc
        .task(&tid)
        .await
        .map_err(|_| Problem::server_error())?
    else {
        return Err(Problem::with(StatusCode::NOT_FOUND, "no such task"));
    };
    Ok(Json(task_record(&state, &account, &task).await?))
}

/// One task in full — the record `GET /tasks/:id` serves, shared with the
/// Tasks agent's `task_lookup` executor (`crate::tasks_intents`) so both read
/// the same view (A4.1b-style: behaviour unchanged, one assembly).
pub(crate) async fn task_record(
    state: &AppState,
    account: &crate::state::Account,
    task: &Task,
) -> Result<Value, Problem> {
    let tid = task.id.clone();
    let subtasks = account
        .acc
        .subtasks(&tid)
        .await
        .map_err(|_| Problem::server_error())?;
    let comments = account
        .acc
        .task_comments(&tid)
        .await
        .map_err(|_| Problem::server_error())?;
    let activity = account
        .acc
        .task_activity(&tid)
        .await
        .map_err(|_| Problem::server_error())?;
    let attachments = account
        .acc
        .task_attachments(&tid)
        .await
        .map_err(|_| Problem::server_error())?;
    let labels = account
        .acc
        .labels_for_task(&tid)
        .await
        .map_err(|_| Problem::server_error())?;
    let followers = account
        .acc
        .task_followers(&tid)
        .await
        .map_err(|_| Problem::server_error())?;
    let following = followers.iter().any(|u| u == account.user.as_str());
    let blocked_by = account
        .acc
        .dependencies(&tid)
        .await
        .map_err(|_| Problem::server_error())?;
    let ts = state.store.for_tenant(account.tenant.clone());
    let emails = resolve_emails(&ts, std::slice::from_ref(task)).await;
    // Resolve comment/activity actors too.
    let mut actor_ids: Vec<String> = comments.iter().map(|c| c.author.clone()).collect();
    actor_ids.extend(activity.iter().map(|a| a.actor.clone()));
    actor_ids.extend(followers.iter().cloned());
    actor_ids.sort();
    actor_ids.dedup();
    let mut actors = HashMap::new();
    for uid in actor_ids {
        if let Ok(Some(e)) = ts.email_of(&UserId::new(uid.clone())).await {
            actors.insert(uid, e);
        }
    }
    let name = |u: &str| actors.get(u).cloned().unwrap_or_else(|| u.to_owned());
    Ok(json!({
        "task": task_json(task, &emails),
        "subtasks": subtasks.iter().map(|s| json!({
            "id": s.id.as_str(), "title": s.title, "done": s.done,
        })).collect::<Vec<_>>(),
        "comments": comments.iter().map(|c| json!({
            "id": c.id.as_str(), "author": name(&c.author), "body": c.body, "createdAt": iso(c.created_at),
        })).collect::<Vec<_>>(),
        "activity": activity.iter().map(|a| json!({
            "actor": name(&a.actor), "kind": a.kind, "detail": a.detail, "createdAt": iso(a.created_at),
        })).collect::<Vec<_>>(),
        "attachments": attachments.iter().map(|a| json!({
            "id": a.id.as_str(), "blobId": a.blob_id, "filename": a.filename,
            "size": a.size, "createdAt": iso(a.created_at),
        })).collect::<Vec<_>>(),
        "labels": labels.iter().map(label_json).collect::<Vec<_>>(),
        "followers": followers.iter().map(|u| name(u)).collect::<Vec<_>>(),
        "following": following,
        "blockedBy": blocked_by.iter().map(|d| json!({
            "id": d.id.as_str(), "title": d.title, "status": d.status,
        })).collect::<Vec<_>>(),
    }))
}

/// `PUT /tasks/:id` → `{status:"ok"}` — edit title/description/assignee/due/
/// priority (status + position move via `/move`).
pub async fn update_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: TaskBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let title = req.title.trim().to_owned();
    if title.is_empty() {
        return Err(Problem::with(
            StatusCode::BAD_REQUEST,
            "a title is required",
        ));
    }
    let due_at = match &req.due_at {
        Some(s) if !s.is_empty() => Some(parse_time(s)?),
        _ => None,
    };
    let assignee = resolve_assignee(&state, &account, &req.assignee).await;
    let edit = TaskEdit {
        title,
        description: req
            .description
            .map(|d| d.trim().to_owned())
            .filter(|d| !d.is_empty()),
        assignee,
        due_at,
        priority: req
            .priority
            .filter(|p| !p.is_empty())
            .unwrap_or_else(|| "none".to_owned()),
    };
    account
        .acc
        .update_task(&TaskId::new(id), &edit)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "status": "ok" })))
}

#[derive(Deserialize)]
struct MoveBody {
    status: String,
    position: f64,
}

/// `POST /tasks/:id/move` → `{status:"ok"}` — the one move (board drag or list
/// status-change), ADR 0022.
pub async fn move_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: MoveBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    account
        .acc
        .move_task(&TaskId::new(id), req.status.trim(), req.position)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "status": "ok" })))
}

/// `DELETE /tasks/:id` → `{status:"ok"}`.
pub async fn delete_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .delete_task(&TaskId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "status": "ok" })))
}

// ---- the connections --------------------------------------------------------

/// `GET /tasks/today` → `{"tasks":[...]}` — the caller's due/overdue assigned
/// tasks ("what's on my plate"). The tasks half of the aggregate.
pub async fn my_plate(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    // End of today, UTC (a per-user timezone refinement can come later).
    let now = OffsetDateTime::now_utc();
    let today_end =
        now.replace_time(time::Time::from_hms(23, 59, 59).unwrap_or(time::Time::MIDNIGHT));
    let tasks = account
        .acc
        .my_plate(today_end)
        .await
        .map_err(|_| Problem::server_error())?;
    let ts = state.store.for_tenant(account.tenant.clone());
    Ok(Json(tasks_response(&ts, &account.acc, tasks).await))
}

#[derive(Deserialize)]
pub struct RangeQuery {
    from: String,
    to: String,
}

/// `GET /tasks/due?from=&to=` → `{"tasks":[...]}` — active tasks with a due date
/// in the window, for the calendar to overlay alongside events.
pub async fn due_tasks(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<RangeQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let from = parse_time(&q.from)?;
    let to = parse_time(&q.to)?;
    let tasks = account
        .acc
        .due_tasks_in_range(from, to)
        .await
        .map_err(|_| Problem::server_error())?;
    let ts = state.store.for_tenant(account.tenant.clone());
    Ok(Json(tasks_response(&ts, &account.acc, tasks).await))
}

// ---- propose-then-approve (ADR 0023) ----------------------------------------

/// `GET /tasks/proposals` → `{"tasks":[...]}` — pending AI proposals.
pub async fn list_proposals(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let tasks = account
        .acc
        .task_proposals()
        .await
        .map_err(|_| Problem::server_error())?;
    let ts = state.store.for_tenant(account.tenant.clone());
    Ok(Json(tasks_response(&ts, &account.acc, tasks).await))
}

#[derive(Deserialize)]
struct ProposeBody {
    #[serde(default, rename = "projectId")]
    project_id: Option<String>,
    tasks: Vec<TaskBody>,
}

/// `POST /tasks/propose` → `{"created":n}` — the AI hook: suggests tasks as
/// `proposed`, never active work (ADR 0023). Sources (meeting/email) plug in
/// here as those modules land; the approval half is live now.
pub async fn propose_tasks(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: ProposeBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let default_project = account
        .acc
        .ensure_personal_project()
        .await
        .map_err(|_| Problem::server_error())?;
    let mut created = 0;
    for t in req.tasks {
        let project = match req.project_id.as_deref().filter(|p| !p.is_empty()) {
            Some(p) => ProjectId::new(p.to_owned()),
            None => default_project.clone(),
        };
        let new = build_new_task(&state, &account, t, true).await?;
        if account.acc.create_task(&project, &new).await.is_ok() {
            created += 1;
        }
    }
    Ok(Json(json!({ "created": created })))
}

/// `POST /tasks/:id/accept` → `{status:"ok"}` — approve a proposal (make it real).
pub async fn accept_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    // An empty body accepts as-is; a body refines the AI's suggestion first.
    let edit = if body.is_empty() {
        None
    } else {
        let req: TaskBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
        let due_at = match &req.due_at {
            Some(s) if !s.is_empty() => Some(parse_time(s)?),
            _ => None,
        };
        Some(TaskEdit {
            title: req.title.trim().to_owned(),
            description: req
                .description
                .map(|d| d.trim().to_owned())
                .filter(|d| !d.is_empty()),
            assignee: resolve_assignee(&state, &account, &req.assignee).await,
            due_at,
            priority: req
                .priority
                .filter(|p| !p.is_empty())
                .unwrap_or_else(|| "none".to_owned()),
        })
    };
    account
        .acc
        .accept_task(&TaskId::new(id), edit.as_ref())
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "status": "ok" })))
}

/// `POST /tasks/:id/reject` → `{status:"ok"}` — drop a proposal.
pub async fn reject_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .reject_task(&TaskId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "status": "ok" })))
}

// ---- subtasks / comments ----------------------------------------------------

#[derive(Deserialize)]
struct SubtaskBody {
    title: String,
}

/// `POST /tasks/:id/subtasks` → `{id}` — add a checklist item.
pub async fn add_subtask(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: SubtaskBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let title = req.title.trim();
    if title.is_empty() {
        return Err(Problem::with(
            StatusCode::BAD_REQUEST,
            "a subtask title is required",
        ));
    }
    let sid = account
        .acc
        .add_subtask(&TaskId::new(id), title)
        .await
        .map_err(map_store_err)?;
    Ok(Json(
        json!({ "id": sid.as_str(), "title": title, "done": false }),
    ))
}

#[derive(Deserialize)]
struct SubtaskDoneBody {
    done: bool,
}

/// `PUT /tasks/:id/subtasks/:sid` → `{status:"ok"}` — check/uncheck.
pub async fn set_subtask(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, sid)): Path<(String, String)>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: SubtaskDoneBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    account
        .acc
        .set_subtask_done(&TaskId::new(id), &SubtaskId::new(sid), req.done)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "status": "ok" })))
}

/// `DELETE /tasks/:id/subtasks/:sid` → `{status:"ok"}`.
pub async fn delete_subtask(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, sid)): Path<(String, String)>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .delete_subtask(&TaskId::new(id), &SubtaskId::new(sid))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "status": "ok" })))
}

#[derive(Deserialize)]
struct CommentBody {
    body: String,
}

/// `POST /tasks/:id/comments` → `{id}` — add a comment.
pub async fn add_comment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: CommentBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let text = req.body.trim();
    if text.is_empty() {
        return Err(Problem::with(StatusCode::BAD_REQUEST, "an empty comment"));
    }
    let cid: CommentId = account
        .acc
        .add_task_comment(&TaskId::new(id), text)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "id": cid.as_str() })))
}

/// The body of `POST /tasks/:id/attachments`: an already-uploaded blob plus its
/// display name and size (the upload itself uses the JMAP blob upload).
#[derive(Deserialize)]
struct AttachBody {
    #[serde(rename = "blobId")]
    blob_id: String,
    filename: String,
    #[serde(default)]
    size: i64,
}

/// `GET /tasks/:id/attachments` → `{"attachments":[...]}` — the files on a task.
pub async fn list_attachments(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let items = account
        .acc
        .task_attachments(&TaskId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "attachments": items.iter().map(|a| json!({
            "id": a.id.as_str(), "blobId": a.blob_id, "filename": a.filename,
            "size": a.size, "createdAt": iso(a.created_at),
        })).collect::<Vec<_>>(),
    })))
}

/// `POST /tasks/:id/attachments` → `{"id": "..."}` — attach an uploaded blob.
pub async fn add_attachment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: AttachBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    if req.blob_id.trim().is_empty() || req.filename.trim().is_empty() {
        return Err(Problem::with(
            StatusCode::BAD_REQUEST,
            "blobId and filename required",
        ));
    }
    let aid = account
        .acc
        .add_task_attachment(
            &TaskId::new(id),
            req.blob_id.trim(),
            req.filename.trim(),
            req.size,
        )
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "id": aid.as_str() })))
}

/// `DELETE /tasks/:id/attachments/:aid` → `{"status":"ok"}`.
pub async fn delete_attachment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, aid)): Path<(String, String)>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .delete_task_attachment(&TaskId::new(id), &AttachmentId::new(aid))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "status": "ok" })))
}

/// `GET /tasks/files?project=` → `{"files":[...]}` — every attachment across the
/// tasks of a project the caller can see (the project-wide Files view).
pub async fn project_files(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ProjectQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let files = account
        .acc
        .project_files(&ProjectId::new(q.project))
        .await
        .map_err(|_| Problem::server_error())?;
    Ok(Json(json!({
        "files": files.iter().map(|f| json!({
            "id": f.id.as_str(), "taskId": f.task_id, "taskTitle": f.task_title,
            "blobId": f.blob_id, "filename": f.filename, "size": f.size,
            "createdAt": iso(f.created_at),
        })).collect::<Vec<_>>(),
    })))
}

/// `GET /tasks/:id/attachments/:aid/download` — stream a task's attached file.
/// Gated by task visibility (a caller who can't see the task gets 404); the blob
/// is then served tenant-scoped, since the attachment reference proves access.
pub async fn download_attachment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, aid)): Path<(String, String)>,
) -> Result<axum::response::Response, Problem> {
    let account = authenticate(&state, &headers).await?;
    let items = account
        .acc
        .task_attachments(&TaskId::new(id))
        .await
        .map_err(map_store_err)?;
    let att = items
        .iter()
        .find(|a| a.id.as_str() == aid)
        .ok_or_else(Problem::not_found)?;
    let bytes = account
        .acc
        .blob_bytes_for_send(&BlobId::new(att.blob_id.clone()))
        .await
        .map_err(map_store_err)?;
    Ok(crate::blob::serve_download(
        bytes,
        "application/octet-stream",
        &att.filename,
    ))
}

// ---- labels -----------------------------------------------------------------

/// `GET /tasks/labels` → `{"labels":[...]}` — every label in the tenant.
pub async fn list_labels(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let labels = account
        .acc
        .task_labels()
        .await
        .map_err(|_| Problem::server_error())?;
    Ok(Json(
        json!({ "labels": labels.iter().map(label_json).collect::<Vec<_>>() }),
    ))
}

#[derive(Deserialize)]
struct LabelBody {
    name: String,
    #[serde(default)]
    color: Option<String>,
}

/// `POST /tasks/labels` → the created label.
pub async fn create_label(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: LabelBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let name = req.name.trim();
    if name.is_empty() {
        return Err(Problem::with(
            StatusCode::BAD_REQUEST,
            "a label name is required",
        ));
    }
    let color = req
        .color
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty());
    let id = account
        .acc
        .create_task_label(name, color)
        .await
        .map_err(|_| Problem::server_error())?;
    Ok(Json(
        json!({ "id": id.as_str(), "name": name, "color": color }),
    ))
}

/// `DELETE /tasks/labels/:id` → remove a label from the tenant (and every task).
pub async fn delete_label(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .delete_task_label(&LabelId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "status": "ok" })))
}

#[derive(Deserialize)]
struct AddLabelBody {
    #[serde(rename = "labelId")]
    label_id: String,
}

/// `POST /tasks/:id/labels` → attach a tenant label to a task.
pub async fn add_task_label(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: AddLabelBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    account
        .acc
        .add_task_label(&TaskId::new(id), &LabelId::new(req.label_id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "status": "ok" })))
}

/// `DELETE /tasks/:id/labels/:lid` → remove a label from a task.
pub async fn remove_task_label(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, lid)): Path<(String, String)>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .remove_task_label(&TaskId::new(id), &LabelId::new(lid))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "status": "ok" })))
}

// ---- followers --------------------------------------------------------------

/// `POST /tasks/:id/followers` → `{"status":"ok"}` — the caller follows a task.
pub async fn follow_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .follow_task(&TaskId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "status": "ok" })))
}

/// `DELETE /tasks/:id/followers` → `{"status":"ok"}` — the caller stops
/// following a task ("Leave task").
pub async fn unfollow_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .unfollow_task(&TaskId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "status": "ok" })))
}

// ---- dependencies -----------------------------------------------------------

#[derive(Deserialize)]
struct DependencyBody {
    #[serde(rename = "dependsOn")]
    depends_on: String,
}

/// `POST /tasks/:id/dependencies` `{dependsOn}` → `{"status":"ok"}` — record that
/// this task is blocked by another. Both must be visible to the caller.
pub async fn add_dependency(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: DependencyBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    if req.depends_on.trim().is_empty() {
        return Err(Problem::with(
            StatusCode::BAD_REQUEST,
            "dependsOn is required",
        ));
    }
    account
        .acc
        .add_dependency(
            &TaskId::new(id),
            &TaskId::new(req.depends_on.trim().to_owned()),
        )
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "status": "ok" })))
}

/// `DELETE /tasks/:id/dependencies/:dep` → `{"status":"ok"}` — drop a "blocked
/// by" edge.
pub async fn remove_dependency(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, dep)): Path<(String, String)>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .remove_dependency(&TaskId::new(id), &TaskId::new(dep))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "status": "ok" })))
}

/// `GET /tasks/dependencies?project=` → `{"edges":[{blocked,blockedBy}]}` — every
/// dependency edge among the caller's visible tasks in a project (Timeline arrows).
pub async fn project_dependencies(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ProjectQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let edges = account
        .acc
        .project_dependencies(&ProjectId::new(q.project))
        .await
        .map_err(|_| Problem::server_error())?;
    Ok(Json(json!({
        "edges": edges.iter().map(|(blocked, blocker)| json!({
            "blocked": blocked.as_str(), "blockedBy": blocker.as_str(),
        })).collect::<Vec<_>>(),
    })))
}
