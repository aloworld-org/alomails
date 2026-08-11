//! Drive — the file tree (ADR 0027), tenant-scoped through the account door.
//! Every node lives in exactly one **location**: the caller's personal "My
//! Files" or a [`crate::spaces`] Space. Access follows location — there is no
//! per-node permission:
//!
//! - personal → only the owning user may read or write;
//! - space → any member may read, an `editor`+ may write (ADR 0026);
//! - hr → the tenant's HR area: a tenant admin or a holder of
//!   [`TenantRole::Hr`] may read and write, and everybody else cannot see that
//!   it exists (alo HR, ADR 0035, wave B6.02b).
//!
//! A node the caller cannot read is [`StoreError::NotFound`] (existence hidden);
//! a node they can read but not write is [`StoreError::Forbidden`]. Bytes live
//! in the blob store; a node only references a blob (`blob_id`).

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::{DriveNodeId, SpaceId};
use crate::spaces::SpaceRole;
use crate::tenant_roles::TenantRole;

/// The stored `location_kind` of the tenant's HR area — see
/// [`DriveLocation::Hr`]. A constant because three places must agree on the
/// word: the two permission gates here, and `hr_documents.rs`, which refuses to
/// file a node that is not in it.
pub(crate) const HR_AREA: &str = "hr";

/// Largest file we pull back to build a content index. Bigger files stay
/// name-searchable; we just don't read the whole blob to index its text. Office
/// files and PDFs are commonly a few MiB, so this is well above the plain-text
/// range while still bounding the memory/CPU an index build can cost.
const INDEX_MAX_BYTES: i64 = 12 * 1024 * 1024;

/// Where a node lives — the unit access is scoped to.
#[derive(Debug, Clone)]
pub enum DriveLocation {
    /// The caller's own private files.
    Personal,
    /// A Space the caller belongs to.
    Space(SpaceId),
    /// **The tenant's HR area** — one per tenant, holding the papers HR keeps
    /// about the people it employs: contracts, amendments, letters (alo HR,
    /// ADR 0035, wave B6.02b; `docs/design/hr.md`).
    ///
    /// Read *and* write are the same gate here, and it is neither ownership nor
    /// a Space membership but the tenant-wide [`TenantRole::Hr`] (or being a
    /// tenant admin). That is the whole point: a contract must be a Drive node
    /// — one file tree, one version history, one download path — **and** must
    /// not be reachable by the colleague who happens to know its id. A person
    /// without the role is answered [`StoreError::NotFound`] rather than
    /// [`StoreError::Forbidden`], because on this location even the knowledge
    /// that a file exists is part of what is being kept.
    ///
    /// It is deliberately not a Space with a careful membership list: a Space's
    /// members are managed per Space by whoever manages it, and an HR area
    /// whose access could drift away from the HR role is an access rule with
    /// two sources of truth.
    Hr,
}

/// A node in the tree: a folder, an uploaded file, or a document.
#[derive(Debug, Clone)]
pub struct DriveNode {
    pub id: DriveNodeId,
    pub parent_id: Option<DriveNodeId>,
    pub location_kind: String,
    pub location_id: String,
    /// `folder` | `file` | `doc` | `sheet` | `slides`.
    pub kind: String,
    pub name: String,
    pub blob_id: Option<String>,
    pub size: i64,
    pub content_type: Option<String>,
    pub trashed: bool,
    pub source_kind: Option<String>,
    pub source_id: Option<String>,
    pub created_by: String,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

/// One entry in a node's version history.
#[derive(Debug, Clone)]
pub struct DriveVersion {
    pub version_no: i32,
    pub blob_id: String,
    pub size: i64,
    pub created_by: String,
    pub created_at: OffsetDateTime,
}

/// The fields to create a file/document node (a folder uses `create_folder`).
#[derive(Debug, Clone, Default)]
pub struct NewDriveFile {
    pub name: String,
    pub blob_id: String,
    pub size: i64,
    pub content_type: Option<String>,
    /// `file` (default), `doc`, `sheet`, or `slides`.
    pub kind: Option<String>,
    pub source_kind: Option<String>,
    pub source_id: Option<String>,
}

impl AccountStore {
    // ---- location permission -------------------------------------------------

    fn loc_parts(&self, loc: &DriveLocation) -> (String, String) {
        match loc {
            DriveLocation::Personal => ("personal".to_owned(), self.user.as_str().to_owned()),
            DriveLocation::Space(id) => ("space".to_owned(), id.as_str().to_owned()),
            // One HR area per tenant, named by the tenant itself. The id comes
            // from the handle, never from a caller, so "the HR area" is not a
            // thing a request can ask to be a different one of.
            DriveLocation::Hr => (HR_AREA.to_owned(), self.tenant.as_str().to_owned()),
        }
    }

    /// Ok if the caller may use the tenant's HR area, else [`StoreError::NotFound`].
    ///
    /// `NotFound` and not `Forbidden`, and the same answer for reading and for
    /// writing: on this location the existence of a file is itself the thing
    /// being kept, so a colleague who guesses a node id must learn nothing at
    /// all — not that it exists, not that they lack a role.
    async fn require_hr_area(&self, id: &str) -> Result<()> {
        if id != self.tenant.as_str() {
            return Err(StoreError::NotFound);
        }
        let facts = self.access_facts().await?;
        if facts.is_admin || facts.has(TenantRole::Hr) {
            Ok(())
        } else {
            Err(StoreError::NotFound)
        }
    }

    /// Ok if the caller may read this location; `NotFound` otherwise (a
    /// non-member, or someone else's personal, must not learn it exists).
    async fn require_location_read(&self, kind: &str, id: &str) -> Result<()> {
        match kind {
            "personal" if id == self.user.as_str() => Ok(()),
            "space" => {
                self.require_space_role(&SpaceId::new(id.to_owned()), SpaceRole::Viewer)
                    .await?;
                Ok(())
            }
            HR_AREA => self.require_hr_area(id).await,
            _ => Err(StoreError::NotFound),
        }
    }

    /// Ok if the caller may write this location. `NotFound` if they can't even
    /// see it; `Forbidden` if they can see it but lack the role (a space
    /// viewer). `require_space_role(Editor)` makes exactly that distinction.
    async fn require_location_write(&self, kind: &str, id: &str) -> Result<()> {
        match kind {
            "personal" if id == self.user.as_str() => Ok(()),
            "space" => {
                self.require_space_role(&SpaceId::new(id.to_owned()), SpaceRole::Editor)
                    .await?;
                Ok(())
            }
            // Reading and writing the HR area are one gate: there is no viewer
            // of somebody's contract who is not also the person who files it.
            HR_AREA => self.require_hr_area(id).await,
            _ => Err(StoreError::NotFound),
        }
    }

    async fn fetch_node_row(&self, id: &DriveNodeId) -> Result<Option<NodeRow>> {
        sqlx::query_as::<_, NodeRow>(
            "SELECT id, parent_id, location_kind, location_id, kind, name, blob_id, size, \
                    content_type, trashed, source_kind, source_id, created_by, created_at, updated_at \
             FROM drive_nodes WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)
    }

    /// The node if the caller may read it (else `NotFound`).
    async fn readable_row(&self, id: &DriveNodeId) -> Result<NodeRow> {
        let row = self.fetch_node_row(id).await?.ok_or(StoreError::NotFound)?;
        self.require_location_read(&row.location_kind, &row.location_id)
            .await?;
        Ok(row)
    }

    /// The node if the caller may write it (`NotFound` if unseeable,
    /// `Forbidden` if read-only).
    async fn writable_row(&self, id: &DriveNodeId) -> Result<NodeRow> {
        let row = self.fetch_node_row(id).await?.ok_or(StoreError::NotFound)?;
        self.require_location_write(&row.location_kind, &row.location_id)
            .await?;
        Ok(row)
    }

    /// Gate for another module attaching to a Drive node (e.g. alo Base, ADR
    /// 0032): the caller may **read** the node's location, else `NotFound`.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] / [`StoreError::Db`].
    pub(crate) async fn drive_require_read(&self, id: &DriveNodeId) -> Result<()> {
        self.readable_row(id).await.map(|_| ())
    }

    /// Gate for another module attaching to a Drive node: the caller may
    /// **write** the node's location, else `NotFound`/`Forbidden`.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] / [`StoreError::Forbidden`] / [`StoreError::Db`].
    pub(crate) async fn drive_require_write(&self, id: &DriveNodeId) -> Result<()> {
        self.writable_row(id).await.map(|_| ())
    }

    /// Whether the caller may write a node they can see: `true` (writable),
    /// `false` (read-only). `NotFound` if they cannot see it at all — used to
    /// decide a WOPI editor's edit-vs-view permission.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] / [`StoreError::Db`].
    pub async fn drive_writable(&self, id: &DriveNodeId) -> Result<bool> {
        self.readable_row(id).await?; // NotFound if invisible
        match self.writable_row(id).await {
            Ok(_) => Ok(true),
            Err(StoreError::Forbidden) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Inserts a bare (blob-less) node — used by attached modules that create a
    /// Drive node of their own kind (e.g. alo Base's `kind='base'`). Write
    /// access to the location is enforced.
    ///
    /// # Errors
    /// [`StoreError::NotFound`]/[`StoreError::Forbidden`] per access;
    /// [`StoreError::Conflict`] on a bad parent; [`StoreError::Db`].
    pub(crate) async fn drive_insert_node(
        &self,
        loc: &DriveLocation,
        parent: Option<&DriveNodeId>,
        kind: &str,
        name: &str,
    ) -> Result<DriveNodeId> {
        let (lkind, lid) = self.loc_parts(loc);
        self.require_location_write(&lkind, &lid).await?;
        self.check_parent(&lkind, &lid, parent).await?;
        let node = DriveNodeId::generate();
        sqlx::query(
            "INSERT INTO drive_nodes \
               (tenant_id, id, location_kind, location_id, parent_id, kind, name, created_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(self.tenant.as_str())
        .bind(node.as_str())
        .bind(&lkind)
        .bind(&lid)
        .bind(parent.map(DriveNodeId::as_str))
        .bind(kind)
        .bind(name)
        .bind(self.user.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(node)
    }

    // ---- reads ---------------------------------------------------------------

    /// A single node the caller can read, or `None`.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn drive_node(&self, id: &DriveNodeId) -> Result<Option<DriveNode>> {
        let Some(row) = self.fetch_node_row(id).await? else {
            return Ok(None);
        };
        match self
            .require_location_read(&row.location_kind, &row.location_id)
            .await
        {
            Ok(()) => Ok(Some(row.into_node())),
            Err(StoreError::NotFound) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// The live (non-trashed) children of a folder in a location the caller can
    /// read — `parent = None` lists the location root. Folders first, then by
    /// name.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the location isn't readable; [`StoreError::Db`].
    pub async fn drive_list(
        &self,
        loc: &DriveLocation,
        parent: Option<&DriveNodeId>,
    ) -> Result<Vec<DriveNode>> {
        let (kind, id) = self.loc_parts(loc);
        self.require_location_read(&kind, &id).await?;
        let rows = sqlx::query_as::<_, NodeRow>(
            "SELECT id, parent_id, location_kind, location_id, kind, name, blob_id, size, \
                    content_type, trashed, source_kind, source_id, created_by, created_at, updated_at \
             FROM drive_nodes \
             WHERE tenant_id = $1 AND location_kind = $2 AND location_id = $3 \
               AND parent_id IS NOT DISTINCT FROM $4 AND trashed = false \
             ORDER BY (kind = 'folder') DESC, lower(name), id",
        )
        .bind(self.tenant.as_str())
        .bind(&kind)
        .bind(&id)
        .bind(parent.map(DriveNodeId::as_str))
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(NodeRow::into_node).collect())
    }

    /// The trashed nodes in a location the caller can read.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the location isn't readable; [`StoreError::Db`].
    pub async fn drive_trash(&self, loc: &DriveLocation) -> Result<Vec<DriveNode>> {
        let (kind, id) = self.loc_parts(loc);
        self.require_location_read(&kind, &id).await?;
        let rows = sqlx::query_as::<_, NodeRow>(
            "SELECT id, parent_id, location_kind, location_id, kind, name, blob_id, size, \
                    content_type, trashed, source_kind, source_id, created_by, created_at, updated_at \
             FROM drive_nodes \
             WHERE tenant_id = $1 AND location_kind = $2 AND location_id = $3 AND trashed = true \
             ORDER BY updated_at DESC, id",
        )
        .bind(self.tenant.as_str())
        .bind(&kind)
        .bind(&id)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(NodeRow::into_node).collect())
    }

    /// A node's version history, newest first.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when unreadable; [`StoreError::Db`].
    pub async fn drive_versions(&self, id: &DriveNodeId) -> Result<Vec<DriveVersion>> {
        self.readable_row(id).await?;
        let rows = sqlx::query_as::<_, VersionRow>(
            "SELECT version_no, blob_id, size, created_by, created_at FROM drive_node_versions \
             WHERE tenant_id = $1 AND node_id = $2 ORDER BY version_no DESC",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(VersionRow::into_version).collect())
    }

    // ---- creates -------------------------------------------------------------

    /// Creates a folder in a location the caller can write. `parent` must be a
    /// folder in the same location.
    ///
    /// # Errors
    /// [`StoreError::NotFound`]/[`StoreError::Forbidden`] per access;
    /// [`StoreError::Conflict`] on a bad parent; [`StoreError::Db`].
    pub async fn drive_create_folder(
        &self,
        loc: &DriveLocation,
        parent: Option<&DriveNodeId>,
        name: &str,
    ) -> Result<DriveNodeId> {
        let (kind, id) = self.loc_parts(loc);
        self.require_location_write(&kind, &id).await?;
        self.check_parent(&kind, &id, parent).await?;
        let node = DriveNodeId::generate();
        sqlx::query(
            "INSERT INTO drive_nodes \
               (tenant_id, id, location_kind, location_id, parent_id, kind, name, created_by) \
             VALUES ($1, $2, $3, $4, $5, 'folder', $6, $7)",
        )
        .bind(self.tenant.as_str())
        .bind(node.as_str())
        .bind(&kind)
        .bind(&id)
        .bind(parent.map(DriveNodeId::as_str))
        .bind(name)
        .bind(self.user.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(node)
    }

    /// Creates a file/document node referencing an already-uploaded blob, and
    /// records it as version 1.
    ///
    /// # Errors
    /// [`StoreError::NotFound`]/[`StoreError::Forbidden`] per access;
    /// [`StoreError::Conflict`] on a bad parent; [`StoreError::Db`].
    pub async fn drive_create_file(
        &self,
        loc: &DriveLocation,
        parent: Option<&DriveNodeId>,
        new: &NewDriveFile,
    ) -> Result<DriveNodeId> {
        let (kind, id) = self.loc_parts(loc);
        self.require_location_write(&kind, &id).await?;
        self.check_parent(&kind, &id, parent).await?;
        let node_kind = new.kind.as_deref().unwrap_or("file");
        let node = DriveNodeId::generate();
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        sqlx::query(
            "INSERT INTO drive_nodes \
               (tenant_id, id, location_kind, location_id, parent_id, kind, name, blob_id, size, \
                content_type, source_kind, source_id, created_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
        )
        .bind(self.tenant.as_str())
        .bind(node.as_str())
        .bind(&kind)
        .bind(&id)
        .bind(parent.map(DriveNodeId::as_str))
        .bind(node_kind)
        .bind(&new.name)
        .bind(&new.blob_id)
        .bind(new.size)
        .bind(new.content_type.as_deref())
        .bind(new.source_kind.as_deref())
        .bind(new.source_id.as_deref())
        .bind(self.user.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        self.insert_version(&mut tx, node.as_str(), 1, &new.blob_id, new.size)
            .await?;
        tx.commit().await.map_err(StoreError::Db)?;
        // Best-effort content index (never fails the create).
        self.drive_index_node(node.as_str()).await;
        Ok(node)
    }

    /// Appends a new version to a writable file (a fresh upload/save), and points
    /// the node at it. Returns the new version number.
    ///
    /// # Errors
    /// [`StoreError::NotFound`]/[`StoreError::Forbidden`] per access; [`StoreError::Db`].
    pub async fn drive_add_version(
        &self,
        id: &DriveNodeId,
        blob_id: &str,
        size: i64,
    ) -> Result<i32> {
        self.writable_row(id).await?;
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let next: i32 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(version_no), 0) + 1 FROM drive_node_versions \
             WHERE tenant_id = $1 AND node_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_one(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        self.insert_version(&mut tx, id.as_str(), next, blob_id, size)
            .await?;
        sqlx::query(
            "UPDATE drive_nodes SET blob_id = $3, size = $4, updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(blob_id)
        .bind(size)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;
        // The bytes changed — re-index the node's content (best-effort).
        self.drive_index_node(id.as_str()).await;
        Ok(next)
    }

    /// Rebuilds a node's `content` full-text index from its current bytes, when
    /// they are text-extractable (a text file, an alo Doc, an Office file, or a
    /// PDF — see [`crate::extract`]). Best-effort: a failure to read the blob or
    /// parse the text leaves the node without a content index (still
    /// name-searchable) rather than failing the save that triggered it.
    /// Extraction runs on the blocking pool, so a slow or panicking parse can't
    /// stall the async runtime or crash the store.
    async fn drive_index_node(&self, node: &str) {
        // Read what we need to decide + extract. A missing/oversized/foreign row
        // simply means "don't index".
        let row = sqlx::query_as::<_, (String, Option<String>, Option<String>, i64)>(
            "SELECT kind, blob_id, content_type, size FROM drive_nodes \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(node)
        .fetch_optional(&self.pool)
        .await;
        let Ok(Some((kind, Some(blob_id), content_type, size))) = row else {
            return;
        };
        if size > INDEX_MAX_BYTES || !crate::extract::is_extractable(&kind, content_type.as_deref())
        {
            return;
        }
        // Resolve the blob's hash, then pull its bytes from the blob store.
        let hash = sqlx::query_scalar::<_, String>(
            "SELECT hash FROM blobs WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(&blob_id)
        .fetch_optional(&self.pool)
        .await;
        let Ok(Some(hash)) = hash else { return };
        let Ok(bytes) = self.blobs.get(self.tenant.as_str(), &hash).await else {
            return;
        };
        // Parse off the async runtime; a parser panic becomes a JoinError (→
        // "not indexed"), never a crash.
        let text = tokio::task::spawn_blocking(move || {
            crate::extract::extract_text(&kind, content_type.as_deref(), &bytes)
        })
        .await;
        let Ok(Some(text)) = text else { return };
        // Set the content vector. Best-effort.
        let _ = sqlx::query(
            "UPDATE drive_nodes SET content = to_tsvector('simple', $3) \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(node)
        .bind(&text)
        .execute(&self.pool)
        .await;
    }

    /// Restores an old version by appending it as a NEW current version (history
    /// is never rewritten — restore is itself undoable).
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when unwritable or no such version;
    /// [`StoreError::Forbidden`]; [`StoreError::Db`].
    pub async fn drive_restore_version(&self, id: &DriveNodeId, version_no: i32) -> Result<i32> {
        self.writable_row(id).await?;
        let old: Option<(String, i64)> = sqlx::query_as(
            "SELECT blob_id, size FROM drive_node_versions \
             WHERE tenant_id = $1 AND node_id = $2 AND version_no = $3",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(version_no)
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        let (blob_id, size) = old.ok_or(StoreError::NotFound)?;
        self.drive_add_version(id, &blob_id, size).await
    }

    // ---- mutations -----------------------------------------------------------

    /// Renames a writable node.
    ///
    /// # Errors
    /// [`StoreError::NotFound`]/[`StoreError::Forbidden`]/[`StoreError::Db`].
    pub async fn drive_rename(&self, id: &DriveNodeId, name: &str) -> Result<()> {
        self.writable_row(id).await?;
        sqlx::query(
            "UPDATE drive_nodes SET name = $3, updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(name)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }

    /// Moves a node (and, for a folder, its whole subtree) to another location
    /// and/or parent — **re-scoping its access** (ADR 0027). Requires write in
    /// both the source and the destination, and refuses to move a folder into
    /// itself or one of its own descendants.
    ///
    /// # Errors
    /// [`StoreError::NotFound`]/[`StoreError::Forbidden`] per access;
    /// [`StoreError::Conflict`] on a cycle or a bad destination parent;
    /// [`StoreError::Db`].
    pub async fn drive_move(
        &self,
        id: &DriveNodeId,
        dest: &DriveLocation,
        dest_parent: Option<&DriveNodeId>,
    ) -> Result<()> {
        self.writable_row(id).await?; // source write
        let (dkind, did) = self.loc_parts(dest);
        self.require_location_write(&dkind, &did).await?; // dest write
        self.check_parent(&dkind, &did, dest_parent).await?;

        let subtree = self.descendant_ids(id).await?;
        if let Some(p) = dest_parent
            && subtree.iter().any(|s| s == p.as_str())
        {
            return Err(StoreError::Conflict(
                "cannot move a folder into itself".into(),
            ));
        }
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        // Whole subtree changes location; only the moved root's parent changes.
        sqlx::query(
            "UPDATE drive_nodes SET location_kind = $3, location_id = $4, updated_at = now() \
             WHERE tenant_id = $1 AND id = ANY($2)",
        )
        .bind(self.tenant.as_str())
        .bind(&subtree)
        .bind(&dkind)
        .bind(&did)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        sqlx::query("UPDATE drive_nodes SET parent_id = $3 WHERE tenant_id = $1 AND id = $2")
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .bind(dest_parent.map(DriveNodeId::as_str))
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(())
    }

    /// Copies a node (and, for a folder, its subtree) into a destination the
    /// caller can write. Files share the source blob (dedup is free); the copy
    /// is independent thereafter. Requires read on the source, write on the
    /// destination.
    ///
    /// # Errors
    /// [`StoreError::NotFound`]/[`StoreError::Forbidden`] per access;
    /// [`StoreError::Conflict`] on a bad destination parent; [`StoreError::Db`].
    pub async fn drive_copy(
        &self,
        id: &DriveNodeId,
        dest: &DriveLocation,
        dest_parent: Option<&DriveNodeId>,
    ) -> Result<DriveNodeId> {
        self.readable_row(id).await?; // source read
        let (dkind, did) = self.loc_parts(dest);
        self.require_location_write(&dkind, &did).await?; // dest write
        self.check_parent(&dkind, &did, dest_parent).await?;

        // Load the whole subtree (already readable — same location as the root).
        let nodes = self.subtree_rows(id).await?;
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        // Map old id -> new id so children re-point at their copied parent.
        let mut remap: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        for n in &nodes {
            remap.insert(n.id.clone(), DriveNodeId::generate().as_str().to_owned());
        }
        let root_new = remap
            .get(id.as_str())
            .cloned()
            .ok_or(StoreError::NotFound)?;
        for n in &nodes {
            let new_id = &remap[&n.id];
            // The root's parent is the destination parent; a descendant keeps
            // its (remapped) parent within the copy.
            let new_parent = if n.id == id.as_str() {
                dest_parent.map(|p| p.as_str().to_owned())
            } else {
                n.parent_id.as_ref().and_then(|p| remap.get(p).cloned())
            };
            sqlx::query(
                "INSERT INTO drive_nodes \
                   (tenant_id, id, location_kind, location_id, parent_id, kind, name, blob_id, \
                    size, content_type, source_kind, source_id, created_by) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
            )
            .bind(self.tenant.as_str())
            .bind(new_id)
            .bind(&dkind)
            .bind(&did)
            .bind(new_parent)
            .bind(&n.kind)
            .bind(&n.name)
            .bind(&n.blob_id)
            .bind(n.size)
            .bind(&n.content_type)
            .bind(&n.source_kind)
            .bind(&n.source_id)
            .bind(self.user.as_str())
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
            if let Some(blob) = &n.blob_id {
                self.insert_version(&mut tx, new_id, 1, blob, n.size)
                    .await?;
            }
        }
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(DriveNodeId::new(root_new))
    }

    /// Soft-deletes a node and its subtree (moves them to trash).
    ///
    /// # Errors
    /// [`StoreError::NotFound`]/[`StoreError::Forbidden`]/[`StoreError::Db`].
    pub async fn drive_trash_node(&self, id: &DriveNodeId) -> Result<()> {
        self.set_trashed(id, true).await
    }

    /// Restores a node and its subtree from trash.
    ///
    /// # Errors
    /// [`StoreError::NotFound`]/[`StoreError::Forbidden`]/[`StoreError::Db`].
    pub async fn drive_restore_node(&self, id: &DriveNodeId) -> Result<()> {
        self.set_trashed(id, false).await
    }

    /// Permanently deletes a node and its subtree (nodes + versions). Blobs are
    /// left in the store — a blob may be shared across copies/versions.
    ///
    /// # Errors
    /// [`StoreError::NotFound`]/[`StoreError::Forbidden`]/[`StoreError::Db`].
    pub async fn drive_purge(&self, id: &DriveNodeId) -> Result<()> {
        self.writable_row(id).await?;
        let subtree = self.descendant_ids(id).await?;
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        sqlx::query("DELETE FROM drive_node_versions WHERE tenant_id = $1 AND node_id = ANY($2)")
            .bind(self.tenant.as_str())
            .bind(&subtree)
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        sqlx::query("DELETE FROM drive_nodes WHERE tenant_id = $1 AND id = ANY($2)")
            .bind(self.tenant.as_str())
            .bind(&subtree)
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(())
    }

    // ---- helpers -------------------------------------------------------------

    async fn set_trashed(&self, id: &DriveNodeId, trashed: bool) -> Result<()> {
        self.writable_row(id).await?;
        let subtree = self.descendant_ids(id).await?;
        sqlx::query(
            "UPDATE drive_nodes SET trashed = $3, updated_at = now() \
             WHERE tenant_id = $1 AND id = ANY($2)",
        )
        .bind(self.tenant.as_str())
        .bind(&subtree)
        .bind(trashed)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }

    /// A destination parent must be a live folder in the same location.
    async fn check_parent(&self, kind: &str, id: &str, parent: Option<&DriveNodeId>) -> Result<()> {
        let Some(parent) = parent else {
            return Ok(());
        };
        let row: Option<(String, bool)> = sqlx::query_as(
            "SELECT kind, trashed FROM drive_nodes \
             WHERE tenant_id = $1 AND id = $2 AND location_kind = $3 AND location_id = $4",
        )
        .bind(self.tenant.as_str())
        .bind(parent.as_str())
        .bind(kind)
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        match row {
            Some((k, false)) if k == "folder" => Ok(()),
            _ => Err(StoreError::Conflict(
                "parent must be a folder in this location".into(),
            )),
        }
    }

    /// A node id plus all of its descendants (via a recursive walk). The node is
    /// assumed already access-checked by the caller.
    async fn descendant_ids(&self, id: &DriveNodeId) -> Result<Vec<String>> {
        let ids: Vec<String> = sqlx::query_scalar(
            "WITH RECURSIVE sub AS ( \
                SELECT id FROM drive_nodes WHERE tenant_id = $1 AND id = $2 \
                UNION ALL \
                SELECT n.id FROM drive_nodes n JOIN sub ON n.parent_id = sub.id \
                  WHERE n.tenant_id = $1 \
             ) SELECT id FROM sub",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(ids)
    }

    /// The full rows of a node's subtree (for copy).
    async fn subtree_rows(&self, id: &DriveNodeId) -> Result<Vec<NodeRow>> {
        sqlx::query_as::<_, NodeRow>(
            "WITH RECURSIVE sub AS ( \
                SELECT * FROM drive_nodes WHERE tenant_id = $1 AND id = $2 \
                UNION ALL \
                SELECT n.* FROM drive_nodes n JOIN sub ON n.parent_id = sub.id \
                  WHERE n.tenant_id = $1 \
             ) SELECT id, parent_id, location_kind, location_id, kind, name, blob_id, size, \
                      content_type, trashed, source_kind, source_id, created_by, created_at, updated_at \
               FROM sub",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)
    }

    async fn insert_version(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        node_id: &str,
        version_no: i32,
        blob_id: &str,
        size: i64,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO drive_node_versions (tenant_id, node_id, version_no, blob_id, size, created_by) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(self.tenant.as_str())
        .bind(node_id)
        .bind(version_no)
        .bind(blob_id)
        .bind(size)
        .bind(self.user.as_str())
        .execute(&mut **tx)
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }
}

// ---- row types --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct NodeRow {
    id: String,
    parent_id: Option<String>,
    location_kind: String,
    location_id: String,
    kind: String,
    name: String,
    blob_id: Option<String>,
    size: i64,
    content_type: Option<String>,
    trashed: bool,
    source_kind: Option<String>,
    source_id: Option<String>,
    created_by: String,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}
impl NodeRow {
    fn into_node(self) -> DriveNode {
        DriveNode {
            id: DriveNodeId::new(self.id),
            parent_id: self.parent_id.map(DriveNodeId::new),
            location_kind: self.location_kind,
            location_id: self.location_id,
            kind: self.kind,
            name: self.name,
            blob_id: self.blob_id,
            size: self.size,
            content_type: self.content_type,
            trashed: self.trashed,
            source_kind: self.source_kind,
            source_id: self.source_id,
            created_by: self.created_by,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct VersionRow {
    version_no: i32,
    blob_id: String,
    size: i64,
    created_by: String,
    created_at: OffsetDateTime,
}
impl VersionRow {
    fn into_version(self) -> DriveVersion {
        DriveVersion {
            version_no: self.version_no,
            blob_id: self.blob_id,
            size: self.size,
            created_by: self.created_by,
            created_at: self.created_at,
        }
    }
}

impl AccountStore {
    /// Files in the caller's **own** Drive whose name matches `query`.
    ///
    /// Personal only, deliberately. A search that also swept every Space the
    /// caller belongs to would need each Space's read rule applied per row,
    /// and a permission check written inside a search is a permission check
    /// somebody will forget to update. Spaces get their own method the day
    /// they are asked for, with that rule stated once.
    ///
    /// Folders are excluded: someone asking where a file is does not mean the
    /// folder. Trashed nodes are excluded for the same reason they are hidden
    /// everywhere else.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn drive_find(&self, query: &str, limit: i64) -> Result<Vec<DriveNode>> {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let limit = limit.clamp(1, 20);
        // Escape the wildcards, or a query of "%" returns the whole drive.
        let like = format!("%{}%", query.replace('%', r"\%").replace('_', r"\_"));
        let rows = sqlx::query_as::<_, NodeRow>(
            "SELECT id, parent_id, location_kind, location_id, kind, name, blob_id, size, \
                    content_type, trashed, source_kind, source_id, created_by, created_at, updated_at \
             FROM drive_nodes \
             WHERE tenant_id = $1 AND location_kind = 'personal' AND location_id = $2 \
               AND trashed = false AND kind <> 'folder' \
               AND lower(name) LIKE $3 \
             ORDER BY updated_at DESC, lower(name) \
             LIMIT $4",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(like)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(NodeRow::into_node).collect())
    }
}
