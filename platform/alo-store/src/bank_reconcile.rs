//! Confirming what a bank line was (alo Finance, ADR 0035, wave B4.09a;
//! `docs/design/finance.md`, "The bank and reconciliation").
//!
//! [`crate::bank_import`] stages what the bank said happened and posts nothing.
//! [`crate::bank_match`] and [`crate::bank_match_heuristic`] say, arithmetically
//! and without a database, which staged line looks like which document, and
//! [`crate::bank_suggest`] folds those rules over a tenant's ledger. **This file
//! is the verb between them**, and it is the one that touches money:
//!
//! 1. a person picks a line and the document it settles;
//! 2. the exact rule is re-run on the server, against the line and the document
//!    as they are *now* and under their row locks — a suggestion a client sends
//!    back is not evidence;
//! 3. the payment is recorded, the receivable is relieved in the journal, and
//!    the line is marked matched.
//!
//! **All three in one transaction.** A tenant must never hold a payment nobody
//! booked, an entry no payment explains, or a line that says it is settled by a
//! match that is not there. That is why this module reaches for the in-transaction
//! forms of the payment door and the journal door
//! ([`AccountStore::record_billing_payment_in`],
//! [`AccountStore::post_fin_entry_in`]) rather than calling their public
//! siblings in sequence.
//!
//! # Reconciliation is where the books actually open
//!
//! Nothing in alo has ever called [`AccountStore::post_invoice_issue`] from a
//! request: B4.04 wrote the posting rules and left the wiring for the periods
//! and the backfill (B4.10), so a tenant who has been invoicing since B1 has an
//! empty journal. Relieving a receivable that was never booked would leave the
//! customer's ledger negative and every aged-debtors report wrong, so a
//! confirmation **books the invoice's issue too** when it is not in the books —
//! at the document's own issue date, which is exactly the entry the backfill
//! would have written. [`ConfirmedMatch::invoice_booked_now`] says when that
//! happened, because a bookkeeper watching their first entries appear deserves
//! to be told which act created them.
//!
//! The one thing a confirmation will not do is invent a chart. A tenant whose
//! chart has no `ar` account is refused, naming the role and the screen that
//! fixes it — the same refusal every other booking path gives, for the same
//! reason: `suspense` is for money whose owner is unknown, not for a
//! configuration mistake nobody would find until the year end.
//!
//! # One transaction, two doors into it
//!
//! [`AccountStore::settle_bank_line`] is that transaction, and it belongs to
//! nobody in particular: this file's [`AccountStore::confirm_bank_match`] takes
//! it with the exact rule, and [`crate::bank_manual`] takes it with the rule a
//! person's own pick has to satisfy. The **rule** is what differs between the
//! two stages; everything after it — the locks, the issue that has to be in the
//! books before it can be relieved, the payment, the settlement, the row, the
//! line's status — is the same act and is stated once.
//!
//! # What is deliberately not here
//!
//! **Unmatching** is [`crate::bank_unmatch`] and **ignoring** is
//! [`crate::bank_ignore`] — one file per verb, because taking money back out of
//! the books is a different act from putting it in and neither should be able
//! to change under the other's tests. And **no screen**: B4.13b is the
//! reconciliation UI, and it calls the routes in `finance_bank_match.rs`.

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::bank_import::{BankLine, BankLineStatus};
use crate::bank_match::{MatchCandidate, ensure_exact_match};
use crate::billing_invoices::{Invoice, InvoiceStatus};
use crate::billing_payments::{
    NewPayment, PAYMENT_REFERENCE_MAX_CHARS, Settlement, payment_in_sequence,
};
use crate::error::{Result, StoreError};
use crate::fin_accounts::AccountRole;
use crate::fin_journal::{EntrySource, SourceEvent, SourceKind};
use crate::fin_rules::{
    InvoiceAccounts, PaymentAccounts, invoice_issue_entry, payment_settle_entry,
    payment_settlement_role, settlement_needs_exchange_account,
};
use crate::id::{
    BankLineId, BankMatchId, BillingInvoiceId, BillingPaymentId, FinEntryId, FinMatchRuleId, UserId,
};

/// The method a confirmed bank match records its payment as.
///
/// A payment's method is free text (B1.19) and it decides one thing here: the
/// account the money landed in, through
/// [`crate::fin_rules::payment_settlement_role`]. Money that arrived on a bank
/// statement arrived in the bank, so the word is fixed rather than asked for —
/// and it is the stored *datum*, not a label: a screen that wants to say it in
/// French translates the token it reads, exactly as it does for any other
/// method a colleague typed.
pub const BANK_MATCH_METHOD: &str = "bank transfer";

/// The columns every read of a match selects, in [`MatchRow`] order.
const MATCH_COLS: &str = "id, line_id, target_kind, target_id, amount_cents, payment_id, \
     entry_id, rule_id, confirmed_by, confirmed_at";

/// What a bank line turned out to be.
///
/// An enum rather than a nullable column per document type: a bill (B5) and an
/// expense reimbursement become variants without touching a row already
/// written, and no reader has to decide what two non-null links at once would
/// mean.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BankMatchTarget {
    /// One of our own invoices, settled by money arriving.
    Invoice(BillingInvoiceId),
}

impl BankMatchTarget {
    /// The stored word for the kind of document this is.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Invoice(_) => "invoice",
        }
    }

    /// The stored id of the document.
    #[must_use]
    pub fn id(&self) -> &str {
        match self {
            Self::Invoice(id) => id.as_str(),
        }
    }

    /// The target a stored (kind, id) pair names, or `None` when the kind is
    /// not one this build knows — a schema disagreement, which is honest to
    /// fail on rather than to guess through, since the guess would be about
    /// what money settled.
    #[must_use]
    pub fn parse(kind: &str, id: &str) -> Option<Self> {
        match kind {
            "invoice" => Some(Self::Invoice(BillingInvoiceId::new(id.to_owned()))),
            _ => None,
        }
    }
}

/// A stored, confirmed match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BankMatch {
    /// Our id for the confirmation.
    pub id: BankMatchId,
    /// The staged line it settles.
    pub line_id: BankLineId,
    /// What the line turned out to be.
    pub target: BankMatchTarget,
    /// What of the line was attributed to that document, in the line's own
    /// currency and with the line's own sign.
    pub amount_cents: i64,
    /// The payment this confirmation created.
    pub payment_id: Option<BillingPaymentId>,
    /// The journal entry that payment's settlement posted.
    pub entry_id: Option<FinEntryId>,
    /// The learned rule that proposed it (B4.09b), or `None` — always `None`
    /// for the exact stage, which needs no rule: the payer quoted our number.
    pub rule_id: Option<String>,
    /// Who confirmed it.
    pub confirmed_by: UserId,
    /// When.
    pub confirmed_at: OffsetDateTime,
}

/// What confirming did — the match, and what it did to the books.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmedMatch {
    /// The stored confirmation.
    pub matched: BankMatch,
    /// The entry the invoice's **issue** has in the books.
    pub invoice_entry_id: FinEntryId,
    /// Whether this confirmation is what put the invoice in the books, rather
    /// than finding it already there.
    pub invoice_booked_now: bool,
}

/// What a caller has decided a staged line settles, and the rule that has to
/// still hold when the row locks are taken.
///
/// The rule is a plain function pointer rather than a closure on purpose: the
/// two stages that settle a line differ in **exactly** this one thing, and
/// making it an argument is what stops the transaction below from being written
/// twice and drifting.
pub(crate) struct LineSettlement<'a> {
    /// The staged line, as it read outside the transaction.
    pub line: &'a BankLine,
    /// The document it settles.
    pub invoice_id: &'a BillingInvoiceId,
    /// What of the line is attributed to that document.
    pub amount_cents: i64,
    /// The learned rule that proposed it, whose hit this settlement counts.
    pub rule_id: Option<&'a FinMatchRuleId>,
    /// The stage's rule, re-run under the row locks against the line and the
    /// document **as they are then**.
    pub rule: fn(&BankLine, &MatchCandidate, i64) -> Result<()>,
}

impl AccountStore {
    /// **Confirms** that a staged line is the settlement of one of this
    /// tenant's invoices: records the payment, moves the books, and marks the
    /// line matched — in one transaction.
    ///
    /// The exact rule is re-derived here, twice: once from the documents as
    /// they read outside the transaction, and once **under the row locks** of
    /// the line and the invoice, because between a suggestion appearing on a
    /// screen and a person clicking it the money may already have been
    /// accounted for by somebody else. Two colleagues confirming the same line,
    /// or two lines against the same invoice, serialise on those locks and the
    /// second one is refused rather than doubling the payment.
    ///
    /// The payment it records is dated the day the **bank booked** the money
    /// (not the day it was reconciled), carries the bank's own reference, and
    /// its method is [`BANK_MATCH_METHOD`], which is what decides the account
    /// the money landed in.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the line or the invoice is absent or
    /// another tenant's; [`StoreError::Conflict`] when the line is already
    /// matched or ignored, or the document cannot take money (a draft, a void
    /// one, a credit note, one already settled); [`StoreError::Validation`]
    /// when the line and the document are not an exact match (the number is not
    /// quoted, the money goes the wrong way, the currencies differ, the amount
    /// is not what is owed, the dates cannot be reconciled), when the chart is
    /// missing a role, or when no reference rate covers the day the money
    /// arrived; [`StoreError::Db`] on failure.
    pub async fn confirm_bank_match(
        &self,
        line_id: &BankLineId,
        target: &BankMatchTarget,
    ) -> Result<ConfirmedMatch> {
        let BankMatchTarget::Invoice(invoice_id) = target;
        let line = self.bank_line(line_id).await?.ok_or(StoreError::NotFound)?;
        self.settle_bank_line(&LineSettlement {
            // The exact rule *is* "the line moves what the document owes", so
            // the amount attributed is the whole line and the rule is what
            // proves it.
            amount_cents: line.amount_cents,
            line: &line,
            invoice_id,
            rule_id: None,
            rule: |line, candidate, _amount| ensure_exact_match(line, candidate).map(drop),
        })
        .await
    }

    /// The transaction both stages settle a line in: the rule, the locks, the
    /// issue that has to be in the books before it can be relieved, the
    /// payment, the settlement, the match row and the line's status.
    ///
    /// The rule is re-derived twice: once from the documents as they read
    /// outside the transaction, and once **under the row locks** of the line
    /// and the invoice, because between a screen being drawn and a person
    /// clicking it the money may already have been accounted for by somebody
    /// else. Two colleagues settling the same line, or two lines against the
    /// same invoice, serialise on those locks and the second is refused rather
    /// than doubling the payment.
    ///
    /// # Errors
    /// As [`AccountStore::confirm_bank_match`], plus [`StoreError::NotFound`]
    /// when a named rule is not this tenant's.
    pub(crate) async fn settle_bank_line(
        &self,
        settlement: &LineSettlement<'_>,
    ) -> Result<ConfirmedMatch> {
        let LineSettlement {
            line,
            invoice_id,
            amount_cents,
            rule_id,
            rule,
        } = *settlement;
        let document = self
            .billing_invoice(invoice_id)
            .await?
            .ok_or(StoreError::NotFound)?;
        // The first pass answers the caller before anything is locked, with the
        // same words the second one would use.
        rule(
            line,
            &match_candidate(
                &document.invoice,
                document.totals.gross_cents,
                document.paid_cents,
            ),
            amount_cents,
        )?;

        // Everything the rules need, resolved before the transaction opens: a
        // chart lookup or a rate lookup while holding a row lock would hold it
        // for the length of another query, and a refusal here has nothing to
        // roll back.
        let base_currency = self.billing_base_currency().await?;
        let invoice_accounts = InvoiceAccounts {
            ar: self.fin_account_required(AccountRole::Ar).await?,
            revenue: self.fin_account_required(AccountRole::Revenue).await?,
            vat_output: self.fin_account_required(AccountRole::VatOutput).await?,
        };
        let payment_accounts = PaymentAccounts {
            settled_into: self
                .fin_account_required(payment_settlement_role(BANK_MATCH_METHOD))
                .await?,
            ar: invoice_accounts.ar.clone(),
            fx_diff: if settlement_needs_exchange_account(&document, &base_currency) {
                Some(self.fin_account_required(AccountRole::FxDiff).await?)
            } else {
                None
            },
        };
        let settled_at = self
            .settlement_rate(&document.invoice.currency, &base_currency, line.booked_on)
            .await?;

        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;

        // The line first, then the document: one order, taken by the only path
        // that touches both.
        let locked_status: Option<String> = sqlx::query_scalar(
            "SELECT status FROM bank_lines WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
        )
        .bind(self.tenant.as_str())
        .bind(line.id.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        let locked_status = locked_status.ok_or(StoreError::NotFound)?;
        let locked_status = BankLineStatus::parse(&locked_status).ok_or_else(|| {
            StoreError::Db(sqlx::Error::Decode(
                "bank_lines.status is not a known status".into(),
            ))
        })?;

        let locked_invoice: Option<(String, bool)> = sqlx::query_as(
            "SELECT status, is_credit_note FROM billing_invoices \
             WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
        )
        .bind(self.tenant.as_str())
        .bind(invoice_id.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        let (locked_invoice_status, is_credit_note) = locked_invoice.ok_or(StoreError::NotFound)?;
        let locked_paid: Option<i64> = sqlx::query_scalar(
            "SELECT sum(amount_cents)::bigint FROM billing_payments \
             WHERE tenant_id = $1 AND invoice_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(invoice_id.as_str())
        .fetch_one(&mut *tx)
        .await
        .map_err(StoreError::Db)?;

        // The authoritative pass. The line's own fields cannot change once it
        // is staged (nothing updates them), so the locked reads are of the two
        // things that can: where the line stands, and what the document is
        // still owed.
        let mut locked_line = line.clone();
        locked_line.status = locked_status;
        let mut locked_candidate = match_candidate(
            &document.invoice,
            document.totals.gross_cents,
            locked_paid.unwrap_or(0),
        );
        locked_candidate.status =
            InvoiceStatus::parse(&locked_invoice_status).ok_or_else(|| {
                StoreError::Db(sqlx::Error::Decode(
                    "billing_invoices.status is not a known status".into(),
                ))
            })?;
        locked_candidate.is_credit_note = is_credit_note;
        rule(&locked_line, &locked_candidate, amount_cents)?;

        // The receivable has to be in the books before it can be relieved. It
        // usually is not, because nothing else in alo books an issue yet.
        let issue = EntrySource {
            kind: SourceKind::Invoice,
            id: invoice_id.as_str().to_owned(),
            event: SourceEvent::Issue,
        };
        let (invoice_entry_id, invoice_booked_now) =
            match self.fin_entry_for_source_on(&mut *tx, &issue).await? {
                Some(entry) => (entry, false),
                None => {
                    let entry = invoice_issue_entry(&document, &base_currency, &invoice_accounts)?;
                    (self.post_fin_entry_in(&mut tx, &entry).await?, true)
                }
            };

        let payment_id = self
            .record_billing_payment_in(
                &mut tx,
                invoice_id,
                &NewPayment {
                    paid_on: Some(line.booked_on),
                    amount_cents,
                    method: BANK_MATCH_METHOD.to_owned(),
                    reference: payment_reference(line),
                },
            )
            .await?;
        let payments = self.billing_payments_on(&mut *tx, invoice_id).await?;
        let (payment, paid_before_cents) = payment_in_sequence(payments, &payment_id)?;
        let settlement = payment_settle_entry(
            &payment,
            &document,
            paid_before_cents,
            &base_currency,
            &settled_at,
            &payment_accounts,
        )?;
        let entry_id = self.post_fin_entry_in(&mut tx, &settlement).await?;

        // The rule that proposed this match is counted **in the same
        // transaction** as the match itself, which is also what proves the rule
        // is this tenant's: a hit counted outside could survive a settlement
        // that rolled back, and a counter nobody can explain is worse than no
        // counter.
        if let Some(rule_id) = rule_id {
            self.fin_match_rule_hit_in(&mut tx, rule_id).await?;
        }

        let target = BankMatchTarget::Invoice(invoice_id.clone());
        let id = BankMatchId::generate();
        // The stored time is answered rather than guessed at: one clock, the
        // database's, for the row and for what the caller is told about it.
        let confirmed_at: OffsetDateTime = sqlx::query_scalar(
            "INSERT INTO bank_matches (tenant_id, id, line_id, target_kind, target_id, \
                 amount_cents, payment_id, entry_id, rule_id, confirmed_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) \
             RETURNING confirmed_at",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(line.id.as_str())
        .bind(target.kind())
        .bind(target.id())
        .bind(amount_cents)
        .bind(payment_id.as_str())
        .bind(entry_id.as_str())
        .bind(rule_id.map(FinMatchRuleId::as_str))
        .bind(self.user.as_str())
        .fetch_one(&mut *tx)
        .await
        .map_err(StoreError::Db)?;

        // The line's status is a projection of this table, moved under the same
        // lock the check above took.
        let moved = sqlx::query(
            "UPDATE bank_lines SET status = 'matched' \
             WHERE tenant_id = $1 AND id = $2 AND status = 'unmatched'",
        )
        .bind(self.tenant.as_str())
        .bind(line.id.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        if moved.rows_affected() != 1 {
            return Err(StoreError::Conflict(
                "this bank line stopped being unmatched while it was being confirmed".to_owned(),
            ));
        }

        tx.commit().await.map_err(StoreError::Db)?;
        Ok(ConfirmedMatch {
            matched: BankMatch {
                id,
                line_id: line.id.clone(),
                target,
                amount_cents,
                payment_id: Some(payment_id),
                entry_id: Some(entry_id),
                rule_id: rule_id.map(|rule| rule.as_str().to_owned()),
                confirmed_by: self.user.clone(),
                confirmed_at,
            },
            invoice_entry_id,
            invoice_booked_now,
        })
    }

    /// The confirmed match on one of this tenant's lines, or `None` when there
    /// is none — including when the line is absent or another tenant's.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure; [`StoreError::Validation`] when the row
    /// names a target kind this build does not know.
    pub async fn bank_match(&self, line_id: &BankLineId) -> Result<Option<BankMatch>> {
        self.bank_match_on(&self.pool, line_id).await
    }

    /// [`AccountStore::bank_match`] against any executor.
    ///
    /// Taking a match back ([`crate::bank_unmatch`]) has to read it **again**
    /// under the line's row lock, because what it is about to delete a payment
    /// for must be the row that is there now and not the one a screen was drawn
    /// from.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure; [`StoreError::Validation`] when the row
    /// names a target kind this build does not know.
    pub(crate) async fn bank_match_on<'e, E>(
        &self,
        executor: E,
        line_id: &BankLineId,
    ) -> Result<Option<BankMatch>>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>,
    {
        let row = sqlx::query_as::<_, MatchRow>(&format!(
            "SELECT {MATCH_COLS} FROM bank_matches WHERE tenant_id = $1 AND line_id = $2"
        ))
        .bind(self.tenant.as_str())
        .bind(line_id.as_str())
        .fetch_optional(executor)
        .await
        .map_err(StoreError::Db)?;
        row.map(MatchRow::into_match).transpose()
    }
}

/// The candidate a document makes: what the exact rule needs, and nothing else.
///
/// Shared with the suggestion read ([`crate::bank_suggest`]) so that the rule
/// re-run under the row locks is fed by exactly the same projection the screen
/// was.
pub(crate) fn match_candidate(
    invoice: &Invoice,
    gross_cents: i64,
    paid_cents: i64,
) -> MatchCandidate {
    MatchCandidate {
        invoice_id: invoice.id.clone(),
        number: invoice.number.clone().unwrap_or_default(),
        currency: invoice.currency.clone(),
        outstanding_cents: Settlement::of(gross_cents, paid_cents).outstanding_cents,
        status: invoice.status,
        is_credit_note: invoice.is_credit_note,
        issue_date: invoice.issue_date,
    }
}

/// The reference the recorded payment carries: the bank's own, or — when the
/// bank stated none, which MT940 and many CSV exports do — what the payer
/// wrote, clipped to what the column holds.
///
/// Never empty when the bank told us anything at all: the reference is how a
/// person finds this movement again on the statement they are holding.
fn payment_reference(line: &BankLine) -> String {
    let raw = if line.bank_ref.trim().is_empty() {
        line.remittance.trim()
    } else {
        line.bank_ref.trim()
    };
    match raw.char_indices().nth(PAYMENT_REFERENCE_MAX_CHARS) {
        None => raw.to_owned(),
        Some((end, _)) => raw[..end].trim_end().to_owned(),
    }
}

/// One row of `bank_matches`, in [`MATCH_COLS`] order.
#[derive(sqlx::FromRow)]
struct MatchRow {
    id: String,
    line_id: String,
    target_kind: String,
    target_id: String,
    amount_cents: i64,
    payment_id: Option<String>,
    entry_id: Option<String>,
    rule_id: Option<String>,
    confirmed_by: String,
    confirmed_at: OffsetDateTime,
}

impl MatchRow {
    /// The stored match.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the row names a target kind the code
    /// does not know — a decode failure rather than a guess, because the guess
    /// would be about which document somebody's money settled.
    fn into_match(self) -> Result<BankMatch> {
        let target =
            BankMatchTarget::parse(&self.target_kind, &self.target_id).ok_or_else(|| {
                StoreError::Validation(
                    "this match names a kind of document this version does not know".to_owned(),
                )
            })?;
        Ok(BankMatch {
            id: BankMatchId::new(self.id),
            line_id: BankLineId::new(self.line_id),
            target,
            amount_cents: self.amount_cents,
            payment_id: self.payment_id.map(BillingPaymentId::new),
            entry_id: self.entry_id.map(FinEntryId::new),
            rule_id: self.rule_id,
            confirmed_by: UserId::new(self.confirmed_by),
            confirmed_at: self.confirmed_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::BankStatementId;
    use time::{Date, Month};

    fn line(bank_ref: &str, remittance: &str) -> BankLine {
        BankLine {
            id: BankLineId::new("line-1".to_owned()),
            statement_id: BankStatementId::new("stmt-1".to_owned()),
            line_no: 1,
            booked_on: Date::from_calendar_date(2026, Month::January, 14).unwrap_or(Date::MIN),
            value_on: Date::from_calendar_date(2026, Month::January, 14).unwrap_or(Date::MIN),
            amount_cents: 130_700,
            currency: "EUR".to_owned(),
            counterparty_name: "Kaffeehaus GmbH".to_owned(),
            counterparty_iban: String::new(),
            remittance: remittance.to_owned(),
            bank_ref: bank_ref.to_owned(),
            status: BankLineStatus::Unmatched,
            ignored_reason: String::new(),
            created_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn the_payment_quotes_the_banks_reference_and_falls_back_to_the_payers_words() {
        assert_eq!(payment_reference(&line("REF9", "INV-2026-00007")), "REF9");
        assert_eq!(
            payment_reference(&line("  ", "INV-2026-00007 vielen Dank")),
            "INV-2026-00007 vielen Dank"
        );
        assert_eq!(payment_reference(&line("", "")), "");
        // A remittance is seven times longer than the column that holds a
        // reference; it is clipped, never refused, because the movement it
        // names is real either way.
        let long = payment_reference(&line("", &"x".repeat(400)));
        assert_eq!(long.chars().count(), PAYMENT_REFERENCE_MAX_CHARS);
    }

    #[test]
    fn a_target_round_trips_through_its_stored_words() {
        let target = BankMatchTarget::Invoice(BillingInvoiceId::new("inv-1".to_owned()));
        assert_eq!(target.kind(), "invoice");
        assert_eq!(target.id(), "inv-1");
        assert_eq!(BankMatchTarget::parse("invoice", "inv-1"), Some(target));
        // A kind from a newer build is not guessed at: money settled something,
        // and which document that was is not a thing to assume.
        assert_eq!(BankMatchTarget::parse("bill", "bill-1"), None);
        assert_eq!(BankMatchTarget::parse("", ""), None);
    }

    #[test]
    fn the_method_a_match_records_lands_the_money_in_the_bank() {
        assert_eq!(
            payment_settlement_role(BANK_MATCH_METHOD),
            AccountRole::Bank,
            "money that arrived on a bank statement arrived in the bank"
        );
    }
}
