//! Suggesting the category of a claim nobody has classified (alo Finance,
//! ADR 0035, wave B4.14a; `docs/design/finance.md`, "The finance agent") — the
//! store half of the agent's `categorise_transactions`.
//!
//! # A suggestion is a different column from a decision
//!
//! [`crate::fin_expenses::Expense::category_id`] is the word a **person** chose,
//! and it is the only thing a posting rule, a report or a VAT return ever
//! reads. What this module writes is [`Expense::proposed_category_id`]: what the
//! machine thinks, in a column nothing downstream looks at. A guess written into
//! the decided column would be indistinguishable from a decision the moment it
//! landed, and it would be in the P&L (ADR 0023: the agent proposes, the person
//! approves).
//!
//! Accepting therefore *moves* the value from one column to the other
//! ([`AccountStore::accept_category_proposal`]) and is subject to every rule a
//! hand-picked category is: the claim must still be the claimant's own to
//! change, and the category must still be one the tenant offers.
//!
//! # Where a suggestion comes from, and why it is not a word list
//!
//! From the claimant's **own past claims**: the categories they have already
//! agreed to for the same merchant. Nothing here contains a vocabulary — no
//! "Uber → Travel" table, no English at all — for the reason
//! [`crate::fin_categories`] ships empty: the categories are the tenant's own
//! words, in the tenant's own language, and a hardcoded list would be a bug in
//! a European product. A tenant who has never classified a coffee gets no
//! suggestion about coffee, which is the honest answer.
//!
//! It reads **only the caller's own history**, and that is a privacy rule
//! rather than a convenience. A claim names a restaurant, a clinic, a city on a
//! date; building a tenant-wide merchant map out of everybody's receipts would
//! answer "who has been to that pharmacy" as a side effect. The personal door
//! has no cross-user read for exactly this reason ([`crate::fin_expenses`]), and
//! this module does not open one.
//!
//! # No model runs here
//!
//! The decision is [`plan_categorisation`], a pure function over rows. It is
//! deterministic, it is unit-tested without a database, and it is the same
//! answer twice for the same history. The agent's part is deciding to ask —
//! everything about *which* category is arithmetic over what the person already
//! agreed to.
//!
//! # Asking twice suggests nothing twice
//!
//! A claim that already carries an open suggestion is left alone, and a claim
//! whose suggestion was **declined** is never suggested again
//! ([`Expense::proposal_declined_at`]): "no" has to survive the clearing of the
//! thing it was said about, or the next run offers the same rejected word and
//! the person stops reading suggestions altogether.

use std::collections::HashMap;

use time::Date;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::fin_expenses::{EXPENSE_COLS, Expense, ExpenseRow};
use crate::id::{FinCategoryId, FinExpenseId};

/// The only reason code this slice writes: the claimant has classified this
/// merchant before. Machine-readable on purpose — the words are the client's,
/// in the reader's language.
pub const REASON_MERCHANT_HISTORY: &str = "merchantHistory";

/// Left out: the claim names no merchant, so there is nothing to recognise it
/// by. Never a guess from the description, which is a sentence a person wrote
/// and not a payee.
pub const SKIP_NO_MERCHANT: &str = "noMerchant";

/// Left out: this claimant has never classified a claim from that merchant.
pub const SKIP_NO_HISTORY: &str = "noHistory";

/// Left out: it already carries a suggestion nobody has answered yet.
pub const SKIP_ALREADY_PROPOSED: &str = "alreadyProposed";

/// Left out: the claimant declined a suggestion on this claim before.
pub const SKIP_DECLINED: &str = "declined";

/// Most claims one call will look at. A person with more than this many
/// unclassified claims in the asked-for period has a backlog no single batch of
/// suggestions helps with; the rest come back as a count, never silently
/// dropped.
pub const CATEGORISE_CLAIMS_MAX: i64 = 100;

/// Most past claims read as evidence. Bounded because it is an unpaged read of
/// somebody's whole history; the most recent ones are what a habit looks like,
/// and a merchant not seen in the last thousand claims is not a habit.
pub const CATEGORISE_HISTORY_MAX: i64 = 1_000;

/// One claim of the caller's past that a category was agreed for — the evidence
/// a suggestion is made of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassifiedClaim {
    /// Who was paid. Personal data: never logged.
    pub merchant: String,
    /// The category the claimant agreed to for it.
    pub category_id: FinCategoryId,
    /// The day it was spent — how a tie between two categories is settled.
    pub spent_on: Date,
}

/// One suggestion this call will write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CategoryProposal {
    /// The claim it is about.
    pub expense_id: FinExpenseId,
    /// The category being suggested.
    pub category_id: FinCategoryId,
    /// Why, as a code ([`REASON_MERCHANT_HISTORY`]).
    pub reason: &'static str,
    /// How many of the claimant's own past claims back it. The figure a person
    /// judges a suggestion by, and the only reason it is carried out of here:
    /// "you booked this merchant here four times" is an argument, "the computer
    /// says so" is not.
    pub evidence: usize,
}

/// One claim this call will not suggest anything for, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedClaim {
    /// The claim.
    pub expense_id: FinExpenseId,
    /// One of the `SKIP_*` codes.
    pub reason: &'static str,
}

/// What a call decided about a period of somebody's unclassified claims.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CategorisePlan {
    /// The suggestions to write, in the order the claims were read.
    pub proposed: Vec<CategoryProposal>,
    /// Everything else that was looked at, with its reason.
    pub skipped: Vec<SkippedClaim>,
}

/// Two claims are from the same merchant when their names match once case and
/// spacing stop mattering — "LUFTHANSA" and "Lufthansa  " are one payee to
/// everybody reading the list.
///
/// Deliberately nothing cleverer. Stripping punctuation or matching prefixes
/// would merge two payees that a person distinguishes, and a suggestion made
/// from the wrong merchant is worse than no suggestion at all.
#[must_use]
pub fn merchant_key(merchant: &str) -> String {
    merchant
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// The whole decision, as pure code: which unclassified claims get a suggestion,
/// which category each gets, and which are left alone.
///
/// The rule is the claimant's own habit — for each claim, the category they have
/// most often agreed to for that merchant. A tie is settled by **recency**: the
/// one they used last is the one they are using now. It suggests nothing at all
/// when the merchant is new, because a suggestion drawn from no evidence is a
/// coin toss a person then has to check.
///
/// `unclassified` are claims with no category; anything that already carries a
/// suggestion, or whose suggestion was declined, is skipped here rather than
/// filtered by the caller, so the reason reaches the person who asked.
#[must_use]
pub fn plan_categorisation(
    unclassified: &[Expense],
    history: &[ClassifiedClaim],
) -> CategorisePlan {
    let habits = habits(history);
    let mut plan = CategorisePlan::default();
    for claim in unclassified {
        let skip = |reason| SkippedClaim {
            expense_id: claim.id.clone(),
            reason,
        };
        if claim.proposal_declined_at.is_some() {
            plan.skipped.push(skip(SKIP_DECLINED));
            continue;
        }
        if claim.proposed_category_id.is_some() {
            plan.skipped.push(skip(SKIP_ALREADY_PROPOSED));
            continue;
        }
        let key = merchant_key(&claim.merchant);
        if key.is_empty() {
            plan.skipped.push(skip(SKIP_NO_MERCHANT));
            continue;
        }
        match habits.get(&key) {
            None => plan.skipped.push(skip(SKIP_NO_HISTORY)),
            Some(habit) => plan.proposed.push(CategoryProposal {
                expense_id: claim.id.clone(),
                category_id: habit.category_id.clone(),
                reason: REASON_MERCHANT_HISTORY,
                evidence: habit.times,
            }),
        }
    }
    plan
}

/// What the claimant has settled on for one merchant.
#[derive(Debug, Clone)]
struct Habit {
    category_id: FinCategoryId,
    times: usize,
}

/// The claimant's habits, one per merchant: most-used category wins, and the
/// most recently used one breaks a tie.
fn habits(history: &[ClassifiedClaim]) -> HashMap<String, Habit> {
    // merchant → category → (times, the last day it was used)
    let mut tally: HashMap<String, HashMap<String, (usize, Date)>> = HashMap::new();
    for past in history {
        let key = merchant_key(&past.merchant);
        if key.is_empty() {
            continue;
        }
        let per_category = tally.entry(key).or_default();
        let seen = per_category
            .entry(past.category_id.as_str().to_owned())
            .or_insert((0, past.spent_on));
        seen.0 += 1;
        seen.1 = seen.1.max(past.spent_on);
    }
    tally
        .into_iter()
        .filter_map(|(merchant, per_category)| {
            let (category, (times, _)) = per_category
                .into_iter()
                // Most used, then most recent, then the id — so the answer is
                // the same one twice however the rows arrived.
                .max_by(|a, b| {
                    let (times_a, last_a) = a.1;
                    let (times_b, last_b) = b.1;
                    times_a
                        .cmp(&times_b)
                        .then(last_a.cmp(&last_b))
                        .then(b.0.cmp(&a.0))
                })?;
            Some((
                merchant,
                Habit {
                    category_id: FinCategoryId::new(category),
                    times,
                },
            ))
        })
        .collect()
}

impl AccountStore {
    /// Suggests a category for the caller's **own** unclassified claims spent
    /// between `from` and `to`, both days included, and writes each suggestion
    /// onto its claim.
    ///
    /// Only claims that are still the claimant's own to change are looked at: a
    /// claim somebody is deciding cannot take a new category anyway, and
    /// offering one on it would be an offer the accept verb then refuses.
    ///
    /// The whole plan is decided before the first write ([`plan_categorisation`])
    /// so a partial batch cannot mean something different from a whole one, and
    /// every write is a suggestion in nobody's books.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the period ends before it starts;
    /// [`StoreError::Db`] on failure.
    pub async fn propose_expense_categories(&self, from: Date, to: Date) -> Result<CategorisePlan> {
        if to < from {
            return Err(StoreError::Validation(
                "the end of the period must not be before its start".to_owned(),
            ));
        }
        let unclassified = self.unclassified_claims(from, to).await?;
        let history = self.classified_history().await?;
        let plan = plan_categorisation(&unclassified, &history);
        for proposal in &plan.proposed {
            sqlx::query(
                "UPDATE fin_expenses \
                    SET proposed_category_id = $4, proposed_at = now(), proposed_reason = $5, \
                        updated_at = now() \
                 WHERE tenant_id = $1 AND user_id = $2 AND id = $3 \
                   AND category_id IS NULL AND proposed_category_id IS NULL \
                   AND proposal_declined_at IS NULL",
            )
            .bind(self.tenant.as_str())
            .bind(self.user.as_str())
            .bind(proposal.expense_id.as_str())
            .bind(proposal.category_id.as_str())
            .bind(proposal.reason)
            .execute(&self.pool)
            .await
            .map_err(StoreError::Db)?;
        }
        Ok(plan)
    }

    /// The caller's own claims in the period that carry no category and are
    /// still theirs to change, newest purchase first.
    async fn unclassified_claims(&self, from: Date, to: Date) -> Result<Vec<Expense>> {
        let rows = sqlx::query_as::<_, ExpenseRow>(&format!(
            "SELECT {EXPENSE_COLS} FROM fin_expenses \
             WHERE tenant_id = $1 AND user_id = $2 AND spent_on >= $3 AND spent_on <= $4 \
               AND category_id IS NULL AND status IN ('draft', 'rejected') \
             ORDER BY spent_on DESC, created_at DESC, id \
             LIMIT {CATEGORISE_CLAIMS_MAX}"
        ))
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter().map(ExpenseRow::into_expense).collect()
    }

    /// The caller's own classified claims, newest first — the evidence.
    ///
    /// Joined to the categories so a word the tenant has **retired** cannot be
    /// suggested afresh: an inactive category may stay on the claims that
    /// already carried it ([`AccountStore::require_links`]), but proposing it
    /// would be an offer the accept verb refuses.
    ///
    /// Not limited to the asked-for period: a habit is a habit whenever it was
    /// formed.
    async fn classified_history(&self) -> Result<Vec<ClassifiedClaim>> {
        let rows = sqlx::query_as::<_, HistoryRow>(&format!(
            "SELECT e.merchant, e.category_id, e.spent_on \
             FROM fin_expenses e \
             JOIN fin_categories c ON c.tenant_id = e.tenant_id AND c.id = e.category_id \
             WHERE e.tenant_id = $1 AND e.user_id = $2 AND e.merchant <> '' AND c.active \
             ORDER BY e.spent_on DESC, e.id \
             LIMIT {CATEGORISE_HISTORY_MAX}"
        ))
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows
            .into_iter()
            .map(|row| ClassifiedClaim {
                merchant: row.merchant,
                category_id: FinCategoryId::new(row.category_id),
                spent_on: row.spent_on,
            })
            .collect())
    }

    /// The claimant agrees: the suggested category becomes the claim's own.
    ///
    /// Every rule a hand-picked category is subject to applies here, because
    /// this *is* picking one: the claim must still be the claimant's to change,
    /// and the category must still be offered — a word the tenant retired
    /// between the suggestion and the answer is refused with the same words the
    /// claim form would use.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the claim is not the caller's own or the
    /// suggested category has since been deleted; [`StoreError::Conflict`] when
    /// the claim has been handed in, or carries no suggestion to accept;
    /// [`StoreError::Validation`] when the suggested category is no longer
    /// offered; [`StoreError::Db`] on failure.
    pub async fn accept_category_proposal(&self, id: &FinExpenseId) -> Result<Expense> {
        let claim = self.expense(id).await?.ok_or(StoreError::NotFound)?;
        let proposed = claim
            .proposed_category_id
            .clone()
            .ok_or_else(nothing_to_answer)?;
        if !claim.is_editable() {
            return Err(handed_in("accepted"));
        }
        let category = self
            .fin_category(&proposed)
            .await?
            .ok_or(StoreError::NotFound)?;
        if !category.active {
            return Err(StoreError::Validation(
                "that category is no longer offered".to_owned(),
            ));
        }
        let row = sqlx::query_as::<_, ExpenseRow>(&format!(
            "UPDATE fin_expenses \
                SET category_id = proposed_category_id, proposed_category_id = NULL, \
                    proposed_at = NULL, proposed_reason = '', updated_at = now() \
             WHERE tenant_id = $1 AND user_id = $2 AND id = $3 \
               AND proposed_category_id IS NOT NULL AND status IN ('draft', 'rejected') \
             RETURNING {EXPENSE_COLS}"
        ))
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?
        // Re-tested inside the statement, so a submit or a second answer that
        // lands between the read and the write wins rather than being
        // overwritten by an answer that never saw it.
        .ok_or_else(|| handed_in("accepted"))?;
        row.into_expense()
    }

    /// The claimant says no: the suggestion is cleared, and nothing suggests
    /// that claim again.
    ///
    /// The refusal outlives the suggestion ([`Expense::proposal_declined_at`]) —
    /// the next run of the tool would otherwise offer the same word, and a
    /// suggestion a person has to decline twice is one they stop reading. The
    /// claim stays classifiable by hand, which is the point of saying no.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the claim is not the caller's own;
    /// [`StoreError::Conflict`] when it carries no suggestion to decline;
    /// [`StoreError::Db`] on failure.
    pub async fn decline_category_proposal(&self, id: &FinExpenseId) -> Result<Expense> {
        let row = sqlx::query_as::<_, ExpenseRow>(&format!(
            "UPDATE fin_expenses \
                SET proposed_category_id = NULL, proposed_at = NULL, proposed_reason = '', \
                    proposal_declined_at = now(), updated_at = now() \
             WHERE tenant_id = $1 AND user_id = $2 AND id = $3 \
               AND proposed_category_id IS NOT NULL \
             RETURNING {EXPENSE_COLS}"
        ))
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        match row {
            Some(row) => row.into_expense(),
            // A claim that is not the caller's own and one that carries no
            // suggestion are different answers, and neither of them discloses
            // the other: absent, or "there is nothing here to answer".
            None => match self.expense(id).await? {
                None => Err(StoreError::NotFound),
                Some(_) => Err(nothing_to_answer()),
            },
        }
    }
}

/// The refusal both answer verbs read when there is no suggestion on the claim.
fn nothing_to_answer() -> StoreError {
    StoreError::Conflict("this claim carries no suggested category".to_owned())
}

/// The refusal a write to a handed-in claim reads. Spelled the way
/// [`crate::fin_expenses`] spells it, because it is the same rule.
fn handed_in(verb: &str) -> StoreError {
    StoreError::Conflict(format!(
        "a claim that has been handed in cannot be {verb}; withdraw it first"
    ))
}

/// One evidence row, exactly as the join returns it.
#[derive(sqlx::FromRow)]
struct HistoryRow {
    merchant: String,
    category_id: String,
    spent_on: Date,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::fin_expenses::{ExpenseMethod, ExpenseStatus};
    use crate::id::UserId;
    use time::{Month, OffsetDateTime};

    fn day(day: u8) -> Date {
        Date::from_calendar_date(2026, Month::March, day).expect("a March day")
    }

    fn past(merchant: &str, category: &str, on: u8) -> ClassifiedClaim {
        ClassifiedClaim {
            merchant: merchant.to_owned(),
            category_id: FinCategoryId::new(category),
            spent_on: day(on),
        }
    }

    fn claim(id: &str, merchant: &str) -> Expense {
        Expense {
            id: FinExpenseId::new(id),
            user_id: UserId::new("u1"),
            spent_on: day(20),
            category_id: None,
            merchant: merchant.to_owned(),
            description: String::new(),
            gross_cents: 1000,
            vat_cents: 0,
            vat_rate_bp: None,
            currency: "EUR".to_owned(),
            method: ExpenseMethod::Personal,
            project_id: None,
            receipt_node_id: None,
            status: ExpenseStatus::Draft,
            submitted_at: None,
            decided_by: None,
            decided_at: None,
            decision_note: String::new(),
            reimbursed_on: None,
            proposed_category_id: None,
            proposed_at: None,
            proposed_reason: String::new(),
            proposal_declined_at: None,
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn one_payee_however_it_was_typed() {
        assert_eq!(merchant_key("  LUFTHANSA  "), "lufthansa");
        assert_eq!(merchant_key("Deutsche\tBahn  AG"), "deutsche bahn ag");
        assert_eq!(merchant_key("   "), "");
        // Nothing cleverer: two payees a person tells apart stay apart.
        assert_ne!(merchant_key("Bahn"), merchant_key("Bahn AG"));
    }

    #[test]
    fn the_category_a_person_uses_most_for_that_merchant_is_the_one_suggested() {
        let history = [
            past("Bahn", "travel", 1),
            past("bahn", "travel", 5),
            past("BAHN", "meals", 9),
            past("Kaffee GmbH", "meals", 3),
        ];
        let plan = plan_categorisation(&[claim("e1", "Bahn")], &history);
        assert!(plan.skipped.is_empty());
        assert_eq!(plan.proposed.len(), 1);
        let proposal = &plan.proposed[0];
        assert_eq!(proposal.category_id.as_str(), "travel");
        assert_eq!(proposal.reason, REASON_MERCHANT_HISTORY);
        // The figure that makes the suggestion an argument rather than an oracle.
        assert_eq!(proposal.evidence, 2);
    }

    #[test]
    fn a_tie_goes_to_the_one_they_used_last() {
        let history = [past("Bahn", "travel", 1), past("Bahn", "meals", 20)];
        let plan = plan_categorisation(&[claim("e1", "Bahn")], &history);
        assert_eq!(plan.proposed[0].category_id.as_str(), "meals");
        assert_eq!(plan.proposed[0].evidence, 1);
    }

    #[test]
    fn a_new_merchant_gets_no_guess() {
        let plan = plan_categorisation(&[claim("e1", "Neuer Laden")], &[past("Bahn", "travel", 1)]);
        assert!(plan.proposed.is_empty(), "nothing is invented");
        assert_eq!(
            plan.skipped,
            vec![SkippedClaim {
                expense_id: FinExpenseId::new("e1"),
                reason: SKIP_NO_HISTORY,
            }]
        );
    }

    #[test]
    fn a_claim_with_no_payee_is_left_alone() {
        let plan = plan_categorisation(&[claim("e1", "   ")], &[past("Bahn", "travel", 1)]);
        assert_eq!(plan.skipped[0].reason, SKIP_NO_MERCHANT);
        assert!(plan.proposed.is_empty());
    }

    #[test]
    fn asking_twice_suggests_nothing_twice_and_a_no_survives() {
        let history = [past("Bahn", "travel", 1)];

        let mut open = claim("e1", "Bahn");
        open.proposed_category_id = Some(FinCategoryId::new("travel"));
        open.proposed_at = Some(OffsetDateTime::UNIX_EPOCH);
        open.proposed_reason = REASON_MERCHANT_HISTORY.to_owned();
        let plan = plan_categorisation(&[open], &history);
        assert_eq!(plan.skipped[0].reason, SKIP_ALREADY_PROPOSED);

        // Declined wins over everything, including a suggestion that is somehow
        // still on the row: a "no" is the last word.
        let mut declined = claim("e2", "Bahn");
        declined.proposal_declined_at = Some(OffsetDateTime::UNIX_EPOCH);
        let plan = plan_categorisation(&[declined], &history);
        assert_eq!(plan.skipped[0].reason, SKIP_DECLINED);
        assert!(plan.proposed.is_empty());
    }

    #[test]
    fn a_history_of_nothing_suggests_nothing_and_never_panics() {
        let plan = plan_categorisation(&[claim("e1", "Bahn")], &[]);
        assert!(plan.proposed.is_empty());
        assert_eq!(plan.skipped[0].reason, SKIP_NO_HISTORY);
        assert_eq!(
            plan_categorisation(&[], &[past("Bahn", "travel", 1)]),
            CategorisePlan::default()
        );
    }

    #[test]
    fn every_claim_looked_at_comes_back_in_one_list_or_the_other() {
        let claims = [
            claim("e1", "Bahn"),
            claim("e2", ""),
            claim("e3", "Unbekannt"),
        ];
        let plan = plan_categorisation(&claims, &[past("Bahn", "travel", 2)]);
        assert_eq!(plan.proposed.len() + plan.skipped.len(), claims.len());
    }
}
