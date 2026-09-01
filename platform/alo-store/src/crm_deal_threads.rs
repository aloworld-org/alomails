//! The conversations a deal belongs to (alo CRM, ADR 0035, wave B2).
//!
//! This is the module's reason to exist: a deal does not need a plugin to see
//! the conversation it came from, because the conversation and the deal are rows
//! in the same database under the same tenant. It is also the place where a
//! careless design would quietly turn a private mailbox into a shared one, so
//! every function here is written against one boundary that the rest of CRM does
//! not have to think about.
//!
//! **A deal is tenant-wide; mail is per user.** `messages.user_id` scopes a
//! message to one colleague, while every member of the tenant reads every deal.
//! Three rules follow, and they are the whole design
//! (`docs/design/crm.md`, "Deal ↔ mail thread"):
//!
//! - **A link stores no message content** — not a body, not a participant list,
//!   not a count. It says *this deal and this conversation belong together*,
//!   plus who said so and when.
//! - **Writing a link requires the thread to resolve through the linker's own
//!   door.** A thread the requesting user has no message in is a
//!   [`StoreError::NotFound`], identical to one that does not exist, so a user
//!   cannot attach a conversation they have never seen by guessing an id.
//! - **Reading resolves through the reader's own door.** A colleague who holds
//!   the conversation sees its subject and can open it in mail; one who does not
//!   sees that a conversation is linked, its base subject, and who linked it —
//!   the useful answer being "ask Sam", not a silent gap.
//!
//! Suggestion is the pure half ([`crate::crm_thread_match`]) folded over a page
//! of the requesting user's **own** recent mail. It never links anything: a
//! proposal becomes a link only on an explicit write.

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::crm_deals::NewDeal;
use crate::crm_thread_match::{MatchReason, match_message, targets};
use crate::error::{Result, StoreError};
use crate::id::{CrmDealId, CrmPipelineId, CrmStageId, ThreadId};

/// The most conversations one deal may carry.
///
/// A deal that names a hundred threads has stopped being a record of an
/// opportunity and become a mailbox; the cap is what keeps the drawer readable
/// and the read bounded.
pub const DEAL_THREADS_MAX: i64 = 100;

/// How many of the requesting user's recent messages a suggestion pass reads.
///
/// A bounded window, not "the mailbox": the point is to propose the
/// conversations that are live right now, and a heuristic that walks ten years
/// of mail to find one more candidate costs every user the wait.
pub const SUGGESTION_SCAN_MESSAGES: i64 = 500;

/// The most suggestions one call will answer with.
pub const SUGGESTIONS_MAX: usize = 50;

/// A conversation linked to a deal, as the reader may see it.
#[derive(Debug, Clone)]
pub struct DealThread {
    /// The conversation.
    pub thread_id: ThreadId,
    /// What to call it. The subject of the **reader's own** newest message in
    /// the thread when they hold it; otherwise `threads.subject_base`, the
    /// normalised (lower-cased, `Re:`-stripped) label that is tenant-scoped by
    /// construction and the one field a link deliberately lets cross a mailbox
    /// boundary.
    pub subject: String,
    /// Whether **this reader** holds the conversation and can open it in mail.
    /// A colleague who does not still sees that it is linked, and who linked it.
    pub readable: bool,
    /// The user who confirmed the link.
    pub linked_by: String,
    /// When they did.
    pub linked_at: OffsetDateTime,
}

/// A conversation proposed for a deal, with the reason it was proposed.
#[derive(Debug, Clone)]
pub struct ThreadSuggestion {
    /// The conversation.
    pub thread_id: ThreadId,
    /// The subject of the requesting user's own newest matching message — this
    /// is always their own mail, so no boundary is crossed.
    pub subject: String,
    /// Why it matched: an exact address, or the customer's domain.
    pub reason: MatchReason,
    /// The correspondent that caused the match, so a user can see *why* and the
    /// proposal is reviewable rather than magic.
    pub matched_address: String,
    /// When the newest matching message arrived.
    pub last_message_at: OffsetDateTime,
}

/// One row of the requesting user's own mail that a suggestion pass reads.
#[derive(sqlx::FromRow)]
struct ScannedMessage {
    thread_id: String,
    subject: String,
    from_addr: String,
    to_addrs: String,
    received_at: OffsetDateTime,
}

/// A candidate under construction while the scan folds messages into threads.
struct Candidate {
    subject: String,
    reason: MatchReason,
    matched_address: String,
    last_message_at: OffsetDateTime,
}

impl AccountStore {
    /// Creates an opportunity and links the source conversation in one
    /// transaction. The thread is checked through the acting user's mailbox
    /// before any deal row is written.
    pub async fn create_crm_deal_from_thread(
        &self,
        pipeline: &CrmPipelineId,
        stage: &CrmStageId,
        input: &NewDeal,
        thread: &ThreadId,
    ) -> Result<CrmDealId> {
        let normalized = self.normalize_deal(input).await?;
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let holds: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM messages \
             WHERE tenant_id = $1 AND user_id = $2 AND thread_id = $3)",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(thread.as_str())
        .fetch_one(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        if !holds {
            return Err(StoreError::NotFound);
        }

        self.share_crm_pipeline(&mut tx, pipeline).await?;
        let deal = self
            .insert_crm_deal_in(&mut tx, pipeline, stage, &normalized)
            .await?;
        sqlx::query(
            "INSERT INTO crm_deal_threads (tenant_id, deal_id, thread_id, linked_by) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(self.tenant.as_str())
        .bind(deal.as_str())
        .bind(thread.as_str())
        .bind(self.user.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(deal)
    }

    /// Links a conversation to a deal, on the confirmation of a user who can
    /// already see it.
    ///
    /// Returns `true` when a link was written and `false` when it was already
    /// there: linking twice is the same link, not an error a user has to read.
    ///
    /// The thread must resolve through **this** user's own door. A thread of
    /// another tenant, one that does not exist, and one the requesting user
    /// simply has no message in are the same [`StoreError::NotFound`] — no
    /// existence oracle, the doctrine every CRM route already follows.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the deal is not this tenant's, or the
    /// thread does not resolve through this user's own mail;
    /// [`StoreError::Conflict`] beyond [`DEAL_THREADS_MAX`];
    /// [`StoreError::Db`] on failure.
    pub async fn link_crm_deal_thread(&self, deal: &CrmDealId, thread: &ThreadId) -> Result<bool> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        // The deal's own row lock: it is both the existence check and what
        // serialises two colleagues linking at once, so the cap below cannot be
        // walked past by a concurrent write.
        sqlx::query_scalar::<_, String>(
            "SELECT id FROM crm_deals WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
        )
        .bind(self.tenant.as_str())
        .bind(deal.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(StoreError::Db)?
        .ok_or(StoreError::NotFound)?;

        // The linker's own door. Scoped to (tenant, user) because a thread id is
        // not authority for what its messages say.
        let holds: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM messages \
             WHERE tenant_id = $1 AND user_id = $2 AND thread_id = $3)",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(thread.as_str())
        .fetch_one(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        if !holds {
            return Err(StoreError::NotFound);
        }

        let already: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM crm_deal_threads \
             WHERE tenant_id = $1 AND deal_id = $2 AND thread_id = $3)",
        )
        .bind(self.tenant.as_str())
        .bind(deal.as_str())
        .bind(thread.as_str())
        .fetch_one(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        if already {
            // Idempotent, and deliberately checked before the cap: a deal that is
            // full must still answer "yes, that one is linked".
            tx.commit().await.map_err(StoreError::Db)?;
            return Ok(false);
        }

        let linked: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM crm_deal_threads WHERE tenant_id = $1 AND deal_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(deal.as_str())
        .fetch_one(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        if linked >= DEAL_THREADS_MAX {
            return Err(StoreError::Conflict(format!(
                "a deal may hold at most {DEAL_THREADS_MAX} linked conversations"
            )));
        }

        sqlx::query(
            "INSERT INTO crm_deal_threads (tenant_id, deal_id, thread_id, linked_by) \
             VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING",
        )
        .bind(self.tenant.as_str())
        .bind(deal.as_str())
        .bind(thread.as_str())
        .bind(self.user.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(true)
    }

    /// The conversations linked to a deal, most recently linked first.
    ///
    /// Every row is resolved through **this** reader's own door: `readable` says
    /// whether they hold the conversation, and the subject is their own
    /// message's when they do. A reader who does not hold it still sees the
    /// link, the base subject and who linked it.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the deal is not this tenant's — never an
    /// empty list, which would be an existence oracle;
    /// [`StoreError::Db`] on failure.
    pub async fn crm_deal_threads(&self, deal: &CrmDealId) -> Result<Vec<DealThread>> {
        if self.crm_deal(deal).await?.is_none() {
            return Err(StoreError::NotFound);
        }
        let rows = sqlx::query_as::<_, (String, Option<String>, String, String, OffsetDateTime)>(
            "SELECT dt.thread_id, NULLIF(btrim(m.subject), ''), t.subject_base, \
                    dt.linked_by, dt.linked_at \
             FROM crm_deal_threads dt \
             JOIN threads t ON t.id = dt.thread_id AND t.tenant_id = dt.tenant_id \
             LEFT JOIN LATERAL ( \
                 SELECT subject FROM messages \
                 WHERE tenant_id = dt.tenant_id AND user_id = $2 AND thread_id = dt.thread_id \
                 ORDER BY received_at DESC, id DESC LIMIT 1 \
             ) m ON true \
             WHERE dt.tenant_id = $1 AND dt.deal_id = $3 \
             ORDER BY dt.linked_at DESC, dt.thread_id",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(deal.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows
            .into_iter()
            .map(
                |(thread_id, own_subject, base, linked_by, linked_at)| DealThread {
                    thread_id: ThreadId::new(thread_id),
                    readable: own_subject.is_some(),
                    subject: own_subject.unwrap_or(base),
                    linked_by,
                    linked_at,
                },
            )
            .collect())
    }

    /// Removes a link.
    ///
    /// Any member of the tenant may unlink, including one who cannot open the
    /// conversation: the link is a tenant-wide record, and a link left by a
    /// colleague who has since left would otherwise be permanent. Removing it
    /// destroys nothing — the mail is untouched, because the link never held it.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the deal is not this tenant's or the
    /// conversation is not linked to it; [`StoreError::Db`] on failure.
    pub async fn unlink_crm_deal_thread(&self, deal: &CrmDealId, thread: &ThreadId) -> Result<()> {
        let done = sqlx::query(
            "DELETE FROM crm_deal_threads \
             WHERE tenant_id = $1 AND deal_id = $2 AND thread_id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(deal.as_str())
        .bind(thread.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Conversations worth linking to a deal, computed over the **requesting
    /// user's own** recent mail. It links nothing.
    ///
    /// The deal's addresses — its contact's, and its customer's when it has one
    /// — are matched against the correspondents of a bounded page of this user's
    /// messages ([`SUGGESTION_SCAN_MESSAGES`]). An exact address match ranks
    /// above a domain match, and a free-mail domain never matches by domain at
    /// all ([`crate::crm_thread_match`]). Conversations already linked to the
    /// deal are left out, because proposing them again is noise.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the deal is not this tenant's;
    /// [`StoreError::Db`] on failure.
    pub async fn suggest_crm_deal_threads(
        &self,
        deal: &CrmDealId,
        limit: usize,
    ) -> Result<Vec<ThreadSuggestion>> {
        let record = self.crm_deal(deal).await?.ok_or(StoreError::NotFound)?;
        let limit = limit.clamp(1, SUGGESTIONS_MAX);
        let customer_email = match &record.customer_id {
            Some(id) => self
                .billing_customer(id)
                .await?
                .and_then(|customer| customer.email),
            None => None,
        };
        let wanted = targets(&[
            record.contact_email.as_str(),
            customer_email.as_deref().unwrap_or_default(),
        ]);
        if wanted.is_empty() {
            // A deal nobody has given an address has nothing to match on. An
            // empty list is the honest answer, not every recent conversation.
            return Ok(Vec::new());
        }

        let scanned = sqlx::query_as::<_, ScannedMessage>(
            "SELECT m.thread_id, m.subject, m.from_addr, m.to_addrs, m.received_at \
             FROM messages m \
             WHERE m.tenant_id = $1 AND m.user_id = $2 \
               AND NOT EXISTS (SELECT 1 FROM crm_deal_threads dt \
                               WHERE dt.tenant_id = m.tenant_id AND dt.deal_id = $3 \
                                 AND dt.thread_id = m.thread_id) \
             ORDER BY m.received_at DESC, m.id DESC LIMIT $4",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(deal.as_str())
        .bind(SUGGESTION_SCAN_MESSAGES)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;

        Ok(rank(&wanted, scanned, limit))
    }
}

/// Folds a page of messages into one candidate per conversation, best reason
/// and newest message winning, and orders the result: address matches first,
/// then the most recent conversation.
///
/// Pure, so the ranking rule is testable without a database — the database part
/// is only "which messages are this user's", which the query above owns.
fn rank(wanted: &[String], scanned: Vec<ScannedMessage>, limit: usize) -> Vec<ThreadSuggestion> {
    let mut best: Vec<(String, Candidate)> = Vec::new();
    for message in scanned {
        let Some((reason, matched_address)) =
            match_message(wanted, &[&message.from_addr, &message.to_addrs])
        else {
            continue;
        };
        let fresh = Candidate {
            subject: message.subject,
            reason,
            matched_address,
            last_message_at: message.received_at,
        };
        match best.iter_mut().find(|(id, _)| *id == message.thread_id) {
            // The scan is newest-first, so the first message of a conversation
            // is the newest one: a later row only ever improves the *reason*,
            // never the subject or the time.
            Some((_, held)) if fresh.reason > held.reason => {
                held.reason = fresh.reason;
                held.matched_address = fresh.matched_address;
            }
            Some(_) => {}
            None => best.push((message.thread_id, fresh)),
        }
    }
    best.sort_by(|(a_id, a), (b_id, b)| {
        b.reason
            .cmp(&a.reason)
            .then(b.last_message_at.cmp(&a.last_message_at))
            .then(a_id.cmp(b_id))
    });
    best.truncate(limit);
    best.into_iter()
        .map(|(thread_id, c)| ThreadSuggestion {
            thread_id: ThreadId::new(thread_id),
            subject: c.subject,
            reason: c.reason,
            matched_address: c.matched_address,
            last_message_at: c.last_message_at,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(thread: &str, subject: &str, from: &str, to: &str, minute: u8) -> ScannedMessage {
        ScannedMessage {
            thread_id: thread.to_owned(),
            subject: subject.to_owned(),
            from_addr: from.to_owned(),
            to_addrs: to.to_owned(),
            received_at: OffsetDateTime::UNIX_EPOCH + time::Duration::minutes(minute.into()),
        }
    }

    fn wanted() -> Vec<String> {
        targets(&["ada@acme.test"])
    }

    #[test]
    fn a_conversation_appears_once_however_many_of_its_messages_match() {
        // Newest first, as the query returns them.
        let ranked = rank(
            &wanted(),
            vec![
                message("thr_1", "Re: Renewal", "ada@acme.test", "me@ourco.test", 9),
                message("thr_1", "Renewal", "me@ourco.test", "ada@acme.test", 3),
            ],
            10,
        );
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].thread_id.as_str(), "thr_1");
        // The newest message names the conversation.
        assert_eq!(ranked[0].subject, "Re: Renewal");
        assert_eq!(ranked[0].reason, MatchReason::Address);
    }

    #[test]
    fn an_address_match_ranks_above_a_domain_match_however_old_it_is() {
        let ranked = rank(
            &wanted(),
            vec![
                message("thr_new", "Colleague", "bob@acme.test", "me@ourco.test", 50),
                message(
                    "thr_old",
                    "The contact",
                    "ada@acme.test",
                    "me@ourco.test",
                    1,
                ),
            ],
            10,
        );
        assert_eq!(
            ranked
                .iter()
                .map(|s| s.thread_id.as_str().to_owned())
                .collect::<Vec<_>>(),
            vec!["thr_old".to_owned(), "thr_new".to_owned()]
        );
        assert_eq!(ranked[0].reason, MatchReason::Address);
        assert_eq!(ranked[1].reason, MatchReason::Domain);
        assert_eq!(ranked[1].matched_address, "bob@acme.test");
    }

    #[test]
    fn a_later_message_can_only_improve_the_reason() {
        // The conversation was found by a colleague's domain, then an older
        // message in it turns out to be from the contact themselves.
        let ranked = rank(
            &wanted(),
            vec![
                message("thr_1", "Newest", "bob@acme.test", "me@ourco.test", 9),
                message("thr_1", "Older", "ada@acme.test", "me@ourco.test", 2),
            ],
            10,
        );
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].reason, MatchReason::Address);
        assert_eq!(ranked[0].matched_address, "ada@acme.test");
        assert_eq!(
            ranked[0].subject, "Newest",
            "the newest message still names it"
        );
    }

    #[test]
    fn conversations_that_match_nothing_are_not_proposed() {
        let ranked = rank(
            &wanted(),
            vec![
                message("thr_1", "Lunch", "friend@gmail.com", "me@ourco.test", 9),
                message("thr_2", "Invoice", "billing@other.test", "me@ourco.test", 8),
            ],
            10,
        );
        assert!(ranked.is_empty());
    }

    #[test]
    fn the_limit_keeps_the_strongest_matches() {
        let ranked = rank(
            &wanted(),
            vec![
                message("thr_a", "a", "bob@acme.test", "", 9),
                message("thr_b", "b", "ada@acme.test", "", 8),
                message("thr_c", "c", "carol@acme.test", "", 7),
            ],
            2,
        );
        assert_eq!(ranked.len(), 2);
        assert_eq!(ranked[0].thread_id.as_str(), "thr_b");
        assert_eq!(ranked[1].thread_id.as_str(), "thr_a");
    }

    #[test]
    fn ties_break_on_the_conversation_id_so_the_order_is_stable() {
        let ranked = rank(
            &wanted(),
            vec![
                message("thr_z", "z", "ada@acme.test", "", 5),
                message("thr_a", "a", "ada@acme.test", "", 5),
            ],
            10,
        );
        assert_eq!(ranked[0].thread_id.as_str(), "thr_a");
        assert_eq!(ranked[1].thread_id.as_str(), "thr_z");
    }
}
