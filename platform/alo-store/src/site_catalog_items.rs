//! The items of an alo Sites catalog — one dish, room, service, or course.
//!
//! Every write is tenant- and catalog-scoped and validated whole: a category
//! from another catalog, an image blob from another tenant, or a price outside
//! the sane range is refused before it can reach a published page. Prices are
//! integer minor units of the catalog's currency ([`crate::site_catalog`]).

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::{BlobId, SiteCatalogCategoryId, SiteCatalogId, SiteCatalogItemId, SiteId};
use crate::site_assets::site_image_content_type;
use crate::site_catalog::{
    SITE_CATALOG_DESCRIPTION_MAX_CHARS, SITE_CATALOG_MAX_ITEMS, SITE_CATALOG_MAX_PRICE_CENTS,
    SITE_CATALOG_PRICE_NOTE_MAX_CHARS, SiteCatalogAvailability, SiteCatalogItem,
    catalog_slug_conflict, validate_catalog_name, validate_catalog_slug,
};

/// Complete input for one catalog item — every write replaces the whole shape,
/// so a partial update can never leave a half-described item on a public page.
pub struct SiteCatalogItemInput<'a> {
    /// The grouping this item belongs to; it must live in the same catalog.
    pub category: Option<&'a SiteCatalogCategoryId>,
    pub name: &'a str,
    pub slug: &'a str,
    pub description: Option<&'a str>,
    /// Price in minor units of the catalog's currency. `None` means the item
    /// shows no price at all (enquiry-only), which is not zero.
    pub price_cents: Option<i64>,
    /// Short qualifier rendered beside the price ("per night", "from").
    pub price_note: Option<&'a str>,
    pub image: Option<&'a BlobId>,
    pub availability: SiteCatalogAvailability,
    pub position: i32,
}

impl AccountStore {
    /// Adds an item to one of the tenant's catalogs.
    ///
    /// # Errors
    /// [`StoreError::Validation`] on any invalid field or a full catalog;
    /// [`StoreError::Conflict`] on a duplicate handle;
    /// [`StoreError::NotFound`] when the catalog, category, or image is not
    /// the tenant's.
    pub async fn create_site_catalog_item(
        &self,
        site: &SiteId,
        catalog: &SiteCatalogId,
        input: &SiteCatalogItemInput<'_>,
    ) -> Result<SiteCatalogItemId> {
        let checked = self.check_catalog_item(site, catalog, input).await?;
        if self.count_site_catalog_items(catalog).await? >= SITE_CATALOG_MAX_ITEMS {
            return Err(StoreError::Validation(format!(
                "a catalog may hold at most {SITE_CATALOG_MAX_ITEMS} items"
            )));
        }
        let id = SiteCatalogItemId::generate();
        sqlx::query(
            "INSERT INTO site_catalog_items \
                (tenant_id, catalog_id, id, category_id, name, slug, description, \
                 price_cents, price_note, image_blob_id, availability, position) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
        )
        .bind(self.tenant.as_str())
        .bind(catalog.as_str())
        .bind(id.as_str())
        .bind(input.category.map(SiteCatalogCategoryId::as_str))
        .bind(&checked.name)
        .bind(&checked.slug)
        .bind(checked.description.as_ref())
        .bind(input.price_cents)
        .bind(checked.price_note.as_ref())
        .bind(input.image.map(BlobId::as_str))
        .bind(input.availability.as_str())
        .bind(input.position)
        .execute(&self.pool)
        .await
        .map_err(catalog_slug_conflict)?;
        Ok(id)
    }

    /// Lists a catalog's items in display order, including hidden ones — this
    /// is the editor's view. Publishing filters; listing does not.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the catalog is not the tenant's.
    pub async fn site_catalog_items(
        &self,
        site: &SiteId,
        catalog: &SiteCatalogId,
    ) -> Result<Vec<SiteCatalogItem>> {
        self.require_site_catalog(site, catalog).await?;
        let rows = sqlx::query_as::<_, SiteCatalogItemRow>(
            "SELECT id, category_id, name, slug, description, price_cents, price_note, \
                    image_blob_id, availability, position, source_key \
             FROM site_catalog_items \
             WHERE tenant_id = $1 AND catalog_id = $2 ORDER BY position, created_at, id",
        )
        .bind(self.tenant.as_str())
        .bind(catalog.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter()
            .map(SiteCatalogItemRow::into_item)
            .collect()
    }

    /// Returns one item of one of the tenant's catalogs.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the catalog is not the tenant's.
    pub async fn site_catalog_item(
        &self,
        site: &SiteId,
        catalog: &SiteCatalogId,
        item: &SiteCatalogItemId,
    ) -> Result<Option<SiteCatalogItem>> {
        self.require_site_catalog(site, catalog).await?;
        let row = sqlx::query_as::<_, SiteCatalogItemRow>(
            "SELECT id, category_id, name, slug, description, price_cents, price_note, \
                    image_blob_id, availability, position, source_key \
             FROM site_catalog_items \
             WHERE tenant_id = $1 AND catalog_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(catalog.as_str())
        .bind(item.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        row.map(SiteCatalogItemRow::into_item).transpose()
    }

    /// Replaces an item's complete editable shape.
    ///
    /// # Errors
    /// [`StoreError::Validation`] on any invalid field; [`StoreError::Conflict`]
    /// on a duplicate handle; [`StoreError::NotFound`] when the item, catalog,
    /// category, or image is not the tenant's.
    pub async fn update_site_catalog_item(
        &self,
        site: &SiteId,
        catalog: &SiteCatalogId,
        item: &SiteCatalogItemId,
        input: &SiteCatalogItemInput<'_>,
    ) -> Result<()> {
        let checked = self.check_catalog_item(site, catalog, input).await?;
        let done = sqlx::query(
            "UPDATE site_catalog_items SET category_id = $4, name = $5, slug = $6, \
                    description = $7, price_cents = $8, price_note = $9, image_blob_id = $10, \
                    availability = $11, position = $12, updated_at = now() \
             WHERE tenant_id = $1 AND catalog_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(catalog.as_str())
        .bind(item.as_str())
        .bind(input.category.map(SiteCatalogCategoryId::as_str))
        .bind(&checked.name)
        .bind(&checked.slug)
        .bind(checked.description.as_ref())
        .bind(input.price_cents)
        .bind(checked.price_note.as_ref())
        .bind(input.image.map(BlobId::as_str))
        .bind(input.availability.as_str())
        .bind(input.position)
        .execute(&self.pool)
        .await
        .map_err(catalog_slug_conflict)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Removes one item. Published snapshots that already carry it are
    /// untouched — the public page changes at the next publish, not before.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the item is not the tenant's.
    pub async fn delete_site_catalog_item(
        &self,
        site: &SiteId,
        catalog: &SiteCatalogId,
        item: &SiteCatalogItemId,
    ) -> Result<()> {
        self.require_site_catalog(site, catalog).await?;
        let done = sqlx::query(
            "DELETE FROM site_catalog_items WHERE tenant_id = $1 AND catalog_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(catalog.as_str())
        .bind(item.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// How many items a catalog holds — the cap the import seam also respects.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub(crate) async fn count_site_catalog_items(&self, catalog: &SiteCatalogId) -> Result<usize> {
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM site_catalog_items WHERE tenant_id = $1 AND catalog_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(catalog.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(usize::try_from(count).unwrap_or(usize::MAX))
    }

    /// Validates one complete item against its catalog, returning the trimmed
    /// values to store. Shared by the editor writes and the Base import.
    pub(crate) async fn check_catalog_item(
        &self,
        site: &SiteId,
        catalog: &SiteCatalogId,
        input: &SiteCatalogItemInput<'_>,
    ) -> Result<CheckedCatalogItem> {
        self.require_site_catalog(site, catalog).await?;
        let name = validate_catalog_name(input.name, "item name")?;
        let slug = validate_catalog_slug(input.slug, "item handle")?;
        let description = trimmed_within(
            input.description,
            SITE_CATALOG_DESCRIPTION_MAX_CHARS,
            "item description",
        )?;
        let price_note = trimmed_within(
            input.price_note,
            SITE_CATALOG_PRICE_NOTE_MAX_CHARS,
            "price note",
        )?;
        if let Some(price) = input.price_cents
            && !(0..=SITE_CATALOG_MAX_PRICE_CENTS).contains(&price)
        {
            return Err(StoreError::Validation(format!(
                "price must be between 0 and {SITE_CATALOG_MAX_PRICE_CENTS} minor units"
            )));
        }
        if price_note.is_some() && input.price_cents.is_none() {
            return Err(StoreError::Validation(
                "a price note needs a price to stand beside".to_owned(),
            ));
        }
        self.require_catalog_category(catalog, input.category)
            .await?;
        self.require_catalog_image(input.image).await?;
        Ok(CheckedCatalogItem {
            name,
            slug,
            description,
            price_note,
        })
    }

    async fn require_catalog_category(
        &self,
        catalog: &SiteCatalogId,
        category: Option<&SiteCatalogCategoryId>,
    ) -> Result<()> {
        let Some(category) = category else {
            return Ok(());
        };
        let found: Option<String> = sqlx::query_scalar(
            "SELECT id FROM site_catalog_categories \
             WHERE tenant_id = $1 AND catalog_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(catalog.as_str())
        .bind(category.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if found.is_none() {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    async fn require_catalog_image(&self, image: Option<&BlobId>) -> Result<()> {
        let Some(image) = image else { return Ok(()) };
        let stored: Option<Option<String>> =
            sqlx::query_scalar("SELECT content_type FROM blobs WHERE tenant_id = $1 AND id = $2")
                .bind(self.tenant.as_str())
                .bind(image.as_str())
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
}

/// The trimmed, bounded values of a validated item write.
pub(crate) struct CheckedCatalogItem {
    pub(crate) name: String,
    pub(crate) slug: String,
    pub(crate) description: Option<String>,
    pub(crate) price_note: Option<String>,
}

fn trimmed_within(value: Option<&str>, max: usize, role: &str) -> Result<Option<String>> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if value.chars().count() > max {
        return Err(StoreError::Validation(format!(
            "{role} must be at most {max} characters"
        )));
    }
    Ok(Some(value.to_owned()))
}

#[derive(sqlx::FromRow)]
struct SiteCatalogItemRow {
    id: String,
    category_id: Option<String>,
    name: String,
    slug: String,
    description: Option<String>,
    price_cents: Option<i64>,
    price_note: Option<String>,
    image_blob_id: Option<String>,
    availability: String,
    position: i32,
    source_key: Option<String>,
}

impl SiteCatalogItemRow {
    fn into_item(self) -> Result<SiteCatalogItem> {
        Ok(SiteCatalogItem {
            id: SiteCatalogItemId::new(self.id),
            category_id: self.category_id.map(SiteCatalogCategoryId::new),
            name: self.name,
            slug: self.slug,
            description: self.description,
            price_cents: self.price_cents,
            price_note: self.price_note,
            image: self.image_blob_id.map(BlobId::new),
            availability: SiteCatalogAvailability::parse(&self.availability)?,
            position: self.position,
            source_key: self.source_key,
        })
    }
}
