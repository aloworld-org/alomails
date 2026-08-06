//! alo Base (ADR 0032): the relational data type — alo's native "sheet". A Base
//! is a Drive node (`kind='base'`); its tables/fields/records/views live here,
//! keyed to that node. Access is the Base node's Drive access, gated through
//! [`AccountStore::drive_require_read`]/[`AccountStore::drive_require_write`] —
//! so a Base in a Space is readable by members, writable by editors+, and a
//! non-member or another tenant is a clean `NotFound`. Records are the truth;
//! views are configuration over them (switching view never changes data).

use serde_json::Value;

use crate::account::AccountStore;
use crate::drive::DriveLocation;
use crate::error::{Result, StoreError};
use crate::id::{BaseFieldId, BaseRecordId, BaseTableId, BaseViewId, DriveNodeId};

/// The field types a Base column may have.
pub const FIELD_TYPES: [&str; 9] = [
    "text",
    "number",
    "date",
    "checkbox",
    "select",
    "multiselect",
    "attachment",
    "person",
    "link",
];
/// The view kinds a table may present (all over the same records).
pub const VIEW_KINDS: [&str; 4] = ["grid", "board", "calendar", "gallery"];

/// A whole Base: its tables, each with fields, views, and records.
#[derive(Debug, Clone)]
pub struct Base {
    pub node_id: DriveNodeId,
    pub tables: Vec<BaseTable>,
}

#[derive(Debug, Clone)]
pub struct BaseTable {
    pub id: BaseTableId,
    pub name: String,
    pub fields: Vec<BaseField>,
    pub views: Vec<BaseView>,
    pub records: Vec<BaseRecord>,
}

#[derive(Debug, Clone)]
pub struct BaseField {
    pub id: BaseFieldId,
    pub name: String,
    /// One of [`FIELD_TYPES`].
    pub field_type: String,
    pub options: Value,
}

#[derive(Debug, Clone)]
pub struct BaseView {
    pub id: BaseViewId,
    /// One of [`VIEW_KINDS`].
    pub kind: String,
    pub name: String,
    pub config: Value,
}

#[derive(Debug, Clone)]
pub struct BaseRecord {
    pub id: BaseRecordId,
    /// Cell values keyed by field id.
    pub cells: Value,
}

impl AccountStore {
    // ---- create -------------------------------------------------------------

    /// Creates a Base at a Drive location: the `base` node plus a default table
    /// (two text fields, a grid view, three empty rows). Write access to the
    /// location is required. Returns the Base's node id.
    ///
    /// # Errors
    /// [`StoreError::NotFound`]/[`StoreError::Forbidden`] per access;
    /// [`StoreError::Db`] on failure.
    pub async fn create_base(
        &self,
        loc: &DriveLocation,
        parent: Option<&DriveNodeId>,
        name: &str,
    ) -> Result<DriveNodeId> {
        // drive_insert_node enforces write access + a valid parent.
        let node = self.drive_insert_node(loc, parent, "base", name).await?;
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let table = BaseTableId::generate();
        sqlx::query(
            "INSERT INTO base_tables (tenant_id, id, node_id, name, position) \
             VALUES ($1, $2, $3, 'Table 1', 0)",
        )
        .bind(self.tenant.as_str())
        .bind(table.as_str())
        .bind(node.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        for (i, (fname, ftype)) in [("Name", "text"), ("Notes", "text")].iter().enumerate() {
            sqlx::query(
                "INSERT INTO base_fields (tenant_id, id, table_id, name, type, position) \
                 VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(self.tenant.as_str())
            .bind(BaseFieldId::generate().as_str())
            .bind(table.as_str())
            .bind(fname)
            .bind(ftype)
            .bind(i as f64)
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        }
        sqlx::query(
            "INSERT INTO base_views (tenant_id, id, table_id, kind, name, position) \
             VALUES ($1, $2, $3, 'grid', 'Grid', 0)",
        )
        .bind(self.tenant.as_str())
        .bind(BaseViewId::generate().as_str())
        .bind(table.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        for i in 0..3 {
            sqlx::query(
                "INSERT INTO base_records (tenant_id, id, table_id, cells, position) \
                 VALUES ($1, $2, $3, '{}'::jsonb, $4)",
            )
            .bind(self.tenant.as_str())
            .bind(BaseRecordId::generate().as_str())
            .bind(table.as_str())
            .bind(i as f64)
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        }
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(node)
    }

    // ---- read ---------------------------------------------------------------

    /// The whole Base at a node the caller can read, or `None`.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn base(&self, node: &DriveNodeId) -> Result<Option<Base>> {
        match self.drive_require_read(node).await {
            Ok(()) => {}
            Err(StoreError::NotFound) => return Ok(None),
            Err(e) => return Err(e),
        }
        let tables = sqlx::query_as::<_, TableRow>(
            "SELECT id, name FROM base_tables WHERE tenant_id = $1 AND node_id = $2 \
             ORDER BY position, created_at, id",
        )
        .bind(self.tenant.as_str())
        .bind(node.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        let mut out = Vec::with_capacity(tables.len());
        for t in tables {
            let fields = sqlx::query_as::<_, FieldRow>(
                "SELECT id, name, type, options FROM base_fields \
                 WHERE tenant_id = $1 AND table_id = $2 ORDER BY position, created_at, id",
            )
            .bind(self.tenant.as_str())
            .bind(&t.id)
            .fetch_all(&self.pool)
            .await
            .map_err(StoreError::Db)?;
            let views = sqlx::query_as::<_, ViewRow>(
                "SELECT id, kind, name, config FROM base_views \
                 WHERE tenant_id = $1 AND table_id = $2 ORDER BY position, created_at, id",
            )
            .bind(self.tenant.as_str())
            .bind(&t.id)
            .fetch_all(&self.pool)
            .await
            .map_err(StoreError::Db)?;
            let records = sqlx::query_as::<_, RecordRow>(
                "SELECT id, cells FROM base_records \
                 WHERE tenant_id = $1 AND table_id = $2 ORDER BY position, created_at, id",
            )
            .bind(self.tenant.as_str())
            .bind(&t.id)
            .fetch_all(&self.pool)
            .await
            .map_err(StoreError::Db)?;
            out.push(BaseTable {
                id: BaseTableId::new(t.id),
                name: t.name,
                fields: fields.into_iter().map(FieldRow::into_field).collect(),
                views: views.into_iter().map(ViewRow::into_view).collect(),
                records: records.into_iter().map(RecordRow::into_record).collect(),
            });
        }
        Ok(Some(Base {
            node_id: node.clone(),
            tables: out,
        }))
    }

    // ---- writes (each gated through the Base node) --------------------------

    /// Adds a table to a Base. Write access required.
    ///
    /// # Errors
    /// [`StoreError::NotFound`]/[`StoreError::Forbidden`]/[`StoreError::Db`].
    pub async fn base_add_table(&self, node: &DriveNodeId, name: &str) -> Result<BaseTableId> {
        self.drive_require_write(node).await?;
        let id = BaseTableId::generate();
        let pos = self
            .next_position("base_tables", "node_id", node.as_str())
            .await?;
        sqlx::query(
            "INSERT INTO base_tables (tenant_id, id, node_id, name, position) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(node.as_str())
        .bind(name)
        .bind(pos)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(id)
    }

    /// Adds a typed field to a table. Write access required; the type must be a
    /// known [`FIELD_TYPES`].
    ///
    /// # Errors
    /// [`StoreError::NotFound`]/[`StoreError::Forbidden`];
    /// [`StoreError::Conflict`] on a bad type; [`StoreError::Db`].
    pub async fn base_add_field(
        &self,
        table: &BaseTableId,
        name: &str,
        field_type: &str,
        options: &Value,
    ) -> Result<BaseFieldId> {
        if !FIELD_TYPES.contains(&field_type) {
            return Err(StoreError::Conflict("unknown field type".into()));
        }
        self.require_table_write(table).await?;
        let id = BaseFieldId::generate();
        let pos = self
            .next_position("base_fields", "table_id", table.as_str())
            .await?;
        sqlx::query(
            "INSERT INTO base_fields (tenant_id, id, table_id, name, type, options, position) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(table.as_str())
        .bind(name)
        .bind(field_type)
        .bind(sqlx::types::Json(options))
        .bind(pos)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(id)
    }

    /// Adds a record (row) to a table. Write access required.
    ///
    /// # Errors
    /// [`StoreError::NotFound`]/[`StoreError::Forbidden`]/[`StoreError::Db`].
    pub async fn base_add_record(
        &self,
        table: &BaseTableId,
        cells: &Value,
    ) -> Result<BaseRecordId> {
        self.require_table_write(table).await?;
        let id = BaseRecordId::generate();
        let pos = self
            .next_position("base_records", "table_id", table.as_str())
            .await?;
        sqlx::query(
            "INSERT INTO base_records (tenant_id, id, table_id, cells, position) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(table.as_str())
        .bind(sqlx::types::Json(cells))
        .bind(pos)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(id)
    }

    /// Replaces a record's cells. Write access required.
    ///
    /// # Errors
    /// [`StoreError::NotFound`]/[`StoreError::Forbidden`]/[`StoreError::Db`].
    pub async fn base_update_record(&self, record: &BaseRecordId, cells: &Value) -> Result<()> {
        self.require_record_write(record).await?;
        sqlx::query("UPDATE base_records SET cells = $3 WHERE tenant_id = $1 AND id = $2")
            .bind(self.tenant.as_str())
            .bind(record.as_str())
            .bind(sqlx::types::Json(cells))
            .execute(&self.pool)
            .await
            .map_err(StoreError::Db)?;
        Ok(())
    }

    /// Deletes a record. Write access required.
    ///
    /// # Errors
    /// [`StoreError::NotFound`]/[`StoreError::Forbidden`]/[`StoreError::Db`].
    pub async fn base_delete_record(&self, record: &BaseRecordId) -> Result<()> {
        self.require_record_write(record).await?;
        sqlx::query("DELETE FROM base_records WHERE tenant_id = $1 AND id = $2")
            .bind(self.tenant.as_str())
            .bind(record.as_str())
            .execute(&self.pool)
            .await
            .map_err(StoreError::Db)?;
        Ok(())
    }

    /// Adds a view over a table. Write access required; the kind must be a known
    /// [`VIEW_KINDS`].
    ///
    /// # Errors
    /// [`StoreError::NotFound`]/[`StoreError::Forbidden`];
    /// [`StoreError::Conflict`] on a bad kind; [`StoreError::Db`].
    pub async fn base_add_view(
        &self,
        table: &BaseTableId,
        kind: &str,
        name: &str,
        config: &Value,
    ) -> Result<BaseViewId> {
        if !VIEW_KINDS.contains(&kind) {
            return Err(StoreError::Conflict("unknown view kind".into()));
        }
        self.require_table_write(table).await?;
        let id = BaseViewId::generate();
        let pos = self
            .next_position("base_views", "table_id", table.as_str())
            .await?;
        sqlx::query(
            "INSERT INTO base_views (tenant_id, id, table_id, kind, name, config, position) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(table.as_str())
        .bind(kind)
        .bind(name)
        .bind(sqlx::types::Json(config))
        .bind(pos)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(id)
    }

    // ---- access helpers -----------------------------------------------------

    /// The Base node owning a table, gating on write access.
    async fn require_table_write(&self, table: &BaseTableId) -> Result<()> {
        let node = self.node_of_table(table).await?;
        self.drive_require_write(&node).await
    }

    /// The Base node owning a record, gating on write access.
    async fn require_record_write(&self, record: &BaseRecordId) -> Result<()> {
        let node: Option<String> = sqlx::query_scalar(
            "SELECT t.node_id FROM base_records r \
             JOIN base_tables t ON t.tenant_id = r.tenant_id AND t.id = r.table_id \
             WHERE r.tenant_id = $1 AND r.id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(record.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        let node = node.ok_or(StoreError::NotFound)?;
        self.drive_require_write(&DriveNodeId::new(node)).await
    }

    async fn node_of_table(&self, table: &BaseTableId) -> Result<DriveNodeId> {
        let node: Option<String> =
            sqlx::query_scalar("SELECT node_id FROM base_tables WHERE tenant_id = $1 AND id = $2")
                .bind(self.tenant.as_str())
                .bind(table.as_str())
                .fetch_optional(&self.pool)
                .await
                .map_err(StoreError::Db)?;
        node.map(DriveNodeId::new).ok_or(StoreError::NotFound)
    }

    async fn next_position(&self, table: &str, col: &str, key: &str) -> Result<f64> {
        let sql = format!(
            "SELECT COALESCE(MAX(position), -1) + 1 FROM {table} WHERE tenant_id = $1 AND {col} = $2"
        );
        sqlx::query_scalar(&sql)
            .bind(self.tenant.as_str())
            .bind(key)
            .fetch_one(&self.pool)
            .await
            .map_err(StoreError::Db)
    }
}

// ---- row types --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct TableRow {
    id: String,
    name: String,
}

#[derive(sqlx::FromRow)]
struct FieldRow {
    id: String,
    name: String,
    r#type: String,
    options: sqlx::types::Json<Value>,
}
impl FieldRow {
    fn into_field(self) -> BaseField {
        BaseField {
            id: BaseFieldId::new(self.id),
            name: self.name,
            field_type: self.r#type,
            options: self.options.0,
        }
    }
}

#[derive(sqlx::FromRow)]
struct ViewRow {
    id: String,
    kind: String,
    name: String,
    config: sqlx::types::Json<Value>,
}
impl ViewRow {
    fn into_view(self) -> BaseView {
        BaseView {
            id: BaseViewId::new(self.id),
            kind: self.kind,
            name: self.name,
            config: self.config.0,
        }
    }
}

#[derive(sqlx::FromRow)]
struct RecordRow {
    id: String,
    cells: sqlx::types::Json<Value>,
}
impl RecordRow {
    fn into_record(self) -> BaseRecord {
        BaseRecord {
            id: BaseRecordId::new(self.id),
            cells: self.cells.0,
        }
    }
}
