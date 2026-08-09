//! Taking a match back (alo Finance, ADR 0035, wave B4.09c;
//! `docs/design/finance.md`, "Unmatching is real").
//!
//! [`crate::bank_reconcile`] is the act; this is the undo, and it is a different
//! act rather than the same one backwards. Three things happen, in one
//! transaction:
//!
//! 1. the settlement entry is **reversed** — a mirror entry
//!    ([`crate::fin_journal::reversal_entry`]), never a deletion, because the
//!    books are the one place in alo where what happened stays visible even when
//!    it was wrong;
//! 2. the payment is **deleted**, because it is not a thing that happened: no
//!    money arrived, and a payment row that stays would keep the invoice
//!    settled and the customer un-chased;
//! 3. the line goes back to `unmatched`, which is where a bookkeeper will find
//!    it again.
//!
//! The asymmetry between 1 and 2 is the point. The journal is a record of the
//! *books*, and a correction there is an event with a date; `billing_payments`
//! is a record of *money received*, and money that was never received has no
//! event to record. B1.19 took the same view when it made deletion the only
//! correction a payment has.
//!
//! # Only the last payment on a document can be taken back
//!
//! A settlement's receivable relief is **cumulative**
//! ([`crate::fin_rules::payment_settle_entry`]): each entry relieves the
//! difference between what the whole prefix of payments relieves and what the
//! shorter prefix did, which is what makes a settled document's receivable go to
//! exactly zero in both columns. Removing a payment from the *middle* of that
//! sequence would leave every later entry computed against a prefix that no
//! longer exists, and — for a document in a currency the books are not kept in —
//! a residue in the base column that no document explains.
//!
//! So a match whose payment is not the newest on its document is refused,
//! naming what to do: take the later one back first. That is one extra click in
//! the rare case, against a class of unexplainable ledger residue in the common
//! one.
//!
//! # What is deliberately not here
//!
//! **The invoice's issue entry stays.** The document is still issued and still
//! owed; only the money that never arrived is taken back.

use crate::account::AccountStore;
use crate::bank_import::BankLineStatus;
use crate::bank_reconcile::{BankMatch, BankMatchTarget};
use crate::billing_payments::PAYMENT_MAX_CENTS;
use crate::error::{Result, StoreError};
use crate::fin_journal::{EntrySource, SourceEvent, SourceKind, reversal_entry};
use crate::id::{BankLineId, BillingInvoiceId, BillingPaymentId, FinEntryId};

/// What taking a match back did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnmatchedLine {
    /// The line, now back in the pile.
    pub line_id: BankLineId,
    /// The document the money is no longer against.
    pub target: BankMatchTarget,
    /// What had been attributed to it.
    pub amount_cents: i64,
    /// The entry that reverses the settlement — the thing a reader of the books
    /// sees instead of a hole where an entry used to be.
    pub reversal_entry_id: FinEntryId,
}

impl AccountStore {
    /// **Takes back** the match on one of this tenant's staged lines: reverses
    /// the settlement, deletes the payment, and returns the line to the pile —
    /// in one transaction.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the line is absent or another tenant's, or
    /// carries no match; [`StoreError::Conflict`] when the line stopped being
    /// matched while this ran, when the match names a kind of document this
    /// build cannot un-book, or when a later payment on the same document has
    /// to be taken back first; [`StoreError::Validation`] when the stored match
    /// is not one this build understands; [`StoreError::Db`] on failure.
    pub async fn unmatch_bank_line(&self, line_id: &BankLineId) -> Result<UnmatchedLine> {
        let matched = self
            .bank_match(line_id)
            .await?
            .ok_or(StoreError::NotFound)?;
        let (invoice_id, payment_id, entry_id) = unbookable(&matched)?;
        // The entry and its postings are immutable once written, so reading
        // them before the transaction opens reads exactly what reading them
        // inside it would — and does not hold a second pool connection while a
        // row lock is out.
        let original = self
            .fin_journal_entry(entry_id)
            .await?
            .ok_or(StoreError::NotFound)?;

        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;

        // The line's lock is what serialises two people clicking undo, and it
        // is taken in the same order the settling path takes it.
        let locked_status: Option<String> = sqlx::query_scalar(
            "SELECT status FROM bank_lines WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
        )
        .bind(self.tenant.as_str())
        .bind(line_id.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        let locked_status = locked_status.ok_or(StoreError::NotFound)?;
        if BankLineStatus::parse(&locked_status) != Some(BankLineStatus::Matched) {
            return Err(StoreError::Conflict(
                "this bank line is not matched to anything".to_owned(),
            ));
        }

        // The authoritative read: what is about to have its money removed must
        // be the match that is there now, not the one a screen was drawn from.
        let locked = self
            .bank_match_on(&mut *tx, line_id)
            .await?
            .ok_or(StoreError::NotFound)?;
        if locked.id != matched.id || locked.payment_id.as_ref() != Some(payment_id) {
            return Err(StoreError::Conflict(
                "this bank line was matched to something else while it was being taken back"
                    .to_owned(),
            ));
        }

        // The cumulative-relief rule: only the newest payment on the document
        // can be taken back (see the module header).
        let payments = self.billing_payments_on(&mut *tx, invoice_id).await?;
        if payments.first().map(|payment| &payment.id) != Some(payment_id) {
            return Err(StoreError::Conflict(
                "another payment was recorded against that invoice after this one; take that one \
                 back first"
                    .to_owned(),
            ));
        }

        let reversal = reversal_entry(
            &original,
            Some(EntrySource {
                kind: SourceKind::Payment,
                id: payment_id.as_str().to_owned(),
                event: SourceEvent::Void,
            }),
        );
        let reversal_entry_id = self.post_fin_entry_in(&mut tx, &reversal).await?;

        // The match row goes first: `bank_matches.payment_id` refuses the
        // payment's deletion while it points at it (migration 0143, ON DELETE
        // RESTRICT), which is the database saying the same thing this file does
        // — a line must never claim to be settled by a payment that is gone.
        let removed = sqlx::query("DELETE FROM bank_matches WHERE tenant_id = $1 AND id = $2")
            .bind(self.tenant.as_str())
            .bind(locked.id.as_str())
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        if removed.rows_affected() != 1 {
            return Err(StoreError::Conflict(
                "this match was taken back while it was being taken back".to_owned(),
            ));
        }
        self.delete_billing_payment_in(&mut tx, invoice_id, payment_id)
            .await?;

        let moved = sqlx::query(
            "UPDATE bank_lines SET status = 'unmatched' \
             WHERE tenant_id = $1 AND id = $2 AND status = 'matched'",
        )
        .bind(self.tenant.as_str())
        .bind(line_id.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        if moved.rows_affected() != 1 {
            return Err(StoreError::Conflict(
                "this bank line stopped being matched while it was being taken back".to_owned(),
            ));
        }

        tx.commit().await.map_err(StoreError::Db)?;
        Ok(UnmatchedLine {
            line_id: line_id.clone(),
            target: matched.target,
            amount_cents: matched.amount_cents,
            reversal_entry_id,
        })
    }
}

/// The three links a match must carry for its money to be taken back, or the
/// reason it cannot be.
///
/// Every `'invoice'` match has all three — a CHECK in migration 0143 requires
/// them exactly for that kind — so this refuses only a match written by a build
/// that knows a kind this one does not.
fn unbookable(matched: &BankMatch) -> Result<(&BillingInvoiceId, &BillingPaymentId, &FinEntryId)> {
    let BankMatchTarget::Invoice(invoice_id) = &matched.target;
    let (Some(payment_id), Some(entry_id)) = (&matched.payment_id, &matched.entry_id) else {
        return Err(StoreError::Conflict(
            "this match booked no money, so there is none to take back".to_owned(),
        ));
    };
    if matched.amount_cents <= 0 || matched.amount_cents > PAYMENT_MAX_CENTS {
        return Err(StoreError::Validation(
            "this match records an amount this version cannot un-book".to_owned(),
        ));
    }
    Ok((invoice_id, payment_id, entry_id))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;
    use crate::id::{BankMatchId, UserId};
    use time::OffsetDateTime;

    fn matched(payment: Option<&str>, entry: Option<&str>, amount_cents: i64) -> BankMatch {
        BankMatch {
            id: BankMatchId::new("match-1".to_owned()),
            line_id: BankLineId::new("line-1".to_owned()),
            target: BankMatchTarget::Invoice(BillingInvoiceId::new("inv-1".to_owned())),
            amount_cents,
            payment_id: payment.map(|id| BillingPaymentId::new(id.to_owned())),
            entry_id: entry.map(|id| FinEntryId::new(id.to_owned())),
            rule_id: None,
            confirmed_by: UserId::new("user-1".to_owned()),
            confirmed_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn an_invoice_match_names_the_payment_and_the_entry_to_undo() {
        let matched = matched(Some("pay-1"), Some("entry-1"), 130_700);
        let (invoice, payment, entry) = unbookable(&matched).expect("all three links");
        assert_eq!(invoice.as_str(), "inv-1");
        assert_eq!(payment.as_str(), "pay-1");
        assert_eq!(entry.as_str(), "entry-1");
    }

    #[test]
    fn a_match_that_booked_nothing_has_nothing_to_take_back() {
        for (payment, entry) in [(None, Some("entry-1")), (Some("pay-1"), None), (None, None)] {
            let message = match unbookable(&matched(payment, entry, 130_700)) {
                Err(StoreError::Conflict(message)) => message,
                other => panic!("expected a conflict, got {other:?}"),
            };
            assert!(message.contains("no money"), "{message}");
        }
    }

    #[test]
    fn an_amount_this_build_cannot_unbook_refuses_before_the_transaction() {
        for amount in [0_i64, -130_700, PAYMENT_MAX_CENTS + 1] {
            let broken = matched(Some("pay-1"), Some("entry-1"), amount);
            assert!(
                matches!(unbookable(&broken), Err(StoreError::Validation(_))),
                "for {amount}"
            );
        }
    }
}
