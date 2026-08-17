//! How we know a person agreed to be mailed (alo Campaigns, ADR 0044 §2, wave
//! C1.2) — the record, not the checkbox.
//!
//! ADR 0044 §2: *every recipient carries the provenance of their consent — when,
//! from which form, from which address — and a campaign cannot be sent to
//! somebody without one.* This module owns the first half of that sentence. The
//! second half is enforced one file over, in
//! [`campaign_audience`](crate::campaign_audience): a person with no row here
//! cannot come out of the recipients query, because the join is in the SQL
//! rather than in a caller's memory.
//!
//! ## Why this is a table of events
//!
//! "Did they agree" and "how do we know" are different questions, and only the
//! second survives a complaint. A boolean answers the first and destroys the
//! evidence for the second the moment it is overwritten. So each act of consent
//! is its own row and **nothing here is ever updated or deleted**: somebody who
//! ticked a box on a site form in March and re-confirmed through an imported
//! list in June has two rows, and a question about June is answered with June's
//! statement rather than with a flag that says yes.
//!
//! Three columns carry the provenance, and each exists because a summary of it
//! would not:
//!
//! - [`ConsentSource`] — *what kind of thing* the agreement came from. `import`
//!   and `manual` are origins the audience has no notion of, which is why this
//!   is not [`AudienceSource`](crate::AudienceSource): ADR 0044 §2 calls
//!   imported lists the dangerous path, and a path that cannot be named as
//!   itself cannot be treated as such.
//! - `source_ref` — *which one*: which form, which import, which conversation.
//!   Required for the origins where that is the whole question
//!   ([`ConsentSource::requires_reference`]).
//! - `statement` — what the tenant says the person agreed to, in the tenant's
//!   own words. Mandatory. A consent record with no statement is a boolean with
//!   extra columns.
//!
//! And two timestamps rather than one: `occurred_at` is when the person agreed,
//! `recorded_at` is when this workspace was told. An import carries consent
//! obtained months before anybody typed it in, and dating it from the typing
//! would overstate how fresh it is.
//!
//! ## Keyed by address
//!
//! There is no list (ADR 0044's central claim), so there is no row to hang
//! consent off: the same person is a billing customer, the contact on two deals
//! and a form submitter at once, and the thing they agreed with is their
//! address. The address is normalised on the way in by
//! [`normalise_address`](crate::normalise_address) — the same fold the audience
//! applies to its three sources — so `ANN@Lead.TEST` and `ann@lead.test` are one
//! person's consent rather than two half-answers.
//!
//! ## What this module deliberately does not do
//!
//! **It cannot take consent away.** An unsubscribe, a hard bounce or a
//! complaint suppresses absolutely and tenant-wide (ADR 0044 §2, queue item
//! C1.3) — a stronger rule than "the yes was withdrawn", and a different table.
//! Deleting the row instead would lose what the person agreed to before they
//! changed their mind, and would let a re-import quietly recreate it.
//!
//! Nothing in this module sends anything.

use time::{Duration, OffsetDateTime};

use crate::account::AccountStore;
use crate::campaign_audience::normalise_address;
use crate::error::{Result, StoreError};
use crate::id::{CampaignConsentId, UserId};

/// The longest statement a consent record will carry.
///
/// Long enough for the sentence beside a form's tick box or the provenance of
/// an imported list; short enough that nobody pastes a contract into it and
/// calls the result evidence.
pub const CONSENT_STATEMENT_MAX: usize = 500;

/// The longest `source_ref` — an identifier, a file name, a form id.
pub const CONSENT_SOURCE_REF_MAX: usize = 200;

/// The most consent records one read of a person's history returns.
///
/// A person's provenance is a handful of rows in every real tenant; a cap keeps
/// a pathological one from being read whole into a screen.
pub const CONSENT_HISTORY_MAX: i64 = 100;

/// How far ahead of this server's clock a consent may be dated.
///
/// Not zero, because the caller's clock is not this one and a few seconds of
/// skew is not a lie; not open-ended, because consent dated next year is either
/// a typo or an attempt to make a stale agreement look fresh, and both are
/// worth refusing at the door.
const CONSENT_FUTURE_SKEW_MINUTES: i64 = 5;

/// What kind of thing an agreement came from.
///
/// Deliberately wider than [`AudienceSource`](crate::AudienceSource): the
/// audience knows the three tenant-wide places a person's *address* is held,
/// while consent also arrives by import and by a colleague writing down what
/// they were told.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ConsentSource {
    /// A form on the tenant's published site, where the person themselves
    /// agreed.
    SiteForm,
    /// A customer the tenant invoices agreed — at the counter, in an order, on
    /// a signup.
    BillingCustomer,
    /// The contact on a CRM deal agreed, in the course of that deal.
    CrmDeal,
    /// An imported list. **The dangerous path** (ADR 0044 §2): the import must
    /// state where the addresses came from, and that statement is what is
    /// stored here.
    Import,
    /// A colleague recording what they were told directly.
    Manual,
}

impl ConsentSource {
    /// The stored token. Stable: it is written into rows that outlive releases
    /// and is read back by the screen.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SiteForm => "site_form",
            Self::BillingCustomer => "billing_customer",
            Self::CrmDeal => "crm_deal",
            Self::Import => "import",
            Self::Manual => "manual",
        }
    }

    /// Parses a stored token, or `None` when it is not one of ours.
    pub fn parse(token: &str) -> Option<Self> {
        match token {
            "site_form" => Some(Self::SiteForm),
            "billing_customer" => Some(Self::BillingCustomer),
            "crm_deal" => Some(Self::CrmDeal),
            "import" => Some(Self::Import),
            "manual" => Some(Self::Manual),
            _ => None,
        }
    }

    /// Whether *which one* is part of the answer, and therefore required.
    ///
    /// A form and an import are both a specific thing with a specific wording,
    /// and "they agreed on a form" is not an answer to which form. A customer,
    /// a deal or a colleague's note may honestly have nothing to point at, and
    /// demanding a made-up reference there would buy a filled-in column rather
    /// than evidence.
    pub fn requires_reference(&self) -> bool {
        matches!(self, Self::SiteForm | Self::Import)
    }
}

/// One recorded act of consent, as it is stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CampaignConsent {
    pub id: CampaignConsentId,
    /// The person, normalised — their identity across every campaign query.
    pub address: String,
    pub source: ConsentSource,
    /// Which form, which import, which conversation. `None` only where
    /// [`ConsentSource::requires_reference`] is false.
    pub source_ref: Option<String>,
    /// What the tenant says this person agreed to.
    pub statement: String,
    /// The colleague whose workspace recorded it — who to ask when the
    /// statement turns out to be wrong. Never the person consenting.
    pub recorded_by: UserId,
    /// When the person agreed.
    pub occurred_at: OffsetDateTime,
    /// When this workspace was told.
    pub recorded_at: OffsetDateTime,
}

/// The consent a recipient is reachable *by* — the freshest record for their
/// address, carried alongside them so that "may we mail this person" and "how
/// do we know" are answered by the same read.
///
/// Deliberately not a `bool`: a
/// [`CampaignRecipient`](crate::campaign_audience::CampaignRecipient) holds one
/// of these and there is no way to build one without it, so a caller cannot
/// arrive at a recipient without also holding the reason they are one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsentEvidence {
    /// The record this evidence comes from — what to read for the statement.
    pub record: CampaignConsentId,
    pub source: ConsentSource,
    /// When the person agreed (not when it was typed in).
    pub occurred_at: OffsetDateTime,
}

/// What to record. A struct rather than five positional arguments, because
/// `record_campaign_consent(a, b, c, None)` is a bug waiting to be written.
#[derive(Debug, Clone)]
pub struct NewCampaignConsent<'a> {
    /// The person's address in whatever casing it arrived in; normalised here.
    pub address: &'a str,
    pub source: ConsentSource,
    /// Which form, which import — required when
    /// [`ConsentSource::requires_reference`].
    pub source_ref: Option<&'a str>,
    /// What they agreed to, in the tenant's words. Must not be blank.
    pub statement: &'a str,
    /// When they agreed. `None` means now — correct for a form submitted this
    /// second, wrong for an import, which knows its own date.
    pub occurred_at: Option<OffsetDateTime>,
}

/// The SQL that writes one record.
fn insert_sql() -> &'static str {
    "INSERT INTO campaign_consent \
         (tenant_id, id, address, source, source_ref, statement, recorded_by, occurred_at) \
     VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
     RETURNING recorded_at"
}

/// The SQL of one person's provenance, freshest first.
fn history_sql() -> &'static str {
    "SELECT id, address, source, source_ref, statement, recorded_by, occurred_at, recorded_at \
       FROM campaign_consent \
      WHERE tenant_id = $1 AND address = $2 \
      ORDER BY occurred_at DESC, id DESC \
      LIMIT $3"
}

/// Every statement this module can issue.
///
/// The same list [`crate::campaign_audience`] keeps, and for the same reason:
/// the promise that no campaign query reads the per-user address book is
/// checked against the strings rather than asserted in a comment.
#[cfg(test)]
fn all_sql() -> Vec<&'static str> {
    vec![insert_sql(), history_sql()]
}

/// A row as [`history_sql`] returns it.
type ConsentRow = (
    String,
    String,
    String,
    Option<String>,
    String,
    String,
    OffsetDateTime,
    OffsetDateTime,
);

/// Turns a stored row into a record, refusing a source token this build does
/// not know.
///
/// A token we do not recognise is a decode failure rather than a dropped field:
/// this module writes those strings itself, so an unknown one means the schema
/// moved under the enum, and reporting the consent with its provenance quietly
/// blanked is the one outcome worse than an error.
fn row_to_consent(row: ConsentRow) -> Result<CampaignConsent> {
    let source = ConsentSource::parse(&row.2).ok_or_else(|| {
        StoreError::Db(sqlx::Error::Decode(
            "campaign consent names a source this build does not know".into(),
        ))
    })?;
    Ok(CampaignConsent {
        id: CampaignConsentId::new(row.0),
        address: row.1,
        source,
        source_ref: row.3,
        statement: row.4,
        recorded_by: UserId::new(row.5),
        occurred_at: row.6,
        recorded_at: row.7,
    })
}

/// The validated shape of a [`NewCampaignConsent`], ready to bind.
struct Validated {
    address: String,
    source_ref: Option<String>,
    statement: String,
    occurred_at: OffsetDateTime,
}

/// Checks one submission, once, in one place.
///
/// Separated from the write so the rules are testable without a database, and
/// so there is exactly one of them: a second validation path is how a "consent
/// record" with an empty statement eventually gets written.
fn validate(consent: &NewCampaignConsent<'_>, now: OffsetDateTime) -> Result<Validated> {
    let address = normalise_address(consent.address).ok_or_else(|| {
        StoreError::Validation(
            "consent needs the address it was given for, and this is not one".to_owned(),
        )
    })?;

    let statement = consent.statement.trim();
    if statement.is_empty() {
        return Err(StoreError::Validation(
            "a consent record says what the person agreed to; without that it is a checkbox"
                .to_owned(),
        ));
    }
    if statement.chars().count() > CONSENT_STATEMENT_MAX {
        return Err(StoreError::Validation(format!(
            "what somebody agreed to fits in {CONSENT_STATEMENT_MAX} characters"
        )));
    }

    let source_ref = consent.source_ref.map(str::trim).filter(|r| !r.is_empty());
    if let Some(reference) = source_ref {
        if reference.chars().count() > CONSENT_SOURCE_REF_MAX {
            return Err(StoreError::Validation(format!(
                "a consent source reference fits in {CONSENT_SOURCE_REF_MAX} characters"
            )));
        }
    } else if consent.source.requires_reference() {
        return Err(StoreError::Validation(format!(
            "consent from {} says which one",
            consent.source.as_str()
        )));
    }

    let occurred_at = consent.occurred_at.unwrap_or(now);
    if occurred_at > now + Duration::minutes(CONSENT_FUTURE_SKEW_MINUTES) {
        return Err(StoreError::Validation(
            "consent cannot be dated after it was given".to_owned(),
        ));
    }

    Ok(Validated {
        address,
        source_ref: source_ref.map(str::to_owned),
        statement: statement.to_owned(),
        occurred_at,
    })
}

impl AccountStore {
    /// Records that somebody agreed to be mailed, with the provenance that
    /// makes it evidence.
    ///
    /// Append-only: a second record for the same address does not replace the
    /// first, it joins it. The recipients query reads the freshest, the history
    /// keeps them all.
    ///
    /// The recorder is always the caller — this handle *is* the colleague whose
    /// workspace is making the claim — so there is no parameter for it and no
    /// way to attribute a consent record to somebody else.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the address is not one, the statement is
    /// blank or too long, a source that names one is given no reference, or the
    /// consent is dated in the future; [`StoreError::Db`] on failure.
    pub async fn record_campaign_consent(
        &self,
        consent: &NewCampaignConsent<'_>,
    ) -> Result<CampaignConsent> {
        let valid = validate(consent, OffsetDateTime::now_utc())?;
        let id = CampaignConsentId::generate();
        let recorded_at: OffsetDateTime = sqlx::query_scalar(insert_sql())
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .bind(&valid.address)
            .bind(consent.source.as_str())
            .bind(valid.source_ref.as_deref())
            .bind(&valid.statement)
            .bind(self.user.as_str())
            .bind(valid.occurred_at)
            .fetch_one(&self.pool)
            .await
            .map_err(StoreError::Db)?;
        Ok(CampaignConsent {
            id,
            address: valid.address,
            source: consent.source,
            source_ref: valid.source_ref,
            statement: valid.statement,
            recorded_by: self.user.clone(),
            occurred_at: valid.occurred_at,
            recorded_at,
        })
    }

    /// One person's provenance, freshest first — the answer to "how do we
    /// know", quoted rather than summarised.
    ///
    /// Empty is a complete answer: this tenant has no evidence for that
    /// address, which is exactly why the recipients query will not produce
    /// them.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the address is not one; [`StoreError::Db`]
    /// on failure.
    pub async fn campaign_consent_for(&self, address: &str) -> Result<Vec<CampaignConsent>> {
        let address = normalise_address(address).ok_or_else(|| {
            StoreError::Validation(
                "consent is held against an address, and this is not one".to_owned(),
            )
        })?;
        let rows: Vec<ConsentRow> = sqlx::query_as(history_sql())
            .bind(self.tenant.as_str())
            .bind(&address)
            .bind(CONSENT_HISTORY_MAX)
            .fetch_all(&self.pool)
            .await
            .map_err(StoreError::Db)?;
        rows.into_iter().map(row_to_consent).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The identifiers a SQL string contains — see the twin of this helper in
    /// `campaign_audience.rs` for why identifiers rather than substrings.
    fn identifiers(sql: &str) -> Vec<&str> {
        sql.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .filter(|token| !token.is_empty())
            .collect()
    }

    fn new(statement: &str) -> NewCampaignConsent<'_> {
        NewCampaignConsent {
            address: "Ann@Lead.TEST",
            source: ConsentSource::Manual,
            source_ref: None,
            statement,
            occurred_at: None,
        }
    }

    #[test]
    fn no_query_in_this_module_can_read_the_per_user_address_book() {
        for sql in all_sql() {
            assert!(
                !identifiers(sql).contains(&"contacts"),
                "a campaign consent query names the per-user address book: {sql}"
            );
        }
    }

    #[test]
    fn every_query_is_scoped_to_one_tenant() {
        for sql in all_sql() {
            assert!(
                sql.contains("tenant_id"),
                "a consent query without a tenant: {sql}"
            );
        }
    }

    #[test]
    fn a_source_token_survives_a_round_trip_and_an_unknown_one_is_not_guessed() {
        for source in [
            ConsentSource::SiteForm,
            ConsentSource::BillingCustomer,
            ConsentSource::CrmDeal,
            ConsentSource::Import,
            ConsentSource::Manual,
        ] {
            assert_eq!(ConsentSource::parse(source.as_str()), Some(source));
        }
        for unknown in ["contacts", "", "Import", "opt_in", "site_forms"] {
            assert_eq!(ConsentSource::parse(unknown), None);
        }
    }

    #[test]
    fn the_dangerous_paths_are_the_ones_that_must_say_which_one() {
        assert!(ConsentSource::Import.requires_reference());
        assert!(ConsentSource::SiteForm.requires_reference());
        for lenient in [
            ConsentSource::BillingCustomer,
            ConsentSource::CrmDeal,
            ConsentSource::Manual,
        ] {
            assert!(!lenient.requires_reference());
        }
    }

    #[test]
    fn a_consent_record_carries_the_address_folded_to_one_identity() {
        let now = OffsetDateTime::UNIX_EPOCH;
        let valid = validate(&new("Said yes at the counter"), now)
            .unwrap_or_else(|e| panic!("rejected a good record: {e:?}"));
        assert_eq!(valid.address, "ann@lead.test");
        assert_eq!(valid.occurred_at, now);
        assert_eq!(valid.source_ref, None);
    }

    #[test]
    fn a_statement_that_says_nothing_is_not_a_consent_record() {
        let now = OffsetDateTime::UNIX_EPOCH;
        for blank in ["", "   ", "\t\n"] {
            assert!(
                matches!(validate(&new(blank), now), Err(StoreError::Validation(_))),
                "accepted a blank statement {blank:?}"
            );
        }
        let long = "x".repeat(CONSENT_STATEMENT_MAX + 1);
        assert!(matches!(
            validate(&new(&long), now),
            Err(StoreError::Validation(_))
        ));
        // The boundary itself is allowed: a limit that refuses its own maximum
        // is a different limit.
        let exact = "x".repeat(CONSENT_STATEMENT_MAX);
        assert!(validate(&new(&exact), now).is_ok());
    }

    #[test]
    fn an_address_nobody_could_be_mailed_at_carries_no_consent() {
        let now = OffsetDateTime::UNIX_EPOCH;
        for junk in ["", "   ", "n/a", "ask reception", "ann@localhost"] {
            let candidate = NewCampaignConsent {
                address: junk,
                ..new("Agreed on the phone")
            };
            assert!(
                matches!(validate(&candidate, now), Err(StoreError::Validation(_))),
                "accepted consent for {junk:?}"
            );
        }
    }

    #[test]
    fn an_import_that_cannot_say_where_it_came_from_is_refused() {
        let now = OffsetDateTime::UNIX_EPOCH;
        for blank in [None, Some(""), Some("   ")] {
            let candidate = NewCampaignConsent {
                source: ConsentSource::Import,
                source_ref: blank,
                ..new("Bought at the trade fair, opt-in box on the form")
            };
            assert!(
                matches!(validate(&candidate, now), Err(StoreError::Validation(_))),
                "an import passed with source_ref {blank:?}"
            );
        }
        let named = NewCampaignConsent {
            source: ConsentSource::Import,
            source_ref: Some("  fair-2026.csv  "),
            ..new("Trade fair sign-up sheet, opt-in box ticked")
        };
        let valid = validate(&named, now).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(valid.source_ref.as_deref(), Some("fair-2026.csv"));
    }

    #[test]
    fn consent_cannot_be_dated_after_it_was_given() {
        let now = OffsetDateTime::UNIX_EPOCH + Duration::days(365);
        let ahead = NewCampaignConsent {
            occurred_at: Some(now + Duration::hours(1)),
            ..new("Agreed at the counter")
        };
        assert!(matches!(
            validate(&ahead, now),
            Err(StoreError::Validation(_))
        ));
        // Clock skew is not a lie.
        let barely = NewCampaignConsent {
            occurred_at: Some(now + Duration::minutes(CONSENT_FUTURE_SKEW_MINUTES - 1)),
            ..new("Agreed at the counter")
        };
        assert!(validate(&barely, now).is_ok());
        // An import's consent is old, and that is the point of the column: it
        // is kept as given rather than dated from the day it was typed in.
        let long_ago = now - Duration::days(200);
        let old = NewCampaignConsent {
            occurred_at: Some(long_ago),
            ..new("Agreed at the counter")
        };
        assert_eq!(
            validate(&old, now)
                .unwrap_or_else(|e| panic!("{e:?}"))
                .occurred_at,
            long_ago
        );
    }

    #[test]
    fn a_row_naming_an_unknown_source_fails_the_read_rather_than_losing_provenance() {
        let row = (
            "id".to_owned(),
            "ann@lead.test".to_owned(),
            "address_book".to_owned(),
            None,
            "Agreed".to_owned(),
            "u1".to_owned(),
            OffsetDateTime::UNIX_EPOCH,
            OffsetDateTime::UNIX_EPOCH,
        );
        assert!(matches!(row_to_consent(row), Err(StoreError::Db(_))));
    }
}
