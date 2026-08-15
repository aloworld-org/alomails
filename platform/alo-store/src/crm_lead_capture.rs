//! CRM's public lead seam — the one write an anonymous site conversation may
//! make into CRM (ADR 0040 §2 and §4).
//!
//! A visitor talking to a published site's assistant is nobody: no session, no
//! user, no membership in the workspace. When they leave a name and an address,
//! the strongest thing the product can do is have the opportunity standing on
//! the tenant's board before anyone has read a message — and the most dangerous
//! thing it could do is let that anonymous path reach the rest of CRM. This
//! module is the boundary: a door opened with a `(tenant, owner)` pair the
//! caller must already have resolved from its own trusted row (for Sites, the
//! published site's record — never a request), able to do exactly one thing.
//!
//! This file belongs to CRM, not to Sites. What a lead *is*, where a new card
//! lands, and when somebody is already known stay CRM's to decide, in CRM's own
//! words: the card is written by [`AccountStore::insert_crm_deal_in`] — the
//! same writer the board, the lead import and the form handoff use — the first
//! board is seeded by [`AccountStore::crm_pipelines_or_seed`] exactly as a
//! first visit to the CRM screen seeds it, and the duplicate rules are the
//! lead import's (`docs/design/crm.md` § Importing leads): an address already
//! on an open deal or a customer is somebody the tenant knows, a company
//! domain already spoken for folds a colleague into the same opportunity, and
//! free-mail domains never fold ([`crate::crm_thread_match`], the same list
//! every other match lives by).
//!
//! What the seam will not carry is as deliberate as what it will:
//! [`ConversationLead`] has no field a transcript, a question or a page view
//! could travel in, so no individual visitor journey can cross this boundary
//! whatever the calling code does (the aggregate counters are Sites' own,
//! elsewhere). And a duplicate is an *answer*, not an error — the conversation
//! that finds an open deal is told which one, so the assistant can say "we
//! know you" instead of raising a twin card. Like the import, the duplicate
//! read is a snapshot, not a lock: two strangers writing in the same second
//! can produce a twin, and that is tidiness a person can delete, not an
//! invariant worth serialising every conversation for.

use sqlx::PgPool;

use crate::account::AccountStore;
use crate::blob::BlobStore;
use crate::crm_deals::NewDeal;
use crate::crm_pipelines::PipelineSeed;
use crate::crm_thread_match::{domain_of, is_free_mail_domain};
use crate::error::{Result, StoreError};
use crate::id::{CrmDealId, CrmPipelineId, CrmStageId, TenantId, UserId};

/// Longest visitor name accepted, matching what a deal card holds.
pub const LEAD_VISITOR_NAME_MAX_CHARS: usize = 200;
/// Longest visitor address accepted (the RFC 5321 path limit, the same cap the
/// public booking and form doors hold a stranger's address to).
pub const LEAD_VISITOR_EMAIL_MAX_CHARS: usize = 254;

/// What one conversation may say about the person in it — the whole of the
/// vocabulary this seam accepts. There is deliberately no message, question or
/// history field: a journey cannot be stored through a type that cannot hold
/// one.
#[derive(Debug, Clone)]
pub struct ConversationLead {
    /// The line on the card. The caller's words, in the caller's language —
    /// CRM writes titles it is handed and invents none
    /// (`docs/design/crm.md` § Seeding).
    pub title: String,
    /// The visitor's own name, as they gave it. May be blank — a name is
    /// theirs to offer.
    pub visitor_name: String,
    /// The visitor's address — required, and the field every duplicate rule
    /// turns on.
    pub visitor_email: String,
    /// The company they named, or blank; a conversation does not insist.
    pub company_name: String,
    /// Where the opportunity came from — a fact the caller states (for Sites,
    /// the site's subdomain), never a word this store makes up.
    pub source: String,
}

/// What a capture did. A duplicate is an answer, never an error: the
/// conversation is told the tenant already knows this person, and nothing is
/// raised twice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapturedLead {
    /// A new opportunity now stands on the tenant's board.
    Created(CrmDealId),
    /// An **open** deal already carries this address (or its company domain);
    /// the conversation is answered with that deal instead of a twin.
    AlreadyKnown(CrmDealId),
    /// The address (or its company domain) belongs to a billing customer and
    /// no open deal — somebody the tenant already does business with, so no
    /// lead is raised.
    AlreadyCustomer,
}

/// A write-only door into one tenant's CRM that can only raise a lead. Open it
/// with a tenant and owner resolved from a trusted row; everything it does is
/// scoped to that pair.
pub struct CrmLeadCapture {
    account: AccountStore,
}

impl CrmLeadCapture {
    /// Opens the lead door of one tenant's CRM.
    ///
    /// The caller vouches for the pair: `tenant` and `owner` must come from a
    /// row the caller already trusts (a site's own record, never a request).
    /// `owner` is who the raised card belongs to, and is re-checked against
    /// the tenant's users before anything is written — a pair that does not
    /// hold writes nothing.
    #[must_use]
    pub fn open(pool: PgPool, blobs: BlobStore, tenant: TenantId, owner: UserId) -> Self {
        Self {
            account: AccountStore {
                pool,
                blobs,
                tenant,
                user: owner,
            },
        }
    }

    /// Raises the conversation's lead, or answers with the record that made it
    /// unnecessary.
    ///
    /// The card lands where CRM's own defaults put a new lead: the tenant's
    /// first active board — seeded from `seed`, in the caller's language, when
    /// the tenant has never had one — in its first live column, the same
    /// landing place the lead import defaults to. It carries the visitor's own
    /// name and address, the caller's title and source, no value (a
    /// conversation states no number, and the assistant may never invent one —
    /// ADR 0040 §2), and the owner the door was opened for.
    ///
    /// # Errors
    /// [`StoreError::Validation`] on a missing or implausible address, an
    /// over-long name, a title or company CRM refuses, an owner that is not a
    /// user of the tenant, or a malformed seed; [`StoreError::Db`] on failure.
    pub async fn capture(
        &self,
        seed: &PipelineSeed,
        lead: &ConversationLead,
    ) -> Result<CapturedLead> {
        let email = normalize_visitor_email(&lead.visitor_email)?;
        let visitor_name = lead.visitor_name.trim();
        if visitor_name.chars().count() > LEAD_VISITOR_NAME_MAX_CHARS {
            return Err(StoreError::Validation(format!(
                "name must be at most {LEAD_VISITOR_NAME_MAX_CHARS} characters"
            )));
        }
        if let Some(known) = self.already_known(&email).await? {
            return Ok(known);
        }
        // Normalised before the transaction, never inside it — CRM's own rule
        // for its own writer — and before the landing place, so a pair that
        // does not hold seeds no board on its way to being refused. Naming the
        // owner explicitly (rather than leaning on the door's default) is what
        // makes normalize_deal prove the owner is a user of this tenant before
        // a row exists.
        let normalized = self
            .account
            .normalize_deal(&NewDeal {
                title: lead.title.clone(),
                company_name: lead.company_name.clone(),
                contact_name: visitor_name.to_owned(),
                contact_email: email,
                value_cents: 0,
                owner_user_id: Some(self.account.user.as_str().to_owned()),
                source: lead.source.clone(),
                ..Default::default()
            })
            .await?;
        let (pipeline, stage) = self.landing_place(seed).await?;
        let mut tx = self.account.pool.begin().await.map_err(StoreError::Db)?;
        // The board is held shared exactly as one card move holds it: the
        // capture may not slip past a column being archived.
        self.account.share_crm_pipeline(&mut tx, &pipeline).await?;
        let deal = self
            .account
            .insert_crm_deal_in(&mut tx, &pipeline, &stage, &normalized)
            .await?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(CapturedLead::Created(deal))
    }

    /// Whether the tenant already knows this address, by the lead import's own
    /// rules: the exact address, then — outside free mail — its company
    /// domain, each looked for on **open** deals first and then on customers.
    /// A closed deal is history, and history must not make tomorrow's lead a
    /// duplicate.
    async fn already_known(&self, email: &str) -> Result<Option<CapturedLead>> {
        let lower = email.to_ascii_lowercase();
        let open_deals: Vec<(String, String)> = sqlx::query_as(
            "SELECT id, lower(contact_email) FROM crm_deals \
             WHERE tenant_id = $1 AND contact_email <> '' AND outcome IS NULL \
             ORDER BY created_at DESC, id",
        )
        .bind(self.account.tenant.as_str())
        .fetch_all(&self.account.pool)
        .await
        .map_err(StoreError::Db)?;
        if let Some((id, _)) = open_deals.iter().find(|(_, address)| *address == lower) {
            return Ok(Some(CapturedLead::AlreadyKnown(CrmDealId::new(id.clone()))));
        }
        let customer_addresses: Vec<(String,)> = sqlx::query_as(
            "SELECT lower(email) FROM billing_customers WHERE tenant_id = $1 AND email <> ''",
        )
        .bind(self.account.tenant.as_str())
        .fetch_all(&self.account.pool)
        .await
        .map_err(StoreError::Db)?;
        if customer_addresses
            .iter()
            .any(|(address,)| *address == lower)
        {
            return Ok(Some(CapturedLead::AlreadyCustomer));
        }
        let Some(domain) = domain_of(&lower) else {
            return Ok(None);
        };
        if is_free_mail_domain(domain) {
            return Ok(None);
        }
        if let Some((id, _)) = open_deals
            .iter()
            .find(|(_, address)| domain_of(address) == Some(domain))
        {
            return Ok(Some(CapturedLead::AlreadyKnown(CrmDealId::new(id.clone()))));
        }
        if customer_addresses
            .iter()
            .any(|(address,)| domain_of(address) == Some(domain))
        {
            return Ok(Some(CapturedLead::AlreadyCustomer));
        }
        Ok(None)
    }

    /// Where a new lead lands: the tenant's first active board — seeded on
    /// first use exactly as the CRM screen seeds it — and its first live
    /// column, the same default the lead import resolves when nobody names a
    /// column.
    async fn landing_place(&self, seed: &PipelineSeed) -> Result<(CrmPipelineId, CrmStageId)> {
        let pipelines = self.account.crm_pipelines_or_seed(seed).await?;
        let pipeline = pipelines
            .first()
            .map(|p| p.id.clone())
            // Unreachable while the seed is well-formed: seeding refuses an
            // empty board, and an existing tenant has at least what it kept.
            .ok_or_else(|| {
                StoreError::Validation("this workspace has no board to land a lead on".to_owned())
            })?;
        let first: Option<String> = sqlx::query_scalar(
            "SELECT id FROM crm_stages \
             WHERE tenant_id = $1 AND pipeline_id = $2 AND archived_at IS NULL \
             ORDER BY position, created_at, id LIMIT 1",
        )
        .bind(self.account.tenant.as_str())
        .bind(pipeline.as_str())
        .fetch_optional(&self.account.pool)
        .await
        .map_err(StoreError::Db)?;
        let stage = first.map(CrmStageId::new).ok_or_else(|| {
            StoreError::Validation("this board has no stage to land a lead in".to_owned())
        })?;
        Ok((pipeline, stage))
    }
}

/// Holds a stranger's address to the same rule the public form door holds it
/// to: present, plausibly shaped, and no larger than an address can be.
fn normalize_visitor_email(value: &str) -> Result<String> {
    let email = value.trim();
    if email.is_empty() {
        return Err(StoreError::Validation("email must not be empty".to_owned()));
    }
    if email.chars().count() > LEAD_VISITOR_EMAIL_MAX_CHARS {
        return Err(StoreError::Validation(format!(
            "email must be at most {LEAD_VISITOR_EMAIL_MAX_CHARS} characters"
        )));
    }
    let looks_like_address = matches!(
        email.split_once('@'),
        Some((local, domain)) if !local.is_empty() && !domain.is_empty()
    );
    if !looks_like_address || email.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(StoreError::Validation(
            "email must be a valid address".to_owned(),
        ));
    }
    Ok(email.to_owned())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn refused(value: &str, rule: &str) {
        match normalize_visitor_email(value) {
            Err(StoreError::Validation(msg)) => {
                assert!(msg.contains(rule), "expected {rule:?} in {msg:?}");
            }
            other => panic!("expected Validation({rule:?}), got: {other:?}"),
        }
    }

    #[test]
    fn a_plausible_address_passes_trimmed() {
        assert_eq!(
            normalize_visitor_email("  visitor@example.test  ").unwrap(),
            "visitor@example.test"
        );
    }

    #[test]
    fn a_missing_address_is_named() {
        refused("", "email must not be empty");
        refused("   ", "email must not be empty");
    }

    #[test]
    fn an_implausible_address_is_refused() {
        refused("not-an-address", "valid address");
        refused("@example.test", "valid address");
        refused("visitor@", "valid address");
        refused("two words@example.test", "valid address");
    }

    #[test]
    fn an_oversized_address_is_refused() {
        let long = format!("{}@example.test", "a".repeat(LEAD_VISITOR_EMAIL_MAX_CHARS));
        refused(&long, "at most");
    }
}
