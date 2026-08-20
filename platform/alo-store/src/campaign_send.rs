//! The record of a send — the durable ledger a campaign is mailed from (alo
//! Campaigns, ADR 0044, wave C4.1, and the control half of C4.4).
//!
//! Design: `docs/design/campaign-send-job.md`. Schema: migration 0800.
//!
//! Everything a campaign needs in order to become mail was already here.
//! [`campaign_content`](crate::campaign_content) holds the blocks,
//! [`campaign_html`](crate::campaign_html) and
//! [`campaign_text`](crate::campaign_text) compile them,
//! [`campaign_mime`](crate::campaign_mime) assembles the
//! `multipart/alternative`, [`campaign_audience`](crate::campaign_audience)
//! answers who may be mailed. What was missing is any record that a send ever
//! happened, and `campaign_mime` says so in its own module doc: *handing this
//! entity to a submission path is one function call on the day there is one.*
//!
//! **This module is not that day either.** It does not render, does not submit
//! and does not decide when anything leaves. It writes down who is to be
//! mailed, exactly once each, in a table that survives the process — so that a
//! dispatcher which dies mid-send is answered by reading a row rather than by
//! guessing whether anybody already got the letter. The dispatcher itself is
//! C4.2/C4.3 and consumes this.
//!
//! ## Enrolment is paged, and the caller drives it
//!
//! [`enrol_campaign_send_page`](AccountStore::enrol_campaign_send_page) writes
//! one page of recipients and returns where it got to. It is deliberately not a
//! single statement that walks the whole audience: an audience of two hundred
//! thousand assembled in one transaction holds locks for minutes across three
//! tables that CRM, Billing and the site forms are still writing to. A page at
//! a time is resumable, and resumability is the entire point of the item.
//!
//! Re-running a page is safe and is the intended recovery. Every insert is
//! `ON CONFLICT DO NOTHING` against the uniqueness described below, so a caller
//! that is unsure whether its last page landed simply asks for it again.
//!
//! ## Once per campaign, not once per send
//!
//! C4.1 asks for *idempotency on (campaign, address)*. That is stronger than
//! the obvious reading, and the migration's index enforces the strong one: a
//! person enrolled by any send of a campaign cannot be enrolled by another send
//! of the same campaign. The accident it prevents is the common one — somebody
//! presses send, spots the typo, stops it, fixes it, and presses send again;
//! with per-send uniqueness everybody who got the broken copy also gets the
//! fixed one.
//!
//! ## A declined topic is recorded, not omitted
//!
//! [`campaign_recipients`](AccountStore::campaign_recipients) already applies
//! consent and tenant-wide suppression. It cannot apply per-topic opt-outs,
//! because the topic is a fact about the campaign rather than about the
//! audience — so this module applies them and writes the declining recipients
//! as `skipped` rows carrying the reason.
//!
//! Writing them rather than dropping them is what keeps the tallies honest.
//! "We mailed 900 of 1000" with no account of the other hundred is precisely
//! the number that cannot be defended to somebody asking what happened to them.

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::campaign_audience::AudiencePage;
use crate::campaign_topic_optout::normalise_topic;
use crate::error::{Result, StoreError};
use crate::id::{CampaignId, CampaignSendId};

/// Why a recipient is not `pending`. A short code rather than prose: a tally
/// groups by it, and the sentence a person reads is the interface's, in their
/// own language.
pub mod reason {
    /// They asked not to receive this kind of mail.
    pub const TOPIC_DECLINED: &str = "topic_declined";
}

/// Where a send is in its life.
///
/// `Enrolling` is its own state rather than a flag on `Sending`, because the
/// two fail differently and an operator has to be able to tell them apart: a
/// send stuck enrolling is a caller that stopped walking pages, one stuck
/// sending is a dispatcher that stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendState {
    /// Recipients are still being written.
    Enrolling,
    /// Enrolment finished; the dispatcher may work.
    Sending,
    /// A person paused it. Resumable.
    Paused,
    /// A person stopped it. Terminal — what has gone has gone.
    Stopped,
    /// Every enrolled recipient has settled. Terminal.
    Done,
}

impl SendState {
    /// The token stored in the `state` column.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Enrolling => "enrolling",
            Self::Sending => "sending",
            Self::Paused => "paused",
            Self::Stopped => "stopped",
            Self::Done => "done",
        }
    }

    /// Reads a stored token, or `None` for one this build does not know.
    #[must_use]
    pub fn parse(token: &str) -> Option<Self> {
        match token {
            "enrolling" => Some(Self::Enrolling),
            "sending" => Some(Self::Sending),
            "paused" => Some(Self::Paused),
            "stopped" => Some(Self::Stopped),
            "done" => Some(Self::Done),
            _ => None,
        }
    }

    /// Whether the send has finished for good. A terminal send is never
    /// enrolled into again — which is what makes "open a new send" the only way
    /// to mail more people, and therefore what makes the campaign-wide
    /// uniqueness meaningful.
    #[must_use]
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Stopped | Self::Done)
    }
}

/// What happened to one enrolled person.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecipientState {
    /// Enrolled, not yet acted on.
    Pending,
    /// The dispatcher handed it to submission.
    Sent,
    /// The dispatcher could not.
    Failed,
    /// Never attempted, and why is recorded beside it.
    Skipped,
}

impl RecipientState {
    /// The token stored in the `state` column.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Sent => "sent",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }

    /// Reads a stored token, or `None` for one this build does not know.
    #[must_use]
    pub fn parse(token: &str) -> Option<Self> {
        match token {
            "pending" => Some(Self::Pending),
            "sent" => Some(Self::Sent),
            "failed" => Some(Self::Failed),
            "skipped" => Some(Self::Skipped),
            _ => None,
        }
    }
}

/// One act of sending one campaign.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CampaignSend {
    pub id: CampaignSendId,
    pub campaign_id: CampaignId,
    /// The topic as folded when the send opened — not read live from the
    /// campaign, so what this send honoured cannot change under it.
    pub topic_fold: String,
    pub state: SendState,
    /// Why a person stopped it, when they said.
    pub stopped_note: Option<String>,
    pub opened_by: String,
    pub opened_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    /// When enrolment finished walking the audience. `None` separates "nobody
    /// is enrolled yet" from "nobody was eligible" — the same row count, and
    /// completely different facts.
    pub enrolled_at: Option<OffsetDateTime>,
}

/// How one page of enrolment went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnrolledPage {
    /// Rows written as `pending` by this call.
    pub enrolled: i64,
    /// Rows written as `skipped` by this call.
    pub skipped: i64,
    /// Addresses this page saw that were already enrolled by an earlier send of
    /// the same campaign, and were therefore left alone. Not a failure — it is
    /// the idempotency working, and it is surfaced so a caller can say so.
    pub already_enrolled: i64,
    /// Where to continue from, or `None` when the audience is exhausted. When
    /// this is `None` the send has moved from `Enrolling` to `Sending`.
    pub next_cursor: Option<String>,
}

/// The count of each recipient state in one send.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SendTally {
    pub pending: i64,
    pub sent: i64,
    pub failed: i64,
    pub skipped: i64,
}

impl SendTally {
    /// Everybody enrolled, whatever became of them.
    #[must_use]
    pub fn total(self) -> i64 {
        self.pending + self.sent + self.failed + self.skipped
    }
}

impl AccountStore {
    /// Opens a send for a campaign.
    ///
    /// The campaign's topic is folded and stored on the send now, so that
    /// editing the campaign afterwards cannot change which opt-outs this send
    /// is recorded as having honoured.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the campaign is not this tenant's;
    /// [`StoreError::Conflict`] when a send of it is already live;
    /// [`StoreError::Db`] on failure.
    pub async fn open_campaign_send(&self, campaign_id: &CampaignId) -> Result<CampaignSend> {
        // Read the campaign first, through the tenant-scoped reader, so a
        // wrong-tenant id is `NotFound` here rather than a foreign-key error
        // from the insert — the caller must not be able to tell the difference
        // between "no such campaign" and "somebody else's campaign".
        let campaign = self
            .campaign(campaign_id)
            .await?
            .ok_or(StoreError::NotFound)?;

        let topic_fold = normalise_topic(&campaign.topic).ok_or_else(|| {
            StoreError::Validation(
                "the campaign's topic cannot be folded, so a recipient could not be told \
                 what they were leaving"
                    .to_owned(),
            )
        })?;

        let id = CampaignSendId::generate();
        let row: SendRow = sqlx::query_as(&insert_send_sql())
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .bind(campaign_id.as_str())
            .bind(&topic_fold)
            .bind(SendState::Enrolling.as_str())
            .bind(self.user.as_str())
            .fetch_one(&self.pool)
            .await
            .map_err(|err| match err {
                // The partial unique index on the live states. Named rather
                // than passed through, because "unique constraint" tells an
                // operator nothing about what to do next.
                sqlx::Error::Database(ref db) if db.is_unique_violation() => StoreError::Conflict(
                    "this campaign already has a send in progress; stop it before opening another"
                        .to_owned(),
                ),
                other => StoreError::Db(other),
            })?;
        row.into_send()
    }

    /// One send of this tenant's, or `None`.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn campaign_send(&self, id: &CampaignSendId) -> Result<Option<CampaignSend>> {
        let row: Option<SendRow> = sqlx::query_as(&select_send_sql())
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .fetch_optional(&self.pool)
            .await
            .map_err(StoreError::Db)?;
        row.map(SendRow::into_send).transpose()
    }

    /// Every send of one campaign, newest first.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn campaign_sends(&self, campaign_id: &CampaignId) -> Result<Vec<CampaignSend>> {
        let rows: Vec<SendRow> = sqlx::query_as(&select_sends_sql())
            .bind(self.tenant.as_str())
            .bind(campaign_id.as_str())
            .fetch_all(&self.pool)
            .await
            .map_err(StoreError::Db)?;
        rows.into_iter().map(SendRow::into_send).collect()
    }

    /// How the enrolled recipients of one send are distributed across the
    /// states.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn campaign_send_tally(&self, id: &CampaignSendId) -> Result<SendTally> {
        let rows: Vec<(String, i64)> = sqlx::query_as(TALLY_SQL)
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .fetch_all(&self.pool)
            .await
            .map_err(StoreError::Db)?;

        let mut tally = SendTally::default();
        for (state, count) in rows {
            match RecipientState::parse(&state) {
                Some(RecipientState::Pending) => tally.pending = count,
                Some(RecipientState::Sent) => tally.sent = count,
                Some(RecipientState::Failed) => tally.failed = count,
                Some(RecipientState::Skipped) => tally.skipped = count,
                // A state written by a newer build. Counted nowhere rather than
                // folded into one of ours: a tally that silently called an
                // unknown state "failed" would be a number nobody could
                // reconcile against the rows.
                None => {}
            }
        }
        Ok(tally)
    }

    /// Writes the next page of recipients for a send.
    ///
    /// Returns what it wrote and where to continue from. A `next_cursor` of
    /// `None` means the audience is exhausted and the send has moved to
    /// [`SendState::Sending`].
    ///
    /// Safe to repeat: every insert is `ON CONFLICT DO NOTHING`, so a caller
    /// that does not know whether its last page landed asks for it again.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the send is not this tenant's;
    /// [`StoreError::Conflict`] when the send is not enrolling;
    /// [`StoreError::Validation`] when the page is malformed;
    /// [`StoreError::Db`] on failure.
    pub async fn enrol_campaign_send_page(
        &self,
        id: &CampaignSendId,
        page: &AudiencePage,
    ) -> Result<EnrolledPage> {
        let send = self.campaign_send(id).await?.ok_or(StoreError::NotFound)?;
        if send.state != SendState::Enrolling {
            return Err(StoreError::Conflict(format!(
                "this send is {}, so its audience is settled and cannot be added to",
                send.state.as_str()
            )));
        }

        // The audience reader validates the cursor and the page size, and
        // already applies consent and tenant-wide suppression.
        let recipients = self.campaign_recipients(page).await?;
        if recipients.is_empty() {
            self.finish_enrolment(id).await?;
            return Ok(EnrolledPage {
                enrolled: 0,
                skipped: 0,
                already_enrolled: 0,
                next_cursor: None,
            });
        }

        let addresses: Vec<String> = recipients.iter().map(|r| r.address.clone()).collect();
        // Per-topic opt-outs are the one filter the audience cannot apply, the
        // topic being a fact about the campaign. Asked once for the whole page
        // rather than once per person: a query per recipient would be a page of
        // round trips to answer a question one `IN` clause answers.
        let declined = self
            .campaign_topic_decliners(&send.topic_fold, &addresses)
            .await?;

        let mut enrolled = 0_i64;
        let mut skipped = 0_i64;
        let mut already = 0_i64;
        for address in &addresses {
            let declined_topic = declined.iter().any(|d| d == address);
            let (state, why) = if declined_topic {
                (RecipientState::Skipped, Some(reason::TOPIC_DECLINED))
            } else {
                (RecipientState::Pending, None)
            };

            let written: Option<(String,)> = sqlx::query_as(INSERT_RECIPIENT_SQL)
                .bind(self.tenant.as_str())
                .bind(id.as_str())
                .bind(send.campaign_id.as_str())
                .bind(address)
                .bind(state.as_str())
                .bind(why)
                .fetch_optional(&self.pool)
                .await
                .map_err(StoreError::Db)?;

            match written {
                // `RETURNING` yields nothing when the row already existed, which
                // is the conflict clause doing its job.
                None => already += 1,
                Some(_) if declined_topic => skipped += 1,
                Some(_) => enrolled += 1,
            }
        }

        // The cursor is the last address of the page, whatever became of it —
        // including the ones already enrolled by an earlier send. Advancing
        // only past the rows this call wrote would loop forever over a page
        // that is entirely already-enrolled.
        let next_cursor = addresses.last().cloned();
        Ok(EnrolledPage {
            enrolled,
            skipped,
            already_enrolled: already,
            next_cursor,
        })
    }

    /// Marks enrolment finished: the send moves to [`SendState::Sending`].
    async fn finish_enrolment(&self, id: &CampaignSendId) -> Result<()> {
        sqlx::query(FINISH_ENROLMENT_SQL)
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .bind(SendState::Sending.as_str())
            .bind(SendState::Enrolling.as_str())
            .execute(&self.pool)
            .await
            .map_err(StoreError::Db)?;
        Ok(())
    }

    /// Pauses a sending send. The dispatcher is expected to stop claiming rows;
    /// what it has already handed to submission has gone.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the send is not this tenant's;
    /// [`StoreError::Conflict`] when it is not sending; [`StoreError::Db`] on
    /// failure.
    pub async fn pause_campaign_send(&self, id: &CampaignSendId) -> Result<CampaignSend> {
        self.transition(id, SendState::Sending, SendState::Paused, None)
            .await
    }

    /// Resumes a paused send.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the send is not this tenant's;
    /// [`StoreError::Conflict`] when it is not paused; [`StoreError::Db`] on
    /// failure.
    pub async fn resume_campaign_send(&self, id: &CampaignSendId) -> Result<CampaignSend> {
        self.transition(id, SendState::Paused, SendState::Sending, None)
            .await
    }

    /// Stops a send for good.
    ///
    /// **Idempotent on purpose.** Stopping an already-stopped send answers
    /// `Ok` with the send as it stands, because an operator pressing the button
    /// twice means the same thing both times, and the second press must not be
    /// an error at the exact moment they are panicking about what is going out.
    ///
    /// A stopped send is terminal. Its enrolled recipients keep their rows, so
    /// what had already gone remains answerable — and because uniqueness is per
    /// campaign, opening a fresh send afterwards reaches only the people this
    /// one never got to.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the send is not this tenant's;
    /// [`StoreError::Conflict`] when the send is already `done`;
    /// [`StoreError::Validation`] when the note is blank or over-long;
    /// [`StoreError::Db`] on failure.
    pub async fn stop_campaign_send(
        &self,
        id: &CampaignSendId,
        note: Option<&str>,
    ) -> Result<CampaignSend> {
        let note = match note.map(str::trim) {
            None | Some("") => None,
            Some(text) if text.chars().count() > NOTE_MAX => {
                return Err(StoreError::Validation(format!(
                    "a note explaining the stop is at most {NOTE_MAX} characters"
                )));
            }
            Some(text) => Some(text.to_owned()),
        };

        let send = self.campaign_send(id).await?.ok_or(StoreError::NotFound)?;
        match send.state {
            // Already stopped: say so calmly and return what stands.
            SendState::Stopped => return Ok(send),
            SendState::Done => {
                return Err(StoreError::Conflict(
                    "this send has already finished; there is nothing left to stop".to_owned(),
                ));
            }
            _ => {}
        }

        let row: Option<SendRow> = sqlx::query_as(&stop_sql())
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .bind(SendState::Stopped.as_str())
            .bind(note.as_deref())
            .fetch_optional(&self.pool)
            .await
            .map_err(StoreError::Db)?;
        // Lost the race with another stopper. Their stop is as good as ours and
        // means the same thing, so read the row back rather than reporting a
        // conflict for an outcome the caller asked for and got.
        match row {
            Some(row) => row.into_send(),
            None => self.campaign_send(id).await?.ok_or(StoreError::NotFound),
        }
    }

    /// The shared body of the two reversible transitions.
    ///
    /// The state is checked **in the `UPDATE`'s own `WHERE`** rather than by
    /// reading first and writing after: two operators pressing pause at once
    /// would both pass a prior read, and the second write would silently move a
    /// send out of a state it was no longer in.
    async fn transition(
        &self,
        id: &CampaignSendId,
        from: SendState,
        to: SendState,
        note: Option<&str>,
    ) -> Result<CampaignSend> {
        let row: Option<SendRow> = sqlx::query_as(&transition_sql())
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .bind(to.as_str())
            .bind(from.as_str())
            .bind(note)
            .fetch_optional(&self.pool)
            .await
            .map_err(StoreError::Db)?;

        match row {
            Some(row) => row.into_send(),
            // Nothing moved. Distinguish "no such send for this tenant" from
            // "the send is in another state", because they need different
            // answers on a screen — and a wrong-tenant id must reach the same
            // `NotFound` a missing one does.
            None => match self.campaign_send(id).await? {
                None => Err(StoreError::NotFound),
                Some(actual) => Err(StoreError::Conflict(format!(
                    "this send is {}, and only a send that is {} can become {}",
                    actual.state.as_str(),
                    from.as_str(),
                    to.as_str()
                ))),
            },
        }
    }
}

/// The longest note a person may leave when stopping a send. Matches the
/// database's own `CHECK`, so the refusal is a sentence rather than a
/// constraint violation.
const NOTE_MAX: usize = 500;

/// The row shape both send readers return.
#[derive(sqlx::FromRow)]
struct SendRow {
    id: String,
    campaign_id: String,
    topic_fold: String,
    state: String,
    stopped_note: Option<String>,
    opened_by: String,
    opened_at: OffsetDateTime,
    updated_at: OffsetDateTime,
    enrolled_at: Option<OffsetDateTime>,
}

impl SendRow {
    fn into_send(self) -> Result<CampaignSend> {
        let state = SendState::parse(&self.state).ok_or_else(|| {
            // A state this build does not know is a row written by a newer one.
            // Refused rather than guessed at: treating an unknown state as any
            // known one would let an operator act on a send whose real
            // condition they have not been told.
            StoreError::Validation(
                "this send is in a state this version does not understand".to_owned(),
            )
        })?;
        Ok(CampaignSend {
            id: CampaignSendId::from(self.id),
            campaign_id: CampaignId::from(self.campaign_id),
            topic_fold: self.topic_fold,
            state,
            stopped_note: self.stopped_note,
            opened_by: self.opened_by,
            opened_at: self.opened_at,
            updated_at: self.updated_at,
            enrolled_at: self.enrolled_at,
        })
    }
}

/// The columns every send reader returns, in the order [`SendRow`] expects.
/// Written once: four copies of a column list is four places to forget a column
/// added to the table.
const SEND_COLUMNS: &str = "id, campaign_id, topic_fold, state, stopped_note, \
                            opened_by, opened_at, updated_at, enrolled_at";

fn insert_send_sql() -> String {
    format!(
        "INSERT INTO campaign_sends \
         (tenant_id, id, campaign_id, topic_fold, state, opened_by) \
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING {SEND_COLUMNS}"
    )
}

fn select_send_sql() -> String {
    format!("SELECT {SEND_COLUMNS} FROM campaign_sends WHERE tenant_id = $1 AND id = $2")
}

fn select_sends_sql() -> String {
    format!(
        "SELECT {SEND_COLUMNS} FROM campaign_sends \
         WHERE tenant_id = $1 AND campaign_id = $2 \
         ORDER BY opened_at DESC, id DESC"
    )
}

const TALLY_SQL: &str = "SELECT state, count(*) FROM campaign_send_recipients \
     WHERE tenant_id = $1 AND send_id = $2 GROUP BY state";

/// `RETURNING address` yields nothing when the row was already there, which is
/// how the caller counts what the idempotency skipped.
const INSERT_RECIPIENT_SQL: &str = "INSERT INTO campaign_send_recipients \
     (tenant_id, send_id, campaign_id, address, state, reason, settled_at) \
     VALUES ($1, $2, $3, $4, $5, $6, CASE WHEN $5 = 'pending' THEN NULL ELSE now() END) \
     ON CONFLICT DO NOTHING \
     RETURNING address";

const FINISH_ENROLMENT_SQL: &str = "UPDATE campaign_sends \
     SET state = $3, enrolled_at = now(), updated_at = now() \
     WHERE tenant_id = $1 AND id = $2 AND state = $4";

fn transition_sql() -> String {
    format!(
        "UPDATE campaign_sends \
         SET state = $3, stopped_note = COALESCE($5, stopped_note), updated_at = now() \
         WHERE tenant_id = $1 AND id = $2 AND state = $4 RETURNING {SEND_COLUMNS}"
    )
}

/// Stops from any non-terminal state, which is why it is not
/// [`transition_sql`]: pause and resume each have exactly one legal
/// predecessor, and stop has four.
fn stop_sql() -> String {
    format!(
        "UPDATE campaign_sends \
         SET state = $3, stopped_note = $4, updated_at = now() \
         WHERE tenant_id = $1 AND id = $2 AND state NOT IN ('stopped', 'done') \
         RETURNING {SEND_COLUMNS}"
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn every_send_state_survives_the_round_trip() {
        for state in [
            SendState::Enrolling,
            SendState::Sending,
            SendState::Paused,
            SendState::Stopped,
            SendState::Done,
        ] {
            assert_eq!(SendState::parse(state.as_str()), Some(state));
        }
        // A token from a newer build is refused rather than folded into a
        // neighbour: acting on a send whose real state you were not told is
        // worse than being unable to act at all.
        assert_eq!(SendState::parse("draining"), None);
        assert_eq!(SendState::parse(""), None);
    }

    #[test]
    fn every_recipient_state_survives_the_round_trip() {
        for state in [
            RecipientState::Pending,
            RecipientState::Sent,
            RecipientState::Failed,
            RecipientState::Skipped,
        ] {
            assert_eq!(RecipientState::parse(state.as_str()), Some(state));
        }
        assert_eq!(RecipientState::parse("bounced"), None);
    }

    #[test]
    fn only_stopped_and_done_are_terminal() {
        assert!(SendState::Stopped.is_terminal());
        assert!(SendState::Done.is_terminal());
        // Paused is emphatically not terminal — a paused send is the one an
        // operator is about to resume.
        assert!(!SendState::Paused.is_terminal());
        assert!(!SendState::Enrolling.is_terminal());
        assert!(!SendState::Sending.is_terminal());
    }

    #[test]
    fn a_tally_counts_everybody_enrolled_whatever_became_of_them() {
        let tally = SendTally {
            pending: 10,
            sent: 5,
            failed: 2,
            skipped: 3,
        };
        assert_eq!(tally.total(), 20);
        assert_eq!(SendTally::default().total(), 0);
    }

    #[test]
    fn the_state_tokens_match_the_database_check_constraint() {
        // Migration 0800 constrains both columns to exactly these sets. A token
        // added here and not there is a row the database refuses at runtime,
        // which is the kind of bug that only appears in production.
        let send_states = ["enrolling", "sending", "paused", "stopped", "done"];
        for token in send_states {
            assert!(SendState::parse(token).is_some(), "{token} must parse");
        }
        let recipient_states = ["pending", "sent", "failed", "skipped"];
        for token in recipient_states {
            assert!(RecipientState::parse(token).is_some(), "{token} must parse");
        }
    }

    #[test]
    fn the_skip_reason_fits_what_the_database_allows() {
        // The `reason` CHECK caps it at 60 characters.
        assert!(!reason::TOPIC_DECLINED.is_empty());
        assert!(reason::TOPIC_DECLINED.chars().count() <= 60);
    }
}
