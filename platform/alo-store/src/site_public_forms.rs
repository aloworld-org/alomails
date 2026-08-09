//! The public contact-form **write** of alo Sites (`docs/design/sites.md`,
//! form flow): a visitor's submission arriving through the anonymous
//! `POST /f/:form_id` endpoint on `alo-sites`. The endpoint holds only the
//! bare form id a rendered `contact_form` section carries; this module
//! resolves it to its owning tenant and writes the submission into that
//! tenant's scope in one conditional statement — there is no way to name a
//! tenant from the outside, so a cross-tenant write is unrepresentable.
//!
//! A form is writable only while its site is **live** (the site currently
//! has a published set — the same condition under which the form is served
//! at all). An unknown id, a deleted form, and a form on a draft site are
//! all the same `Ok(None)`: the public wire turns that into one uniform 404
//! with no existence leak.
//!
//! Per the privacy model, a submission stores the three posted fields and
//! nothing about the visitor's connection — the schema has no IP or
//! user-agent columns, and this door takes none.

use crate::error::{Result, StoreError};
use crate::id::SiteFormSubmissionId;
use crate::site_forms::normalize_submission;
use crate::site_public::SitePublicStore;

/// The longest id token this door will even send to the database. Real ids
/// are 22 characters (base64url of 16 random bytes); anything far outside
/// that shape is noise, not a lookup.
const FORM_ID_MAX_LEN: usize = 64;

impl SitePublicStore {
    /// Records a visitor's submission to the form with the bare id
    /// `form_id`, provided that form exists and its site is live. The three
    /// fields pass [`normalize_submission`] — the same write gate the
    /// authenticated door uses — before anything touches the database.
    ///
    /// Returns `Ok(None)` when the id resolves to no live form (unknown,
    /// deleted, or on a draft site — deliberately indistinguishable).
    ///
    /// # Errors
    /// [`StoreError::Validation`] naming the violated field rule (safe to
    /// show the visitor); [`StoreError::Db`] on failure.
    pub async fn add_public_form_submission(
        &self,
        form_id: &str,
        sender_name: &str,
        sender_email: &str,
        message: &str,
    ) -> Result<Option<SiteFormSubmissionId>> {
        let content = normalize_submission(sender_name, sender_email, message)?;
        if form_id.is_empty()
            || form_id.len() > FORM_ID_MAX_LEN
            || !form_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Ok(None);
        }
        let id = SiteFormSubmissionId::generate();
        // One statement resolves and writes: the inserted tenant_id is the
        // form's own, straight from the resolving subquery — the caller
        // never supplies a scope, so it can never supply the wrong one.
        let done = sqlx::query(
            "INSERT INTO site_form_submissions \
                 (tenant_id, form_id, id, sender_name, sender_email, message) \
             SELECT f.tenant_id, f.id, $2, $3, $4, $5 \
             FROM site_forms f \
             JOIN sites s ON s.tenant_id = f.tenant_id AND s.id = f.site_id \
             WHERE f.id = $1 AND s.published_publish_id IS NOT NULL",
        )
        .bind(form_id)
        .bind(id.as_str())
        .bind(&content.sender_name)
        .bind(&content.sender_email)
        .bind(&content.message)
        .execute(self.pool())
        .await
        .map_err(StoreError::Db)?;
        Ok((done.rows_affected() > 0).then_some(id))
    }
}
