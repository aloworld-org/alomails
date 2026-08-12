//! The public serving door of the anonymous `alo-sites`
//! service (ADR 0036, `docs/design/sites.md`). The service holds no session;
//! its tenant scope is derived from the Host lookup: [`SitePublicStore`]
//! resolves a subdomain to a [`PublishedSite`] in one indexed read, and every
//! further read takes that resolved value, whose tenant/publish pairing is
//! private — so serving another tenant's rows is unrepresentable, the same
//! by-construction guarantee the account door gives authenticated code.
//!
//! Its reads expose **public state only**: immutable page snapshots
//! (`site_publishes`, `site_page_snapshots`) plus explicitly published blog
//! posts. Draft pages, draft posts, forms, and everything else in a tenant's
//! scope are simply not reachable through it. It is the module's one
//! deliberate global surface: what it exposes is public by definition —
//! exactly what `<subdomain>.<SITES_DOMAIN>` serves to the internet.

use bytes::Bytes;
use serde_json::Value;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use time::OffsetDateTime;

use crate::blob::BlobStore;
use crate::error::{Result, StoreError};
use crate::id::{BlobId, SiteId, SitePublishId, TenantId};
use crate::site_assets::{SiteImageData, site_image_content_type};
use crate::site_catalog_publish::{SiteCatalogSnapshot, SiteCatalogSnapshotRow};
use crate::site_collections::SiteCollectionSnapshot;
use crate::site_publish::{SiteCollectionSnapshotRow, SitePageSnapshot, SitePageSnapshotRow};

/// A site resolved for public serving: the current publish of a live site.
/// Only the [`SitePublicStore`] Host resolvers construct this — the private
/// tenant field is what keeps every follow-up read scoped to the site the
/// Host header actually named.
#[derive(Debug, Clone)]
pub struct PublishedSite {
    pub(crate) tenant: TenantId,
    /// The site (stable across publishes; useful for logging/metrics keys).
    pub site: SiteId,
    /// The site's display name (nav brand fallback, title suffix).
    pub name: String,
    /// The publish being served. Changes exactly when the tenant republishes,
    /// which makes it the natural cache-validity key.
    pub publish: SitePublishId,
    /// The theme envelope frozen by that publish.
    pub theme: Value,
    /// The default language frozen by the publish.
    pub default_locale: String,
    /// The language choices frozen by the publish, in editor order.
    pub enabled_locales: Vec<String>,
}

/// Public metadata for one published blog post. The document id and tenant
/// never leave the store door: a public caller gets only what the article
/// page and index are allowed to reveal.
#[derive(Debug, Clone)]
pub struct PublishedSitePost {
    pub slug: String,
    pub title: String,
    pub excerpt: String,
    pub cover_blob_id: Option<BlobId>,
    pub published_at: OffsetDateTime,
}

/// One bounded page of published post metadata. `total` counts only posts
/// that passed the same tenant/site/publication boundary as `posts`.
#[derive(Debug, Clone)]
pub struct PublishedSitePostPage {
    pub posts: Vec<PublishedSitePost>,
    pub total: u64,
}

/// One published post plus the current alo Docs bytes that form its body.
#[derive(Debug, Clone)]
pub struct PublishedSitePostBody {
    pub post: PublishedSitePost,
    pub body: Bytes,
}

/// The narrow store handle of the public `alo-sites` service: a Postgres
/// pool exposing published-snapshot reads, privacy-reduced analytics and
/// form writes, plus the blob backend published images live in. Deliberately not
/// [`crate::Store`] — the public service gets no system operations and no way
/// to open a tenant or account door; the blob backend is reachable only
/// through [`Self::published_image`], which takes a resolved
/// [`PublishedSite`].
#[derive(Clone)]
pub struct SitePublicStore {
    pool: PgPool,
    blobs: BlobStore,
}

impl SitePublicStore {
    /// Connects a small pool to `database_url`, serving image bytes from
    /// `blobs` (the same backend the authenticated services write).
    ///
    /// # Errors
    /// [`StoreError::Db`] if the pool cannot connect.
    pub async fn connect(database_url: &str, blobs: BlobStore) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(8)
            .connect(database_url)
            .await
            .map_err(StoreError::Db)?;
        Ok(Self { pool, blobs })
    }

    /// Wraps an existing pool + blob backend (used by tests that share them).
    #[must_use]
    pub fn new(pool: PgPool, blobs: BlobStore) -> Self {
        Self { pool, blobs }
    }

    /// The underlying pool, for sibling modules implementing the public
    /// form and privacy-analytics writes. Crate-internal:
    /// the pool itself is never part of the public surface.
    pub(crate) fn pool(&self) -> &PgPool {
        &self.pool
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
            "SELECT s.tenant_id, s.id AS site_id, s.name, p.id AS publish_id, p.theme, \
                    p.default_locale, p.enabled_locales \
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

    /// Resolves a live custom domain to the owning site's current publish.
    /// Pending and merely verified claims are deliberately absent: Caddy and
    /// the public server share this exact readiness boundary, so TLS is never
    /// authorized before the domain can serve useful bytes.
    ///
    /// The global domain claim is the public routing key. The joined tenant,
    /// site and publish ids come only from that claim, preserving the same
    /// by-construction tenant boundary as [`Self::resolve_published`].
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn resolve_custom_published(&self, domain: &str) -> Result<Option<PublishedSite>> {
        let row = sqlx::query_as::<_, PublishedSiteRow>(
            "SELECT s.tenant_id, s.id AS site_id, s.name, p.id AS publish_id, p.theme, \
                    p.default_locale, p.enabled_locales \
             FROM site_domains d \
             JOIN sites s ON s.tenant_id = d.tenant_id AND s.id = d.site_id \
             JOIN site_publishes p \
               ON p.tenant_id = s.tenant_id AND p.id = s.published_publish_id \
             WHERE d.domain = $1 AND d.status = 'live'",
        )
        .bind(domain)
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
            "SELECT page_id, locale, slug, title, sections, \
                    seo_title, seo_description, nav_order, is_home \
             FROM site_page_snapshots \
             WHERE tenant_id = $1 AND publish_id = $2 \
             ORDER BY nav_order, page_id, locale",
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

    /// The immutable Base-backed collections frozen beside the resolved
    /// pages. The caller cannot supply tenant or publish identifiers, so a
    /// live host can reach only its own current collection rows.
    pub async fn published_collections(
        &self,
        site: &PublishedSite,
    ) -> Result<Vec<SiteCollectionSnapshot>> {
        let rows = sqlx::query_as::<_, SiteCollectionSnapshotRow>(
            "SELECT collection_id, name, items FROM site_collection_snapshots \
             WHERE tenant_id = $1 AND publish_id = $2 ORDER BY collection_id",
        )
        .bind(site.tenant.as_str())
        .bind(site.publish.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter()
            .map(SiteCollectionSnapshotRow::into_snapshot)
            .collect()
    }

    /// The immutable catalogs frozen beside the resolved pages — what the
    /// site offered at publish time, prices included. The caller cannot supply
    /// tenant or publish identifiers, so a live host can reach only its own
    /// current catalog rows, and a hidden item was never written here at all.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure; [`StoreError::Conflict`] when a stored
    /// snapshot cannot be read back.
    pub async fn published_catalogs(
        &self,
        site: &PublishedSite,
    ) -> Result<Vec<SiteCatalogSnapshot>> {
        let rows = sqlx::query_as::<_, SiteCatalogSnapshotRow>(
            "SELECT catalog_id, name, currency, orders_enabled, categories, items \
             FROM site_catalog_snapshots \
             WHERE tenant_id = $1 AND publish_id = $2 ORDER BY catalog_id",
        )
        .bind(site.tenant.as_str())
        .bind(site.publish.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter()
            .map(SiteCatalogSnapshotRow::into_snapshot)
            .collect()
    }

    /// A bounded page of published blog metadata for a resolved live site,
    /// newest first. Draft posts are absent at both query boundaries and the
    /// private tenant/site pair comes only from the Host resolution.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn published_posts_page(
        &self,
        site: &PublishedSite,
        offset: u32,
        limit: u32,
    ) -> Result<PublishedSitePostPage> {
        let total = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM site_posts \
             WHERE tenant_id = $1 AND site_id = $2 AND status = 'published'",
        )
        .bind(site.tenant.as_str())
        .bind(site.site.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        let rows = sqlx::query_as::<_, PublishedSitePostRow>(
            "SELECT slug, title, excerpt, cover_blob_id, published_at \
             FROM site_posts \
             WHERE tenant_id = $1 AND site_id = $2 AND status = 'published' \
             ORDER BY published_at DESC, id \
             OFFSET $3 LIMIT $4",
        )
        .bind(site.tenant.as_str())
        .bind(site.site.as_str())
        .bind(i64::from(offset))
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(PublishedSitePostPage {
            posts: rows
                .into_iter()
                .map(PublishedSitePostRow::into_post)
                .collect(),
            total: u64::try_from(total).unwrap_or_default(),
        })
    }

    /// One published blog post and its current alo Docs body. A missing,
    /// draft, trashed, wrong-kind, or foreign document is one clean absence.
    /// Blob bytes are fetched only after the tenant/site-scoped row resolves.
    ///
    /// # Errors
    /// [`StoreError::Db`]/[`StoreError::Blob`] on backend failure.
    pub async fn published_post(
        &self,
        site: &PublishedSite,
        slug: &str,
    ) -> Result<Option<PublishedSitePostBody>> {
        let row = sqlx::query_as::<_, PublishedSitePostBodyRow>(
            "SELECT p.slug, p.title, p.excerpt, p.cover_blob_id, p.published_at, b.hash \
             FROM site_posts p \
             JOIN drive_nodes d \
               ON d.tenant_id = p.tenant_id AND d.id = p.doc_node_id \
             JOIN blobs b \
               ON b.tenant_id = d.tenant_id AND b.id = d.blob_id \
             WHERE p.tenant_id = $1 AND p.site_id = $2 AND p.slug = $3 \
               AND p.status = 'published' AND d.kind = 'doc' AND NOT d.trashed",
        )
        .bind(site.tenant.as_str())
        .bind(site.site.as_str())
        .bind(slug)
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        let Some(row) = row else { return Ok(None) };
        let body = self.blobs.get(site.tenant.as_str(), &row.hash).await?;
        Ok(Some(PublishedSitePostBody {
            post: row.into_post(),
            body,
        }))
    }

    /// Whether `blob_id` is the cover of a published post on this resolved
    /// site. This is the reference gate paired with [`Self::published_image`]
    /// for `/assets/img/<blob_id>`; a known foreign or draft cover stays
    /// indistinguishable from an absent id.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn published_post_uses_cover(
        &self,
        site: &PublishedSite,
        blob_id: &str,
    ) -> Result<bool> {
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM site_posts \
             WHERE tenant_id = $1 AND site_id = $2 AND status = 'published' \
               AND cover_blob_id = $3)",
        )
        .bind(site.tenant.as_str())
        .bind(site.site.as_str())
        .bind(blob_id)
        .fetch_one(&self.pool)
        .await
        .map_err(StoreError::Db)
    }

    /// An image blob of a resolved site's tenant, for the public
    /// `/assets/img/<blob_id>` path: `None` when the id does not resolve in
    /// that tenant or the stored content type is not a servable image type.
    /// Tenant scope comes from the resolved value's private tenant — a Host
    /// can never lead to another tenant's bytes. The **caller** additionally
    /// gates ids to the ones the served publish actually references (the
    /// render layer knows that set); this read enforces the tenant boundary,
    /// not the reference set.
    ///
    /// # Errors
    /// [`StoreError::Db`]/[`StoreError::Blob`] on backend failure.
    pub async fn published_image(
        &self,
        site: &PublishedSite,
        blob_id: &str,
    ) -> Result<Option<SiteImageData>> {
        let row: Option<(String, Option<String>)> =
            sqlx::query_as("SELECT hash, content_type FROM blobs WHERE tenant_id = $1 AND id = $2")
                .bind(site.tenant.as_str())
                .bind(blob_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(StoreError::Db)?;
        let Some((hash, stored_type)) = row else {
            return Ok(None);
        };
        let Some(content_type) = site_image_content_type(stored_type.as_deref()) else {
            return Ok(None);
        };
        let bytes = self.blobs.get(site.tenant.as_str(), &hash).await?;
        Ok(Some(SiteImageData {
            content_type,
            bytes,
        }))
    }
}

#[derive(sqlx::FromRow)]
struct PublishedSiteRow {
    tenant_id: String,
    site_id: String,
    name: String,
    publish_id: String,
    theme: sqlx::types::Json<Value>,
    default_locale: String,
    enabled_locales: Vec<String>,
}
impl PublishedSiteRow {
    fn into_site(self) -> PublishedSite {
        PublishedSite {
            tenant: TenantId::new(self.tenant_id),
            site: SiteId::new(self.site_id),
            name: self.name,
            publish: SitePublishId::new(self.publish_id),
            theme: self.theme.0,
            default_locale: self.default_locale,
            enabled_locales: self.enabled_locales,
        }
    }
}

#[derive(sqlx::FromRow)]
struct PublishedSitePostRow {
    slug: String,
    title: String,
    excerpt: String,
    cover_blob_id: Option<String>,
    published_at: OffsetDateTime,
}

impl PublishedSitePostRow {
    fn into_post(self) -> PublishedSitePost {
        PublishedSitePost {
            slug: self.slug,
            title: self.title,
            excerpt: self.excerpt,
            cover_blob_id: self.cover_blob_id.map(BlobId::new),
            published_at: self.published_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct PublishedSitePostBodyRow {
    slug: String,
    title: String,
    excerpt: String,
    cover_blob_id: Option<String>,
    published_at: OffsetDateTime,
    hash: String,
}

impl PublishedSitePostBodyRow {
    fn into_post(self) -> PublishedSitePost {
        PublishedSitePostRow {
            slug: self.slug,
            title: self.title,
            excerpt: self.excerpt,
            cover_blob_id: self.cover_blob_id,
            published_at: self.published_at,
        }
        .into_post()
    }
}
