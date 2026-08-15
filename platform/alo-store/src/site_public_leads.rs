//! The public service's side of lead capture (ADR 0040 §2, item S3.03d):
//! from a **resolved** published site to CRM's own lead seam, and nothing
//! else.
//!
//! [`crate::crm_lead_capture`] is CRM's door and demands a `(tenant, owner)`
//! pair the caller resolved from a trusted row. For the anonymous public
//! service that row is the site the Host header resolved to — the same row
//! every other scope on [`SitePublicStore`] is taken from — and the owner is
//! the user who created the site. This module is where that resolution
//! lives, so the serving edge never touches a tenant or user id at all: it
//! hands over the [`PublishedSite`] it already holds and the visitor's typed
//! fields, and the pair travels from the database row straight into CRM's
//! door.
//!
//! What is deliberately absent: any attribution write. The aggregate 'chat'
//! counters ([`crate::site_public_conversions`]) are the only attribution
//! this feature stores, and they are recorded by the serving edge beside —
//! never inside — this call, so no failure mode can couple "the tenant's
//! funnel moved" to "a stranger's input reached CRM". The lead itself
//! carries its provenance in CRM's own vocabulary (the deal's `source`
//! field, stated by the caller), which is per-deal, tenant-owned data — not
//! a visitor journey.

use crate::crm_lead_capture::{CapturedLead, ConversationLead, CrmLeadCapture};
use crate::crm_pipelines::PipelineSeed;
use crate::error::{Result, StoreError};
use crate::id::UserId;
use crate::site_public::{PublishedSite, SitePublicStore};

impl SitePublicStore {
    /// Raises the conversation's lead in the resolved site's own tenant, or
    /// answers with the fact that made it unnecessary — CRM's duplicate
    /// rules, applied by CRM ([`CrmLeadCapture::capture`]).
    ///
    /// The owner of the raised card is the site's creator, read here from
    /// the site's own row; a site whose creator is somehow no longer a user
    /// of the tenant writes nothing (the seam re-proves the pair before any
    /// write). `seed` is the first-use board in the site's language, used
    /// only when the tenant has never opened CRM.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the site row is gone (unpublished or
    /// deleted since resolution); [`StoreError::Validation`] with CRM's own
    /// sentence on a field the visitor can fix; [`StoreError::Db`] on
    /// failure.
    pub async fn capture_conversation_lead(
        &self,
        site: &PublishedSite,
        seed: &PipelineSeed,
        lead: &ConversationLead,
    ) -> Result<CapturedLead> {
        let created_by: Option<String> =
            sqlx::query_scalar("SELECT created_by FROM sites WHERE tenant_id = $1 AND id = $2")
                .bind(site.tenant.as_str())
                .bind(site.site.as_str())
                .fetch_optional(self.pool())
                .await
                .map_err(StoreError::Db)?;
        let owner = created_by.ok_or(StoreError::NotFound)?;
        let door = CrmLeadCapture::open(
            self.pool().clone(),
            self.blobs().clone(),
            site.tenant.clone(),
            UserId::new(owner),
        );
        door.capture(seed, lead).await
    }
}
