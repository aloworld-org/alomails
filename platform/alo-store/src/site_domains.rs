//! Tenant-safe custom-domain claims for alo Sites.

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::{self, SiteId};

/// Maximum DNS-name length without the optional root dot.
pub const SITE_DOMAIN_MAX_LEN: usize = 253;

/// A custom host's activation state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiteDomainStatus {
    /// Claimed, waiting for the ownership TXT record.
    Pending,
    /// Ownership TXT record observed; serving may now provision the host.
    Verified,
    /// The public Sites service is prepared to serve this host.
    Live,
}

impl SiteDomainStatus {
    /// Stable storage and wire token.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Verified => "verified",
            Self::Live => "live",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "verified" => Some(Self::Verified),
            "live" => Some(Self::Live),
            _ => None,
        }
    }
}

/// One custom DNS host connected to a site.
#[derive(Debug, Clone)]
pub struct SiteDomain {
    pub site_id: SiteId,
    pub domain: String,
    /// Opaque ownership proof placed in the TXT record exposed by the API.
    pub verify_token: String,
    pub status: SiteDomainStatus,
    pub verified_at: Option<OffsetDateTime>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

/// Canonicalizes and validates a bare DNS host.
///
/// The input may contain surrounding whitespace or uppercase ASCII; the
/// returned value is lowercase. Schemes, ports, paths, wildcards, IP literals,
/// underscores, empty labels and leading/trailing label hyphens are refused.
/// Punycode labels are accepted; raw Unicode is deliberately not normalized.
///
/// # Errors
/// [`StoreError::Validation`] with a safe, field-level sentence.
pub fn normalize_site_domain(value: &str) -> Result<String> {
    let domain = value.trim().to_ascii_lowercase();
    if !domain.contains('.') {
        return Err(StoreError::Validation(
            "domain must include a public suffix, such as example.com".to_owned(),
        ));
    }
    if domain.len() > SITE_DOMAIN_MAX_LEN {
        return Err(StoreError::Validation(format!(
            "domain must be at most {SITE_DOMAIN_MAX_LEN} characters"
        )));
    }
    if domain.parse::<std::net::IpAddr>().is_ok() {
        return Err(StoreError::Validation(
            "domain must be a DNS name, not an IP address".to_owned(),
        ));
    }
    if !domain.bytes().all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'.')
    }) {
        return Err(StoreError::Validation(
            "domain may only contain ASCII letters, digits, dots, and hyphens".to_owned(),
        ));
    }
    if domain.split('.').any(|label| {
        label.is_empty() || label.len() > 63 || label.starts_with('-') || label.ends_with('-')
    }) {
        return Err(StoreError::Validation(
            "each domain label must be 1-63 characters and may not start or end with a hyphen"
                .to_owned(),
        ));
    }
    Ok(domain)
}

fn map_domain_unique(error: sqlx::Error) -> StoreError {
    if let sqlx::Error::Database(ref database) = error
        && database.constraint() == Some("site_domains_domain_unique")
    {
        return StoreError::Conflict("domain is already connected".to_owned());
    }
    error.into()
}

impl AccountStore {
    /// Claims a custom host for one of this account's tenant sites.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] for a missing or foreign site;
    /// [`StoreError::Validation`] for an invalid host;
    /// [`StoreError::Conflict`] when any site already owns the host.
    pub async fn create_site_domain(&self, site: &SiteId, value: &str) -> Result<SiteDomain> {
        let domain = normalize_site_domain(value)?;
        let verify_token = id::generate_token();
        let row = sqlx::query_as::<_, SiteDomainRow>(
            "INSERT INTO site_domains (tenant_id, site_id, domain, verify_token) \
             SELECT $1, $2, $3, $4 FROM sites \
             WHERE tenant_id = $1 AND id = $2 \
             RETURNING site_id, domain, verify_token, status, verified_at, created_at, updated_at",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(&domain)
        .bind(&verify_token)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_domain_unique)?
        .ok_or(StoreError::NotFound)?;
        row.into_domain()
    }

    /// Lists custom hosts for an owned site in creation order.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] for a missing or foreign site.
    pub async fn site_domains(&self, site: &SiteId) -> Result<Vec<SiteDomain>> {
        let owns_site: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM sites WHERE tenant_id = $1 AND id = $2)",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if !owns_site {
            return Err(StoreError::NotFound);
        }
        let rows = sqlx::query_as::<_, SiteDomainRow>(
            "SELECT site_id, domain, verify_token, status, verified_at, created_at, updated_at \
             FROM site_domains WHERE tenant_id = $1 AND site_id = $2 \
             ORDER BY created_at, domain",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter().map(SiteDomainRow::into_domain).collect()
    }

    /// Marks an owned claim verified after its TXT token has been observed.
    /// Existing live claims stay live.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] for a missing/foreign site or claim.
    pub async fn verify_site_domain(&self, site: &SiteId, value: &str) -> Result<SiteDomain> {
        let domain = normalize_site_domain(value)?;
        let row = sqlx::query_as::<_, SiteDomainRow>(
            "UPDATE site_domains SET \
                 status = CASE WHEN status = 'pending' THEN 'verified' ELSE status END, \
                 verified_at = COALESCE(verified_at, now()), updated_at = now() \
             WHERE tenant_id = $1 AND site_id = $2 AND domain = $3 \
             RETURNING site_id, domain, verify_token, status, verified_at, created_at, updated_at",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(&domain)
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?
        .ok_or(StoreError::NotFound)?;
        row.into_domain()
    }

    /// Marks a verified claim ready for public serving (used by S1.25b).
    ///
    /// # Errors
    /// [`StoreError::NotFound`] for a missing/foreign claim;
    /// [`StoreError::Conflict`] while ownership is still pending.
    pub async fn activate_site_domain(&self, site: &SiteId, value: &str) -> Result<SiteDomain> {
        let domain = normalize_site_domain(value)?;
        let row = sqlx::query_as::<_, SiteDomainRow>(
            "UPDATE site_domains SET status = 'live', updated_at = now() \
             WHERE tenant_id = $1 AND site_id = $2 AND domain = $3 \
               AND status IN ('verified', 'live') \
             RETURNING site_id, domain, verify_token, status, verified_at, created_at, updated_at",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(&domain)
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if let Some(row) = row {
            return row.into_domain();
        }
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM site_domains \
             WHERE tenant_id = $1 AND site_id = $2 AND domain = $3)",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(&domain)
        .fetch_one(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if exists {
            Err(StoreError::Conflict(
                "domain must be verified before it can go live".to_owned(),
            ))
        } else {
            Err(StoreError::NotFound)
        }
    }

    /// Releases an owned site's custom host.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] for a missing or foreign claim.
    pub async fn delete_site_domain(&self, site: &SiteId, value: &str) -> Result<()> {
        let domain = normalize_site_domain(value)?;
        let done = sqlx::query(
            "DELETE FROM site_domains \
             WHERE tenant_id = $1 AND site_id = $2 AND domain = $3",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(&domain)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct SiteDomainRow {
    site_id: String,
    domain: String,
    verify_token: String,
    status: String,
    verified_at: Option<OffsetDateTime>,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl SiteDomainRow {
    fn into_domain(self) -> Result<SiteDomain> {
        Ok(SiteDomain {
            site_id: SiteId::new(self.site_id),
            domain: self.domain,
            verify_token: self.verify_token,
            status: SiteDomainStatus::parse(&self.status).ok_or(StoreError::NotFound)?,
            verified_at: self.verified_at,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domains_are_canonical_and_dns_safe() {
        let Ok(canonical) = normalize_site_domain("  WWW.Example.COM ") else {
            panic!("a valid mixed-case domain was refused");
        };
        assert_eq!(canonical, "www.example.com");
        assert!(normalize_site_domain("xn--bcher-kva.example").is_ok());
        for bad in [
            "example",
            "https://example.com",
            "example.com/path",
            "example.com:443",
            "*.example.com",
            "under_score.example",
            "-bad.example",
            "bad-.example",
            "bad..example",
            "example.com.",
            "bücher.example",
            "127.0.0.1",
        ] {
            assert!(normalize_site_domain(bad).is_err(), "accepted {bad:?}");
        }
    }

    #[test]
    fn status_tokens_are_stable() {
        for status in [
            SiteDomainStatus::Pending,
            SiteDomainStatus::Verified,
            SiteDomainStatus::Live,
        ] {
            assert_eq!(SiteDomainStatus::parse(status.as_str()), Some(status));
        }
    }
}
