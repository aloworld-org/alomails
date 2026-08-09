//! The line that is not ours to book (alo Finance, ADR 0035, wave B4.09c;
//! `docs/design/finance.md`, "Matching is three stages", stage 3).
//!
//! Not every line on a statement is a document of ours. A bank charge, a
//! transfer between two of the tenant's own accounts, a payment somebody keyed
//! in by hand months before the file was imported: each is real money that has
//! nothing here to match. Without a way to say so they stay on the
//! reconciliation screen forever, and a screen that never empties is a screen
//! nobody finishes reading — which is how the *matchable* lines get missed.
//!
//! # It moves no money, and that is the whole design
//!
//! Ignoring writes one column and one status. It posts nothing, records no
//! payment, and creates no match row, because nothing happened in the books:
//! the tenant is simply saying this movement is accounted for somewhere else, or
//! nowhere. That is why this file is three statements long and
//! [`crate::bank_unmatch`] is not.
//!
//! # The reason is required
//!
//! A blank reason is refused. "Ignored" with nothing beside it is the state a
//! bookkeeper cannot audit, cannot hand to their accountant and cannot undo with
//! any confidence six months later — and the person who has the answer is the
//! one clicking the button, once, now. Who ignored it and when are not stored
//! here: every mutating `/finance/*` route already writes an audit entry naming
//! the actor, the act and the line (B2.13), and a second answer to a question
//! that has one is how two answers start disagreeing.
//!
//! Taking it back clears the sentence with the status (migration 0145 makes that
//! an invariant, not a habit), so a line in the pile never carries a stale
//! explanation of a decision somebody already reversed.

use crate::account::AccountStore;
use crate::bank_import::{BankLine, BankLineStatus};
use crate::error::{Result, StoreError};
use crate::id::BankLineId;

/// The longest reason kept — a sentence, not a memo. Matches the column bound
/// in migration 0145.
pub const IGNORE_REASON_MAX_CHARS: usize = 200;

impl AccountStore {
    /// Says that a staged line settles nothing of this tenant's, with the
    /// reason it does not.
    ///
    /// Only an **unmatched** line can be ignored: one already matched is money
    /// in the books, and it has to be taken back
    /// ([`AccountStore::unmatch_bank_line`]) before it can be dismissed. Saying
    /// it again with a different reason is allowed — a person correcting their
    /// own sentence is not an error, and the reason a line carries should be the
    /// last one they meant.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the line is absent or another tenant's;
    /// [`StoreError::Validation`] when the reason is blank or longer than
    /// [`IGNORE_REASON_MAX_CHARS`]; [`StoreError::Conflict`] when the line is
    /// matched; [`StoreError::Db`] on failure.
    pub async fn ignore_bank_line(&self, line_id: &BankLineId, reason: &str) -> Result<BankLine> {
        let reason = reason.trim();
        if reason.is_empty() {
            return Err(StoreError::Validation(
                "say why this line is not ours to book; a line dismissed without a reason is one \
                 nobody can check later"
                    .to_owned(),
            ));
        }
        if reason.chars().count() > IGNORE_REASON_MAX_CHARS {
            return Err(StoreError::Validation(format!(
                "a reason may be at most {IGNORE_REASON_MAX_CHARS} characters"
            )));
        }
        self.move_bank_line(line_id, BankLineStatus::Ignored, reason)
            .await
    }

    /// Takes an ignored line back into the pile, clearing the reason with it.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the line is absent or another tenant's;
    /// [`StoreError::Conflict`] when the line is not ignored;
    /// [`StoreError::Db`] on failure.
    pub async fn unignore_bank_line(&self, line_id: &BankLineId) -> Result<BankLine> {
        self.move_bank_line(line_id, BankLineStatus::Unmatched, "")
            .await
    }

    /// Moves a line between `unmatched` and `ignored`, and answers it as it now
    /// stands.
    ///
    /// The `WHERE` clause carries the state the move is allowed **from**, so two
    /// people clicking at once do not both succeed and the second is told what
    /// happened rather than shown a line that says something else. A move that
    /// changed nothing is then read back once to tell "not this tenant's" (which
    /// is a [`StoreError::NotFound`]) from "not in that state" (a
    /// [`StoreError::Conflict`]) — never an oracle for the other tenant's ids,
    /// because the read is tenant-scoped too.
    async fn move_bank_line(
        &self,
        line_id: &BankLineId,
        to: BankLineStatus,
        reason: &str,
    ) -> Result<BankLine> {
        // Dismissing is allowed from either open state — saying it again with a
        // corrected sentence is a person fixing their own words, not an error —
        // and taking it back only from `ignored`.
        let from: &[&str] = match to {
            BankLineStatus::Ignored => &["unmatched", "ignored"],
            _ => &["ignored"],
        };
        let moved = sqlx::query(
            "UPDATE bank_lines SET status = $3, ignored_reason = $4 \
             WHERE tenant_id = $1 AND id = $2 AND status = ANY($5)",
        )
        .bind(self.tenant.as_str())
        .bind(line_id.as_str())
        .bind(to.as_str())
        .bind(reason)
        .bind(from)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;

        if moved.rows_affected() == 0 {
            let line = self.bank_line(line_id).await?.ok_or(StoreError::NotFound)?;
            return Err(StoreError::Conflict(match line.status {
                BankLineStatus::Matched => {
                    "this bank line is matched to a document; take that back before dismissing it"
                        .to_owned()
                }
                _ => "this bank line is not marked as not ours to book".to_owned(),
            }));
        }
        self.bank_line(line_id).await?.ok_or(StoreError::NotFound)
    }
}
