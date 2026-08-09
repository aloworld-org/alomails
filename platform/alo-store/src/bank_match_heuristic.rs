//! The heuristic stage of reconciliation (alo Finance, ADR 0035, wave B4.09b;
//! `docs/design/finance.md`, "Matching is three stages, and only the first is
//! arithmetic").
//!
//! # What this stage is for
//!
//! The exact stage ([`crate::bank_match`]) answers one question and answers it
//! by construction: *did the payer quote our own number and send exactly what
//! the document is still owed?* Most of a European bank statement does not look
//! like that. A customer pays two invoices with one transfer, or pays half of
//! one, or quotes nothing at all and is recognisable only because their name is
//! on the line and they owe precisely that amount.
//!
//! This module ranks those. Given a staged line, the tenant's open documents and
//! the rules the tenant has taught ([`crate::fin_match_rules`]), it answers with
//! an ordered list of [`LikelyMatch`]es — **each carrying the evidence that
//! produced it**, so the sentence on the screen is "the amount is exactly what
//! it owes, and it is the only invoice that owes it" rather than "87 % sure".
//!
//! # Three rules keep it honest
//!
//! **Nothing here is ever confirmed automatically** (ADR 0023, and here a money
//! rule): a wrong automatic match marks an invoice paid that is not, and the
//! customer stops being chased. This module writes nothing, reads nothing, and
//! knows no tenant — it is a function of its arguments, which is what lets the
//! precision tests below state each rule as arithmetic.
//!
//! **A resemblance alone is never a suggestion.** A score is a sum of points,
//! and [`SCORE_MIN`] is set so that no single soft signal reaches it: a name
//! that looks similar, or an amount that happens to fit, is not enough on its
//! own. Something has to *identify* the document — the payer quoted its number,
//! a rule the tenant saved points at its customer, the counterparty is that
//! customer, or the amount fits exactly one open document and no other.
//!
//! **The exact stage's preconditions still hold.** [`ensure_matchable`] is
//! shared with it, so a credit note, a draft, a settled document, money leaving
//! the account, a foreign currency and a date before the invoice existed are all
//! refused here for exactly the same reasons and in exactly the same words.
//!
//! # What it deliberately does not do
//!
//! - **Overpayment.** A line moving more than a document is owed is never
//!   suggested for it: attributing it would record a payment larger than the
//!   debt. It is a split, a duplicate or a mistake, and all three are a person's
//!   question (B4.09c).
//! - **Splitting one line across several documents.** The ranked list may well
//!   name three invoices whose sum is the transfer; picking that combination is
//!   the manual stage's verb, and `bank_matches.amount_cents` is already where
//!   the parts would live.
//! - **Learning by itself.** A rule exists because a person saved it
//!   ([`crate::fin_match_rules`]), and a hit count never changes what a rule
//!   scores. A heuristic that quietly re-weights itself is one nobody can
//!   predict, and this one has to be explainable to a bookkeeper.

use time::Date;

use crate::bank_import::BankLine;
use crate::bank_match::{MatchCandidate, ensure_matchable, exact_match};
use crate::fin_match_rules::{MatchOn, MatchRule};
use crate::id::{BillingCustomerId, BillingInvoiceId, FinMatchRuleId};

/// The most documents one line is offered for. Past a handful, a ranked list is
/// no longer a suggestion — it is the whole ledger with an ordering, and the
/// manual stage (B4.09c) is the honest way to search that.
pub const LIKELY_MATCHES_MAX: usize = 5;

/// A payment this many days either side of the due date is "when it was due" —
/// the fortnight around a due date in which most invoices are actually paid.
pub const NEAR_DUE_DAYS: i64 = 7;

/// Still recognisably about that due date: a month either side.
pub const AROUND_DUE_DAYS: i64 = 30;

/// How alike two names have to be, in basis points of
/// [`name_similarity_bp`], before the resemblance is worth showing at all.
///
/// 60 % of the words shared: "Kaffeehaus Bergmann" and "Kaffeehaus Bergmann
/// GmbH" are the same company, "Kaffeehaus Bergmann" and "Bäckerei Bergmann" are
/// not. Below the bar the resemblance is not recorded as evidence and scores
/// nothing — it is not treated as weak evidence, because a weak name signal
/// added to an amount that fits is exactly how a wrong invoice gets marked paid.
pub const NAME_SIMILAR_MIN_BP: i64 = 6_000;

/// The score a document needs before it is offered at all.
///
/// It is set to the points of the weakest *identifying* combination there is —
/// the amount fits exactly one open document and no other
/// ([`MatchEvidence::WholeAmount`] + [`MatchEvidence::OnlyDocumentForTheAmount`])
/// — so that no soft signal on its own can reach it. A name that merely looks
/// similar scores 20; an amount that merely fits scores 30; neither is a
/// suggestion, and together they are only one if the name is the customer's
/// exactly.
pub const SCORE_MIN: i32 = 45;

/// An open document the heuristic stage may offer, and the two facts about it
/// the exact stage has no use for: whose it is, and when it was due.
///
/// A wrapper rather than more fields on [`MatchCandidate`], because the exact
/// rule must not be able to read a customer's name: it decides by our own
/// number and the arithmetic, and a field it cannot see is a field it cannot
/// come to depend on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// What the shared preconditions and the exact rule read.
    pub invoice: MatchCandidate,
    /// The customer the document was raised for.
    pub customer_id: BillingCustomerId,
    /// Their name, as the tenant wrote it in the customer record — compared
    /// against the name the *bank* put on the line, which is rarely spelled the
    /// same way.
    pub customer_name: String,
    /// The day the document was due; `None` when it carries no due date.
    pub due_date: Option<Date>,
}

/// One reason a document was offered for a line. The list of these is the whole
/// point of the stage: a bookkeeper confirms because of a sentence they can
/// check, not because of a number a machine produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchEvidence {
    /// The remittance quotes this document's number — but the amount is not the
    /// whole of what it owes, which is why the exact stage passed it here.
    NumberQuoted,
    /// A rule the tenant saved points at this document's customer.
    RuleSaved {
        /// Which rule, so a screen can show it and a person can delete it.
        rule_id: FinMatchRuleId,
        /// What that rule looks at.
        match_on: MatchOn,
    },
    /// The name the bank put on the line is the document's customer.
    CustomerNamed {
        /// How alike the two names are, in basis points — 10 000 is word for
        /// word after folding.
        similarity_bp: i64,
    },
    /// The line moves exactly what the document still owes.
    WholeAmount,
    /// …and it is the only open document of this tenant that owes exactly that,
    /// which is what makes an amount on its own worth showing.
    OnlyDocumentForTheAmount,
    /// The bank booked the money around the day the document was due. Negative
    /// days are early, positive are late.
    NearDue {
        /// Days between the due date and the booking.
        days: i64,
    },
    /// The line moves less than the document owes: quoted by number, it is a
    /// part payment, and this is what would be left after it.
    PartPayment {
        /// What the document would still owe afterwards, in integer cents.
        remaining_cents: i64,
    },
}

impl MatchEvidence {
    /// What this evidence is worth towards [`SCORE_MIN`].
    ///
    /// The numbers are deliberately coarse and deliberately here, in one
    /// readable list, rather than spread across the ranking function: the whole
    /// scale is four identifying signals and two supporting ones, and a person
    /// arguing that a name should be worth less than a saved rule should be able
    /// to see both figures at once.
    #[must_use]
    pub fn points(&self) -> i32 {
        match self {
            // Our own number, printed by us. The strongest thing a payer can
            // write; it is only here rather than in the exact stage because the
            // amount did not settle the document.
            Self::NumberQuoted => 60,
            // Somebody who knows this tenant's payers said so, once, deliberately.
            Self::RuleSaved { .. } => 45,
            // The bank's name for the payer is the customer's — worth more when
            // it is word for word than when it merely resembles it.
            Self::CustomerNamed { similarity_bp } => {
                if *similarity_bp >= 10_000 {
                    35
                } else {
                    20
                }
            }
            // Arithmetic that fits, which is not by itself an identification.
            Self::WholeAmount => 30,
            Self::OnlyDocumentForTheAmount => 15,
            // Timing: supporting evidence only. Paying near the due date is what
            // everybody does, so it can never carry a suggestion on its own.
            Self::NearDue { days } => {
                if days.abs() <= NEAR_DUE_DAYS {
                    10
                } else {
                    5
                }
            }
            // A statement about the document, not evidence about the line.
            Self::PartPayment { .. } => 0,
        }
    }
}

/// One document offered for one line, with the evidence and the score that put
/// it where it is in the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LikelyMatch {
    /// The document.
    pub invoice_id: BillingInvoiceId,
    /// Its number as we printed it.
    pub number: String,
    /// What of the line would be attributed to it: the whole line, since this
    /// stage never splits one.
    pub amount_cents: i64,
    /// What the document still owes — equal to the amount when the line settles
    /// it whole, more when this would be a part payment.
    pub outstanding_cents: i64,
    /// Whose document it is.
    pub customer_id: BillingCustomerId,
    /// How many days after the document's issue date the bank booked the money.
    pub days_after_issue: i64,
    /// The sum of the evidence's points, and the order of the list.
    pub score: i32,
    /// Why, in the order the reasons were established.
    pub evidence: Vec<MatchEvidence>,
    /// The rule that fired, when one did — what a confirmation records in
    /// `bank_matches.rule_id` and what a hit is counted against.
    pub rule_id: Option<FinMatchRuleId>,
}

/// The documents this line is likely to be, best first.
///
/// Every candidate is put through the shared preconditions
/// ([`ensure_matchable`]) and then through the evidence below; those that reach
/// [`SCORE_MIN`] are sorted and the best [`LIKELY_MATCHES_MAX`] are answered.
///
/// **Documents the exact stage already claims are left out.** A line that quotes
/// a number and settles that document whole is an exact match, and offering it
/// twice — once as certainty, once as a guess — would make the screen argue with
/// itself.
///
/// Ties are broken by the **oldest debt first** (the largest number of days
/// since issue) and then by document number, so the same statement read twice
/// produces the same list in the same order.
///
/// `ledger_complete` says whether `candidates` really is everything this tenant
/// still has open. When it is not — the caller had to cap the read
/// ([`crate::bank_suggest::OPEN_LEDGER_MAX`]) — the stage stops claiming
/// [`MatchEvidence::OnlyDocumentForTheAmount`], because "no other document owes
/// this" is not something a partial ledger can say, and it is the one claim that
/// carries a suggestion on its own.
#[must_use]
pub fn likely_matches(
    line: &BankLine,
    candidates: &[Candidate],
    rules: &[MatchRule],
    ledger_complete: bool,
) -> Vec<LikelyMatch> {
    // Which rules fire on this line at all is a property of the line, so it is
    // decided once rather than once per candidate.
    let fired: Vec<&MatchRule> = rules.iter().filter(|rule| rule.fires_on(line)).collect();

    let eligible: Vec<&Candidate> = candidates
        .iter()
        .filter(|candidate| ensure_matchable(line, &candidate.invoice).is_ok())
        .collect();
    // Counted over **every** eligible document, including the one the exact
    // stage has already claimed: "the only invoice that owes this" has to be
    // true of the tenant's ledger, not merely of what is left after filtering.
    let owing_this_amount = if ledger_complete {
        Some(
            eligible
                .iter()
                .filter(|candidate| candidate.invoice.outstanding_cents == line.amount_cents)
                .count(),
        )
    } else {
        None
    };

    let mut ranked: Vec<LikelyMatch> = eligible
        .iter()
        .filter(|candidate| {
            // Never more than is owed, and never what the exact stage has.
            line.amount_cents <= candidate.invoice.outstanding_cents
                && exact_match(line, &candidate.invoice).is_none()
        })
        .filter_map(|candidate| weigh(line, candidate, &fired, owing_this_amount))
        .collect();
    ranked.sort_by(|one, other| {
        other
            .score
            .cmp(&one.score)
            .then(other.days_after_issue.cmp(&one.days_after_issue))
            .then(one.number.cmp(&other.number))
    });
    ranked.truncate(LIKELY_MATCHES_MAX);
    ranked
}

/// The evidence for one eligible candidate, or `None` when it does not reach
/// [`SCORE_MIN`].
fn weigh(
    line: &BankLine,
    candidate: &Candidate,
    fired: &[&MatchRule],
    owing_this_amount: Option<usize>,
) -> Option<LikelyMatch> {
    let Ok(days_after_issue) = ensure_matchable(line, &candidate.invoice) else {
        return None;
    };
    let outstanding_cents = candidate.invoice.outstanding_cents;
    let mut evidence: Vec<MatchEvidence> = Vec::new();

    if crate::bank_match::document_numbers(
        &line.remittance,
        crate::billing_sequence::INVOICE_NUMBER_PREFIX,
    )
    .iter()
    .any(|quoted| quoted.eq_ignore_ascii_case(candidate.invoice.number.trim()))
    {
        evidence.push(MatchEvidence::NumberQuoted);
    }

    let rule = fired
        .iter()
        .find(|rule| rule.customer_id == candidate.customer_id);
    if let Some(rule) = rule {
        evidence.push(MatchEvidence::RuleSaved {
            rule_id: rule.id.clone(),
            match_on: rule.match_on,
        });
    }

    let similarity_bp = name_similarity_bp(&line.counterparty_name, &candidate.customer_name);
    if similarity_bp >= NAME_SIMILAR_MIN_BP {
        evidence.push(MatchEvidence::CustomerNamed { similarity_bp });
    }

    if line.amount_cents == outstanding_cents {
        evidence.push(MatchEvidence::WholeAmount);
        if owing_this_amount == Some(1) {
            evidence.push(MatchEvidence::OnlyDocumentForTheAmount);
        }
    } else {
        evidence.push(MatchEvidence::PartPayment {
            remaining_cents: outstanding_cents - line.amount_cents,
        });
    }

    if let Some(due) = candidate.due_date {
        let days = (line.booked_on - due).whole_days();
        if days.abs() <= AROUND_DUE_DAYS {
            evidence.push(MatchEvidence::NearDue { days });
        }
    }

    let score: i32 = evidence.iter().map(MatchEvidence::points).sum();
    if score < SCORE_MIN {
        return None;
    }
    Some(LikelyMatch {
        invoice_id: candidate.invoice.invoice_id.clone(),
        number: candidate.invoice.number.clone(),
        amount_cents: line.amount_cents,
        outstanding_cents,
        customer_id: candidate.customer_id.clone(),
        days_after_issue,
        score,
        evidence,
        rule_id: rule.map(|rule| rule.id.clone()),
    })
}

/// Text as this stage compares it: lower case, the common European diacritics
/// folded to their base letter, everything that is not a letter or a digit
/// turned into a single blank, and the ends trimmed.
///
/// It is the one normalisation in the stage, used for the rules' patterns when
/// they are **stored** ([`crate::fin_match_rules`]) and for the line's fields
/// when they are compared, so the two can never drift apart.
///
/// **The fold is base letters, not transliterations.** `Müller` folds to
/// `muller`, and a bank that writes `MUELLER` therefore does *not* match it by
/// name. That is deliberate: undoing the German transliteration would also turn
/// `Bauer` into `Bar`, and a name signal that manufactures resemblances is worse
/// than one that misses some. The case is exactly what a saved rule is for — a
/// person confirms it once and the rule recognises that payer for ever after.
#[must_use]
pub fn folded(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            'ä' | 'Ä' | 'à' | 'À' | 'á' | 'Á' | 'â' | 'Â' | 'ã' | 'Ã' | 'å' | 'Å' => {
                out.push('a')
            }
            'æ' | 'Æ' => out.push_str("ae"),
            'ç' | 'Ç' | 'č' | 'Č' | 'ć' | 'Ć' => out.push('c'),
            'è' | 'È' | 'é' | 'É' | 'ê' | 'Ê' | 'ë' | 'Ë' | 'ě' | 'Ě' => out.push('e'),
            'ì' | 'Ì' | 'í' | 'Í' | 'î' | 'Î' | 'ï' | 'Ï' => out.push('i'),
            'ł' | 'Ł' => out.push('l'),
            'ñ' | 'Ñ' | 'ń' | 'Ń' => out.push('n'),
            'ö' | 'Ö' | 'ò' | 'Ò' | 'ó' | 'Ó' | 'ô' | 'Ô' | 'õ' | 'Õ' | 'ø' | 'Ø' => {
                out.push('o')
            }
            'ß' => out.push_str("ss"),
            'š' | 'Š' | 'ś' | 'Ś' => out.push('s'),
            'ü' | 'Ü' | 'ù' | 'Ù' | 'ú' | 'Ú' | 'û' | 'Û' => out.push('u'),
            'ý' | 'Ý' | 'ÿ' => out.push('y'),
            'ž' | 'Ž' | 'ź' | 'Ź' | 'ż' | 'Ż' => out.push('z'),
            _ if c.is_alphanumeric() => out.extend(c.to_lowercase()),
            _ => out.push(' '),
        }
    }
    out.split_whitespace().collect::<Vec<&str>>().join(" ")
}

/// An IBAN as this stage compares it: every blank and separator removed, folded
/// to lower case. No checksum is applied — that is the rule's business when it
/// is **written** ([`crate::fin_match_rules`]); here the question is only
/// whether the bank's string and the saved one are the same account.
#[must_use]
pub fn folded_iban(text: &str) -> String {
    folded(text).replace(' ', "")
}

/// The words that say what kind of company somebody is rather than which
/// company they are, dropped before two names are compared.
///
/// Sorted, and the suite below fails if the order slips: [`is_legal_form`]
/// binary-searches it. It leans European because our customers are, and it is a
/// list a person can read and correct rather than a rule about suffixes —
/// "Company" is a legal form, "Compagnie du Nord" is a name.
const LEGAL_FORMS: &[&str] = &[
    "ag", "aps", "as", "asa", "bv", "bvba", "co", "company", "cv", "eg", "ev", "gbr", "gmbh",
    "holding", "inc", "kft", "kg", "kk", "limited", "ltd", "mbh", "nv", "ohg", "oy", "oyj", "plc",
    "sa", "sarl", "sas", "spa", "sprl", "srl", "sro", "ug", "vof", "zoo",
];

/// Whether a folded word is one of [`LEGAL_FORMS`].
#[must_use]
fn is_legal_form(word: &str) -> bool {
    LEGAL_FORMS.binary_search(&word).is_ok()
}

/// The words of a name that say *which* company it is: folded, split, with the
/// legal forms and single characters dropped, sorted and de-duplicated.
fn name_words(name: &str) -> Vec<String> {
    let mut words: Vec<String> = folded(name)
        .split_whitespace()
        .filter(|word| word.chars().count() > 1 && !is_legal_form(word))
        .map(str::to_owned)
        .collect();
    words.sort();
    words.dedup();
    words
}

/// How alike two names are, in basis points: twice the words they share over the
/// words they have between them (a Dice coefficient in integer arithmetic).
///
/// Word sets rather than letters, because the differences that matter between a
/// bank's rendering of a payer and a customer record are whole words — a legal
/// form, a branch, a first name, the order of the two. `Kaffeehaus Bergmann
/// GmbH` and `Bergmann Kaffeehaus` are the same company at 10 000; `Bäckerei
/// Bergmann` shares one word of two and scores 5 000, which is below the bar
/// [`NAME_SIMILAR_MIN_BP`] sets.
///
/// Zero when either name has no words of its own left — a payer the bank did not
/// name, or a customer recorded as "GmbH", resembles nothing.
#[must_use]
pub fn name_similarity_bp(one: &str, other: &str) -> i64 {
    let one = name_words(one);
    let other = name_words(other);
    if one.is_empty() || other.is_empty() {
        return 0;
    }
    let shared = one.iter().filter(|word| other.contains(word)).count();
    // Both lengths are word counts of bounded strings; the product cannot
    // approach i64.
    let total = one.len() + other.len();
    (2 * shared as i64 * 10_000) / total as i64
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;
    use crate::bank_import::BankLineStatus;
    use crate::billing_invoices::InvoiceStatus;
    use crate::fin_match_rules::NewMatchRule;
    use crate::id::{BankLineId, BankStatementId, UserId};
    use time::{Month, OffsetDateTime};

    fn day(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).unwrap_or(Date::MIN)
    }

    fn line(amount_cents: i64, remittance: &str, counterparty: &str) -> BankLine {
        BankLine {
            id: BankLineId::new("line-1".to_owned()),
            statement_id: BankStatementId::new("stmt-1".to_owned()),
            line_no: 1,
            booked_on: day(2026, Month::February, 10),
            value_on: day(2026, Month::February, 10),
            amount_cents,
            currency: "EUR".to_owned(),
            counterparty_name: counterparty.to_owned(),
            counterparty_iban: "DE02 1203 0000 0000 2020 51".to_owned(),
            remittance: remittance.to_owned(),
            bank_ref: "REF9".to_owned(),
            status: BankLineStatus::Unmatched,
            ignored_reason: String::new(),
            created_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    fn candidate(tag: &str, outstanding_cents: i64, customer: &str) -> Candidate {
        Candidate {
            invoice: MatchCandidate {
                invoice_id: BillingInvoiceId::new(format!("inv-{tag}")),
                number: format!("INV-2026-{tag}"),
                currency: "EUR".to_owned(),
                outstanding_cents,
                status: InvoiceStatus::Issued,
                is_credit_note: false,
                issue_date: Some(day(2026, Month::January, 5)),
            },
            customer_id: BillingCustomerId::new(format!("cust-{customer}")),
            customer_name: customer.to_owned(),
            due_date: Some(day(2026, Month::February, 4)),
        }
    }

    fn rule(match_on: MatchOn, pattern: &str, customer: &str) -> MatchRule {
        MatchRule {
            id: FinMatchRuleId::new(format!("rule-{pattern}")),
            match_on,
            pattern: NewMatchRule {
                match_on,
                pattern: pattern.to_owned(),
                customer_id: BillingCustomerId::new(format!("cust-{customer}")),
            }
            .normalized_pattern()
            .expect("a valid pattern"),
            customer_id: BillingCustomerId::new(format!("cust-{customer}")),
            hits: 0,
            created_by: UserId::new("u-1".to_owned()),
            created_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn a_part_payment_quoting_our_number_is_the_case_the_exact_stage_hands_over() {
        // €1 307 owed, €500 arrives quoting the number: not exact, obviously
        // right, and the remainder is what a person needs to see.
        let found = likely_matches(
            &line(
                50_000,
                "INV-2026-00007 Teilzahlung",
                "Kaffeehaus Bergmann GmbH",
            ),
            &[candidate("00007", 130_700, "Kaffeehaus Bergmann")],
            &[],
            true,
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].number, "INV-2026-00007");
        assert_eq!(found[0].amount_cents, 50_000);
        assert_eq!(found[0].outstanding_cents, 130_700);
        assert!(found[0].evidence.contains(&MatchEvidence::NumberQuoted));
        assert!(found[0].evidence.contains(&MatchEvidence::PartPayment {
            remaining_cents: 80_700
        }));
    }

    #[test]
    fn what_the_exact_stage_already_claims_is_not_offered_a_second_time() {
        // Number quoted and the whole outstanding amount: that is an exact
        // match, and this stage says nothing about it.
        let whole = line(130_700, "INV-2026-00007", "Kaffeehaus Bergmann GmbH");
        assert!(
            likely_matches(
                &whole,
                &[candidate("00007", 130_700, "Kaffeehaus Bergmann")],
                &[],
                true
            )
            .is_empty()
        );
    }

    #[test]
    fn the_only_document_that_owes_the_amount_is_a_suggestion_and_two_of_them_are_not() {
        let paid = line(130_700, "no reference at all", "Anonymous Payer");
        let one = candidate("00007", 130_700, "Kaffeehaus Bergmann");
        let other = candidate("00009", 90_000, "Bäckerei Nord");
        let found = likely_matches(&paid, &[one.clone(), other], &[], true);
        assert_eq!(found.len(), 1, "the amount fits exactly one open document");
        assert_eq!(
            found[0].score,
            SCORE_MIN + MatchEvidence::NearDue { days: 6 }.points(),
            "the uniqueness is what carries it; the timing only adds to it"
        );
        assert!(
            found[0]
                .evidence
                .contains(&MatchEvidence::OnlyDocumentForTheAmount)
        );

        // A second document owing the same amount takes the uniqueness away,
        // and with it the only thing that identified either of them.
        let twin = candidate("00011", 130_700, "Bäckerei Nord");
        assert!(likely_matches(&paid, &[one, twin], &[], true).is_empty());
    }

    #[test]
    fn the_customers_own_name_on_the_line_carries_an_amount_that_fits() {
        let found = likely_matches(
            &line(130_700, "", "KAFFEEHAUS BERGMANN GMBH"),
            &[
                candidate("00007", 130_700, "Kaffeehaus Bergmann"),
                candidate("00011", 130_700, "Bäckerei Nord"),
            ],
            &[],
            true,
        );
        // The amount is no longer unique, so the name is what identifies it —
        // and it identifies exactly one of the two.
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].number, "INV-2026-00007");
        assert!(found[0].evidence.iter().any(|why| matches!(
            why,
            MatchEvidence::CustomerNamed { similarity_bp } if *similarity_bp == 10_000
        )));
    }

    #[test]
    fn a_name_that_merely_resembles_is_never_a_suggestion_on_its_own() {
        // Shares one word of two: below the bar, so not even recorded.
        let found = likely_matches(
            &line(50_000, "", "Baeckerei Bergmann"),
            &[candidate("00007", 130_700, "Kaffeehaus Bergmann")],
            &[],
            true,
        );
        assert!(found.is_empty());
        // And a resemblance that IS above the bar still does not reach the
        // floor while the amount says nothing: 20 points, and the floor is 45.
        let found = likely_matches(
            &line(50_000, "", "Kaffeehaus Bergmann Filiale Nord"),
            &[candidate("00007", 130_700, "Kaffeehaus Bergmann")],
            &[],
            true,
        );
        assert!(
            found.is_empty(),
            "a name and a part amount identify nothing"
        );
    }

    #[test]
    fn a_saved_rule_recognises_the_payer_a_name_alone_cannot() {
        // The transliteration the fold deliberately does not undo: the bank
        // writes MUELLER, the customer record says Müller. A rule the tenant
        // saved once is exactly what closes that gap.
        let transfer = line(50_000, "Abschlag", "MUELLER BAU");
        let owed = Candidate {
            customer_name: "Müller Bau".to_owned(),
            ..candidate("00007", 130_700, "muller-bau")
        };
        assert!(
            likely_matches(&transfer, std::slice::from_ref(&owed), &[], true).is_empty(),
            "without the rule the spelling is not recognised"
        );

        let saved = MatchRule {
            customer_id: owed.customer_id.clone(),
            ..rule(MatchOn::Counterparty, "mueller bau", "muller-bau")
        };
        let found = likely_matches(&transfer, &[owed], std::slice::from_ref(&saved), true);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].rule_id, Some(saved.id.clone()));
        assert!(found[0].evidence.contains(&MatchEvidence::RuleSaved {
            rule_id: saved.id,
            match_on: MatchOn::Counterparty,
        }));
    }

    #[test]
    fn a_rule_pointing_at_another_customer_ranks_nothing_of_this_ones() {
        let transfer = line(50_000, "Abschlag", "MUELLER BAU");
        let owed = candidate("00007", 130_700, "Kaffeehaus Bergmann");
        let elsewhere = rule(MatchOn::Counterparty, "mueller bau", "muller-bau");
        assert!(likely_matches(&transfer, &[owed], &[elsewhere], true).is_empty());
    }

    #[test]
    fn the_preconditions_of_the_exact_stage_are_the_preconditions_here() {
        let paid = line(130_700, "no reference", "Kaffeehaus Bergmann GmbH");
        let sound = candidate("00007", 130_700, "Kaffeehaus Bergmann");
        assert_eq!(
            likely_matches(&paid, std::slice::from_ref(&sound), &[], true).len(),
            1
        );

        for mutate in [
            Box::new(|c: &mut Candidate| c.invoice.is_credit_note = true)
                as Box<dyn Fn(&mut Candidate)>,
            Box::new(|c: &mut Candidate| c.invoice.status = InvoiceStatus::Draft),
            Box::new(|c: &mut Candidate| c.invoice.status = InvoiceStatus::Void),
            Box::new(|c: &mut Candidate| c.invoice.status = InvoiceStatus::Paid),
            Box::new(|c: &mut Candidate| c.invoice.currency = "USD".to_owned()),
            Box::new(|c: &mut Candidate| c.invoice.issue_date = None),
            // Issued after the bank booked the money.
            Box::new(|c: &mut Candidate| {
                c.invoice.issue_date = Some(day(2026, Month::March, 1));
            }),
        ] {
            let mut broken = sound.clone();
            mutate(&mut broken);
            assert!(
                likely_matches(&paid, &[broken], &[], true).is_empty(),
                "a document the exact stage would refuse is refused here too"
            );
        }

        // Money leaving the account settles no receivable, whatever it quotes.
        let leaving = line(-130_700, "INV-2026-00007", "Kaffeehaus Bergmann GmbH");
        assert!(likely_matches(&leaving, std::slice::from_ref(&sound), &[], true).is_empty());
        // And a line that is no longer open is not a line to suggest for.
        let mut settled = paid.clone();
        settled.status = BankLineStatus::Matched;
        assert!(likely_matches(&settled, &[sound], &[], true).is_empty());
    }

    #[test]
    fn more_money_than_the_document_owes_is_never_offered_for_it() {
        // A cent over is not an overpayment to record; it is a question.
        let over = line(130_701, "INV-2026-00007", "Kaffeehaus Bergmann GmbH");
        assert!(
            likely_matches(
                &over,
                &[candidate("00007", 130_700, "Kaffeehaus Bergmann")],
                &[],
                true
            )
            .is_empty()
        );
    }

    #[test]
    fn the_list_is_ordered_by_evidence_then_by_the_oldest_debt() {
        // Two documents of the same customer, same amount, both plausible: the
        // one quoted by number leads, and between two equals the older debt
        // does.
        let transfer = line(50_000, "INV-2026-00011", "Kaffeehaus Bergmann GmbH");
        // Issued 5 January: the oldest of the three.
        let older = candidate("00007", 130_700, "Kaffeehaus Bergmann");
        let mut quoted = candidate("00011", 130_700, "Kaffeehaus Bergmann");
        quoted.invoice.issue_date = Some(day(2026, Month::January, 20));
        let mut newer = candidate("00013", 130_700, "Kaffeehaus Bergmann");
        newer.invoice.issue_date = Some(day(2026, Month::January, 20));
        let found = likely_matches(&transfer, &[newer, older, quoted], &[], true);
        assert_eq!(found.len(), 3);
        assert_eq!(found[0].number, "INV-2026-00011", "quoted by number");
        assert_eq!(found[1].number, "INV-2026-00007", "the older debt next");
        assert_eq!(found[2].number, "INV-2026-00013");
        assert!(found[0].score > found[1].score);
    }

    #[test]
    fn no_more_than_a_handful_is_ever_offered() {
        let transfer = line(50_000, "INV-2026-00001", "Kaffeehaus Bergmann GmbH");
        let candidates: Vec<Candidate> = (1..=LIKELY_MATCHES_MAX + 3)
            .map(|n| candidate(&format!("{n:05}"), 130_700, "Kaffeehaus Bergmann"))
            .collect();
        let found = likely_matches(&transfer, &candidates, &[], true);
        assert_eq!(found.len(), LIKELY_MATCHES_MAX);
        assert_eq!(found[0].number, "INV-2026-00001", "the quoted one survives");
    }

    #[test]
    fn a_ledger_that_was_not_read_whole_makes_no_claim_about_being_the_only_one() {
        // The caller had to cap the open documents it read, so "no other
        // invoice owes this" is not something it can know — and it is the one
        // claim that carries a suggestion by itself.
        let paid = line(130_700, "no reference at all", "Anonymous Payer");
        let one = candidate("00007", 130_700, "Kaffeehaus Bergmann");
        assert_eq!(
            likely_matches(&paid, std::slice::from_ref(&one), &[], true).len(),
            1
        );
        assert!(likely_matches(&paid, &[one], &[], false).is_empty());

        // What the cap does not do is silence the evidence that identifies a
        // document on its own.
        let quoted = line(50_000, "INV-2026-00007", "Anonymous Payer");
        let owed = candidate("00007", 130_700, "Kaffeehaus Bergmann");
        assert_eq!(likely_matches(&quoted, &[owed], &[], false).len(), 1);
    }

    #[test]
    fn timing_is_supporting_evidence_and_never_the_reason() {
        let paid = line(130_700, "", "Anonymous Payer");
        // Due six days before the money arrived: near, worth ten.
        let near = candidate("00007", 130_700, "Kaffeehaus Bergmann");
        let found = likely_matches(&paid, std::slice::from_ref(&near), &[], true);
        assert_eq!(
            found[0].evidence.last(),
            Some(&MatchEvidence::NearDue { days: 6 })
        );
        assert_eq!(found[0].score, SCORE_MIN + 10);

        // Due half a year earlier: the timing says nothing and is not recorded.
        let stale = Candidate {
            due_date: Some(day(2025, Month::August, 1)),
            ..near
        };
        let found = likely_matches(&paid, &[stale], &[], true);
        assert_eq!(found[0].score, SCORE_MIN);
        assert!(
            !found[0]
                .evidence
                .iter()
                .any(|why| matches!(why, MatchEvidence::NearDue { .. }))
        );
    }

    #[test]
    fn folding_is_lower_case_base_letters_and_single_blanks() {
        assert_eq!(
            folded("  Kaffeehaus   Bergmann GmbH "),
            "kaffeehaus bergmann gmbh"
        );
        assert_eq!(folded("Müller & Söhne, Zürich"), "muller sohne zurich");
        assert_eq!(folded("Æblegård ApS"), "aeblegard aps");
        assert_eq!(folded("Straße 7"), "strasse 7");
        assert_eq!(folded("ŁÓDŹ"), "lodz");
        assert_eq!(folded(""), "");
        assert_eq!(folded("---"), "");
        // The transliteration is deliberately not undone (see `folded`).
        assert_ne!(folded("Müller"), folded("Mueller"));
        // An IBAN is the same fold with its blanks closed up.
        assert_eq!(
            folded_iban("DE02 1203 0000 0000 2020 51"),
            "de02120300000000202051"
        );
        assert_eq!(folded_iban(""), "");
    }

    #[test]
    fn a_name_is_its_words_without_its_legal_form() {
        assert_eq!(
            name_similarity_bp("Kaffeehaus Bergmann GmbH", "Kaffeehaus Bergmann"),
            10_000
        );
        assert_eq!(
            name_similarity_bp("Bergmann Kaffeehaus", "Kaffeehaus Bergmann GmbH"),
            10_000,
            "the order of the words is the bank's business, not the company's"
        );
        assert_eq!(
            name_similarity_bp("Bäckerei Bergmann", "Kaffeehaus Bergmann"),
            5_000
        );
        assert_eq!(
            name_similarity_bp("Kaffeehaus", "Kaffeehaus Bergmann"),
            6_666
        );
        assert_eq!(name_similarity_bp("", "Kaffeehaus Bergmann"), 0);
        assert_eq!(
            name_similarity_bp("GmbH", "Kaffeehaus Bergmann"),
            0,
            "a name that is only a legal form is no name"
        );
        assert!(
            name_similarity_bp("Kaffeehaus", "Kaffeehaus Bergmann") >= NAME_SIMILAR_MIN_BP,
            "two words of three is above the bar"
        );
        assert!(
            name_similarity_bp("Bäckerei Bergmann", "Kaffeehaus Bergmann") < NAME_SIMILAR_MIN_BP
        );
    }

    #[test]
    fn the_legal_forms_are_sorted_because_the_lookup_binary_searches_them() {
        let mut sorted = LEGAL_FORMS.to_vec();
        sorted.sort_unstable();
        assert_eq!(LEGAL_FORMS, sorted.as_slice());
        assert!(is_legal_form("gmbh"));
        assert!(!is_legal_form("kaffeehaus"));
    }

    #[test]
    fn the_score_floor_is_exactly_what_a_unique_amount_is_worth() {
        // The invariant the whole precision argument rests on: no soft signal
        // reaches the floor alone, and the weakest identifying combination
        // reaches it exactly.
        assert_eq!(
            MatchEvidence::WholeAmount.points() + MatchEvidence::OnlyDocumentForTheAmount.points(),
            SCORE_MIN
        );
        assert!(MatchEvidence::WholeAmount.points() < SCORE_MIN);
        assert!(
            MatchEvidence::CustomerNamed {
                similarity_bp: 10_000
            }
            .points()
                < SCORE_MIN
        );
        assert!(MatchEvidence::NearDue { days: 0 }.points() < SCORE_MIN);
        assert_eq!(
            MatchEvidence::PartPayment { remaining_cents: 1 }.points(),
            0
        );
        // …and the two strongest identifications each carry a suggestion alone,
        // which is what makes a part payment quoting our number visible.
        assert!(MatchEvidence::NumberQuoted.points() >= SCORE_MIN);
        assert!(
            MatchEvidence::RuleSaved {
                rule_id: FinMatchRuleId::new("r".to_owned()),
                match_on: MatchOn::Iban,
            }
            .points()
                >= SCORE_MIN
        );
    }
}
