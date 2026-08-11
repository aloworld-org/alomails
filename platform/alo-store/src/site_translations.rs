//! Atomic approval writes for reviewed whole-site translations.

use serde_json::Value;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::{SiteId, SitePageId, SitePostId};
use crate::site_model::SectionsEnvelope;
use crate::site_pages::{map_page_constraints, validate_page_slug, validate_page_title};
use crate::site_posts::{map_constraints as map_post_constraints, validate_text};
use crate::sites::normalize_locale_tag;

type PageTranslationRow = (String, String, Option<String>, Option<String>, Value);

#[derive(Debug, Clone, PartialEq)]
pub struct SiteTranslationPageContent {
    pub title: String,
    pub slug: String,
    pub seo_title: Option<String>,
    pub seo_description: Option<String>,
    pub sections: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteTranslationPostContent {
    pub title: String,
    pub slug: String,
    pub excerpt: String,
}

pub struct SiteTranslationPageWrite {
    pub id: SitePageId,
    pub before: SiteTranslationPageContent,
    pub after: SiteTranslationPageContent,
}

pub struct SiteTranslationPostWrite {
    pub id: SitePostId,
    pub before: SiteTranslationPostContent,
    pub after: SiteTranslationPostContent,
}

impl AccountStore {
    /// Applies a reviewed translation as one transaction. Every source row is
    /// locked and compared with the proposal's `before` snapshot first; one
    /// changed item aborts the entire write as stale.
    pub async fn apply_site_translation(
        &self,
        site: &SiteId,
        source_locale: &str,
        target_locale: &str,
        pages: &[SiteTranslationPageWrite],
        posts: &[SiteTranslationPostWrite],
    ) -> Result<()> {
        let source_locale = normalize_locale_tag(source_locale)?;
        let target_locale = normalize_locale_tag(target_locale)?;
        if source_locale == target_locale {
            return Err(StoreError::Conflict(
                "source and target languages must differ".into(),
            ));
        }
        for page in pages {
            validate_page_title(&page.after.title)?;
            if !page.after.slug.is_empty() {
                validate_page_slug(&page.after.slug)?;
            }
            SectionsEnvelope::from_value(page.after.sections.clone())
                .map_err(|error| StoreError::Conflict(error.to_string()))?;
        }
        for post in posts {
            validate_page_slug(&post.after.slug)?;
            validate_text(&post.after.title, &post.after.excerpt)?;
        }

        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let enabled: Option<Vec<String>> = sqlx::query_scalar(
            "SELECT enabled_locales FROM sites WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        let enabled = enabled.ok_or(StoreError::NotFound)?;
        if !enabled.contains(&source_locale) || !enabled.contains(&target_locale) {
            return Err(StoreError::Conflict(
                "both translation languages must be enabled".into(),
            ));
        }

        for page in pages {
            let is_home: Option<bool> = sqlx::query_scalar(
                "SELECT is_home FROM site_pages WHERE tenant_id=$1 AND site_id=$2 AND id=$3 FOR UPDATE",
            )
            .bind(self.tenant.as_str())
            .bind(site.as_str())
            .bind(page.id.as_str())
            .fetch_optional(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
            let is_home = is_home.ok_or(StoreError::NotFound)?;
            if !is_home && page.after.slug.is_empty() {
                return Err(StoreError::Conflict(
                    "a translated page path cannot be empty".into(),
                ));
            }
            let current: Option<PageTranslationRow> = sqlx::query_as(
                "SELECT title, slug, seo_title, seo_description, sections FROM (\
                   SELECT title, slug, seo_title, seo_description, sections, content_locale AS locale \
                   FROM site_pages WHERE tenant_id=$1 AND site_id=$2 AND id=$3 \
                   UNION ALL \
                   SELECT title, slug, seo_title, seo_description, sections, locale \
                   FROM site_page_locales WHERE tenant_id=$1 AND site_id=$2 AND page_id=$3\
                 ) exact WHERE locale=$4 LIMIT 1",
            ).bind(self.tenant.as_str()).bind(site.as_str()).bind(page.id.as_str()).bind(&source_locale)
                .fetch_optional(&mut *tx).await.map_err(StoreError::Db)?;
            let current = current.ok_or_else(|| {
                StoreError::Conflict("a source page translation is missing".into())
            })?;
            let before = &page.before;
            if current
                != (
                    before.title.clone(),
                    before.slug.clone(),
                    before.seo_title.clone(),
                    before.seo_description.clone(),
                    before.sections.clone(),
                )
            {
                return Err(StoreError::Conflict(
                    "the website changed; prepare a fresh translation".into(),
                ));
            }
        }
        for post in posts {
            let current: Option<(String, String, String)> = sqlx::query_as(
                "SELECT title, slug, excerpt FROM (\
                   SELECT title, slug, excerpt, content_locale AS locale FROM site_posts \
                   WHERE tenant_id=$1 AND site_id=$2 AND id=$3 \
                   UNION ALL \
                   SELECT title, slug, excerpt, locale FROM site_post_locales \
                   WHERE tenant_id=$1 AND site_id=$2 AND post_id=$3\
                 ) exact WHERE locale=$4 LIMIT 1",
            )
            .bind(self.tenant.as_str())
            .bind(site.as_str())
            .bind(post.id.as_str())
            .bind(&source_locale)
            .fetch_optional(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
            let current = current.ok_or_else(|| {
                StoreError::Conflict("a source post translation is missing".into())
            })?;
            if current
                != (
                    post.before.title.clone(),
                    post.before.slug.clone(),
                    post.before.excerpt.clone(),
                )
            {
                return Err(StoreError::Conflict(
                    "the website changed; prepare a fresh translation".into(),
                ));
            }
        }

        for page in pages {
            sqlx::query(
                "INSERT INTO site_page_locales (tenant_id,site_id,page_id,locale,slug,title,sections,seo_title,seo_description) \
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT (tenant_id,page_id,locale) DO UPDATE SET \
                 slug=EXCLUDED.slug,title=EXCLUDED.title,sections=EXCLUDED.sections,seo_title=EXCLUDED.seo_title,seo_description=EXCLUDED.seo_description,updated_at=now()",
            ).bind(self.tenant.as_str()).bind(site.as_str()).bind(page.id.as_str()).bind(&target_locale)
                .bind(&page.after.slug).bind(page.after.title.trim()).bind(sqlx::types::Json(&page.after.sections))
                .bind(&page.after.seo_title).bind(&page.after.seo_description)
                .execute(&mut *tx).await.map_err(map_page_constraints)?;
        }
        for post in posts {
            sqlx::query(
                "INSERT INTO site_post_locales (tenant_id,site_id,post_id,locale,slug,title,excerpt) \
                 VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT (tenant_id,post_id,locale) DO UPDATE SET \
                 slug=EXCLUDED.slug,title=EXCLUDED.title,excerpt=EXCLUDED.excerpt,updated_at=now()",
            ).bind(self.tenant.as_str()).bind(site.as_str()).bind(post.id.as_str()).bind(&target_locale)
                .bind(&post.after.slug).bind(post.after.title.trim()).bind(post.after.excerpt.trim())
                .execute(&mut *tx).await.map_err(map_post_constraints)?;
        }
        tx.commit().await.map_err(StoreError::Db)
    }
}
