//! Durable provenance between a won Sales opportunity and the Projects
//! engagement created from it. Conversion is transactional and idempotent:
//! retrying the same confirmed action returns the original project.

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::{CrmDealId, ProjectId};
use crate::project_clients::NewProjectClient;
use crate::project_templates::PROJECT_NAME_MAX;

/// The one-to-one relationship between a Sales deal and a project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DealProject {
    /// The won opportunity that originated the work.
    pub deal_id: CrmDealId,
    /// The project created for delivery.
    pub project_id: ProjectId,
    /// Current project name, returned for useful relationship cards.
    pub project_name: String,
    /// User who confirmed conversion.
    pub created_by: String,
    /// When conversion was confirmed.
    pub created_at: OffsetDateTime,
}

impl AccountStore {
    /// Returns the project related to a deal. A foreign-tenant deal is absent.
    pub async fn crm_deal_project(&self, deal: &CrmDealId) -> Result<Option<DealProject>> {
        if !self.crm_deal_exists(deal).await? {
            return Ok(None);
        }
        deal_project_row(&self.pool, self.tenant.as_str(), deal.as_str(), None).await
    }

    /// Returns the Sales deal that originated a project. A foreign-tenant
    /// project is absent.
    pub async fn crm_project_deal(&self, project: &ProjectId) -> Result<Option<DealProject>> {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM task_projects WHERE tenant_id = $1 AND id = $2)",
        )
        .bind(self.tenant.as_str())
        .bind(project.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if !exists {
            return Ok(None);
        }
        deal_project_row(&self.pool, self.tenant.as_str(), "", Some(project.as_str())).await
    }

    /// Creates a project from a won deal in one transaction.
    ///
    /// Returns `(relationship, created)`. A retry returns the same relationship
    /// with `created == false`; open/lost and foreign-tenant deals are refused.
    pub async fn create_project_from_won_deal(
        &self,
        deal: &CrmDealId,
        name: &str,
        color: Option<&str>,
        client: Option<&NewProjectClient>,
    ) -> Result<(DealProject, bool)> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > PROJECT_NAME_MAX {
            return Err(StoreError::Validation(format!(
                "project name must be between 1 and {PROJECT_NAME_MAX} characters"
            )));
        }

        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let (outcome,): (Option<String>,) = sqlx::query_as(
            "SELECT outcome FROM crm_deals WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
        )
        .bind(self.tenant.as_str())
        .bind(deal.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(StoreError::Db)?
        .ok_or(StoreError::NotFound)?;

        if let Some(existing) = deal_project_row_in(&mut tx, self.tenant.as_str(), deal).await? {
            tx.commit().await.map_err(StoreError::Db)?;
            return Ok((existing, false));
        }
        if outcome.as_deref() != Some("won") {
            return Err(StoreError::Validation(
                "only a won deal can be converted to a project".to_owned(),
            ));
        }

        let prepared = match client {
            Some(facts) => Some((facts, self.prepare_project_client_in(&mut tx, facts).await?)),
            None => None,
        };
        let project = self
            .insert_project_in(&mut tx, name, color, prepared)
            .await?;
        sqlx::query(
            "INSERT INTO crm_deal_projects (tenant_id, deal_id, project_id, created_by) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(self.tenant.as_str())
        .bind(deal.as_str())
        .bind(project.as_str())
        .bind(self.user.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;

        let relationship = deal_project_row_in(&mut tx, self.tenant.as_str(), deal)
            .await?
            .ok_or_else(|| StoreError::Db(sqlx::Error::RowNotFound))?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok((relationship, true))
    }

    async fn crm_deal_exists(&self, deal: &CrmDealId) -> Result<bool> {
        sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM crm_deals WHERE tenant_id = $1 AND id = $2)",
        )
        .bind(self.tenant.as_str())
        .bind(deal.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(StoreError::Db)
    }
}

async fn deal_project_row(
    pool: &sqlx::PgPool,
    tenant: &str,
    deal: &str,
    project: Option<&str>,
) -> Result<Option<DealProject>> {
    let row = sqlx::query_as::<_, DealProjectRow>(
        "SELECT l.deal_id, l.project_id, p.name AS project_name, l.created_by, l.created_at \
         FROM crm_deal_projects l JOIN task_projects p \
           ON p.tenant_id = l.tenant_id AND p.id = l.project_id \
         WHERE l.tenant_id = $1 AND (($2 <> '' AND l.deal_id = $2) OR ($3::text IS NOT NULL AND l.project_id = $3))",
    )
    .bind(tenant)
    .bind(deal)
    .bind(project)
    .fetch_optional(pool)
    .await
    .map_err(StoreError::Db)?;
    Ok(row.map(DealProjectRow::into_relationship))
}

async fn deal_project_row_in(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant: &str,
    deal: &CrmDealId,
) -> Result<Option<DealProject>> {
    let row = sqlx::query_as::<_, DealProjectRow>(
        "SELECT l.deal_id, l.project_id, p.name AS project_name, l.created_by, l.created_at \
         FROM crm_deal_projects l JOIN task_projects p \
           ON p.tenant_id = l.tenant_id AND p.id = l.project_id \
         WHERE l.tenant_id = $1 AND l.deal_id = $2",
    )
    .bind(tenant)
    .bind(deal.as_str())
    .fetch_optional(&mut **tx)
    .await
    .map_err(StoreError::Db)?;
    Ok(row.map(DealProjectRow::into_relationship))
}

#[derive(sqlx::FromRow)]
struct DealProjectRow {
    deal_id: String,
    project_id: String,
    project_name: String,
    created_by: String,
    created_at: OffsetDateTime,
}

impl DealProjectRow {
    fn into_relationship(self) -> DealProject {
        DealProject {
            deal_id: CrmDealId::new(self.deal_id),
            project_id: ProjectId::new(self.project_id),
            project_name: self.project_name,
            created_by: self.created_by,
            created_at: self.created_at,
        }
    }
}
