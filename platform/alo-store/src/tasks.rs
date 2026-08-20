//! Tasks — the third leg of the mail + calendar + tasks wedge, tenant/user-
//! scoped through the account door exactly like [`crate::calendar`]. One task is
//! one row (ADR 0021); board and list are groupings of the same rows (ADR 0022),
//! so a card move is a single-field update. Personal and team are one model,
//! differing only by which project (and thus scope) a task belongs to: a
//! `personal` project resolves only for its owner; a `team` project is visible
//! tenant-wide (v1). AI-created tasks land in `state = 'proposed'` and are never
//! returned as active work until accepted (ADR 0023).

use serde_json::json;
use time::{Date, OffsetDateTime};

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::{AttachmentId, CommentId, LabelId, ProjectId, SubtaskId, TaskId};

/// A task project (board): the group a task belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskProject {
    pub id: ProjectId,
    pub name: String,
    /// `personal` (private to owner) or `team` (shared).
    pub kind: String,
    pub owner: String,
    pub color: Option<String>,
    pub description: Option<String>,
    pub status: String,
    pub starts_on: Option<Date>,
    pub target_on: Option<Date>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

/// The whole editable lifecycle record of a team project.
pub struct TaskProjectEdit {
    pub name: String,
    pub description: Option<String>,
    pub status: String,
    pub starts_on: Option<Date>,
    pub target_on: Option<Date>,
}

/// The core task record, plus the small counts the card/list need.
#[derive(Debug, Clone)]
pub struct Task {
    pub id: TaskId,
    pub project_id: ProjectId,
    pub title: String,
    pub description: Option<String>,
    /// The board column.
    pub status: String,
    /// Fractional order within the column.
    pub position: f64,
    pub assignee: Option<String>,
    pub due_at: Option<OffsetDateTime>,
    /// `none` | `low` | `medium` | `high`.
    pub priority: String,
    /// `active` | `proposed`.
    pub state: String,
    /// The source link: `email` / `event` + its id (ADR 0021).
    pub source_kind: Option<String>,
    pub source_id: Option<String>,
    pub created_by: String,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    pub completed_at: Option<OffsetDateTime>,
    pub subtask_done: i64,
    pub subtask_total: i64,
    pub comment_count: i64,
}

/// A checklist item inside a task.
#[derive(Debug, Clone)]
pub struct Subtask {
    pub id: SubtaskId,
    pub title: String,
    pub done: bool,
    pub position: f64,
}

/// A comment on a task.
#[derive(Debug, Clone)]
pub struct TaskComment {
    pub id: CommentId,
    pub author: String,
    pub body: String,
    pub created_at: OffsetDateTime,
}

/// A file attached to a task: a reference to a tenant blob (uploaded via the
/// JMAP blob upload) plus its display name and size.
#[derive(Debug, Clone)]
pub struct TaskAttachment {
    pub id: AttachmentId,
    pub blob_id: String,
    pub filename: String,
    pub size: i64,
    pub created_at: OffsetDateTime,
}

/// A task attachment rolled up to the project level (with the task it hangs on),
/// for the project-wide "Files" view.
#[derive(Debug, Clone)]
pub struct ProjectFile {
    pub id: AttachmentId,
    pub task_id: String,
    pub task_title: String,
    pub blob_id: String,
    pub filename: String,
    pub size: i64,
    pub created_at: OffsetDateTime,
}

/// A reusable, tenant-scoped label (tag) a task can carry.
#[derive(Debug, Clone)]
pub struct TaskLabel {
    pub id: LabelId,
    pub name: String,
    pub color: Option<String>,
}

/// A lightweight reference to another task, used for dependency edges: enough
/// to render a "blocked by" chip and colour a Timeline arrow.
#[derive(Debug, Clone)]
pub struct TaskDepRef {
    pub id: TaskId,
    pub title: String,
    /// The blocker's board column, so the UI can colour the arrow by state.
    pub status: String,
}

/// One entry in a task's history.
#[derive(Debug, Clone)]
pub struct TaskActivity {
    pub actor: String,
    pub kind: String,
    pub detail: serde_json::Value,
    pub created_at: OffsetDateTime,
}

/// The editable fields when creating a task (status/position are chosen by the
/// store on create; a move changes them later).
#[derive(Debug, Clone, Default)]
pub struct NewTask {
    pub title: String,
    pub description: Option<String>,
    pub status: Option<String>,
    pub assignee: Option<String>,
    pub due_at: Option<OffsetDateTime>,
    pub priority: Option<String>,
    /// `active` (default) or `proposed` (an AI suggestion, ADR 0023).
    pub state: Option<String>,
    pub source_kind: Option<String>,
    pub source_id: Option<String>,
}

/// The editable fields of an existing task (title/description/assignee/due/
/// priority). Status + position move via [`AccountStore::move_task`].
#[derive(Debug, Clone, Default)]
pub struct TaskEdit {
    pub title: String,
    pub description: Option<String>,
    pub assignee: Option<String>,
    pub due_at: Option<OffsetDateTime>,
    pub priority: String,
}

/// The most tasks one source record answers with
/// ([`AccountStore::tasks_for_source`]). The caller — a deal drawer, an email —
/// renders them whole, so the read is bounded rather than paged.
pub const SOURCE_TASKS_MAX: i64 = 200;

/// SQL predicate (tasks aliased `t`, viewer `$2`): the task's project is visible
/// — a team project (shared tenant-wide, v1) or the viewer's own personal one.
fn visible_projects() -> &'static str {
    "t.project_id IN (SELECT p.id FROM task_projects p WHERE p.tenant_id = $1 \
       AND p.archived = false \
       AND (p.kind = 'team' OR (p.kind = 'personal' AND p.owner_user_id = $2)))"
}

const TASK_COLS: &str = "t.id, t.project_id, t.title, t.description, t.status, t.position, \
     t.assignee_user_id, t.due_at, t.priority, t.state, t.source_kind, t.source_id, \
     t.created_by, t.created_at, t.updated_at, t.completed_at, \
     (SELECT count(*) FILTER (WHERE s.done) FROM task_subtasks s \
        WHERE s.tenant_id = t.tenant_id AND s.task_id = t.id) AS subtask_done, \
     (SELECT count(*) FROM task_subtasks s \
        WHERE s.tenant_id = t.tenant_id AND s.task_id = t.id) AS subtask_total, \
     (SELECT count(*) FROM task_comments c \
        WHERE c.tenant_id = t.tenant_id AND c.task_id = t.id) AS comment_count";

impl AccountStore {
    // ---- projects --------------------------------------------------------

    /// The projects visible to the caller (their personal project, ensured to
    /// exist, plus every team project), creation order.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn task_projects(&self) -> Result<Vec<TaskProject>> {
        self.ensure_personal_project().await?;
        let rows = sqlx::query_as::<_, ProjectRow>(
            "SELECT id, name, kind, owner_user_id, color, description, status, \
                    starts_on, target_on, created_at, updated_at FROM task_projects \
             WHERE tenant_id = $1 AND archived = false \
               AND (kind = 'team' OR (kind = 'personal' AND owner_user_id = $2)) \
             ORDER BY (kind = 'personal') DESC, created_at, id",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(ProjectRow::into_project).collect())
    }

    /// The caller's personal project id, creating it if absent (deterministic
    /// `proj_personal_<user>`, stable across calls).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn ensure_personal_project(&self) -> Result<ProjectId> {
        let id = format!("proj_personal_{}", self.user.as_str());
        let inserted = sqlx::query(
            "INSERT INTO task_projects (tenant_id, id, name, kind, owner_user_id) \
             VALUES ($1, $2, 'My tasks', 'personal', $3) \
             ON CONFLICT (tenant_id, id) DO NOTHING",
        )
        .bind(self.tenant.as_str())
        .bind(&id)
        .bind(self.user.as_str())
        .execute(&self.pool)
        .await;
        match inserted {
            Ok(_) => {}
            // The tenant was deleted out from under this door (offboarding
            // racing a request): there is nothing to ensure, and every
            // tenant-scoped read correctly comes back empty — a deleted
            // tenant must read as absent, never as an internal error.
            Err(sqlx::Error::Database(ref db)) if db.code().as_deref() == Some("23503") => {}
            Err(e) => return Err(StoreError::Db(e)),
        }
        Ok(ProjectId::new(id))
    }

    /// Creates a team project owned by the caller.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn create_task_project(&self, name: &str, color: Option<&str>) -> Result<ProjectId> {
        let id = ProjectId::generate();
        sqlx::query(
            "INSERT INTO task_projects (tenant_id, id, name, kind, owner_user_id, color) \
             VALUES ($1, $2, $3, 'team', $4, $5)",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(name)
        .bind(self.user.as_str())
        .bind(color)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(id)
    }

    /// Replaces the editable lifecycle facts of a visible team project.
    pub async fn update_task_project(
        &self,
        id: &ProjectId,
        edit: &TaskProjectEdit,
    ) -> Result<TaskProject> {
        let row = sqlx::query_as::<_, ProjectRow>(
            "UPDATE task_projects SET name = $3, description = $4, status = $5, \
                    starts_on = $6, target_on = $7, updated_at = now() \
             WHERE tenant_id = $1 AND id = $2 AND kind = 'team' AND archived = false \
             RETURNING id, name, kind, owner_user_id, color, description, status, \
                       starts_on, target_on, created_at, updated_at",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(&edit.name)
        .bind(&edit.description)
        .bind(&edit.status)
        .bind(edit.starts_on)
        .bind(edit.target_on)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(StoreError::NotFound)?;
        Ok(row.into_project())
    }

    /// Whether the caller can see (and, v1, edit) the project.
    async fn project_visible(&self, project: &ProjectId) -> Result<bool> {
        sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM task_projects p WHERE p.tenant_id = $1 AND p.id = $3 \
               AND p.archived = false \
               AND (p.kind = 'team' OR (p.kind = 'personal' AND p.owner_user_id = $2)))",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(project.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(StoreError::Db)
    }

    // ---- tasks (board/list are groupings of these rows) ------------------

    /// The active tasks on a project, ordered by status then position — the rows
    /// both the board (grouped by status) and the list render (ADR 0022).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn tasks_in_project(&self, project: &ProjectId) -> Result<Vec<Task>> {
        let sql = format!(
            "SELECT {TASK_COLS} FROM tasks t \
             WHERE t.tenant_id = $1 AND t.project_id = $3 AND t.state = 'active' AND {vis} \
             ORDER BY t.status, t.position, t.created_at",
            vis = visible_projects(),
        );
        let rows = sqlx::query_as::<_, TaskRow>(&sql)
            .bind(self.tenant.as_str())
            .bind(self.user.as_str())
            .bind(project.as_str())
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(TaskRow::into_task).collect())
    }

    /// One task by id, if visible to the caller.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn task(&self, id: &TaskId) -> Result<Option<Task>> {
        // Visible if the task's project is visible, OR the task is assigned to
        // the caller — so an assignee can open (and work on) a task even when it
        // lives in someone else's personal project. Tenant-scoped either way.
        let sql = format!(
            "SELECT {TASK_COLS} FROM tasks t \
             WHERE t.tenant_id = $1 AND t.id = $3 AND ({vis} OR t.assignee_user_id = $2)",
            vis = visible_projects(),
        );
        let row = sqlx::query_as::<_, TaskRow>(&sql)
            .bind(self.tenant.as_str())
            .bind(self.user.as_str())
            .bind(id.as_str())
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(TaskRow::into_task))
    }

    /// Creates a task on a project the caller can see, appending it to the end
    /// of its status column. Records a `created` (or `proposed`) activity.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the project isn't visible;
    /// [`StoreError::Db`] on failure.
    pub async fn create_task(&self, project: &ProjectId, new: &NewTask) -> Result<TaskId> {
        if !self.project_visible(project).await? {
            return Err(StoreError::NotFound);
        }
        let id = TaskId::generate();
        let status = new.status.as_deref().unwrap_or("todo");
        let state = new.state.as_deref().unwrap_or("active");
        let priority = new.priority.as_deref().unwrap_or("none");
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        // Append: one past the current max position in this column.
        let next_pos: f64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(position), 0) + 1 FROM tasks \
             WHERE tenant_id = $1 AND project_id = $2 AND status = $3",
        )
        .bind(self.tenant.as_str())
        .bind(project.as_str())
        .bind(status)
        .fetch_one(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        sqlx::query(
            "INSERT INTO tasks (tenant_id, id, project_id, title, description, status, position, \
                 assignee_user_id, due_at, priority, state, source_kind, source_id, created_by) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(project.as_str())
        .bind(&new.title)
        .bind(&new.description)
        .bind(status)
        .bind(next_pos)
        .bind(&new.assignee)
        .bind(new.due_at)
        .bind(priority)
        .bind(state)
        .bind(&new.source_kind)
        .bind(&new.source_id)
        .bind(self.user.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        let kind = if state == "proposed" {
            "proposed"
        } else {
            "created"
        };
        self.record_activity(&mut tx, id.as_str(), kind, json!({}))
            .await?;
        // The creator follows their own real task (not AI proposals).
        if state != "proposed" {
            sqlx::query(
                "INSERT INTO task_followers (tenant_id, task_id, user_id) VALUES ($1, $2, $3) \
                 ON CONFLICT DO NOTHING",
            )
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .bind(self.user.as_str())
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        }
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(id)
    }

    /// Edits a task's core fields (not status/position — that is a move).
    /// Records `assigned` / `due_changed` activities when those change.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the task isn't visible;
    /// [`StoreError::Db`] on failure.
    pub async fn update_task(&self, id: &TaskId, edit: &TaskEdit) -> Result<()> {
        let Some(before) = self.task(id).await? else {
            return Err(StoreError::NotFound);
        };
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        sqlx::query(
            "UPDATE tasks SET title = $3, description = $4, assignee_user_id = $5, \
                    due_at = $6, priority = $7, updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(&edit.title)
        .bind(&edit.description)
        .bind(&edit.assignee)
        .bind(edit.due_at)
        .bind(&edit.priority)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        if before.assignee != edit.assignee {
            self.record_activity(
                &mut tx,
                id.as_str(),
                "assigned",
                json!({ "to": edit.assignee }),
            )
            .await?;
        }
        if before.due_at != edit.due_at {
            self.record_activity(&mut tx, id.as_str(), "due_changed", json!({}))
                .await?;
        }
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(())
    }

    /// Moves a task to a status/position — the one operation both a board drag
    /// and a list status-change go through (ADR 0022). Sets `completed_at` when
    /// entering/leaving the `done` column, and records a `status_changed`
    /// activity when the column changed.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the task isn't visible;
    /// [`StoreError::Db`] on failure.
    pub async fn move_task(&self, id: &TaskId, status: &str, position: f64) -> Result<()> {
        let Some(before) = self.task(id).await? else {
            return Err(StoreError::NotFound);
        };
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        sqlx::query(
            "UPDATE tasks SET status = $3, position = $4, \
                    completed_at = CASE WHEN $3 = 'done' THEN COALESCE(completed_at, now()) \
                                        ELSE NULL END, \
                    updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(status)
        .bind(position)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        if before.status != status {
            self.record_activity(
                &mut tx,
                id.as_str(),
                "status_changed",
                json!({ "from": before.status, "to": status }),
            )
            .await?;
        }
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(())
    }

    /// Deletes a task and all its children.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the task isn't visible;
    /// [`StoreError::Db`] on failure.
    pub async fn delete_task(&self, id: &TaskId) -> Result<()> {
        if self.task(id).await?.is_none() {
            return Err(StoreError::NotFound);
        }
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        for table in [
            "task_subtasks",
            "task_comments",
            "task_activity",
            "task_attachments",
        ] {
            sqlx::query(&format!(
                "DELETE FROM {table} WHERE tenant_id = $1 AND task_id = $2"
            ))
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        }
        sqlx::query("DELETE FROM tasks WHERE tenant_id = $1 AND id = $2")
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(())
    }

    // ---- the connections: my-plate + calendar-readable due tasks ---------

    /// "What's on my plate": the caller's active, assigned tasks that are due on
    /// or before end-of-day `today_end` (overdue + due today), earliest first.
    /// The tasks half of the aggregate the AI assembles (calendar + mail join
    /// later).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn my_plate(&self, today_end: OffsetDateTime) -> Result<Vec<Task>> {
        let sql = format!(
            "SELECT {TASK_COLS} FROM tasks t \
             WHERE t.tenant_id = $1 AND t.state = 'active' AND t.status <> 'done' \
               AND t.assignee_user_id = $2 AND t.due_at IS NOT NULL AND t.due_at <= $3 \
             ORDER BY t.due_at",
        );
        let rows = sqlx::query_as::<_, TaskRow>(&sql)
            .bind(self.tenant.as_str())
            .bind(self.user.as_str())
            .bind(today_end)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(TaskRow::into_task).collect())
    }

    /// Active tasks with a due date in `[from, to)`, visible to the caller — what
    /// the calendar overlays alongside events (ADR 0021).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn due_tasks_in_range(
        &self,
        from: OffsetDateTime,
        to: OffsetDateTime,
    ) -> Result<Vec<Task>> {
        let sql = format!(
            "SELECT {TASK_COLS} FROM tasks t \
             WHERE t.tenant_id = $1 AND t.state = 'active' AND t.due_at >= $3 AND t.due_at < $4 \
               AND {vis} \
             ORDER BY t.due_at",
            vis = visible_projects(),
        );
        let rows = sqlx::query_as::<_, TaskRow>(&sql)
            .bind(self.tenant.as_str())
            .bind(self.user.as_str())
            .bind(from)
            .bind(to)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(TaskRow::into_task).collect())
    }

    /// Active tasks that came from one source record, visible to the caller —
    /// ADR 0021's source link read backwards, so the record a task came *from*
    /// can show what is still to be done about it.
    ///
    /// Visibility is a task's own rule and is applied here rather than by the
    /// caller: a task on a colleague's **personal** project is theirs, and it
    /// appears for somebody else only when it is assigned to them. A source
    /// record that is tenant-wide (a CRM deal) therefore shows each reader the
    /// next steps that are actually theirs to see — never a list that leaks the
    /// contents of a private board.
    ///
    /// Ordered as work is read: unfinished first, then by due date (undated
    /// last), then oldest first. Bounded by [`SOURCE_TASKS_MAX`], because the
    /// caller renders it whole.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn tasks_for_source(&self, kind: &str, source_id: &str) -> Result<Vec<Task>> {
        let sql = format!(
            "SELECT {TASK_COLS} FROM tasks t \
             WHERE t.tenant_id = $1 AND t.state = 'active' \
               AND t.source_kind = $3 AND t.source_id = $4 \
               AND ({vis} OR t.assignee_user_id = $2) \
             ORDER BY (t.completed_at IS NOT NULL), t.due_at NULLS LAST, t.created_at, t.id \
             LIMIT {SOURCE_TASKS_MAX}",
            vis = visible_projects(),
        );
        let rows = sqlx::query_as::<_, TaskRow>(&sql)
            .bind(self.tenant.as_str())
            .bind(self.user.as_str())
            .bind(kind)
            .bind(source_id)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(TaskRow::into_task).collect())
    }

    // ---- propose-then-approve (ADR 0023) ---------------------------------

    /// The caller's pending AI proposals (tasks in `state = 'proposed'`),
    /// newest first — awaiting accept/reject.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn task_proposals(&self) -> Result<Vec<Task>> {
        let sql = format!(
            "SELECT {TASK_COLS} FROM tasks t \
             WHERE t.tenant_id = $1 AND t.state = 'proposed' AND {vis} \
             ORDER BY t.created_at DESC",
            vis = visible_projects(),
        );
        let rows = sqlx::query_as::<_, TaskRow>(&sql)
            .bind(self.tenant.as_str())
            .bind(self.user.as_str())
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(TaskRow::into_task).collect())
    }

    /// Accepts a proposed task: flips it to `active` and records who approved it.
    /// Optional edits refine the AI's suggested assignee/due/priority first.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when it isn't a visible proposal;
    /// [`StoreError::Db`] on failure.
    pub async fn accept_task(&self, id: &TaskId, edit: Option<&TaskEdit>) -> Result<()> {
        let Some(task) = self.task(id).await? else {
            return Err(StoreError::NotFound);
        };
        if task.state != "proposed" {
            return Err(StoreError::NotFound);
        }
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        if let Some(e) = edit {
            sqlx::query(
                "UPDATE tasks SET state = 'active', title = $3, description = $4, \
                        assignee_user_id = $5, due_at = $6, priority = $7, updated_at = now() \
                 WHERE tenant_id = $1 AND id = $2",
            )
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .bind(&e.title)
            .bind(&e.description)
            .bind(&e.assignee)
            .bind(e.due_at)
            .bind(&e.priority)
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        } else {
            sqlx::query(
                "UPDATE tasks SET state = 'active', updated_at = now() \
                 WHERE tenant_id = $1 AND id = $2",
            )
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        }
        self.record_activity(&mut tx, id.as_str(), "accepted", json!({}))
            .await?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(())
    }

    /// Rejects a proposed task (deletes it). A no-op-safe delete of a proposal.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when it isn't a visible proposal;
    /// [`StoreError::Db`] on failure.
    pub async fn reject_task(&self, id: &TaskId) -> Result<()> {
        match self.task(id).await? {
            Some(t) if t.state == "proposed" => self.delete_task(id).await,
            _ => Err(StoreError::NotFound),
        }
    }

    // ---- subtasks / comments / activity ----------------------------------

    /// A task's checklist items, ordered.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn subtasks(&self, task: &TaskId) -> Result<Vec<Subtask>> {
        let rows = sqlx::query_as::<_, SubtaskRow>(
            "SELECT id, title, done, position FROM task_subtasks \
             WHERE tenant_id = $1 AND task_id = $2 ORDER BY position, created_at",
        )
        .bind(self.tenant.as_str())
        .bind(task.as_str())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(SubtaskRow::into_subtask).collect())
    }

    /// Adds a checklist item to a visible task.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the task isn't visible;
    /// [`StoreError::Db`] on failure.
    pub async fn add_subtask(&self, task: &TaskId, title: &str) -> Result<SubtaskId> {
        if self.task(task).await?.is_none() {
            return Err(StoreError::NotFound);
        }
        let id = SubtaskId::generate();
        let next_pos: f64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(position), 0) + 1 FROM task_subtasks \
             WHERE tenant_id = $1 AND task_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(task.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        sqlx::query(
            "INSERT INTO task_subtasks (tenant_id, id, task_id, title, position) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(task.as_str())
        .bind(title)
        .bind(next_pos)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(id)
    }

    /// Sets a checklist item's done state.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn set_subtask_done(
        &self,
        task: &TaskId,
        subtask: &SubtaskId,
        done: bool,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE task_subtasks SET done = $4 \
             WHERE tenant_id = $1 AND task_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(task.as_str())
        .bind(subtask.as_str())
        .bind(done)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }

    /// Deletes a checklist item.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn delete_subtask(&self, task: &TaskId, subtask: &SubtaskId) -> Result<()> {
        sqlx::query("DELETE FROM task_subtasks WHERE tenant_id = $1 AND task_id = $2 AND id = $3")
            .bind(self.tenant.as_str())
            .bind(task.as_str())
            .bind(subtask.as_str())
            .execute(&self.pool)
            .await
            .map_err(StoreError::Db)?;
        Ok(())
    }

    /// A task's comments, oldest first.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn task_comments(&self, task: &TaskId) -> Result<Vec<TaskComment>> {
        let rows = sqlx::query_as::<_, CommentRow>(
            "SELECT id, author_user_id, body, created_at FROM task_comments \
             WHERE tenant_id = $1 AND task_id = $2 ORDER BY created_at",
        )
        .bind(self.tenant.as_str())
        .bind(task.as_str())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(CommentRow::into_comment).collect())
    }

    /// Adds a comment to a visible task (records a `commented` activity).
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the task isn't visible;
    /// [`StoreError::Db`] on failure.
    pub async fn add_task_comment(&self, task: &TaskId, body: &str) -> Result<CommentId> {
        if self.task(task).await?.is_none() {
            return Err(StoreError::NotFound);
        }
        let id = CommentId::generate();
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        sqlx::query(
            "INSERT INTO task_comments (tenant_id, id, task_id, author_user_id, body) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(task.as_str())
        .bind(self.user.as_str())
        .bind(body)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        self.record_activity(&mut tx, task.as_str(), "commented", json!({}))
            .await?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(id)
    }

    /// The files attached to a visible task (a reference to a tenant blob each).
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the task isn't visible to the caller;
    /// [`StoreError::Db`] on failure.
    pub async fn task_attachments(&self, task: &TaskId) -> Result<Vec<TaskAttachment>> {
        if self.task(task).await?.is_none() {
            return Err(StoreError::NotFound);
        }
        let rows = sqlx::query_as::<_, AttachmentRow>(
            "SELECT id, blob_id, filename, size, created_at FROM task_attachments \
             WHERE tenant_id = $1 AND task_id = $2 ORDER BY created_at",
        )
        .bind(self.tenant.as_str())
        .bind(task.as_str())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(AttachmentRow::into_attachment)
            .collect())
    }

    /// Attaches an already-uploaded blob to a visible task.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the task isn't visible to the caller;
    /// [`StoreError::Db`] on failure.
    pub async fn add_task_attachment(
        &self,
        task: &TaskId,
        blob_id: &str,
        filename: &str,
        size: i64,
    ) -> Result<AttachmentId> {
        if self.task(task).await?.is_none() {
            return Err(StoreError::NotFound);
        }
        let id = AttachmentId::generate();
        sqlx::query(
            "INSERT INTO task_attachments (tenant_id, id, task_id, blob_id, filename, size) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(task.as_str())
        .bind(blob_id)
        .bind(filename)
        .bind(size)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(id)
    }

    /// Removes an attachment from a visible task (the blob itself is left in the
    /// store; task attachments are references, and a blob may be shared).
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the task isn't visible;
    /// [`StoreError::Db`] on failure.
    pub async fn delete_task_attachment(
        &self,
        task: &TaskId,
        attachment: &AttachmentId,
    ) -> Result<()> {
        if self.task(task).await?.is_none() {
            return Err(StoreError::NotFound);
        }
        sqlx::query(
            "DELETE FROM task_attachments \
             WHERE tenant_id = $1 AND task_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(task.as_str())
        .bind(attachment.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }

    /// Every attachment across the tasks of a project the caller can see — the
    /// project-wide "Files" roll-up. Scoped by the same project-visibility rule
    /// as the task lists, so a personal project's files stay private to its owner
    /// and nothing crosses tenants.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn project_files(&self, project: &ProjectId) -> Result<Vec<ProjectFile>> {
        let sql = format!(
            "SELECT a.id, a.task_id, t.title AS task_title, a.blob_id, a.filename, a.size, \
                    a.created_at \
             FROM task_attachments a \
             JOIN tasks t ON t.tenant_id = a.tenant_id AND t.id = a.task_id \
             WHERE a.tenant_id = $1 AND t.project_id = $3 AND {vis} \
             ORDER BY a.created_at DESC",
            vis = visible_projects(),
        );
        let rows = sqlx::query_as::<_, ProjectFileRow>(&sql)
            .bind(self.tenant.as_str())
            .bind(self.user.as_str())
            .bind(project.as_str())
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(ProjectFileRow::into_file).collect())
    }

    /// Every label defined in the tenant (reusable across tasks), by name.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn task_labels(&self) -> Result<Vec<TaskLabel>> {
        let rows = sqlx::query_as::<_, LabelRow>(
            "SELECT id, name, color FROM task_labels WHERE tenant_id = $1 ORDER BY name",
        )
        .bind(self.tenant.as_str())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(LabelRow::into_label).collect())
    }

    /// Creates a tenant label. Names aren't forced unique (two "Design"s are
    /// allowed); the UI dedups by offering existing ones first.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn create_task_label(&self, name: &str, color: Option<&str>) -> Result<LabelId> {
        let id = LabelId::generate();
        sqlx::query("INSERT INTO task_labels (tenant_id, id, name, color) VALUES ($1, $2, $3, $4)")
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .bind(name)
            .bind(color)
            .execute(&self.pool)
            .await
            .map_err(StoreError::Db)?;
        Ok(id)
    }

    /// Deletes a tenant label and unlinks it from every task (one transaction).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn delete_task_label(&self, label: &LabelId) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        sqlx::query("DELETE FROM task_label_links WHERE tenant_id = $1 AND label_id = $2")
            .bind(self.tenant.as_str())
            .bind(label.as_str())
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        sqlx::query("DELETE FROM task_labels WHERE tenant_id = $1 AND id = $2")
            .bind(self.tenant.as_str())
            .bind(label.as_str())
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(())
    }

    /// The labels on a visible task.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the task isn't visible; [`StoreError::Db`].
    pub async fn labels_for_task(&self, task: &TaskId) -> Result<Vec<TaskLabel>> {
        if self.task(task).await?.is_none() {
            return Err(StoreError::NotFound);
        }
        let rows = sqlx::query_as::<_, LabelRow>(
            "SELECT tl.id, tl.name, tl.color FROM task_label_links l \
             JOIN task_labels tl ON tl.tenant_id = l.tenant_id AND tl.id = l.label_id \
             WHERE l.tenant_id = $1 AND l.task_id = $2 ORDER BY tl.name",
        )
        .bind(self.tenant.as_str())
        .bind(task.as_str())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(LabelRow::into_label).collect())
    }

    /// Attaches a tenant label to a visible task (idempotent).
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the task isn't visible or the label isn't
    /// this tenant's; [`StoreError::Db`].
    pub async fn add_task_label(&self, task: &TaskId, label: &LabelId) -> Result<()> {
        if self.task(task).await?.is_none() {
            return Err(StoreError::NotFound);
        }
        let exists: Option<String> =
            sqlx::query_scalar("SELECT id FROM task_labels WHERE tenant_id = $1 AND id = $2")
                .bind(self.tenant.as_str())
                .bind(label.as_str())
                .fetch_optional(&self.pool)
                .await
                .map_err(StoreError::Db)?;
        if exists.is_none() {
            return Err(StoreError::NotFound);
        }
        sqlx::query(
            "INSERT INTO task_label_links (tenant_id, task_id, label_id) VALUES ($1, $2, $3) \
             ON CONFLICT DO NOTHING",
        )
        .bind(self.tenant.as_str())
        .bind(task.as_str())
        .bind(label.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }

    /// Removes a label from a visible task.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the task isn't visible; [`StoreError::Db`].
    pub async fn remove_task_label(&self, task: &TaskId, label: &LabelId) -> Result<()> {
        if self.task(task).await?.is_none() {
            return Err(StoreError::NotFound);
        }
        sqlx::query(
            "DELETE FROM task_label_links WHERE tenant_id = $1 AND task_id = $2 AND label_id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(task.as_str())
        .bind(label.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }

    /// Labels for a batch of task ids (tenant-scoped), grouped by task id — for
    /// stamping chips onto a task list. Callers pass ids they already resolved
    /// as visible, so this only reads within the tenant.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn labels_for_task_ids(
        &self,
        task_ids: &[String],
    ) -> Result<std::collections::HashMap<String, Vec<TaskLabel>>> {
        let mut map: std::collections::HashMap<String, Vec<TaskLabel>> =
            std::collections::HashMap::new();
        if task_ids.is_empty() {
            return Ok(map);
        }
        let rows = sqlx::query_as::<_, TaskLabelRow>(
            "SELECT l.task_id, tl.id, tl.name, tl.color FROM task_label_links l \
             JOIN task_labels tl ON tl.tenant_id = l.tenant_id AND tl.id = l.label_id \
             WHERE l.tenant_id = $1 AND l.task_id = ANY($2) ORDER BY tl.name",
        )
        .bind(self.tenant.as_str())
        .bind(task_ids)
        .fetch_all(&self.pool)
        .await?;
        for r in rows {
            map.entry(r.task_id).or_default().push(TaskLabel {
                id: LabelId::new(r.id),
                name: r.name,
                color: r.color,
            });
        }
        Ok(map)
    }

    /// The user ids following a visible task.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the task isn't visible; [`StoreError::Db`].
    pub async fn task_followers(&self, task: &TaskId) -> Result<Vec<String>> {
        if self.task(task).await?.is_none() {
            return Err(StoreError::NotFound);
        }
        let ids: Vec<String> = sqlx::query_scalar(
            "SELECT user_id FROM task_followers WHERE tenant_id = $1 AND task_id = $2 \
             ORDER BY created_at",
        )
        .bind(self.tenant.as_str())
        .bind(task.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(ids)
    }

    /// The caller follows a visible task (idempotent).
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the task isn't visible; [`StoreError::Db`].
    pub async fn follow_task(&self, task: &TaskId) -> Result<()> {
        if self.task(task).await?.is_none() {
            return Err(StoreError::NotFound);
        }
        sqlx::query(
            "INSERT INTO task_followers (tenant_id, task_id, user_id) VALUES ($1, $2, $3) \
             ON CONFLICT DO NOTHING",
        )
        .bind(self.tenant.as_str())
        .bind(task.as_str())
        .bind(self.user.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }

    /// The caller stops following a visible task.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the task isn't visible; [`StoreError::Db`].
    pub async fn unfollow_task(&self, task: &TaskId) -> Result<()> {
        if self.task(task).await?.is_none() {
            return Err(StoreError::NotFound);
        }
        sqlx::query(
            "DELETE FROM task_followers WHERE tenant_id = $1 AND task_id = $2 AND user_id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(task.as_str())
        .bind(self.user.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }

    /// The tasks a visible task is blocked by ("blocked by" list), each itself
    /// visible to the caller. A blocker that has become invisible is silently
    /// omitted rather than leaking its existence.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the task isn't visible; [`StoreError::Db`].
    pub async fn dependencies(&self, task: &TaskId) -> Result<Vec<TaskDepRef>> {
        if self.task(task).await?.is_none() {
            return Err(StoreError::NotFound);
        }
        let sql = format!(
            "SELECT t.id, t.title, t.status FROM task_dependencies d \
             JOIN tasks t ON t.tenant_id = d.tenant_id AND t.id = d.depends_on_task_id \
             WHERE d.tenant_id = $1 AND d.task_id = $3 AND ({vis}) \
             ORDER BY d.created_at",
            vis = visible_projects(),
        );
        let rows = sqlx::query_as::<_, DepRow>(&sql)
            .bind(self.tenant.as_str())
            .bind(self.user.as_str())
            .bind(task.as_str())
            .fetch_all(&self.pool)
            .await
            .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(DepRow::into_ref).collect())
    }

    /// Records that `task` is blocked by `depends_on` (idempotent). Both tasks
    /// must be visible to the caller, so a dependency can never point at another
    /// tenant's — or another user's private — task.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when either task isn't visible;
    /// [`StoreError::Conflict`] when a task is made to depend on itself;
    /// [`StoreError::Db`] on failure.
    pub async fn add_dependency(&self, task: &TaskId, depends_on: &TaskId) -> Result<()> {
        if task.as_str() == depends_on.as_str() {
            return Err(StoreError::Conflict(
                "a task cannot depend on itself".into(),
            ));
        }
        if self.task(task).await?.is_none() || self.task(depends_on).await?.is_none() {
            return Err(StoreError::NotFound);
        }
        sqlx::query(
            "INSERT INTO task_dependencies (tenant_id, task_id, depends_on_task_id) \
             VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
        )
        .bind(self.tenant.as_str())
        .bind(task.as_str())
        .bind(depends_on.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }

    /// Removes the "blocked by" edge from `task` to `depends_on`.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the task isn't visible; [`StoreError::Db`].
    pub async fn remove_dependency(&self, task: &TaskId, depends_on: &TaskId) -> Result<()> {
        if self.task(task).await?.is_none() {
            return Err(StoreError::NotFound);
        }
        sqlx::query(
            "DELETE FROM task_dependencies \
             WHERE tenant_id = $1 AND task_id = $2 AND depends_on_task_id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(task.as_str())
        .bind(depends_on.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }

    /// Every dependency edge among the visible tasks of a project, as
    /// `(blocked_task, blocking_task)` pairs — the Timeline's arrow set. Both
    /// endpoints are filtered by project visibility, so nothing crosses tenants
    /// and a personal project's edges stay private to its owner.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn project_dependencies(&self, project: &ProjectId) -> Result<Vec<(TaskId, TaskId)>> {
        let sql = format!(
            "SELECT d.task_id, d.depends_on_task_id FROM task_dependencies d \
             JOIN tasks t ON t.tenant_id = d.tenant_id AND t.id = d.task_id \
             JOIN tasks b ON b.tenant_id = d.tenant_id AND b.id = d.depends_on_task_id \
             WHERE d.tenant_id = $1 AND t.project_id = $3 AND b.project_id = $3 AND ({vis}) \
             ORDER BY d.created_at",
            vis = visible_projects(),
        );
        let rows = sqlx::query_as::<_, (String, String)>(&sql)
            .bind(self.tenant.as_str())
            .bind(self.user.as_str())
            .bind(project.as_str())
            .fetch_all(&self.pool)
            .await
            .map_err(StoreError::Db)?;
        Ok(rows
            .into_iter()
            .map(|(t, b)| (TaskId::new(t), TaskId::new(b)))
            .collect())
    }

    /// A task's activity history, newest first.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn task_activity(&self, task: &TaskId) -> Result<Vec<TaskActivity>> {
        let rows = sqlx::query_as::<_, ActivityRow>(
            "SELECT actor_user_id, kind, detail, created_at FROM task_activity \
             WHERE tenant_id = $1 AND task_id = $2 ORDER BY created_at DESC",
        )
        .bind(self.tenant.as_str())
        .bind(task.as_str())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(ActivityRow::into_activity).collect())
    }

    /// Records one activity row inside an open transaction.
    async fn record_activity(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        task_id: &str,
        kind: &str,
        detail: serde_json::Value,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO task_activity (tenant_id, id, task_id, actor_user_id, kind, detail) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(self.tenant.as_str())
        .bind(crate::id::CommentId::generate().as_str())
        .bind(task_id)
        .bind(self.user.as_str())
        .bind(kind)
        .bind(sqlx::types::Json(detail))
        .execute(&mut **tx)
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }
}

// ---- row types --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct ProjectRow {
    id: String,
    name: String,
    kind: String,
    owner_user_id: String,
    color: Option<String>,
    description: Option<String>,
    status: String,
    starts_on: Option<Date>,
    target_on: Option<Date>,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}
impl ProjectRow {
    fn into_project(self) -> TaskProject {
        TaskProject {
            id: ProjectId::new(self.id),
            name: self.name,
            kind: self.kind,
            owner: self.owner_user_id,
            color: self.color,
            description: self.description,
            status: self.status,
            starts_on: self.starts_on,
            target_on: self.target_on,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct TaskRow {
    id: String,
    project_id: String,
    title: String,
    description: Option<String>,
    status: String,
    position: f64,
    assignee_user_id: Option<String>,
    due_at: Option<OffsetDateTime>,
    priority: String,
    state: String,
    source_kind: Option<String>,
    source_id: Option<String>,
    created_by: String,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
    completed_at: Option<OffsetDateTime>,
    subtask_done: i64,
    subtask_total: i64,
    comment_count: i64,
}
impl TaskRow {
    fn into_task(self) -> Task {
        Task {
            id: TaskId::new(self.id),
            project_id: ProjectId::new(self.project_id),
            title: self.title,
            description: self.description,
            status: self.status,
            position: self.position,
            assignee: self.assignee_user_id,
            due_at: self.due_at,
            priority: self.priority,
            state: self.state,
            source_kind: self.source_kind,
            source_id: self.source_id,
            created_by: self.created_by,
            created_at: self.created_at,
            updated_at: self.updated_at,
            completed_at: self.completed_at,
            subtask_done: self.subtask_done,
            subtask_total: self.subtask_total,
            comment_count: self.comment_count,
        }
    }
}

#[derive(sqlx::FromRow)]
struct SubtaskRow {
    id: String,
    title: String,
    done: bool,
    position: f64,
}
impl SubtaskRow {
    fn into_subtask(self) -> Subtask {
        Subtask {
            id: SubtaskId::new(self.id),
            title: self.title,
            done: self.done,
            position: self.position,
        }
    }
}

#[derive(sqlx::FromRow)]
struct CommentRow {
    id: String,
    author_user_id: String,
    body: String,
    created_at: OffsetDateTime,
}
impl CommentRow {
    fn into_comment(self) -> TaskComment {
        TaskComment {
            id: CommentId::new(self.id),
            author: self.author_user_id,
            body: self.body,
            created_at: self.created_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct AttachmentRow {
    id: String,
    blob_id: String,
    filename: String,
    size: i64,
    created_at: OffsetDateTime,
}
impl AttachmentRow {
    fn into_attachment(self) -> TaskAttachment {
        TaskAttachment {
            id: AttachmentId::new(self.id),
            blob_id: self.blob_id,
            filename: self.filename,
            size: self.size,
            created_at: self.created_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct ProjectFileRow {
    id: String,
    task_id: String,
    task_title: String,
    blob_id: String,
    filename: String,
    size: i64,
    created_at: OffsetDateTime,
}
impl ProjectFileRow {
    fn into_file(self) -> ProjectFile {
        ProjectFile {
            id: AttachmentId::new(self.id),
            task_id: self.task_id,
            task_title: self.task_title,
            blob_id: self.blob_id,
            filename: self.filename,
            size: self.size,
            created_at: self.created_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct LabelRow {
    id: String,
    name: String,
    color: Option<String>,
}
impl LabelRow {
    fn into_label(self) -> TaskLabel {
        TaskLabel {
            id: LabelId::new(self.id),
            name: self.name,
            color: self.color,
        }
    }
}

#[derive(sqlx::FromRow)]
struct TaskLabelRow {
    task_id: String,
    id: String,
    name: String,
    color: Option<String>,
}

#[derive(sqlx::FromRow)]
struct DepRow {
    id: String,
    title: String,
    status: String,
}
impl DepRow {
    fn into_ref(self) -> TaskDepRef {
        TaskDepRef {
            id: TaskId::new(self.id),
            title: self.title,
            status: self.status,
        }
    }
}

#[derive(sqlx::FromRow)]
struct ActivityRow {
    actor_user_id: String,
    kind: String,
    detail: sqlx::types::Json<serde_json::Value>,
    created_at: OffsetDateTime,
}
impl ActivityRow {
    fn into_activity(self) -> TaskActivity {
        TaskActivity {
            actor: self.actor_user_id,
            kind: self.kind,
            detail: self.detail.0,
            created_at: self.created_at,
        }
    }
}
