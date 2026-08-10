//! Site pages — the pages of an alo Sites website (ADR 0036), reached through
//! the account door like [`crate::sites`]. A page is addressed as
//! (site, page): every statement scopes by tenant AND site, so a page id can
//! never be reached through another tenant — or another site. The `sections`
//! JSON is validated against the typed schema in [`crate::site_model`] on
//! every write; reads return it as stored (read-side tolerance for newer
//! snapshots is the renderer's job, `docs/design/sites.md`).

use serde_json::Value;
use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::{SiteId, SitePageId};
use crate::site_model::SectionsEnvelope;

/// Maximum length of a page slug. Slugs are single URL path segments; 80 is
/// generous for readable URLs and far under any protocol limit.
pub const PAGE_SLUG_MAX_LEN: usize = 80;
/// Maximum pages one site may hold.
pub const MAX_PAGES_PER_SITE: i64 = 200;
/// A page title is a human label — generous but bounded.
const PAGE_TITLE_MAX_CHARS: usize = 200;
/// SEO title override cap (search engines truncate far earlier).
const SEO_TITLE_MAX_CHARS: usize = 200;
/// SEO description override cap.
const SEO_DESCRIPTION_MAX_CHARS: usize = 500;

/// Slugs a page can never claim: path prefixes the public `alo-sites` service
/// owns (`/blog`, `/f/:form_id`, feeds, well-known files) plus static-asset
/// conventions. Checked after the syntax rules, so entries are all lowercase
/// and slug-safe.
const RESERVED_SLUGS: &[&str] = &[
    "blog", "f", "feed", "rss", "atom", "sitemap", "robots", "healthz", "assets", "static",
];

/// One page of a site. `sections` is returned as stored — always a value that
/// passed the typed-schema gate at write time.
#[derive(Debug, Clone)]
pub struct SitePage {
    pub id: SitePageId,
    /// URL path segment; empty exactly when this is the home page.
    pub slug: String,
    pub title: String,
    /// The sections envelope as stored (see [`crate::site_model`]).
    pub sections: Value,
    /// SEO title override; `None` derives from `title`.
    pub seo_title: Option<String>,
    /// SEO description override; `None` means no meta description.
    pub seo_description: Option<String>,
    /// Position in the site's navigation, ascending.
    pub nav_order: i32,
    pub is_home: bool,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

/// Validates a non-empty page slug: `[a-z0-9-]`, 1–80 chars, no leading or
/// trailing hyphen, and not a reserved public path. The empty slug (the home
/// page's spelling) is not accepted here — the store's write paths allow it
/// only for the home page.
///
/// # Errors
/// [`StoreError::Conflict`] naming the violated rule (safe to surface as a
/// field-level validation detail).
pub fn validate_page_slug(slug: &str) -> Result<()> {
    if slug.is_empty() || slug.len() > PAGE_SLUG_MAX_LEN {
        return Err(StoreError::Conflict(format!(
            "slug must be 1-{PAGE_SLUG_MAX_LEN} characters"
        )));
    }
    if !slug
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err(StoreError::Conflict(
            "slug may only contain lowercase letters, digits, and hyphens".to_owned(),
        ));
    }
    if slug.starts_with('-') || slug.ends_with('-') {
        return Err(StoreError::Conflict(
            "slug may not start or end with a hyphen".to_owned(),
        ));
    }
    if RESERVED_SLUGS.contains(&slug) {
        return Err(StoreError::Conflict("slug is reserved".to_owned()));
    }
    Ok(())
}

/// Validates a page's display title: non-blank after trimming, bounded.
pub(crate) fn validate_page_title(title: &str) -> Result<()> {
    if title.trim().is_empty() {
        return Err(StoreError::Conflict(
            "page title must not be empty".to_owned(),
        ));
    }
    if title.chars().count() > PAGE_TITLE_MAX_CHARS {
        return Err(StoreError::Conflict(format!(
            "page title must be at most {PAGE_TITLE_MAX_CHARS} characters"
        )));
    }
    Ok(())
}

/// Normalizes an optional SEO override: trims, treats blank as absent, and
/// bounds the length.
pub(crate) fn normalize_seo(
    value: Option<&str>,
    cap: usize,
    field: &str,
) -> Result<Option<String>> {
    match value.map(str::trim) {
        None | Some("") => Ok(None),
        Some(text) => {
            if text.chars().count() > cap {
                return Err(StoreError::Conflict(format!(
                    "{field} must be at most {cap} characters"
                )));
            }
            Ok(Some(text.to_owned()))
        }
    }
}

/// Translates the table's named constraints into caller-facing conflicts.
/// Anything else passes through the standard mapping.
pub(crate) fn map_page_constraints(error: sqlx::Error) -> StoreError {
    if let sqlx::Error::Database(ref db) = error {
        match db.constraint() {
            Some("site_pages_slug_unique") => {
                return StoreError::Conflict("slug is already used on this site".to_owned());
            }
            Some("site_pages_one_home") => {
                return StoreError::Conflict("site already has a home page".to_owned());
            }
            Some("site_pages_slug_shape") => {
                return StoreError::Conflict(
                    "only the home page may have an empty slug".to_owned(),
                );
            }
            _ => {}
        }
    }
    error.into()
}

impl AccountStore {
    /// Creates a page on `site` at the end of the nav order, with an empty
    /// sections envelope. `home` marks it as the site's home page — only then
    /// may `slug` be empty (the site root).
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the site isn't the tenant's;
    /// [`StoreError::Conflict`] on an invalid title or slug, a slug already
    /// used on the site, a second home page, or a full site
    /// ([`MAX_PAGES_PER_SITE`]); [`StoreError::Db`] on failure.
    pub async fn create_site_page(
        &self,
        site: &SiteId,
        title: &str,
        slug: &str,
        home: bool,
    ) -> Result<SitePageId> {
        validate_page_title(title)?;
        if !(home && slug.is_empty()) {
            validate_page_slug(slug)?;
        }
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let pages: Option<i64> = sqlx::query_scalar(
            "SELECT (SELECT count(*) FROM site_pages p \
                     WHERE p.tenant_id = s.tenant_id AND p.site_id = s.id) \
             FROM sites s WHERE s.tenant_id = $1 AND s.id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        let pages = pages.ok_or(StoreError::NotFound)?;
        if pages >= MAX_PAGES_PER_SITE {
            return Err(StoreError::Conflict(format!(
                "a site may have at most {MAX_PAGES_PER_SITE} pages"
            )));
        }
        let id = SitePageId::generate();
        sqlx::query(
            "INSERT INTO site_pages (tenant_id, site_id, id, slug, title, nav_order, is_home) \
             SELECT $1, $2, $3, $4, $5, \
                    COALESCE((SELECT max(nav_order) + 1 FROM site_pages \
                              WHERE tenant_id = $1 AND site_id = $2), 0), \
                    $6",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(id.as_str())
        .bind(slug)
        .bind(title.trim())
        .bind(home)
        .execute(&mut *tx)
        .await
        .map_err(map_page_constraints)?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(id)
    }

    /// The site's pages in navigation order. Empty when the site isn't the
    /// tenant's — indistinguishable from a site with no pages, by design.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn site_pages(&self, site: &SiteId) -> Result<Vec<SitePage>> {
        let rows = sqlx::query_as::<_, SitePageRow>(
            "SELECT id, slug, title, sections, seo_title, seo_description, nav_order, is_home, \
                    created_at, updated_at \
             FROM site_pages WHERE tenant_id = $1 AND site_id = $2 ORDER BY nav_order, id",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(SitePageRow::into_page).collect())
    }

    /// A single page of the tenant's site, or `None` — including when the
    /// site or page belongs to another tenant (indistinguishable by design).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn site_page(&self, site: &SiteId, page: &SitePageId) -> Result<Option<SitePage>> {
        let row = sqlx::query_as::<_, SitePageRow>(
            "SELECT id, slug, title, sections, seo_title, seo_description, nav_order, is_home, \
                    created_at, updated_at \
             FROM site_pages WHERE tenant_id = $1 AND site_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(page.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(row.map(SitePageRow::into_page))
    }

    /// Retitles a page.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the page isn't the tenant's;
    /// [`StoreError::Conflict`] on an invalid title; [`StoreError::Db`].
    pub async fn set_page_title(
        &self,
        site: &SiteId,
        page: &SitePageId,
        title: &str,
    ) -> Result<()> {
        validate_page_title(title)?;
        let done = sqlx::query(
            "UPDATE site_pages SET title = $4, updated_at = now() \
             WHERE tenant_id = $1 AND site_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(page.as_str())
        .bind(title.trim())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Moves a page to a new slug. The empty slug is accepted only for the
    /// home page (enforced by the table's CHECK, so the rule holds even under
    /// concurrent home changes).
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the page isn't the tenant's;
    /// [`StoreError::Conflict`] on an invalid, reserved, or taken slug, or an
    /// empty slug on a non-home page; [`StoreError::Db`].
    pub async fn set_page_slug(&self, site: &SiteId, page: &SitePageId, slug: &str) -> Result<()> {
        if !slug.is_empty() {
            validate_page_slug(slug)?;
        }
        let done = sqlx::query(
            "UPDATE site_pages SET slug = $4, updated_at = now() \
             WHERE tenant_id = $1 AND site_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(page.as_str())
        .bind(slug)
        .execute(&self.pool)
        .await
        .map_err(map_page_constraints)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Sets or clears a page's SEO overrides. Blank strings clear like `None`.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the page isn't the tenant's;
    /// [`StoreError::Conflict`] on an over-long value; [`StoreError::Db`].
    pub async fn set_page_seo(
        &self,
        site: &SiteId,
        page: &SitePageId,
        seo_title: Option<&str>,
        seo_description: Option<&str>,
    ) -> Result<()> {
        let seo_title = normalize_seo(seo_title, SEO_TITLE_MAX_CHARS, "SEO title")?;
        let seo_description = normalize_seo(
            seo_description,
            SEO_DESCRIPTION_MAX_CHARS,
            "SEO description",
        )?;
        let done = sqlx::query(
            "UPDATE site_pages SET seo_title = $4, seo_description = $5, updated_at = now() \
             WHERE tenant_id = $1 AND site_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(page.as_str())
        .bind(seo_title)
        .bind(seo_description)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Replaces a page's sections with `sections`, which must be a valid
    /// current-version envelope — this is the schema write gate. The stored
    /// value is the canonical serialization of the parsed envelope, so
    /// whatever is on disk always round-trips through the typed model.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the page isn't the tenant's;
    /// [`StoreError::Conflict`] carrying the schema violation (version,
    /// shape, or content rule — see [`crate::site_model::SectionSchemaError`]);
    /// [`StoreError::Db`].
    pub async fn set_page_sections(
        &self,
        site: &SiteId,
        page: &SitePageId,
        sections: Value,
    ) -> Result<()> {
        let envelope = SectionsEnvelope::from_value(sections)
            .map_err(|schema| StoreError::Conflict(schema.to_string()))?;
        let canonical = envelope
            .to_value()
            .map_err(|schema| StoreError::Conflict(schema.to_string()))?;
        let done = sqlx::query(
            "UPDATE site_pages SET sections = $4, updated_at = now() \
             WHERE tenant_id = $1 AND site_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(page.as_str())
        .bind(sqlx::types::Json(canonical))
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Makes `page` the site's home page, demoting the current one in the
    /// same transaction. The outgoing home page keeps its slug — which is why
    /// a home page at the empty slug must be given a real slug first.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the page isn't the tenant's;
    /// [`StoreError::Conflict`] when the current home page lives at the empty
    /// slug and cannot be demoted; [`StoreError::Db`].
    pub async fn set_home_page(&self, site: &SiteId, page: &SitePageId) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        sqlx::query(
            "UPDATE site_pages SET is_home = false, updated_at = now() \
             WHERE tenant_id = $1 AND site_id = $2 AND is_home AND id <> $3",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(page.as_str())
        .execute(&mut *tx)
        .await
        .map_err(|error| {
            // Demotion trips the slug-shape CHECK exactly when the current
            // home page has the empty slug — name that, not the constraint.
            if let sqlx::Error::Database(ref db) = error
                && db.constraint() == Some("site_pages_slug_shape")
            {
                return StoreError::Conflict(
                    "give the current home page a slug before choosing a new one".to_owned(),
                );
            }
            error.into()
        })?;
        let done = sqlx::query(
            "UPDATE site_pages SET is_home = true, updated_at = now() \
             WHERE tenant_id = $1 AND site_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(page.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(())
    }

    /// Rewrites the site's navigation order to exactly `order`, which must
    /// list every page of the site exactly once.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the site isn't the tenant's;
    /// [`StoreError::Conflict`] when `order` is not a permutation of the
    /// site's pages; [`StoreError::Db`].
    pub async fn reorder_site_pages(&self, site: &SiteId, order: &[SitePageId]) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM sites WHERE tenant_id = $1 AND id = $2)",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .fetch_one(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        if !exists {
            return Err(StoreError::NotFound);
        }
        let current: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM site_pages WHERE tenant_id = $1 AND site_id = $2 FOR UPDATE",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .fetch_all(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        let current: std::collections::HashSet<&str> = current.iter().map(String::as_str).collect();
        let requested: std::collections::HashSet<&str> =
            order.iter().map(SitePageId::as_str).collect();
        if requested.len() != order.len() || requested != current {
            return Err(StoreError::Conflict(
                "order must list every page of the site exactly once".to_owned(),
            ));
        }
        for (index, id) in order.iter().enumerate() {
            let position = i32::try_from(index)
                .map_err(|_| StoreError::Conflict("order is too long".to_owned()))?;
            sqlx::query(
                "UPDATE site_pages SET nav_order = $4, updated_at = now() \
                 WHERE tenant_id = $1 AND site_id = $2 AND id = $3",
            )
            .bind(self.tenant.as_str())
            .bind(site.as_str())
            .bind(id.as_str())
            .bind(position)
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        }
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(())
    }

    /// Deletes a page. Deleting the home page is allowed while drafting —
    /// the publish flow is what requires a home page to exist.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the page isn't the tenant's;
    /// [`StoreError::Db`].
    pub async fn delete_site_page(&self, site: &SiteId, page: &SitePageId) -> Result<()> {
        let done =
            sqlx::query("DELETE FROM site_pages WHERE tenant_id = $1 AND site_id = $2 AND id = $3")
                .bind(self.tenant.as_str())
                .bind(site.as_str())
                .bind(page.as_str())
                .execute(&self.pool)
                .await
                .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }
}

// ---- row types --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct SitePageRow {
    id: String,
    slug: String,
    title: String,
    sections: sqlx::types::Json<Value>,
    seo_title: Option<String>,
    seo_description: Option<String>,
    nav_order: i32,
    is_home: bool,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}
impl SitePageRow {
    fn into_page(self) -> SitePage {
        SitePage {
            id: SitePageId::new(self.id),
            slug: self.slug,
            title: self.title,
            sections: self.sections.0,
            seo_title: self.seo_title,
            seo_description: self.seo_description,
            nav_order: self.nav_order,
            is_home: self.is_home,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn slug_rules_accept_url_safe_segments() {
        for ok in ["about", "our-team", "a", "x2", "x".repeat(80).as_str()] {
            assert!(validate_page_slug(ok).is_ok(), "expected valid: {ok}");
        }
    }

    #[test]
    fn slug_rules_reject_bad_syntax() {
        let too_long = "x".repeat(81);
        for bad in [
            "",
            too_long.as_str(),
            "-leading",
            "trailing-",
            "Upper",
            "under_score",
            "dot.dot",
            "spa ce",
            "slash/nested",
            "ünïcode",
        ] {
            assert!(
                matches!(validate_page_slug(bad), Err(StoreError::Conflict(_))),
                "expected rejection: {bad:?}"
            );
        }
    }

    #[test]
    fn slug_rules_reject_reserved_public_paths() {
        for reserved in RESERVED_SLUGS {
            assert!(
                matches!(validate_page_slug(reserved), Err(StoreError::Conflict(_))),
                "expected reserved: {reserved}"
            );
        }
    }

    #[test]
    fn reserved_slug_entries_all_pass_the_syntax_rules() {
        // A reserved slug that fails syntax would be dead weight — the syntax
        // check runs first and would already have rejected it.
        for entry in RESERVED_SLUGS {
            assert!(
                !entry.is_empty()
                    && entry.len() <= PAGE_SLUG_MAX_LEN
                    && entry
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
                    && !entry.starts_with('-')
                    && !entry.ends_with('-'),
                "reserved slug not slug-safe: {entry}"
            );
        }
    }

    #[test]
    fn seo_overrides_normalize_blank_to_absent_and_bound_length() {
        assert_eq!(normalize_seo(None, 10, "SEO title").unwrap(), None);
        assert_eq!(normalize_seo(Some("   "), 10, "SEO title").unwrap(), None);
        assert_eq!(
            normalize_seo(Some("  hi  "), 10, "SEO title").unwrap(),
            Some("hi".to_owned())
        );
        assert!(matches!(
            normalize_seo(Some("12345678901"), 10, "SEO title"),
            Err(StoreError::Conflict(_))
        ));
    }
}
