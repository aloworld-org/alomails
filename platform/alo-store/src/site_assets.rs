//! Site image assets (alo Sites, ADR 0036): the tenant-scoped blob read the
//! sites family renders images from, and the shared answer to "which stored
//! content types may be served as a site image".
//!
//! A site references images (theme logo/favicon, section images) as plain
//! tenant blob ids — uploaded through the existing blob layer, typically
//! registered in Drive so the bytes stay referenced and user-visible. Reading
//! them for rendering is deliberately **not** the message-scoped
//! [`AccountStore::blob`] path (a site image is referenced by site JSON, not
//! by a message): [`AccountStore::site_image`] scopes by tenant alone, the
//! same posture as the Drive download path. The public serving counterpart
//! lives on [`crate::SitePublicStore`], scoped by the resolved published
//! site.

use bytes::Bytes;

use crate::account::AccountStore;
use crate::error::Result;
use crate::id::BlobId;

/// The stored content types the sites surfaces serve as images — anything
/// else answers "no image" rather than putting non-image bytes on an `<img>`
/// path. SVG is included (vector logos); the public service defangs it with a
/// `Content-Security-Policy` that forbids scripts, so an SVG opened as a
/// top-level document cannot run code on the site's origin.
pub const SITE_IMAGE_CONTENT_TYPES: &[&str] = &[
    "image/png",
    "image/jpeg",
    "image/gif",
    "image/webp",
    "image/avif",
    "image/svg+xml",
    "image/x-icon",
    "image/vnd.microsoft.icon",
];

/// The servable image content type of a stored blob, or `None` when the blob
/// must not be served on an image path. Parameters (`;charset=…`) are
/// tolerated; matching is case-insensitive on the media type.
#[must_use]
pub fn site_image_content_type(stored: Option<&str>) -> Option<&'static str> {
    let media = stored?.split(';').next()?.trim().to_ascii_lowercase();
    SITE_IMAGE_CONTENT_TYPES
        .iter()
        .find(|&&allowed| allowed == media)
        .copied()
}

/// One servable site image: its (allowlisted) content type and bytes.
#[derive(Debug, Clone)]
pub struct SiteImageData {
    /// The content type the bytes should be served with (from
    /// [`SITE_IMAGE_CONTENT_TYPES`]).
    pub content_type: &'static str,
    /// The image bytes.
    pub bytes: Bytes,
}

impl AccountStore {
    /// A tenant blob as a site image: `None` when the id does not resolve in
    /// this tenant **or** its stored content type is not a servable image
    /// type — the two are indistinguishable by design (an image path either
    /// serves an image or nothing). Used by the authenticated draft preview
    /// to inline images; tenant isolation is the SQL tenant scope, exactly
    /// like the Drive download path.
    ///
    /// # Errors
    /// [`crate::StoreError::Db`]/[`crate::StoreError::Blob`] on backend
    /// failure. (Runtime-checked query, like the other blob-by-tenant
    /// lookups.)
    pub async fn site_image(&self, id: &BlobId) -> Result<Option<SiteImageData>> {
        let row: Option<(String, Option<String>)> =
            sqlx::query_as("SELECT hash, content_type FROM blobs WHERE tenant_id = $1 AND id = $2")
                .bind(self.tenant.as_str())
                .bind(id.as_str())
                .fetch_optional(&self.pool)
                .await?;
        let Some((hash, stored_type)) = row else {
            return Ok(None);
        };
        let Some(content_type) = site_image_content_type(stored_type.as_deref()) else {
            return Ok(None);
        };
        let bytes = self.blobs.get(self.tenant.as_str(), &hash).await?;
        Ok(Some(SiteImageData {
            content_type,
            bytes,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowlist_matches_media_type_case_insensitively_with_parameters() {
        assert_eq!(
            site_image_content_type(Some("image/png")),
            Some("image/png")
        );
        assert_eq!(
            site_image_content_type(Some("IMAGE/JPEG; charset=binary")),
            Some("image/jpeg")
        );
        assert_eq!(
            site_image_content_type(Some("image/svg+xml")),
            Some("image/svg+xml")
        );
    }

    #[test]
    fn non_image_and_absent_types_are_refused() {
        for stored in [
            Some("text/html"),
            Some("application/octet-stream"),
            Some("text/html; charset=utf-8"),
            Some("image/png2"),
            Some(""),
            None,
        ] {
            assert_eq!(site_image_content_type(stored), None, "{stored:?}");
        }
    }
}
