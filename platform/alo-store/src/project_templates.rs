//! Project templates — a board a tenant has marked reusable, and the copy that
//! turns it into a new engagement (alo Projects, ADR 0035, wave B3.09),
//! reached through the account door like every other business record.
//!
//! **A template is a project.** This module stores one mark per reusable board
//! and copies rows that already exist; there is no template schema, no JSON
//! shape and no second editor (`docs/design/projects.md`, "Milestones and
//! templates"). The board *is* the template, so a template is opened, reviewed
//! and corrected with the screens the team already uses, and it cannot drift
//! from the model it copies the first time a task gains a field.
//!
//! Four rules decide what a copy contains, and each is a decision rather than
//! an implementation detail:
//!
//! - **Only a `team` board may be marked.** The template list is tenant-wide —
//!   everyone who opens the dialog sees every template — so a personal board in
//!   it would hand a colleague's private work to the whole tenant. The same
//!   rule [`crate::project_clients`] enforces, for a related reason.
//! - **The shape is copied; progress is not.** Titles, descriptions, board
//!   columns, order, priorities, labels, checklists, milestones and the
//!   task→milestone links come along. Assignees, comments, activity, followers,
//!   attachments, dependencies and time entries do not — they are facts about
//!   the engagement that was worked, not about the one being started.
//! - **Finished work is not copied at all.** A card left in the `done` column
//!   is a leftover of the project the template was built from, not part of the
//!   shape of the next one, and a new project that opens with work already
//!   completed is a lie its milestone counts would repeat.
//!   [`ProjectTemplate::task_count`] counts exactly what a copy would carry, so
//!   the dialog promises what it delivers.
//! - **The plan lands on the start date.** Every date moves by
//!   `starts_on − (the template's earliest milestone date)`: a template with
//!   milestones at day 0, 14 and 30 lands 14 and 30 days after the start date
//!   the caller gave, and task due dates move by the same delta. A template
//!   with no milestones shifts nothing — there is nothing to anchor to, and a
//!   delta invented from somewhere else would silently re-date work.

use std::collections::HashMap;

use time::{Date, Duration, OffsetDateTime};

use crate::account::AccountStore;
use crate::billing_field::required;
use crate::error::{Result, StoreError};
use crate::id::{BillingCustomerId, CommentId, ProjectId, ProjectMilestoneId, SubtaskId, TaskId};
use crate::project_clients::{NewProjectClient, normalize};

/// The most tasks one instantiation copies. The same restraint the plan's
/// 200-milestone ceiling expresses: over it the store refuses and names the
/// rule, never truncating a copy into a project that silently lacks work.
pub const TEMPLATE_TASKS_MAX: i64 = 500;

/// Longest name for a project created from a template. The board's own names
/// are unbounded today; a name arriving over HTTP is not.
pub const PROJECT_NAME_MAX: usize = 120;

/// The `task_projects.kind` that may be a template — and the kind every copy
/// is created as.
const TEAM_KIND: &str = "team";

/// The board column whose cards are finished work, and are therefore not part
/// of a template's shape.
const DONE_STATUS: &str = "done";

/// The columns every read of a template selects, in `TemplateRow` order.
///
/// Both counts are the counts of *what a copy would carry*: tasks that are
/// open (`done` cards are left behind, as is the AI's proposed work, which
/// nobody has accepted yet) and every milestone of the plan.
const TEMPLATE_COLS: &str = "t.project_id, p.name, p.color, p.archived, t.created_by, \
     t.created_at, \
     (SELECT count(*) FROM tasks k \
        WHERE k.tenant_id = t.tenant_id AND k.project_id = t.project_id \
          AND k.state = 'active' AND k.status <> 'done' AND k.completed_at IS NULL) AS task_count, \
     (SELECT count(*) FROM project_milestones m \
        WHERE m.tenant_id = t.tenant_id AND m.project_id = t.project_id) AS milestone_count";

/// One reusable board, with what the create-from-template dialog shows.
#[derive(Debug, Clone)]
pub struct ProjectTemplate {
    /// The board that is the template — also its id.
    pub project_id: ProjectId,
    /// The board's name.
    pub name: String,
    /// The board's colour, carried onto every copy.
    pub color: Option<String>,
    /// Whether the board itself has been archived. Archiving a template is an
    /// ordinary way to keep a shape without keeping it in the board list, so it
    /// stays listed here and stays instantiable.
    pub archived: bool,
    /// How many tasks a copy would carry — open work only.
    pub task_count: i64,
    /// How many milestones a copy would carry.
    pub milestone_count: i64,
    /// Who marked the board reusable.
    pub created_by: String,
    /// When they marked it.
    pub created_at: OffsetDateTime,
}

/// What the caller states when starting a project from a template.
#[derive(Debug, Clone)]
pub struct TemplateInstance {
    /// The new project's name. Required — a copy named after its template is a
    /// second board nobody can tell apart from the first.
    pub name: String,
    /// The day the new engagement starts. The whole plan is shifted so the
    /// template's earliest milestone lands here; `None` copies every date as
    /// it stands.
    pub starts_on: Option<Date>,
    /// The customer the new engagement is for, or `None` for internal work.
    /// **The template's own customer is never copied** — a template is an
    /// engagement shape, not a client — but its currency, rate and budgets are,
    /// because a rate that travelled without its currency would be a different
    /// number.
    pub customer_id: Option<BillingCustomerId>,
}

/// What one instantiation produced: the new board, and what landed on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateCopy {
    /// The new project.
    pub project_id: ProjectId,
    /// How many tasks were copied.
    pub task_count: i64,
    /// How many milestones were copied.
    pub milestone_count: i64,
}

/// The rows of the template's board a copy carries, read once before the write.
struct SourceTask {
    id: String,
    title: String,
    description: Option<String>,
    status: String,
    position: f64,
    priority: String,
    due_at: Option<OffsetDateTime>,
}

/// Moves a day by whole days, refusing rather than wrapping at the ends of the
/// calendar. Pure — the shift is the one piece of arithmetic in this module and
/// it is unit-tested directly.
fn shift_date(day: Date, delta: Duration) -> Result<Date> {
    day.checked_add(delta).ok_or_else(|| {
        StoreError::Validation(
            "the start date moves this plan outside the calendar; choose a nearer date".to_owned(),
        )
    })
}

/// Moves an instant by whole days, with the same refusal.
fn shift_instant(at: OffsetDateTime, delta: Duration) -> Result<OffsetDateTime> {
    at.checked_add(delta).ok_or_else(|| {
        StoreError::Validation(
            "the start date moves this plan outside the calendar; choose a nearer date".to_owned(),
        )
    })
}

/// How far the plan moves: from the template's earliest milestone to the day
/// the caller is starting on. Nothing to anchor to (no milestones) or nothing
/// to anchor at (no start date) means nothing moves.
fn plan_delta(starts_on: Option<Date>, anchor: Option<Date>) -> Duration {
    match (starts_on, anchor) {
        (Some(start), Some(anchor)) => start - anchor,
        _ => Duration::ZERO,
    }
}

impl AccountStore {
    /// Marks a board reusable.
    ///
    /// Idempotent: marking twice leaves one mark and keeps the first `created_at`,
    /// because "since when has this been a template" is a fact about the first
    /// time somebody said so.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the project is not this tenant's, or is a
    /// colleague's personal board — existence is never disclosed;
    /// [`StoreError::Validation`] when it is the caller's own personal board or
    /// is archived, each naming the rule; [`StoreError::Db`] on failure.
    pub async fn mark_template(&self, project: &ProjectId) -> Result<ProjectTemplate> {
        self.require_templatable_project(project).await?;
        sqlx::query(
            "INSERT INTO project_templates (tenant_id, project_id, created_by) \
             VALUES ($1, $2, $3) ON CONFLICT (tenant_id, project_id) DO NOTHING",
        )
        .bind(self.tenant.as_str())
        .bind(project.as_str())
        .bind(self.user.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        self.template(project).await?.ok_or(StoreError::NotFound)
    }

    /// Every template of the tenant, in the order they were marked.
    ///
    /// Tenant-wide by construction: only `team` boards can be marked, so this
    /// list can never name a colleague's private work.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn templates(&self) -> Result<Vec<ProjectTemplate>> {
        let rows = sqlx::query_as::<_, TemplateRow>(&format!(
            "SELECT {TEMPLATE_COLS} FROM project_templates t \
             JOIN task_projects p ON p.tenant_id = t.tenant_id AND p.id = t.project_id \
             WHERE t.tenant_id = $1 AND p.kind = '{TEAM_KIND}' \
             ORDER BY t.created_at, t.project_id"
        ))
        .bind(self.tenant.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(TemplateRow::into_template).collect())
    }

    /// One template, or `None` when the board is not this tenant's or was never
    /// marked — indistinguishable by design.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn template(&self, project: &ProjectId) -> Result<Option<ProjectTemplate>> {
        let row = sqlx::query_as::<_, TemplateRow>(&format!(
            "SELECT {TEMPLATE_COLS} FROM project_templates t \
             JOIN task_projects p ON p.tenant_id = t.tenant_id AND p.id = t.project_id \
             WHERE t.tenant_id = $1 AND t.project_id = $2 AND p.kind = '{TEAM_KIND}'"
        ))
        .bind(self.tenant.as_str())
        .bind(project.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(row.map(TemplateRow::into_template))
    }

    /// Takes the mark off a board. The board, its tasks and its plan are
    /// untouched: what is deleted is the claim that it is reusable, never work.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the board is not this tenant's or was not
    /// marked — unmarking twice is a clean denial, not a silent success;
    /// [`StoreError::Db`] on failure.
    pub async fn unmark_template(&self, project: &ProjectId) -> Result<()> {
        let done =
            sqlx::query("DELETE FROM project_templates WHERE tenant_id = $1 AND project_id = $2")
                .bind(self.tenant.as_str())
                .bind(project.as_str())
                .execute(&self.pool)
                .await
                .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Starts a new project from a template.
    ///
    /// One transaction: a copy either lands whole or does not land at all,
    /// because a half-copied board is worse than none — nobody can tell which
    /// half is missing.
    ///
    /// What lands is stated in this module's note: the board and its colour,
    /// its open tasks with their columns, order, priorities, labels and
    /// checklists, its milestones and the task→milestone links, every date
    /// shifted onto `starts_on`. What does not: assignees, comments, activity,
    /// followers, attachments, dependencies, time entries, finished cards, and
    /// the template's customer.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the template is not one this tenant has
    /// marked, or the customer is not this tenant's;
    /// [`StoreError::Validation`] when the name is blank or too long, when the
    /// template carries more than [`TEMPLATE_TASKS_MAX`] copyable tasks, when
    /// the customer is archived, or when the shift moves a date off the
    /// calendar; [`StoreError::Db`] on failure.
    pub async fn instantiate_template(
        &self,
        template: &ProjectId,
        instance: &TemplateInstance,
    ) -> Result<TemplateCopy> {
        let source = self.template(template).await?.ok_or(StoreError::NotFound)?;
        let name = required("project name", &instance.name, PROJECT_NAME_MAX)?;
        if source.task_count > TEMPLATE_TASKS_MAX {
            return Err(StoreError::Validation(format!(
                "a template copies at most {TEMPLATE_TASKS_MAX} tasks; this one has {}",
                source.task_count
            )));
        }
        // The client facts are validated *before* anything is written, against
        // the customer the caller named: a copy that landed and then failed on
        // an archived customer would leave a board nobody asked for.
        let facts = self.resolve_instance_facts(template, instance).await?;

        let tasks = self.template_tasks(template).await?;
        let milestones = self.template_milestones(template).await?;
        let delta = plan_delta(
            instance.starts_on,
            milestones.first().map(|(_, due_on)| *due_on),
        );

        // Ids are minted here, in one place, so every child row can be pointed
        // at its new parent without reading anything back.
        let new_project = ProjectId::generate();
        let task_ids: Vec<(String, String)> = tasks
            .iter()
            .map(|task| (task.id.clone(), TaskId::generate().as_str().to_owned()))
            .collect();
        let milestone_ids: Vec<(String, String)> = milestones
            .iter()
            .map(|(id, _)| {
                (
                    id.clone(),
                    ProjectMilestoneId::generate().as_str().to_owned(),
                )
            })
            .collect();

        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        sqlx::query(
            "INSERT INTO task_projects (tenant_id, id, name, kind, owner_user_id, color) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(self.tenant.as_str())
        .bind(new_project.as_str())
        .bind(&name)
        .bind(TEAM_KIND)
        .bind(self.user.as_str())
        .bind(&source.color)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;

        self.copy_tasks(&mut tx, &new_project, &tasks, &task_ids, delta)
            .await?;
        self.copy_milestones(&mut tx, &new_project, &milestones, &milestone_ids, delta)
            .await?;
        self.copy_placements(&mut tx, template, &task_ids, &milestone_ids)
            .await?;
        if let Some((customer_id, facts)) = facts {
            sqlx::query(
                "INSERT INTO project_clients (tenant_id, project_id, customer_id, currency, \
                     rate_cents, budget_minutes, budget_cents, starts_on) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
            )
            .bind(self.tenant.as_str())
            .bind(new_project.as_str())
            .bind(customer_id.as_str())
            .bind(&facts.currency)
            .bind(facts.rate_cents)
            .bind(facts.budget_minutes)
            .bind(facts.budget_cents)
            .bind(facts.starts_on)
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        }
        tx.commit().await.map_err(StoreError::Db)?;

        Ok(TemplateCopy {
            project_id: new_project,
            task_count: i64::try_from(task_ids.len()).unwrap_or(i64::MAX),
            milestone_count: i64::try_from(milestone_ids.len()).unwrap_or(i64::MAX),
        })
    }

    /// The client facts a copy will carry, resolved and validated before the
    /// write. `None` when the caller named no customer — internal work, which
    /// is expressed by having no client facts at all.
    ///
    /// The template's own customer is never among them; its currency, rate and
    /// budgets are, and they travel together, because a rate is a number in a
    /// currency and the two cannot be allowed to disagree.
    async fn resolve_instance_facts(
        &self,
        template: &ProjectId,
        instance: &TemplateInstance,
    ) -> Result<Option<(BillingCustomerId, crate::project_clients::Normalized)>> {
        let Some(customer_id) = instance.customer_id.clone() else {
            return Ok(None);
        };
        let customer = self
            .billing_customer(&customer_id)
            .await?
            .ok_or(StoreError::NotFound)?;
        if customer.is_archived() {
            return Err(StoreError::Validation(
                "the customer is archived; restore it before billing work to it".to_owned(),
            ));
        }
        let shape = self.project_client(template).await?;
        let input = NewProjectClient {
            customer_id: customer.id.clone(),
            currency: shape.as_ref().map(|facts| facts.currency.clone()),
            rate_cents: shape.as_ref().and_then(|facts| facts.rate_cents),
            budget_minutes: shape.as_ref().and_then(|facts| facts.budget_minutes),
            budget_cents: shape.as_ref().and_then(|facts| facts.budget_cents),
            starts_on: instance.starts_on,
        };
        Ok(Some((customer.id, normalize(&input, &customer.currency)?)))
    }

    /// The template's copyable tasks: open, accepted work, in board order.
    /// Finished cards and the AI's unaccepted proposals are not part of a
    /// shape, so neither is read.
    async fn template_tasks(&self, template: &ProjectId) -> Result<Vec<SourceTask>> {
        let rows = sqlx::query_as::<_, TaskShapeRow>(
            "SELECT id, title, description, status, position, priority, due_at FROM tasks \
             WHERE tenant_id = $1 AND project_id = $2 AND state = 'active' \
               AND status <> $3 AND completed_at IS NULL \
             ORDER BY status, position, created_at, id",
        )
        .bind(self.tenant.as_str())
        .bind(template.as_str())
        .bind(DONE_STATUS)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(TaskShapeRow::into_source).collect())
    }

    /// The template's milestones, earliest first — so the head of the list is
    /// the anchor every date is shifted from.
    async fn template_milestones(&self, template: &ProjectId) -> Result<Vec<(String, Date)>> {
        sqlx::query_as::<_, (String, Date)>(
            "SELECT id, due_on FROM project_milestones \
             WHERE tenant_id = $1 AND project_id = $2 \
             ORDER BY due_on, position, id",
        )
        .bind(self.tenant.as_str())
        .bind(template.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)
    }

    /// Writes the copied tasks, their checklists, their labels and one `created`
    /// entry each — the new task's own history, which says where it came from.
    ///
    /// Each child table is one statement over an `unnest` of the id map rather
    /// than a statement per row: a 500-task template is a plausible template,
    /// and five hundred round trips inside one transaction is a lock held for
    /// no reason.
    async fn copy_tasks(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        project: &ProjectId,
        tasks: &[SourceTask],
        ids: &[(String, String)],
        delta: Duration,
    ) -> Result<()> {
        if tasks.is_empty() {
            return Ok(());
        }
        let new_ids: Vec<String> = ids.iter().map(|(_, new)| new.clone()).collect();
        let titles: Vec<String> = tasks.iter().map(|task| task.title.clone()).collect();
        let descriptions: Vec<Option<String>> =
            tasks.iter().map(|task| task.description.clone()).collect();
        let statuses: Vec<String> = tasks.iter().map(|task| task.status.clone()).collect();
        let positions: Vec<f64> = tasks.iter().map(|task| task.position).collect();
        let priorities: Vec<String> = tasks.iter().map(|task| task.priority.clone()).collect();
        let due_at: Vec<Option<OffsetDateTime>> = tasks
            .iter()
            .map(|task| task.due_at.map(|at| shift_instant(at, delta)).transpose())
            .collect::<Result<Vec<_>>>()?;
        sqlx::query(
            "INSERT INTO tasks (tenant_id, id, project_id, title, description, status, position, \
                 priority, state, created_by, due_at) \
             SELECT $1, c.id, $2, c.title, c.description, c.status, c.position, c.priority, \
                 'active', $3, c.due_at \
             FROM unnest($4::text[], $5::text[], $6::text[], $7::text[], $8::float8[], \
                 $9::text[], $10::timestamptz[]) \
                 AS c(id, title, description, status, position, priority, due_at)",
        )
        .bind(self.tenant.as_str())
        .bind(project.as_str())
        .bind(self.user.as_str())
        .bind(&new_ids)
        .bind(&titles)
        .bind(&descriptions)
        .bind(&statuses)
        .bind(&positions)
        .bind(&priorities)
        .bind(&due_at)
        .execute(&mut **tx)
        .await
        .map_err(StoreError::Db)?;

        let old_ids: Vec<String> = ids.iter().map(|(old, _)| old.clone()).collect();
        // The checklists, unchecked: a copy carries the steps, never the ticks.
        let subtasks = sqlx::query_as::<_, (String, String)>(
            "SELECT id, task_id FROM task_subtasks WHERE tenant_id = $1 AND task_id = ANY($2) \
             ORDER BY task_id, position, id",
        )
        .bind(self.tenant.as_str())
        .bind(&old_ids)
        .fetch_all(&mut **tx)
        .await
        .map_err(StoreError::Db)?;
        if !subtasks.is_empty() {
            let map: HashMap<&str, &str> = ids
                .iter()
                .map(|(old, new)| (old.as_str(), new.as_str()))
                .collect();
            // The three arrays are built in one pass so they stay the same
            // length and the same order: `unnest` pads a short array with
            // NULLs, which would silently misalign every row after the gap.
            let mut source_ids: Vec<String> = Vec::with_capacity(subtasks.len());
            let mut fresh_ids: Vec<String> = Vec::with_capacity(subtasks.len());
            let mut parents: Vec<String> = Vec::with_capacity(subtasks.len());
            for (id, task_id) in &subtasks {
                let Some(parent) = map.get(task_id.as_str()) else {
                    continue;
                };
                source_ids.push(id.clone());
                fresh_ids.push(SubtaskId::generate().as_str().to_owned());
                parents.push((*parent).to_owned());
            }
            sqlx::query(
                "INSERT INTO task_subtasks (tenant_id, id, task_id, title, done, position) \
                 SELECT $1, c.id, c.task_id, s.title, false, s.position \
                 FROM unnest($2::text[], $3::text[], $4::text[]) AS c(source_id, id, task_id) \
                 JOIN task_subtasks s ON s.tenant_id = $1 AND s.id = c.source_id",
            )
            .bind(self.tenant.as_str())
            .bind(&source_ids)
            .bind(&fresh_ids)
            .bind(&parents)
            .execute(&mut **tx)
            .await
            .map_err(StoreError::Db)?;
        }

        // Labels are tenant-wide records, so a copy links the same labels
        // rather than minting near-duplicates nobody asked for.
        sqlx::query(
            "INSERT INTO task_label_links (tenant_id, task_id, label_id) \
             SELECT $1, c.new_id, l.label_id \
             FROM unnest($2::text[], $3::text[]) AS c(old_id, new_id) \
             JOIN task_label_links l ON l.tenant_id = $1 AND l.task_id = c.old_id \
             ON CONFLICT DO NOTHING",
        )
        .bind(self.tenant.as_str())
        .bind(&old_ids)
        .bind(&new_ids)
        .execute(&mut **tx)
        .await
        .map_err(StoreError::Db)?;

        // Not the template's activity — the new task's own first line, which
        // says it was created from a template and which one.
        let activity_ids: Vec<String> = ids
            .iter()
            .map(|_| CommentId::generate().as_str().to_owned())
            .collect();
        sqlx::query(
            "INSERT INTO task_activity (tenant_id, id, task_id, actor_user_id, kind, detail) \
             SELECT $1, c.activity_id, c.task_id, $2, 'created', \
                 jsonb_build_object('fromTemplateTask', c.source_id) \
             FROM unnest($3::text[], $4::text[], $5::text[]) \
                 AS c(activity_id, task_id, source_id)",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(&activity_ids)
        .bind(&new_ids)
        .bind(&old_ids)
        .execute(&mut **tx)
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }

    /// Writes the copied milestones, every date moved by `delta` and the
    /// tie-break order kept.
    async fn copy_milestones(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        project: &ProjectId,
        milestones: &[(String, Date)],
        ids: &[(String, String)],
        delta: Duration,
    ) -> Result<()> {
        if milestones.is_empty() {
            return Ok(());
        }
        let source_ids: Vec<String> = ids.iter().map(|(old, _)| old.clone()).collect();
        let new_ids: Vec<String> = ids.iter().map(|(_, new)| new.clone()).collect();
        let due_on: Vec<Date> = milestones
            .iter()
            .map(|(_, due_on)| shift_date(*due_on, delta))
            .collect::<Result<Vec<_>>>()?;
        sqlx::query(
            "INSERT INTO project_milestones \
                 (tenant_id, id, project_id, name, due_on, position, created_by) \
             SELECT $1, c.id, $2, m.name, c.due_on, m.position, $3 \
             FROM unnest($4::text[], $5::text[], $6::date[]) AS c(source_id, id, due_on) \
             JOIN project_milestones m ON m.tenant_id = $1 AND m.id = c.source_id",
        )
        .bind(self.tenant.as_str())
        .bind(project.as_str())
        .bind(self.user.as_str())
        .bind(&source_ids)
        .bind(&new_ids)
        .bind(&due_on)
        .execute(&mut **tx)
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }

    /// Rebuilds the task→milestone links between the copies, dropping any whose
    /// task was not copied (a finished card's place in the plan is finished
    /// too).
    async fn copy_placements(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        template: &ProjectId,
        task_ids: &[(String, String)],
        milestone_ids: &[(String, String)],
    ) -> Result<()> {
        if task_ids.is_empty() || milestone_ids.is_empty() {
            return Ok(());
        }
        let placements = sqlx::query_as::<_, (String, String)>(
            "SELECT l.task_id, l.milestone_id FROM task_milestones l \
             JOIN project_milestones m ON m.tenant_id = l.tenant_id AND m.id = l.milestone_id \
             WHERE l.tenant_id = $1 AND m.project_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(template.as_str())
        .fetch_all(&mut **tx)
        .await
        .map_err(StoreError::Db)?;
        let tasks: HashMap<&str, &str> = task_ids
            .iter()
            .map(|(old, new)| (old.as_str(), new.as_str()))
            .collect();
        let milestones: HashMap<&str, &str> = milestone_ids
            .iter()
            .map(|(old, new)| (old.as_str(), new.as_str()))
            .collect();
        let mut new_tasks: Vec<String> = Vec::new();
        let mut new_milestones: Vec<String> = Vec::new();
        for (task_id, milestone_id) in &placements {
            if let (Some(task), Some(milestone)) = (
                tasks.get(task_id.as_str()),
                milestones.get(milestone_id.as_str()),
            ) {
                new_tasks.push((*task).to_owned());
                new_milestones.push((*milestone).to_owned());
            }
        }
        if new_tasks.is_empty() {
            return Ok(());
        }
        sqlx::query(
            "INSERT INTO task_milestones (tenant_id, task_id, milestone_id) \
             SELECT $1, c.task_id, c.milestone_id \
             FROM unnest($2::text[], $3::text[]) AS c(task_id, milestone_id)",
        )
        .bind(self.tenant.as_str())
        .bind(&new_tasks)
        .bind(&new_milestones)
        .execute(&mut **tx)
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }

    /// Confirms a project may be marked reusable: it is **this tenant's**, it
    /// is a shared team board, and it is not archived.
    ///
    /// A colleague's personal board reads as absent rather than as refused —
    /// naming the rule would confirm a row they may not see. Their own personal
    /// board gets the honest reason instead.
    async fn require_templatable_project(&self, project: &ProjectId) -> Result<()> {
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
        if kind != TEAM_KIND {
            if owner_user_id != self.user.as_str() {
                return Err(StoreError::NotFound);
            }
            return Err(StoreError::Validation(
                "only a team project can be a template; a personal board is private work"
                    .to_owned(),
            ));
        }
        if archived {
            return Err(StoreError::Validation(
                "the project is archived; restore it before making it a template".to_owned(),
            ));
        }
        Ok(())
    }
}

// ---- row types --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct TemplateRow {
    project_id: String,
    name: String,
    color: Option<String>,
    archived: bool,
    created_by: String,
    created_at: OffsetDateTime,
    task_count: i64,
    milestone_count: i64,
}

impl TemplateRow {
    fn into_template(self) -> ProjectTemplate {
        ProjectTemplate {
            project_id: ProjectId::new(self.project_id),
            name: self.name,
            color: self.color,
            archived: self.archived,
            task_count: self.task_count,
            milestone_count: self.milestone_count,
            created_by: self.created_by,
            created_at: self.created_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct TaskShapeRow {
    id: String,
    title: String,
    description: Option<String>,
    status: String,
    position: f64,
    priority: String,
    due_at: Option<OffsetDateTime>,
}

impl TaskShapeRow {
    fn into_source(self) -> SourceTask {
        SourceTask {
            id: self.id,
            title: self.title,
            description: self.description,
            status: self.status,
            position: self.position,
            priority: self.priority,
            due_at: self.due_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Month;

    fn day(month: Month, day: u8) -> Date {
        Date::from_calendar_date(2026, month, day).unwrap_or_else(|e| panic!("test date: {e:?}"))
    }

    /// The shifted day, or the test's own failure.
    fn shifted(from: Date, delta: Duration) -> Date {
        shift_date(from, delta).unwrap_or_else(|e| panic!("shift refused: {e:?}"))
    }

    #[test]
    fn the_plan_moves_from_its_first_milestone_to_the_start_date() {
        let delta = plan_delta(Some(day(Month::October, 1)), Some(day(Month::September, 1)));
        assert_eq!(delta.whole_days(), 30);
        assert_eq!(
            shifted(day(Month::September, 15), delta),
            day(Month::October, 15),
            "every other date moves by the same amount"
        );
    }

    #[test]
    fn a_plan_moves_backwards_as_readily_as_forwards() {
        let delta = plan_delta(Some(day(Month::August, 1)), Some(day(Month::September, 1)));
        assert_eq!(delta.whole_days(), -31);
        assert_eq!(
            shifted(day(Month::September, 30), delta),
            day(Month::August, 30)
        );
    }

    #[test]
    fn nothing_to_anchor_to_or_at_means_nothing_moves() {
        assert_eq!(
            plan_delta(Some(day(Month::October, 1)), None),
            Duration::ZERO,
            "a template with no milestones shifts nothing (docs/design/projects.md)"
        );
        assert_eq!(
            plan_delta(None, Some(day(Month::September, 1))),
            Duration::ZERO,
            "and neither does a copy with no start date"
        );
    }

    #[test]
    fn a_shift_off_the_calendar_is_refused_not_wrapped() {
        let delta = Duration::days(400_000_000);
        match shift_date(day(Month::September, 1), delta) {
            Err(StoreError::Validation(msg)) => assert!(msg.contains("calendar"), "{msg}"),
            other => panic!("expected Validation, got {other:?}"),
        }
        match shift_instant(OffsetDateTime::UNIX_EPOCH, delta) {
            Err(StoreError::Validation(msg)) => assert!(msg.contains("calendar"), "{msg}"),
            other => panic!("expected Validation, got {other:?}"),
        }
    }
}
