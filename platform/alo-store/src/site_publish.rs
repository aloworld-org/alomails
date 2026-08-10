//! Site publishing — the flow that makes an alo Sites website live (ADR
//! 0036), reached through the account door like [`crate::sites`]. A publish
//! freezes the draft into immutable rows: one `site_publishes` record holding
//! the theme, plus one `site_page_snapshots` row per page. The site's
//! published-set pointer is flipped to the new publish in the same
//! transaction, and the public service reads only snapshots — so drafts are
//! unreachable from the internet by construction, not by filtering, and
//! editing (or deleting) a draft page never changes what is being served
//! until the next publish (`docs/design/sites.md`).

use serde_json::Value;
use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::{SiteId, SitePageId, SitePublishId};
use crate::sites::SiteStatus;

/// One publish of a site: the immutable record the current (or a past)
/// published set hangs off. `theme` is the theme envelope frozen at publish
/// time — later theme edits only reach the public site via a republish.
#[derive(Debug, Clone)]
pub struct SitePublish {
    pub id: SitePublishId,
    /// The site's theme envelope as it was when this publish was made.
    pub theme: Value,
    /// The site's default language frozen with this publish.
    pub default_locale: String,
    /// The language choices frozen with this publish, in editor order.
    pub enabled_locales: Vec<String>,
    pub published_by: String,
    pub published_at: OffsetDateTime,
}

/// One page frozen by a publish. Field for field the page as it was —
/// `page_id` names the draft page it came from, which may since have been
/// edited or deleted without affecting this row.
#[derive(Debug, Clone)]
pub struct SitePageSnapshot {
    /// The draft page this snapshot froze (no longer guaranteed to exist).
    pub page_id: SitePageId,
    /// The exact language this immutable page snapshot contains.
    pub locale: String,
    /// URL path segment; empty exactly when this is the home page.
    pub slug: String,
    pub title: String,
    /// The sections envelope as frozen (see [`crate::site_model`]).
    pub sections: Value,
    pub seo_title: Option<String>,
    pub seo_description: Option<String>,
    pub nav_order: i32,
    pub is_home: bool,
}

impl AccountStore {
    /// Publishes `site`: freezes every page and the theme into a new
    /// immutable snapshot set, flips the site's published-set pointer to it,
    /// and marks the site `live` — all in one transaction, so the public
    /// service switches between complete sets and never sees a half-publish.
    /// Republishing simply creates the next set; earlier sets are retained
    /// (immutable history, the substrate for S2 rollback).
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the site isn't the tenant's;
    /// [`StoreError::Conflict`] when the site has no pages or no home page
    /// (an empty or root-less site must not go live); [`StoreError::Db`].
    pub async fn publish_site(&self, site: &SiteId) -> Result<SitePublishId> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        // Lock the site row so concurrent publishes serialize instead of
        // racing on the pointer flip.
        let settings: Option<(String, Vec<String>)> = sqlx::query_as(
            "SELECT default_locale, enabled_locales FROM sites \
             WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        let (default_locale, enabled_locales) = settings.ok_or(StoreError::NotFound)?;
        let (pages, homes): (i64, i64) = sqlx::query_as(
            "SELECT count(*), count(*) FILTER (WHERE is_home) \
             FROM site_pages WHERE tenant_id = $1 AND site_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .fetch_one(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        if pages == 0 {
            return Err(StoreError::Conflict(
                "site has no pages to publish".to_owned(),
            ));
        }
        if homes == 0 {
            return Err(StoreError::Conflict("site has no home page".to_owned()));
        }
        let id = SitePublishId::generate();
        // Freeze the theme and the pages by copying inside SQL — nothing
        // round-trips through the application, so the snapshot is exactly
        // what the write gates already admitted to the draft tables.
        sqlx::query(
            "INSERT INTO site_publishes \
                (tenant_id, site_id, id, theme, default_locale, enabled_locales, published_by) \
             SELECT tenant_id, id, $3, theme, $4, $5, $6 \
             FROM sites WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(id.as_str())
        .bind(&default_locale)
        .bind(&enabled_locales)
        .bind(self.user.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        sqlx::query(
            "INSERT INTO site_page_snapshots \
                 (tenant_id, publish_id, page_id, locale, slug, title, sections, \
                  seo_title, seo_description, nav_order, is_home) \
             SELECT tenant_id, $3, id, content_locale, slug, title, sections, \
                    seo_title, seo_description, nav_order, is_home \
             FROM site_pages WHERE tenant_id = $1 AND site_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(id.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        sqlx::query(
            "INSERT INTO site_page_snapshots \
                 (tenant_id, publish_id, page_id, locale, slug, title, sections, \
                  seo_title, seo_description, nav_order, is_home) \
             SELECT l.tenant_id, $3, l.page_id, l.locale, l.slug, l.title, l.sections, \
                    l.seo_title, l.seo_description, p.nav_order, p.is_home \
             FROM site_page_locales l \
             JOIN site_pages p \
               ON p.tenant_id = l.tenant_id AND p.site_id = l.site_id AND p.id = l.page_id \
             WHERE l.tenant_id = $1 AND l.site_id = $2 AND l.locale = ANY($4)",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(id.as_str())
        .bind(&enabled_locales)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        sqlx::query(
            "UPDATE sites SET published_publish_id = $3, status = $4, updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(id.as_str())
        .bind(SiteStatus::Live.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(id)
    }

    /// Takes `site` offline: clears the published-set pointer and marks the
    /// site `draft` again. Past publishes and their snapshots are retained —
    /// unpublishing hides the site, it does not erase history. Idempotent on
    /// a site that isn't live.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the site isn't the tenant's;
    /// [`StoreError::Db`].
    pub async fn unpublish_site(&self, site: &SiteId) -> Result<()> {
        let done = sqlx::query(
            "UPDATE sites SET published_publish_id = NULL, status = $3, updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(SiteStatus::Draft.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// The site's current publish — the set the public service is serving —
    /// or `None` when the site is not live. A site of another tenant reads
    /// as `None` too (indistinguishable by design).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn current_site_publish(&self, site: &SiteId) -> Result<Option<SitePublish>> {
        let row = sqlx::query_as::<_, SitePublishRow>(
            "SELECT p.id, p.theme, p.default_locale, p.enabled_locales, \
                    p.published_by, p.published_at \
             FROM sites s \
             JOIN site_publishes p \
               ON p.tenant_id = s.tenant_id AND p.id = s.published_publish_id \
             WHERE s.tenant_id = $1 AND s.id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(row.map(SitePublishRow::into_publish))
    }

    /// The page snapshots of one publish of the tenant's `site`, in
    /// navigation order. Empty when the publish isn't the tenant's or isn't
    /// this site's — indistinguishable from an unknown publish, by design.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn site_publish_snapshots(
        &self,
        site: &SiteId,
        publish: &SitePublishId,
    ) -> Result<Vec<SitePageSnapshot>> {
        let rows = sqlx::query_as::<_, SitePageSnapshotRow>(
            "SELECT sn.page_id, sn.locale, sn.slug, sn.title, sn.sections, \
                    sn.seo_title, sn.seo_description, sn.nav_order, sn.is_home \
             FROM site_page_snapshots sn \
             JOIN site_publishes p ON p.tenant_id = sn.tenant_id AND p.id = sn.publish_id \
             WHERE sn.tenant_id = $1 AND p.site_id = $2 AND sn.publish_id = $3 \
             ORDER BY sn.nav_order, sn.page_id, sn.locale",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(publish.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows
            .into_iter()
            .map(SitePageSnapshotRow::into_snapshot)
            .collect())
    }
}

// ---- row types --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct SitePublishRow {
    id: String,
    theme: sqlx::types::Json<Value>,
    default_locale: String,
    enabled_locales: Vec<String>,
    published_by: String,
    published_at: OffsetDateTime,
}
impl SitePublishRow {
    fn into_publish(self) -> SitePublish {
        SitePublish {
            id: SitePublishId::new(self.id),
            theme: self.theme.0,
            default_locale: self.default_locale,
            enabled_locales: self.enabled_locales,
            published_by: self.published_by,
            published_at: self.published_at,
        }
    }
}

#[derive(sqlx::FromRow)]
pub(crate) struct SitePageSnapshotRow {
    page_id: String,
    locale: String,
    slug: String,
    title: String,
    sections: sqlx::types::Json<Value>,
    seo_title: Option<String>,
    seo_description: Option<String>,
    nav_order: i32,
    is_home: bool,
}
impl SitePageSnapshotRow {
    pub(crate) fn into_snapshot(self) -> SitePageSnapshot {
        SitePageSnapshot {
            page_id: SitePageId::new(self.page_id),
            locale: self.locale,
            slug: self.slug,
            title: self.title,
            sections: self.sections.0,
            seo_title: self.seo_title,
            seo_description: self.seo_description,
            nav_order: self.nav_order,
            is_home: self.is_home,
        }
    }
}
