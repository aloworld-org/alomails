//! One recipient's copy of a campaign, compiled at send time (alo Campaigns,
//! ADR 0044, wave C4.2).
//!
//! Queue item C4.2: *rendering happens per recipient at send time, and a render
//! failure suppresses that one recipient rather than failing the campaign.*
//!
//! [`campaign_send`](crate::campaign_send) wrote down **who** is to be mailed.
//! This is what turns one of those rows into bytes: their own merge values,
//! their own unsubscribe token, their own `multipart/alternative`.
//!
//! ## It prepares; it does not send
//!
//! Nothing here opens a socket. This crate is the store, and a store that
//! reached the network would be a store nobody could test without one. What it
//! returns is a [`PreparedCampaignMessage`] — the entity plus the headers a
//! sender must write — and the sender puts the envelope round it.
//!
//! **The caller marks a recipient sent *before* handing them to submission**,
//! not after, and that ordering is the ledger's whole promise rather than a
//! preference. [`mark_campaign_recipient_sent`](crate::AccountStore::mark_campaign_recipient_sent)
//! moves only a `pending` row, so it is the point at which two dispatchers
//! racing for the same recipient are resolved: the loser is told `false` and
//! must not submit. Marking afterwards would make that check useless — both
//! would have already sent. The cost is that a crash between the mark and the
//! submission loses one message rather than duplicating it, which is the trade
//! C4.1 asks for in as many words: *nobody is mailed twice.*
//!
//! ## Why the render is per recipient rather than once per campaign
//!
//! Because both of the things wrapped around the letter are personal. The merge
//! values are theirs ([`campaign_merge`](crate::campaign_merge)), and so is the
//! unsubscribe token — RFC 8058 §7 requires an unguessable per-recipient URI,
//! and a campaign rendered once and mailed to everybody would carry one link
//! that unsubscribes whoever clicks it. Rendering once would be faster and
//! would be a bug in the one place a bug cannot be recalled.
//!
//! ## A render failure is one recipient, never the campaign
//!
//! A letter that will not compile for *this* person — a merge value carrying
//! something the renderer refuses, a body that no longer passes the write gate
//! — marks that recipient `failed` with the reason and the pass continues. The
//! alternative is a send that stops on its four-hundredth recipient and leaves
//! an operator with no way to tell which of the remaining nine thousand were
//! reached. The failure is recorded per recipient because that is the only
//! shape in which it is answerable afterwards.

use crate::account::AccountStore;
use crate::campaign_merge::{CampaignMergeValues, personalise_campaign};
use crate::campaign_mime::{CampaignMessage, render_campaign_message};
use crate::campaign_send::{RecipientState, SendState};
use crate::campaign_unsubscribe::NewUnsubscribeToken;
use crate::campaign_unsubscribe_link::UnsubscribeInvitation;
use crate::campaign_warm_up::SendAllowance;
use crate::error::{Result, StoreError};
use crate::id::CampaignSendId;
use crate::store::Store;

/// Why a recipient could not be rendered. Short codes, for the reason
/// [`crate::campaign_send::reason`] gives: a tally groups by them, and the
/// sentence a person reads is the interface's.
pub mod reason {
    /// The letter would not compile for this recipient.
    pub const RENDER_REFUSED: &str = "render_refused";
    /// They are no longer somebody this tenant may mail — suppressed, or their
    /// consent record went away — between enrolment and this pass.
    pub const NO_LONGER_MAILABLE: &str = "no_longer_mailable";
}

/// What the sender needs in order to build one recipient's URLs.
///
/// Both halves come from outside this crate and neither can be guessed here:
/// the origin is deployment configuration, and the words are the audience's
/// language, which a store has no notion of.
#[derive(Debug, Clone, Copy)]
pub struct DispatchLinks<'a> {
    /// The public origin the recipient's links are built on, e.g.
    /// `https://mail.alomails.com`. A trailing slash is tolerated.
    pub base_url: &'a str,
    /// The words of the visible link, in the audience's language.
    pub link_text: &'a str,
}

/// One recipient's letter, ready for an envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedCampaignMessage {
    /// Who it is for, normalised.
    pub address: String,
    /// The subject, personalised for them — the most consequential string in
    /// the letter, and the one a filter scores.
    pub subject: String,
    /// The `multipart/alternative` entity, and the `List-Unsubscribe` pair the
    /// sender must write around it.
    pub message: CampaignMessage,
}

/// What one pass prepared, and what it wrote off.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchPass {
    /// Letters ready to send, in the order the recipients were enrolled.
    pub prepared: Vec<PreparedCampaignMessage>,
    /// Recipients marked `failed` by this pass, each with a reason recorded on
    /// their row. Counted rather than returned: the row is the record, and a
    /// caller that had to remember them would be a second place to look.
    pub failed: i64,
    /// What the warm-up allowed today, as it stood when the pass began.
    ///
    /// Returned rather than left for the caller to ask again, because it is the
    /// answer to the question a caller has after a short pass: *why did I get
    /// twelve when I asked for five hundred?* C2.3 requires the cap and its
    /// reason to be visible in the send flow, and a number a screen has to
    /// derive is a number two screens will derive differently.
    pub allowance: SendAllowance,
}

impl AccountStore {
    /// Compiles the next `limit` pending recipients of a send.
    ///
    /// Each gets their own merge values, their own unsubscribe token, and their
    /// own rendered message. A recipient whose letter will not compile is
    /// marked `failed` with the reason and the pass continues — see the module
    /// docs.
    ///
    /// Nothing is marked `sent` here: that is the caller's, immediately before
    /// it submits, and it is what resolves two dispatchers racing for one
    /// recipient.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the send is not this tenant's;
    /// [`StoreError::Conflict`] when the send is not in a state that may
    /// dispatch — enrolling, stopped or done;
    /// [`StoreError::Validation`] when `limit` is outside `1..=`[`BATCH_MAX`],
    /// or the links are not usable; [`StoreError::Db`] on failure.
    pub async fn prepare_campaign_send_batch(
        &self,
        id: &CampaignSendId,
        limit: i64,
        links: &DispatchLinks<'_>,
    ) -> Result<DispatchPass> {
        if !(1..=BATCH_MAX).contains(&limit) {
            return Err(StoreError::Validation(format!(
                "a dispatch pass takes between 1 and {BATCH_MAX} recipients"
            )));
        }
        let send = self.campaign_send(id).await?.ok_or(StoreError::NotFound)?;
        // `Paused` is a conflict as much as `Stopped` is: an operator who
        // pressed pause has said stop handing people to submission, and a
        // dispatcher that kept preparing would be racing their decision.
        if send.state != SendState::Sending {
            return Err(StoreError::Conflict(format!(
                "this send is {}, so it is not handing anybody to submission",
                send.state.as_str()
            )));
        }
        let campaign = self
            .campaign(&send.campaign_id)
            .await?
            .ok_or(StoreError::NotFound)?;

        // The warm-up ceiling (C2.3) is the identity's, not this campaign's, so
        // it is spent across every send the tenant is running today. Clamping
        // the pass rather than refusing it is deliberate: a caller asking for
        // 500 on a day with 12 left should get 12 and be told, not an error it
        // has to interpret before it can make progress.
        let allowance = self.campaign_send_allowance().await?;
        if allowance.is_exhausted() {
            return Ok(DispatchPass {
                prepared: Vec::new(),
                failed: 0,
                allowance,
            });
        }
        let limit = limit.min(allowance.remaining);

        let addresses = self.campaign_send_pending(id, limit).await?;
        let mut pass = DispatchPass {
            prepared: Vec::new(),
            failed: 0,
            allowance: allowance.clone(),
        };
        for address in addresses {
            match self.prepare_one(&campaign, &send.id, &address, links).await {
                Ok(prepared) => pass.prepared.push(prepared),
                // A refusal is this recipient's, recorded on their row. Only a
                // database failure stops the pass — everything else is a fact
                // about one letter.
                Err(StoreError::Db(err)) => return Err(StoreError::Db(err)),
                Err(other) => {
                    let why = match other {
                        StoreError::NotFound => reason::NO_LONGER_MAILABLE,
                        _ => reason::RENDER_REFUSED,
                    };
                    self.mark_campaign_recipient_failed(id, &address, why)
                        .await?;
                    pass.failed += 1;
                }
            }
        }
        Ok(pass)
    }

    /// One recipient's letter, or the refusal that belongs on their row.
    async fn prepare_one(
        &self,
        campaign: &crate::campaign_record::Campaign,
        send_id: &CampaignSendId,
        address: &str,
        links: &DispatchLinks<'_>,
    ) -> Result<PreparedCampaignMessage> {
        // Read at `Reach::Mailable`, so consent and suppression are applied in
        // SQL **again, now** rather than trusted from enrolment. Somebody who
        // unsubscribed since the send opened is `NotFound` here and is written
        // off rather than mailed — which is C2.9's promise, kept at the last
        // moment it can be kept.
        let recipient = self
            .campaign_recipient(address)
            .await?
            .ok_or(StoreError::NotFound)?;

        // Their own token. Minted per recipient because RFC 8058 §7 requires an
        // unguessable URI: one link shared across an audience unsubscribes
        // whoever clicks it, including somebody forwarding the mail.
        let issued = Store::tenant_scope(self)
            .mint_campaign_unsubscribe_token(&NewUnsubscribeToken {
                send_ref: send_id.as_str(),
                address: &recipient.address,
                topic: Some(campaign.topic.as_str()),
            })
            .await?;

        let base = links.base_url.trim_end_matches('/');
        let invitation = UnsubscribeInvitation {
            // The endpoint a client POSTs, and the page a person opens. Two
            // routes doing two jobs — see `UnsubscribeInvitation::page_url`.
            one_click_url: format!("{base}/jmap/campaign-unsubscribe/{}", issued.token),
            page_url: format!("{base}/unsubscribe/{}", issued.token),
            topic: Some(campaign.topic.clone()),
            link_text: links.link_text.to_owned(),
        };

        let letter = crate::campaign_html::CampaignLetter {
            subject: &campaign.subject,
            preheader: campaign.preheader.as_deref(),
            content: &campaign.content,
            unsubscribe: &invitation,
        };
        let personalised =
            personalise_campaign(&letter, &CampaignMergeValues::for_recipient(&recipient))?;
        let message = render_campaign_message(&personalised.letter(&invitation))?;

        Ok(PreparedCampaignMessage {
            address: recipient.address,
            subject: personalised.subject.clone(),
            message,
        })
    }

    /// The next pending recipients of a send, oldest enrolled first.
    ///
    /// Served by `campaign_send_recipients_pending`, which is partial on
    /// exactly this predicate.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn campaign_send_pending(
        &self,
        id: &CampaignSendId,
        limit: i64,
    ) -> Result<Vec<String>> {
        let rows: Vec<(String,)> = sqlx::query_as(PENDING_SQL)
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(|row| row.0).collect())
    }

    /// Records that one recipient could not be sent to, and why.
    ///
    /// Moves only a `pending` row, exactly as
    /// [`mark_campaign_recipient_sent`](Self::mark_campaign_recipient_sent)
    /// does: a recipient already sent to must never be rewritten as failed by a
    /// retry, because the mail has gone and the row is the only record that it
    /// did.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the send is not this tenant's;
    /// [`StoreError::Validation`] when the address or the reason is not one a
    /// row can hold; [`StoreError::Db`] on failure.
    pub async fn mark_campaign_recipient_failed(
        &self,
        id: &CampaignSendId,
        address: &str,
        why: &str,
    ) -> Result<bool> {
        let address = crate::campaign_audience::normalise_address(address).ok_or_else(|| {
            StoreError::Validation("a recipient is named by an address".to_owned())
        })?;
        let why = why.trim();
        if why.is_empty() || why.chars().count() > REASON_MAX {
            return Err(StoreError::Validation(format!(
                "a failure reason is between 1 and {REASON_MAX} characters"
            )));
        }
        self.campaign_send(id).await?.ok_or(StoreError::NotFound)?;

        let moved = sqlx::query(MARK_FAILED_SQL)
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .bind(&address)
            .bind(RecipientState::Failed.as_str())
            .bind(RecipientState::Pending.as_str())
            .bind(why)
            .execute(&self.pool)
            .await
            .map_err(StoreError::Db)?;
        Ok(moved.rows_affected() > 0)
    }
}

/// The most recipients one pass will compile.
///
/// A bound rather than a policy: pacing is C4.3's, and this only stops a caller
/// asking for the whole audience in one allocation. Each prepared message holds
/// a rendered HTML and text part, so a pass of ten thousand is tens of
/// megabytes of letters nobody has sent yet.
///
/// That it is *enforced* is held by
/// `tests/campaign_dispatch_tenancy.rs::a_batch_is_bounded_and_the_bound_is_the_callers_error`,
/// against the real call. A unit test asserting the constant is inside its own
/// range would be a tautology the compiler already knows.
pub const BATCH_MAX: i64 = 500;

/// Matches the `reason` column's own `CHECK` in migration 0800.
const REASON_MAX: usize = 60;

const PENDING_SQL: &str = "SELECT address FROM campaign_send_recipients \
     WHERE tenant_id = $1 AND send_id = $2 AND state = 'pending' \
     ORDER BY enrolled_at, address LIMIT $3";

const MARK_FAILED_SQL: &str = "UPDATE campaign_send_recipients \
     SET state = $4, reason = $6, settled_at = now() \
     WHERE tenant_id = $1 AND send_id = $2 AND address = $3 AND state = $5";

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn the_failure_reasons_fit_what_a_row_can_hold() {
        for why in [reason::RENDER_REFUSED, reason::NO_LONGER_MAILABLE] {
            assert!(!why.is_empty());
            assert!(why.chars().count() <= REASON_MAX, "{why} is too long");
        }
    }

    #[test]
    fn a_pass_that_prepared_nothing_still_says_why() {
        // The shape a caller meets when the day's ceiling is spent: no letters,
        // no failures, and the allowance that explains both.
        let pass = DispatchPass {
            prepared: Vec::new(),
            failed: 0,
            allowance: SendAllowance {
                day: 1,
                ceiling: 5,
                sent_today: 5,
                remaining: 0,
            },
        };
        assert!(pass.prepared.is_empty());
        assert_eq!(pass.failed, 0);
        assert!(
            pass.allowance.is_exhausted(),
            "an empty pass without the reason is a caller left guessing"
        );
    }
}
