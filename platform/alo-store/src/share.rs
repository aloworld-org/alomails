//! Large-file share links (alo Transfer): stream a file to storage and mint a
//! private, expiring download link for it, so a message can carry a link instead
//! of an oversized inline attachment. There is no size ceiling — the file is
//! streamed to its own object key (`<tenant>/share/<id>`), never buffered whole,
//! and the caller chooses the expiry window. The link token is stored **hashed**
//! (a DB read never yields a live link); the public download path hashes the
//! incoming token to look the row up. Creation is account-scoped; resolution +
//! reclaim are cross-tenant maintenance on [`Store`] (the public route has no
//! account).

use bytes::Bytes;
use futures::stream::Stream;

use crate::account::AccountStore;
use crate::blob::{ShareStream, hash_hex};
use crate::error::Result;
use crate::id::{TenantId, generate_token};
use crate::store::Store;

/// A freshly created share: the raw token (goes into the link URL, shown once),
/// the streamed byte count, and when it expires.
#[derive(Debug, Clone)]
pub struct ShareCreated {
    pub token: String,
    pub size: i64,
    pub expires_at_epoch: i64,
}

/// A resolved (live, unexpired) share, enough to stream its bytes.
#[derive(Debug, Clone)]
pub struct ShareTarget {
    pub tenant: TenantId,
    pub object_key: String,
    pub filename: String,
    pub content_type: String,
    pub size: i64,
}

impl AccountStore {
    /// Stream `content` to storage and create an expiring share link for it.
    /// Returns the raw token (it lives only in the link) and the streamed size.
    /// No size ceiling is applied; `expires_at_epoch` is caller-chosen Unix
    /// seconds.
    ///
    /// # Errors
    /// [`StoreError::Blob`](crate::error::StoreError::Blob) on a storage/stream
    /// failure; [`StoreError::Db`](crate::error::StoreError::Db) on failure.
    pub async fn create_share<S>(
        &self,
        content: S,
        filename: &str,
        content_type: &str,
        expires_at_epoch: i64,
    ) -> Result<ShareCreated>
    where
        S: Stream<Item = std::io::Result<Bytes>> + Unpin,
    {
        // A random object key for the bytes, and a separate 256-bit link token.
        let object_key = generate_token();
        let size = self
            .blobs
            .put_share_stream(self.tenant.as_str(), &object_key, content)
            .await? as i64;
        let token = format!("{}{}", generate_token(), generate_token());
        let token_hash = hash_hex(token.as_bytes());
        sqlx::query(
            "INSERT INTO file_shares \
                 (token_hash, tenant_id, user_id, object_key, filename, content_type, size, expires_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, to_timestamp($8))",
        )
        .bind(&token_hash)
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(&object_key)
        .bind(filename)
        .bind(content_type)
        .bind(size)
        .bind(expires_at_epoch)
        .execute(&self.pool)
        .await?;
        Ok(ShareCreated {
            token,
            size,
            expires_at_epoch,
        })
    }
}

impl Store {
    /// Resolve a share token to its target if it exists and has not expired.
    /// Cross-tenant: the token itself identifies the owning tenant. Returns
    /// `None` for an unknown or expired token (no oracle for "existed but
    /// expired").
    ///
    /// # Errors
    /// [`StoreError::Db`](crate::error::StoreError::Db) on failure.
    pub async fn resolve_share(&self, token: &str) -> Result<Option<ShareTarget>> {
        let token_hash = hash_hex(token.as_bytes());
        let row: Option<(String, String, String, String, i64)> = sqlx::query_as(
            "SELECT tenant_id, object_key, filename, content_type, size FROM file_shares \
             WHERE token_hash = $1 AND expires_at > now()",
        )
        .bind(&token_hash)
        .fetch_optional(self.pool())
        .await?;
        Ok(row.map(
            |(tenant, object_key, filename, content_type, size)| ShareTarget {
                tenant: TenantId::new(tenant),
                object_key,
                filename,
                content_type,
                size,
            },
        ))
    }

    /// Open a resolved share for streaming download (size + chunk stream). Never
    /// buffers the whole file.
    ///
    /// # Errors
    /// [`StoreError::NotFound`](crate::error::StoreError::NotFound) if the object
    /// is gone; [`StoreError::Blob`](crate::error::StoreError::Blob) otherwise.
    pub async fn open_share(&self, target: &ShareTarget) -> Result<ShareStream> {
        self.blobs()
            .get_share_stream(target.tenant.as_str(), &target.object_key)
            .await
    }

    /// Delete every expired share and reclaim its bytes. Safe to delete the
    /// object here (unlike message blobs): a share file has its own key and is
    /// never content-addressed/deduplicated with anything else. Returns how many
    /// shares expired. Cross-tenant maintenance.
    ///
    /// # Errors
    /// [`StoreError::Db`](crate::error::StoreError::Db) on failure.
    pub async fn sweep_expired_shares(&self) -> Result<usize> {
        let expired: Vec<(String, String)> = sqlx::query_as(
            "DELETE FROM file_shares WHERE expires_at <= now() \
             RETURNING tenant_id, object_key",
        )
        .fetch_all(self.pool())
        .await?;
        let count = expired.len();
        for (tenant, object_key) in expired {
            // Best-effort byte reclaim; a leftover object is harmless (the link
            // is already dead) and will be retried never — log-free by design.
            let _ = self.blobs().delete_share(&tenant, &object_key).await;
        }
        Ok(count)
    }
}
