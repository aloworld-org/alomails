//! The exact stage of reconciliation (alo Finance, ADR 0035, wave B4.09a;
//! `docs/design/finance.md`, "Matching is three stages, and only the first is
//! arithmetic").
//!
//! # What "exact" means, and why it is still only a suggestion
//!
//! A staged bank line ([`crate::bank_import`]) is what the bank says happened.
//! This module answers one question about it, arithmetically: **does this line
//! quote one of our own document numbers, and does it move exactly what that
//! document is still owed?** Four facts have to line up — the number in the
//! remittance, the direction of the money, the currency, and the amount — and
//! the date window is there to stop a number reused years later from being
//! read as its own payment.
//!
//! When they all line up the answer is *still* a suggestion. Nothing here
//! confirms anything and nothing here touches a database: confirming is
//! [`crate::bank_reconcile`]'s verb and it happens because a person said so
//! (ADR 0023). Here that rule is also a money rule — a wrong automatic match
//! marks an invoice paid that is not, and the customer stops being chased.
//!
//! # The module is pure
//!
//! Everything below is a function of its arguments: no clock, no tenant, no
//! query. That is what lets the precision tests state the rules as arithmetic
//! ("one cent short is not exact", "a number quoted inside a longer word is not
//! a number") instead of as fixtures, and it is what lets the confirming path
//! re-run **the same rule** on the server rather than trusting the suggestion a
//! client sends back.
//!
//! # What the exact stage deliberately does not do
//!
//! - **Partial payments.** A customer quoting our number and paying half of it
//!   is a real and common event, and it is not *exact*: the amount does not
//!   equal what is owed. It belongs to the heuristic stage (B4.09b), which
//!   ranks with evidence, and to the manual one (B4.09c), where a person states
//!   the amount.
//! - **Credit notes.** They share the invoice series, so a credit note's number
//!   can appear in a remittance, but money moving against one is a *refund* —
//!   an event in the other direction that [`crate::billing_payments`] refuses
//!   by design.
//! - **Bills and expenses.** A supplier's own number is free text (B1.24) and
//!   an expense has no number at all, so neither can be matched by the rule
//!   this module is: "our number, printed by us, unambiguous since B1.08".

use time::Date;

use crate::bank_import::{BankLine, BankLineStatus};
use crate::billing_invoices::InvoiceStatus;
use crate::billing_sequence::INVOICE_NUMBER_PREFIX;
use crate::error::{Result, StoreError};
use crate::id::BillingInvoiceId;

/// How long after a document's issue date the exact stage will still read a
/// quoted number as that document's payment — two years.
///
/// The number already carries its year ([`crate::billing_sequence`]), so this
/// is belt and braces rather than the main defence. It is generous on purpose:
/// an invoice paid four hundred days late is a real event a bookkeeper still
/// has to reconcile, and refusing to *suggest* it would send the most obviously
/// correct match in the file to the manual pile.
pub const EXACT_WINDOW_DAYS: i64 = 730;

/// The most document numbers one remittance is read for.
///
/// A payer settling several invoices in one transfer lists them all, and the
/// list is what the heuristic stage will one day split a line by. The cap is a
/// bound on work, not a rule about payers: a remittance is at most
/// [`crate::bank_import::REMITTANCE_MAX`] characters, and a thousand characters
/// of digits must not become a thousand database lookups.
pub const NUMBERS_PER_REMITTANCE_MAX: usize = 8;

/// The most digits a counter may have for a run of digits to be read as one.
///
/// Numbers grow (`INV-2026-100000` is legitimate), so this is not the printed
/// width; it is the point past which a digit run is an account number, an
/// order id or a date, and not a counter of ours.
const COUNTER_DIGITS_MAX: usize = 9;

/// The width a counter is printed to, and therefore the width a shorter one is
/// read as. A payer who types `INV-2026-7` means `INV-2026-00007`, because that
/// is what we printed on the document they are holding.
const COUNTER_PRINTED_DIGITS: usize = 5;

/// A document the exact stage may match a line against — everything the rule
/// needs about an invoice and nothing else.
///
/// Built from an [`crate::billing_invoices::InvoiceSummary`] by the store side,
/// which is what keeps this module free of a database while still deciding
/// against real settled amounts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchCandidate {
    /// The document.
    pub invoice_id: BillingInvoiceId,
    /// Its number as we printed it, e.g. `INV-2026-00007`.
    pub number: String,
    /// The currency it was raised in.
    pub currency: String,
    /// What is still owed on it: gross minus everything already received
    /// ([`crate::billing_payments::Settlement`]).
    pub outstanding_cents: i64,
    /// Where it is in its life.
    pub status: InvoiceStatus,
    /// Whether it credits another document rather than charging for anything.
    pub is_credit_note: bool,
    /// The day it was issued; `None` while it is a draft.
    pub issue_date: Option<Date>,
}

/// One exact match: a line, a document, and the evidence a person is shown
/// before they confirm it.
///
/// The evidence is not decoration. A bookkeeper confirming a match is taking
/// responsibility for money being marked as arrived, and "the payer wrote
/// INV-2026-00007 and sent exactly what it owes, nine days after it was issued"
/// is a sentence they can check against the screen in front of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExactMatch {
    /// The document matched.
    pub invoice_id: BillingInvoiceId,
    /// The number as we printed it — which is the number found in the
    /// remittance, canonicalised.
    pub number: String,
    /// What the line moves, in integer cents, in the line's own currency. Equal
    /// to the document's outstanding amount, which is what makes it exact.
    pub amount_cents: i64,
    /// How many days after the document's issue date the bank booked it.
    /// Negative is impossible here — money that arrived before the number
    /// existed cannot be quoting it.
    pub days_after_issue: i64,
}

/// The document numbers a remittance quotes, canonicalised and de-duplicated,
/// in the order they appear.
///
/// Payers and their banks mangle the separators (`INV-2026-00007`,
/// `inv 2026 00007`, `INV/2026/00007`, `INV20260007`), reformat the case, and
/// split the string across fixed-width chunks — MT940's `?2n` subfields are
/// joined with nothing at all for precisely this reason
/// ([`crate::bank_mt940`]). So the shape is read rather than the spelling: the
/// prefix, a four-digit year, and a counter, with at most one separator between
/// the parts.
///
/// **The digits are what carry the safety, not the punctuation around them.**
/// `INV-2026-000078` is a *different* counter, never this one with a digit
/// stuck to it, so a run of digits is read whole or not at all — reading a
/// prefix of it would mark the wrong invoice paid. Letters on either side, by
/// contrast, are refused nothing: an MT940 remittance arrives glued to the word
/// before and after it, and a payer's own system reference that happens to
/// contain our number in full is still, overwhelmingly, our number. What keeps
/// that safe is not the boundary but the conjunction: a match also has to move
/// exactly what is owed, in the same currency, inside the window — and even
/// then a person confirms it.
#[must_use]
pub fn document_numbers(remittance: &str, prefix: &str) -> Vec<String> {
    let text: Vec<char> = remittance.chars().collect();
    let needle: Vec<char> = prefix.chars().flat_map(char::to_lowercase).collect();
    let mut found: Vec<String> = Vec::new();
    let mut at = 0usize;

    while at < text.len() && found.len() < NUMBERS_PER_REMITTANCE_MAX {
        let Some(start) = find_prefix(&text, at, &needle) else {
            break;
        };
        at = start + needle.len();
        if let Some((number, end)) = read_number(&text, at, prefix) {
            if !found.contains(&number) {
                found.push(number);
            }
            at = end;
        }
    }
    found
}

/// The exact match between a line and a candidate, or `None` when the rule does
/// not hold — the form a suggestion list is built with.
///
/// [`ensure_exact_match`] is the same rule with its reasons; this is it with
/// the reasons thrown away, because a list of suggestions has nothing to say
/// about the thousands of documents that did not match.
#[must_use]
pub fn exact_match(line: &BankLine, candidate: &MatchCandidate) -> Option<ExactMatch> {
    ensure_exact_match(line, candidate).ok()
}

/// The exact match between a line and a candidate, or the reason there is none.
///
/// **This is the rule the confirming path re-runs**, which is the whole reason
/// it returns words. A client confirms a match by naming a line and a document;
/// the suggestion it saw is not evidence, so the server derives the match again
/// from the line and the document as they are *now* — a document paid in the
/// meantime, or a line already matched, refuses here rather than booking money
/// twice.
///
/// # Errors
/// [`StoreError::Conflict`] when the line or the document is not in a state
/// that can be matched at all (already matched, a draft, a void document, a
/// credit note, a settled one); [`StoreError::Validation`] when both are fine
/// but they are not each other's: the number is not quoted, the money goes the
/// wrong way, the currencies differ, the amount is not what is owed, or the
/// dates cannot be reconciled.
///
/// No message quotes an amount, a name or a remittance: a refusal is read on a
/// screen and written to a log, and a bank line is the tenant's own money
/// moving (Law 1).
pub fn ensure_exact_match(line: &BankLine, candidate: &MatchCandidate) -> Result<ExactMatch> {
    match line.status {
        BankLineStatus::Unmatched => {}
        BankLineStatus::Matched => {
            return Err(StoreError::Conflict(
                "this bank line is already matched; unmatch it before matching it to something \
                 else"
                    .to_owned(),
            ));
        }
        BankLineStatus::Ignored => {
            return Err(StoreError::Conflict(
                "this bank line was marked as not ours to book; take that back before matching it"
                    .to_owned(),
            ));
        }
    }
    if candidate.is_credit_note {
        return Err(StoreError::Conflict(
            "that document is a credit note — money moving against it is a refund, which is not \
             a payment"
                .to_owned(),
        ));
    }
    match candidate.status {
        InvoiceStatus::Issued => {}
        InvoiceStatus::Draft => {
            return Err(StoreError::Conflict(
                "that invoice is still a draft, so it is owed by nobody; issue it first".to_owned(),
            ));
        }
        InvoiceStatus::Void => {
            return Err(StoreError::Conflict(
                "that invoice was cancelled; money cannot be recorded against it".to_owned(),
            ));
        }
        InvoiceStatus::Paid => {
            return Err(StoreError::Conflict(
                "that invoice is already settled in full".to_owned(),
            ));
        }
    }
    let Some(issue_date) = candidate.issue_date else {
        return Err(StoreError::Conflict(
            "that invoice carries no issue date, so nothing can be matched to it".to_owned(),
        ));
    };
    if candidate.outstanding_cents <= 0 {
        return Err(StoreError::Conflict(
            "that invoice has nothing outstanding".to_owned(),
        ));
    }

    if line.amount_cents <= 0 {
        return Err(StoreError::Validation(
            "this bank line is money leaving the account, and an invoice is settled by money \
             arriving"
                .to_owned(),
        ));
    }
    let quoted = document_numbers(&line.remittance, INVOICE_NUMBER_PREFIX);
    if !quoted
        .iter()
        .any(|number| same_number(number, &candidate.number))
    {
        return Err(StoreError::Validation(
            "this bank line does not quote that invoice's number, so the exact stage cannot \
             confirm it; match it by hand instead"
                .to_owned(),
        ));
    }
    if line.currency != candidate.currency {
        return Err(StoreError::Validation(
            "this bank line is in a different currency from that invoice".to_owned(),
        ));
    }
    if line.amount_cents != candidate.outstanding_cents {
        return Err(StoreError::Validation(
            "this bank line does not move exactly what that invoice still owes; the exact stage \
             confirms whole settlements only"
                .to_owned(),
        ));
    }

    let days_after_issue = (line.booked_on - issue_date).whole_days();
    if days_after_issue < 0 {
        return Err(StoreError::Validation(
            "the bank booked this money before that invoice was issued, so it cannot be its \
             payment"
                .to_owned(),
        ));
    }
    if days_after_issue > EXACT_WINDOW_DAYS {
        return Err(StoreError::Validation(format!(
            "the bank booked this money more than {EXACT_WINDOW_DAYS} days after that invoice \
             was issued; match it by hand if it really is its payment"
        )));
    }

    Ok(ExactMatch {
        invoice_id: candidate.invoice_id.clone(),
        number: candidate.number.clone(),
        amount_cents: line.amount_cents,
        days_after_issue,
    })
}

/// Whether two printed numbers are the same document's, ignoring case and
/// surrounding blanks — the comparison
/// [`crate::billing_invoices::AccountStore::billing_invoice_id_by_number`]
/// makes in SQL, made here so the pure rule and the lookup can never disagree.
fn same_number(one: &str, other: &str) -> bool {
    one.trim().eq_ignore_ascii_case(other.trim())
}

/// The index of the next case-insensitive occurrence of `needle` in `text` at
/// or after `from`, or `None`.
fn find_prefix(text: &[char], from: usize, needle: &[char]) -> Option<usize> {
    if needle.is_empty() || needle.len() > text.len() {
        return None;
    }
    (from..=text.len() - needle.len()).find(|&start| {
        text[start..start + needle.len()]
            .iter()
            .zip(needle)
            .all(|(have, want)| have.to_lowercase().eq(std::iter::once(*want)))
    })
}

/// Reads `[sep] YYYY [sep] NNNNN` starting at `from`, returning the canonical
/// number and the index just past it.
///
/// The year and the counter are one digit run when the payer's bank dropped the
/// punctuation (`INV202600007`), and two when it did not. The two readings are
/// kept apart on purpose: a run of digits is split into `YYYY` + counter **only
/// when nothing separated it from the prefix either**, so `INV-20261-00007` —
/// which is punctuated, and whose first part is not a year — is refused rather
/// than read as `INV-2026-00001` with the rest thrown away.
fn read_number(text: &[char], from: usize, prefix: &str) -> Option<(String, usize)> {
    let at = skip_separator(text, from);
    let glued = at == from;
    let (run, after) = read_digits(text, at, 1, 4 + COUNTER_DIGITS_MAX)?;
    let (year, counter, at) = if run.len() == 4 {
        let at = skip_separator(text, after);
        let (counter, at) = read_digits(text, at, 1, COUNTER_DIGITS_MAX)?;
        (run, counter, at)
    } else if glued && run.len() > 4 {
        let (year, counter) = run.split_at(4);
        (year.to_owned(), counter.to_owned(), after)
    } else {
        return None;
    };
    let counter = if counter.len() < COUNTER_PRINTED_DIGITS {
        format!("{:0>COUNTER_PRINTED_DIGITS$}", counter)
    } else {
        counter
    };
    Some((format!("{prefix}-{year}-{counter}"), at))
}

/// The index past at most one separator — the punctuation or single blank a
/// payer's bank puts between the parts of a number, or nothing at all.
fn skip_separator(text: &[char], at: usize) -> usize {
    match text.get(at) {
        Some(&('-' | '/' | '.' | '_' | ' ' | ':')) => at + 1,
        _ => at,
    }
}

/// Reads a run of `min..=max` ASCII digits at `at`, returning it and the index
/// just past it.
///
/// The run is always taken **whole**: too short or too long reads as no run at
/// all, rather than as a prefix of one. That is what makes `INV-2026-000078` a
/// different counter instead of ours with a digit stuck to it, and it is the
/// one boundary rule the extractor needs.
fn read_digits(text: &[char], at: usize, min: usize, max: usize) -> Option<(String, usize)> {
    let digits: String = text[at.min(text.len())..]
        .iter()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if digits.len() < min || digits.len() > max {
        return None;
    }
    let end = at + digits.len();
    Some((digits, end))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;
    use crate::bank_import::BankLine;
    use crate::id::{BankLineId, BankStatementId};
    use time::{Month, OffsetDateTime};

    const PREFIX: &str = INVOICE_NUMBER_PREFIX;

    fn day(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).unwrap_or(Date::MIN)
    }

    fn line(amount_cents: i64, remittance: &str) -> BankLine {
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
            remittance: remittance.to_owned(),
            bank_ref: "REF9".to_owned(),
            status: BankLineStatus::Unmatched,
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

    fn refused<T: std::fmt::Debug>(result: Result<T>) -> String {
        match result {
            Err(StoreError::Validation(message) | StoreError::Conflict(message)) => message,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_number_is_read_however_the_payers_bank_spelled_it() {
        for spelling in [
            "INV-2026-00007",
            "inv-2026-00007",
            "Payment INV 2026 00007 thanks",
            "ZAHLUNGINV-2026-00007VIELENDANK",
            "INV/2026/00007",
            "INV.2026.00007",
            "INV_2026_00007",
            "INV202600007",
            "rechnung inv2026-00007",
        ] {
            let found = document_numbers(spelling, PREFIX);
            assert_eq!(found, vec!["INV-2026-00007".to_owned()], "for {spelling:?}");
        }
        // A payer who drops the printed padding means the document they are
        // holding, which we printed padded.
        assert_eq!(
            document_numbers("INV-2026-7", PREFIX),
            vec!["INV-2026-00007".to_owned()]
        );
        // And a counter that has outgrown the padding keeps its own width.
        assert_eq!(
            document_numbers("INV-2026-100000", PREFIX),
            vec!["INV-2026-100000".to_owned()]
        );
    }

    #[test]
    fn a_shape_that_is_not_a_number_yields_nothing() {
        for noise in [
            // Not a year.
            "INV-26-00007",
            "INV-202-00007",
            "INV-20261-00007",
            // Nothing to count.
            "INV-2026-",
            "INV-2026",
            // More digits than any counter of ours: an order id, an account,
            // a date — read whole and refused whole, never as a prefix.
            "INV-2026-1234567890",
            // A separator we do not read as one, twice over.
            "INV--2026--00007",
            "",
            "no reference at all",
        ] {
            assert_eq!(
                document_numbers(noise, PREFIX),
                Vec::<String>::new(),
                "for {noise:?}"
            );
        }
    }

    #[test]
    fn a_longer_counter_is_a_different_document_never_ours_with_a_digit_stuck_on() {
        // The dangerous false positive: reading `INV-2026-0000781` as
        // `INV-2026-00007` would settle a document the payer never named.
        assert_eq!(
            document_numbers("INV-2026-0000781", PREFIX),
            vec!["INV-2026-0000781".to_owned()]
        );
        assert!(
            ensure_exact_match(&line(130_700, "INV-2026-0000781"), &candidate(130_700)).is_err()
        );
    }

    #[test]
    fn letters_around_the_number_are_the_banks_formatting_not_a_boundary() {
        // MT940's `?2n` chunks are joined with nothing at all, so the number
        // arrives welded to the words on both sides of it.
        assert_eq!(
            document_numbers("ZAHLUNGINV-2026-00007VIELENDANK", PREFIX),
            vec!["INV-2026-00007".to_owned()]
        );
    }

    #[test]
    fn a_remittance_listing_several_numbers_yields_each_once_in_order() {
        let found = document_numbers(
            "settling INV-2026-00007, INV-2026-00009 and INV-2026-00007 again",
            PREFIX,
        );
        assert_eq!(
            found,
            vec!["INV-2026-00007".to_owned(), "INV-2026-00009".to_owned()]
        );
        // The cap is a bound on work, not a rule about payers.
        let many: String = (1..=20)
            .map(|n| format!("INV-2026-{n:05} "))
            .collect::<String>();
        assert_eq!(
            document_numbers(&many, PREFIX).len(),
            NUMBERS_PER_REMITTANCE_MAX
        );
    }

    #[test]
    fn the_whole_outstanding_amount_quoted_by_number_is_an_exact_match() {
        let matched = ensure_exact_match(&line(130_700, "INV-2026-00007"), &candidate(130_700))
            .expect("an exact match");
        assert_eq!(matched.number, "INV-2026-00007");
        assert_eq!(matched.amount_cents, 130_700);
        assert_eq!(matched.days_after_issue, 9);
    }

    #[test]
    fn one_cent_short_is_not_exact_and_neither_is_one_cent_over() {
        // There is no tolerance band. A cent is exactly the kind of difference
        // a bank charge leaves behind, and a bookkeeper has to see it — the
        // heuristic stage (B4.09b) is where a near miss gets ranked and shown.
        for amount in [130_699_i64, 130_701] {
            let message = refused(ensure_exact_match(
                &line(amount, "INV-2026-00007"),
                &candidate(130_700),
            ));
            assert!(message.contains("exactly what"), "{message}");
        }
    }

    #[test]
    fn a_partly_paid_document_is_matched_against_what_is_left_not_its_gross() {
        // €1 307.00 gross with €300.00 already received: the transfer that
        // settles it moves €1 007.00, and the gross is not the figure to match.
        let rest = candidate(100_700);
        assert!(ensure_exact_match(&line(100_700, "INV-2026-00007"), &rest).is_ok());
        assert!(ensure_exact_match(&line(130_700, "INV-2026-00007"), &rest).is_err());
    }

    #[test]
    fn money_leaving_the_account_never_settles_a_receivable() {
        let message = refused(ensure_exact_match(
            &line(-130_700, "INV-2026-00007"),
            &candidate(130_700),
        ));
        assert!(message.contains("money leaving"), "{message}");
    }

    #[test]
    fn a_line_that_does_not_quote_the_number_is_left_to_the_later_stages() {
        for remittance in ["", "thanks", "INV-2026-00009", "order 4711"] {
            let message = refused(ensure_exact_match(
                &line(130_700, remittance),
                &candidate(130_700),
            ));
            assert!(
                message.contains("does not quote"),
                "{remittance:?}: {message}"
            );
        }
    }

    #[test]
    fn the_currencies_have_to_be_the_same_currency() {
        let mut other = candidate(130_700);
        other.currency = "USD".to_owned();
        let message = refused(ensure_exact_match(&line(130_700, "INV-2026-00007"), &other));
        assert!(message.contains("different currency"), "{message}");
    }

    #[test]
    fn money_that_arrived_before_the_number_existed_is_not_its_payment() {
        let mut later = candidate(130_700);
        later.issue_date = Some(day(2026, Month::January, 20));
        let message = refused(ensure_exact_match(&line(130_700, "INV-2026-00007"), &later));
        assert!(
            message.contains("before that invoice was issued"),
            "{message}"
        );
    }

    #[test]
    fn the_window_ends_two_years_after_the_issue_date_and_not_a_day_earlier() {
        let issued = day(2026, Month::January, 5);
        let mut on_the_last_day = line(130_700, "INV-2026-00007");
        on_the_last_day.booked_on = issued + time::Duration::days(EXACT_WINDOW_DAYS);
        assert_eq!(
            ensure_exact_match(&on_the_last_day, &candidate(130_700))
                .expect("the last day is inside the window")
                .days_after_issue,
            EXACT_WINDOW_DAYS
        );

        let mut a_day_late = on_the_last_day.clone();
        a_day_late.booked_on = issued + time::Duration::days(EXACT_WINDOW_DAYS + 1);
        assert!(
            refused(ensure_exact_match(&a_day_late, &candidate(130_700))).contains("more than"),
        );
    }

    #[test]
    fn a_document_that_cannot_take_money_refuses_before_the_arithmetic() {
        let quoted = line(130_700, "INV-2026-00007");
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
                Box::new(|c: &mut MatchCandidate| c.outstanding_cents = 0),
                "nothing outstanding",
            ),
        ] {
            let mut broken = candidate(130_700);
            mutate(&mut broken);
            let message = refused(ensure_exact_match(&quoted, &broken));
            assert!(
                message.contains(expect),
                "expected {expect:?}, got {message}"
            );
        }
    }

    #[test]
    fn a_line_that_is_no_longer_unmatched_refuses_whatever_the_arithmetic_says() {
        for (status, expect) in [
            (BankLineStatus::Matched, "already matched"),
            (BankLineStatus::Ignored, "not ours to book"),
        ] {
            let mut settled = line(130_700, "INV-2026-00007");
            settled.status = status;
            let message = refused(ensure_exact_match(&settled, &candidate(130_700)));
            assert!(message.contains(expect), "{message}");
        }
    }

    #[test]
    fn no_refusal_quotes_the_tenants_own_money_or_words() {
        // Law 1: a refusal is read on a screen and written to a log, and a bank
        // line is the tenant's money moving. The rule is named; the values are
        // not.
        let secret = "INV-2026-00009 from Kaffeehaus GmbH";
        let mut broken = candidate(130_700);
        broken.currency = "USD".to_owned();
        for message in [
            refused(ensure_exact_match(
                &line(999_999, secret),
                &candidate(130_700),
            )),
            refused(ensure_exact_match(&line(130_700, secret), &broken)),
            refused(ensure_exact_match(
                &line(-130_700, secret),
                &candidate(130_700),
            )),
        ] {
            assert!(!message.contains("999999"), "{message}");
            assert!(!message.contains("130700"), "{message}");
            assert!(!message.contains("Kaffeehaus"), "{message}");
            assert!(!message.contains("INV-2026-00009"), "{message}");
        }
    }

    #[test]
    fn the_suggestion_form_is_the_same_rule_with_its_reasons_thrown_away() {
        assert!(exact_match(&line(130_700, "INV-2026-00007"), &candidate(130_700)).is_some());
        assert!(exact_match(&line(130_700, "no number"), &candidate(130_700)).is_none());
    }
}
