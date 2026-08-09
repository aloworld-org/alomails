//! Blog post metadata for alo Sites (ADR 0036). Post bodies remain alo Docs
//! documents; this module binds a tenant-visible Drive `doc` node to one site
//! and stores the public URL/title metadata around it.

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::{BlobId, DriveNodeId, SiteId, SitePostId};
use crate::site_assets::site_image_content_type;
use crate::site_pages::validate_page_slug;

const POST_TITLE_MAX_CHARS: usize = 200;
const POST_EXCERPT_MAX_CHARS: usize = 500;

/// The publication state of one post.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SitePostStatus {
    Draft,
    Published,
}

impl SitePostStatus {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Published => "published",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "draft" => Ok(Self::Draft),
            "published" => Ok(Self::Published),
            _ => Err(StoreError::Conflict(
                "post has an invalid status".to_owned(),
            )),
        }
    }
}

/// Site-facing metadata around an alo Docs document.
#[derive(Debug, Clone)]
pub struct SitePost {
    pub id: SitePostId,
    pub doc_node_id: DriveNodeId,
    pub slug: String,
    pub title: String,
    pub excerpt: String,
    pub cover_blob_id: Option<BlobId>,
    pub status: SitePostStatus,
    pub published_at: Option<OffsetDateTime>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

/// Fields required to create a post. Its initial state is always `draft`.
pub struct NewSitePost<'a> {
    pub doc_node_id: &'a DriveNodeId,
    pub slug: &'a str,
    pub title: &'a str,
    pub excerpt: &'a str,
    pub cover_blob_id: Option<&'a BlobId>,
}

/// Full editable metadata of a post. `PUT` callers provide this whole shape,
/// so `None` deliberately clears the cover image.
pub struct SitePostUpdate<'a> {
    pub slug: &'a str,
    pub title: &'a str,
    pub excerpt: &'a str,
    pub cover_blob_id: Option<&'a BlobId>,
}

fn validate_text(title: &str, excerpt: &str) -> Result<()> {
    if title.trim().is_empty() {
        return Err(StoreError::Conflict(
            "post title must not be empty".to_owned(),
        ));
    }
    if title.chars().count() > POST_TITLE_MAX_CHARS {
        return Err(StoreError::Conflict(format!(
            "post title must be at most {POST_TITLE_MAX_CHARS} characters"
        )));
    }
    if excerpt.chars().count() > POST_EXCERPT_MAX_CHARS {
        return Err(StoreError::Conflict(format!(
            "post excerpt must be at most {POST_EXCERPT_MAX_CHARS} characters"
        )));
    }
    Ok(())
}

fn map_constraints(error: sqlx::Error) -> StoreError {
    if let sqlx::Error::Database(ref db) = error {
        match db.constraint() {
            Some("site_posts_slug_unique") => {
                return StoreError::Conflict("post slug is already used on this site".to_owned());
            }
            Some("site_posts_doc_unique") => {
                return StoreError::Conflict(
                    "this document is already a post on this site".to_owned(),
                );
            }
            _ => {}
        }
    }
    error.into()
}

impl AccountStore {
    async fn require_post_document(&self, id: &DriveNodeId) -> Result<()> {
        let node = self.drive_node(id).await?.ok_or(StoreError::NotFound)?;
        if node.kind != "doc" || node.trashed {
            return Err(StoreError::Conflict(
                "post body must be a non-trashed alo document".to_owned(),
            ));
        }
        Ok(())
    }

    async fn require_post_cover(&self, id: Option<&BlobId>) -> Result<()> {
        let Some(id) = id else { return Ok(()) };
        let stored: Option<Option<String>> =
            sqlx::query_scalar("SELECT content_type FROM blobs WHERE tenant_id = $1 AND id = $2")
                .bind(self.tenant.as_str())
                .bind(id.as_str())
                .fetch_optional(&self.pool)
                .await
                .map_err(StoreError::Db)?;
        if stored
            .as_ref()
            .and_then(|value| site_image_content_type(value.as_deref()))
            .is_none()
        {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Creates a draft post whose document is readable by this account.
    pub async fn create_site_post(
        &self,
        site: &SiteId,
        new: &NewSitePost<'_>,
    ) -> Result<SitePostId> {
        validate_page_slug(new.slug)?;
        validate_text(new.title, new.excerpt)?;
        self.require_post_document(new.doc_node_id).await?;
        self.require_post_cover(new.cover_blob_id).await?;
        let id = SitePostId::generate();
        let done = sqlx::query(
            "INSERT INTO site_posts \
                (tenant_id, site_id, id, doc_node_id, slug, title, excerpt, cover_blob_id) \
             SELECT $1, $2, $3, $4, $5, $6, $7, $8 \
             FROM sites WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(id.as_str())
        .bind(new.doc_node_id.as_str())
        .bind(new.slug)
        .bind(new.title.trim())
        .bind(new.excerpt.trim())
        .bind(new.cover_blob_id.map(BlobId::as_str))
        .execute(&self.pool)
        .await
        .map_err(map_constraints)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(id)
    }

    /// Lists a site's posts, newest first. A foreign or missing site is an
    /// empty list, matching the rest of the Sites read surface.
    pub async fn site_posts(&self, site: &SiteId) -> Result<Vec<SitePost>> {
        let rows = sqlx::query_as::<_, SitePostRow>(
            "SELECT id, doc_node_id, slug, title, excerpt, cover_blob_id, status, \
                    published_at, created_at, updated_at \
             FROM site_posts WHERE tenant_id = $1 AND site_id = $2 \
             ORDER BY created_at DESC, id",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter().map(SitePostRow::into_post).collect()
    }

    /// Returns one tenant/site-scoped post.
    pub async fn site_post(&self, site: &SiteId, post: &SitePostId) -> Result<Option<SitePost>> {
        let row = sqlx::query_as::<_, SitePostRow>(
            "SELECT id, doc_node_id, slug, title, excerpt, cover_blob_id, status, \
                    published_at, created_at, updated_at \
             FROM site_posts WHERE tenant_id = $1 AND site_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(post.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        row.map(SitePostRow::into_post).transpose()
    }

    /// Replaces the editable public metadata while preserving body and state.
    pub async fn update_site_post(
        &self,
        site: &SiteId,
        post: &SitePostId,
        update: &SitePostUpdate<'_>,
    ) -> Result<()> {
        validate_page_slug(update.slug)?;
        validate_text(update.title, update.excerpt)?;
        self.require_post_cover(update.cover_blob_id).await?;
        let done = sqlx::query(
            "UPDATE site_posts SET slug = $4, title = $5, excerpt = $6, cover_blob_id = $7, \
                    updated_at = now() \
             WHERE tenant_id = $1 AND site_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(post.as_str())
        .bind(update.slug)
        .bind(update.title.trim())
        .bind(update.excerpt.trim())
        .bind(update.cover_blob_id.map(BlobId::as_str))
        .execute(&self.pool)
        .await
        .map_err(map_constraints)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Publishes a post and records the publication instant.
    pub async fn publish_site_post(&self, site: &SiteId, post: &SitePostId) -> Result<()> {
        self.set_site_post_publication(site, post, true).await
    }

    /// Returns a post to draft and clears its public publication instant.
    pub async fn unpublish_site_post(&self, site: &SiteId, post: &SitePostId) -> Result<()> {
        self.set_site_post_publication(site, post, false).await
    }

    async fn set_site_post_publication(
        &self,
        site: &SiteId,
        post: &SitePostId,
        publish: bool,
    ) -> Result<()> {
        let done = sqlx::query(
            "UPDATE site_posts SET status = CASE WHEN $4 THEN 'published' ELSE 'draft' END, \
                    published_at = CASE WHEN $4 THEN COALESCE(published_at, now()) ELSE NULL END, \
                    updated_at = now() \
             WHERE tenant_id = $1 AND site_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(post.as_str())
        .bind(publish)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Deletes draft metadata only; the alo document remains in Drive.
    pub async fn delete_site_post(&self, site: &SiteId, post: &SitePostId) -> Result<()> {
        let done =
            sqlx::query("DELETE FROM site_posts WHERE tenant_id = $1 AND site_id = $2 AND id = $3")
                .bind(self.tenant.as_str())
                .bind(site.as_str())
                .bind(post.as_str())
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
struct SitePostRow {
    id: String,
    doc_node_id: String,
    slug: String,
    title: String,
    excerpt: String,
    cover_blob_id: Option<String>,
    status: String,
    published_at: Option<OffsetDateTime>,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl SitePostRow {
    fn into_post(self) -> Result<SitePost> {
        Ok(SitePost {
            id: SitePostId::new(self.id),
            doc_node_id: DriveNodeId::new(self.doc_node_id),
            slug: self.slug,
            title: self.title,
            excerpt: self.excerpt,
            cover_blob_id: self.cover_blob_id.map(BlobId::new),
            status: SitePostStatus::parse(&self.status)?,
            published_at: self.published_at,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn post_status_tokens_are_stable() {
        assert_eq!(SitePostStatus::Draft.as_str(), "draft");
        assert_eq!(SitePostStatus::Published.as_str(), "published");
        assert!(SitePostStatus::parse("live").is_err());
    }
}
