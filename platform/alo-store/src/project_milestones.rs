//! Milestones — the named dates a project plan is made of, and the link that
//! puts one task under one of them (alo Projects, ADR 0035, wave B3.09),
//! reached through the account door like every other business record.
//!
//! A milestone is a **named date on a project**: "Design signed off,
//! 30 September". That is the whole model. The timeline a screen draws is a
//! *rendering* of these rows over the board that already exists
//! (`docs/design/projects.md`, "Milestones and templates") — the tasks on it
//! are the same [`crate::tasks`] rows the board shows, read through
//! `task_milestones`, so there is no second list of work to drift.
//!
//! Four rules, each of which is a decision rather than an implementation
//! detail:
//!
//! - **`tasks.rs` is untouched.** The link is a side table keyed on the task,
//!   exactly as `project_clients` is keyed on the project (law 3). Its primary
//!   key is `task_id`, so "which milestone is this task in" has exactly one
//!   answer — a task under two milestones is a plan that cannot be drawn.
//! - **A milestone is done when a human says so**, never when its tasks are. A
//!   plan whose states move themselves is a plan nobody trusts, and "the last
//!   task closed" is not the statement "the client accepted the deliverable".
//!   The task counts this module reports are *information beside* the flag,
//!   never the flag itself.
//! - **A task can only be placed under a milestone of its own project.** The
//!   alternative — a plan reaching across boards — would put a date on work
//!   that a different project's timeline also claims.
//! - **Any project the caller can see may carry a plan**, team or their own
//!   personal board. Unlike client facts (B3.02) a milestone is not a claim
//!   about money or somebody else's approval, so there is no reason to withhold
//!   it from a private board.

use time::{Date, OffsetDateTime};

use crate::account::AccountStore;
use crate::billing_field::required;
use crate::error::{Result, StoreError};
use crate::id::{ProjectId, ProjectMilestoneId, TaskId};

/// The most milestones one project may carry: a plan a human reads, not a
/// generated schedule. Over it the store refuses rather than truncates.
pub const MILESTONES_MAX: i64 = 200;

/// Longest milestone name, in characters. A milestone is named like a heading
/// ("Beta with the pilot customer"), not written like a note.
pub const NAME_MAX: usize = 120;

/// The `task_projects.kind` that is visible tenant-wide.
const TEAM_KIND: &str = "team";

/// The columns every read of a milestone selects, in `MilestoneRow` order.
/// The two counts come from the link table and the tasks it points at, so one
/// read answers "what is this milestone and how is it going".
const MILESTONE_COLS: &str = "m.id, m.project_id, m.name, m.due_on, m.done_at, m.position, \
     m.created_by, m.created_at, m.updated_at, \
     (SELECT count(*) FROM task_milestones l \
        WHERE l.tenant_id = m.tenant_id AND l.milestone_id = m.id) AS task_count, \
     (SELECT count(*) FROM task_milestones l JOIN tasks t \
          ON t.tenant_id = l.tenant_id AND t.id = l.task_id \
        WHERE l.tenant_id = m.tenant_id AND l.milestone_id = m.id \
          AND t.completed_at IS NOT NULL) AS task_done_count";

/// The writable shape of a milestone: a name and the day it falls on.
#[derive(Debug, Clone)]
pub struct NewMilestone {
    /// What the date is for, in the tenant's own words.
    pub name: String,
    /// The day itself. Required — a milestone without a date is a label, and
    /// the timeline has nowhere to draw it.
    pub due_on: Date,
}

/// The editable fields of an existing milestone. Whether it is *done* is not
/// among them: reaching a milestone is [`AccountStore::set_milestone_done`], a
/// separate act with its own audit line, not a field somebody can flip while
/// correcting a spelling.
#[derive(Debug, Clone)]
pub struct MilestoneEdit {
    /// The new name.
    pub name: String,
    /// The new day. Moving a milestone is ordinary — a plan that cannot be
    /// re-planned is a plan that gets kept in a spreadsheet instead.
    pub due_on: Date,
}

/// One milestone, with what the timeline needs to draw it.
#[derive(Debug, Clone)]
pub struct Milestone {
    /// The milestone's id.
    pub id: ProjectMilestoneId,
    /// The board it belongs to.
    pub project_id: ProjectId,
    /// What the date is for.
    pub name: String,
    /// The day it falls on.
    pub due_on: Date,
    /// When a human marked it reached, or `None` while it is still ahead.
    pub done_at: Option<OffsetDateTime>,
    /// Tie-break within one day, assigned on create.
    pub position: i64,
    /// Who planned it.
    pub created_by: String,
    /// When it was planned.
    pub created_at: OffsetDateTime,
    /// When it was last edited or reached.
    pub updated_at: OffsetDateTime,
    /// How many tasks are placed under it.
    pub task_count: i64,
    /// How many of those are completed. Information beside the flag, never the
    /// flag: a milestone whose tasks are all closed is still not reached until
    /// somebody says it is.
    pub task_done_count: i64,
}

impl Milestone {
    /// Whether a human has marked this milestone reached.
    pub fn is_done(&self) -> bool {
        self.done_at.is_some()
    }

    /// Whether the day has passed without the milestone being reached, as at
    /// `today`. A *late* milestone is one nobody has closed and whose date is
    /// behind us — a done milestone is never late, however late it was closed,
    /// because the timeline reports the plan's state today and not its history.
    pub fn is_late(&self, today: Date) -> bool {
        self.done_at.is_none() && self.due_on < today
    }
}

/// One task's place in the plan: which milestone it sits under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskPlacement {
    /// The task.
    pub task_id: TaskId,
    /// The milestone it is placed under.
    pub milestone_id: ProjectMilestoneId,
}

/// Validates a milestone's writable fields. Pure — no database, so the rules
/// are unit-tested directly.
fn normalize_name(name: &str) -> Result<String> {
    required("milestone name", name, NAME_MAX)
}

impl AccountStore {
    /// Plans a milestone on a project.
    ///
    /// The new milestone takes the next `position` within the project, so two
    /// milestones planned for the same day keep the order they were planned in
    /// rather than an accidental one.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the project is not one this caller can see
    /// — another tenant's or a colleague's personal board, the same answer;
    /// [`StoreError::Validation`] when the name is blank or too long, when the
    /// project is archived, or when the project already carries
    /// [`MILESTONES_MAX`] milestones; [`StoreError::Db`] on failure.
    pub async fn create_milestone(
        &self,
        project: &ProjectId,
        new: &NewMilestone,
    ) -> Result<Milestone> {
        self.require_planable_project(project).await?;
        let name = normalize_name(&new.name)?;
        let id = ProjectMilestoneId::generate();
        // One statement, so the cap is checked against the same snapshot the
        // insert writes into: `INSERT … SELECT … WHERE (count) < max` inserts
        // no row when the plan is full, and `fetch_optional` turns that into
        // the refusal below rather than a row that quietly exceeded it.
        let inserted = sqlx::query_scalar::<_, String>(
            "INSERT INTO project_milestones \
                 (tenant_id, id, project_id, name, due_on, position, created_by) \
             SELECT $1, $2, $3, $4, $5, \
                 coalesce((SELECT max(position) + 1 FROM project_milestones \
                     WHERE tenant_id = $1 AND project_id = $3), 0), $6 \
             WHERE (SELECT count(*) FROM project_milestones \
                 WHERE tenant_id = $1 AND project_id = $3) < $7 \
             RETURNING id",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(project.as_str())
        .bind(&name)
        .bind(new.due_on)
        .bind(self.user.as_str())
        .bind(MILESTONES_MAX)
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if inserted.is_none() {
            return Err(StoreError::Validation(format!(
                "a project carries at most {MILESTONES_MAX} milestones"
            )));
        }
        self.milestone(&id).await?.ok_or(StoreError::NotFound)
    }

    /// One project's plan, earliest date first — the rows a timeline is drawn
    /// from.
    ///
    /// Empty is a real answer (a project with no plan), and so is empty for a
    /// project this caller cannot see: existence is never disclosed by the
    /// shape of a list.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn milestones(&self, project: &ProjectId) -> Result<Vec<Milestone>> {
        let rows = sqlx::query_as::<_, MilestoneRow>(&format!(
            "SELECT {MILESTONE_COLS} FROM project_milestones m \
             WHERE m.tenant_id = $1 AND m.project_id = $2 \
               AND {} \
             ORDER BY m.due_on, m.position, m.id",
            visible_project_predicate("m.project_id", "$3")
        ))
        .bind(self.tenant.as_str())
        .bind(project.as_str())
        .bind(self.user.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(MilestoneRow::into_milestone).collect())
    }

    /// One milestone, or `None` when it is not one this caller can see —
    /// including when the id belongs to another tenant, which is
    /// indistinguishable by design.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn milestone(&self, id: &ProjectMilestoneId) -> Result<Option<Milestone>> {
        let row = sqlx::query_as::<_, MilestoneRow>(&format!(
            "SELECT {MILESTONE_COLS} FROM project_milestones m \
             WHERE m.tenant_id = $1 AND m.id = $2 AND {}",
            visible_project_predicate("m.project_id", "$3")
        ))
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(self.user.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(row.map(MilestoneRow::into_milestone))
    }

    /// Renames a milestone or moves its date.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the milestone is not one this caller can
    /// see; [`StoreError::Validation`] when the name is blank or too long;
    /// [`StoreError::Db`] on failure.
    pub async fn update_milestone(
        &self,
        id: &ProjectMilestoneId,
        edit: &MilestoneEdit,
    ) -> Result<Milestone> {
        let name = normalize_name(&edit.name)?;
        let done = sqlx::query(&format!(
            "UPDATE project_milestones m SET name = $3, due_on = $4, updated_at = now() \
             WHERE m.tenant_id = $1 AND m.id = $2 AND {}",
            visible_project_predicate("m.project_id", "$5")
        ))
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(&name)
        .bind(edit.due_on)
        .bind(self.user.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        self.milestone(id).await?.ok_or(StoreError::NotFound)
    }

    /// Marks a milestone reached, or puts it back ahead of us.
    ///
    /// Idempotent in both directions, and marking an already-reached milestone
    /// done again does **not** restamp it: `done_at` answers "when was this
    /// reached", and a second click on a button is not a second event.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the milestone is not one this caller can
    /// see; [`StoreError::Db`] on failure.
    pub async fn set_milestone_done(
        &self,
        id: &ProjectMilestoneId,
        done: bool,
    ) -> Result<Milestone> {
        let affected = sqlx::query(&format!(
            "UPDATE project_milestones m \
             SET done_at = CASE WHEN $3 THEN coalesce(m.done_at, now()) ELSE NULL END, \
                 updated_at = now() \
             WHERE m.tenant_id = $1 AND m.id = $2 AND {}",
            visible_project_predicate("m.project_id", "$4")
        ))
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(done)
        .bind(self.user.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if affected.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        self.milestone(id).await?.ok_or(StoreError::NotFound)
    }

    /// Deletes a milestone from the plan.
    ///
    /// The tasks under it stay exactly where they are on the board; what is
    /// deleted is the *date they were placed against*. Deleting a plan never
    /// deletes work.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the milestone is not one this caller can
    /// see, or was already deleted; [`StoreError::Db`] on failure.
    pub async fn delete_milestone(&self, id: &ProjectMilestoneId) -> Result<()> {
        let done = sqlx::query(&format!(
            "DELETE FROM project_milestones m \
             WHERE m.tenant_id = $1 AND m.id = $2 AND {}",
            visible_project_predicate("m.project_id", "$3")
        ))
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(self.user.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Places a task under a milestone — or moves it to another one, which is
    /// the same write because a task has exactly one place in the plan.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when either the task or the milestone is not
    /// one this caller can see; [`StoreError::Validation`] when they belong to
    /// different projects — a plan does not reach across boards;
    /// [`StoreError::Db`] on failure.
    pub async fn set_task_milestone(
        &self,
        task: &TaskId,
        milestone: &ProjectMilestoneId,
    ) -> Result<()> {
        let target = self
            .milestone(milestone)
            .await?
            .ok_or(StoreError::NotFound)?;
        let task_project = self.visible_task_project(task).await?;
        if task_project != target.project_id {
            return Err(StoreError::Validation(
                "a task can only be placed under a milestone of its own project".to_owned(),
            ));
        }
        sqlx::query(
            "INSERT INTO task_milestones (tenant_id, task_id, milestone_id) \
             VALUES ($1, $2, $3) \
             ON CONFLICT (tenant_id, task_id) DO UPDATE SET \
                 milestone_id = EXCLUDED.milestone_id, created_at = now()",
        )
        .bind(self.tenant.as_str())
        .bind(task.as_str())
        .bind(milestone.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }

    /// Takes a task out of the plan, leaving it on the board.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the task is not one this caller can see or
    /// was not placed under any milestone; [`StoreError::Db`] on failure.
    pub async fn clear_task_milestone(&self, task: &TaskId) -> Result<()> {
        self.visible_task_project(task).await?;
        let done = sqlx::query("DELETE FROM task_milestones WHERE tenant_id = $1 AND task_id = $2")
            .bind(self.tenant.as_str())
            .bind(task.as_str())
            .execute(&self.pool)
            .await
            .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Where every placed task of one project sits in its plan — the second
    /// read a timeline makes, beside [`AccountStore::tasks_in_project`].
    ///
    /// Returned as placements rather than folded into the tasks, because the
    /// board's task record is [`crate::tasks`]' and this wave does not widen it.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn task_placements(&self, project: &ProjectId) -> Result<Vec<TaskPlacement>> {
        let rows = sqlx::query_as::<_, (String, String)>(&format!(
            "SELECT l.task_id, l.milestone_id FROM task_milestones l \
             JOIN project_milestones m ON m.tenant_id = l.tenant_id AND m.id = l.milestone_id \
             WHERE l.tenant_id = $1 AND m.project_id = $2 AND {} \
             ORDER BY l.task_id",
            visible_project_predicate("m.project_id", "$3")
        ))
        .bind(self.tenant.as_str())
        .bind(project.as_str())
        .bind(self.user.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows
            .into_iter()
            .map(|(task_id, milestone_id)| TaskPlacement {
                task_id: TaskId::new(task_id),
                milestone_id: ProjectMilestoneId::new(milestone_id),
            })
            .collect())
    }

    /// Confirms a project may be planned on: it is **this tenant's**, it is
    /// visible to this caller, and it is not archived.
    ///
    /// A colleague's personal board reads as absent rather than as refused —
    /// naming the rule would confirm a row they may not see. An archived board
    /// they *can* see gets the honest reason instead.
    async fn require_planable_project(&self, project: &ProjectId) -> Result<()> {
        let row = sqlx::query_as::<_, (String, String, bool)>(
            "SELECT kind, owner_user_id, archived FROM task_projects \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(project.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        let (kind, owner_user_id, archived) = row.ok_or(StoreError::NotFound)?;
        if kind != TEAM_KIND && owner_user_id != self.user.as_str() {
            return Err(StoreError::NotFound);
        }
        if archived {
            return Err(StoreError::Validation(
                "the project is archived; restore it before planning on it".to_owned(),
            ));
        }
        Ok(())
    }

    /// The project of a task this caller can see, or [`StoreError::NotFound`].
    async fn visible_task_project(&self, task: &TaskId) -> Result<ProjectId> {
        let row = sqlx::query_scalar::<_, String>(&format!(
            "SELECT t.project_id FROM tasks t WHERE t.tenant_id = $1 AND t.id = $2 AND {}",
            visible_project_predicate("t.project_id", "$3")
        ))
        .bind(self.tenant.as_str())
        .bind(task.as_str())
        .bind(self.user.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        row.map(ProjectId::new).ok_or(StoreError::NotFound)
    }
}

/// SQL predicate: `column` names a project this caller may see — a team board,
/// shared tenant-wide, or their own personal one. The tenant is always `$1`;
/// `viewer` is the placeholder the caller binds their own user id to, stated
/// rather than assumed because each statement below has a different number of
/// values in front of it.
///
/// The same rule [`crate::tasks`] enforces, expressed once here rather than
/// copied into six statements. Archived boards stay readable, unlike the task
/// list's: a plan drawn on a project that was archived last month is still the
/// answer to "what was planned", and only *writing* a new milestone is refused.
fn visible_project_predicate(column: &str, viewer: &str) -> String {
    format!(
        "{column} IN (SELECT p.id FROM task_projects p WHERE p.tenant_id = $1 \
           AND (p.kind = 'team' OR (p.kind = 'personal' AND p.owner_user_id = {viewer})))"
    )
}

// ---- row types --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct MilestoneRow {
    id: String,
    project_id: String,
    name: String,
    due_on: Date,
    done_at: Option<OffsetDateTime>,
    position: i64,
    created_by: String,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
    task_count: i64,
    task_done_count: i64,
}

impl MilestoneRow {
    fn into_milestone(self) -> Milestone {
        Milestone {
            id: ProjectMilestoneId::new(self.id),
            project_id: ProjectId::new(self.project_id),
            name: self.name,
            due_on: self.due_on,
            done_at: self.done_at,
            position: self.position,
            created_by: self.created_by,
            created_at: self.created_at,
            updated_at: self.updated_at,
            task_count: self.task_count,
            task_done_count: self.task_done_count,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Month;

    fn message<T: std::fmt::Debug>(result: Result<T>) -> String {
        match result {
            Err(StoreError::Validation(msg)) => msg,
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    fn day(day: u8) -> Date {
        Date::from_calendar_date(2026, Month::September, day)
            .unwrap_or_else(|e| panic!("test date: {e:?}"))
    }

    fn milestone(due_on: Date, done_at: Option<OffsetDateTime>) -> Milestone {
        Milestone {
            id: ProjectMilestoneId::new("m1"),
            project_id: ProjectId::new("p1"),
            name: "Design signed off".to_owned(),
            due_on,
            done_at,
            position: 0,
            created_by: "u1".to_owned(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
            task_count: 3,
            task_done_count: 3,
        }
    }

    #[test]
    fn a_name_is_required_and_bounded() {
        assert_eq!(
            normalize_name("  Beta with the pilot customer  ").unwrap_or_default(),
            "Beta with the pilot customer",
            "and it is trimmed"
        );
        assert!(message(normalize_name("   ")).contains("milestone name"));
        assert!(message(normalize_name(&"x".repeat(NAME_MAX + 1))).contains("milestone name"));
        assert!(
            normalize_name(&"x".repeat(NAME_MAX)).is_ok(),
            "the bound itself is legal"
        );
    }

    #[test]
    fn all_its_tasks_closed_is_not_the_same_statement_as_reached() {
        let open = milestone(day(30), None);
        assert_eq!(open.task_count, open.task_done_count);
        assert!(
            !open.is_done(),
            "only a human marks a milestone reached (docs/design/projects.md)"
        );
    }

    #[test]
    fn late_is_a_date_behind_us_that_nobody_has_closed() {
        assert!(milestone(day(10), None).is_late(day(11)));
        assert!(
            !milestone(day(10), None).is_late(day(10)),
            "today is not late"
        );
        assert!(!milestone(day(30), None).is_late(day(11)));
        assert!(
            !milestone(day(10), Some(OffsetDateTime::UNIX_EPOCH)).is_late(day(30)),
            "a reached milestone is never late, however late it was reached"
        );
    }

    #[test]
    fn the_visibility_predicate_names_the_viewer_and_both_kinds() {
        let sql = visible_project_predicate("m.project_id", "$3");
        assert!(sql.starts_with("m.project_id IN (SELECT p.id FROM task_projects p"));
        assert!(sql.contains("p.tenant_id = $1"), "always tenant-bound");
        assert!(sql.contains("p.kind = 'team'"));
        assert!(
            sql.contains("p.owner_user_id = $3"),
            "the viewer is bound where the caller says, not where a default assumed"
        );
    }
}
