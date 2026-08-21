//! The letter as one person will receive it (alo Campaigns, ADR 0044, wave
//! C3.6) — the preview, and the only honest thing it can claim to be.
//!
//! Queue item C3.6: *preview and seed test send within the tenant — the
//! rendered HTML, the text part, and the merge fields resolved against a real
//! record.*
//!
//! Everything above this module compiles a letter ([`campaign_html`], with
//! [`campaign_text`] and [`campaign_mime`] beside it) and resolves a
//! personalisation ([`campaign_merge`]). None of them knows a recipient exists.
//! This is where a stored campaign meets a stored person, and it holds three
//! decisions.
//!
//! ## A preview is against a real record, or it says it is not
//!
//! The tempting shape is a sample recipient — a *Jane Doe, Belgium* the module
//! invents so the screen always has something to draw. It is exactly the
//! failure C3.4 was written to prevent, one layer up: an invented record makes
//! every preview look personalised, including the previews of the letters that
//! will arrive at half an audience as *"Hi ,"*. So there is no sample. A
//! preview resolves against somebody this tenant actually holds
//! ([`PreviewAgainst::Recipient`]) or against **nobody**
//! ([`PreviewAgainst::Fallbacks`]), and the second says so in the value the
//! screen draws rather than in a comment.
//!
//! Resolving against nobody is not a degraded mode, and that is why it is
//! offered by name rather than only reached when the audience is empty: it is
//! the copy of the letter that goes to every recipient with nothing recorded,
//! which on an audience built from web forms is most of them. A writer who has
//! read only the personalised preview has not read the mail most people get.
//!
//! ## The person previewed is a person we could actually mail
//!
//! [`AccountStore::campaign_recipient`] reads at `Reach::Mailable`, so consent
//! and suppression apply in SQL to a preview exactly as they will to a send.
//! Previewing "as" somebody who unsubscribed is refused — not because a preview
//! would reach them (it reaches nobody), but because it is the one operation
//! that ends with a colleague looking at a rendered letter addressed to a person
//! the tenant has promised never to mail again, and deciding to send it. The
//! suppression is absolute (ADR 0044 §2) or it is a filter somebody remembers.
//!
//! ## Nothing here sends, and the seed test is a draft
//!
//! There is no send in this module and none above it. What the HTTP edge does
//! with a preview — writing it into the caller's own Drafts, for the caller to
//! read in their own mail client and send themselves through the ordinary
//! audited submission path — is the same thing `alo-jmap`'s `billing_send` does
//! with an invoice, and it is not a campaign send: one message, to the colleague
//! who asked, on the transactional identity. The campaign send waits on the
//! second IP ADR 0044 §1 requires, which is a purchase.
//!
//! [`campaign_html`]: crate::campaign_html
//! [`campaign_text`]: crate::campaign_text
//! [`campaign_mime`]: crate::campaign_mime
//! [`campaign_merge`]: crate::campaign_merge

use crate::account::AccountStore;
use crate::campaign_audience::AudiencePage;
use crate::campaign_html::CampaignLetter;
use crate::campaign_merge::{CampaignMergeValues, ResolvedMergeField, personalise_campaign};
use crate::campaign_mime::render_campaign_message;
use crate::campaign_unsubscribe_link::UnsubscribeInvitation;
use crate::error::{Result, StoreError};
use crate::id::CampaignId;

/// Whose copy of the letter to render.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewAs {
    /// The first person, in address order, this tenant may mail — so that
    /// opening a preview shows a real record without anybody having to pick
    /// one. Falls through to [`Self::Fallbacks`] when there is nobody yet, and
    /// the answer says which it did.
    AnyRecipient,
    /// This address, which must be somebody this tenant may mail.
    Recipient(String),
    /// Nobody: every merge field prints the writer's fallback. The copy that
    /// goes to every recipient with nothing recorded — see the module docs.
    Fallbacks,
}

/// Why a preview resolved against nobody.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FallbackReason {
    /// The caller asked for this copy of the letter.
    Asked,
    /// This tenant has nobody it may mail yet, so there is no record to
    /// resolve against. Reported rather than substituted: a preview that
    /// quietly showed fallbacks would read as a letter that is simply not
    /// personalised.
    NobodyToMailYet,
}

impl FallbackReason {
    /// The token an API answers with.
    pub fn as_str(self) -> &'static str {
        match self {
            FallbackReason::Asked => "asked",
            FallbackReason::NobodyToMailYet => "nobody_to_mail_yet",
        }
    }
}

/// Whose values a preview used.
///
/// An enum rather than an `Option<CampaignRecipient>` because the two cases are
/// two different sentences on the screen — *this is what Jean receives* and
/// *this is what somebody we know nothing about receives* — and a screen that
/// had to derive the second from a `None` would eventually derive it as the
/// first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewAgainst {
    /// A person this tenant holds a record of and may mail.
    Recipient {
        /// Their address, normalised.
        address: String,
        /// The best name any source offers, or `None`.
        name: Option<String>,
        /// ISO 3166-1 alpha-2, where a billing customer names one.
        country: Option<String>,
    },
    /// Nobody, and why.
    Fallbacks(FallbackReason),
}

/// A campaign, rendered for one reader.
///
/// [`html`](Self::html) and [`text`](Self::text) are taken from the assembled
/// `multipart/alternative` rather than rendered again here: a preview that
/// re-ran the renderers would be a second opinion about the letter, and the
/// whole point of showing one is that it is the same compilation a send uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CampaignPreview {
    /// The subject, personalised — what lands in this reader's inbox list.
    pub subject: String,
    /// The preview text beside it, personalised, or `None`.
    pub preheader: Option<String>,
    /// The HTML part, exactly as assembled.
    pub html: String,
    /// The plain-text alternative, exactly as assembled.
    pub text: String,
    /// What each merge field printed, and whether it was this reader's own
    /// value or the writer's fallback.
    pub fields: Vec<ResolvedMergeField>,
    /// Whose values these are.
    pub against: PreviewAgainst,
}

impl AccountStore {
    /// Renders one of this tenant's campaigns as one reader will receive it.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the campaign is absent or another
    /// tenant's, and when [`PreviewAs::Recipient`] names somebody this tenant
    /// may not mail — an address with no consent record, a suppressed one and
    /// one nobody has ever heard of are one answer, because the difference
    /// between them is the audience screen's to show and not a preview's to
    /// leak. [`StoreError::Validation`] when the address is not one this
    /// audience could hold, or when the stored letter would not pass the write
    /// gate. [`StoreError::Db`] on failure.
    pub async fn preview_campaign(
        &self,
        id: &CampaignId,
        against: &PreviewAs,
        unsubscribe: &UnsubscribeInvitation,
    ) -> Result<CampaignPreview> {
        let campaign = self.campaign(id).await?.ok_or(StoreError::NotFound)?;
        let (values, against) = self.values_for(against).await?;

        // The preview carries the footer because the recipient will see it: a
        // preview that hid the one control a reader looks for would be a
        // preview of a different letter. Its URL is the caller's placeholder —
        // there is no recipient here, so there is no token to mint.
        let letter = CampaignLetter {
            subject: &campaign.subject,
            preheader: campaign.preheader.as_deref(),
            content: &campaign.content,
            unsubscribe,
        };
        let personalised = personalise_campaign(&letter, &values)?;
        let message = render_campaign_message(&personalised.letter(unsubscribe))?;

        Ok(CampaignPreview {
            subject: personalised.subject,
            preheader: personalised.preheader,
            html: message.html,
            text: message.text,
            fields: personalised.fields,
            against,
        })
    }

    /// The values a preview resolves with, and the honest account of where they
    /// came from.
    async fn values_for(
        &self,
        against: &PreviewAs,
    ) -> Result<(CampaignMergeValues, PreviewAgainst)> {
        match against {
            PreviewAs::Fallbacks => Ok((
                CampaignMergeValues::default(),
                PreviewAgainst::Fallbacks(FallbackReason::Asked),
            )),
            PreviewAs::Recipient(address) => {
                // `NotFound` rather than a validation message naming the
                // reason: see the doc comment on `preview_campaign`.
                let recipient = self
                    .campaign_recipient(address)
                    .await?
                    .ok_or(StoreError::NotFound)?;
                Ok((
                    CampaignMergeValues::for_recipient(&recipient),
                    PreviewAgainst::Recipient {
                        address: recipient.address,
                        name: recipient.name,
                        country: recipient.country,
                    },
                ))
            }
            PreviewAs::AnyRecipient => {
                let page = AudiencePage {
                    after: None,
                    limit: 1,
                };
                match self.campaign_recipients(&page).await?.pop() {
                    Some(recipient) => Ok((
                        CampaignMergeValues::for_recipient(&recipient),
                        PreviewAgainst::Recipient {
                            address: recipient.address,
                            name: recipient.name,
                            country: recipient.country,
                        },
                    )),
                    None => Ok((
                        CampaignMergeValues::default(),
                        PreviewAgainst::Fallbacks(FallbackReason::NobodyToMailYet),
                    )),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_reason_a_preview_used_nobody_is_a_token_an_api_can_answer_with() {
        // Two different sentences on the screen — "you asked for this copy" and
        // "you have nobody to mail yet" — so they are two tokens and not one
        // `null`.
        assert_eq!(FallbackReason::Asked.as_str(), "asked");
        assert_eq!(
            FallbackReason::NobodyToMailYet.as_str(),
            "nobody_to_mail_yet"
        );
        assert_ne!(
            FallbackReason::Asked.as_str(),
            FallbackReason::NobodyToMailYet.as_str()
        );
    }

    #[test]
    fn resolving_against_nobody_prints_every_writers_fallback() {
        // The copy most of a form-built audience receives. `CampaignMergeValues`
        // is what a preview against nobody uses, and every field of it has to be
        // absent for that to be true — an `email` that defaulted to something
        // would make one field look personalised in that preview.
        let nobody = CampaignMergeValues::default();
        assert!(nobody.email.is_empty());
        assert_eq!(nobody.name, None);
        assert_eq!(nobody.country, None);
    }
}
