//! Executing the **Finance** tools of an approved agent proposal (ADR 0034,
//! ADR 0035 wave B4.14a) — the acting half of what [`alo_ai::agent_finance`]
//! describes to the model.
//!
//! Called only from [`crate::agent::agent_execute`], which is the single acting
//! path: the user saw the proposal and approved it. Everything here runs
//! through the caller's own tenant-scoped store handle, on the **personal**
//! door — a claim is personal data about one employee, and no function on that
//! door takes somebody else's id.
//!
//! Three rules shape this module:
//!
//! - **The model chooses no category.** The whole classification is the store's
//!   ([`alo_store::plan_categorisation`]): a deterministic reading of the words
//!   this person has already agreed to for the same merchant. This executor
//!   parses a *period* and nothing else, because a period is the only thing the
//!   proposal is allowed to carry. Were a category ever to arrive in the args,
//!   there is no code here that would read it.
//! - **What it writes is a suggestion, all the way down.** Each lands in a
//!   column no posting rule, report or VAT return reads, waiting for the
//!   claimant to accept it one claim at a time
//!   (`docs/design/finance.md` § The finance agent). Running the tool never
//!   changes what the books say.
//! - **What was left out is part of the answer.** A claim with no payee, a
//!   merchant this person has never classified, a suggestion already waiting, a
//!   suggestion they declined: each comes back as a skipped line with a
//!   machine-readable reason the client writes words for. Silence would read as
//!   "everything is sorted", which is a different and usually wrong statement.
//!
//! The result carries figures and reason codes only — never a sentence. A
//! sentence composed here would be a user-facing string authored in the server
//! in one language, which is a bug in a European product (CLAUDE.md).
//!
//! Merchant names are personal data (a clinic, a bar, a pharmacy on a date).
//! They travel back to the person whose claims they are, exactly as the expense
//! list does, and reach no log on the way.

use std::collections::HashMap;

use axum::Json;
use serde_json::{Value, json};
use time::{Date, Duration, OffsetDateTime};

use alo_store::{CategoryProposal, Expense, SkippedClaim};

use crate::agent_args::{string_arg, unprocessable};
use crate::billing::{iso_date, map_store_err, parse_iso_date};
use crate::error::Problem;
use crate::state::Account;

/// How far back a call looks when the proposal states no period.
///
/// A quarter is the backlog a person actually means by "sort out my expenses";
/// a year would be a batch of suggestions nobody reads to the end, and the
/// model is free to ask for one explicitly when the user did.
const DEFAULT_PERIOD_DAYS: i64 = 90;

/// The longest period one call may cover, both ends included — the same ceiling
/// the expense list itself refuses past ([`crate::finance_expenses`]).
///
/// Shared with the answering tools ([`crate::agent_finance_answers`]): one
/// ceiling for the whole finance agent, so a period refused by one of its tools
/// is refused by all of them in the same words.
pub(crate) const MAX_PERIOD_DAYS: i64 = 366;

/// `categorise_transactions` — suggest a category for the caller's own
/// unclassified claims over a period.
///
/// The order is: read the period, let the store decide and write the whole
/// plan, then name what it did in the caller's own vocabulary. The category
/// *names* are joined on afterwards because the store answers in ids and a
/// person reads words — and the words are the tenant's own, which is why no
/// string in this file is one of them.
///
/// # Errors
/// `422` when a bound is not a plain `YYYY-MM-DD`, when the period runs
/// backwards or is longer than [`MAX_PERIOD_DAYS`]; the store's own `500`
/// otherwise.
pub async fn execute_categorise_transactions(
    account: &Account,
    args: &Value,
) -> Result<Json<Value>, Problem> {
    let (from, to) = period(args, OffsetDateTime::now_utc().date(), DEFAULT_PERIOD_DAYS)?;
    let plan = account
        .acc
        .propose_expense_categories(from, to)
        .await
        .map_err(map_store_err)?;

    // One read of the tenant's own words, so every suggestion can be shown as
    // the category rather than as an id. Inactive ones included: the store
    // never suggests a retired word, but a *claim* may still carry one, and a
    // list that could not name it would print an id at somebody.
    let categories = account
        .acc
        .fin_categories(true)
        .await
        .map_err(map_store_err)?;
    let names: HashMap<&str, &str> = categories
        .iter()
        .map(|category| (category.id.as_str(), category.name.as_str()))
        .collect();

    // The claims the plan is about, so the answer shows what each suggestion is
    // for — a payee and an amount, which is what a person recognises a claim by.
    let claims = account
        .acc
        .expenses(from, to, None)
        .await
        .map_err(map_store_err)?;
    let by_id: HashMap<&str, &Expense> = claims
        .iter()
        .map(|claim| (claim.id.as_str(), claim))
        .collect();

    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "categoryProposals",
            "from": iso_date(from),
            "to": iso_date(to),
            "proposed": plan
                .proposed
                .iter()
                .map(|proposal| proposed_json(proposal, &by_id, &names))
                .collect::<Vec<_>>(),
            "skipped": plan
                .skipped
                .iter()
                .map(|skipped| skipped_json(skipped, &by_id))
                .collect::<Vec<_>>(),
            // The batch's own figures, stated rather than left to a client
            // adding up the list it was just given.
            "suggested": plan.proposed.len(),
            "considered": plan.proposed.len() + plan.skipped.len(),
        }
    })))
}

/// One suggestion, as the claim a person recognises plus the word being
/// offered for it.
fn proposed_json(
    proposal: &CategoryProposal,
    claims: &HashMap<&str, &Expense>,
    names: &HashMap<&str, &str>,
) -> Value {
    let claim = claims.get(proposal.expense_id.as_str());
    json!({
        "id": proposal.expense_id.as_str(),
        "merchant": claim.map(|claim| claim.merchant.clone()),
        "spentOn": claim.map(|claim| iso_date(claim.spent_on)),
        "grossCents": claim.map(|claim| claim.gross_cents),
        "currency": claim.map(|claim| claim.currency.clone()),
        "categoryId": proposal.category_id.as_str(),
        "categoryName": names.get(proposal.category_id.as_str()),
        "reason": proposal.reason,
        // How many of this person's own past claims back it: the argument for
        // the suggestion, which is the only reason to show one at all.
        "evidence": proposal.evidence,
    })
}

/// One claim nothing was suggested for, and why.
fn skipped_json(skipped: &SkippedClaim, claims: &HashMap<&str, &Expense>) -> Value {
    let claim = claims.get(skipped.expense_id.as_str());
    json!({
        "id": skipped.expense_id.as_str(),
        "merchant": claim.map(|claim| claim.merchant.clone()),
        "spentOn": claim.map(|claim| iso_date(claim.spent_on)),
        "reason": skipped.reason,
    })
}

/// The period to look at: what the proposal states, or the last `default_days`
/// days ending today.
///
/// `today` is passed in rather than read here so the rule is testable without a
/// clock, and `default_days` is passed in because what an unstated period means
/// is the *tool's* decision — a quarter of one's own claims to tidy up, a year
/// of the books to look over — while what a stated one is allowed to be is the
/// agent's, and belongs in one place ([`MAX_PERIOD_DAYS`]).
///
/// Both bounds are plain days in the reader's own reading of the calendar, as
/// everywhere on the finance surface.
///
/// # Errors
/// `422` when a bound is not a plain `YYYY-MM-DD`, when the period runs
/// backwards, or when it is longer than [`MAX_PERIOD_DAYS`].
pub(crate) fn period(
    args: &Value,
    today: Date,
    default_days: i64,
) -> Result<(Date, Date), Problem> {
    let to = match string_arg(args, "to") {
        None => today,
        Some(stated) => parse_iso_date(&stated).ok_or_else(|| {
            unprocessable("the end of the period must be a date written YYYY-MM-DD")
        })?,
    };
    let from = match string_arg(args, "from") {
        None => to - Duration::days(default_days - 1),
        Some(stated) => parse_iso_date(&stated).ok_or_else(|| {
            unprocessable("the start of the period must be a date written YYYY-MM-DD")
        })?,
    };
    if to < from {
        return Err(unprocessable(
            "the end of the period must not be before its start",
        ));
    }
    if (to - from).whole_days() >= MAX_PERIOD_DAYS {
        return Err(unprocessable(format!(
            "the period must be shorter than {MAX_PERIOD_DAYS} days"
        )));
    }
    Ok((from, to))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use time::Month;

    fn day(month: Month, day: u8) -> Date {
        Date::from_calendar_date(2026, month, day).expect("a real day")
    }

    #[test]
    fn an_unstated_period_is_the_quarter_up_to_today() {
        let today = day(Month::August, 10);
        let (from, to) = period(&json!({}), today, DEFAULT_PERIOD_DAYS).unwrap();
        assert_eq!(to, today);
        // Both ends included, so the span is one day shorter than the count.
        assert_eq!((to - from).whole_days(), DEFAULT_PERIOD_DAYS - 1);
    }

    #[test]
    fn a_stated_end_moves_the_whole_window_with_it() {
        let (from, to) = period(
            &json!({ "to": "2026-06-30" }),
            day(Month::August, 10),
            DEFAULT_PERIOD_DAYS,
        )
        .unwrap();
        assert_eq!(to, day(Month::June, 30));
        assert_eq!((to - from).whole_days(), DEFAULT_PERIOD_DAYS - 1);
    }

    #[test]
    fn both_bounds_are_taken_as_stated() {
        let (from, to) = period(
            &json!({ "from": " 2026-07-01 ", "to": "2026-07-31" }),
            day(Month::August, 10),
            DEFAULT_PERIOD_DAYS,
        )
        .unwrap();
        assert_eq!(from, day(Month::July, 1));
        assert_eq!(to, day(Month::July, 31));
    }

    #[test]
    fn a_bound_that_is_not_a_plain_day_is_refused_rather_than_guessed() {
        for bad in [
            json!({ "from": "yesterday" }),
            json!({ "from": "01/07/2026" }),
            json!({ "to": "2026-07-01T00:00:00Z" }),
            json!({ "to": "2026-13-01" }),
        ] {
            let problem =
                period(&bad, day(Month::August, 10), DEFAULT_PERIOD_DAYS).expect_err("accepted");
            assert_eq!(problem.status, axum::http::StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[test]
    fn a_backwards_or_endless_period_is_refused() {
        let backwards = period(
            &json!({ "from": "2026-08-01", "to": "2026-07-01" }),
            day(Month::August, 10),
            DEFAULT_PERIOD_DAYS,
        )
        .expect_err("accepted a backwards period");
        assert!(
            backwards
                .detail
                .as_deref()
                .unwrap_or_default()
                .contains("before its start")
        );

        let endless = period(
            &json!({ "from": "2024-01-01", "to": "2026-08-01" }),
            day(Month::August, 10),
            DEFAULT_PERIOD_DAYS,
        )
        .expect_err("accepted an endless period");
        assert!(
            endless
                .detail
                .as_deref()
                .unwrap_or_default()
                .contains("shorter than 366 days")
        );

        // Exactly the ceiling is allowed: 366 days, both ends included.
        let ok = period(
            &json!({ "from": "2026-01-01", "to": "2026-12-31" }),
            day(Month::August, 10),
            DEFAULT_PERIOD_DAYS,
        );
        assert!(ok.is_ok(), "a whole year must fit");
    }

    #[test]
    fn a_category_the_model_states_reaches_nothing() {
        // The rule the module header states, as code: the args are a period and
        // nothing else, so a category smuggled into them changes no outcome.
        let with = json!({ "from": "2026-07-01", "to": "2026-07-31", "category": "Travel" });
        let without = json!({ "from": "2026-07-01", "to": "2026-07-31" });
        assert_eq!(
            period(&with, day(Month::August, 10), DEFAULT_PERIOD_DAYS).unwrap(),
            period(&without, day(Month::August, 10), DEFAULT_PERIOD_DAYS).unwrap()
        );
    }
}
