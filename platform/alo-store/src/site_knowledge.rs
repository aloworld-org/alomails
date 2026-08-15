//! The site assistant's **Public knowledge** collection (ADR 0040 §1).
//!
//! One rule makes this module safe, and it is a sentence, not a permission
//! model: **whatever the assistant can read, the internet can read.** A row
//! here is the result of a deliberate act — the tenant published a document to
//! the assistant, past a screen that says exactly that. There is no picker
//! over Drive folders, no Space, no mail, no CRM record: a source is one
//! readable document, added one at a time.
//!
//! The collection stores only the *binding* (site → document). What the
//! assistant actually reads is assembled by [`crate::site_grounding`], which
//! extracts the document's current text — the same live-read the published
//! blog already does for a post's body, and for the same reason: publishing
//! the document to the assistant is the deliberate act on that document,
//! exactly as flipping a post to `published` is on its body. A trashed or
//! deleted document simply stops contributing (fail-closed); it is still
//! listed here until removed, so the owner can see and undo the binding.

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::extract::is_extractable;
use crate::id::{DriveNodeId, SiteId, SiteKnowledgeSourceId};

/// The most documents one site may publish to its assistant. A bound corpus
/// is an auditable corpus — and fifty documents is far past the point where a
/// site should be publishing pages instead.
pub const SITE_KNOWLEDGE_MAX_SOURCES: i64 = 50;

/// One document in a site's Public knowledge collection: the binding plus the
/// document's current Drive name (its display title).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteKnowledgeSource {
    pub id: SiteKnowledgeSourceId,
    /// The Drive node whose text the assistant reads.
    pub doc_node_id: DriveNodeId,
    /// The document's current Drive name.
    pub title: String,
    /// Whether the document is currently in the trash — still listed (so the
    /// binding is visible and removable) but contributing nothing to the
    /// grounding corpus.
    pub trashed: bool,
    pub added_by: String,
    pub added_at: OffsetDateTime,
}

impl AccountStore {
    /// Publishes one of the tenant's documents to `site`'s visitor assistant.
    /// The document must be readable text — an alo Doc, or a file whose bytes
    /// text extraction understands (PDF, Office, plain text) — because a
    /// source the assistant cannot read is a source that can only mislead.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the site or the document isn't the
    /// tenant's; [`StoreError::Conflict`] when the document is trashed, is a
    /// folder or an unreadable file type, is already in the collection, or
    /// the collection is full; [`StoreError::Db`].
    pub async fn add_site_knowledge_source(
        &self,
        site: &SiteId,
        doc: &DriveNodeId,
    ) -> Result<SiteKnowledgeSourceId> {
        self.require_site(site).await?;
        let node = self.drive_node(doc).await?.ok_or(StoreError::NotFound)?;
        if node.trashed {
            return Err(StoreError::Conflict(
                "a trashed file cannot be published to the site assistant".to_owned(),
            ));
        }
        let readable = matches!(node.kind.as_str(), "doc" | "file")
            && is_extractable(&node.kind, node.content_type.as_deref());
        if !readable {
            return Err(StoreError::Conflict(
                "only readable documents can be published to the site assistant \
                 (an alo document, a PDF, an Office file, or a text file)"
                    .to_owned(),
            ));
        }
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM site_knowledge_sources WHERE tenant_id = $1 AND site_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if count >= SITE_KNOWLEDGE_MAX_SOURCES {
            return Err(StoreError::Conflict(format!(
                "the Public knowledge collection holds at most {SITE_KNOWLEDGE_MAX_SOURCES} documents"
            )));
        }
        let id = SiteKnowledgeSourceId::generate();
        sqlx::query(
            "INSERT INTO site_knowledge_sources (tenant_id, site_id, id, doc_node_id, added_by) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(id.as_str())
        .bind(doc.as_str())
        .bind(self.user.as_str())
        .execute(&self.pool)
        .await
        .map_err(|error| match &error {
            sqlx::Error::Database(db)
                if db.constraint() == Some("site_knowledge_sources_doc_unique") =>
            {
                StoreError::Conflict(
                    "this document is already in the site's public knowledge".to_owned(),
                )
            }
            _ => StoreError::Db(error),
        })?;
        Ok(id)
    }

    /// The site's Public knowledge collection, oldest first. Trashed documents
    /// stay listed (flagged) so the owner can see and remove the binding; the
    /// grounding corpus already excludes them.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the site isn't the tenant's;
    /// [`StoreError::Db`].
    pub async fn site_knowledge_sources(&self, site: &SiteId) -> Result<Vec<SiteKnowledgeSource>> {
        self.require_site(site).await?;
        let rows = sqlx::query_as::<_, SiteKnowledgeSourceRow>(
            "SELECT k.id, k.doc_node_id, d.name, d.trashed, k.added_by, k.added_at \
             FROM site_knowledge_sources k \
             JOIN drive_nodes d ON d.tenant_id = k.tenant_id AND d.id = k.doc_node_id \
             WHERE k.tenant_id = $1 AND k.site_id = $2 \
             ORDER BY k.added_at, k.id",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows
            .into_iter()
            .map(SiteKnowledgeSourceRow::into_source)
            .collect())
    }

    /// Removes a document from the site's Public knowledge collection. The
    /// document itself stays in Drive untouched — only the assistant loses it.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the site or the source isn't the
    /// tenant's; [`StoreError::Db`].
    pub async fn remove_site_knowledge_source(
        &self,
        site: &SiteId,
        source: &SiteKnowledgeSourceId,
    ) -> Result<()> {
        let done = sqlx::query(
            "DELETE FROM site_knowledge_sources \
             WHERE tenant_id = $1 AND site_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(source.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// The tenant-scoped existence gate every method here starts with: a
    /// foreign or unknown site is one clean [`StoreError::NotFound`].
    pub(crate) async fn require_site(&self, site: &SiteId) -> Result<()> {
        let owned: Option<String> =
            sqlx::query_scalar("SELECT id FROM sites WHERE tenant_id = $1 AND id = $2")
                .bind(self.tenant.as_str())
                .bind(site.as_str())
                .fetch_optional(&self.pool)
                .await
                .map_err(StoreError::Db)?;
        owned.map(|_| ()).ok_or(StoreError::NotFound)
    }
}

#[derive(sqlx::FromRow)]
struct SiteKnowledgeSourceRow {
    id: String,
    doc_node_id: String,
    name: String,
    trashed: bool,
    added_by: String,
    added_at: OffsetDateTime,
}

impl SiteKnowledgeSourceRow {
    fn into_source(self) -> SiteKnowledgeSource {
        SiteKnowledgeSource {
            id: SiteKnowledgeSourceId::new(self.id),
            doc_node_id: DriveNodeId::new(self.doc_node_id),
            title: self.name,
            trashed: self.trashed,
            added_by: self.added_by,
            added_at: self.added_at,
        }
    }
}
