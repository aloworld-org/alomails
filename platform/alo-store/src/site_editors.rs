//! Per-site grants for alo Sites collaborators.
//!
//! A [`TenantRole::SiteEditor`](crate::TenantRole::SiteEditor) is the global
//! restricted-account signal; a row here is the only site that signal opens.
//! Keeping the resource beside the role avoids both unsafe extremes: a normal
//! member is not narrowed merely because somebody shared a site, and a Sites
//! collaborator cannot wander across every site in the tenant.

use crate::error::{Result, StoreError};
use crate::id::{SiteId, UserId};
use crate::sites::{Site, SiteRow};
use crate::store::TenantStore;
use crate::{AccountStore, TenantRole};

impl TenantStore {
    /// Grants one tenant member edit access to one tenant site. The write is
    /// atomic with the restricted role, so there is never a grant whose user
    /// still has ordinary workspace access.
    pub async fn grant_site_editor(
        &self,
        user: &UserId,
        site: &SiteId,
        granted_by: &UserId,
    ) -> Result<()> {
        self.assert_user(user).await?;
        self.assert_user(granted_by).await?;
        let mut tx = self.pool().begin().await?;
        let site_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM sites WHERE tenant_id = $1 AND id = $2)",
        )
        .bind(self.tenant().as_str())
        .bind(site.as_str())
        .fetch_one(&mut *tx)
        .await?;
        if !site_exists {
            return Err(StoreError::NotFound);
        }
        sqlx::query(
            "INSERT INTO tenant_user_roles (tenant_id, user_id, role, granted_by) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT (tenant_id, user_id, role) DO NOTHING",
        )
        .bind(self.tenant().as_str())
        .bind(user.as_str())
        .bind(TenantRole::SiteEditor.as_str())
        .bind(granted_by.as_str())
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO site_editor_grants (tenant_id, site_id, user_id, granted_by) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT (tenant_id, site_id, user_id) DO NOTHING",
        )
        .bind(self.tenant().as_str())
        .bind(site.as_str())
        .bind(user.as_str())
        .bind(granted_by.as_str())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Revokes one site. Removing the final grant also removes the restricted
    /// role, restoring the account to an ordinary tenant member atomically.
    pub async fn revoke_site_editor(&self, user: &UserId, site: &SiteId) -> Result<()> {
        self.assert_user(user).await?;
        let mut tx = self.pool().begin().await?;
        let site_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM sites WHERE tenant_id = $1 AND id = $2)",
        )
        .bind(self.tenant().as_str())
        .bind(site.as_str())
        .fetch_one(&mut *tx)
        .await?;
        if !site_exists {
            return Err(StoreError::NotFound);
        }
        sqlx::query(
            "DELETE FROM site_editor_grants \
             WHERE tenant_id = $1 AND site_id = $2 AND user_id = $3",
        )
        .bind(self.tenant().as_str())
        .bind(site.as_str())
        .bind(user.as_str())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// The site ids granted to one tenant member, sorted. A foreign user id
    /// sees an empty list and reveals no membership or grant fact.
    pub async fn site_editor_grants(&self, user: &UserId) -> Result<Vec<SiteId>> {
        let ids: Vec<String> = sqlx::query_scalar(
            "SELECT g.site_id FROM site_editor_grants g \
             JOIN users u ON u.tenant_id = g.tenant_id AND u.id = g.user_id \
             WHERE g.tenant_id = $1 AND g.user_id = $2 ORDER BY g.site_id",
        )
        .bind(self.tenant().as_str())
        .bind(user.as_str())
        .fetch_all(self.pool())
        .await?;
        Ok(ids.into_iter().map(SiteId::new).collect())
    }
}

impl AccountStore {
    /// Whether this restricted account may open `site`.
    pub async fn can_edit_site(&self, site: &SiteId) -> Result<bool> {
        sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM site_editor_grants \
             WHERE tenant_id = $1 AND user_id = $2 AND site_id = $3)",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(site.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(StoreError::Db)
    }

    /// The complete site records granted to this restricted account.
    pub async fn editable_sites(&self) -> Result<Vec<Site>> {
        let rows = sqlx::query_as::<_, SiteRow>(
            "SELECT s.id, s.name, s.subdomain, s.status, s.theme, s.default_locale, \
                    s.enabled_locales, s.created_by, s.created_at, s.updated_at \
             FROM sites s JOIN site_editor_grants g \
               ON g.tenant_id = s.tenant_id AND g.site_id = s.id \
             WHERE g.tenant_id = $1 AND g.user_id = $2 \
             ORDER BY lower(s.name), s.id",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(SiteRow::into_site).collect()
    }
}
