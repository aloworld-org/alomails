//! What the reconciliation screen is offered (alo Finance, ADR 0035, waves
//! B4.09a–b; `docs/design/finance.md`, "The bank and reconciliation").
//!
//! The two matching stages are pure and know no database: [`crate::bank_match`]
//! decides whether a line and a document are *exactly* each other, and
//! [`crate::bank_match_heuristic`] ranks the ones that are merely likely. This
//! file is the read that feeds them — it gathers the tenant's unmatched lines,
//! the documents those lines could be about and the rules the tenant has taught
//! ([`crate::fin_match_rules`]), applies both stages, and answers with one list
//! per line.
//!
//! It **writes nothing and posts nothing**. Confirming is
//! [`crate::bank_reconcile`]'s verb and it happens because a person said so
//! (ADR 0023) — a wrong automatic match marks an invoice paid that is not, and
//! the customer stops being chased.
//!
//! # Two candidate sets, for two different reasons
//!
//! The **exact** stage needs the documents whose numbers the statement quotes,
//! however old or however many they are: a payer who quotes our number is the
//! most certain thing on the file and must never be missed because the tenant
//! has a long ledger. So those are fetched **by number**, bounded only by
//! [`SUGGESTION_NUMBERS_MAX`].
//!
//! The **heuristic** stage compares against everything still owed, which is a
//! set no number list can narrow. It is therefore bounded by
//! [`OPEN_LEDGER_MAX`], and when that bound bites the read says so
//! ([`BankSuggestions::ledger_capped`]) *and* the stage stops claiming that an
//! amount fits only one document — a uniqueness argument over a ledger we did
//! not finish reading would be the most confident wrong suggestion on the
//! screen.
//!
//! # The cost of a read
//!
//! Five statements, whatever the size of the statement file: the lines, the
//! quoted documents (with their lines and payments), the open ledger (likewise),
//! the customers those documents belong to, and the rules. Everything after that
//! is arithmetic in memory.

use crate::account::AccountStore;
use crate::bank_import::{BankLine, BankLineStatus};
use crate::bank_match::{ExactMatch, MatchCandidate, document_numbers, exact_match};
use crate::bank_match_heuristic::{Candidate, LikelyMatch, likely_matches};
use crate::bank_reconcile::match_candidate;
use crate::billing_invoices::{InvoiceStatus, InvoiceSummary};
use crate::billing_sequence::INVOICE_NUMBER_PREFIX;
use crate::error::Result;
use crate::fin_match_rules::MatchRule;
use crate::id::BankStatementId;

/// The most distinct document numbers one suggestion read will look up.
///
/// A statement can stage five thousand lines and each may quote several
/// numbers; the cap keeps that from becoming one enormous query. It is never
/// silent: [`BankSuggestions::numbers_capped`] says when it bit, so a screen can
/// tell a bookkeeper to work a statement at a time rather than quietly showing
/// them a shorter list of suggestions than exists.
pub const SUGGESTION_NUMBERS_MAX: usize = 1_000;

/// The most open documents the heuristic stage will weigh a line against.
///
/// Every unmatched line is compared with every one of them, so this bounds the
/// arithmetic as much as the query. Five thousand documents still owed is a
/// large European SME's whole year; past it the read says
/// [`BankSuggestions::ledger_capped`] and the stage drops the one claim that
/// depends on having read everything.
pub const OPEN_LEDGER_MAX: usize = 5_000;

/// One staged line and the documents the two stages think it settles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineSuggestions {
    /// The line, as staged.
    pub line: BankLine,
    /// Every exact match found for it, in the order the documents were read.
    /// Usually one; empty for most lines; more than one when a payer quoted
    /// two documents that owe the same amount, which is a question for a person
    /// and not something to resolve by picking the first.
    pub exact: Vec<ExactMatch>,
    /// The documents it is merely *likely* to be, best first, each with the
    /// evidence that put it there. Empty whenever the exact stage is certain, by
    /// construction: a document the exact stage claims is not offered again as a
    /// guess.
    pub likely: Vec<LikelyMatch>,
}

/// The suggestions for a set of staged lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BankSuggestions {
    /// The unmatched lines, oldest first, each with its matches.
    pub lines: Vec<LineSuggestions>,
    /// Whether more distinct document numbers were quoted than
    /// [`SUGGESTION_NUMBERS_MAX`] allows to be looked up — in which case some
    /// lines late in the list may show no suggestion that a narrower read would
    /// have found. Never silent, so a screen can say so.
    pub numbers_capped: bool,
    /// Whether the tenant has more open documents than [`OPEN_LEDGER_MAX`], so
    /// the heuristic stage weighed the newest of them and made no claim about
    /// an amount fitting only one document.
    pub ledger_capped: bool,
}

impl AccountStore {
    /// The suggestions for this tenant's unmatched lines, optionally narrowed to
    /// one import: the exact matches first, then the ranked likely ones.
    ///
    /// Nothing here writes, nothing here posts, and a suggestion is worth
    /// exactly as much as the person who looks at it (ADR 0023).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure; [`StoreError::Validation`] when a stored
    /// rule names a field of a bank line this build does not know.
    pub async fn bank_match_suggestions(
        &self,
        statement: Option<&BankStatementId>,
    ) -> Result<BankSuggestions> {
        let lines = self
            .bank_lines(statement, Some(BankLineStatus::Unmatched))
            .await?;
        if lines.is_empty() {
            // Nothing to suggest for, and therefore no reason to read a ledger.
            return Ok(BankSuggestions {
                lines: Vec::new(),
                numbers_capped: false,
                ledger_capped: false,
            });
        }

        let (numbers, numbers_capped) = quoted_numbers(&lines);
        let quoted: Vec<MatchCandidate> = self
            .billing_invoices_by_numbers(&numbers)
            .await?
            .iter()
            .map(summary_candidate)
            .collect();

        let mut open = self.billing_invoices(Some(InvoiceStatus::Issued)).await?;
        let ledger_capped = open.len() > OPEN_LEDGER_MAX;
        open.truncate(OPEN_LEDGER_MAX);
        let names = self
            .billing_customer_names(
                &open
                    .iter()
                    .map(|s| s.invoice.customer_id.clone())
                    .collect::<Vec<_>>(),
            )
            .await?;
        let candidates: Vec<Candidate> = open
            .iter()
            .map(|summary| Candidate {
                invoice: summary_candidate(summary),
                customer_id: summary.invoice.customer_id.clone(),
                customer_name: names
                    .get(summary.invoice.customer_id.as_str())
                    .cloned()
                    .unwrap_or_default(),
                due_date: summary.invoice.due_date,
            })
            .collect();
        let rules: Vec<MatchRule> = self.fin_match_rules().await?;

        let lines = lines
            .into_iter()
            .map(|line| {
                let exact = quoted
                    .iter()
                    .filter_map(|candidate| exact_match(&line, candidate))
                    .collect();
                let likely = likely_matches(&line, &candidates, &rules, !ledger_capped);
                LineSuggestions {
                    line,
                    exact,
                    likely,
                }
            })
            .collect();
        Ok(BankSuggestions {
            lines,
            numbers_capped,
            ledger_capped,
        })
    }
}

/// Every distinct document number the staged lines quote, in the order they
/// were read, and whether the cap bit.
fn quoted_numbers(lines: &[BankLine]) -> (Vec<String>, bool) {
    let mut numbers: Vec<String> = Vec::new();
    let mut capped = false;
    for line in lines {
        for number in document_numbers(&line.remittance, INVOICE_NUMBER_PREFIX) {
            if numbers.contains(&number) {
                continue;
            }
            if numbers.len() >= SUGGESTION_NUMBERS_MAX {
                capped = true;
                break;
            }
            numbers.push(number);
        }
    }
    (numbers, capped)
}

/// The candidate a listed document makes, with its settlement resolved from the
/// totals and the payments the list already read.
fn summary_candidate(summary: &InvoiceSummary) -> MatchCandidate {
    match_candidate(
        &summary.invoice,
        summary.totals.gross_cents,
        summary.paid_cents,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bank_import::BankLineStatus;
    use crate::id::{BankLineId, BankStatementId};
    use time::{Date, Month, OffsetDateTime};

    fn line(remittance: &str) -> BankLine {
        BankLine {
            id: BankLineId::new("line-1".to_owned()),
            statement_id: BankStatementId::new("stmt-1".to_owned()),
            line_no: 1,
            booked_on: Date::from_calendar_date(2026, Month::February, 10).unwrap_or(Date::MIN),
            value_on: Date::from_calendar_date(2026, Month::February, 10).unwrap_or(Date::MIN),
            amount_cents: 130_700,
            currency: "EUR".to_owned(),
            counterparty_name: "Kaffeehaus Bergmann GmbH".to_owned(),
            counterparty_iban: String::new(),
            remittance: remittance.to_owned(),
            bank_ref: "REF9".to_owned(),
            status: BankLineStatus::Unmatched,
            ignored_reason: String::new(),
            created_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn the_numbers_looked_up_are_the_distinct_ones_the_statement_quotes() {
        let (numbers, capped) = quoted_numbers(&[
            line("INV-2026-00007 and INV-2026-00009"),
            line("INV-2026-00007 again"),
            line("no reference"),
        ]);
        assert_eq!(
            numbers,
            vec!["INV-2026-00007".to_owned(), "INV-2026-00009".to_owned()]
        );
        assert!(!capped);
    }

    #[test]
    fn the_number_cap_is_reported_rather_than_hidden() {
        let lines: Vec<BankLine> = (1..=SUGGESTION_NUMBERS_MAX + 5)
            .map(|n| line(&format!("INV-2026-{n:05}")))
            .collect();
        let (numbers, capped) = quoted_numbers(&lines);
        assert_eq!(numbers.len(), SUGGESTION_NUMBERS_MAX);
        assert!(capped, "a screen has to be able to say the list is short");
    }
}
