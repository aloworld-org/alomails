//! Fewer, rather than only none (alo Campaigns, ADR 0044 §3, wave C2s.2) — one
//! person declining one kind of mail, without declining all of it.
//!
//! Queue item C2s.2: *offering **fewer** rather than only none — this kind of
//! mail, or all of it. One click either way, no confirmation maze. A recipient
//! offered only all-or-nothing presses the spam button instead, and that is the
//! signal that ends a sending reputation.*
//!
//! That sentence is the whole module. All-or-nothing is not a smaller feature
//! than a preference centre, it is a worse one: somebody who wants the invoices
//! but not the newsletter, offered only *stop everything*, has two options —
//! keep receiving what they do not want, or take the option that costs the
//! sender its delivery. The spam button is one press; a reply asking to be
//! taken off one list is a paragraph nobody writes.
//!
//! ## Why this is a table of its own
//!
//! An opt-out is a fact about a **person**, and it has to outlive every token,
//! every send and every campaign: somebody who declines the newsletter in 2026
//! is still declining it in 2029, when the link they used is a row nobody reads
//! and the campaign it came from has been deleted. One person can decline
//! several kinds, which is the point — a flag on the token or on the audience
//! could hold neither.
//!
//! The *topic* itself is a property of the send, and lives on the token
//! ([`crate::campaign_unsubscribe`]) because the token row is the only thing
//! that knows which send a link came from.
//!
//! ## The fold, and the failure it prevents
//!
//! [`normalise_topic`] lowercases, trims and collapses inner whitespace, and the
//! stored topic is always the folded form — the schema holds it to that. The
//! token keeps the label as the sender wrote it, because a human reads it; this
//! table keeps the fold, because a query compares it. Without the fold, somebody
//! who declined `Newsletter` and is later sent `newsletter` has unsubscribed
//! from one copy of themselves, which is exactly what ADR 0044's *there is no
//! list* claim exists to make impossible — the same argument, and the same
//! shape, as the address fold in [`crate::campaign_audience`].
//!
//! ## What is deliberately absent
//!
//! - **No way to lift one**, and no update path: the same discipline as
//!   [`crate::campaign_suppression`]. The first decision stands. Somebody who
//!   changes their mind says so through a form like anyone else, which is
//!   evidence, where a tenant deleting the row is not.
//! - **No listing by topic.** Nothing answers *who declined the newsletter*.
//!   That is a list of people, and this queue's argument is that there is no
//!   list; the only read is *what has this person declined*, keyed by the
//!   address the caller already holds.
//!
//! ## What is not built yet, and where it goes
//!
//! **Nothing reads these rows to decide who gets a message**, because nothing
//! yet builds a message that names a kind — the campaign record is queue item
//! C3.1 and the per-recipient send record is C5m.1. The exclusion belongs in
//! [`campaign_audience`](crate::campaign_audience)'s `Reach` predicate, beside
//! consent and suppression, on the day a send can say which topic it is.
//! Threading a topic parameter through four queries now, for a caller that does
//! not exist, is the same guess this queue refused when it left
//! `received_campaign_id` out of the segments migration.
//!
//! Nothing in this module sends anything.

use time::{Duration, OffsetDateTime};

use crate::campaign_audience::normalise_address;
use crate::error::{Result, StoreError};
use crate::id::CampaignTopicOptOutId;
use crate::store::TenantStore;

/// The longest a topic label may be, folded or as written.
///
/// It goes on a landing page and into a sentence a recipient reads under
/// pressure — *stop sending me `<topic>`* — not into a report. A label longer
/// than this is a paragraph, and a paragraph is a maze.
pub const TOPIC_MAX: usize = 80;

/// The longest `source_ref` — matched to
/// [`SUPPRESSION_SOURCE_REF_MAX`](crate::campaign_suppression::SUPPRESSION_SOURCE_REF_MAX),
/// because both columns hold the same thing: the *record* id of the unsubscribe
/// token somebody used.
pub const TOPIC_SOURCE_REF_MAX: usize = 200;

/// Folds a topic label to the one form that is ever compared, or `None` when it
/// is not a label at all.
///
/// Trims, collapses runs of inner whitespace to one space, and lowercases.
/// Collapsing the inside as well as the ends is not tidiness: `"product
/// updates"` typed with two spaces by one send and one by the next would be two
/// topics, so a person who declined the first would be mailed the second.
///
/// Unlike [`normalise_address`], this **produces** the value that is stored and
/// bound, because a topic is our own short label rather than an identifier a
/// database collation has an opinion about — and the schema asserts the same
/// fold on the way in, so a row that skipped this function cannot exist.
pub fn normalise_topic(raw: &str) -> Option<String> {
    let folded = raw
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    if folded.is_empty() || folded.chars().count() > TOPIC_MAX {
        return None;
    }
    Some(folded)
}

/// One kind of mail one person has declined, as it is stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CampaignTopicOptOut {
    /// The record's handle — safe to log, and what a screen links to when it
    /// names somebody as having declined a kind of mail.
    pub id: CampaignTopicOptOutId,
    /// The person, normalised — their identity across every campaign query.
    pub address: String,
    /// The kind of mail, folded ([`normalise_topic`]).
    pub topic: String,
    /// Which link they used, as the unsubscribe token's **record** id, or
    /// `None` where a colleague recorded the decision by some other route.
    pub source_ref: Option<String>,
    /// When they decided.
    pub occurred_at: OffsetDateTime,
    /// When this workspace was told.
    pub recorded_at: OffsetDateTime,
}

/// What to decline, and on whose behalf. A struct rather than four positional
/// arguments, because the argument that gets swapped is always the one nobody
/// reads back — and swapped here means declining the wrong kind of mail for the
/// wrong person, with no way to undo it.
#[derive(Debug, Clone)]
pub struct NewTopicOptOut<'a> {
    /// The person's address in whatever casing it arrived in; normalised here.
    pub address: &'a str,
    /// The kind of mail, as written; folded here.
    pub topic: &'a str,
    /// The unsubscribe token's record id, or a note of which conversation.
    pub source_ref: Option<&'a str>,
    /// When they decided. `None` means now — correct for a click being handled
    /// this second, wrong for a decision a colleague is relaying.
    pub occurred_at: Option<OffsetDateTime>,
}

/// How far ahead of this server's clock a decision may be dated — the same
/// tolerance [`crate::campaign_consent`] and [`crate::campaign_suppression`]
/// allow, for the same reason: a few seconds of clock skew is not a lie, next
/// year is.
const TOPIC_FUTURE_SKEW_MINUTES: i64 = 5;

/// The write: decline, or leave the existing decision exactly as it is.
///
/// The shape [`crate::campaign_suppression`] uses, and for the same reason.
/// `ON CONFLICT DO NOTHING` then a `UNION ALL` guarded by `NOT EXISTS (SELECT 1
/// FROM inserted)`, so exactly one branch produces a row and the caller always
/// gets the decision actually in force. Pressing the same link twice — which
/// every recipient who is not sure it worked will do — must answer the same
/// thing both times, and must not restamp the date somebody decided.
fn decline_sql() -> &'static str {
    "WITH inserted AS ( \
       INSERT INTO campaign_topic_optouts \
           (tenant_id, id, address, topic, source_ref, occurred_at) \
       VALUES ($1, $2, $3, $4, $5, $6) \
       ON CONFLICT (tenant_id, address, topic) DO NOTHING \
       RETURNING id, address, topic, source_ref, occurred_at, recorded_at \
     ) \
     SELECT id, address, topic, source_ref, occurred_at, recorded_at FROM inserted \
     UNION ALL \
     SELECT id, address, topic, source_ref, occurred_at, recorded_at \
       FROM campaign_topic_optouts \
      WHERE tenant_id = $1 AND address = $3 AND topic = $4 \
        AND NOT EXISTS (SELECT 1 FROM inserted)"
}

/// The SQL of *what has this person declined* — the only read.
///
/// Keyed on the address the caller already holds. There is deliberately no
/// query the other way round: *who declined the newsletter* is a list of
/// people, and a table that can produce one is a table somebody exports.
fn declined_sql() -> &'static str {
    "SELECT id, address, topic, source_ref, occurred_at, recorded_at \
       FROM campaign_topic_optouts \
      WHERE tenant_id = $1 AND address = $2 \
      ORDER BY topic"
}

/// Every statement this module can issue.
///
/// The list [`crate::campaign_audience`], [`crate::campaign_consent`],
/// [`crate::campaign_suppression`] and [`crate::campaign_unsubscribe`] all keep,
/// so the promise that no campaign query reads the per-user address book is
/// checked against the strings rather than asserted in a comment.
#[cfg(test)]
fn all_sql() -> Vec<&'static str> {
    vec![decline_sql(), declined_sql()]
}

/// A row as either query returns it.
type OptOutRow = (
    String,
    String,
    String,
    Option<String>,
    OffsetDateTime,
    OffsetDateTime,
);

/// Turns a stored row into a record.
fn row_to_optout(row: OptOutRow) -> CampaignTopicOptOut {
    CampaignTopicOptOut {
        id: CampaignTopicOptOutId::new(row.0),
        address: row.1,
        topic: row.2,
        source_ref: row.3,
        occurred_at: row.4,
        recorded_at: row.5,
    }
}

/// The validated shape of a [`NewTopicOptOut`], ready to bind.
struct Validated {
    address: String,
    topic: String,
    source_ref: Option<String>,
    occurred_at: OffsetDateTime,
}

/// Checks one decision, once, in one place — separated from the write so the
/// rules are testable without a database, and so there is exactly one of them.
fn validate(optout: &NewTopicOptOut<'_>, now: OffsetDateTime) -> Result<Validated> {
    let address = normalise_address(optout.address).ok_or_else(|| {
        StoreError::Validation(
            "a preference is held against an address, and this is not one".to_owned(),
        )
    })?;

    let topic = normalise_topic(optout.topic).ok_or_else(|| {
        StoreError::Validation(format!(
            "a kind of mail is named in 1 to {TOPIC_MAX} characters"
        ))
    })?;

    let source_ref = optout.source_ref.map(str::trim).filter(|r| !r.is_empty());
    if source_ref.is_some_and(|r| r.chars().count() > TOPIC_SOURCE_REF_MAX) {
        return Err(StoreError::Validation(format!(
            "a preference source reference fits in {TOPIC_SOURCE_REF_MAX} characters"
        )));
    }

    let occurred_at = optout.occurred_at.unwrap_or(now);
    if occurred_at > now + Duration::minutes(TOPIC_FUTURE_SKEW_MINUTES) {
        return Err(StoreError::Validation(
            "a preference cannot be dated after it was decided".to_owned(),
        ));
    }

    Ok(Validated {
        address,
        topic,
        source_ref: source_ref.map(str::to_owned),
        occurred_at,
    })
}

impl TenantStore {
    /// Records that one person no longer wants one kind of this tenant's mail.
    ///
    /// **Idempotent, and the first decision stands.** Pressing the same link
    /// twice — which everybody who is not sure it worked does — writes nothing
    /// the second time and answers with the decision already in force, so the
    /// date somebody decided is never restamped.
    ///
    /// This is the *narrower* half of the unsubscribe. The wider one is
    /// [`suppress_campaign_address`](Self::suppress_campaign_address), which
    /// ends everything; offering only that is what makes a recipient press the
    /// spam button instead (ADR 0044 §3).
    ///
    /// On [`TenantStore`] rather than [`AccountStore`](crate::AccountStore) for
    /// the reason [`crate::campaign_suppression`] gives: the endpoint that calls
    /// this has no account and no login at all.
    ///
    /// There is no method that undoes this — see the module docs.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the address is not one, the topic is
    /// blank or too long, the source reference is too long, or the decision is
    /// dated in the future; [`StoreError::Db`] on failure.
    pub async fn decline_campaign_topic(
        &self,
        optout: &NewTopicOptOut<'_>,
    ) -> Result<CampaignTopicOptOut> {
        let valid = validate(optout, OffsetDateTime::now_utc())?;
        let id = CampaignTopicOptOutId::generate();
        let row: OptOutRow = sqlx::query_as(decline_sql())
            .bind(self.tenant().as_str())
            .bind(id.as_str())
            .bind(&valid.address)
            .bind(&valid.topic)
            .bind(valid.source_ref.as_deref())
            .bind(valid.occurred_at)
            .fetch_one(self.pool())
            .await
            .map_err(StoreError::Db)?;
        Ok(row_to_optout(row))
    }

    /// Every kind of mail this person has declined, in topic order.
    ///
    /// The only read, and it is keyed on an address the caller already holds:
    /// there is no query for *who declined the newsletter*, because that is a
    /// list of people (see the module docs).
    ///
    /// An empty answer is not "they want everything" — consent and suppression
    /// are separate questions, answered together by
    /// [`campaign_recipients`](crate::AccountStore::campaign_recipients).
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the address is not one;
    /// [`StoreError::Db`] on failure.
    pub async fn campaign_topics_declined_by(
        &self,
        address: &str,
    ) -> Result<Vec<CampaignTopicOptOut>> {
        let address = normalise_address(address).ok_or_else(|| {
            StoreError::Validation(
                "a preference is held against an address, and this is not one".to_owned(),
            )
        })?;
        let rows: Vec<OptOutRow> = sqlx::query_as(declined_sql())
            .bind(self.tenant().as_str())
            .bind(&address)
            .fetch_all(self.pool())
            .await
            .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(row_to_optout).collect())
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

    fn new() -> NewTopicOptOut<'static> {
        NewTopicOptOut {
            address: "Ann@Lead.TEST",
            topic: "Newsletter",
            source_ref: None,
            occurred_at: None,
        }
    }

    #[test]
    fn no_query_in_this_module_can_read_the_per_user_address_book() {
        for sql in all_sql() {
            assert!(
                !identifiers(sql).contains(&"contacts"),
                "a topic preference query names the per-user address book: {sql}"
            );
        }
    }

    #[test]
    fn every_query_is_scoped_to_one_tenant() {
        for sql in all_sql() {
            assert!(
                sql.contains("tenant_id = $1"),
                "a topic preference query without a tenant: {sql}"
            );
        }
        assert!(
            decline_sql().starts_with(
                "WITH inserted AS ( INSERT INTO campaign_topic_optouts (tenant_id, id, "
            ),
            "the insert must lead with the tenant it is bound to: {}",
            decline_sql()
        );
        assert!(
            decline_sql().contains("VALUES ($1, $2, "),
            "the tenant is bound, not inlined: {}",
            decline_sql()
        );
    }

    #[test]
    fn nothing_in_this_module_can_take_a_preference_away() {
        // The same rule `campaign_suppression` holds itself to. A person's
        // decision about what they want is not a row a bulk importer gets to
        // tidy up, so the absence is checked against the SQL rather than
        // trusted to review — a convenience "resubscribe" added next year fails
        // a test instead of shipping.
        for sql in all_sql() {
            let statements = identifiers(sql);
            for forbidden in ["DELETE", "delete", "UPDATE", "update"] {
                assert!(
                    !statements.contains(&forbidden),
                    "a topic preference can be lifted: {sql}"
                );
            }
        }
    }

    #[test]
    fn nothing_answers_who_declined_a_kind_of_mail() {
        // The read is keyed on an address the caller already holds. A query the
        // other way round — `WHERE topic = $2` — would answer "who is off the
        // newsletter", which is a list of people, and this queue's whole
        // argument is that there is no list.
        let sql = declined_sql();
        assert!(sql.contains("WHERE tenant_id = $1 AND address = $2"));
        assert!(
            !sql.contains("topic = $2"),
            "a preference can be read by topic: {sql}"
        );
    }

    #[test]
    fn pressing_the_link_twice_keeps_the_first_decision() {
        // Everybody who is not sure it worked presses it again, and the second
        // press must not restamp the date they decided.
        let sql = decline_sql();
        assert!(sql.contains("ON CONFLICT (tenant_id, address, topic) DO NOTHING"));
        assert!(
            !sql.contains("DO UPDATE"),
            "the decision in force must survive a second press: {sql}"
        );
        assert!(sql.contains("NOT EXISTS (SELECT 1 FROM inserted)"));
    }

    #[test]
    fn one_kind_of_mail_is_one_topic_however_it_is_spelled() {
        // The failure the fold prevents: declining "Newsletter" and then being
        // sent "newsletter" is unsubscribing from one copy of yourself.
        for spelling in [
            "Newsletter",
            "  newsletter ",
            "NEWSLETTER",
            "\tNewsLetter\n",
        ] {
            assert_eq!(
                normalise_topic(spelling),
                Some("newsletter".to_owned()),
                "{spelling:?} became a second topic"
            );
        }
        // And the inside as well as the ends: two spaces from one send and one
        // from the next would otherwise be two kinds of mail.
        assert_eq!(
            normalise_topic("Product   updates"),
            normalise_topic("product updates")
        );
    }

    #[test]
    fn a_label_nobody_could_read_is_not_a_kind_of_mail() {
        for junk in ["", "   ", "\t\n"] {
            assert_eq!(normalise_topic(junk), None, "accepted {junk:?}");
        }
        let long = "n".repeat(TOPIC_MAX + 1);
        assert_eq!(normalise_topic(&long), None);
        let just_fits = "n".repeat(TOPIC_MAX);
        assert_eq!(normalise_topic(&just_fits), Some(just_fits));
    }

    #[test]
    fn a_preference_carries_the_address_and_the_topic_folded() {
        let now = OffsetDateTime::UNIX_EPOCH;
        let valid = validate(&new(), now).unwrap_or_else(|e| panic!("refused a good one: {e:?}"));
        assert_eq!(valid.address, "ann@lead.test");
        assert_eq!(valid.topic, "newsletter");
        assert_eq!(valid.occurred_at, now);
        assert_eq!(valid.source_ref, None);
    }

    #[test]
    fn an_address_nobody_could_be_mailed_at_cannot_hold_a_preference() {
        // A preference row that does not join the audience is somebody who
        // pressed the button and is still being mailed.
        let now = OffsetDateTime::UNIX_EPOCH;
        for junk in ["", "   ", "n/a", "ask reception", "ann@localhost"] {
            let candidate = NewTopicOptOut {
                address: junk,
                ..new()
            };
            assert!(
                matches!(validate(&candidate, now), Err(StoreError::Validation(_))),
                "accepted a preference for {junk:?}"
            );
        }
    }

    #[test]
    fn a_preference_says_which_kind_of_mail_it_is_about() {
        let now = OffsetDateTime::UNIX_EPOCH;
        let long = "n".repeat(TOPIC_MAX + 1);
        for junk in ["", "   ", &long] {
            let candidate = NewTopicOptOut {
                topic: junk,
                ..new()
            };
            assert!(
                matches!(validate(&candidate, now), Err(StoreError::Validation(_))),
                "accepted a preference about {junk:?}"
            );
        }
    }

    #[test]
    fn a_source_reference_is_trimmed_and_bounded() {
        let now = OffsetDateTime::UNIX_EPOCH;
        let padded = NewTopicOptOut {
            source_ref: Some("  cut_2026  "),
            ..new()
        };
        assert_eq!(
            validate(&padded, now)
                .unwrap_or_else(|e| panic!("{e:?}"))
                .source_ref
                .as_deref(),
            Some("cut_2026")
        );
        let blank = NewTopicOptOut {
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
        let long = "x".repeat(TOPIC_SOURCE_REF_MAX + 1);
        let overlong = NewTopicOptOut {
            source_ref: Some(&long),
            ..new()
        };
        assert!(matches!(
            validate(&overlong, now),
            Err(StoreError::Validation(_))
        ));
    }

    #[test]
    fn a_preference_cannot_be_dated_after_it_was_decided() {
        let now = OffsetDateTime::UNIX_EPOCH + Duration::days(365);
        let ahead = NewTopicOptOut {
            occurred_at: Some(now + Duration::hours(1)),
            ..new()
        };
        assert!(matches!(
            validate(&ahead, now),
            Err(StoreError::Validation(_))
        ));
        // Clock skew is not a lie.
        let barely = NewTopicOptOut {
            occurred_at: Some(now + Duration::minutes(TOPIC_FUTURE_SKEW_MINUTES - 1)),
            ..new()
        };
        assert!(validate(&barely, now).is_ok());
        // A colleague relaying a decision names when it was made, not when they
        // got round to typing it.
        let earlier = now - Duration::hours(6);
        let relayed = NewTopicOptOut {
            occurred_at: Some(earlier),
            ..new()
        };
        assert_eq!(
            validate(&relayed, now)
                .unwrap_or_else(|e| panic!("{e:?}"))
                .occurred_at,
            earlier
        );
    }

    #[test]
    fn a_stored_row_reads_back_whole() {
        let row = (
            "opt".to_owned(),
            "ann@lead.test".to_owned(),
            "newsletter".to_owned(),
            Some("cut_2026".to_owned()),
            OffsetDateTime::UNIX_EPOCH,
            OffsetDateTime::UNIX_EPOCH + Duration::hours(2),
        );
        let stored = row_to_optout(row);
        assert_eq!(stored.id, CampaignTopicOptOutId::new("opt"));
        assert_eq!(stored.topic, "newsletter");
        assert_eq!(stored.source_ref.as_deref(), Some("cut_2026"));
        assert!(stored.occurred_at < stored.recorded_at);
    }
}
