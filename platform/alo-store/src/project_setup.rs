//! Explicit, retry-safe setup of the resources around a delivery project.

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::chat::ChannelVisibility;
use crate::error::{Result, StoreError};
use crate::id::{EventId, ProjectId};
use crate::model::CalendarEvent;
use crate::tasks::NewTask;

const STARTER_TASKS_MAX: usize = 20;

#[derive(Debug, Clone, Default)]
pub struct ProjectSetupPlan {
    pub create_files_space: bool,
    pub create_chat_room: bool,
    pub kickoff: Option<KickoffPlan>,
    pub starter_tasks: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct KickoffPlan {
    pub starts_at: OffsetDateTime,
    pub ends_at: OffsetDateTime,
    pub timezone: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectSetup {
    pub project_id: ProjectId,
    pub space_id: Option<String>,
    pub chat_channel_id: Option<String>,
    pub kickoff_event_id: Option<String>,
    pub starter_task_ids: Vec<String>,
    pub created_by: String,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

impl AccountStore {
    pub async fn project_setup(&self, project: &ProjectId) -> Result<Option<ProjectSetup>> {
        if self.visible_project_name(project).await?.is_none() {
            return Err(StoreError::NotFound);
        }
        setup_row(self, project).await
    }

    /// Creates only explicitly requested missing resources. Retrying reads the
    /// stored ids and does not duplicate successfully completed setup.
    pub async fn setup_project(
        &self,
        project: &ProjectId,
        plan: &ProjectSetupPlan,
    ) -> Result<ProjectSetup> {
        let name = self
            .visible_project_name(project)
            .await?
            .ok_or(StoreError::NotFound)?;
        let tasks = normalized_tasks(&plan.starter_tasks)?;
        if !plan.create_files_space
            && !plan.create_chat_room
            && plan.kickoff.is_none()
            && tasks.is_empty()
        {
            return Err(StoreError::Validation(
                "select at least one project resource to set up".to_owned(),
            ));
        }
        if let Some(kickoff) = &plan.kickoff
            && kickoff.ends_at <= kickoff.starts_at
        {
            return Err(StoreError::Validation(
                "the kickoff meeting must end after it starts".to_owned(),
            ));
        }
        sqlx::query(
            "INSERT INTO project_setup (tenant_id, project_id, created_by) VALUES ($1,$2,$3) \
             ON CONFLICT (tenant_id, project_id) DO NOTHING",
        )
        .bind(self.tenant.as_str())
        .bind(project.as_str())
        .bind(self.user.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        let mut setup = setup_row(self, project)
            .await?
            .ok_or_else(|| StoreError::Db(sqlx::Error::RowNotFound))?;

        if plan.create_files_space && setup.space_id.is_none() {
            let space = self.create_space(&format!("{name} files")).await?;
            self.save_setup_resource(project, SetupResource::Space, space.as_str())
                .await?;
            setup.space_id = Some(space.as_str().to_owned());
        }
        if plan.create_chat_room && setup.chat_channel_id.is_none() {
            let channel = self
                .create_channel(
                    &format!("{name} · project"),
                    Some("Delivery coordination for this project"),
                    ChannelVisibility::Public,
                )
                .await?;
            self.save_setup_resource(project, SetupResource::Chat, channel.as_str())
                .await?;
            setup.chat_channel_id = Some(channel.as_str().to_owned());
        }
        if let Some(kickoff) = &plan.kickoff
            && setup.kickoff_event_id.is_none()
        {
            let calendar = self.ensure_personal_calendar().await?;
            let event = CalendarEvent {
                id: EventId::new(String::new()),
                calendar_id: calendar,
                summary: format!("{name} kickoff"),
                description: Some(
                    "Project kickoff created from the confirmed Sales handoff.".to_owned(),
                ),
                location: None,
                starts_at: kickoff.starts_at,
                ends_at: kickoff.ends_at,
                all_day: false,
                recurrence: None,
                attendees: Vec::new(),
                exdates: Vec::new(),
                recurrence_id: None,
                timezone: kickoff.timezone.clone(),
                rdates: Vec::new(),
                reminder_minutes: Some(15),
                attendee_status: Vec::new(),
            };
            let event_id = self.create_event(&event).await?;
            self.save_setup_resource(project, SetupResource::Kickoff, event_id.as_str())
                .await?;
            setup.kickoff_event_id = Some(event_id.as_str().to_owned());
        }
        if !tasks.is_empty() {
            let existing = self
                .tasks_for_source("project_setup", project.as_str())
                .await?;
            let mut ids = setup.starter_task_ids.clone();
            for task in &existing {
                let id = task.id.as_str();
                if !ids.iter().any(|stored| stored == id) {
                    ids.push(id.to_owned());
                }
            }
            for title in tasks {
                if existing.iter().any(|task| task.title == title) {
                    continue;
                }
                let id = self
                    .create_task(
                        project,
                        &NewTask {
                            title,
                            source_kind: Some("project_setup".to_owned()),
                            source_id: Some(project.as_str().to_owned()),
                            ..NewTask::default()
                        },
                    )
                    .await?;
                ids.push(id.as_str().to_owned());
            }
            if ids != setup.starter_task_ids {
                sqlx::query(
                    "UPDATE project_setup SET starter_task_ids = $3, updated_at = now() \
                     WHERE tenant_id = $1 AND project_id = $2",
                )
                .bind(self.tenant.as_str())
                .bind(project.as_str())
                .bind(sqlx::types::Json(&ids))
                .execute(&self.pool)
                .await
                .map_err(StoreError::Db)?;
            }
        }
        setup_row(self, project)
            .await?
            .ok_or_else(|| StoreError::Db(sqlx::Error::RowNotFound))
    }

    async fn visible_project_name(&self, project: &ProjectId) -> Result<Option<String>> {
        sqlx::query_scalar(
            "SELECT name FROM task_projects WHERE tenant_id = $1 AND id = $2 AND archived = false \
             AND (kind = 'team' OR owner_user_id = $3)",
        )
        .bind(self.tenant.as_str())
        .bind(project.as_str())
        .bind(self.user.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)
    }

    async fn save_setup_resource(
        &self,
        project: &ProjectId,
        resource: SetupResource,
        id: &str,
    ) -> Result<()> {
        sqlx::query(resource.update_sql())
            .bind(self.tenant.as_str())
            .bind(project.as_str())
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(StoreError::Db)?;
        Ok(())
    }
}

enum SetupResource {
    Space,
    Chat,
    Kickoff,
}

impl SetupResource {
    fn update_sql(&self) -> &'static str {
        match self {
            Self::Space => {
                "UPDATE project_setup SET space_id=$3, updated_at=now() WHERE tenant_id=$1 AND project_id=$2 AND space_id IS NULL"
            }
            Self::Chat => {
                "UPDATE project_setup SET chat_channel_id=$3, updated_at=now() WHERE tenant_id=$1 AND project_id=$2 AND chat_channel_id IS NULL"
            }
            Self::Kickoff => {
                "UPDATE project_setup SET kickoff_event_id=$3, updated_at=now() WHERE tenant_id=$1 AND project_id=$2 AND kickoff_event_id IS NULL"
            }
        }
    }
}

fn normalized_tasks(tasks: &[String]) -> Result<Vec<String>> {
    if tasks.len() > STARTER_TASKS_MAX {
        return Err(StoreError::Validation(format!(
            "project setup accepts at most {STARTER_TASKS_MAX} starter tasks"
        )));
    }
    let mut normalized = Vec::new();
    for task in tasks {
        let title = task.trim();
        if title.is_empty() {
            return Err(StoreError::Validation(
                "a starter task needs a title".to_owned(),
            ));
        }
        if !normalized.iter().any(|existing| existing == title) {
            normalized.push(title.to_owned());
        }
    }
    Ok(normalized)
}

#[derive(sqlx::FromRow)]
struct SetupRow {
    project_id: String,
    space_id: Option<String>,
    chat_channel_id: Option<String>,
    kickoff_event_id: Option<String>,
    starter_task_ids: sqlx::types::Json<Vec<String>>,
    created_by: String,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

async fn setup_row(store: &AccountStore, project: &ProjectId) -> Result<Option<ProjectSetup>> {
    let row = sqlx::query_as::<_, SetupRow>(
        "SELECT project_id, space_id, chat_channel_id, kickoff_event_id, starter_task_ids, \
         created_by, created_at, updated_at FROM project_setup \
         WHERE tenant_id = $1 AND project_id = $2",
    )
    .bind(store.tenant.as_str())
    .bind(project.as_str())
    .fetch_optional(&store.pool)
    .await
    .map_err(StoreError::Db)?;
    Ok(row.map(|row| ProjectSetup {
        project_id: ProjectId::new(row.project_id),
        space_id: row.space_id,
        chat_channel_id: row.chat_channel_id,
        kickoff_event_id: row.kickoff_event_id,
        starter_task_ids: row.starter_task_ids.0,
        created_by: row.created_by,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }))
}
