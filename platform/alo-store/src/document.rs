//! alo Docs documents (ADR 0015): tenant- and owner-scoped CRUD for
//! technical-authoring documents (Law 3: kept out of `account.rs`). Block
//! content is stored as JSONB and moved across this boundary as a JSON **string**
//! (`blocks::text` / `$::jsonb`), so the store needs no JSON-typed column mapping
//! and the shape stays owned by the caller.
//!
//! The `documents` table lands in migration 0020 and is not in the offline query
//! cache, so these use the runtime `sqlx::query*` path. Every statement carries
//! `tenant_id = $tenant AND owner_id = $user`, so a document is reachable only by
//! its owner — a cross-account or cross-tenant access matches zero rows.

use crate::account::AccountStore;
use crate::error::{Result, StoreError};

/// A document list entry — metadata only, no block content.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DocumentSummary {
    pub id: String,
    pub title: String,
    /// Last-modified timestamp, RFC 3339-ish (`YYYY-MM-DD HH:MM:SS+00`).
    pub updated_at: String,
}

/// A full document: metadata plus its blocks as a JSON string.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Document {
    pub id: String,
    pub title: String,
    /// The block array, serialized JSON (the caller owns the shape).
    pub blocks: String,
    pub updated_at: String,
}

impl AccountStore {
    /// This user's documents, newest-first (metadata only).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn list_documents(&self) -> Result<Vec<DocumentSummary>> {
        let rows = sqlx::query_as::<_, DocumentSummary>(
            "SELECT id, title, updated_at::text AS updated_at FROM documents \
             WHERE tenant_id = $1 AND owner_id = $2 ORDER BY updated_at DESC",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Creates an empty document owned by this user and returns it.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn create_document(&self, title: &str) -> Result<Document> {
        let row = sqlx::query_as::<_, Document>(
            "INSERT INTO documents (tenant_id, owner_id, title) VALUES ($1, $2, $3) \
             RETURNING id, title, blocks::text AS blocks, updated_at::text AS updated_at",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(title)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    /// Loads one of this user's documents by id, or `None` if it does not exist
    /// or is not theirs (the owner predicate makes those indistinguishable).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn get_document(&self, id: &str) -> Result<Option<Document>> {
        let row = sqlx::query_as::<_, Document>(
            "SELECT id, title, blocks::text AS blocks, updated_at::text AS updated_at \
             FROM documents WHERE id = $1 AND tenant_id = $2 AND owner_id = $3",
        )
        .bind(id)
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Saves the title and blocks of one of this user's documents. `blocks` is a
    /// JSON string, stored as JSONB. Returns [`StoreError::NotFound`] if the
    /// document does not exist or is not owned by this user.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when no owned row matches; [`StoreError::Db`] on
    /// failure (including invalid JSON in `blocks`).
    pub async fn save_document(&self, id: &str, title: &str, blocks: &str) -> Result<()> {
        let done = sqlx::query(
            "UPDATE documents SET title = $4, blocks = $5::jsonb, updated_at = now() \
             WHERE id = $1 AND tenant_id = $2 AND owner_id = $3",
        )
        .bind(id)
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(title)
        .bind(blocks)
        .execute(&self.pool)
        .await?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Deletes one of this user's documents. Returns [`StoreError::NotFound`] if
    /// no owned row matches.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when no owned row matches; [`StoreError::Db`] on
    /// failure.
    pub async fn delete_document(&self, id: &str) -> Result<()> {
        let done =
            sqlx::query("DELETE FROM documents WHERE id = $1 AND tenant_id = $2 AND owner_id = $3")
                .bind(id)
                .bind(self.tenant.as_str())
                .bind(self.user.as_str())
                .execute(&self.pool)
                .await?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }
}
