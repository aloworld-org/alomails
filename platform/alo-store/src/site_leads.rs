//! The handoff from a website enquiry to a sales opportunity (ADR 0036,
//! S2.10b) — the write half of the Sites → CRM seam.
//!
//! A contact-form submission is a stranger's message in an inbox
//! ([`crate::site_forms`]); a deal is a company's opportunity on a board
//! ([`crate::crm_deals`]). This module is the one place that says the second
//! came from the first, and it is deliberately the *only* new fact stored:
//! everything the funnel later reports about the opportunity is read from
//! CRM's and Billing's own rows ([`crate::site_attribution`]), never copied
//! here, because a copied deal value is wrong the moment somebody edits the
//! deal.
//!
//! Three rules hold, and each is enforced against the database rather than
//! trusted:
//!
//! - **Nothing crosses a tenant.** Every statement scopes by tenant *and*
//!   site, and every foreign key is composite, so a guessed submission id or
//!   deal id from a neighbour cannot be linked even if a `WHERE` clause were
//!   wrong — the row cannot exist.
//! - **One enquiry becomes at most one lead.** A second link on the same
//!   submission is a [`StoreError::Conflict`] naming the rule; re-linking the
//!   *same* deal answers the existing link, so a double click is not an error
//!   and never a twin opportunity.
//! - **CRM writes the deal, not this file.** The create path normalises and
//!   inserts through [`AccountStore::insert_crm_deal_in`] — the same writer
//!   the board and the lead import use — inside one transaction with the link,
//!   so a handoff either produces both rows or neither. CRM's own module is
//!   used, never edited or reimplemented.
//!
//! The words on the card are the caller's. The store writes names it is
//! handed and invents none (`docs/design/crm.md` § Seeding), so the title of a
//! handed-off lead comes from the edge, in the language the person asked for.
//! The two facts this module does supply are facts, not words: the enquirer's
//! own name and address, and — when the caller states no source — the site's
//! subdomain, which is where the enquiry demonstrably came from.

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::crm_deals::{DealState, NewDeal};
use crate::error::{Result, StoreError};
use crate::id::{
    CrmDealId, CrmPipelineId, CrmStageId, SiteFormId, SiteFormSubmissionId, SiteId, SiteLeadLinkId,
};
use crate::site_public_conversions::ConversionSource;

/// The opportunity a link names, as much of it as a submissions list needs to
/// show "this one became a deal, and here is how it is doing".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteLeadDeal {
    pub id: CrmDealId,
    pub title: String,
    /// What the opportunity is worth, in integer cents of `currency`.
    pub value_cents: i64,
    /// ISO 4217, uppercase — never converted, never summed across codes.
    pub currency: String,
    /// Where the deal stands right now, read from CRM at query time.
    pub state: DealState,
}

/// One handoff: the submission, the conversion point it came through, and the
/// opportunity it became.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteLeadLink {
    pub id: SiteLeadLinkId,
    pub site_id: SiteId,
    /// The conversion point's kind, in the vocabulary the aggregate counters
    /// use (`form` today).
    pub source_kind: String,
    /// The conversion point's site-owned id — a [`SiteFormId`] today.
    pub source_id: String,
    pub submission_id: SiteFormSubmissionId,
    /// The user who made the handoff. A handoff is a person's decision.
    pub linked_by: String,
    pub linked_at: OffsetDateTime,
    pub deal: SiteLeadDeal,
}

/// What the caller adds to a submission to make it an opportunity. The
/// enquirer's name and address come from the submission itself and are not
/// re-entered here.
#[derive(Debug, Clone, Default)]
pub struct SiteLeadDraft {
    /// The line on the card. Required and validated by CRM's own gate — the
    /// store writes titles it is handed and invents none.
    pub title: String,
    /// The company, when the enquirer named one. A contact form does not ask,
    /// so this is usually blank and the deal carries the person's name.
    pub company_name: String,
    /// What the opportunity is thought to be worth, in integer cents.
    pub value_cents: i64,
    /// ISO 4217 code, or blank for the tenant's default.
    pub currency: String,
    /// Whose deal it is, or `None` for the acting user.
    pub owner_user_id: Option<String>,
    /// Where the opportunity came from, in the tenant's own vocabulary. Blank
    /// falls back to the site's subdomain — a fact about the enquiry, not a
    /// word this store made up.
    pub source: String,
}

/// The submission a handoff is about, resolved inside the tenant.
struct ResolvedSubmission {
    form_id: SiteFormId,
    sender_name: String,
    sender_email: String,
}

#[derive(sqlx::FromRow)]
struct LinkRow {
    id: String,
    site_id: String,
    source_kind: String,
    source_id: String,
    submission_id: String,
    linked_by: String,
    linked_at: OffsetDateTime,
    deal_id: String,
    title: String,
    value_cents: i64,
    currency: String,
    state: String,
}

impl LinkRow {
    fn into_link(self) -> SiteLeadLink {
        SiteLeadLink {
            id: SiteLeadLinkId::new(self.id),
            site_id: SiteId::new(self.site_id),
            source_kind: self.source_kind,
            source_id: self.source_id,
            submission_id: SiteFormSubmissionId::new(self.submission_id),
            linked_by: self.linked_by,
            linked_at: self.linked_at,
            deal: SiteLeadDeal {
                id: CrmDealId::new(self.deal_id),
                title: self.title,
                value_cents: self.value_cents,
                currency: self.currency,
                // A stored outcome outside the column's check constraint is
                // impossible; an unreadable one reads as still being worked
                // rather than as a failed request.
                state: DealState::parse(&self.state).unwrap_or(DealState::Open),
            },
        }
    }
}

/// Every column a link read selects, in [`LinkRow`] order.
const LINK_COLS: &str = "a.id, a.site_id, a.source_kind, a.source_id, a.submission_id, \
     a.linked_by, a.linked_at, d.id AS deal_id, d.title, d.value_cents, d.currency, \
     COALESCE(d.outcome, 'open') AS state";

/// The message a second, different lead on one enquiry is refused with.
const ONE_LEAD_PER_SUBMISSION: &str = "this enquiry has already been handed to an opportunity";

impl AccountStore {
    /// Hands an existing opportunity the enquiry it came from.
    ///
    /// Idempotent on the pair: linking the same deal to the same submission
    /// again answers the link that is already there.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the site, the submission or the deal is
    /// not this tenant's, or the submission is not on that site;
    /// [`StoreError::Conflict`] when the enquiry already became a different
    /// opportunity; [`StoreError::Db`] on failure.
    pub async fn link_site_lead(
        &self,
        site: &SiteId,
        submission: &SiteFormSubmissionId,
        deal: &CrmDealId,
    ) -> Result<SiteLeadLink> {
        let resolved = self.resolve_submission(site, submission).await?;
        // The deal is read through CRM's own tenant-scoped table; a
        // neighbour's id is as absent as one that never existed.
        let owns_deal = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM crm_deals WHERE tenant_id = $1 AND id = $2)",
        )
        .bind(self.tenant.as_str())
        .bind(deal.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if !owns_deal {
            return Err(StoreError::NotFound);
        }
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let id = self
            .insert_link_in(&mut tx, site, submission, &resolved.form_id, deal)
            .await?;
        tx.commit().await.map_err(StoreError::Db)?;
        self.site_lead_link(site, &id)
            .await?
            .ok_or(StoreError::NotFound)
    }

    /// Raises a new opportunity **from** an enquiry and links the two in one
    /// transaction, so a handoff produces both rows or neither.
    ///
    /// The card is created by CRM's own writer, on the board and in the column
    /// the caller named, carrying the enquirer's own name and address — never
    /// re-typed, which is the point of a handoff.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the site, the submission, the board or
    /// the column is not this tenant's; [`StoreError::Validation`] on a title,
    /// value, currency or owner CRM refuses; [`StoreError::Conflict`] when the
    /// enquiry already became an opportunity; [`StoreError::Db`] on failure.
    pub async fn create_site_lead(
        &self,
        site: &SiteId,
        submission: &SiteFormSubmissionId,
        pipeline: &CrmPipelineId,
        stage: &CrmStageId,
        draft: &SiteLeadDraft,
    ) -> Result<SiteLeadLink> {
        let subdomain = self.site_subdomain(site).await?;
        let resolved = self.resolve_submission(site, submission).await?;
        self.refuse_second_lead(submission).await?;

        let source = if draft.source.trim().is_empty() {
            subdomain
        } else {
            draft.source.clone()
        };
        // Normalised before the transaction, never inside it — CRM's own rule
        // for its own writer.
        let normalized = self
            .normalize_deal(&NewDeal {
                title: draft.title.clone(),
                company_name: draft.company_name.clone(),
                contact_name: resolved.sender_name,
                contact_email: resolved.sender_email,
                value_cents: draft.value_cents,
                currency: if draft.currency.trim().is_empty() {
                    crate::billing_field::DEFAULT_CURRENCY.to_owned()
                } else {
                    draft.currency.clone()
                },
                owner_user_id: draft.owner_user_id.clone(),
                source,
                ..Default::default()
            })
            .await?;

        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        self.share_crm_pipeline(&mut tx, pipeline).await?;
        let deal = self
            .insert_crm_deal_in(&mut tx, pipeline, stage, &normalized)
            .await?;
        let id = self
            .insert_link_in(&mut tx, site, submission, &resolved.form_id, &deal)
            .await?;
        tx.commit().await.map_err(StoreError::Db)?;
        self.site_lead_link(site, &id)
            .await?
            .ok_or(StoreError::NotFound)
    }

    /// Unclaims an opportunity for the website. The deal is CRM's and is left
    /// exactly as it is; only the claim that a form produced it is removed.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the link is not this tenant's or not on
    /// that site; [`StoreError::Db`] on failure.
    pub async fn unlink_site_lead(&self, site: &SiteId, link: &SiteLeadLinkId) -> Result<()> {
        let done = sqlx::query(
            "DELETE FROM site_lead_attribution \
             WHERE tenant_id = $1 AND site_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(link.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Every handoff made on a site, newest first — the read the submissions
    /// list uses to say which enquiries have already been dealt with.
    ///
    /// Empty for a site that is not this tenant's, which is the same answer as
    /// a site nobody has handed anything off from.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn site_lead_links(&self, site: &SiteId) -> Result<Vec<SiteLeadLink>> {
        let rows = sqlx::query_as::<_, LinkRow>(&format!(
            "SELECT {LINK_COLS} FROM site_lead_attribution a \
             JOIN crm_deals d ON d.tenant_id = a.tenant_id AND d.id = a.deal_id \
             WHERE a.tenant_id = $1 AND a.site_id = $2 \
             ORDER BY a.linked_at DESC, a.id DESC"
        ))
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(LinkRow::into_link).collect())
    }

    /// One handoff, or `None` when it is not this tenant's or not on that
    /// site.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn site_lead_link(
        &self,
        site: &SiteId,
        link: &SiteLeadLinkId,
    ) -> Result<Option<SiteLeadLink>> {
        let row = sqlx::query_as::<_, LinkRow>(&format!(
            "SELECT {LINK_COLS} FROM site_lead_attribution a \
             JOIN crm_deals d ON d.tenant_id = a.tenant_id AND d.id = a.deal_id \
             WHERE a.tenant_id = $1 AND a.site_id = $2 AND a.id = $3"
        ))
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(link.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(row.map(LinkRow::into_link))
    }

    /// The site's subdomain, and the proof the site is this tenant's.
    async fn site_subdomain(&self, site: &SiteId) -> Result<String> {
        sqlx::query_scalar::<_, String>(
            "SELECT subdomain FROM sites WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?
        .ok_or(StoreError::NotFound)
    }

    /// Resolves a submission to the form it was posted to, proving in one
    /// statement that both belong to this tenant *and* to this site.
    async fn resolve_submission(
        &self,
        site: &SiteId,
        submission: &SiteFormSubmissionId,
    ) -> Result<ResolvedSubmission> {
        let row = sqlx::query_as::<_, (String, String, String)>(
            "SELECT s.form_id, s.sender_name, s.sender_email \
             FROM site_form_submissions s \
             JOIN site_forms f ON f.tenant_id = s.tenant_id AND f.id = s.form_id \
             WHERE s.tenant_id = $1 AND f.site_id = $2 AND s.id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(submission.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?
        .ok_or(StoreError::NotFound)?;
        Ok(ResolvedSubmission {
            form_id: SiteFormId::new(row.0),
            sender_name: row.1,
            sender_email: row.2,
        })
    }

    /// Refuses the second lead on one enquiry before any opportunity is
    /// raised, so a refused handoff leaves no orphan card behind. The unique
    /// constraint is still the authority — this is the early, cheap read.
    async fn refuse_second_lead(&self, submission: &SiteFormSubmissionId) -> Result<()> {
        let taken = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM site_lead_attribution \
             WHERE tenant_id = $1 AND submission_id = $2)",
        )
        .bind(self.tenant.as_str())
        .bind(submission.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if taken {
            return Err(StoreError::Conflict(ONE_LEAD_PER_SUBMISSION.to_owned()));
        }
        Ok(())
    }

    /// Writes the link inside a transaction the caller owns. An existing link
    /// to the same deal answers that link's id; one to a different deal is the
    /// conflict this table exists to make impossible.
    async fn insert_link_in(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        site: &SiteId,
        submission: &SiteFormSubmissionId,
        form: &SiteFormId,
        deal: &CrmDealId,
    ) -> Result<SiteLeadLinkId> {
        let id = SiteLeadLinkId::generate();
        let written = sqlx::query_scalar::<_, String>(
            "INSERT INTO site_lead_attribution \
                 (tenant_id, id, site_id, source_kind, source_id, submission_id, deal_id, \
                  linked_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
             ON CONFLICT (tenant_id, submission_id) DO NOTHING \
             RETURNING id",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(site.as_str())
        .bind(ConversionSource::Form.as_str())
        .bind(form.as_str())
        .bind(submission.as_str())
        .bind(deal.as_str())
        .bind(self.user.as_str())
        .fetch_optional(&mut **tx)
        .await
        .map_err(StoreError::Db)?;
        if let Some(written) = written {
            return Ok(SiteLeadLinkId::new(written));
        }
        // Somebody linked this enquiry first. The same deal is the same
        // decision made twice and answers that link; a different one is the
        // rule this table enforces.
        let existing = sqlx::query_as::<_, (String, String)>(
            "SELECT id, deal_id FROM site_lead_attribution \
             WHERE tenant_id = $1 AND submission_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(submission.as_str())
        .fetch_optional(&mut **tx)
        .await
        .map_err(StoreError::Db)?
        .ok_or(StoreError::NotFound)?;
        if existing.1 == deal.as_str() {
            Ok(SiteLeadLinkId::new(existing.0))
        } else {
            Err(StoreError::Conflict(ONE_LEAD_PER_SUBMISSION.to_owned()))
        }
    }
}
