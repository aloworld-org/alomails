//! Site publishing — the flow that makes an alo Sites website live (ADR
//! 0036), reached through the account door like [`crate::sites`]. A publish
//! freezes the draft into immutable rows: one `site_publishes` record holding
//! the theme, one `site_page_snapshots` row per page, and normalized Base rows
//! for every referenced collection. The site's
//! published-set pointer is flipped to the new publish in the same
//! transaction, and the public service reads only snapshots — so drafts are
//! unreachable from the internet by construction, not by filtering, and
//! editing (or deleting) a draft page never changes what is being served
//! until the next publish (`docs/design/sites.md`).

use std::collections::{BTreeSet, HashSet};

use serde_json::Value;
use time::format_description::well_known::Iso8601;
use time::{Date, OffsetDateTime};

use crate::account::AccountStore;
use crate::base::{BaseField, BaseTable};
use crate::error::{Result, StoreError};
use crate::id::{
    BaseFieldId, BaseTableId, BlobId, DriveNodeId, SiteCollectionId, SiteId, SitePageId,
    SitePublishId,
};
use crate::site_assets::site_image_content_type;
use crate::site_collections::{
    SITE_COLLECTION_BODY_MAX_CHARS, SITE_COLLECTION_MAX_ITEMS, SITE_COLLECTION_TITLE_MAX_CHARS,
    SiteCollectionFieldMapping, SiteCollectionItem, SiteCollectionSnapshot, validate_mapping,
};
use crate::site_model::{Section, SectionsEnvelope};
use crate::site_pages::validate_page_slug;
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
        // The draft pages and Base rows belong to one logical snapshot. A
        // repeatable-read transaction prevents an edit racing the copy from
        // producing a collection assembled from two source revisions.
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ")
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
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
        self.freeze_referenced_collections(site, &id, &mut tx)
            .await?;
        self.freeze_referenced_catalogs(site, &id, &mut tx).await?;
        self.freeze_referenced_bookings(site, &id, &mut tx).await?;
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

    /// One publish of the tenant's `site` — the frozen envelope (theme and
    /// language contract) behind a past or present published set. A publish of
    /// another tenant, or of another site, reads as `None`, exactly as an
    /// unknown id does.
    ///
    /// [`Self::current_site_publish`] answers the same shape for whichever
    /// publish is on the internet; this one answers for a named version, which
    /// is what previewing history needs.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn site_publish(
        &self,
        site: &SiteId,
        publish: &SitePublishId,
    ) -> Result<Option<SitePublish>> {
        let row = sqlx::query_as::<_, SitePublishRow>(
            "SELECT p.id, p.theme, p.default_locale, p.enabled_locales, \
                    p.published_by, p.published_at \
             FROM site_publishes p \
             WHERE p.tenant_id = $1 AND p.site_id = $2 AND p.id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(publish.as_str())
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

    /// The immutable collection snapshots belonging to one tenant-owned
    /// publish. A foreign site/publish pair is indistinguishable from an
    /// empty result.
    pub async fn site_publish_collection_snapshots(
        &self,
        site: &SiteId,
        publish: &SitePublishId,
    ) -> Result<Vec<SiteCollectionSnapshot>> {
        let rows = sqlx::query_as::<_, SiteCollectionSnapshotRow>(
            "SELECT sn.collection_id, sn.name, sn.items \
             FROM site_collection_snapshots sn \
             JOIN site_publishes p ON p.tenant_id = sn.tenant_id AND p.id = sn.publish_id \
             WHERE sn.tenant_id = $1 AND p.site_id = $2 AND sn.publish_id = $3 \
             ORDER BY sn.collection_id",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(publish.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter()
            .map(SiteCollectionSnapshotRow::into_snapshot)
            .collect()
    }

    /// Resolves one connected collection through the same normalization and
    /// validation path publishing uses, without writing a snapshot. The
    /// authenticated editor uses this for an honest draft preview: Base
    /// remains the editable source, while publishing still freezes its own
    /// immutable copy.
    pub async fn site_collection_preview(
        &self,
        site: &SiteId,
        collection: &SiteCollectionId,
    ) -> Result<SiteCollectionSnapshot> {
        // The publish path intentionally turns a dangling page reference into
        // a validation refusal. The direct editor endpoint is different: an
        // absent or foreign connection is an ordinary tenant-hidden 404.
        if self.site_collection(site, collection).await?.is_none() {
            return Err(StoreError::NotFound);
        }
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        self.resolve_collection_snapshot(site, collection, &mut tx)
            .await
    }

    async fn freeze_referenced_collections(
        &self,
        site: &SiteId,
        publish: &SitePublishId,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> Result<()> {
        let section_values = sqlx::query_scalar::<_, sqlx::types::Json<Value>>(
            "SELECT sections FROM site_page_snapshots \
             WHERE tenant_id = $1 AND publish_id = $2 ORDER BY page_id, locale",
        )
        .bind(self.tenant.as_str())
        .bind(publish.as_str())
        .fetch_all(&mut **tx)
        .await
        .map_err(StoreError::Db)?;
        let mut referenced = BTreeSet::new();
        for stored in section_values {
            let envelope = SectionsEnvelope::from_value(stored.0).map_err(|error| {
                StoreError::Conflict(format!("site page has invalid collection content: {error}"))
            })?;
            for section in envelope.sections {
                if let Section::Collection(collection) = section {
                    referenced.insert(collection.collection_id.as_str().to_owned());
                }
            }
        }
        for collection in referenced {
            let snapshot = self
                .resolve_collection_snapshot(site, &SiteCollectionId::new(collection), tx)
                .await?;
            let items = serde_json::to_value(&snapshot.items).map_err(|error| {
                StoreError::Conflict(format!("collection could not be frozen: {error}"))
            })?;
            sqlx::query(
                "INSERT INTO site_collection_snapshots \
                    (tenant_id, publish_id, collection_id, name, items) \
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(self.tenant.as_str())
            .bind(publish.as_str())
            .bind(snapshot.collection_id.as_str())
            .bind(&snapshot.name)
            .bind(sqlx::types::Json(items))
            .execute(&mut **tx)
            .await
            .map_err(StoreError::Db)?;
        }
        Ok(())
    }

    async fn resolve_collection_snapshot(
        &self,
        site: &SiteId,
        collection: &SiteCollectionId,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> Result<SiteCollectionSnapshot> {
        let binding: Option<(String, String, String, sqlx::types::Json<Value>)> = sqlx::query_as(
            "SELECT c.name, t.node_id, c.base_table_id, c.mapping \
             FROM site_collections c \
             JOIN base_tables t ON t.tenant_id = c.tenant_id AND t.id = c.base_table_id \
             WHERE c.tenant_id = $1 AND c.site_id = $2 AND c.id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(collection.as_str())
        .fetch_optional(&mut **tx)
        .await
        .map_err(StoreError::Db)?;
        let Some((name, node_id, table_id, mapping)) = binding else {
            return Err(StoreError::Conflict(
                "a page references a collection that is not connected".to_owned(),
            ));
        };
        self.drive_require_read(&DriveNodeId::new(node_id)).await?;
        let mapping: SiteCollectionFieldMapping =
            serde_json::from_value(mapping.0).map_err(|_| {
                StoreError::Conflict("collection has an invalid stored field mapping".to_owned())
            })?;
        let field_rows = sqlx::query_as::<_, (String, String, String, sqlx::types::Json<Value>)>(
            "SELECT id, name, type, options FROM base_fields \
             WHERE tenant_id = $1 AND table_id = $2 ORDER BY position, created_at, id",
        )
        .bind(self.tenant.as_str())
        .bind(&table_id)
        .fetch_all(&mut **tx)
        .await
        .map_err(StoreError::Db)?;
        let table = BaseTable {
            id: BaseTableId::new(table_id.clone()),
            name: String::new(),
            fields: field_rows
                .into_iter()
                .map(|(id, name, field_type, options)| BaseField {
                    id: BaseFieldId::new(id),
                    name,
                    field_type,
                    options: options.0,
                })
                .collect(),
            views: Vec::new(),
            records: Vec::new(),
        };
        validate_mapping(&table, &mapping).map_err(|error| {
            StoreError::Conflict(format!("collection mapping is no longer valid: {error}"))
        })?;
        let rows = sqlx::query_scalar::<_, sqlx::types::Json<Value>>(
            "SELECT cells FROM base_records \
             WHERE tenant_id = $1 AND table_id = $2 \
             ORDER BY position, created_at, id LIMIT $3",
        )
        .bind(self.tenant.as_str())
        .bind(&table_id)
        .bind(i64::try_from(SITE_COLLECTION_MAX_ITEMS + 1).unwrap_or(i64::MAX))
        .fetch_all(&mut **tx)
        .await
        .map_err(StoreError::Db)?;
        if rows.len() > SITE_COLLECTION_MAX_ITEMS {
            return Err(StoreError::Conflict(format!(
                "collection has more than {SITE_COLLECTION_MAX_ITEMS} rows; reduce it before publishing"
            )));
        }
        let mut items = Vec::with_capacity(rows.len());
        let mut slugs = HashSet::new();
        for (index, cells) in rows.into_iter().enumerate() {
            if let Some(item) = self
                .collection_item(&mapping, &cells.0, index + 1, tx)
                .await?
            {
                if let Some(slug) = &item.slug
                    && !slugs.insert(slug.clone())
                {
                    return Err(StoreError::Conflict(format!(
                        "collection row {} repeats slug {slug}",
                        index + 1
                    )));
                }
                items.push(item);
            }
        }
        Ok(SiteCollectionSnapshot {
            collection_id: collection.clone(),
            name,
            items,
        })
    }

    async fn collection_item(
        &self,
        mapping: &SiteCollectionFieldMapping,
        cells: &Value,
        row: usize,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> Result<Option<SiteCollectionItem>> {
        let title = mapped_text(cells, &mapping.title, "title", row)?;
        let slug = mapped_optional_text(cells, mapping.slug.as_ref(), "slug", row)?;
        let summary = mapped_optional_text(cells, mapping.summary.as_ref(), "summary", row)?;
        let body = mapped_optional_text(cells, mapping.body.as_ref(), "body", row)?;
        let link = mapped_optional_text(cells, mapping.link.as_ref(), "link", row)?;
        let published_at =
            mapped_optional_text(cells, mapping.published_at.as_ref(), "published date", row)?;
        let image_id = mapped_attachment(cells, mapping.image.as_ref(), row)?;
        let has_optional = [
            slug.as_ref(),
            summary.as_ref(),
            body.as_ref(),
            link.as_ref(),
            published_at.as_ref(),
            image_id.as_ref(),
        ]
        .into_iter()
        .any(|value| value.is_some());
        let Some(title) = title else {
            if has_optional {
                return Err(StoreError::Conflict(format!(
                    "collection row {row} has content but no title"
                )));
            }
            return Ok(None);
        };
        require_collection_length(&title, SITE_COLLECTION_TITLE_MAX_CHARS, "title", row)?;
        if let Some(value) = &summary {
            require_collection_length(value, SITE_COLLECTION_BODY_MAX_CHARS, "summary", row)?;
        }
        if let Some(value) = &body {
            require_collection_length(value, SITE_COLLECTION_BODY_MAX_CHARS, "body", row)?;
        }
        if let Some(value) = &slug {
            validate_page_slug(value).map_err(|error| {
                StoreError::Conflict(format!("collection row {row} has an invalid slug: {error}"))
            })?;
        }
        if let Some(value) = &link
            && !valid_collection_href(value)
        {
            return Err(StoreError::Conflict(format!(
                "collection row {row} link must be a site path, fragment, or http(s)/mailto/tel URL"
            )));
        }
        if let Some(value) = &published_at {
            Date::parse(value, &Iso8601::DATE).map_err(|_| {
                StoreError::Conflict(format!(
                    "collection row {row} published date must use YYYY-MM-DD"
                ))
            })?;
        }
        let image = match image_id {
            Some(id) => {
                let stored_type: Option<Option<String>> = sqlx::query_scalar(
                    "SELECT content_type FROM blobs WHERE tenant_id = $1 AND id = $2",
                )
                .bind(self.tenant.as_str())
                .bind(&id)
                .fetch_optional(&mut **tx)
                .await
                .map_err(StoreError::Db)?;
                if site_image_content_type(stored_type.flatten().as_deref()).is_none() {
                    return Err(StoreError::Conflict(format!(
                        "collection row {row} image is missing or is not a supported image"
                    )));
                }
                Some(BlobId::new(id))
            }
            None => None,
        };
        Ok(Some(SiteCollectionItem {
            title,
            slug,
            summary,
            body,
            image,
            link,
            published_at,
        }))
    }
}

fn mapped_optional_text(
    cells: &Value,
    field: Option<&BaseFieldId>,
    role: &str,
    row: usize,
) -> Result<Option<String>> {
    field.map_or(Ok(None), |field| mapped_text(cells, field, role, row))
}

fn mapped_attachment(
    cells: &Value,
    field: Option<&BaseFieldId>,
    row: usize,
) -> Result<Option<String>> {
    let Some(field) = field else {
        return Ok(None);
    };
    let Some(value) = cells
        .as_object()
        .and_then(|cells| cells.get(field.as_str()))
    else {
        return Ok(None);
    };
    let value = match value {
        Value::Null => return Ok(None),
        Value::Array(values) if values.is_empty() => return Ok(None),
        Value::Array(values) => &values[0],
        value => value,
    };
    let blob = match value {
        Value::String(value) => value.trim(),
        Value::Object(value) => value
            .get("blob_id")
            .or_else(|| value.get("blobId"))
            .or_else(|| value.get("id"))
            .and_then(Value::as_str)
            .map(str::trim)
            .ok_or_else(|| {
                StoreError::Conflict(format!(
                    "collection row {row} image has the wrong value type"
                ))
            })?,
        _ => {
            return Err(StoreError::Conflict(format!(
                "collection row {row} image has the wrong value type"
            )));
        }
    };
    if blob.is_empty() {
        Ok(None)
    } else {
        Ok(Some(blob.to_owned()))
    }
}

fn mapped_text(
    cells: &Value,
    field: &BaseFieldId,
    role: &str,
    row: usize,
) -> Result<Option<String>> {
    let Some(value) = cells
        .as_object()
        .and_then(|cells| cells.get(field.as_str()))
    else {
        return Ok(None);
    };
    match value {
        Value::Null => Ok(None),
        Value::String(value) if value.trim().is_empty() => Ok(None),
        Value::String(value) => Ok(Some(value.trim().to_owned())),
        _ => Err(StoreError::Conflict(format!(
            "collection row {row} {role} has the wrong value type"
        ))),
    }
}

fn require_collection_length(value: &str, max: usize, role: &str, row: usize) -> Result<()> {
    if value.chars().count() > max {
        return Err(StoreError::Conflict(format!(
            "collection row {row} {role} must be at most {max} characters"
        )));
    }
    Ok(())
}

fn valid_collection_href(href: &str) -> bool {
    if href.is_empty() || href.len() > 2_000 || href.starts_with("//") {
        return false;
    }
    if href.starts_with('/') || href.starts_with('#') {
        return true;
    }
    let lower = href.to_ascii_lowercase();
    ["http://", "https://", "mailto:", "tel:"]
        .iter()
        .any(|scheme| lower.starts_with(scheme))
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

#[derive(sqlx::FromRow)]
pub(crate) struct SiteCollectionSnapshotRow {
    collection_id: String,
    name: String,
    items: sqlx::types::Json<Value>,
}

impl SiteCollectionSnapshotRow {
    pub(crate) fn into_snapshot(self) -> Result<SiteCollectionSnapshot> {
        let items = serde_json::from_value(self.items.0).map_err(|_| {
            StoreError::Conflict("collection snapshot has invalid stored items".to_owned())
        })?;
        Ok(SiteCollectionSnapshot {
            collection_id: SiteCollectionId::new(self.collection_id),
            name: self.name,
            items,
        })
    }
}
