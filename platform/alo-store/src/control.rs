//! Control-plane persistence (ADR 0012, Law 3: kept out of `store.rs`).
//!
//! These are **deployment-global** operations — the platform operator's view
//! across all tenants — plus the tenant-scoped domain reads a tenant admin
//! needs for its own domains. They are the only store methods besides the
//! identity pre-tenant lookups that legitimately span tenants, and they do so
//! for *governance* (list/suspend/delete a tenant, own a domain), never to read
//! a tenant's mail: there is no method here that returns message, blob, or
//! mailbox data.
//!
//! New tables/columns land in migration 0013 and are not yet in the compiled
//! offline query cache, so every statement here uses the runtime `sqlx::query*`
//! path (the deployment DB is unreachable from dev to regenerate the cache).

use time::OffsetDateTime;

use crate::error::{Result, StoreError};
use crate::id::{self, TenantId, UserId};
use crate::model::{DomainRow, TenantSummary};
use crate::store::{Store, TenantStore};

/// The reserved system tenant that owns platform operators. Its name is fixed
/// so `bootstrap-operator` is idempotent about which tenant operators live in.
pub const PLATFORM_TENANT_NAME: &str = "_platform";

impl Store {
    // ---- platform operator (cross-tenant governance) ------------------

    /// Whether `(tenant, user)` is a platform operator — the single
    /// cross-tenant role (ADR 0012). Used to gate `/control/*`.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn user_is_platform_admin(&self, tenant: &TenantId, user: &UserId) -> Result<bool> {
        let found: Option<bool> = sqlx::query_scalar(
            "SELECT is_platform_admin FROM users WHERE tenant_id = $1 AND id = $2",
        )
        .bind(tenant.as_str())
        .bind(user.as_str())
        .fetch_optional(self.pool())
        .await?;
        Ok(found.unwrap_or(false))
    }

    /// The reserved `_platform` system tenant that owns operators, if it has
    /// been created (by `bootstrap-operator`), else `None`.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn platform_tenant(&self) -> Result<Option<TenantId>> {
        let id: Option<String> = sqlx::query_scalar(
            "SELECT id FROM tenants WHERE name = $1 ORDER BY created_at LIMIT 1",
        )
        .bind(PLATFORM_TENANT_NAME)
        .fetch_optional(self.pool())
        .await?;
        Ok(id.map(TenantId::new))
    }

    /// Sets or clears a user's platform-operator flag.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the user does not exist;
    /// [`StoreError::Db`] on failure.
    pub async fn set_platform_admin(
        &self,
        tenant: &TenantId,
        user: &UserId,
        is_platform_admin: bool,
    ) -> Result<()> {
        let done =
            sqlx::query("UPDATE users SET is_platform_admin = $3 WHERE tenant_id = $1 AND id = $2")
                .bind(tenant.as_str())
                .bind(user.as_str())
                .bind(is_platform_admin)
                .execute(self.pool())
                .await?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    // ---- tenant lifecycle (control plane) -----------------------------

    /// Lists every tenant with lifecycle status and aggregated usage, newest
    /// first. Deployment-global — the operator's view, not a tenant's data.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn list_tenants(&self) -> Result<Vec<TenantSummary>> {
        let rows = sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                OffsetDateTime,
                i64,
                i64,
                Option<i64>,
            ),
        >(
            "SELECT t.id, t.name, t.status, t.created_at, \
                    COALESCE(u.n, 0)::bigint AS user_count, \
                    COALESCE(b.bytes, 0)::bigint AS storage_bytes, \
                    t.storage_quota_bytes \
             FROM tenants t \
             LEFT JOIN (SELECT tenant_id, COUNT(*) AS n FROM users GROUP BY tenant_id) u \
                    ON u.tenant_id = t.id \
             LEFT JOIN (SELECT tenant_id, SUM(size) AS bytes FROM blobs GROUP BY tenant_id) b \
                    ON b.tenant_id = t.id \
             ORDER BY t.created_at DESC",
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(
                |(id, name, status, created_at, user_count, storage_bytes, storage_quota_bytes)| {
                    TenantSummary {
                        id,
                        name,
                        status,
                        created_at,
                        user_count,
                        storage_bytes,
                        storage_quota_bytes,
                    }
                },
            )
            .collect())
    }

    /// Sets (or clears, with `None`) a tenant's storage quota in bytes. `None`
    /// is unlimited. Enforced at the blob-write choke points (ADR 0012).
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the tenant does not exist;
    /// [`StoreError::Db`] on failure.
    pub async fn set_tenant_quota(&self, tenant: &TenantId, quota: Option<i64>) -> Result<()> {
        let done = sqlx::query("UPDATE tenants SET storage_quota_bytes = $2 WHERE id = $1")
            .bind(tenant.as_str())
            .bind(quota)
            .execute(self.pool())
            .await?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// The lifecycle status of one tenant (`active` | `suspended`), or `None`
    /// if the tenant does not exist.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn tenant_status(&self, tenant: &TenantId) -> Result<Option<String>> {
        let status: Option<String> = sqlx::query_scalar("SELECT status FROM tenants WHERE id = $1")
            .bind(tenant.as_str())
            .fetch_optional(self.pool())
            .await?;
        Ok(status)
    }

    /// Sets a tenant's lifecycle status. `suspended` denies auth and defers
    /// inbound mail; `active` restores it. Reversible, touches no data.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the tenant does not exist;
    /// [`StoreError::Db`] on failure.
    pub async fn set_tenant_status(&self, tenant: &TenantId, status: &str) -> Result<()> {
        if status != "active" && status != "suspended" {
            return Err(StoreError::Conflict(format!("invalid status: {status}")));
        }
        let done = sqlx::query("UPDATE tenants SET status = $2 WHERE id = $1")
            .bind(tenant.as_str())
            .bind(status)
            .execute(self.pool())
            .await?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Permanently deletes a tenant and, by `ON DELETE CASCADE`, all of its
    /// rows. Garage blob objects for the tenant are not swept here (a recorded
    /// storage-cost follow-up, not a leak — see ADR 0012).
    ///
    /// One table is cleared by hand first, in the same transaction (B7.02).
    /// `bank_matches` carries no key to `tenants`: its erasure rides the
    /// cascade `tenants → bank_statements → bank_lines → bank_matches`, and
    /// Postgres fires queued foreign-key events in order — so the check
    /// guarding its payment and entry links (migration 0174) can run while
    /// that two-hop cascade has not reached the matches yet, and refuse an
    /// erasure the next ordering would have allowed. Deleting the tenant's
    /// matches before the cascade starts makes erasure — a GDPR obligation —
    /// independent of that order.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the tenant does not exist;
    /// [`StoreError::Db`] on failure.
    pub async fn delete_tenant(&self, tenant: &TenantId) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM bank_matches WHERE tenant_id = $1")
            .bind(tenant.as_str())
            .execute(&mut *tx)
            .await?;
        let done = sqlx::query("DELETE FROM tenants WHERE id = $1")
            .bind(tenant.as_str())
            .execute(&mut *tx)
            .await?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        tx.commit().await?;
        Ok(())
    }

    // ---- domains (tenant -> domain ownership) -------------------------

    /// Registers `domain` (lowercased) as owned by `tenant`, unverified, and
    /// returns the DNS verification token to publish at `_alo-verify.<domain>`.
    /// The `domains` primary key makes a second tenant's claim impossible.
    ///
    /// # Errors
    /// [`StoreError::Conflict`] if the domain is already registered;
    /// [`StoreError::Db`] on failure.
    pub async fn create_domain(&self, tenant: &TenantId, domain: &str) -> Result<DomainRow> {
        let domain = domain.trim().to_lowercase();
        let token = id::generate_token();
        // A duplicate `domain` PK surfaces as SQLSTATE 23505, mapped to
        // `StoreError::Conflict` by the blanket `From<sqlx::Error>`.
        let created_at: OffsetDateTime = sqlx::query_scalar(
            "INSERT INTO domains (domain, tenant_id, verify_token) VALUES ($1, $2, $3) \
             RETURNING created_at",
        )
        .bind(&domain)
        .bind(tenant.as_str())
        .bind(&token)
        .fetch_one(self.pool())
        .await?;
        Ok(DomainRow {
            domain,
            tenant_id: tenant.as_str().to_owned(),
            verify_token: token,
            verified_at: None,
            created_at,
        })
    }

    /// The domain record, or `None` if the domain is not registered.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn domain_record(&self, domain: &str) -> Result<Option<DomainRow>> {
        let domain = domain.trim().to_lowercase();
        let row = sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                Option<OffsetDateTime>,
                OffsetDateTime,
            ),
        >(
            "SELECT domain, tenant_id, verify_token, verified_at, created_at \
             FROM domains WHERE domain = $1",
        )
        .bind(&domain)
        .fetch_optional(self.pool())
        .await?;
        Ok(row.map(
            |(domain, tenant_id, verify_token, verified_at, created_at)| DomainRow {
                domain,
                tenant_id,
                verify_token,
                verified_at,
                created_at,
            },
        ))
    }

    /// Every registered domain across the deployment (operator view), newest
    /// first. Deployment-global — not a tenant's data.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn list_all_domains(&self) -> Result<Vec<DomainRow>> {
        let rows = sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                Option<OffsetDateTime>,
                OffsetDateTime,
            ),
        >(
            "SELECT domain, tenant_id, verify_token, verified_at, created_at \
             FROM domains ORDER BY created_at DESC",
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(
                |(domain, tenant_id, verify_token, verified_at, created_at)| DomainRow {
                    domain,
                    tenant_id,
                    verify_token,
                    verified_at,
                    created_at,
                },
            )
            .collect())
    }

    /// Stamps a domain verified (the DNS TXT proof was observed by the caller).
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the domain is not registered;
    /// [`StoreError::Db`] on failure.
    pub async fn set_domain_verified(&self, domain: &str) -> Result<()> {
        let domain = domain.trim().to_lowercase();
        let done = sqlx::query("UPDATE domains SET verified_at = now() WHERE domain = $1")
            .bind(&domain)
            .execute(self.pool())
            .await?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Removes a domain registration.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the domain is not registered;
    /// [`StoreError::Db`] on failure.
    pub async fn delete_domain(&self, domain: &str) -> Result<()> {
        let domain = domain.trim().to_lowercase();
        let done = sqlx::query("DELETE FROM domains WHERE domain = $1")
            .bind(&domain)
            .execute(self.pool())
            .await?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Whether `tenant` owns `domain` **and** it is verified — the predicate the
    /// address-assignment guard uses (ADR 0012 security spine).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn tenant_owns_verified_domain(
        &self,
        tenant: &TenantId,
        domain: &str,
    ) -> Result<bool> {
        let domain = domain.trim().to_lowercase();
        let owned: Option<bool> = sqlx::query_scalar(
            "SELECT (verified_at IS NOT NULL) FROM domains WHERE domain = $1 AND tenant_id = $2",
        )
        .bind(&domain)
        .bind(tenant.as_str())
        .fetch_optional(self.pool())
        .await?;
        Ok(owned.unwrap_or(false))
    }

    /// Whether any domain is registered on this deployment. When none are, the
    /// deployment is single-tenant/dev and ownership enforcement stays inert
    /// even if the flag is on (nothing to enforce against).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn any_domains_registered(&self) -> Result<bool> {
        let one: Option<i32> = sqlx::query_scalar("SELECT 1 FROM domains LIMIT 1")
            .fetch_optional(self.pool())
            .await?;
        Ok(one.is_some())
    }
}

impl TenantStore {
    /// Lists this tenant's own domains (tenant-admin view). Scoped by the
    /// handle's tenant, so it can never see another tenant's domains.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn list_domains(&self) -> Result<Vec<DomainRow>> {
        let rows = sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                Option<OffsetDateTime>,
                OffsetDateTime,
            ),
        >(
            "SELECT domain, tenant_id, verify_token, verified_at, created_at \
             FROM domains WHERE tenant_id = $1 ORDER BY created_at",
        )
        .bind(self.tenant().as_str())
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(
                |(domain, tenant_id, verify_token, verified_at, created_at)| DomainRow {
                    domain,
                    tenant_id,
                    verify_token,
                    verified_at,
                    created_at,
                },
            )
            .collect())
    }
}
