//! Spaces — the membership spine of the workspace (ADR 0026), tenant/user-
//! scoped through the account door exactly like [`crate::tasks`]. A Space owns
//! members and per-member roles; modules (Drive first) attach and inherit them.
//! A non-member — same tenant or another — gets [`StoreError::NotFound`], which
//! hides the Space's existence; a member who lacks the role for an action gets
//! [`StoreError::Forbidden`], which does not (they already know it exists).

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::{SpaceId, UserId};

/// A member's role in a Space. Ordered: a higher role can do everything a
/// lower one can (`Manager` ≥ `Editor` ≥ `Viewer`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SpaceRole {
    /// Read the Space's contents.
    Viewer,
    /// Viewer + create/edit/upload/move/delete within the Space.
    Editor,
    /// Editor + membership, rename/archive, enable/disable modules.
    Manager,
}

impl SpaceRole {
    /// The wire/storage token for this role.
    pub fn as_str(self) -> &'static str {
        match self {
            SpaceRole::Viewer => "viewer",
            SpaceRole::Editor => "editor",
            SpaceRole::Manager => "manager",
        }
    }

    /// Parses a stored/wire token, rejecting anything else.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "viewer" => Some(SpaceRole::Viewer),
            "editor" => Some(SpaceRole::Editor),
            "manager" => Some(SpaceRole::Manager),
            _ => None,
        }
    }
}

/// A Space as seen by a member, including that member's own role.
#[derive(Debug, Clone)]
pub struct Space {
    pub id: SpaceId,
    pub name: String,
    pub created_by: String,
    pub created_at: OffsetDateTime,
    pub archived: bool,
    /// The calling user's role in this Space.
    pub my_role: SpaceRole,
}

/// One membership row.
#[derive(Debug, Clone)]
pub struct SpaceMember {
    pub user_id: String,
    pub role: SpaceRole,
    pub added_at: OffsetDateTime,
}

impl AccountStore {
    /// Creates a Space, making the caller its first `manager` and enabling the
    /// `files` module — all in one transaction.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn create_space(&self, name: &str) -> Result<SpaceId> {
        let id = SpaceId::generate();
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        sqlx::query("INSERT INTO spaces (tenant_id, id, name, created_by) VALUES ($1, $2, $3, $4)")
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .bind(name)
            .bind(self.user.as_str())
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        sqlx::query(
            "INSERT INTO space_members (tenant_id, space_id, user_id, role) \
             VALUES ($1, $2, $3, 'manager')",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(self.user.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        sqlx::query(
            "INSERT INTO space_modules (tenant_id, space_id, module) VALUES ($1, $2, 'files')",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(id)
    }

    /// The Spaces the caller belongs to (with their own role), name order,
    /// archived last.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn spaces(&self) -> Result<Vec<Space>> {
        let rows = sqlx::query_as::<_, SpaceRow>(
            "SELECT s.id, s.name, s.created_by, s.created_at, s.archived, m.role \
             FROM spaces s \
             JOIN space_members m ON m.tenant_id = s.tenant_id AND m.space_id = s.id \
             WHERE s.tenant_id = $1 AND m.user_id = $2 \
             ORDER BY s.archived, lower(s.name), s.id",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter().map(SpaceRow::into_space).collect()
    }

    /// A single Space the caller can see (is a member of), or `None`.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn space(&self, id: &SpaceId) -> Result<Option<Space>> {
        let row = sqlx::query_as::<_, SpaceRow>(
            "SELECT s.id, s.name, s.created_by, s.created_at, s.archived, m.role \
             FROM spaces s \
             JOIN space_members m ON m.tenant_id = s.tenant_id AND m.space_id = s.id \
             WHERE s.tenant_id = $1 AND s.id = $3 AND m.user_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        row.map(SpaceRow::into_space).transpose()
    }

    /// The caller's role in a Space, or `None` if they are not a member (or it
    /// isn't their tenant's). The reusable gate every attached module uses.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn space_role(&self, id: &SpaceId) -> Result<Option<SpaceRole>> {
        let role: Option<String> = sqlx::query_scalar(
            "SELECT role FROM space_members WHERE tenant_id = $1 AND space_id = $2 AND user_id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(self.user.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(role.and_then(|r| SpaceRole::parse(&r)))
    }

    /// Requires the caller to hold at least `min` in the Space. Returns their
    /// actual role, or `NotFound` if they are not a member, or `Forbidden` if
    /// they are a member but below `min`.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] / [`StoreError::Forbidden`] / [`StoreError::Db`].
    pub async fn require_space_role(&self, id: &SpaceId, min: SpaceRole) -> Result<SpaceRole> {
        match self.space_role(id).await? {
            None => Err(StoreError::NotFound),
            Some(role) if role < min => Err(StoreError::Forbidden),
            Some(role) => Ok(role),
        }
    }

    /// The members of a Space the caller can see. Any member may see the
    /// membership — trust is visible (Law).
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the caller isn't a member; [`StoreError::Db`].
    pub async fn space_members(&self, id: &SpaceId) -> Result<Vec<SpaceMember>> {
        self.require_space_role(id, SpaceRole::Viewer).await?;
        let rows = sqlx::query_as::<_, MemberRow>(
            "SELECT user_id, role, added_at FROM space_members \
             WHERE tenant_id = $1 AND space_id = $2 ORDER BY added_at, user_id",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter().map(MemberRow::into_member).collect()
    }

    /// Adds (or re-roles) a member. Manager only. The target user must belong to
    /// the same tenant.
    ///
    /// # Errors
    /// [`StoreError::NotFound`]/[`StoreError::Forbidden`] per role;
    /// [`StoreError::Conflict`] if the target user isn't in this tenant;
    /// [`StoreError::Db`].
    pub async fn add_space_member(
        &self,
        id: &SpaceId,
        user: &UserId,
        role: SpaceRole,
    ) -> Result<()> {
        self.require_space_role(id, SpaceRole::Manager).await?;
        // The target must be a real user in this tenant — never add a user id
        // from another tenant into our space.
        let exists: Option<String> =
            sqlx::query_scalar("SELECT id FROM users WHERE tenant_id = $1 AND id = $2")
                .bind(self.tenant.as_str())
                .bind(user.as_str())
                .fetch_optional(&self.pool)
                .await
                .map_err(StoreError::Db)?;
        if exists.is_none() {
            return Err(StoreError::Conflict("user is not in this tenant".into()));
        }
        // Demoting the Space's last manager would leave it unmanageable.
        if role < SpaceRole::Manager {
            self.guard_last_manager(id, user).await?;
        }
        sqlx::query(
            "INSERT INTO space_members (tenant_id, space_id, user_id, role) VALUES ($1, $2, $3, $4) \
             ON CONFLICT (tenant_id, space_id, user_id) DO UPDATE SET role = EXCLUDED.role",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(user.as_str())
        .bind(role.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }

    /// Removes a member. Manager only. Refuses to remove the Space's last
    /// manager (a Space must always be manageable).
    ///
    /// # Errors
    /// [`StoreError::NotFound`]/[`StoreError::Forbidden`] per role;
    /// [`StoreError::Conflict`] on removing the last manager; [`StoreError::Db`].
    pub async fn remove_space_member(&self, id: &SpaceId, user: &UserId) -> Result<()> {
        self.require_space_role(id, SpaceRole::Manager).await?;
        self.guard_last_manager(id, user).await?;
        sqlx::query(
            "DELETE FROM space_members WHERE tenant_id = $1 AND space_id = $2 AND user_id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(user.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }

    /// Renames a Space. Manager only.
    ///
    /// # Errors
    /// [`StoreError::NotFound`]/[`StoreError::Forbidden`]/[`StoreError::Db`].
    pub async fn rename_space(&self, id: &SpaceId, name: &str) -> Result<()> {
        self.require_space_role(id, SpaceRole::Manager).await?;
        sqlx::query("UPDATE spaces SET name = $3 WHERE tenant_id = $1 AND id = $2")
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .bind(name)
            .execute(&self.pool)
            .await
            .map_err(StoreError::Db)?;
        Ok(())
    }

    /// Archives or unarchives a Space. Manager only.
    ///
    /// # Errors
    /// [`StoreError::NotFound`]/[`StoreError::Forbidden`]/[`StoreError::Db`].
    pub async fn set_space_archived(&self, id: &SpaceId, archived: bool) -> Result<()> {
        self.require_space_role(id, SpaceRole::Manager).await?;
        sqlx::query("UPDATE spaces SET archived = $3 WHERE tenant_id = $1 AND id = $2")
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .bind(archived)
            .execute(&self.pool)
            .await
            .map_err(StoreError::Db)?;
        Ok(())
    }

    /// The modules enabled on a Space the caller can see.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when not a member; [`StoreError::Db`].
    pub async fn space_modules(&self, id: &SpaceId) -> Result<Vec<String>> {
        self.require_space_role(id, SpaceRole::Viewer).await?;
        let mods: Vec<String> = sqlx::query_scalar(
            "SELECT module FROM space_modules WHERE tenant_id = $1 AND space_id = $2 ORDER BY module",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(mods)
    }

    /// Enables or disables a module on a Space. Manager only.
    ///
    /// # Errors
    /// [`StoreError::NotFound`]/[`StoreError::Forbidden`]/[`StoreError::Db`].
    pub async fn set_space_module(&self, id: &SpaceId, module: &str, enabled: bool) -> Result<()> {
        self.require_space_role(id, SpaceRole::Manager).await?;
        if enabled {
            sqlx::query(
                "INSERT INTO space_modules (tenant_id, space_id, module) VALUES ($1, $2, $3) \
                 ON CONFLICT DO NOTHING",
            )
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .bind(module)
            .execute(&self.pool)
            .await
            .map_err(StoreError::Db)?;
        } else {
            sqlx::query(
                "DELETE FROM space_modules WHERE tenant_id = $1 AND space_id = $2 AND module = $3",
            )
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .bind(module)
            .execute(&self.pool)
            .await
            .map_err(StoreError::Db)?;
        }
        Ok(())
    }

    /// Refuses an operation that would leave a Space with no manager.
    async fn guard_last_manager(&self, id: &SpaceId, target: &UserId) -> Result<()> {
        let managers: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM space_members \
             WHERE tenant_id = $1 AND space_id = $2 AND role = 'manager'",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        let target_is_manager = self
            .space_member_role(id, target)
            .await?
            .is_some_and(|r| r == SpaceRole::Manager);
        if target_is_manager && managers <= 1 {
            return Err(StoreError::Conflict(
                "a space must keep at least one manager".into(),
            ));
        }
        Ok(())
    }

    /// A specific member's role in a Space (not the caller's), or `None`.
    async fn space_member_role(&self, id: &SpaceId, user: &UserId) -> Result<Option<SpaceRole>> {
        let role: Option<String> = sqlx::query_scalar(
            "SELECT role FROM space_members WHERE tenant_id = $1 AND space_id = $2 AND user_id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(user.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(role.and_then(|r| SpaceRole::parse(&r)))
    }
}

// ---- row types --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct SpaceRow {
    id: String,
    name: String,
    created_by: String,
    created_at: OffsetDateTime,
    archived: bool,
    role: String,
}
impl SpaceRow {
    fn into_space(self) -> Result<Space> {
        Ok(Space {
            id: SpaceId::new(self.id),
            name: self.name,
            created_by: self.created_by,
            created_at: self.created_at,
            archived: self.archived,
            my_role: SpaceRole::parse(&self.role).ok_or(StoreError::NotFound)?,
        })
    }
}

#[derive(sqlx::FromRow)]
struct MemberRow {
    user_id: String,
    role: String,
    added_at: OffsetDateTime,
}
impl MemberRow {
    fn into_member(self) -> Result<SpaceMember> {
        Ok(SpaceMember {
            user_id: self.user_id,
            role: SpaceRole::parse(&self.role).ok_or(StoreError::NotFound)?,
            added_at: self.added_at,
        })
    }
}
