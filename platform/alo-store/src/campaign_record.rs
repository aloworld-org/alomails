//! The campaign itself — subject, preheader, and a body in the Docs block model
//! (alo Campaigns, ADR 0044, wave C3.1).
//!
//! Waves C1 and C2s decided **who** may be mailed and how somebody leaves. This
//! is the first module that holds **what the mail says**, and it holds only
//! that: the block vocabulary and its rules are
//! [`campaign_content`](crate::campaign_content), and everything about sending
//! is absent on purpose (below).
//!
//! ## Nothing here sends, and nothing here is a send
//!
//! There is no status, no schedule, no segment link and no recipient list. ADR
//! 0044 §1 blocks the sending path on a second egress IP that has to be bought,
//! and a record that could be marked *sent* would be a lifecycle invented ahead
//! of the thing it describes — the shape C5m.1 exists to get right once, with
//! the send events in front of it. A campaign here is a draft in the only sense
//! that matters: it is text somebody is writing.
//!
//! ## Why a campaign must say what kind of mail it is
//!
//! `topic` is required, and that is the one field the queue item does not name.
//! C2s.2 built a landing page that offers a recipient **fewer rather than only
//! none** — *this kind of mail, or all of it* — and the "kind" has to come from
//! the campaign that mailed them. A campaign with no topic can offer nothing but
//! all-or-nothing, and ADR 0044 §3's argument is that a recipient offered
//! all-or-nothing presses the spam button instead. So the requirement lands here
//! rather than as a default some sending code would have to remember, and
//! [`campaign_topic_optout`](crate::campaign_topic_optout)'s rows finally have a
//! source for the label they are declining.
//!
//! The label is stored **as the sender wrote it** — whitespace collapsed, case
//! kept — because a human reads it on that page. The fold
//! ([`normalise_topic`](crate::campaign_topic_optout::normalise_topic)) is what
//! a query compares, and it is applied by the opt-out table; one rule, applied
//! at both ends, exactly as the address fold is.
//!
//! ## Tenancy
//!
//! Every statement carries `tenant_id = $1` and the primary key is
//! `(tenant_id, id)`, so another tenant's campaign is not a `403` but a row that
//! does not exist — the same non-oracle every other module keeps. A campaign is
//! **tenant-wide, not private to its author**: `created_by` records who to ask
//! what it was for, and a colleague can edit the mail their colleague drafted,
//! because a campaign is the company's letter and not somebody's document.

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::campaign_content::CampaignContent;
use crate::campaign_merge::{reject_merge_fields, validate_merge_text};
use crate::campaign_topic_optout::{TOPIC_MAX, normalise_topic};
use crate::error::{Result, StoreError};
use crate::id::{CampaignId, UserId};

/// The longest subject a campaign may carry — matching the migration's `CHECK`.
///
/// Mail clients truncate a subject well before this; the cap is here to stop a
/// paste, not to have an opinion about copywriting.
pub const CAMPAIGN_SUBJECT_MAX: usize = 200;

/// The longest preheader a campaign may carry. The preview text a client shows
/// beside the subject; same reasoning as the subject.
pub const CAMPAIGN_PREHEADER_MAX: usize = 200;

/// The most campaigns one read returns.
pub const CAMPAIGN_PAGE_MAX: i64 = 200;

/// A campaign, whole — the record plus its body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Campaign {
    /// The record's handle. Opaque and safe to log.
    pub id: CampaignId,
    /// The subject line, as it would arrive in an inbox — and what a colleague
    /// recognises the campaign by.
    pub subject: String,
    /// The preview text beside the subject, or `None` when there is none (in
    /// which case a client falls back to the first line of the body).
    pub preheader: Option<String>,
    /// Which kind of mail this is, as the sender wrote it. Never blank — see
    /// the module docs.
    pub topic: String,
    /// The body, validated.
    pub content: CampaignContent,
    /// The colleague who wrote it. Who to ask what it was for; never a claim
    /// that anybody agreed to receive it.
    pub created_by: UserId,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

/// A campaign as a list shows it: everything except the body.
///
/// The body is left out rather than truncated, because half a body is a thing a
/// caller can accidentally save back. `blocks` is how far along it is — the one
/// honest signal a list can give about a mail it is not carrying.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CampaignSummary {
    pub id: CampaignId,
    pub subject: String,
    pub preheader: Option<String>,
    pub topic: String,
    /// How many blocks the body holds. Zero is a campaign named and not yet
    /// written.
    pub blocks: i64,
    pub created_by: UserId,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

/// A campaign to write, or to write over an existing one.
#[derive(Debug, Clone)]
pub struct NewCampaign<'a> {
    /// The subject, in whatever spacing it arrived in; trimmed here.
    pub subject: &'a str,
    /// The preheader, or `None`. A blank one is `None` rather than an error: a
    /// form that clears the field sends an empty string, and refusing that would
    /// make "remove the preview text" an error message.
    pub preheader: Option<&'a str>,
    /// The kind of mail, as written.
    pub topic: &'a str,
    /// The body. Validated again here, so a value built in Rust rather than
    /// parsed from the wire cannot skip the rules.
    pub content: CampaignContent,
}

/// A campaign's fields as the CRUD statements return them, body included.
type CampaignRow = (
    String,
    String,
    Option<String>,
    String,
    String,
    String,
    OffsetDateTime,
    OffsetDateTime,
);

/// A campaign's fields as the list returns them, with the block count in place
/// of the body.
type SummaryRow = (
    String,
    String,
    Option<String>,
    String,
    i64,
    String,
    OffsetDateTime,
    OffsetDateTime,
);

/// The columns every whole-campaign statement returns, in the order
/// [`CampaignRow`] reads them.
const CAMPAIGN_COLUMNS: &str = "id, subject, preheader, topic, content::text, created_by, \
                                created_at, updated_at";

/// The columns the list returns. `jsonb_array_length` is the block count, which
/// the envelope's `CHECK` guarantees is an array to ask about.
const SUMMARY_COLUMNS: &str = "id, subject, preheader, topic, \
                               jsonb_array_length(content -> 'blocks')::bigint AS blocks, \
                               created_by, created_at, updated_at";

fn insert_sql() -> String {
    format!(
        "INSERT INTO campaigns (tenant_id, id, subject, preheader, topic, content, created_by) \
         VALUES ($1, $2, $3, $4, $5, $6::jsonb, $7) \
         RETURNING {CAMPAIGN_COLUMNS}"
    )
}

fn lookup_sql() -> String {
    format!("SELECT {CAMPAIGN_COLUMNS} FROM campaigns WHERE tenant_id = $1 AND id = $2")
}

fn list_sql() -> String {
    format!(
        "SELECT {SUMMARY_COLUMNS} FROM campaigns \
          WHERE tenant_id = $1 \
          ORDER BY created_at DESC, id \
          LIMIT $2"
    )
}

fn update_sql() -> String {
    format!(
        "UPDATE campaigns \
            SET subject = $3, preheader = $4, topic = $5, content = $6::jsonb, updated_at = now() \
          WHERE tenant_id = $1 AND id = $2 \
         RETURNING {CAMPAIGN_COLUMNS}"
    )
}

fn delete_sql() -> &'static str {
    "DELETE FROM campaigns WHERE tenant_id = $1 AND id = $2 RETURNING id"
}

/// Every statement this module can issue against the database.
///
/// The list the whole campaigns track keeps, for the promise C1.1 carries: **no
/// query in this module may read the per-user address book.** A campaign record
/// has no business anywhere near `contacts`, and the cheapest way to keep it
/// that way is to assert it against the strings rather than to trust a reading.
#[cfg(test)]
fn all_sql() -> Vec<String> {
    vec![
        insert_sql(),
        lookup_sql(),
        list_sql(),
        update_sql(),
        delete_sql().to_owned(),
    ]
}

/// Turns a stored row into a campaign, refusing a body this build cannot read.
///
/// A body that fails validation on the way **out** is a decode failure, not an
/// empty campaign: it means the column holds something no writer here could have
/// put there, and answering with a blank mail would hide that from everybody.
fn row_to_campaign(row: CampaignRow) -> Result<Campaign> {
    let content = CampaignContent::parse(&row.4).map_err(|_| {
        StoreError::Db(sqlx::Error::Decode(
            "a stored campaign body is not in a block model this build can read".into(),
        ))
    })?;
    Ok(Campaign {
        id: CampaignId::new(row.0),
        subject: row.1,
        preheader: row.2,
        topic: row.3,
        content,
        created_by: UserId::new(row.5),
        created_at: row.6,
        updated_at: row.7,
    })
}

/// Turns a stored row into a list entry.
fn row_to_summary(row: SummaryRow) -> CampaignSummary {
    CampaignSummary {
        id: CampaignId::new(row.0),
        subject: row.1,
        preheader: row.2,
        topic: row.3,
        blocks: row.4,
        created_by: UserId::new(row.5),
        created_at: row.6,
        updated_at: row.7,
    }
}

/// Checks a subject: present, trimmed, bounded.
fn validate_subject(raw: &str) -> Result<String> {
    let subject = raw.trim();
    if subject.is_empty() {
        return Err(StoreError::Validation(
            "a campaign needs a subject line — it is what arrives in the inbox".to_owned(),
        ));
    }
    if subject.chars().count() > CAMPAIGN_SUBJECT_MAX {
        return Err(StoreError::Validation(format!(
            "a subject line fits in {CAMPAIGN_SUBJECT_MAX} characters"
        )));
    }
    // The cap is measured on what the writer typed, not on what a recipient
    // reads: a personalised subject is usually shorter once resolved, and a
    // limit that moved per recipient would be one nobody could compose against.
    validate_merge_text("the subject line", subject)?;
    Ok(subject.to_owned())
}

/// Checks a preheader: absent, or present and bounded. Blank is absent.
fn validate_preheader(raw: Option<&str>) -> Result<Option<String>> {
    let Some(preheader) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if preheader.chars().count() > CAMPAIGN_PREHEADER_MAX {
        return Err(StoreError::Validation(format!(
            "preview text fits in {CAMPAIGN_PREHEADER_MAX} characters"
        )));
    }
    validate_merge_text("preview text", preheader)?;
    Ok(Some(preheader.to_owned()))
}

/// Checks a topic and returns it as it will be stored: whitespace collapsed,
/// case kept.
///
/// Held to what [`normalise_topic`] can fold, so a label that reached the
/// unsubscribe page could always be compared with what somebody declined. A
/// topic that folded to nothing — or to more than [`TOPIC_MAX`] characters — is
/// a campaign whose recipients could not be offered "fewer".
fn validate_topic(raw: &str) -> Result<String> {
    let collapsed = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    // A topic is drawn on the unsubscribe page and resolved by nothing on the
    // way there, so a merge field in one would arrive in front of a leaving
    // recipient verbatim. Refused by name rather than left to leak.
    reject_merge_fields("a topic", &collapsed)?;
    if normalise_topic(&collapsed).is_none() {
        return Err(StoreError::Validation(format!(
            "a campaign says which kind of mail it is, in 1 to {TOPIC_MAX} characters — a \
             recipient can then stop this kind without stopping all of it"
        )));
    }
    Ok(collapsed)
}

/// The validated fields of a campaign, ready to bind.
struct ValidCampaign {
    subject: String,
    preheader: Option<String>,
    topic: String,
    content: String,
}

/// Checks a whole campaign, in one place, without a database.
fn validate(campaign: &NewCampaign<'_>) -> Result<ValidCampaign> {
    campaign.content.validate()?;
    Ok(ValidCampaign {
        subject: validate_subject(campaign.subject)?,
        preheader: validate_preheader(campaign.preheader)?,
        topic: validate_topic(campaign.topic)?,
        content: campaign.content.to_json()?,
    })
}

impl AccountStore {
    /// Writes a new campaign and returns it.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the subject is blank or too long, the
    /// preheader is too long, the topic cannot be folded, or the body breaks one
    /// of [`campaign_content`](crate::campaign_content)'s rules;
    /// [`StoreError::Db`] on failure.
    pub async fn create_campaign(&self, campaign: &NewCampaign<'_>) -> Result<Campaign> {
        let valid = validate(campaign)?;
        let id = CampaignId::generate();
        let row: CampaignRow = sqlx::query_as(&insert_sql())
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .bind(&valid.subject)
            .bind(&valid.preheader)
            .bind(&valid.topic)
            .bind(&valid.content)
            .bind(self.user.as_str())
            .fetch_one(&self.pool)
            .await
            .map_err(StoreError::Db)?;
        row_to_campaign(row)
    }

    /// One campaign, whole, or `None` when this tenant has no such id.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure, including a stored body this build cannot
    /// read.
    pub async fn campaign(&self, id: &CampaignId) -> Result<Option<Campaign>> {
        let row: Option<CampaignRow> = sqlx::query_as(&lookup_sql())
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .fetch_optional(&self.pool)
            .await
            .map_err(StoreError::Db)?;
        row.map(row_to_campaign).transpose()
    }

    /// This tenant's campaigns, newest first, without their bodies.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when `limit` is outside
    /// `1..=`[`CAMPAIGN_PAGE_MAX`]; [`StoreError::Db`] on failure.
    pub async fn campaigns(&self, limit: i64) -> Result<Vec<CampaignSummary>> {
        if !(1..=CAMPAIGN_PAGE_MAX).contains(&limit) {
            return Err(StoreError::Validation(format!(
                "a page of campaigns is between 1 and {CAMPAIGN_PAGE_MAX}"
            )));
        }
        let rows: Vec<SummaryRow> = sqlx::query_as(&list_sql())
            .bind(self.tenant.as_str())
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(row_to_summary).collect())
    }

    /// Rewrites a campaign whole — subject, preheader, topic and body.
    ///
    /// Whole-record rather than field-by-field, for the reason
    /// `update_campaign_segment` is: a partial write is how a body loses its
    /// last paragraph without anybody deciding it should. A caller that means to
    /// change one field reads the campaign, changes it, and writes it back; the
    /// HTTP edge does exactly that.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the campaign is absent or another
    /// tenant's; [`StoreError::Validation`] as for
    /// [`create_campaign`](Self::create_campaign); [`StoreError::Db`] on
    /// failure.
    pub async fn update_campaign(
        &self,
        id: &CampaignId,
        campaign: &NewCampaign<'_>,
    ) -> Result<Campaign> {
        let valid = validate(campaign)?;
        let row: Option<CampaignRow> = sqlx::query_as(&update_sql())
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .bind(&valid.subject)
            .bind(&valid.preheader)
            .bind(&valid.topic)
            .bind(&valid.content)
            .fetch_optional(&self.pool)
            .await
            .map_err(StoreError::Db)?;
        row_to_campaign(row.ok_or(StoreError::NotFound)?)
    }

    /// Forgets a campaign.
    ///
    /// Deleting a campaign deletes a letter, never evidence: consent records,
    /// suppressions and topic opt-outs live in their own tables and are
    /// untouched, so a tenant tidying up its drafts cannot lose the reason
    /// somebody may or may not be mailed.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the campaign is absent or another
    /// tenant's; [`StoreError::Db`] on failure.
    pub async fn delete_campaign(&self, id: &CampaignId) -> Result<()> {
        let deleted: Option<String> = sqlx::query_scalar(delete_sql())
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .fetch_optional(&self.pool)
            .await
            .map_err(StoreError::Db)?;
        deleted.map(|_| ()).ok_or(StoreError::NotFound)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn detail(result: Result<String>) -> String {
        match result {
            Err(StoreError::Validation(detail)) => detail,
            other => panic!("expected a validation error, got {other:?}"),
        }
    }

    #[test]
    fn a_subject_is_required_because_it_is_what_arrives_in_the_inbox() {
        assert!(detail(validate_subject("   ")).contains("subject"));
        assert_eq!(
            validate_subject("  Spring prices  ").ok(),
            Some("Spring prices".to_owned())
        );
        let long = "s".repeat(CAMPAIGN_SUBJECT_MAX + 1);
        assert!(detail(validate_subject(&long)).contains("characters"));
    }

    #[test]
    fn a_blank_preheader_is_no_preheader_rather_than_an_error() {
        // A form that clears the field sends an empty string; refusing it would
        // make "remove the preview text" an error message.
        assert_eq!(validate_preheader(Some("  ")).ok(), Some(None));
        assert_eq!(validate_preheader(None).ok(), Some(None));
        assert_eq!(
            validate_preheader(Some(" Ten per cent off ")).ok(),
            Some(Some("Ten per cent off".to_owned()))
        );
        let long = "p".repeat(CAMPAIGN_PREHEADER_MAX + 1);
        match validate_preheader(Some(&long)) {
            Err(StoreError::Validation(detail)) => assert!(detail.contains("characters")),
            other => panic!("expected a validation error, got {other:?}"),
        }
    }

    #[test]
    fn a_campaign_must_say_which_kind_of_mail_it_is() {
        // Without it, C2s.2's page can only offer "stop everything" — and a
        // recipient offered that presses the spam button instead.
        assert!(detail(validate_topic("   ")).contains("kind of mail"));
        let long = "t".repeat(TOPIC_MAX + 1);
        assert!(detail(validate_topic(&long)).contains("kind of mail"));
    }

    #[test]
    fn the_topic_is_stored_as_written_and_folds_to_what_a_query_compares() {
        // The label is for a human on the unsubscribe page; the fold is what
        // `campaign_topic_optouts` stores. One rule, applied at both ends.
        let stored = validate_topic("  Monthly   Newsletter ").ok();
        assert_eq!(stored, Some("Monthly Newsletter".to_owned()));
        assert_eq!(
            stored.as_deref().and_then(normalise_topic),
            Some("monthly newsletter".to_owned()),
            "what the sender wrote and what a decline compares must fold together"
        );
    }

    #[test]
    fn a_subject_or_a_preheader_with_an_undefaulted_merge_field_cannot_be_saved() {
        // C3.4's rule, applied at the same gate as the body's — a subject that
        // would arrive as "Hi ," is refused while somebody is writing it.
        assert!(detail(validate_subject("Hi {{first_name}}, spring prices")).contains("fallback"));
        assert!(
            validate_subject("Hi {{first_name|there}}, spring prices").is_ok(),
            "a defaulted field is ordinary text to save"
        );
        match validate_preheader(Some("Written to {{email}}")) {
            Err(StoreError::Validation(reported)) => {
                assert!(reported.starts_with("preview text: "), "{reported}");
            }
            other => panic!("expected a validation error, got {other:?}"),
        }
    }

    #[test]
    fn a_topic_cannot_be_personalised_because_a_leaving_recipient_reads_it_as_written() {
        // The unsubscribe page (C2s.2) draws the topic verbatim and nothing
        // resolves it on the way there.
        let reported = detail(validate_topic("News for {{first_name|you}}"));
        assert!(reported.contains("as written"), "{reported}");
        assert!(validate_topic("Monthly newsletter").is_ok());
    }

    #[test]
    fn no_query_in_this_module_can_read_the_per_user_address_book() {
        // C1.1's promise, asserted against the SQL rather than by inspection:
        // a campaign record has no business near somebody's own address book.
        for sql in all_sql() {
            assert!(
                !sql.contains("contacts"),
                "a campaign statement reached the per-user address book: {sql}"
            );
        }
    }
}
