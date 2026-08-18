//! Per-tenant DKIM signing-key persistence (ADR 0014, Law 3: kept out of
//! `store.rs`). Storage only — key *generation* is crypto and lives in
//! `alo-auth-mail`; this module persists the seed + public bytes it produces
//! and hands the seed back only to the signer. The secret seed is never
//! returned to a client (the admin/operator API exposes only the public record
//! via [`DkimKeyRow`]).
//!
//! New table (migration 0016) is not in the offline query cache, so these use
//! the runtime `sqlx::query*` path.

use time::OffsetDateTime;

use crate::error::{Result, StoreError};
use crate::id::{self, TenantId};
use crate::model::DkimKeyRow;
use crate::store::{Store, TenantStore};

/// The material the signer needs to sign for a domain: the active key's
/// selector, its algorithm tag, and the secret seed. Internal — never leaves
/// the process except as a DKIM signature.
pub struct DkimSigningMaterial {
    pub selector: String,
    pub algorithm: String,
    pub seed: Vec<u8>,
}

impl Store {
    /// Installs `(selector, seed, public_raw)` as the **active** DKIM key for
    /// `(tenant, domain)`, deactivating any previous active key for that domain
    /// in the same transaction. This is both initial provisioning and rotation
    /// (rotation just passes a new selector). The previous key rows are kept
    /// `active = false` so their published records still verify in-flight mail
    /// until the operator removes them.
    ///
    /// # Errors
    /// [`StoreError::Conflict`] if the selector already exists for the domain;
    /// [`StoreError::Db`] on failure.
    pub async fn install_active_dkim_key(
        &self,
        tenant: &TenantId,
        domain: &str,
        selector: &str,
        // `"rsa"` or `"ed25519"` - the `a=` family this key signs with. A domain
        // holds one active key per algorithm, not one overall.
        algorithm: &str,
        seed: &[u8],
        public_raw: &[u8],
    ) -> Result<()> {
        let domain = domain.trim().to_lowercase();
        let mut tx = self.pool().begin().await.map_err(StoreError::Db)?;
        // Retires the previous key **of this algorithm only**. A domain that
        // signs with both RSA and Ed25519 must be able to rotate one without
        // unpublishing the other - retiring both here would halve its
        // verifiable audience for as long as DNS took to catch up.
        sqlx::query(
            "UPDATE dkim_keys SET active = FALSE \
             WHERE tenant_id = $1 AND domain = $2 AND algorithm = $3",
        )
        .bind(tenant.as_str())
        .bind(&domain)
        .bind(algorithm)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO dkim_keys \
                 (id, tenant_id, domain, selector, algorithm, seed, public_raw, active) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, TRUE)",
        )
        .bind(id::generate_token())
        .bind(tenant.as_str())
        .bind(&domain)
        .bind(selector)
        .bind(algorithm)
        .bind(seed)
        .bind(public_raw)
        .execute(&mut *tx)
        .await?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(())
    }

    /// The active DKIM signing material for `domain` (the sending domain), or
    /// `None` if the domain has no stored key (the signer then falls back to the
    /// configured file key). Domains are globally unique to one tenant, so this
    /// resolves by domain alone.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn active_dkim_material(&self, domain: &str) -> Result<Option<DkimSigningMaterial>> {
        Ok(self.active_dkim_materials(domain).await?.into_iter().next())
    }

    /// **Every** active signing key for `domain` - one per algorithm, which is
    /// what a dual-signing sender needs (roadmap C2.1a).
    ///
    /// RSA is ordered first. Both signatures are valid over the same message and
    /// a verifier may take either, but RFC 8463 is young enough that some cannot
    /// read Ed25519 at all, and a campaign is exactly where an unverifiable
    /// signature costs delivery. Putting the widely-understood one first is a
    /// courtesy to old verifiers rather than a correctness matter.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn active_dkim_materials(&self, domain: &str) -> Result<Vec<DkimSigningMaterial>> {
        let domain = domain.trim().to_lowercase();
        let rows = sqlx::query_as::<_, (String, String, Vec<u8>)>(
            "SELECT selector, algorithm, seed FROM dkim_keys \
             WHERE domain = $1 AND active \
             ORDER BY (algorithm <> 'rsa'), selector",
        )
        .bind(&domain)
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|(selector, algorithm, seed)| DkimSigningMaterial {
                selector,
                algorithm,
                seed,
            })
            .collect())
    }
}

impl TenantStore {
    /// This tenant's DKIM keys for `domain` (active first), for the Domains
    /// view. Returns only public material — never the secret seed.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn list_dkim_keys(&self, domain: &str) -> Result<Vec<DkimKeyRow>> {
        let domain = domain.trim().to_lowercase();
        let rows = sqlx::query_as::<_, (String, String, Vec<u8>, bool, OffsetDateTime)>(
            "SELECT selector, algorithm, public_raw, active, created_at FROM dkim_keys \
             WHERE tenant_id = $1 AND domain = $2 ORDER BY active DESC, created_at DESC",
        )
        .bind(self.tenant().as_str())
        .bind(&domain)
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(
                |(selector, algorithm, public_raw, active, created_at)| DkimKeyRow {
                    selector,
                    algorithm,
                    public_raw,
                    active,
                    created_at,
                },
            )
            .collect())
    }

    /// Removes all DKIM keys for `domain` in this tenant (used when a domain is
    /// removed — `dkim_keys` references the tenant, not the domain, so this is
    /// explicit rather than a cascade).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn delete_dkim_keys(&self, domain: &str) -> Result<()> {
        let domain = domain.trim().to_lowercase();
        sqlx::query("DELETE FROM dkim_keys WHERE tenant_id = $1 AND domain = $2")
            .bind(self.tenant().as_str())
            .bind(&domain)
            .execute(self.pool())
            .await?;
        Ok(())
    }
}
