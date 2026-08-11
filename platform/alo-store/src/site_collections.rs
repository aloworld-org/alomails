//! Reusable alo Sites collections backed by rows in alo Base.
//!
//! Bindings store stable table/field ids rather than display names. Every
//! write resolves the selected Base through the account's Drive access and
//! validates both table membership and field types, so a rename is harmless
//! while a missing or cross-tenant table can never become a dangling source.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::base::{BaseField, BaseTable};
use crate::error::{Result, StoreError};
use crate::id::{BaseFieldId, BaseTableId, DriveNodeId, SiteCollectionId, SiteId};

/// Maximum length of the editor-facing collection name.
pub const SITE_COLLECTION_NAME_MAX_CHARS: usize = 120;
/// Maximum number of Base rows one collection may freeze in one publish.
pub const SITE_COLLECTION_MAX_ITEMS: usize = 200;
/// Maximum display length of one collection card title.
pub const SITE_COLLECTION_TITLE_MAX_CHARS: usize = 300;
/// Maximum length of collection summary/body values.
pub const SITE_COLLECTION_BODY_MAX_CHARS: usize = 5_000;

/// One normalized public card frozen from a Base record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SiteCollectionItem {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<crate::id::BlobId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_at: Option<String>,
}

/// One immutable collection snapshot belonging to a site publish.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SiteCollectionSnapshot {
    pub collection_id: SiteCollectionId,
    pub name: String,
    pub items: Vec<SiteCollectionItem>,
}

/// Stable semantic roles used by collection cards and detail pages.
///
/// `title` is the only required role. Optional roles let a small Base table
/// work immediately while richer tables can supply URLs, media, and dates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SiteCollectionFieldMapping {
    pub title: BaseFieldId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slug: Option<BaseFieldId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<BaseFieldId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<BaseFieldId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<BaseFieldId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<BaseFieldId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_at: Option<BaseFieldId>,
}

/// One tenant/site-owned connection to a Base table.
#[derive(Debug, Clone)]
pub struct SiteCollection {
    pub id: SiteCollectionId,
    pub name: String,
    pub base_node_id: DriveNodeId,
    pub base_table_id: BaseTableId,
    pub mapping: SiteCollectionFieldMapping,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

/// Complete input for a collection binding.
pub struct SiteCollectionInput<'a> {
    pub name: &'a str,
    pub base_node_id: &'a DriveNodeId,
    pub base_table_id: &'a BaseTableId,
    pub mapping: &'a SiteCollectionFieldMapping,
}

impl AccountStore {
    /// Connects one site to a readable Base table after validating all mapped
    /// fields. A missing/foreign site, Base, or table is indistinguishable.
    pub async fn create_site_collection(
        &self,
        site: &SiteId,
        input: &SiteCollectionInput<'_>,
    ) -> Result<SiteCollectionId> {
        self.require_site_collection_input(site, input).await?;
        let id = SiteCollectionId::generate();
        let mapping = serialize_mapping(input.mapping)?;
        let done = sqlx::query(
            "INSERT INTO site_collections \
                (tenant_id, site_id, id, name, base_table_id, mapping) \
             SELECT $1, $2, $3, $4, $5, $6 FROM sites \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(id.as_str())
        .bind(input.name.trim())
        .bind(input.base_table_id.as_str())
        .bind(sqlx::types::Json(mapping))
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(id)
    }

    /// Lists a site's collection bindings in stable creation order. A missing
    /// or foreign site has no visible collections.
    pub async fn site_collections(&self, site: &SiteId) -> Result<Vec<SiteCollection>> {
        let rows = sqlx::query_as::<_, SiteCollectionRow>(
            "SELECT c.id, c.name, t.node_id AS base_node_id, c.base_table_id, c.mapping, \
                    c.created_at, c.updated_at \
             FROM site_collections c \
             JOIN base_tables t ON t.tenant_id = c.tenant_id AND t.id = c.base_table_id \
             WHERE c.tenant_id = $1 AND c.site_id = $2 \
             ORDER BY c.created_at, c.id",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter()
            .map(SiteCollectionRow::into_collection)
            .collect()
    }

    /// Returns one tenant/site-scoped binding.
    pub async fn site_collection(
        &self,
        site: &SiteId,
        collection: &SiteCollectionId,
    ) -> Result<Option<SiteCollection>> {
        let row = sqlx::query_as::<_, SiteCollectionRow>(
            "SELECT c.id, c.name, t.node_id AS base_node_id, c.base_table_id, c.mapping, \
                    c.created_at, c.updated_at \
             FROM site_collections c \
             JOIN base_tables t ON t.tenant_id = c.tenant_id AND t.id = c.base_table_id \
             WHERE c.tenant_id = $1 AND c.site_id = $2 AND c.id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(collection.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        row.map(SiteCollectionRow::into_collection).transpose()
    }

    /// Replaces a binding's source and mapping atomically after validating the
    /// new complete shape.
    pub async fn update_site_collection(
        &self,
        site: &SiteId,
        collection: &SiteCollectionId,
        input: &SiteCollectionInput<'_>,
    ) -> Result<()> {
        self.require_site_collection_input(site, input).await?;
        let mapping = serialize_mapping(input.mapping)?;
        let done = sqlx::query(
            "UPDATE site_collections SET name = $4, base_table_id = $5, mapping = $6, \
                    updated_at = now() \
             WHERE tenant_id = $1 AND site_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(collection.as_str())
        .bind(input.name.trim())
        .bind(input.base_table_id.as_str())
        .bind(sqlx::types::Json(mapping))
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Disconnects a collection. Base rows are never deleted.
    pub async fn delete_site_collection(
        &self,
        site: &SiteId,
        collection: &SiteCollectionId,
    ) -> Result<()> {
        let done = sqlx::query(
            "DELETE FROM site_collections WHERE tenant_id = $1 AND site_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(collection.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    async fn require_site_collection_input(
        &self,
        site: &SiteId,
        input: &SiteCollectionInput<'_>,
    ) -> Result<()> {
        if self.site(site).await?.is_none() {
            return Err(StoreError::NotFound);
        }
        validate_collection_name(input.name)?;
        let base = self
            .base(input.base_node_id)
            .await?
            .ok_or(StoreError::NotFound)?;
        let table = base
            .tables
            .iter()
            .find(|table| table.id == *input.base_table_id)
            .ok_or(StoreError::NotFound)?;
        validate_mapping(table, input.mapping)
    }
}

fn serialize_mapping(mapping: &SiteCollectionFieldMapping) -> Result<serde_json::Value> {
    serde_json::to_value(mapping)
        .map_err(|error| StoreError::Conflict(format!("invalid collection mapping: {error}")))
}

fn validate_collection_name(name: &str) -> Result<()> {
    let name = name.trim();
    if name.is_empty() {
        return Err(StoreError::Validation(
            "collection name must not be empty".to_owned(),
        ));
    }
    if name.chars().count() > SITE_COLLECTION_NAME_MAX_CHARS {
        return Err(StoreError::Validation(format!(
            "collection name must be at most {SITE_COLLECTION_NAME_MAX_CHARS} characters"
        )));
    }
    Ok(())
}

pub(crate) fn validate_mapping(
    table: &BaseTable,
    mapping: &SiteCollectionFieldMapping,
) -> Result<()> {
    require_field(table, &mapping.title, "title", &["text"])?;
    require_optional_field(table, mapping.slug.as_ref(), "slug", &["text"])?;
    require_optional_field(table, mapping.summary.as_ref(), "summary", &["text"])?;
    require_optional_field(table, mapping.body.as_ref(), "body", &["text"])?;
    require_optional_field(table, mapping.image.as_ref(), "image", &["attachment"])?;
    // Base's `link` field is relational (an array of record ids), not a URL.
    // Public card destinations therefore intentionally map from plain text.
    require_optional_field(table, mapping.link.as_ref(), "link", &["text"])?;
    require_optional_field(
        table,
        mapping.published_at.as_ref(),
        "published date",
        &["date"],
    )
}

fn require_optional_field(
    table: &BaseTable,
    field: Option<&BaseFieldId>,
    role: &str,
    allowed_types: &[&str],
) -> Result<()> {
    match field {
        Some(field) => require_field(table, field, role, allowed_types),
        None => Ok(()),
    }
}

fn require_field(
    table: &BaseTable,
    field: &BaseFieldId,
    role: &str,
    allowed_types: &[&str],
) -> Result<()> {
    let field = table
        .fields
        .iter()
        .find(|candidate| candidate.id == *field)
        .ok_or_else(|| {
            StoreError::Validation(format!(
                "collection {role} references a field outside the selected table"
            ))
        })?;
    require_field_type(field, role, allowed_types)
}

fn require_field_type(field: &BaseField, role: &str, allowed_types: &[&str]) -> Result<()> {
    if allowed_types.contains(&field.field_type.as_str()) {
        return Ok(());
    }
    Err(StoreError::Validation(format!(
        "collection {role} must use a {} field",
        allowed_types.join(" or ")
    )))
}

#[derive(sqlx::FromRow)]
struct SiteCollectionRow {
    id: String,
    name: String,
    base_node_id: String,
    base_table_id: String,
    mapping: sqlx::types::Json<serde_json::Value>,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl SiteCollectionRow {
    fn into_collection(self) -> Result<SiteCollection> {
        let mapping = serde_json::from_value(self.mapping.0).map_err(|_| {
            StoreError::Conflict("collection has an invalid stored field mapping".to_owned())
        })?;
        Ok(SiteCollection {
            id: SiteCollectionId::new(self.id),
            name: self.name,
            base_node_id: DriveNodeId::new(self.base_node_id),
            base_table_id: BaseTableId::new(self.base_table_id),
            mapping,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}
