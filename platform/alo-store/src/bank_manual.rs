//! The manual pick (alo Finance, ADR 0035, wave B4.09c;
//! `docs/design/finance.md`, "Matching is three stages", stage 3).
//!
//! The first two stages guess. [`crate::bank_match`] guesses with certainty —
//! the payer quoted our own number and sent exactly what it owes — and
//! [`crate::bank_match_heuristic`] guesses with evidence and a score. This stage
//! does not guess at all: **a person states which document a line settled**, and
//! the store's job is to refuse only the things that are not true.
//!
//! # What is still refused, and why each one is money
//!
//! A pick is not a licence. Everything [`ensure_settleable`] demands still holds
//! — the line is still open, the document can take money at all, the money
//! arrives rather than leaves, the currencies agree — because none of those is a
//! matter of opinion. Two more rules are this stage's own:
//!
//! - **Never more than the document owes.** A line moving a cent more than the
//!   remaining debt is a split, a duplicate or a mistake, and attributing it
//!   would record a payment larger than the debt — the same reading
//!   [`crate::bank_match_heuristic`] takes when it declines to *offer* such a
//!   match. Less is fine and is the common case: a part payment is what the
//!   manual stage exists for.
//! - **The whole line, or nothing.** `bank_matches` is unique per line
//!   (migration 0143) and splitting one transfer across three invoices is the
//!   additive change that drops it. Until then, attributing part of a line would
//!   mark the line settled while some of the money it moved is attributed to
//!   nobody — invisible, and found at the year end.
//!
//! # What is deliberately **not** refused
//!
//! **The date window.** [`crate::bank_match::EXACT_WINDOW_DAYS`] bounds how far
//! a *guess* may reach, and the refusal it raises says, in as many words, "match
//! it by hand if it really is its payment". A pick that fell under the same rule
//! would take that sentence back. Money that arrived before the document was
//! issued is likewise allowed here: a deposit taken in advance is real, and
//! [`crate::billing_payments::AccountStore::record_billing_payment`] has allowed
//! it since B1.19.
//!
//! **The amount a person states** is compared, never trusted: it is what they
//! saw on the screen they clicked, and the line is what the bank said. They have
//! to agree, which is what makes a stale screen a refusal instead of a payment
//! for the wrong money.

use crate::account::AccountStore;
use crate::bank_import::BankLine;
use crate::bank_match::{MatchCandidate, ensure_settleable};
use crate::bank_reconcile::{ConfirmedMatch, LineSettlement};
use crate::error::{Result, StoreError};
use crate::id::{BankLineId, BillingInvoiceId, FinMatchRuleId};

/// The rule a manual pick has to satisfy: everything every stage demands, plus
/// this stage's own two — the whole line and no more than is owed.
///
/// Pure, like the two stages before it, and for the same reason: it is re-run
/// **under the row locks** by the settling path, so the rule a screen was shown
/// and the rule the money is booked against are one function.
///
/// # Errors
/// [`StoreError::Conflict`] when the line or the document is in no state to be
/// matched at all (already matched, ignored, a draft, a void document, a credit
/// note, one already settled); [`StoreError::Validation`] when the money moves
/// the wrong way, the currencies differ, the amount stated is not what the line
/// moves, or it is more than the document still owes.
///
/// No message quotes an amount, a name or a remittance: a refusal is read on a
/// screen and written to a log, and a bank line is the tenant's own money moving
/// (Law 1).
pub fn ensure_manual_match(
    line: &BankLine,
    candidate: &MatchCandidate,
    amount_cents: i64,
) -> Result<()> {
    ensure_settleable(line, candidate)?;
    if amount_cents != line.amount_cents {
        return Err(StoreError::Validation(
            "the amount to attribute is not what this bank line moves; one line settles one \
             document in full, and splitting a transfer across several is not supported yet"
                .to_owned(),
        ));
    }
    if amount_cents > candidate.outstanding_cents {
        return Err(StoreError::Validation(
            "this bank line moves more than that invoice still owes; a payment larger than the \
             debt is a split, a duplicate or a mistake"
                .to_owned(),
        ));
    }
    Ok(())
}

impl AccountStore {
    /// **Matches a staged line to one of this tenant's invoices because a person
    /// said so**: records the payment (a part payment when that is what it is),
    /// moves the books, counts the rule that proposed it, and marks the line
    /// matched — in one transaction.
    ///
    /// `amount_cents` is what the person saw attributed on their screen, and it
    /// is compared with what the bank said the line moves rather than believed.
    /// `rule_id` is the learned rule ([`crate::fin_match_rules`]) whose
    /// suggestion they took, when they took one: it is recorded on the match and
    /// its hit counted, both inside this transaction.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the line, the invoice or the rule is absent
    /// or another tenant's; [`StoreError::Conflict`] when the line is already
    /// matched or ignored, or the document cannot take money (a draft, a void
    /// one, a credit note, one already settled); [`StoreError::Validation`] when
    /// [`ensure_manual_match`] refuses, when the chart is missing a role, or
    /// when no reference rate covers the day the money arrived;
    /// [`StoreError::Db`] on failure.
    pub async fn match_bank_line(
        &self,
        line_id: &BankLineId,
        invoice_id: &BillingInvoiceId,
        amount_cents: i64,
        rule_id: Option<&FinMatchRuleId>,
    ) -> Result<ConfirmedMatch> {
        let line = self.bank_line(line_id).await?.ok_or(StoreError::NotFound)?;
        self.settle_bank_line(&LineSettlement {
            line: &line,
            invoice_id,
            amount_cents,
            rule_id,
            rule: ensure_manual_match,
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bank_import::BankLineStatus;
    use crate::billing_invoices::InvoiceStatus;
    use crate::id::{BankStatementId, BillingInvoiceId};
    use time::{Date, Month, OffsetDateTime};

    fn day(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).unwrap_or(Date::MIN)
    }

    fn line(amount_cents: i64) -> BankLine {
        BankLine {
            id: BankLineId::new("line-1".to_owned()),
            statement_id: BankStatementId::new("stmt-1".to_owned()),
            line_no: 1,
            booked_on: day(2026, Month::January, 14),
            value_on: day(2026, Month::January, 14),
            amount_cents,
            currency: "EUR".to_owned(),
            counterparty_name: "Kaffeehaus GmbH".to_owned(),
            counterparty_iban: String::new(),
            remittance: "unsere Bestellung 4711".to_owned(),
            bank_ref: "REF9".to_owned(),
            status: BankLineStatus::Unmatched,
            ignored_reason: String::new(),
            created_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    fn candidate(outstanding_cents: i64) -> MatchCandidate {
        MatchCandidate {
            invoice_id: BillingInvoiceId::new("inv-1".to_owned()),
            number: "INV-2026-00007".to_owned(),
            currency: "EUR".to_owned(),
            outstanding_cents,
            status: InvoiceStatus::Issued,
            is_credit_note: false,
            issue_date: Some(day(2026, Month::January, 5)),
        }
    }

    fn refused(result: Result<()>) -> String {
        match result {
            Err(StoreError::Validation(message) | StoreError::Conflict(message)) => message,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_person_may_pick_a_document_the_payer_never_named() {
        // The whole point of the stage: no number in the remittance, nothing
        // for either guessing stage to work with, and a bookkeeper who knows.
        assert!(ensure_manual_match(&line(130_700), &candidate(130_700), 130_700).is_ok());
    }

    #[test]
    fn a_part_payment_is_the_ordinary_case_here() {
        // €500 against €1 307.00 outstanding: not exact, not a guess, true.
        assert!(ensure_manual_match(&line(50_000), &candidate(130_700), 50_000).is_ok());
    }

    #[test]
    fn more_than_is_owed_is_refused_however_certain_the_person_is() {
        let message = refused(ensure_manual_match(
            &line(130_701),
            &candidate(130_700),
            130_701,
        ));
        assert!(message.contains("more than"), "{message}");
    }

    #[test]
    fn the_amount_stated_has_to_be_what_the_bank_said() {
        // A screen drawn before the line was read differently, or a client
        // proposing a split: both are the same refusal, and neither books money.
        for stated in [50_000_i64, 130_701, 0, -130_700] {
            let message = refused(ensure_manual_match(
                &line(130_700),
                &candidate(130_700),
                stated,
            ));
            assert!(message.contains("splitting a transfer"), "{message}");
        }
    }

    #[test]
    fn the_date_window_is_not_this_stages_rule() {
        // Four years late, and a person who knows it is that invoice. The exact
        // stage's own refusal tells them to do exactly this.
        let mut late = line(130_700);
        late.booked_on = day(2030, Month::January, 14);
        assert!(ensure_manual_match(&late, &candidate(130_700), 130_700).is_ok());

        // And a deposit that arrived before the document existed.
        let mut early = line(130_700);
        early.booked_on = day(2025, Month::December, 1);
        assert!(ensure_manual_match(&early, &candidate(130_700), 130_700).is_ok());
    }

    #[test]
    fn everything_that_is_not_a_matter_of_opinion_still_refuses() {
        for (mutate, expect) in [
            (
                Box::new(|c: &mut MatchCandidate| c.status = InvoiceStatus::Draft)
                    as Box<dyn Fn(&mut MatchCandidate)>,
                "draft",
            ),
            (
                Box::new(|c: &mut MatchCandidate| c.status = InvoiceStatus::Void),
                "cancelled",
            ),
            (
                Box::new(|c: &mut MatchCandidate| c.status = InvoiceStatus::Paid),
                "already settled",
            ),
            (
                Box::new(|c: &mut MatchCandidate| c.is_credit_note = true),
                "credit note",
            ),
            (
                Box::new(|c: &mut MatchCandidate| c.currency = "USD".to_owned()),
                "different currency",
            ),
        ] {
            let mut broken = candidate(130_700);
            mutate(&mut broken);
            let message = refused(ensure_manual_match(&line(130_700), &broken, 130_700));
            assert!(
                message.contains(expect),
                "expected {expect:?}, got {message}"
            );
        }

        // Money leaving the account never settles a receivable, whoever says so.
        let message = refused(ensure_manual_match(
            &line(-130_700),
            &candidate(130_700),
            -130_700,
        ));
        assert!(message.contains("money leaving"), "{message}");
    }

    #[test]
    fn a_line_that_is_no_longer_in_the_pile_refuses() {
        for (status, expect) in [
            (BankLineStatus::Matched, "already matched"),
            (BankLineStatus::Ignored, "not ours to book"),
        ] {
            let mut settled = line(130_700);
            settled.status = status;
            let message = refused(ensure_manual_match(&settled, &candidate(130_700), 130_700));
            assert!(message.contains(expect), "{message}");
        }
    }

    #[test]
    fn no_refusal_quotes_the_tenants_own_money_or_words() {
        let mut secret = line(130_701);
        secret.counterparty_name = "Kaffeehaus GmbH".to_owned();
        secret.remittance = "INV-2026-00009".to_owned();
        let message = refused(ensure_manual_match(&secret, &candidate(130_700), 130_701));
        assert!(!message.contains("130701"), "{message}");
        assert!(!message.contains("Kaffeehaus"), "{message}");
        assert!(!message.contains("INV-2026-00009"), "{message}");
    }
}
