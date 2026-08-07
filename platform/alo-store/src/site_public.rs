//! The public serving door — the read surface of the anonymous `alo-sites`
//! service (ADR 0036, `docs/design/sites.md`). The service holds no session;
//! its tenant scope is derived from the Host lookup: [`SitePublicStore`]
//! resolves a subdomain to a [`PublishedSite`] in one indexed read, and every
//! further read takes that resolved value, whose tenant/publish pairing is
//! private — so serving another tenant's rows is unrepresentable, the same
//! by-construction guarantee the account door gives authenticated code.
//!
//! This door reads **published snapshots only** (`site_publishes`,
//! `site_page_snapshots`). Drafts, forms, and everything else in a tenant's
//! scope are simply not reachable through it. It is the module's one
//! deliberate global surface: what it exposes is public by definition —
//! exactly what `<subdomain>.<SITES_DOMAIN>` serves to the internet.

use serde_json::Value;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

use crate::error::{Result, StoreError};
use crate::id::{SiteId, SitePublishId, TenantId};
use crate::site_publish::{SitePageSnapshot, SitePageSnapshotRow};

/// A site resolved for public serving: the current publish of a live site.
/// Only [`SitePublicStore::resolve_published`] constructs this — the private
/// tenant field is what keeps every follow-up read scoped to the site the
/// Host header actually named.
#[derive(Debug, Clone)]
pub struct PublishedSite {
    tenant: TenantId,
    /// The site (stable across publishes; useful for logging/metrics keys).
    pub site: SiteId,
    /// The site's display name (nav brand fallback, title suffix).
    pub name: String,
    /// The publish being served. Changes exactly when the tenant republishes,
    /// which makes it the natural cache-validity key.
    pub publish: SitePublishId,
    /// The theme envelope frozen by that publish.
    pub theme: Value,
}

/// The read-only store handle of the public `alo-sites` service: a Postgres
/// pool exposing published-snapshot reads and nothing else. Deliberately not
/// [`crate::Store`] — the public service gets no system operations, no blob
/// backend, and no way to open a tenant or account door.
#[derive(Clone)]
pub struct SitePublicStore {
    pool: PgPool,
}

impl SitePublicStore {
    /// Connects a small pool to `database_url`.
    ///
    /// # Errors
    /// [`StoreError::Db`] if the pool cannot connect.
    pub async fn connect(database_url: &str) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(8)
            .connect(database_url)
            .await
            .map_err(StoreError::Db)?;
        Ok(Self { pool })
    }

    /// Wraps an existing pool (used by tests that share one).
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Resolves a subdomain to the site's current publish — the one indexed
    /// read that maps a Host header to a tenant scope. `None` for an unknown
    /// subdomain and for a site that is not live: the two are
    /// indistinguishable by design (no tenant-existence leak).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn resolve_published(&self, subdomain: &str) -> Result<Option<PublishedSite>> {
        let row = sqlx::query_as::<_, PublishedSiteRow>(
            "SELECT s.tenant_id, s.id AS site_id, s.name, p.id AS publish_id, p.theme \
             FROM sites s \
             JOIN site_publishes p \
               ON p.tenant_id = s.tenant_id AND p.id = s.published_publish_id \
             WHERE s.subdomain = $1",
        )
        .bind(subdomain)
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(row.map(PublishedSiteRow::into_site))
    }

    /// The frozen pages of a resolved site's publish, in navigation order.
    /// Scoped by the resolved value's private tenant/publish pair — there is
    /// no way to ask this door for pages the Host lookup didn't lead to.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn published_pages(&self, site: &PublishedSite) -> Result<Vec<SitePageSnapshot>> {
        let rows = sqlx::query_as::<_, SitePageSnapshotRow>(
            "SELECT page_id, slug, title, sections, \
                    seo_title, seo_description, nav_order, is_home \
             FROM site_page_snapshots \
             WHERE tenant_id = $1 AND publish_id = $2 \
             ORDER BY nav_order, page_id",
        )
        .bind(site.tenant.as_str())
        .bind(site.publish.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows
            .into_iter()
            .map(SitePageSnapshotRow::into_snapshot)
            .collect())
    }
}

#[derive(sqlx::FromRow)]
struct PublishedSiteRow {
    tenant_id: String,
    site_id: String,
    name: String,
    publish_id: String,
    theme: sqlx::types::Json<Value>,
}
impl PublishedSiteRow {
    fn into_site(self) -> PublishedSite {
        PublishedSite {
            tenant: TenantId::new(self.tenant_id),
            site: SiteId::new(self.site_id),
            name: self.name,
            publish: SitePublishId::new(self.publish_id),
            theme: self.theme.0,
        }
    }
}
