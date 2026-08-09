//! alo Finance (ADR 0035, wave B4.11b): **the balance sheet** — what the
//! business owns, what it owes, and what is left over, on one day
//! (`docs/design/finance.md`, "The four reports").
//!
//! Like [`crate::fin_pl`] it is a **fold over the journal and nothing else**,
//! and it holds no query: it asks [`AccountStore::fin_trial_balance`] once, with
//! no lower bound, and splits the answer into the three types that stand on a
//! balance sheet plus the two that make the result standing on it. One journal
//! is the whole reason the two reports cannot disagree — the same postings, read
//! twice.
//!
//! Four things a reader should know before reading a figure this module returns.
//!
//! **A balance sheet is cumulative, not a period.** There is one date, and every
//! posting on or before it counts, back to the day the books opened. A quarter's
//! movement of a bank account is a ledger question ([`crate::fin_ledger`]); what
//! is *in* the bank account on the thirty-first of December is this one.
//!
//! **The signs are an accountant's.** In the journal a debit is positive, so
//! what the business owes sits there as a negative number; on a balance sheet a
//! liability of a thousand euro is `100_000` and reads on the side it belongs
//! to. The flip happens once, in [`natural_cents`], so a screen, a CSV and a
//! later report cannot each invent their own convention — the rule
//! [`crate::fin_pl`] states for the other two account types.
//!
//! **The result is on the sheet because nothing has closed it into equity.**
//! alo writes no year-end closing entry (`docs/design/finance.md` — a close is
//! about *writes*, not about moving balances), so income less expense to the
//! date is carried as its own figure, [`BalanceSheet::result_cents`], beside
//! equity rather than inside it. That is what makes the sheet balance, and it is
//! also honest: an accountant who wants it inside equity books the entry, and
//! then it is inside equity here too, because this is the journal added up.
//!
//! **It must balance, and the report says whether it does.** P10:
//! `assets = liabilities + equity + result`. It is arithmetic rather than luck —
//! every entry balances in the base column, so any sum of whole entries does —
//! which is exactly why [`BalanceSheet::difference_cents`] is carried rather
//! than assumed away: a non-zero difference means postings were written by
//! something other than [`AccountStore::post_fin_entry`], and a report that
//! quietly printed it would look precisely like a correct one.
//!
//! Tenancy is structural, as everywhere in this crate: the fold is over a read
//! that carries `tenant_id` from the handle, so another tenant's postings are
//! never read into a total rather than filtered out of one.

use time::Date;

use crate::account::AccountStore;
use crate::error::Result;
use crate::fin_accounts::{AccountRole, AccountType};
use crate::fin_ledger::TrialBalance;
use crate::id::FinAccountId;

/// One account's line of a balance sheet: what it holds on the date.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BalanceLine {
    /// The account.
    pub account_id: FinAccountId,
    /// The code an accountant types, and what the report sorts by.
    pub code: String,
    /// What the account is called, as this tenant renamed it.
    pub name: String,
    /// [`AccountType::Asset`], [`AccountType::Liability`] or
    /// [`AccountType::Equity`] — never one of the two that make a result.
    pub kind: AccountType,
    /// The posting-rule job it does, if any — what lets a screen show the bank
    /// and the receivables apart without reading their codes.
    pub role: Option<AccountRole>,
    /// The balance on the date, in the accounting currency, with an
    /// accountant's sign: an asset and a liability are both positive.
    pub amount_cents: i64,
    /// How many postings there have ever been on it, up to the date. An account
    /// with none is not on the sheet at all, so this is never zero.
    pub postings: i64,
}

/// What the business owns, owes, and is left with, on one day.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BalanceSheet {
    /// The day asked for, inclusive: every posting on or before it counts.
    pub on: Date,
    /// The accounting currency every figure below is in.
    pub currency: String,
    /// The asset accounts with a balance, in code order.
    pub assets: Vec<BalanceLine>,
    /// The liability accounts with a balance, in code order.
    pub liabilities: Vec<BalanceLine>,
    /// The equity accounts with a balance, in code order.
    pub equity: Vec<BalanceLine>,
    /// What is owned: the sum of [`Self::assets`].
    pub asset_cents: i64,
    /// What is owed: the sum of [`Self::liabilities`].
    pub liability_cents: i64,
    /// The owners' stake as booked: the sum of [`Self::equity`].
    pub equity_cents: i64,
    /// Income less expense since the books opened, up to the date — the result
    /// no closing entry has moved into equity. A profit is positive.
    pub result_cents: i64,
    /// [`Self::liability_cents`] + [`Self::equity_cents`] +
    /// [`Self::result_cents`]: the side that must equal [`Self::asset_cents`].
    pub liability_equity_cents: i64,
    /// [`Self::asset_cents`] less [`Self::liability_equity_cents`] — zero on
    /// every honest sheet, and stated rather than assumed because the figure a
    /// broken one prints looks exactly like a real one.
    pub difference_cents: i64,
}

impl BalanceSheet {
    /// Whether the sheet balances (P10). Always `true` on books written through
    /// [`AccountStore::post_fin_entry`]; a `false` is a defect to surface, never
    /// to round away.
    pub fn balances(&self) -> bool {
        self.difference_cents == 0
    }
}

/// A ledger balance as a balance sheet reads it: assets debit-positive,
/// liabilities and equity credit-positive, income credit-positive, expense
/// debit-positive — so every side of the report is a positive number and the
/// result is one subtraction.
///
/// The one place the sign is flipped. `saturating_neg` rather than `-` because
/// `i64::MIN` has no positive, and a report that panicked on a corrupt figure
/// would be worse than one that showed its ceiling.
fn natural_cents(kind: AccountType, balance_cents: i64) -> i64 {
    match kind {
        AccountType::Asset | AccountType::Expense => balance_cents,
        AccountType::Liability | AccountType::Equity | AccountType::Income => {
            balance_cents.saturating_neg()
        }
    }
}

/// One side of the sheet, added up. Saturating for [`natural_cents`]' reason —
/// the journal's own ceilings leave four orders of magnitude of headroom, and a
/// wrapped total is the one number a report must never print.
fn total(lines: &[BalanceLine]) -> i64 {
    lines
        .iter()
        .map(|line| line.amount_cents)
        .fold(0_i64, |sum, cents| sum.saturating_add(cents))
}

/// Builds the sheet from the cumulative trial balance — pure, so every figure
/// below is unit-tested without a database.
///
/// The totals are summed from the lines rather than taken from
/// [`TrialBalance::net_of`]: a total that does not add up to the page under it
/// is the one defect a financial report must not have, and summing the same
/// vectors the caller prints makes that impossible rather than unlikely.
fn fold(on: Date, currency: String, balance: &TrialBalance) -> BalanceSheet {
    let mut assets = Vec::new();
    let mut liabilities = Vec::new();
    let mut equity = Vec::new();
    // The result is folded from the same pass, so an income account can never be
    // counted on one page and missed on the other.
    let mut result_cents = 0_i64;
    // The trial balance is already in code order, and staying in its order is
    // what keeps three reports sorting a chart the same way.
    for account in &balance.accounts {
        let amount_cents = natural_cents(account.kind, account.balance_cents);
        let line = BalanceLine {
            account_id: account.account_id.clone(),
            code: account.code.clone(),
            name: account.name.clone(),
            kind: account.kind,
            role: account.role,
            amount_cents,
            postings: account.postings,
        };
        match account.kind {
            AccountType::Asset => assets.push(line),
            AccountType::Liability => liabilities.push(line),
            AccountType::Equity => equity.push(line),
            AccountType::Income => result_cents = result_cents.saturating_add(amount_cents),
            AccountType::Expense => result_cents = result_cents.saturating_sub(amount_cents),
        }
    }

    let asset_cents = total(&assets);
    let liability_cents = total(&liabilities);
    let equity_cents = total(&equity);
    let liability_equity_cents = liability_cents
        .saturating_add(equity_cents)
        .saturating_add(result_cents);
    BalanceSheet {
        on,
        currency,
        assets,
        liabilities,
        equity,
        asset_cents,
        liability_cents,
        equity_cents,
        result_cents,
        liability_equity_cents,
        difference_cents: asset_cents.saturating_sub(liability_equity_cents),
    }
}

impl AccountStore {
    /// **The balance sheet** on one day: what the tenant owns, what it owes,
    /// its equity, and the result no closing entry has moved into equity.
    ///
    /// The date is inclusive and there is no lower bound — a balance sheet is
    /// cumulative by definition, so re-running last year-end next year answers
    /// last year-end. One fold over [`AccountStore::fin_trial_balance`] and one
    /// read of the tenant's accounting currency; no query of its own, which is
    /// what keeps this report and the P&L it shares a journal with incapable of
    /// disagreeing.
    ///
    /// The sheet balances (P10) and says so — see
    /// [`BalanceSheet::difference_cents`].
    ///
    /// # Errors
    /// [`crate::StoreError::Validation`] when a stored account type or role is
    /// one this build does not know; [`crate::StoreError::Db`] on failure.
    pub async fn fin_balance_sheet(&self, on: Date) -> Result<BalanceSheet> {
        let currency = self.billing_base_currency().await?;
        let balance = self.fin_trial_balance(None, Some(on)).await?;
        Ok(fold(on, currency, &balance))
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

    /// An account holding `balance_cents` in the ledger's own convention:
    /// positive is a debit, so a liability arrives here negative.
    fn held(code: &str, kind: AccountType, balance_cents: i64) -> AccountBalance {
        AccountBalance {
            account_id: FinAccountId::new(format!("acc-{code}")),
            code: code.to_owned(),
            name: format!("Account {code}"),
            kind,
            role: None,
            debit_cents: balance_cents.max(0),
            credit_cents: (-balance_cents).max(0),
            balance_cents,
            postings: 4,
        }
    }

    fn trial(to: Date, accounts: Vec<AccountBalance>) -> TrialBalance {
        let debit_cents = accounts.iter().map(|a| a.debit_cents).sum();
        let credit_cents = accounts.iter().map(|a| a.credit_cents).sum();
        TrialBalance {
            from: None,
            to: Some(to),
            accounts,
            debit_cents,
            credit_cents,
        }
    }

    /// A small consultancy's books at a year end, folded the way the store folds
    /// them. Every entry behind it balanced, so the trial balance nets to zero.
    fn year_end() -> BalanceSheet {
        let date = on(2026, Month::December, 31);
        fold(
            date,
            "EUR".to_owned(),
            &trial(
                date,
                vec![
                    held("1000", AccountType::Asset, 121_000),
                    held("1100", AccountType::Asset, 50_000),
                    held("2000", AccountType::Liability, -22_500),
                    held("2100", AccountType::Liability, -21_000),
                    held("3000", AccountType::Equity, -10_000),
                    held("4000", AccountType::Income, -160_000),
                    held("6000", AccountType::Expense, 42_500),
                ],
            ),
        )
    }

    #[test]
    fn an_asset_and_a_liability_both_read_positive_on_the_side_they_belong_to() {
        let sheet = year_end();
        assert_eq!(sheet.asset_cents, 171_000, "121_000 + 50_000, debit-side");
        assert_eq!(
            sheet.liability_cents, 43_500,
            "22_500 + 21_000, credit-side and positive"
        );
        assert_eq!(sheet.equity_cents, 10_000);
        assert_eq!(sheet.result_cents, 117_500, "160_000 earned, 42_500 spent");
        // The totals are the lines added up, not a second opinion about them.
        assert_eq!(total(&sheet.assets), sheet.asset_cents);
        assert_eq!(total(&sheet.liabilities), sheet.liability_cents);
        assert_eq!(total(&sheet.equity), sheet.equity_cents);
    }

    #[test]
    fn the_sheet_balances_and_says_so() {
        let sheet = year_end();
        assert_eq!(
            sheet.liability_equity_cents, 171_000,
            "43_500 owed + 10_000 equity + 117_500 result"
        );
        assert_eq!(sheet.asset_cents, sheet.liability_equity_cents);
        assert_eq!(sheet.difference_cents, 0);
        assert!(sheet.balances());
    }

    #[test]
    fn a_result_account_is_never_a_line_but_is_always_in_the_result() {
        let sheet = year_end();
        for line in sheet
            .assets
            .iter()
            .chain(&sheet.liabilities)
            .chain(&sheet.equity)
        {
            assert!(
                line.kind.is_balance_sheet(),
                "{} is a {:?}",
                line.code,
                line.kind
            );
        }
        assert_eq!(
            sheet.assets.len() + sheet.liabilities.len() + sheet.equity.len(),
            5,
            "two assets, two liabilities, one equity — the sales and the hosting are not held"
        );
        assert_eq!(sheet.result_cents, 117_500);
    }

    #[test]
    fn every_side_is_in_the_charts_own_code_order() {
        let sheet = year_end();
        let codes: Vec<&str> = sheet.assets.iter().map(|l| l.code.as_str()).collect();
        assert_eq!(codes, ["1000", "1100"]);
        let codes: Vec<&str> = sheet.liabilities.iter().map(|l| l.code.as_str()).collect();
        assert_eq!(codes, ["2000", "2100"]);
        let codes: Vec<&str> = sheet.equity.iter().map(|l| l.code.as_str()).collect();
        assert_eq!(codes, ["3000"]);
    }

    #[test]
    fn a_loss_is_a_negative_result_and_the_sheet_still_balances() {
        let date = on(2026, Month::June, 30);
        let sheet = fold(
            date,
            "EUR".to_owned(),
            &trial(
                date,
                vec![
                    // Owner put in a thousand, spent two hundred and fifty of it.
                    held("1000", AccountType::Asset, 75_000),
                    held("3000", AccountType::Equity, -100_000),
                    held("6000", AccountType::Expense, 25_000),
                ],
            ),
        );
        assert_eq!(sheet.result_cents, -25_000);
        assert_eq!(sheet.asset_cents, 75_000);
        assert_eq!(sheet.liability_equity_cents, 75_000);
        assert!(sheet.balances());
    }

    #[test]
    fn an_overdrawn_bank_account_stays_negative_on_the_asset_side() {
        // A credit balance on an asset is what an overdraft is; it is not moved
        // to the other side, because the account is still the bank account.
        let date = on(2026, Month::June, 30);
        let sheet = fold(
            date,
            "EUR".to_owned(),
            &trial(
                date,
                vec![
                    held("1000", AccountType::Asset, -5_000),
                    held("2000", AccountType::Liability, 5_000),
                ],
            ),
        );
        assert_eq!(sheet.assets[0].amount_cents, -5_000);
        assert_eq!(
            sheet.liabilities[0].amount_cents, -5_000,
            "a debit balance on a payable reads negative for the same reason"
        );
        assert!(sheet.balances());
    }

    #[test]
    fn a_day_before_the_books_opened_is_a_sheet_of_zeroes_not_an_absence() {
        let date = on(2019, Month::December, 31);
        let sheet = fold(date, "EUR".to_owned(), &trial(date, Vec::new()));
        assert!(sheet.assets.is_empty() && sheet.liabilities.is_empty() && sheet.equity.is_empty());
        assert_eq!(sheet.asset_cents, 0);
        assert_eq!(sheet.liability_equity_cents, 0);
        assert_eq!(sheet.result_cents, 0);
        assert_eq!(sheet.currency, "EUR");
        assert_eq!(sheet.on, date);
        assert!(sheet.balances());
    }

    #[test]
    fn postings_written_by_something_other_than_the_journal_show_as_a_difference() {
        // A trial balance that does not net to zero cannot come out of
        // `post_fin_entry`; if one ever does, the sheet must say so rather than
        // present the shortfall as equity.
        let date = on(2026, Month::December, 31);
        let sheet = fold(
            date,
            "EUR".to_owned(),
            &trial(
                date,
                vec![
                    held("1000", AccountType::Asset, 100_000),
                    held("2000", AccountType::Liability, -60_000),
                ],
            ),
        );
        assert_eq!(sheet.difference_cents, 40_000);
        assert!(!sheet.balances());
    }

    #[test]
    fn the_sign_is_flipped_in_exactly_one_place_and_cannot_wrap() {
        assert_eq!(natural_cents(AccountType::Asset, 121_000), 121_000);
        assert_eq!(natural_cents(AccountType::Liability, -22_500), 22_500);
        assert_eq!(natural_cents(AccountType::Equity, -10_000), 10_000);
        assert_eq!(natural_cents(AccountType::Income, -160_000), 160_000);
        assert_eq!(natural_cents(AccountType::Expense, 42_500), 42_500);
        assert_eq!(natural_cents(AccountType::Liability, i64::MIN), i64::MAX);
    }

    #[test]
    fn a_role_travels_with_the_line_so_a_screen_need_not_read_codes() {
        let date = on(2026, Month::December, 31);
        let mut bank = held("1000", AccountType::Asset, 121_000);
        bank.role = Some(AccountRole::Bank);
        let sheet = fold(date, "EUR".to_owned(), &trial(date, vec![bank]));
        assert_eq!(sheet.assets[0].role, Some(AccountRole::Bank));
        assert_eq!(sheet.assets[0].postings, 4);
    }
}
