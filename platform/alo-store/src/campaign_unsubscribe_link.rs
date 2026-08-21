//! The unsubscribe as a recipient meets it: a header their client turns into a
//! button, and a link in the footer for everyone whose client draws no button
//! (alo Campaigns, ADR 0044 §3, queue items C2.4 and C2.5).
//!
//! Design: `docs/design/campaign-unsubscribe-in-the-mail.md`.
//!
//! Everything about *leaving* already existed except the half a recipient can
//! reach. [`campaign_unsubscribe`](crate::campaign_unsubscribe) mints the
//! 256-bit per-recipient token and keeps only its digest;
//! [`campaign_topic_optout`](crate::campaign_topic_optout) records the narrower
//! choice; the landing page offers this kind of mail or all of it, with no
//! account and no login. This module is the two doors to that page.
//!
//! ## Why this is a condition on sending rather than a feature of it
//!
//! A bulk message with no working opt-out is not an incomplete campaign, it is
//! an unlawful one: GDPR Art. 21(3) and ePrivacy Art. 13 require the right to
//! withdraw *at the time of each message*, and since February 2024 both Gmail
//! and Outlook require RFC 8058 one-click from bulk senders as a condition of
//! delivery. So [`UnsubscribeInvitation`] is a **required** part of a letter
//! rather than an optional one: a campaign that cannot be left cannot be
//! rendered, which puts the guarantee in the type system instead of in
//! somebody's memory.
//!
//! ## Why one HTTPS URL and no `mailto:`
//!
//! RFC 2369 §3.2 allows several URLs, most-preferred first, and a `mailto:`
//! form is traditional. We emit exactly one HTTPS URL. A `mailto:` alternative
//! needs a mailbox that parses unsubscribe mail — a second implementation of
//! the same promise, with its own failure modes — and RFC 8058 one-click, which
//! is the form the clients that matter actually use, applies only to HTTPS.

use crate::error::{Result, StoreError};

/// The exact value RFC 8058 §3.1 defines for the one-click signal. A literal,
/// not a format: a client matches it verbatim, and anything else means the
/// header is ignored and the mail is treated as having no one-click at all.
pub const ONE_CLICK_POST: &str = "List-Unsubscribe=One-Click";

/// The header a mail client turns into its Unsubscribe button (RFC 2369 §3.2).
pub const LIST_UNSUBSCRIBE: &str = "List-Unsubscribe";

/// The header that says the button may act on a single POST (RFC 8058 §3.1).
pub const LIST_UNSUBSCRIBE_POST: &str = "List-Unsubscribe-Post";

/// The longest URL we will write into a header. RFC 5322 lines fold, so this is
/// not a wire limit; it is a sanity bound on caller-supplied data.
const URL_MAX: usize = 998;

/// How one recipient leaves.
///
/// Built by whatever is sending — only that knows which recipient a given
/// render is for, and therefore which token to name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsubscribeInvitation {
    /// Where a mail client's own button POSTs (RFC 8058 §3.2) — **the API**.
    ///
    /// HTTPS, and carrying the unguessable token
    /// [`campaign_unsubscribe`](crate::campaign_unsubscribe) minted — RFC 8058
    /// §7 is explicit that a guessable URI lets anybody unsubscribe anybody.
    pub one_click_url: String,
    /// Where a **person** who clicks the footer lands — the page.
    ///
    /// A separate URL from [`one_click_url`](Self::one_click_url), and the
    /// separation is load-bearing rather than tidy. The one-click endpoint
    /// answers JSON and exists to be POSTed by software; the page is the SPA
    /// route that offers this kind of mail or all of it (ADR 0044 §3). Point
    /// the footer at the API and a human who clicks it reads a JSON object;
    /// point the header at the page and the client's POST lands on the SPA and
    /// silently does nothing — the button appears to work and the recipient
    /// keeps receiving mail, which is the worse of the two.
    pub page_url: String,
    /// The kind of mail this is, when the campaign named one.
    ///
    /// `None` means the page can only offer all-or-nothing. That is the choice
    /// ADR 0044 §3 says pushes a recipient toward the spam button instead, so a
    /// campaign is required to carry a topic — but this type does not enforce
    /// that, because the campaign record already does and one rule in two
    /// places is one rule that can differ.
    pub topic: Option<String>,
    /// The words of the visible link, **in the recipient's language**.
    ///
    /// Supplied by the caller rather than written here, and that is an i18n
    /// decision rather than a convenience. This crate renders letters and knows
    /// nothing about locale; the send path knows which language a tenant writes
    /// to a given audience in. A hardcoded "Unsubscribe" would be the one
    /// English string in a European product's bulk mail, sitting in the single
    /// place a recipient looks when they want it to stop — and a footer they
    /// cannot read is a spam complaint rather than an unsubscribe.
    ///
    /// It is never a URL and never markup: it is escaped like any other text on
    /// its way into the letter.
    pub link_text: String,
}

/// One address, held to what a letter can carry.
///
/// `what` names the address in the refusal, because "an unsubscribe address is
/// not https" is unactionable when a letter carries two of them.
fn checked_url(raw: &str, what: &str) -> Result<()> {
    // Checked on the RAW string, before any trimming. `trim` would strip a
    // trailing CR or LF and quietly accept a URL that arrived with one — safe
    // in that instance, but it hides the fact that whatever produced the URL is
    // emitting line breaks into it, which is the thing worth knowing.
    if raw.contains('\r') || raw.contains('\n') {
        return Err(StoreError::Validation(format!(
            "{what}: an unsubscribe address cannot contain a line break"
        )));
    }
    let url = raw.trim();
    if url.is_empty() {
        return Err(StoreError::Validation(format!(
            "{what}: a campaign carries a way to leave it, and this names no address"
        )));
    }
    if url.len() > URL_MAX {
        return Err(StoreError::Validation(format!(
            "{what}: an unsubscribe address is at most {URL_MAX} characters"
        )));
    }
    // RFC 8058 §3.1 ties one-click to HTTPS. Emitting the POST header beside
    // any other scheme produces a header the client ignores, which reads as
    // "we offer one-click" while offering nothing — and a plaintext page for
    // somebody exercising a privacy right is its own answer.
    if !url.starts_with("https://") {
        return Err(StoreError::Validation(format!(
            "{what}: an unsubscribe address must be https — RFC 8058 one-click applies to no              other scheme, and a header the client ignores is worse than none"
        )));
    }
    Ok(())
}

impl UnsubscribeInvitation {
    /// Checks the invitation, or says why it cannot be put in a message.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the URL is blank, over-long, not HTTPS,
    /// or carries a CR or LF; or when the topic is present but blank.
    pub fn validated(&self) -> Result<()> {
        // Both addresses, held to the same rule. The header URL and the page
        // URL are different endpoints doing different jobs, and either one
        // being wrong breaks the way out in a way the recipient discovers and
        // we do not.
        checked_url(&self.one_click_url, "the mail client's Unsubscribe button")?;
        checked_url(&self.page_url, "the link in the letter")?;

        if self.topic.as_deref().is_some_and(|t| t.trim().is_empty()) {
            return Err(StoreError::Validation(
                "a blank topic would offer to stop receiving nothing in particular".to_owned(),
            ));
        }
        // A link with no words is a link nobody finds. This is the one control
        // a recipient is looking for when they have decided they want the mail
        // to stop, and the alternative they reach for when they cannot find it
        // is the spam button.
        if self.link_text.trim().is_empty() {
            return Err(StoreError::Validation(
                "the unsubscribe link needs words a recipient can read and click".to_owned(),
            ));
        }
        if self.link_text.contains('\r') || self.link_text.contains('\n') {
            return Err(StoreError::Validation(
                "the unsubscribe link's words are one line".to_owned(),
            ));
        }
        Ok(())
    }

    /// The two headers, ready for a sender to write.
    ///
    /// Returned together because they are only correct together:
    /// `List-Unsubscribe` alone is the pre-2018 behaviour (a client may open
    /// the URL, and a human finishes the job), and `List-Unsubscribe-Post`
    /// alone means nothing at all.
    ///
    /// # Errors
    /// Whatever [`validated`](Self::validated) refuses.
    pub fn header_pair(&self) -> Result<[(&'static str, String); 2]> {
        self.validated()?;
        Ok([
            // RFC 2369 §3.2: each URL in angle brackets.
            (LIST_UNSUBSCRIBE, format!("<{}>", self.one_click_url.trim())),
            (LIST_UNSUBSCRIBE_POST, ONE_CLICK_POST.to_owned()),
        ])
    }
}

/// An invitation for the crate's own tests.
///
/// Deliberately **not English**: every renderer test that uses it would
/// otherwise pin an English footer into a golden file, and the first person to
/// read one would reasonably conclude the words belong to this crate. They
/// belong to the caller — see [`UnsubscribeInvitation::link_text`].
#[cfg(test)]
pub(crate) fn an_invitation() -> UnsubscribeInvitation {
    UnsubscribeInvitation {
        one_click_url: "https://alo.test/jmap/campaign-unsubscribe/9tOKENx".to_owned(),
        page_url: "https://alo.test/unsubscribe/9tOKENx".to_owned(),
        topic: Some("Nieuwsbrief".to_owned()),
        link_text: "Uitschrijven".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    /// An invitation whose BOTH addresses are `url`.
    ///
    /// The URL tests below are about one rule applied to each address, so they
    /// set the two together; `the_two_addresses_are_not_interchangeable` is the
    /// one that holds them apart.
    fn invitation(url: &str) -> UnsubscribeInvitation {
        UnsubscribeInvitation {
            one_click_url: url.to_owned(),
            page_url: url.to_owned(),
            topic: Some("Monthly Newsletter".to_owned()),
            // Deliberately not English: the words come from the caller, and a
            // fixture that only ever spoke English would let a hardcoded
            // "Unsubscribe" creep back in unnoticed.
            link_text: "Se désabonner".to_owned(),
        }
    }

    #[test]
    fn the_headers_are_exactly_what_rfc_8058_defines() {
        let pair = invitation("https://alo.test/u/abc123")
            .header_pair()
            .expect("a good invitation");
        assert_eq!(pair[0].0, "List-Unsubscribe");
        // RFC 2369 §3.2 — angle brackets around the URL.
        assert_eq!(pair[0].1, "<https://alo.test/u/abc123>");
        assert_eq!(pair[1].0, "List-Unsubscribe-Post");
        // RFC 8058 §3.1 — matched verbatim by clients, so it is a literal and
        // never a format string.
        assert_eq!(pair[1].1, "List-Unsubscribe=One-Click");
    }

    #[test]
    fn a_url_that_is_not_https_is_refused_rather_than_offered() {
        // One-click applies to no other scheme (RFC 8058 §3.1), so the POST
        // header beside it would be ignored — which reads as offering one-click
        // while offering nothing.
        for url in [
            "http://alo.test/u/abc",
            "mailto:leave@alo.test",
            "ftp://alo.test/u/abc",
        ] {
            assert!(
                invitation(url).header_pair().is_err(),
                "{url} must be refused"
            );
        }
    }

    #[test]
    fn a_url_carrying_a_line_break_cannot_smuggle_a_header() {
        for url in [
            "https://alo.test/u/abc\r\nBcc: everyone@alo.test",
            "https://alo.test/u/abc\nX-Injected: 1",
            "https://alo.test/u/abc\r",
        ] {
            let error = invitation(url).header_pair().expect_err("refused");
            assert!(matches!(error, StoreError::Validation(_)));
        }
    }

    #[test]
    fn an_absent_address_is_refused_because_a_campaign_must_be_leavable() {
        assert!(invitation("").header_pair().is_err());
        assert!(invitation("   ").header_pair().is_err());
    }

    #[test]
    fn an_over_long_address_is_refused_by_its_own_bound() {
        let long = format!("https://alo.test/u/{}", "a".repeat(URL_MAX));
        assert!(invitation(&long).header_pair().is_err());
    }

    #[test]
    fn a_blank_topic_is_refused_but_no_topic_is_allowed() {
        let mut none = invitation("https://alo.test/u/abc");
        none.topic = None;
        assert!(none.validated().is_ok(), "all-or-nothing is still a choice");

        let mut blank = invitation("https://alo.test/u/abc");
        blank.topic = Some("   ".to_owned());
        assert!(blank.validated().is_err());
    }

    #[test]
    fn the_url_is_trimmed_into_the_header_rather_than_carried_with_its_spaces() {
        let pair = invitation("  https://alo.test/u/abc  ")
            .header_pair()
            .unwrap();
        assert_eq!(pair[0].1, "<https://alo.test/u/abc>");
    }
    #[test]
    fn a_link_with_no_words_is_refused() {
        let mut wordless = invitation("https://alo.test/u/abc");
        wordless.link_text = "   ".to_owned();
        assert!(
            wordless.validated().is_err(),
            "a link nobody can see is a spam complaint waiting to happen"
        );
    }

    #[test]
    fn the_link_words_are_the_callers_language_rather_than_ours() {
        // The store renders letters and knows nothing about locale. Nothing in
        // this module may assume English.
        let dutch = UnsubscribeInvitation {
            one_click_url: "https://alo.test/jmap/campaign-unsubscribe/abc".to_owned(),
            page_url: "https://alo.test/unsubscribe/abc".to_owned(),
            topic: Some("Nieuwsbrief".to_owned()),
            link_text: "Uitschrijven".to_owned(),
        };
        assert!(dutch.validated().is_ok());
    }
    #[test]
    fn the_two_addresses_are_not_interchangeable() {
        // The header is POSTed by software and must reach the API; the footer
        // is clicked by a person and must reach the page. Swap them and either
        // a human reads a JSON object, or — worse — the client's POST lands on
        // the SPA, does nothing, and the button appears to have worked while
        // the recipient keeps receiving mail.
        let invitation = UnsubscribeInvitation {
            one_click_url: "https://alo.test/jmap/campaign-unsubscribe/tok".to_owned(),
            page_url: "https://alo.test/unsubscribe/tok".to_owned(),
            topic: Some("Nieuwsbrief".to_owned()),
            link_text: "Uitschrijven".to_owned(),
        };
        let pair = invitation.header_pair().expect("a good invitation");
        assert_eq!(
            pair[0].1, "<https://alo.test/jmap/campaign-unsubscribe/tok>",
            "the header names the endpoint that answers a POST, not the page"
        );
        assert_ne!(
            invitation.page_url, invitation.one_click_url,
            "if these are ever the same, one of the two doors is broken"
        );
    }

    #[test]
    fn each_address_is_refused_by_name_so_a_refusal_is_actionable() {
        let mut bad_header = UnsubscribeInvitation {
            one_click_url: "http://alo.test/jmap/campaign-unsubscribe/tok".to_owned(),
            page_url: "https://alo.test/unsubscribe/tok".to_owned(),
            topic: None,
            link_text: "Uitschrijven".to_owned(),
        };
        match bad_header.validated() {
            Err(StoreError::Validation(detail)) => {
                assert!(detail.contains("button"), "names which address: {detail}");
            }
            other => panic!("expected a refusal, got {other:?}"),
        }

        bad_header.one_click_url = "https://alo.test/jmap/campaign-unsubscribe/tok".to_owned();
        bad_header.page_url = "http://alo.test/unsubscribe/tok".to_owned();
        match bad_header.validated() {
            Err(StoreError::Validation(detail)) => {
                assert!(detail.contains("letter"), "names which address: {detail}");
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
    }
}
