//! The Base import seam: seeding a site catalog from a table in alo Base.
//!
//! A tenant who already keeps the menu, the price list or the room types in a
//! Base should not retype it. This copies those rows into the catalog **once**
//! and stops there — unlike a [collection](crate::site_collections), which
//! stays bound and is re-read on every publish. After the import the catalog is
//! the tenant's to edit; Base can change underneath without changing the site.
//!
//! Importing twice is safe: each imported row remembers the Base record it came
//! from (`source_key`), so a second run updates the row it created before
//! instead of duplicating it. Rows added by hand are never touched.
//!
//! Nothing here is a guess. A price the parser cannot read unambiguously, a
//! cell of the wrong type, or an image that is not the tenant's stops the
//! import with the row named, rather than publishing a wrong number.

use serde_json::Value;

use crate::account::AccountStore;
use crate::base::{BaseField, BaseTable};
use crate::error::{Result, StoreError};
use crate::id::{
    BaseFieldId, BaseTableId, BlobId, DriveNodeId, SiteCatalogCategoryId, SiteCatalogId, SiteId,
};
use crate::site_catalog::{
    SITE_CATALOG_MAX_CATEGORIES, SITE_CATALOG_MAX_ITEMS, SiteCatalogAvailability,
    catalog_slug_from_name, parse_price_minor_units, validate_catalog_slug,
};
use crate::site_catalog_items::SiteCatalogItemInput;

/// Which Base column plays which part in a catalog item. Only `name` is
/// required — a two-column table works on the first try.
#[derive(Debug, Clone)]
pub struct SiteCatalogImportMapping {
    pub name: BaseFieldId,
    pub description: Option<BaseFieldId>,
    /// A `number` or `text` column holding the price in major units.
    pub price: Option<BaseFieldId>,
    /// A `text` or `select` column naming the grouping; missing groupings are
    /// created in the catalog as they are met.
    pub category: Option<BaseFieldId>,
    /// An `attachment` column whose first file is the item's picture.
    pub image: Option<BaseFieldId>,
}

/// One import request: a readable Base table plus the mapping to read it with.
pub struct SiteCatalogImport<'a> {
    pub base_node_id: &'a DriveNodeId,
    pub base_table_id: &'a BaseTableId,
    pub mapping: &'a SiteCatalogImportMapping,
}

/// What an import did — the sentence the editor shows afterwards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SiteCatalogImportReport {
    /// Rows that became new catalog items.
    pub created: usize,
    /// Rows that updated an item a previous import of the same table created.
    pub updated: usize,
    /// Rows skipped because they had no name at all (an empty Base row).
    pub skipped: usize,
    /// Groupings created because the rows named them.
    pub categories_created: usize,
}

impl AccountStore {
    /// Copies the rows of a readable Base table into one of the tenant's
    /// catalogs, creating the categories they name.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the site, catalog, Base, or table is not
    /// the tenant's; [`StoreError::Validation`] naming the first row that
    /// cannot be read, or when the import would exceed the catalog's ceilings.
    pub async fn import_site_catalog_from_base(
        &self,
        site: &SiteId,
        catalog: &SiteCatalogId,
        import: &SiteCatalogImport<'_>,
    ) -> Result<SiteCatalogImportReport> {
        let stored = self.require_site_catalog(site, catalog).await?;
        let base = self
            .base(import.base_node_id)
            .await?
            .ok_or(StoreError::NotFound)?;
        let table = base
            .tables
            .iter()
            .find(|table| table.id == *import.base_table_id)
            .ok_or(StoreError::NotFound)?;
        validate_import_mapping(table, import.mapping)?;
        if table.records.len() > SITE_CATALOG_MAX_ITEMS {
            return Err(StoreError::Validation(format!(
                "that table has more than {SITE_CATALOG_MAX_ITEMS} rows; \
                 a catalog holds at most that many items"
            )));
        }

        let mut categories = self.import_category_index(site, catalog).await?;
        let mut existing_slugs = self.import_slug_index(catalog).await?;
        let mut report = SiteCatalogImportReport::default();
        for (index, record) in table.records.iter().enumerate() {
            let row = index + 1;
            let Some(name) = cell_text(&record.cells, Some(&import.mapping.name), "name", row)?
            else {
                report.skipped += 1;
                continue;
            };
            let price = match cell_text(&record.cells, import.mapping.price.as_ref(), "price", row)?
            {
                Some(written) => Some(
                    parse_price_minor_units(&written, &stored.currency)
                        .map_err(|error| StoreError::Validation(format!("row {row}: {error}")))?,
                ),
                None => None,
            };
            let category = match cell_text(
                &record.cells,
                import.mapping.category.as_ref(),
                "category",
                row,
            )? {
                Some(name) => Some(
                    self.import_category(catalog, &name, &mut categories, &mut report)
                        .await?,
                ),
                None => None,
            };
            let image = cell_attachment(&record.cells, import.mapping.image.as_ref(), row)?;
            let source_key = record.id.as_str().to_owned();
            let existing = self.import_existing_item(catalog, &source_key).await?;
            let slug = match &existing {
                Some((_, slug)) => slug.clone(),
                None => unique_slug(&name, &existing_slugs),
            };
            let description = cell_text(
                &record.cells,
                import.mapping.description.as_ref(),
                "description",
                row,
            )?;
            let input = SiteCatalogItemInput {
                category: category.as_ref(),
                name: &name,
                slug: &slug,
                description: description.as_deref(),
                price_cents: price,
                price_note: None,
                image: image.as_ref(),
                availability: SiteCatalogAvailability::Available,
                position: i32::try_from(index).unwrap_or(i32::MAX),
            };
            match existing {
                Some((id, _)) => {
                    self.update_site_catalog_item(site, catalog, &id, &input)
                        .await
                        .map_err(|error| import_row_error(row, error))?;
                    report.updated += 1;
                }
                None => {
                    let id = self
                        .create_site_catalog_item(site, catalog, &input)
                        .await
                        .map_err(|error| import_row_error(row, error))?;
                    sqlx::query(
                        "UPDATE site_catalog_items SET source_key = $4 \
                         WHERE tenant_id = $1 AND catalog_id = $2 AND id = $3",
                    )
                    .bind(self.tenant.as_str())
                    .bind(catalog.as_str())
                    .bind(id.as_str())
                    .bind(&source_key)
                    .execute(&self.pool)
                    .await
                    .map_err(StoreError::Db)?;
                    existing_slugs.push(slug);
                    report.created += 1;
                }
            }
        }
        Ok(report)
    }

    /// Handle → id for the catalog's existing categories.
    async fn import_category_index(
        &self,
        site: &SiteId,
        catalog: &SiteCatalogId,
    ) -> Result<Vec<(String, SiteCatalogCategoryId)>> {
        Ok(self
            .site_catalog_categories(site, catalog)
            .await?
            .into_iter()
            .map(|category| (category.slug, category.id))
            .collect())
    }

    async fn import_slug_index(&self, catalog: &SiteCatalogId) -> Result<Vec<String>> {
        sqlx::query_scalar::<_, String>(
            "SELECT slug FROM site_catalog_items WHERE tenant_id = $1 AND catalog_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(catalog.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)
    }

    async fn import_existing_item(
        &self,
        catalog: &SiteCatalogId,
        source_key: &str,
    ) -> Result<Option<(crate::id::SiteCatalogItemId, String)>> {
        let row: Option<(String, String)> = sqlx::query_as(
            "SELECT id, slug FROM site_catalog_items \
             WHERE tenant_id = $1 AND catalog_id = $2 AND source_key = $3",
        )
        .bind(self.tenant.as_str())
        .bind(catalog.as_str())
        .bind(source_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(row.map(|(id, slug)| (crate::id::SiteCatalogItemId::new(id), slug)))
    }

    /// Finds the category a row names, creating it when it is new.
    async fn import_category(
        &self,
        catalog: &SiteCatalogId,
        name: &str,
        index: &mut Vec<(String, SiteCatalogCategoryId)>,
        report: &mut SiteCatalogImportReport,
    ) -> Result<SiteCatalogCategoryId> {
        let slug = catalog_slug_from_name(name);
        let slug = validate_catalog_slug(&slug, "category slug").map_err(|_| {
            StoreError::Validation(format!(
                "the category {name} cannot become a handle; give it a name with letters or digits"
            ))
        })?;
        if let Some((_, id)) = index.iter().find(|(known, _)| *known == slug) {
            return Ok(id.clone());
        }
        if index.len() >= SITE_CATALOG_MAX_CATEGORIES {
            return Err(StoreError::Validation(format!(
                "that table names more than {SITE_CATALOG_MAX_CATEGORIES} categories"
            )));
        }
        let position = i32::try_from(index.len()).unwrap_or(i32::MAX);
        let id = SiteCatalogCategoryId::generate();
        sqlx::query(
            "INSERT INTO site_catalog_categories \
                (tenant_id, catalog_id, id, name, slug, position) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(self.tenant.as_str())
        .bind(catalog.as_str())
        .bind(id.as_str())
        .bind(name.trim())
        .bind(&slug)
        .bind(position)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        index.push((slug, id.clone()));
        report.categories_created += 1;
        Ok(id)
    }
}

fn import_row_error(row: usize, error: StoreError) -> StoreError {
    match error {
        StoreError::Validation(detail) => StoreError::Validation(format!("row {row}: {detail}")),
        StoreError::Conflict(detail) => StoreError::Validation(format!("row {row}: {detail}")),
        other => other,
    }
}

/// A handle no other item in this catalog is using, by adding `-2`, `-3`, … .
fn unique_slug(name: &str, taken: &[String]) -> String {
    let base = catalog_slug_from_name(name);
    let base = if base.is_empty() {
        "item".to_owned()
    } else {
        base
    };
    if !taken.contains(&base) {
        return base;
    }
    for suffix in 2..=SITE_CATALOG_MAX_ITEMS + 1 {
        let candidate = format!("{base}-{suffix}");
        if !taken.contains(&candidate) {
            return candidate;
        }
    }
    // Unreachable while the catalog is under its item ceiling; the insert's
    // own uniqueness constraint is the backstop if it ever is not.
    format!("{base}-x")
}

fn validate_import_mapping(table: &BaseTable, mapping: &SiteCatalogImportMapping) -> Result<()> {
    require_field(table, Some(&mapping.name), "name", &["text"])?;
    require_field(
        table,
        mapping.description.as_ref(),
        "description",
        &["text"],
    )?;
    require_field(table, mapping.price.as_ref(), "price", &["number", "text"])?;
    require_field(
        table,
        mapping.category.as_ref(),
        "category",
        &["text", "select"],
    )?;
    require_field(table, mapping.image.as_ref(), "image", &["attachment"])
}

fn require_field(
    table: &BaseTable,
    field: Option<&BaseFieldId>,
    role: &str,
    allowed: &[&str],
) -> Result<()> {
    let Some(field) = field else { return Ok(()) };
    let field: &BaseField = table
        .fields
        .iter()
        .find(|candidate| candidate.id == *field)
        .ok_or_else(|| {
            StoreError::Validation(format!("the {role} column is not in the selected table"))
        })?;
    if !allowed.contains(&field.field_type.as_str()) {
        return Err(StoreError::Validation(format!(
            "the {role} column must be a {} column",
            allowed.join(" or ")
        )));
    }
    Ok(())
}

/// A cell as text: text and select cells verbatim, number cells in their exact
/// decimal spelling (never through a float format), everything else refused.
fn cell_text(
    cells: &Value,
    field: Option<&BaseFieldId>,
    role: &str,
    row: usize,
) -> Result<Option<String>> {
    let Some(field) = field else { return Ok(None) };
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
        Value::Number(value) => Ok(Some(value.to_string())),
        _ => Err(StoreError::Validation(format!(
            "row {row}: the {role} cell is not a value a catalog can read"
        ))),
    }
}

/// The first attachment of a cell, as a blob id. The item write re-checks that
/// the blob is this tenant's and is an image, so a foreign id cannot pass here.
fn cell_attachment(
    cells: &Value,
    field: Option<&BaseFieldId>,
    row: usize,
) -> Result<Option<BlobId>> {
    let Some(field) = field else { return Ok(None) };
    let Some(value) = cells
        .as_object()
        .and_then(|cells| cells.get(field.as_str()))
    else {
        return Ok(None);
    };
    let first = match value {
        Value::Null => return Ok(None),
        Value::Array(values) if values.is_empty() => return Ok(None),
        Value::Array(values) => &values[0],
        value => value,
    };
    let blob = match first {
        Value::String(value) => value.trim().to_owned(),
        Value::Object(value) => value
            .get("blob_id")
            .or_else(|| value.get("blobId"))
            .or_else(|| value.get("id"))
            .and_then(Value::as_str)
            .map(str::trim)
            .ok_or_else(|| {
                StoreError::Validation(format!("row {row}: the image cell holds no file"))
            })?
            .to_owned(),
        _ => {
            return Err(StoreError::Validation(format!(
                "row {row}: the image cell is not a file"
            )));
        }
    };
    Ok((!blob.is_empty()).then(|| BlobId::new(blob)))
}
