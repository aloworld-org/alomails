//! Who this tenant may never mail again (alo Campaigns, ADR 0044 §2, wave
//! C1.3) — absolute, tenant-wide, and enforced in SQL.
//!
//! ADR 0044 §2: *suppression is absolute and global to the tenant. An
//! unsubscribe, a hard bounce or a complaint removes somebody from every future
//! send, and no segment, import or re-upload can bring them back.*
//!
//! This module owns the record. The enforcement is one file over, inside
//! [`campaign_audience`](crate::campaign_audience)'s `people_cte`, where the
//! recipients query carries `suppressed_at IS NULL` in the same `WHERE` as the
//! consent gate — because a rule the sender applies is not absolute. That is
//! the whole of queue item C1.3: not "the sender checks", but "there is nothing
//! for the sender to check".
//!
//! ## Why this is a table of state, where consent is a table of events
//!
//! [`campaign_consent`](crate::campaign_consent) keeps every statement ever
//! given, because "how do we know they agreed" is answered by the wording of a
//! particular agreement. Suppression asks one question — *is this address
//! suppressed* — and that question must have exactly one answer, so there is
//! exactly one row per address.
//!
//! **The first reason stands.** Suppressing an already-suppressed person is a
//! no-op that keeps the original record
//! ([`suppress_campaign_address`](crate::TenantStore::suppress_campaign_address)
//! is idempotent), because the earliest reason is the moment the tenant lost
//! the right to mail them. A hard bounce arriving three months after somebody
//! unsubscribed must not rewrite the record into "their mailbox was full":
//! that reads as a technical problem somebody might try to fix, and the person
//! asked to be left alone.
//!
//! ## What is deliberately absent
//!
//! - **No way to lift one.** No `unsuppress`, no `lifted_at`, no delete. ADR
//!   0044 §2 says *no segment, import or re-upload can bring them back*, and an
//!   API that can take a row away is an API a bulk importer will eventually be
//!   pointed at. Somebody who suppressed themselves by mistake and wants back
//!   in gives fresh consent through the site form like anyone else — which is
//!   evidence, where a tenant deleting the row is not.
//! - **No `recorded_by`.** These methods live on
//!   [`TenantStore`](crate::TenantStore) rather than
//!   [`AccountStore`](crate::AccountStore) precisely because the loudest source
//!   of suppression has no logged-in colleague behind it: the one-click
//!   unsubscribe endpoint (RFC 8058, queue item C2s.2) works with no account
//!   and no login. A column that would be NULL for the case that matters most
//!   is not provenance. Who acted is answered by [`SuppressionReason`] and
//!   `source_ref`.
//!
//! Recording consent for a suppressed address is **not** refused — the record
//! is kept, because an import claiming an agreement is itself evidence worth
//! having — and it grants nothing. `tests/campaign_suppression_tenancy.rs`
//! proves that an import cannot resurrect a suppressed address.
//!
//! Nothing in this module sends anything.

use time::{Duration, OffsetDateTime};

use crate::campaign_audience::normalise_address;
use crate::error::{Result, StoreError};
use crate::id::CampaignSuppressionId;
use crate::store::TenantStore;

/// The longest `source_ref` — a send id, a bounce report reference, a note of
/// which conversation.
pub const SUPPRESSION_SOURCE_REF_MAX: usize = 200;

/// The most suppressions one read returns.
///
/// The list is the answer to "who have we lost, and why", which a screen shows
/// a window of; nothing in this crate ever holds a tenant's whole suppression
/// list in memory.
pub const SUPPRESSION_PAGE_MAX: i64 = 500;

/// How far ahead of this server's clock a suppression may be dated — the same
/// tolerance [`campaign_consent`](crate::campaign_consent) allows, for the same
/// reason: a few seconds of a caller's clock skew is not a lie, next year is.
const SUPPRESSION_FUTURE_SKEW_MINUTES: i64 = 5;

/// Why somebody may never be mailed again.
///
/// The three ADR 0044 §2 names, plus one. `Manual` exists because the person
/// who phones and asks to be taken off the list is real, and recording that as
/// an [`Unsubscribe`](Self::Unsubscribe) would put it into the number a sending
/// reputation is judged on — a complaint rate that counts phone calls as clicks
/// is a lie told to ourselves rather than to a regulator, which makes it worse
/// rather than better.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SuppressionReason {
    /// They asked to stop — the one-click link in the mail, or the landing page
    /// behind it (RFC 8058; queue item C2s.2).
    Unsubscribe,
    /// The address does not exist. A permanent SMTP failure, not a full mailbox
    /// or a greylist: soft failures retry, and only a settled one suppresses
    /// (ADR 0044 §4).
    HardBounce,
    /// They pressed the spam button and their provider told us. The most
    /// expensive of the four, because the signal has already been sent to the
    /// people who decide whether our mail is delivered at all.
    Complaint,
    /// A colleague recording that somebody asked to be removed by some other
    /// route — at the counter, on the phone, in a reply.
    Manual,
}

impl SuppressionReason {
    /// The stored token. Stable: it is written into rows that outlive releases
    /// and is read back by the screen.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unsubscribe => "unsubscribe",
            Self::HardBounce => "hard_bounce",
            Self::Complaint => "complaint",
            Self::Manual => "manual",
        }
    }

    /// Parses a stored token, or `None` when it is not one of ours.
    pub fn parse(token: &str) -> Option<Self> {
        match token {
            "unsubscribe" => Some(Self::Unsubscribe),
            "hard_bounce" => Some(Self::HardBounce),
            "complaint" => Some(Self::Complaint),
            "manual" => Some(Self::Manual),
            _ => None,
        }
    }

    /// Whether this reason came from the recipient themselves rather than from
    /// the machinery.
    ///
    /// The distinction the tenant's own numbers turn on: an unsubscribe and a
    /// complaint are a person's decision about this tenant's mail, while a hard
    /// bounce is a mailbox that no longer exists and says nothing about whether
    /// the mail was wanted.
    pub fn is_a_persons_decision(&self) -> bool {
        matches!(self, Self::Unsubscribe | Self::Complaint | Self::Manual)
    }
}

/// One address this tenant may never mail again, as it is stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CampaignSuppression {
    /// The record's handle — what a screen links to when it names somebody as
    /// excluded and says why.
    pub id: CampaignSuppressionId,
    /// The person, normalised — their identity across every campaign query.
    pub address: String,
    pub reason: SuppressionReason,
    /// Which send, which bounce report, which conversation. `None` where there
    /// is honestly nothing to point at.
    pub source_ref: Option<String>,
    /// When it happened: when they clicked, when the mail bounced, when they
    /// phoned.
    pub occurred_at: OffsetDateTime,
    /// When this workspace was told.
    pub recorded_at: OffsetDateTime,
}

/// Why somebody is not a recipient, carried beside them in the audience so the
/// screen (C1.5) can name the exclusion rather than merely apply it.
///
/// The mirror of [`ConsentEvidence`](crate::ConsentEvidence): an
/// [`AudienceMember`](crate::AudienceMember) whose `suppression` is `Some` is a
/// person this tenant may not mail whatever their consent record says, and the
/// record id is what to read for the rest of the story.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuppressionEvidence {
    /// The record this comes from.
    pub record: CampaignSuppressionId,
    pub reason: SuppressionReason,
    /// When they stopped being reachable.
    pub occurred_at: OffsetDateTime,
}

/// What to suppress, and why. A struct rather than four positional arguments,
/// because the argument that gets swapped is always the one nobody reads back.
#[derive(Debug, Clone)]
pub struct NewSuppression<'a> {
    /// The person's address in whatever casing it arrived in; normalised here.
    pub address: &'a str,
    pub reason: SuppressionReason,
    /// Which send, which bounce report, which conversation.
    pub source_ref: Option<&'a str>,
    /// When it happened. `None` means now — correct for a click being handled
    /// this second, wrong for a bounce report processed hours later, which
    /// knows its own time.
    pub occurred_at: Option<OffsetDateTime>,
}

/// The write: suppress, or leave the existing record exactly as it is.
///
/// `ON CONFLICT DO NOTHING` and then a `UNION ALL` against the table, rather
/// than `DO UPDATE`: the row is never rewritten, so the earliest reason
/// survives every later event. The second branch is guarded by `NOT EXISTS
/// (SELECT 1 FROM inserted)` so exactly one of the two produces a row, and the
/// caller always gets the record that is actually in force — which is the whole
/// point of an idempotent suppress: the answer to "so are they suppressed, and
/// why" must be the same whether this call was the one that did it or not.
fn suppress_sql() -> &'static str {
    "WITH inserted AS ( \
       INSERT INTO campaign_suppression \
           (tenant_id, id, address, reason, source_ref, occurred_at) \
       VALUES ($1, $2, $3, $4, $5, $6) \
       ON CONFLICT (tenant_id, address) DO NOTHING \
       RETURNING id, address, reason, source_ref, occurred_at, recorded_at \
     ) \
     SELECT id, address, reason, source_ref, occurred_at, recorded_at FROM inserted \
     UNION ALL \
     SELECT id, address, reason, source_ref, occurred_at, recorded_at \
       FROM campaign_suppression \
      WHERE tenant_id = $1 AND address = $3 \
        AND NOT EXISTS (SELECT 1 FROM inserted)"
}

/// The SQL of one person's suppression, if any.
fn lookup_sql() -> &'static str {
    "SELECT id, address, reason, source_ref, occurred_at, recorded_at \
       FROM campaign_suppression \
      WHERE tenant_id = $1 AND address = $2"
}

/// The SQL of the list, freshest first.
fn list_sql() -> &'static str {
    "SELECT id, address, reason, source_ref, occurred_at, recorded_at \
       FROM campaign_suppression \
      WHERE tenant_id = $1 \
      ORDER BY occurred_at DESC, address \
      LIMIT $2"
}

/// Every statement this module can issue.
///
/// The same list [`crate::campaign_audience`] and
/// [`crate::campaign_consent`] keep, and for the same reason: the promise that
/// no campaign query reads the per-user address book is checked against the
/// strings rather than asserted in a comment.
#[cfg(test)]
fn all_sql() -> Vec<&'static str> {
    vec![suppress_sql(), lookup_sql(), list_sql()]
}

/// A row as any of the three queries returns it.
type SuppressionRow = (
    String,
    String,
    String,
    Option<String>,
    OffsetDateTime,
    OffsetDateTime,
);

/// Turns a stored row into a record, refusing a reason this build does not
/// know.
///
/// A token we do not recognise is a decode failure rather than a dropped field.
/// This module writes those strings itself, so an unknown one means the schema
/// moved under the enum — and the one outcome worse than an error here is a
/// suppressed person being reported with their reason quietly blanked, because
/// the obvious next step is to wonder whether the row belongs there at all.
fn row_to_suppression(row: SuppressionRow) -> Result<CampaignSuppression> {
    let reason = SuppressionReason::parse(&row.2).ok_or_else(|| {
        StoreError::Db(sqlx::Error::Decode(
            "campaign suppression names a reason this build does not know".into(),
        ))
    })?;
    Ok(CampaignSuppression {
        id: CampaignSuppressionId::new(row.0),
        address: row.1,
        reason,
        source_ref: row.3,
        occurred_at: row.4,
        recorded_at: row.5,
    })
}

/// The validated shape of a [`NewSuppression`], ready to bind.
struct Validated {
    address: String,
    source_ref: Option<String>,
    occurred_at: OffsetDateTime,
}

/// Checks one submission, once, in one place — separated from the write so the
/// rules are testable without a database, and so there is exactly one of them.
fn validate(suppression: &NewSuppression<'_>, now: OffsetDateTime) -> Result<Validated> {
    let address = normalise_address(suppression.address).ok_or_else(|| {
        StoreError::Validation(
            "suppression is held against an address, and this is not one".to_owned(),
        )
    })?;

    let source_ref = suppression
        .source_ref
        .map(str::trim)
        .filter(|r| !r.is_empty());
    if source_ref.is_some_and(|r| r.chars().count() > SUPPRESSION_SOURCE_REF_MAX) {
        return Err(StoreError::Validation(format!(
            "a suppression source reference fits in {SUPPRESSION_SOURCE_REF_MAX} characters"
        )));
    }

    let occurred_at = suppression.occurred_at.unwrap_or(now);
    if occurred_at > now + Duration::minutes(SUPPRESSION_FUTURE_SKEW_MINUTES) {
        return Err(StoreError::Validation(
            "suppression cannot be dated after it happened".to_owned(),
        ));
    }

    Ok(Validated {
        address,
        source_ref: source_ref.map(str::to_owned),
        occurred_at,
    })
}

impl TenantStore {
    /// Suppresses an address for this whole tenant — absolutely, and for good.
    ///
    /// **Idempotent, and the first reason stands.** Calling this for somebody
    /// already suppressed writes nothing and returns the record already in
    /// force, so a bounce report replayed twice, or a hard bounce arriving
    /// after the person had already unsubscribed, cannot rewrite why they are
    /// gone.
    ///
    /// On [`TenantStore`] rather than
    /// [`AccountStore`](crate::AccountStore) deliberately: an unsubscribe is
    /// not a colleague's action and the endpoint that will call this (queue
    /// item C2s.2) has no account at all. Suppression is a fact about the
    /// tenant, not about a user's mailbox.
    ///
    /// There is no method that undoes this, and that is ADR 0044 §2 rather than
    /// an omission — see the module docs.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the address is not one, the source
    /// reference is too long, or the suppression is dated in the future;
    /// [`StoreError::Db`] on failure.
    pub async fn suppress_campaign_address(
        &self,
        suppression: &NewSuppression<'_>,
    ) -> Result<CampaignSuppression> {
        let valid = validate(suppression, OffsetDateTime::now_utc())?;
        let id = CampaignSuppressionId::generate();
        let row: SuppressionRow = sqlx::query_as(suppress_sql())
            .bind(self.tenant().as_str())
            .bind(id.as_str())
            .bind(&valid.address)
            .bind(suppression.reason.as_str())
            .bind(valid.source_ref.as_deref())
            .bind(valid.occurred_at)
            .fetch_one(self.pool())
            .await
            .map_err(StoreError::Db)?;
        row_to_suppression(row)
    }

    /// Whether this tenant may mail an address, and if not, why not.
    ///
    /// `None` means there is no suppression — which is not the same as "may be
    /// mailed": consent is a separate question, answered by
    /// [`campaign_recipients`](crate::AccountStore::campaign_recipients), which
    /// applies both rules in one query so the two cannot be asked apart.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the address is not one;
    /// [`StoreError::Db`] on failure.
    pub async fn campaign_suppression_for(
        &self,
        address: &str,
    ) -> Result<Option<CampaignSuppression>> {
        let address = normalise_address(address).ok_or_else(|| {
            StoreError::Validation(
                "suppression is held against an address, and this is not one".to_owned(),
            )
        })?;
        let row: Option<SuppressionRow> = sqlx::query_as(lookup_sql())
            .bind(self.tenant().as_str())
            .bind(&address)
            .fetch_optional(self.pool())
            .await
            .map_err(StoreError::Db)?;
        row.map(row_to_suppression).transpose()
    }

    /// Everybody this tenant has suppressed, freshest first — the answer to
    /// "who have we lost, and why".
    ///
    /// # Errors
    /// [`StoreError::Validation`] when `limit` is outside
    /// `1..=`[`SUPPRESSION_PAGE_MAX`]; [`StoreError::Db`] on failure.
    pub async fn campaign_suppressions(&self, limit: i64) -> Result<Vec<CampaignSuppression>> {
        if !(1..=SUPPRESSION_PAGE_MAX).contains(&limit) {
            return Err(StoreError::Validation(format!(
                "a page of suppressions is between 1 and {SUPPRESSION_PAGE_MAX} people"
            )));
        }
        let rows: Vec<SuppressionRow> = sqlx::query_as(list_sql())
            .bind(self.tenant().as_str())
            .bind(limit)
            .fetch_all(self.pool())
            .await
            .map_err(StoreError::Db)?;
        rows.into_iter().map(row_to_suppression).collect()
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

    fn new() -> NewSuppression<'static> {
        NewSuppression {
            address: "Ann@Lead.TEST",
            reason: SuppressionReason::Unsubscribe,
            source_ref: None,
            occurred_at: None,
        }
    }

    #[test]
    fn no_query_in_this_module_can_read_the_per_user_address_book() {
        for sql in all_sql() {
            assert!(
                !identifiers(sql).contains(&"contacts"),
                "a campaign suppression query names the per-user address book: {sql}"
            );
        }
    }

    #[test]
    fn every_query_is_scoped_to_one_tenant() {
        for sql in all_sql() {
            assert!(
                sql.contains("tenant_id = $1"),
                "a suppression query without a tenant: {sql}"
            );
        }
    }

    #[test]
    fn nothing_in_this_module_can_take_a_suppression_away() {
        // ADR 0044 §2: "no segment, import or re-upload can bring them back."
        // An API that can remove a row is an API a bulk importer is eventually
        // pointed at, so the absence is checked rather than trusted to review.
        for sql in all_sql() {
            let statements = identifiers(sql);
            for forbidden in ["DELETE", "delete", "UPDATE", "update"] {
                assert!(
                    !statements.contains(&forbidden),
                    "a suppression can be lifted: {sql}"
                );
            }
        }
    }

    #[test]
    fn suppressing_somebody_twice_keeps_the_first_reason() {
        // `DO NOTHING` rather than `DO UPDATE`: the row is never rewritten, so
        // a hard bounce months after an unsubscribe cannot turn "they asked to
        // stop" into "their mailbox was full" — which reads as a technical
        // problem somebody might try to fix.
        let sql = suppress_sql();
        assert!(sql.contains("ON CONFLICT (tenant_id, address) DO NOTHING"));
        assert!(
            !sql.contains("DO UPDATE"),
            "the record in force must survive a second suppression: {sql}"
        );
        // And the caller still gets the record that is in force, from the
        // second branch, exactly when the insert did nothing.
        assert!(sql.contains("NOT EXISTS (SELECT 1 FROM inserted)"));
    }

    #[test]
    fn a_reason_token_survives_a_round_trip_and_an_unknown_one_is_not_guessed() {
        for reason in [
            SuppressionReason::Unsubscribe,
            SuppressionReason::HardBounce,
            SuppressionReason::Complaint,
            SuppressionReason::Manual,
        ] {
            assert_eq!(SuppressionReason::parse(reason.as_str()), Some(reason));
        }
        for unknown in ["contacts", "", "Unsubscribe", "bounce", "soft_bounce"] {
            assert_eq!(SuppressionReason::parse(unknown), None);
        }
    }

    #[test]
    fn a_bounce_is_not_a_person_changing_their_mind() {
        assert!(SuppressionReason::Unsubscribe.is_a_persons_decision());
        assert!(SuppressionReason::Complaint.is_a_persons_decision());
        assert!(SuppressionReason::Manual.is_a_persons_decision());
        assert!(!SuppressionReason::HardBounce.is_a_persons_decision());
    }

    #[test]
    fn a_suppression_carries_the_address_folded_to_one_identity() {
        let now = OffsetDateTime::UNIX_EPOCH;
        let valid = validate(&new(), now).unwrap_or_else(|e| panic!("refused a good one: {e:?}"));
        assert_eq!(valid.address, "ann@lead.test");
        assert_eq!(valid.occurred_at, now);
        assert_eq!(valid.source_ref, None);
    }

    #[test]
    fn an_address_nobody_could_be_mailed_at_cannot_be_suppressed() {
        // Not pedantry: a suppression row that does not join the audience is
        // somebody who asked to stop and is still being mailed.
        let now = OffsetDateTime::UNIX_EPOCH;
        for junk in ["", "   ", "n/a", "ask reception", "ann@localhost"] {
            let candidate = NewSuppression {
                address: junk,
                ..new()
            };
            assert!(
                matches!(validate(&candidate, now), Err(StoreError::Validation(_))),
                "accepted a suppression for {junk:?}"
            );
        }
    }

    #[test]
    fn a_source_reference_is_trimmed_and_bounded() {
        let now = OffsetDateTime::UNIX_EPOCH;
        let padded = NewSuppression {
            source_ref: Some("  bounce-report-2026-08  "),
            ..new()
        };
        assert_eq!(
            validate(&padded, now)
                .unwrap_or_else(|e| panic!("{e:?}"))
                .source_ref
                .as_deref(),
            Some("bounce-report-2026-08")
        );
        let blank = NewSuppression {
            source_ref: Some("   "),
            ..new()
        };
        assert_eq!(
            validate(&blank, now)
                .unwrap_or_else(|e| panic!("{e:?}"))
                .source_ref,
            None,
            "a reference nobody filled in is absent rather than blank"
        );
        let long = "x".repeat(SUPPRESSION_SOURCE_REF_MAX + 1);
        let overlong = NewSuppression {
            source_ref: Some(&long),
            ..new()
        };
        assert!(matches!(
            validate(&overlong, now),
            Err(StoreError::Validation(_))
        ));
    }

    #[test]
    fn suppression_cannot_be_dated_after_it_happened() {
        let now = OffsetDateTime::UNIX_EPOCH + Duration::days(365);
        let ahead = NewSuppression {
            occurred_at: Some(now + Duration::hours(1)),
            ..new()
        };
        assert!(matches!(
            validate(&ahead, now),
            Err(StoreError::Validation(_))
        ));
        // Clock skew is not a lie.
        let barely = NewSuppression {
            occurred_at: Some(now + Duration::minutes(SUPPRESSION_FUTURE_SKEW_MINUTES - 1)),
            ..new()
        };
        assert!(validate(&barely, now).is_ok());
        // A bounce report is processed after the fact, and the row says when
        // the bounce happened rather than when we got round to reading it.
        let earlier = now - Duration::hours(6);
        let processed_late = NewSuppression {
            reason: SuppressionReason::HardBounce,
            occurred_at: Some(earlier),
            ..new()
        };
        assert_eq!(
            validate(&processed_late, now)
                .unwrap_or_else(|e| panic!("{e:?}"))
                .occurred_at,
            earlier
        );
    }

    #[test]
    fn a_row_naming_an_unknown_reason_fails_the_read_rather_than_losing_it() {
        let row = (
            "sup".to_owned(),
            "ann@lead.test".to_owned(),
            "changed_their_mind".to_owned(),
            None,
            OffsetDateTime::UNIX_EPOCH,
            OffsetDateTime::UNIX_EPOCH,
        );
        assert!(matches!(row_to_suppression(row), Err(StoreError::Db(_))));
    }

    #[test]
    fn a_stored_row_reads_back_whole() {
        let row = (
            "sup".to_owned(),
            "ann@lead.test".to_owned(),
            "hard_bounce".to_owned(),
            Some("bounce-report-2026-08".to_owned()),
            OffsetDateTime::UNIX_EPOCH,
            OffsetDateTime::UNIX_EPOCH + Duration::hours(2),
        );
        let stored = row_to_suppression(row).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(stored.id, CampaignSuppressionId::new("sup"));
        assert_eq!(stored.reason, SuppressionReason::HardBounce);
        assert_eq!(stored.source_ref.as_deref(), Some("bounce-report-2026-08"));
        assert!(stored.occurred_at < stored.recorded_at);
    }
}
