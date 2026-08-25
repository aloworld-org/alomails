//! Durable project status updates: the small narrative beside computed health.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::billing_field::required;
use crate::error::{Result, StoreError};
use crate::id::{ProjectId, ProjectUpdateId};

pub const UPDATE_BODY_MAX: usize = 4_000;
pub const UPDATE_LIST_MAX: i64 = 50;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectUpdateAttachment {
    pub blob_id: String,
    pub filename: String,
    pub size: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectUpdateState {
    OnTrack,
    AtRisk,
    OffTrack,
    Complete,
}

impl ProjectUpdateState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OnTrack => "on_track",
            Self::AtRisk => "at_risk",
            Self::OffTrack => "off_track",
            Self::Complete => "complete",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "on_track" => Ok(Self::OnTrack),
            "at_risk" => Ok(Self::AtRisk),
            "off_track" => Ok(Self::OffTrack),
            "complete" => Ok(Self::Complete),
            _ => Err(StoreError::Validation(
                "update state is not valid".to_owned(),
            )),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProjectUpdate {
    pub id: ProjectUpdateId,
    pub project_id: ProjectId,
    pub state: ProjectUpdateState,
    pub body: String,
    pub author_id: String,
    pub author_email: String,
    pub created_at: OffsetDateTime,
    pub attachments: Vec<ProjectUpdateAttachment>,
}

impl AccountStore {
    pub async fn project_updates(&self, project: &ProjectId) -> Result<Vec<ProjectUpdate>> {
        let rows = sqlx::query_as::<_, UpdateRow>(
            "SELECT x.id, x.project_id, x.state, x.body, x.created_by, \
                    COALESCE(u.email, '') AS author_email, x.created_at, x.attachments \
             FROM project_updates x \
             LEFT JOIN users u ON u.tenant_id = x.tenant_id AND u.id = x.created_by \
             JOIN task_projects p ON p.tenant_id = x.tenant_id AND p.id = x.project_id \
             WHERE x.tenant_id = $1 AND x.project_id = $2 \
               AND (p.kind = 'team' OR (p.kind = 'personal' AND p.owner_user_id = $3)) \
             ORDER BY x.created_at DESC, x.id DESC LIMIT $4",
        )
        .bind(self.tenant.as_str())
        .bind(project.as_str())
        .bind(self.user.as_str())
        .bind(UPDATE_LIST_MAX)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter().map(UpdateRow::into_update).collect()
    }

    pub async fn create_project_update(
        &self,
        project: &ProjectId,
        state: ProjectUpdateState,
        body: &str,
        attachments: &[ProjectUpdateAttachment],
    ) -> Result<ProjectUpdate> {
        let body = required("project update", body, UPDATE_BODY_MAX)?;
        let id = ProjectUpdateId::generate();
        let inserted = sqlx::query_scalar::<_, String>(
            "INSERT INTO project_updates (tenant_id, id, project_id, state, body, created_by, attachments) \
             SELECT $1, $2, p.id, $4, $5, $6, $7 FROM task_projects p \
             WHERE p.tenant_id = $1 AND p.id = $3 AND p.archived = false \
               AND (p.kind = 'team' OR (p.kind = 'personal' AND p.owner_user_id = $6)) \
             RETURNING id",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(project.as_str())
        .bind(state.as_str())
        .bind(body)
        .bind(self.user.as_str())
        .bind(serde_json::to_value(attachments).map_err(|error| StoreError::Validation(error.to_string()))?)
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if inserted.is_none() {
            return Err(StoreError::NotFound);
        }
        self.project_updates(project)
            .await?
            .into_iter()
            .find(|update| update.id == id)
            .ok_or(StoreError::NotFound)
    }
}

#[derive(sqlx::FromRow)]
struct UpdateRow {
    id: String,
    project_id: String,
    state: String,
    body: String,
    created_by: String,
    author_email: String,
    created_at: OffsetDateTime,
    attachments: serde_json::Value,
}

impl UpdateRow {
    fn into_update(self) -> Result<ProjectUpdate> {
        Ok(ProjectUpdate {
            id: ProjectUpdateId::new(self.id),
            project_id: ProjectId::new(self.project_id),
            state: ProjectUpdateState::parse(&self.state)?,
            body: self.body,
            author_id: self.created_by,
            author_email: self.author_email,
            created_at: self.created_at,
            attachments: serde_json::from_value(self.attachments)
                .map_err(|error| StoreError::Validation(error.to_string()))?,
        })
    }
}
