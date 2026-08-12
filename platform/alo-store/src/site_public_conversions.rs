//! The anonymous write door for aggregate conversion events: how often a
//! site's own conversion point was seen, started, and completed.
//!
//! Like the other public analytics doors ([`crate::site_public_analytics`],
//! [`crate::site_public_heatmap`]) everything arriving here has already been
//! reduced by the public service, and the reduction is the privacy argument:
//!
//! - **The only identity is the site's own.** A conversion is attributed to a
//!   [`ConversionSource`] the tenant created — today a contact form, whose id
//!   is already public in the page's markup. No visitor token, no session, no
//!   cookie: there is no type here that could carry one.
//! - **Three counters, never a journey.** [`ConversionStage`] values are
//!   counted independently; nothing records that one browser viewed *and*
//!   submitted, so a funnel is a ratio of totals and resolves to nobody.
//! - **Bounded by construction.** A source id is only counted when it resolves
//!   to a row the tenant owns, so — unlike a page path — a visitor's browser
//!   cannot open new buckets by inventing ids, and no daily cap is needed.
//!
//! Two doors, because the two halves of the funnel are seen in two places.
//! The page's beacon reports the view and the start over the resolved Host
//! ([`SitePublicStore::record_public_site_conversion`]); the submit is counted
//! where a submission is actually written, from the bare form id the
//! submission endpoint holds ([`SitePublicStore::record_public_form_conversion`]).
//! Counting the submit at the real write rather than from the browser is
//! deliberate: it is the one stage a server can see for itself, and a script
//! is easier to lie to than a socket.

use time::Date;

use crate::error::{Result, StoreError};
use crate::site_public::{PublishedSite, SitePublicStore};

/// The longest source id this door will send to the database. Real ids are 22
/// characters (base64url of 16 random bytes); anything far outside that shape
/// is noise, not a lookup. Mirrors the migration's own bound.
pub const CONVERSION_SOURCE_ID_MAX_LEN: usize = 64;

/// The kind of site-owned object a conversion happened on. One variant today;
/// the later commerce and booking slices convert on their own objects, and the
/// stored word is what keeps those additive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversionSource {
    /// A contact form of the site (`site_forms`).
    Form,
}

impl ConversionSource {
    /// The stored word.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Form => "form",
        }
    }
}

/// How far a visitor got with a conversion point. The three stages are
/// counted independently and are deliberately *not* nested: a start with no
/// view (a visitor who arrived on an anchor, or whose view beacon was lost)
/// is a real observation and is counted as one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversionStage {
    /// The conversion point was on a page that was rendered.
    View,
    /// A visitor began filling it in.
    Start,
    /// A submission was written.
    Submit,
}

impl ConversionStage {
    /// The stages in funnel order — the order a report shows them in.
    pub const ORDERED: [Self; 3] = [Self::View, Self::Start, Self::Submit];

    /// The stored word.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::View => "view",
            Self::Start => "start",
            Self::Submit => "submit",
        }
    }

    /// The stage a wire word names, or `None` — the public service's parser
    /// and the store agree on exactly these three tokens. Deliberately not
    /// `FromStr`: this is the wire vocabulary of one endpoint, not a general
    /// parse of the type.
    #[must_use]
    pub fn from_word(word: &str) -> Option<Self> {
        match word {
            "view" => Some(Self::View),
            "start" => Some(Self::Start),
            "submit" => Some(Self::Submit),
            _ => None,
        }
    }
}

impl SitePublicStore {
    /// Counts one conversion stage on a form of the **resolved** site.
    ///
    /// Used by the page beacon, whose tenant scope is the Host it was sent to.
    /// The form must belong to that resolved site: a visitor who reads another
    /// site's form id out of its public markup and posts it here writes
    /// nothing, because the tenant and site in the inserted key come from a
    /// row that had to match the resolved site in the same statement.
    ///
    /// Returns whether a counter moved. An unknown or foreign source is
    /// `Ok(false)` rather than an error — the wire answer must not tell a
    /// prober whether an id exists.
    ///
    /// # Errors
    /// [`StoreError::Db`] if the aggregate write fails.
    pub async fn record_public_site_conversion(
        &self,
        site: &PublishedSite,
        day: Date,
        form_id: &str,
        stage: ConversionStage,
    ) -> Result<bool> {
        if !plausible_source_id(form_id) {
            return Ok(false);
        }
        // The insert resolves the form and writes in one statement, taking
        // tenant and site from the resolved row — the caller never supplies a
        // scope, so it can never supply the wrong one.
        let done = sqlx::query(
            "INSERT INTO site_conversion_daily \
                 (tenant_id, site_id, day, source_kind, source_id, stage, hits) \
             SELECT f.tenant_id, f.site_id, $3, 'form', f.id, $4, 1 \
             FROM site_forms f \
             WHERE f.id = $5 AND f.tenant_id = $1 AND f.site_id = $2 \
             ON CONFLICT (tenant_id, site_id, day, source_kind, source_id, stage) \
             DO UPDATE SET hits = site_conversion_daily.hits + 1",
        )
        .bind(site.tenant.as_str())
        .bind(site.site.as_str())
        .bind(day)
        .bind(stage.as_str())
        .bind(form_id)
        .execute(self.pool())
        .await
        .map_err(StoreError::Db)?;
        Ok(done.rows_affected() > 0)
    }

    /// Counts one conversion stage from a bare form id, for the submission
    /// endpoint — which holds the id a rendered `contact_form` section carries
    /// and nothing else.
    ///
    /// The form's own tenant and site are used, and only while its site is
    /// live, exactly as [`SitePublicStore::add_public_form_submission`] does:
    /// the two writes must agree about which forms exist at all, or a submit
    /// could be counted for a form that could not have been submitted.
    ///
    /// Returns whether a counter moved; an unknown, deleted, or draft-site
    /// form is `Ok(false)`.
    ///
    /// # Errors
    /// [`StoreError::Db`] if the aggregate write fails.
    pub async fn record_public_form_conversion(
        &self,
        form_id: &str,
        day: Date,
        stage: ConversionStage,
    ) -> Result<bool> {
        if !plausible_source_id(form_id) {
            return Ok(false);
        }
        let done = sqlx::query(
            "INSERT INTO site_conversion_daily \
                 (tenant_id, site_id, day, source_kind, source_id, stage, hits) \
             SELECT f.tenant_id, f.site_id, $1, 'form', f.id, $2, 1 \
             FROM site_forms f \
             JOIN sites s ON s.tenant_id = f.tenant_id AND s.id = f.site_id \
             WHERE f.id = $3 AND s.published_publish_id IS NOT NULL \
             ON CONFLICT (tenant_id, site_id, day, source_kind, source_id, stage) \
             DO UPDATE SET hits = site_conversion_daily.hits + 1",
        )
        .bind(day)
        .bind(stage.as_str())
        .bind(form_id)
        .execute(self.pool())
        .await
        .map_err(StoreError::Db)?;
        Ok(done.rows_affected() > 0)
    }
}

/// Whether a token is shaped like one of our ids at all. Not a lookup and not
/// a guarantee — the statement decides existence — just the door refusing to
/// send a kilobyte of someone's imagination to the database.
fn plausible_source_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= CONVERSION_SOURCE_ID_MAX_LEN
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_three_stages_have_stable_wire_words() {
        assert_eq!(ConversionStage::View.as_str(), "view");
        assert_eq!(ConversionStage::Start.as_str(), "start");
        assert_eq!(ConversionStage::Submit.as_str(), "submit");
        assert_eq!(ConversionSource::Form.as_str(), "form");
        for stage in ConversionStage::ORDERED {
            assert_eq!(ConversionStage::from_word(stage.as_str()), Some(stage));
        }
    }

    #[test]
    fn a_stage_is_one_of_three_words_or_it_is_nothing() {
        for hostile in ["", "VIEW", "views", "submitted", "form", "1", "view "] {
            assert_eq!(
                ConversionStage::from_word(hostile),
                None,
                "{hostile} became a stage"
            );
        }
    }

    #[test]
    fn a_source_id_is_an_id_shape_or_it_never_reaches_the_database() {
        assert!(plausible_source_id("Qk9tX3zvS1aQmN2pRt4uYw"));
        assert!(plausible_source_id("a-b_c"));
        for hostile in [
            "",
            "id with space",
            "id'or'1=1",
            "../../etc/passwd",
            "<script>",
            "id\n",
        ] {
            assert!(!plausible_source_id(hostile), "{hostile} was accepted");
        }
        assert!(!plausible_source_id(
            &"a".repeat(CONVERSION_SOURCE_ID_MAX_LEN + 1)
        ));
    }
}
