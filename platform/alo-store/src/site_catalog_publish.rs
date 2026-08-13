//! Freezing an alo Sites catalog into a publish, and reading it back.
//!
//! The editable catalog keeps moving — a dish sells out at eight in the
//! evening, a price rises in January. A published page must not: it shows
//! exactly what was true when the tenant pressed publish, until they press it
//! again. So publishing copies every catalog a page references into
//! `site_catalog_snapshots`, and public rendering reads only that copy.
//!
//! Two rules are enforced here rather than left to the renderer: **hidden
//! items never leave the editor** (they are absent from the snapshot, so no
//! public path can reach them), and a category is frozen with its handle, not
//! its id, so a published page never depends on an editable row.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::{BlobId, SiteCatalogId, SiteId, SitePublishId};
use crate::site_catalog::SITE_CATALOG_MAX_ITEMS;
use crate::site_model::{Section, SectionsEnvelope};

/// One grouping, frozen with a publish.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SiteCatalogSnapshotCategory {
    /// Stable public handle — what a catalog section filters by.
    pub slug: String,
    /// Display name at publish time.
    pub name: String,
}

/// One item, frozen with a publish.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SiteCatalogSnapshotItem {
    pub slug: String,
    pub name: String,
    /// Handle of the category this item belonged to, when it belonged to one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Price in minor units of the snapshot's currency.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price_cents: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price_note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<BlobId>,
    /// What the image shows, in words. Absent means the owner wrote none and
    /// the card falls back to the item name — which is also what every
    /// snapshot frozen before the description existed says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_alt: Option<String>,
    /// Shown, but marked unavailable. Hidden items are absent entirely.
    #[serde(default, skip_serializing_if = "is_false")]
    pub sold_out: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// One immutable catalog copy belonging to a site publish.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SiteCatalogSnapshot {
    pub catalog_id: SiteCatalogId,
    pub name: String,
    /// ISO 4217 code the prices in `items` are denominated in.
    pub currency: String,
    /// Whether this publish offers an order form over the catalog. Frozen
    /// like everything else here: what the published page shows is what the
    /// public order door ([`crate::site_public_orders`]) accepts.
    pub orders_enabled: bool,
    /// Groupings in display order; a category with no visible item is kept so
    /// the frozen structure matches what the editor saw.
    pub categories: Vec<SiteCatalogSnapshotCategory>,
    /// Visible items in display order.
    pub items: Vec<SiteCatalogSnapshotItem>,
}

impl AccountStore {
    /// The immutable catalog snapshots belonging to one tenant-owned publish.
    /// A foreign site/publish pair is indistinguishable from an empty result.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure; [`StoreError::Conflict`] when
    /// a stored snapshot cannot be read back.
    pub async fn site_publish_catalog_snapshots(
        &self,
        site: &SiteId,
        publish: &SitePublishId,
    ) -> Result<Vec<SiteCatalogSnapshot>> {
        let rows = sqlx::query_as::<_, SiteCatalogSnapshotRow>(
            "SELECT sn.catalog_id, sn.name, sn.currency, sn.orders_enabled, \
                    sn.categories, sn.items \
             FROM site_catalog_snapshots sn \
             JOIN site_publishes p ON p.tenant_id = sn.tenant_id AND p.id = sn.publish_id \
             WHERE sn.tenant_id = $1 AND p.site_id = $2 AND sn.publish_id = $3 \
             ORDER BY sn.catalog_id",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(publish.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter()
            .map(SiteCatalogSnapshotRow::into_snapshot)
            .collect()
    }

    /// Resolves one catalog exactly as publishing would, without writing a
    /// snapshot — the honest draft preview: hidden items are already gone,
    /// prices are already frozen, but nothing is published.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the catalog is not the tenant's;
    /// [`StoreError::Conflict`] when it is too large to freeze.
    pub async fn site_catalog_preview(
        &self,
        site: &SiteId,
        catalog: &SiteCatalogId,
    ) -> Result<SiteCatalogSnapshot> {
        // The publish path turns a dangling section reference into a refusal to
        // publish; this direct endpoint is an ordinary tenant-hidden 404.
        self.require_site_catalog(site, catalog).await?;
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let snapshot = self
            .resolve_catalog_snapshot(site, catalog, &mut tx)
            .await?;
        tx.rollback().await.map_err(StoreError::Db)?;
        Ok(snapshot)
    }

    /// Freezes every catalog referenced by the pages of this publish. Called
    /// inside the publish transaction, so a catalog that cannot be frozen
    /// refuses the whole publish rather than producing a half-published site.
    pub(crate) async fn freeze_referenced_catalogs(
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
        let mut referenced = std::collections::BTreeSet::new();
        for stored in section_values {
            let envelope = SectionsEnvelope::from_value(stored.0).map_err(|error| {
                StoreError::Conflict(format!("site page has invalid catalog content: {error}"))
            })?;
            for section in envelope.sections {
                if let Section::Catalog(catalog) = section {
                    referenced.insert(catalog.catalog_id.as_str().to_owned());
                }
            }
        }
        for catalog in referenced {
            let snapshot = self
                .resolve_catalog_snapshot(site, &SiteCatalogId::new(catalog), tx)
                .await?;
            let categories = serde_json::to_value(&snapshot.categories).map_err(|error| {
                StoreError::Conflict(format!("catalog could not be frozen: {error}"))
            })?;
            let items = serde_json::to_value(&snapshot.items).map_err(|error| {
                StoreError::Conflict(format!("catalog could not be frozen: {error}"))
            })?;
            sqlx::query(
                "INSERT INTO site_catalog_snapshots \
                    (tenant_id, publish_id, catalog_id, name, currency, orders_enabled, \
                     categories, items) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
            )
            .bind(self.tenant.as_str())
            .bind(publish.as_str())
            .bind(snapshot.catalog_id.as_str())
            .bind(&snapshot.name)
            .bind(&snapshot.currency)
            .bind(snapshot.orders_enabled)
            .bind(sqlx::types::Json(categories))
            .bind(sqlx::types::Json(items))
            .execute(&mut **tx)
            .await
            .map_err(StoreError::Db)?;
        }
        Ok(())
    }

    async fn resolve_catalog_snapshot(
        &self,
        site: &SiteId,
        catalog: &SiteCatalogId,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> Result<SiteCatalogSnapshot> {
        let header: Option<(String, String, bool)> = sqlx::query_as(
            "SELECT name, currency, orders_enabled FROM site_catalogs \
             WHERE tenant_id = $1 AND site_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(catalog.as_str())
        .fetch_optional(&mut **tx)
        .await
        .map_err(StoreError::Db)?;
        let Some((name, currency, orders_enabled)) = header else {
            return Err(StoreError::Conflict(
                "a page references a catalog that no longer exists".to_owned(),
            ));
        };
        let categories: Vec<(String, String)> = sqlx::query_as(
            "SELECT slug, name FROM site_catalog_categories \
             WHERE tenant_id = $1 AND catalog_id = $2 ORDER BY position, created_at, id",
        )
        .bind(self.tenant.as_str())
        .bind(catalog.as_str())
        .fetch_all(&mut **tx)
        .await
        .map_err(StoreError::Db)?;
        let rows = sqlx::query_as::<_, VisibleItemRow>(
            "SELECT i.slug, i.name, c.slug AS category, i.description, i.price_cents, \
                    i.price_note, i.image_blob_id, i.image_alt, i.availability \
             FROM site_catalog_items i \
             LEFT JOIN site_catalog_categories c \
                 ON c.tenant_id = i.tenant_id AND c.id = i.category_id \
             WHERE i.tenant_id = $1 AND i.catalog_id = $2 AND i.availability <> 'hidden' \
             ORDER BY i.position, i.created_at, i.id LIMIT $3",
        )
        .bind(self.tenant.as_str())
        .bind(catalog.as_str())
        .bind(i64::try_from(SITE_CATALOG_MAX_ITEMS + 1).unwrap_or(i64::MAX))
        .fetch_all(&mut **tx)
        .await
        .map_err(StoreError::Db)?;
        if rows.len() > SITE_CATALOG_MAX_ITEMS {
            return Err(StoreError::Conflict(format!(
                "catalog has more than {SITE_CATALOG_MAX_ITEMS} visible items; reduce it before publishing"
            )));
        }
        Ok(SiteCatalogSnapshot {
            catalog_id: catalog.clone(),
            name,
            currency,
            orders_enabled,
            categories: categories
                .into_iter()
                .map(|(slug, name)| SiteCatalogSnapshotCategory { slug, name })
                .collect(),
            items: rows.into_iter().map(VisibleItemRow::into_item).collect(),
        })
    }
}

#[derive(sqlx::FromRow)]
struct VisibleItemRow {
    slug: String,
    name: String,
    category: Option<String>,
    description: Option<String>,
    price_cents: Option<i64>,
    price_note: Option<String>,
    image_blob_id: Option<String>,
    image_alt: Option<String>,
    availability: String,
}

impl VisibleItemRow {
    fn into_item(self) -> SiteCatalogSnapshotItem {
        SiteCatalogSnapshotItem {
            slug: self.slug,
            name: self.name,
            category: self.category,
            description: self.description,
            price_cents: self.price_cents,
            price_note: self.price_note,
            image: self.image_blob_id.map(BlobId::new),
            image_alt: self.image_alt,
            // The query already excluded `hidden`; anything that is not
            // `available` is shown as unavailable rather than silently sold.
            sold_out: self.availability != "available",
        }
    }
}

#[derive(sqlx::FromRow)]
pub(crate) struct SiteCatalogSnapshotRow {
    catalog_id: String,
    name: String,
    currency: String,
    orders_enabled: bool,
    categories: sqlx::types::Json<Value>,
    items: sqlx::types::Json<Value>,
}

impl SiteCatalogSnapshotRow {
    pub(crate) fn into_snapshot(self) -> Result<SiteCatalogSnapshot> {
        let categories = serde_json::from_value(self.categories.0).map_err(|_| {
            StoreError::Conflict("catalog snapshot has invalid stored categories".to_owned())
        })?;
        let items = serde_json::from_value(self.items.0).map_err(|_| {
            StoreError::Conflict("catalog snapshot has invalid stored items".to_owned())
        })?;
        Ok(SiteCatalogSnapshot {
            catalog_id: SiteCatalogId::new(self.catalog_id),
            name: self.name,
            currency: self.currency,
            orders_enabled: self.orders_enabled,
            categories,
            items,
        })
    }
}
