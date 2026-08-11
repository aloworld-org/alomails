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

/// A collaborator shown on one site's sharing surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteEditorCollaborator {
    pub user: UserId,
    pub email: String,
    pub pending: bool,
}

/// Result of inviting an address. Existing restricted collaborators need no
/// new setup link; a new or still-pending account does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SiteEditorInviteOutcome {
    Active(SiteEditorCollaborator),
    Pending(SiteEditorCollaborator),
}

/// The non-secret facts behind a live one-time setup token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteEditorInviteTarget {
    pub tenant: crate::TenantId,
    pub user: UserId,
    pub email: String,
    pub site_name: String,
}

impl TenantStore {
    /// Invites a new restricted collaborator, or adds another site to an
    /// existing restricted collaborator. An ordinary workspace member is
    /// deliberately refused: turning their existing account into a Sites-only
    /// account would silently remove access to the rest of alo.
    pub async fn invite_site_editor(
        &self,
        email: &str,
        site: &SiteId,
        granted_by: &UserId,
        token_hash: &str,
        ttl_hours: i64,
    ) -> Result<SiteEditorInviteOutcome> {
        self.assert_user(granted_by).await?;
        let email = email.trim().to_ascii_lowercase();
        if email.is_empty() {
            return Err(StoreError::Validation(
                "Enter the collaborator's email address.".to_owned(),
            ));
        }
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

        let existing: Option<(String, bool, bool, bool)> = sqlx::query_as(
            "SELECT u.id, u.is_admin, \
                    EXISTS(SELECT 1 FROM tenant_user_roles r \
                            WHERE r.tenant_id = u.tenant_id AND r.user_id = u.id \
                              AND r.role = 'site_editor'), \
                    EXISTS(SELECT 1 FROM credentials c WHERE c.user_id = u.id) \
               FROM users u WHERE u.tenant_id = $1 AND lower(u.email) = $2",
        )
        .bind(self.tenant().as_str())
        .bind(&email)
        .fetch_optional(&mut *tx)
        .await?;

        let (user, already_active) = match existing {
            Some((id, is_admin, is_site_editor, has_credentials)) => {
                if is_admin {
                    return Err(StoreError::Conflict(
                        "Workspace administrators already have access to every website.".to_owned(),
                    ));
                }
                if !is_site_editor {
                    return Err(StoreError::Conflict(
                        "That address already has a workspace account. Use a different address for site-only access."
                            .to_owned(),
                    ));
                }
                (UserId::new(id), has_credentials)
            }
            None => {
                let user = UserId::generate();
                sqlx::query("INSERT INTO users (id, tenant_id, email) VALUES ($1, $2, $3)")
                    .bind(user.as_str())
                    .bind(self.tenant().as_str())
                    .bind(&email)
                    .execute(&mut *tx)
                    .await?;
                (user, false)
            }
        };

        sqlx::query(
            "INSERT INTO tenant_user_roles (tenant_id, user_id, role, granted_by) \
             VALUES ($1, $2, 'site_editor', $3) \
             ON CONFLICT (tenant_id, user_id, role) DO NOTHING",
        )
        .bind(self.tenant().as_str())
        .bind(user.as_str())
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

        if !already_active {
            sqlx::query(
                "INSERT INTO site_editor_invites \
                     (token_hash, tenant_id, user_id, email, invited_by, expires_at) \
                 VALUES ($1, $2, $3, $4, $5, now() + ($6::bigint * interval '1 hour')) \
                 ON CONFLICT (tenant_id, user_id) DO UPDATE SET \
                     token_hash = EXCLUDED.token_hash, email = EXCLUDED.email, \
                     invited_by = EXCLUDED.invited_by, created_at = now(), \
                     expires_at = EXCLUDED.expires_at, accepted_at = NULL",
            )
            .bind(token_hash)
            .bind(self.tenant().as_str())
            .bind(user.as_str())
            .bind(&email)
            .bind(granted_by.as_str())
            .bind(ttl_hours)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;

        let collaborator = SiteEditorCollaborator {
            user,
            email,
            pending: !already_active,
        };
        Ok(if already_active {
            SiteEditorInviteOutcome::Active(collaborator)
        } else {
            SiteEditorInviteOutcome::Pending(collaborator)
        })
    }

    /// Every collaborator granted to one tenant-owned site, email-sorted.
    pub async fn site_editors(&self, site: &SiteId) -> Result<Vec<SiteEditorCollaborator>> {
        let site_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM sites WHERE tenant_id = $1 AND id = $2)",
        )
        .bind(self.tenant().as_str())
        .bind(site.as_str())
        .fetch_one(self.pool())
        .await?;
        if !site_exists {
            return Err(StoreError::NotFound);
        }
        let rows: Vec<(String, String, bool)> = sqlx::query_as(
            "SELECT u.id, u.email, NOT EXISTS(SELECT 1 FROM credentials c WHERE c.user_id = u.id) \
               FROM site_editor_grants g \
               JOIN users u ON u.tenant_id = g.tenant_id AND u.id = g.user_id \
              WHERE g.tenant_id = $1 AND g.site_id = $2 \
              ORDER BY lower(u.email), u.id",
        )
        .bind(self.tenant().as_str())
        .bind(site.as_str())
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|(user, email, pending)| SiteEditorCollaborator {
                user: UserId::new(user),
                email,
                pending,
            })
            .collect())
    }

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
        // An account created by the Sites invitation flow exists only to edit
        // its granted sites. Removing its final grant removes the account too;
        // otherwise the cleanup trigger would turn it into an ordinary member.
        sqlx::query(
            "DELETE FROM users u WHERE u.tenant_id = $1 AND u.id = $2 \
               AND EXISTS(SELECT 1 FROM site_editor_invites i \
                          WHERE i.tenant_id = u.tenant_id AND i.user_id = u.id) \
               AND NOT EXISTS(SELECT 1 FROM site_editor_grants g \
                              WHERE g.tenant_id = u.tenant_id AND g.user_id = u.id)",
        )
        .bind(self.tenant().as_str())
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

impl crate::Store {
    /// Resolves a live setup link without revealing expired/used tokens.
    pub async fn site_editor_invite(
        &self,
        token_hash: &str,
    ) -> Result<Option<SiteEditorInviteTarget>> {
        let row: Option<(String, String, String, String)> = sqlx::query_as(
            "SELECT i.tenant_id, i.user_id, i.email, s.name \
               FROM site_editor_invites i \
               JOIN site_editor_grants g \
                 ON g.tenant_id = i.tenant_id AND g.user_id = i.user_id \
               JOIN sites s ON s.tenant_id = g.tenant_id AND s.id = g.site_id \
              WHERE i.token_hash = $1 AND i.accepted_at IS NULL AND i.expires_at > now() \
              ORDER BY i.created_at DESC, s.name LIMIT 1",
        )
        .bind(token_hash)
        .fetch_optional(self.pool())
        .await?;
        Ok(
            row.map(|(tenant, user, email, site_name)| SiteEditorInviteTarget {
                tenant: crate::TenantId::new(tenant),
                user: UserId::new(user),
                email,
                site_name,
            }),
        )
    }

    /// Atomically installs the new credential and spends its setup token.
    pub async fn accept_site_editor_invite(
        &self,
        token_hash: &str,
        password_hash: &str,
    ) -> Result<Option<SiteEditorInviteTarget>> {
        let mut tx = self.pool().begin().await?;
        let row: Option<(String, String, String, String)> = sqlx::query_as(
            "SELECT i.tenant_id, i.user_id, i.email, s.name \
               FROM site_editor_invites i \
               JOIN site_editor_grants g \
                 ON g.tenant_id = i.tenant_id AND g.user_id = i.user_id \
               JOIN sites s ON s.tenant_id = g.tenant_id AND s.id = g.site_id \
              WHERE i.token_hash = $1 AND i.accepted_at IS NULL AND i.expires_at > now() \
              ORDER BY s.name LIMIT 1 FOR UPDATE OF i",
        )
        .bind(token_hash)
        .fetch_optional(&mut *tx)
        .await?;
        let Some((tenant, user, email, site_name)) = row else {
            return Ok(None);
        };
        sqlx::query(
            "INSERT INTO credentials (user_id, tenant_id, username, password_hash) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(&user)
        .bind(&tenant)
        .bind(&email)
        .bind(password_hash)
        .execute(&mut *tx)
        .await?;
        sqlx::query("UPDATE site_editor_invites SET accepted_at = now() WHERE token_hash = $1")
            .bind(token_hash)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(Some(SiteEditorInviteTarget {
            tenant: crate::TenantId::new(tenant),
            user: UserId::new(user),
            email,
            site_name,
        }))
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
