//! alo Finance (ADR 0035, wave B4.11a): **the profit and loss** — what the
//! business earned and spent over a period, and the same period a year's
//! length ago beside it (`docs/design/finance.md`, "The four reports").
//!
//! It is a **fold over the journal and nothing else**. There is no query in
//! this module: it asks [`AccountStore::fin_trial_balance`] twice — once for
//! the period, once for the comparative — and adds up the two account types
//! that belong to a result. That is the whole point of having one journal: a
//! P&L that read invoices would agree with the ledger until the first manual
//! entry, and then quietly stop.
//!
//! Four things a reader should know before reading a figure this module
//! returns.
//!
//! **The signs are an accountant's, not the ledger's.** In the journal a debit
//! is positive, so income sits there as a negative number; on a P&L revenue of
//! a thousand euro is `100_000`, cost of a hundred is `10_000`, and the result
//! is income less expense. The flip happens once, in [`natural_cents`], so a
//! screen, a CSV and a later report cannot each invent their own convention.
//!
//! **The comparative period is derived, never asked for.** It is the period of
//! the same length immediately before this one ([`comparative_period`]): the
//! quarter before a quarter, the year before a year, yesterday beside today.
//! *Rejected: a second pair of dates on the request* — every caller would have
//! to compute the obvious answer, three of them would compute it differently,
//! and the one that got it wrong would present two periods of unequal length
//! side by side as though the difference meant something.
//!
//! **Every amount is in the tenant's accounting currency**, for
//! [`crate::fin_ledger`]'s reason: a total that adds dollars to euro means
//! nothing. [`ProfitAndLoss::currency`] says which one, because a figure a
//! human copies has to name its unit.
//!
//! **A line appears when either period moved it.** An account that earned
//! nothing this quarter and ten thousand last quarter is exactly the line a
//! comparative exists to show; dropping it because the current period is
//! silent would hide the fall. Accounts that never moved in either period are
//! absent rather than listed at zero — a hundred-line chart printed in full is
//! a page nobody reads.
//!
//! Tenancy is structural, as everywhere in this crate: both folds are over
//! reads that carry `tenant_id` from the handle, so another tenant's postings
//! are never read into a total rather than filtered out of one.

use std::collections::BTreeMap;

use time::{Date, Duration};

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::fin_accounts::AccountType;
use crate::fin_ledger::TrialBalance;
use crate::id::FinAccountId;

/// One account's line of a P&L: what it did in the period, and what the same
/// account did in the comparative one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlLine {
    /// The account.
    pub account_id: FinAccountId,
    /// The code an accountant types, and what the report sorts by.
    pub code: String,
    /// What the account is called, as this tenant renamed it. Taken from the
    /// current period when both periods have it, so a rename shows up once.
    pub name: String,
    /// [`AccountType::Income`] or [`AccountType::Expense`] — never one of the
    /// three that belong to a balance sheet.
    pub kind: AccountType,
    /// The period's movement, in the accounting currency, with an
    /// accountant's sign: revenue and cost are both positive.
    pub amount_cents: i64,
    /// The same account over the comparative period, on the same convention.
    /// Zero when it did not move then — which reads correctly, because it
    /// earned nothing.
    pub previous_cents: i64,
    /// How many postings the **current** period put on it. Zero on a line that
    /// only the comparative period moved, which is what says so.
    pub postings: i64,
}

/// A period's result, its lines, and the period before it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfitAndLoss {
    /// The inclusive first day asked for.
    pub from: Date,
    /// The inclusive last day asked for.
    pub to: Date,
    /// The first day of the comparative period ([`comparative_period`]).
    pub previous_from: Date,
    /// Its last day — always the day before [`Self::from`].
    pub previous_to: Date,
    /// The accounting currency every figure below is in.
    pub currency: String,
    /// The income accounts that moved, in code order.
    pub income: Vec<PlLine>,
    /// The expense accounts that moved, in code order.
    pub expense: Vec<PlLine>,
    /// What was earned: the sum of [`Self::income`].
    pub income_cents: i64,
    /// What was spent: the sum of [`Self::expense`].
    pub expense_cents: i64,
    /// Income less expense — a profit is positive, a loss negative.
    pub result_cents: i64,
    /// What was earned in the comparative period.
    pub previous_income_cents: i64,
    /// What was spent in it.
    pub previous_expense_cents: i64,
    /// Its result, on the same convention.
    pub previous_result_cents: i64,
}

/// The period of the same length immediately before this one: the quarter
/// before a quarter, the year before a year.
///
/// Both bounds are inclusive, so a one-day period compares against the day
/// before, and the comparative always ends on the day before `from` —
/// the two periods touch and never overlap.
///
/// Calendar length is deliberately *not* used: February against January is
/// twenty-eight days against thirty-one, and the alternative ("the same
/// calendar month a year ago") is a different report rather than a nuance.
/// The length is exactly the number of days asked for, so a February compares
/// against the twenty-eight days that preceded it.
///
/// A period so close to the beginning of the calendar that there is no room
/// before it clamps rather than panicking — that date cannot carry postings,
/// so the comparative is empty either way.
#[must_use]
pub fn comparative_period(from: Date, to: Date) -> (Date, Date) {
    let days = (to - from).whole_days().max(0);
    let previous_to = from.previous_day().unwrap_or(from);
    let previous_from = previous_to
        .checked_sub(Duration::days(days))
        .unwrap_or(previous_to);
    (previous_from, previous_to)
}

/// A ledger balance as a P&L reads it: income credit-positive, expense
/// debit-positive, so both sides of the report are positive numbers and the
/// result is one subtraction.
///
/// The one place the sign is flipped. `saturating_neg` rather than `-` because
/// `i64::MIN` has no positive, and a report that panicked on a corrupt figure
/// would be worse than one that showed its ceiling.
fn natural_cents(kind: AccountType, balance_cents: i64) -> i64 {
    match kind {
        AccountType::Income => balance_cents.saturating_neg(),
        _ => balance_cents,
    }
}

/// Builds the report from the two trial balances — pure, so every figure below
/// is unit-tested without a database.
///
/// The totals are summed from the lines rather than taken from
/// [`TrialBalance::net_of`]: a total that does not add up to the page under it
/// is the one defect a financial report must not have, and summing the same
/// vector the caller prints makes that impossible rather than unlikely.
fn fold(
    from: Date,
    to: Date,
    previous_from: Date,
    previous_to: Date,
    currency: String,
    current: &TrialBalance,
    previous: &TrialBalance,
) -> ProfitAndLoss {
    // Keyed by (code, id) so the order is the trial balance's own — code
    // first, and the id only to keep two accounts sharing a code apart.
    let mut lines: BTreeMap<(String, String), PlLine> = BTreeMap::new();
    for account in current
        .accounts
        .iter()
        .filter(|account| !account.kind.is_balance_sheet())
    {
        lines.insert(
            (account.code.clone(), account.account_id.as_str().to_owned()),
            PlLine {
                account_id: account.account_id.clone(),
                code: account.code.clone(),
                name: account.name.clone(),
                kind: account.kind,
                amount_cents: natural_cents(account.kind, account.balance_cents),
                previous_cents: 0,
                postings: account.postings,
            },
        );
    }
    for account in previous
        .accounts
        .iter()
        .filter(|account| !account.kind.is_balance_sheet())
    {
        let line = lines
            .entry((account.code.clone(), account.account_id.as_str().to_owned()))
            .or_insert_with(|| PlLine {
                account_id: account.account_id.clone(),
                code: account.code.clone(),
                name: account.name.clone(),
                kind: account.kind,
                amount_cents: 0,
                previous_cents: 0,
                postings: 0,
            });
        line.previous_cents = natural_cents(account.kind, account.balance_cents);
    }

    let (income, expense): (Vec<PlLine>, Vec<PlLine>) = lines
        .into_values()
        .partition(|line| line.kind == AccountType::Income);

    let income_cents = total(&income, |line| line.amount_cents);
    let expense_cents = total(&expense, |line| line.amount_cents);
    let previous_income_cents = total(&income, |line| line.previous_cents);
    let previous_expense_cents = total(&expense, |line| line.previous_cents);
    ProfitAndLoss {
        from,
        to,
        previous_from,
        previous_to,
        currency,
        income,
        expense,
        income_cents,
        expense_cents,
        result_cents: income_cents.saturating_sub(expense_cents),
        previous_income_cents,
        previous_expense_cents,
        previous_result_cents: previous_income_cents.saturating_sub(previous_expense_cents),
    }
}

/// One column of a side, added up. Saturating for
/// [`natural_cents`]' reason — the journal's own ceilings leave four orders of
/// magnitude of headroom, and a wrapped total is the one number a report must
/// never print.
fn total(lines: &[PlLine], of: fn(&PlLine) -> i64) -> i64 {
    lines
        .iter()
        .map(of)
        .fold(0_i64, |sum, cents| sum.saturating_add(cents))
}

impl AccountStore {
    /// **The profit and loss** for a period, with the period of the same length
    /// before it beside every figure.
    ///
    /// Both bounds are inclusive and judged on each entry's accounting date, so
    /// re-running last quarter next year answers last quarter. Two folds over
    /// [`AccountStore::fin_trial_balance`] and one read of the tenant's
    /// accounting currency — no query of its own, which is what keeps this
    /// report and the ledger it is a summary of incapable of disagreeing.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the period ends before it starts, or
    /// when a stored account type is one this build does not know;
    /// [`StoreError::Db`] on failure.
    pub async fn fin_profit_and_loss(&self, from: Date, to: Date) -> Result<ProfitAndLoss> {
        if to < from {
            return Err(StoreError::Validation(
                "the end of the period must not be before its start".to_owned(),
            ));
        }
        let (previous_from, previous_to) = comparative_period(from, to);
        let currency = self.billing_base_currency().await?;
        let current = self.fin_trial_balance(Some(from), Some(to)).await?;
        let previous = self
            .fin_trial_balance(Some(previous_from), Some(previous_to))
            .await?;
        Ok(fold(
            from,
            to,
            previous_from,
            previous_to,
            currency,
            &current,
            &previous,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fin_ledger::AccountBalance;
    use time::Month;

    fn on(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).unwrap_or_else(|e| panic!("{e}"))
    }

    /// An account that moved `balance_cents` in the ledger's own convention:
    /// positive is a debit, so income arrives here negative.
    fn moved(code: &str, kind: AccountType, balance_cents: i64) -> AccountBalance {
        AccountBalance {
            account_id: FinAccountId::new(format!("acc-{code}")),
            code: code.to_owned(),
            name: format!("Account {code}"),
            kind,
            role: None,
            debit_cents: balance_cents.max(0),
            credit_cents: (-balance_cents).max(0),
            balance_cents,
            postings: 2,
        }
    }

    fn trial(from: Date, to: Date, accounts: Vec<AccountBalance>) -> TrialBalance {
        let debit_cents = accounts.iter().map(|a| a.debit_cents).sum();
        let credit_cents = accounts.iter().map(|a| a.credit_cents).sum();
        TrialBalance {
            from: Some(from),
            to: Some(to),
            accounts,
            debit_cents,
            credit_cents,
        }
    }

    /// A quarter of a small consultancy, and the quarter before it, folded the
    /// way the store folds them.
    fn quarter() -> ProfitAndLoss {
        let from = on(2026, Month::July, 1);
        let to = on(2026, Month::September, 30);
        let (previous_from, previous_to) = comparative_period(from, to);
        let current = trial(
            from,
            to,
            vec![
                // The balance sheet's three types are in a trial balance and
                // must not reach a P&L.
                moved("1100", AccountType::Asset, 151_877),
                moved("2100", AccountType::Liability, -23_880),
                moved("4000", AccountType::Income, -127_997),
                moved("4900", AccountType::Income, -2_000),
                moved("6000", AccountType::Expense, 40_000),
                moved("6100", AccountType::Expense, 12_500),
            ],
        );
        let previous = trial(
            previous_from,
            previous_to,
            vec![
                moved("4000", AccountType::Income, -100_000),
                moved("6000", AccountType::Expense, 35_000),
                // Moved last quarter, silent this one — the line a comparative
                // exists to show.
                moved("6200", AccountType::Expense, 9_900),
            ],
        );
        fold(
            from,
            to,
            previous_from,
            previous_to,
            "EUR".to_owned(),
            &current,
            &previous,
        )
    }

    #[test]
    fn the_comparative_is_the_period_of_the_same_length_that_ends_the_day_before() {
        // A quarter.
        assert_eq!(
            comparative_period(on(2026, Month::July, 1), on(2026, Month::September, 30)),
            (on(2026, Month::March, 31), on(2026, Month::June, 30)),
            "ninety-two days, ending the day before the period starts"
        );
        // A calendar year against the year before it.
        assert_eq!(
            comparative_period(on(2026, Month::January, 1), on(2026, Month::December, 31)),
            (on(2025, Month::January, 1), on(2025, Month::December, 31))
        );
        // A single day against the day before.
        assert_eq!(
            comparative_period(on(2026, Month::March, 1), on(2026, Month::March, 1)),
            (on(2026, Month::February, 28), on(2026, Month::February, 28))
        );
        // February, whose length is its own and not January's.
        let (previous_from, previous_to) =
            comparative_period(on(2026, Month::February, 1), on(2026, Month::February, 28));
        assert_eq!(previous_to, on(2026, Month::January, 31));
        assert_eq!(
            previous_from,
            on(2026, Month::January, 4),
            "twenty-eight days"
        );
    }

    #[test]
    fn a_leap_day_is_a_day_like_any_other_in_the_length() {
        // 2028 is a leap year: the first quarter is ninety-one days, so its
        // comparative reaches one day further back than a common year's does.
        assert_eq!(
            comparative_period(on(2028, Month::January, 1), on(2028, Month::March, 31)),
            (on(2027, Month::October, 2), on(2027, Month::December, 31))
        );
    }

    #[test]
    fn the_beginning_of_the_calendar_clamps_rather_than_panicking() {
        let (previous_from, previous_to) = comparative_period(Date::MIN, Date::MIN);
        assert_eq!(previous_from, Date::MIN);
        assert_eq!(previous_to, Date::MIN);
    }

    #[test]
    fn income_reads_positive_expense_reads_positive_and_the_result_is_the_difference() {
        let report = quarter();
        assert_eq!(report.income_cents, 129_997, "127_997 + 2_000, credit-side");
        assert_eq!(report.expense_cents, 52_500, "40_000 + 12_500, debit-side");
        assert_eq!(report.result_cents, 77_497);
        // The totals are the lines added up, not a second opinion about them.
        assert_eq!(
            report.income.iter().map(|l| l.amount_cents).sum::<i64>(),
            report.income_cents
        );
        assert_eq!(
            report.expense.iter().map(|l| l.amount_cents).sum::<i64>(),
            report.expense_cents
        );
    }

    #[test]
    fn a_balance_sheet_account_never_reaches_a_profit_and_loss() {
        let report = quarter();
        for line in report.income.iter().chain(&report.expense) {
            assert!(
                !line.kind.is_balance_sheet(),
                "{} is a {:?}",
                line.code,
                line.kind
            );
        }
        assert_eq!(
            report.income.len() + report.expense.len(),
            5,
            "two income, three expense — the receivable and the VAT are not a result"
        );
    }

    #[test]
    fn a_line_the_current_period_never_touched_still_shows_what_it_did_before() {
        let report = quarter();
        let silent = report
            .expense
            .iter()
            .find(|line| line.code == "6200")
            .unwrap_or_else(|| panic!("6200 is in the comparative"));
        assert_eq!(silent.amount_cents, 0);
        assert_eq!(silent.previous_cents, 9_900);
        assert_eq!(
            silent.postings, 0,
            "no posting this period, which is what says the zero is real"
        );
        // And it is counted in the comparative's total, not the current one.
        assert_eq!(report.previous_expense_cents, 44_900);
        assert_eq!(report.previous_income_cents, 100_000);
        assert_eq!(report.previous_result_cents, 55_100);
        assert_eq!(report.expense_cents, 52_500);
    }

    #[test]
    fn every_side_is_in_code_order() {
        let report = quarter();
        let codes: Vec<&str> = report.income.iter().map(|l| l.code.as_str()).collect();
        assert_eq!(codes, ["4000", "4900"]);
        let codes: Vec<&str> = report.expense.iter().map(|l| l.code.as_str()).collect();
        assert_eq!(codes, ["6000", "6100", "6200"]);
    }

    #[test]
    fn a_period_that_moved_nothing_is_a_report_of_zeroes_not_an_absence() {
        let from = on(2026, Month::January, 1);
        let to = on(2026, Month::March, 31);
        let (previous_from, previous_to) = comparative_period(from, to);
        let report = fold(
            from,
            to,
            previous_from,
            previous_to,
            "EUR".to_owned(),
            &trial(from, to, Vec::new()),
            &trial(previous_from, previous_to, Vec::new()),
        );
        assert!(report.income.is_empty() && report.expense.is_empty());
        assert_eq!(report.income_cents, 0);
        assert_eq!(report.expense_cents, 0);
        assert_eq!(report.result_cents, 0);
        assert_eq!(report.previous_result_cents, 0);
        assert_eq!(report.currency, "EUR");
    }

    #[test]
    fn a_loss_is_a_negative_result() {
        let from = on(2026, Month::July, 1);
        let to = on(2026, Month::July, 31);
        let (previous_from, previous_to) = comparative_period(from, to);
        let report = fold(
            from,
            to,
            previous_from,
            previous_to,
            "EUR".to_owned(),
            &trial(
                from,
                to,
                vec![
                    moved("4000", AccountType::Income, -10_000),
                    moved("6000", AccountType::Expense, 25_000),
                ],
            ),
            &trial(previous_from, previous_to, Vec::new()),
        );
        assert_eq!(report.result_cents, -15_000);
    }

    #[test]
    fn a_renamed_account_reads_under_the_name_it_has_now() {
        let from = on(2026, Month::July, 1);
        let to = on(2026, Month::July, 31);
        let (previous_from, previous_to) = comparative_period(from, to);
        let mut older = moved("4000", AccountType::Income, -10_000);
        older.name = "Turnover".to_owned();
        let report = fold(
            from,
            to,
            previous_from,
            previous_to,
            "EUR".to_owned(),
            &trial(from, to, vec![moved("4000", AccountType::Income, -20_000)]),
            &trial(previous_from, previous_to, vec![older]),
        );
        assert_eq!(report.income.len(), 1, "one account, not two");
        assert_eq!(report.income[0].name, "Account 4000");
        assert_eq!(report.income[0].amount_cents, 20_000);
        assert_eq!(report.income[0].previous_cents, 10_000);
    }

    #[test]
    fn the_sign_is_flipped_in_exactly_one_place_and_cannot_wrap() {
        assert_eq!(natural_cents(AccountType::Income, -127_997), 127_997);
        assert_eq!(natural_cents(AccountType::Expense, 40_000), 40_000);
        // A credit on an expense account (a refund) stays negative, which is
        // what a cost that came back is.
        assert_eq!(natural_cents(AccountType::Expense, -1_000), -1_000);
        assert_eq!(natural_cents(AccountType::Income, i64::MIN), i64::MAX);
    }
}
