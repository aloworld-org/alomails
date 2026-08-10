//! alo Finance (ADR 0035, wave B4.11d): **the VAT-return figures** — the tax
//! charged on sales, the tax paid on purchases, and what is owed to the
//! authority as the difference (`docs/design/finance.md`, "The four reports").
//!
//! Like the P&L and the balance sheet, this file holds **no query**: it is four
//! folds over [`AccountStore::fin_dimension_balances`], grouped by
//! [`LedgerDimension::VatRate`]. That is the whole point of B4.04a's rule that
//! *the rate travels on the revenue posting too*, not only on the tax posting —
//! the taxable base per rate and the tax per rate come out of the same journal
//! the P&L and the balance sheet are folded from, so a return and the books are
//! provably one statement rather than two that agree today.
//!
//! Five things a reader should know before copying a figure off it.
//!
//! **The two sides are found by role and by type, never by code.** The tax is
//! whatever sits on the accounts doing the `vat_output` and `vat_input` jobs in
//! *this* tenant's chart; the base is whatever moved on their income accounts
//! (output) and expense accounts (input). A tenant who recodes their chart
//! changes neither. *Rejected: an account per rate* — the rate is a dimension on
//! the posting (`docs/design/finance.md`), and a chart that grows a line every
//! time a government moves a rate is a chart nobody can read a year later.
//!
//! **The signs are an accountant's, not the ledger's.** Output tax is a credit
//! and output turnover is a credit, so both arrive negative and the output side
//! is flipped once, in [`natural_cents`]; the input side is debit-positive and
//! is not flipped. A return therefore shows two positive columns and one
//! subtraction, which is the arithmetic the form asks for.
//!
//! **Only postings that state a rate are on the return.** A rate is what makes
//! a posting a taxable base, and revenue booked without one is not turnover at
//! a rate of nothing — it is turnover nobody attributed. It is *reported*
//! rather than dropped ([`VatReturnSide::unrated_base_cents`]), because a
//! return whose base is far below the period's turnover is a fact the filer has
//! to see; and tax sitting on a VAT account with no rate
//! ([`VatReturnSide::unrated_vat_cents`]) is a posting rule with a bug, which is
//! worth the same treatment.
//!
//! **Every amount is in the tenant's accounting currency**, for
//! [`crate::fin_ledger`]'s reason and for EU VAT Directive art. 91's: each
//! document was crossed at the rate frozen on it when it was booked, so
//! re-running last quarter answers last quarter. [`VatReturn::currency`] says
//! which currency, because a figure copied onto a form has to name its unit.
//!
//! **A period whose rates cannot all be read is refused, never half-reported.**
//! [`crate::fin_ledger::LEDGER_GROUPS_MAX`] caps a grouped read, and a VAT
//! return summed from a capped read would be a plausible wrong number on a legal
//! document. It cannot arise from books alo itself writes — a tenant bills at a
//! handful of rates — and if it ever does, the caller gets
//! [`StoreError::Validation`] naming the reason.
//!
//! **These are figures for a return, not a return** (ADR 0035's non-goal): alo
//! produces correct, exportable numbers, and filing goes through the national
//! portal. Nothing here knows about boxes, deadlines or reverse charge — see
//! that note's "Not built" list.
//!
//! Tenancy is structural, as everywhere in this crate: all four reads carry
//! `tenant_id` from the handle, so another tenant's postings are never read into
//! a total rather than filtered out of one.

use std::collections::BTreeMap;

use time::Date;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::fin_accounts::{AccountRole, AccountType};
use crate::fin_ledger::{DimensionBalances, LedgerDimension, LedgerScope};

/// What one VAT rate did on one side of the return.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VatReturnRate {
    /// The rate, in basis points (2100 = 21 %).
    pub rate_bp: i32,
    /// The taxable base at this rate — net turnover on the output side, net
    /// purchases on the input one, in the accounting currency.
    pub base_cents: i64,
    /// The tax itself at this rate, in the accounting currency.
    pub vat_cents: i64,
}

/// One side of the return: the rates, what they add up to, and what is *not* in
/// them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VatReturnSide {
    /// One row per rate that either the base or the tax used, ascending by
    /// rate. A rate that netted to zero over the period is still a row: it was
    /// used, and it netted out.
    pub rates: Vec<VatReturnRate>,
    /// The taxable base: the sum of [`Self::rates`], and nothing else.
    pub base_cents: i64,
    /// The tax: the sum of [`Self::rates`], and nothing else.
    pub vat_cents: i64,
    /// What moved on this side's base accounts **without** stating a rate — the
    /// part of the period's turnover (or cost) that is on no line of the
    /// return. Zero on books alo wrote by itself.
    pub unrated_base_cents: i64,
    /// Tax sitting on the VAT account with no rate on it. Always zero unless a
    /// posting rule forgot, which is exactly why it is reported rather than
    /// folded into a rate that did not charge it.
    pub unrated_vat_cents: i64,
}

/// A period's VAT figures: what was charged, what may be recovered, and the
/// difference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VatReturn {
    /// The inclusive first day asked for.
    pub from: Date,
    /// The inclusive last day asked for.
    pub to: Date,
    /// The accounting currency every figure below is in.
    pub currency: String,
    /// Tax charged on sales, and the turnover it was charged on.
    pub output: VatReturnSide,
    /// Tax paid on purchases, and the cost it was paid on — recoverable input
    /// tax, which alo states in full and never apportions (partial
    /// deductibility is `docs/design/finance.md`'s "Not built").
    pub input: VatReturnSide,
    /// Output tax less input tax: **positive is owed to the authority**, and a
    /// negative figure is a refund claim.
    pub net_payable_cents: i64,
}

/// What the return says when a grouped read hit its cap — a legal document is
/// refused rather than summed from part of the period.
const TOO_MANY_RATES: &str = "this period states more VAT rates than a return can be summed from; check the rates on the \
     documents in it";

/// A ledger balance as a return reads it: credit-side figures (output tax and
/// turnover) positive, debit-side ones (input tax and cost) unchanged.
///
/// The one place the sign is flipped, as [`crate::fin_pl`] flips its own once.
/// `saturating_neg` rather than `-` because `i64::MIN` has no positive, and a
/// report that panicked on a corrupt figure would be worse than one that showed
/// its ceiling.
fn natural_cents(credit_side: bool, balance_cents: i64) -> i64 {
    if credit_side {
        balance_cents.saturating_neg()
    } else {
        balance_cents
    }
}

/// Folds one side of the return from its two grouped reads — pure, so every
/// figure below is unit-tested without a database.
///
/// The totals are summed from the rows rather than from the reads: a total that
/// does not add up to the table under it is the one defect a report a return is
/// filed from must not have.
///
/// # Errors
/// [`StoreError::Validation`] when either read was truncated by
/// [`crate::fin_ledger::LEDGER_GROUPS_MAX`].
fn side(
    tax: &DimensionBalances,
    base: &DimensionBalances,
    credit_side: bool,
) -> Result<VatReturnSide> {
    if tax.truncated || base.truncated {
        return Err(StoreError::Validation(TOO_MANY_RATES.to_owned()));
    }

    let mut rates: BTreeMap<i32, VatReturnRate> = BTreeMap::new();
    let mut side = VatReturnSide::default();
    for row in &base.rows {
        let cents = natural_cents(credit_side, row.balance_cents);
        match row.vat_rate_bp() {
            Some(rate_bp) => {
                let rate = rates.entry(rate_bp).or_insert(VatReturnRate {
                    rate_bp,
                    base_cents: 0,
                    vat_cents: 0,
                });
                rate.base_cents = rate.base_cents.saturating_add(cents);
            }
            None => side.unrated_base_cents = side.unrated_base_cents.saturating_add(cents),
        }
    }
    for row in &tax.rows {
        let cents = natural_cents(credit_side, row.balance_cents);
        match row.vat_rate_bp() {
            Some(rate_bp) => {
                let rate = rates.entry(rate_bp).or_insert(VatReturnRate {
                    rate_bp,
                    base_cents: 0,
                    vat_cents: 0,
                });
                rate.vat_cents = rate.vat_cents.saturating_add(cents);
            }
            None => side.unrated_vat_cents = side.unrated_vat_cents.saturating_add(cents),
        }
    }

    side.rates = rates.into_values().collect();
    side.base_cents = total(&side.rates, |rate| rate.base_cents);
    side.vat_cents = total(&side.rates, |rate| rate.vat_cents);
    Ok(side)
}

/// One column of a side, added up. Saturating for [`natural_cents`]' reason —
/// the journal's own ceilings leave four orders of magnitude of headroom, and a
/// wrapped total is the one number a return must never carry.
fn total(rates: &[VatReturnRate], of: fn(&VatReturnRate) -> i64) -> i64 {
    rates
        .iter()
        .map(of)
        .fold(0_i64, |sum, cents| sum.saturating_add(cents))
}

impl AccountStore {
    /// **The VAT-return figures** for a period: output tax per rate, input tax
    /// per rate, and the net payable.
    ///
    /// Both bounds are inclusive and judged on each entry's accounting date —
    /// an invoice's issue date, a payment's `paid_on` — so re-running last
    /// quarter answers last quarter. Four folds over
    /// [`AccountStore::fin_dimension_balances`] and one read of the tenant's
    /// accounting currency: no query of its own, which is what keeps this
    /// return and the ledger it summarises incapable of disagreeing.
    ///
    /// It is the journal's answer to the same question `billing_vat_period`
    /// (B1.20) answers from the documents. The two are asserted equal on a
    /// seeded year in `tests/fin_vat_return.rs`, and they can only differ if
    /// something was billed and not booked, or booked and not billed — which is
    /// what that test is for.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the period ends before it starts, or
    /// when it states more rates than one read returns
    /// ([`crate::fin_ledger::LEDGER_GROUPS_MAX`]); [`StoreError::Db`] on failure.
    pub async fn fin_vat_return(&self, from: Date, to: Date) -> Result<VatReturn> {
        if to < from {
            return Err(StoreError::Validation(
                "the end of the period must not be before its start".to_owned(),
            ));
        }
        let currency = self.billing_base_currency().await?;
        let output_tax = self
            .rates_of(&LedgerScope::Role(AccountRole::VatOutput), from, to)
            .await?;
        let output_base = self
            .rates_of(&LedgerScope::Type(AccountType::Income), from, to)
            .await?;
        let input_tax = self
            .rates_of(&LedgerScope::Role(AccountRole::VatInput), from, to)
            .await?;
        let input_base = self
            .rates_of(&LedgerScope::Type(AccountType::Expense), from, to)
            .await?;
        let output = side(&output_tax, &output_base, true)?;
        let input = side(&input_tax, &input_base, false)?;
        Ok(VatReturn {
            from,
            to,
            currency,
            net_payable_cents: output.vat_cents.saturating_sub(input.vat_cents),
            output,
            input,
        })
    }

    /// One of the four reads: what a scope's accounts moved in the period, by
    /// VAT rate. Named once so the four cannot drift into four spellings of the
    /// same question.
    async fn rates_of(
        &self,
        scope: &LedgerScope,
        from: Date,
        to: Date,
    ) -> Result<DimensionBalances> {
        self.fin_dimension_balances(scope, LedgerDimension::VatRate, Some(from), Some(to))
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fin_ledger::DimensionBalance;

    /// A grouped read as the ledger hands one back: `(rate, balance)` pairs in
    /// the ledger's own sign, `None` for the postings that state no rate.
    fn read(rows: &[(Option<i32>, i64)]) -> DimensionBalances {
        DimensionBalances {
            rows: rows
                .iter()
                .map(|&(rate_bp, balance_cents)| DimensionBalance {
                    value: rate_bp.map(|rate| rate.to_string()),
                    debit_cents: balance_cents.max(0),
                    credit_cents: balance_cents.saturating_neg().max(0),
                    balance_cents,
                    postings: 1,
                })
                .collect(),
            truncated: false,
        }
    }

    fn nothing() -> DimensionBalances {
        read(&[])
    }

    /// A quarter of a small consultancy: €1 500 at 21 % and €250 at 9 % billed,
    /// €400 at 21 % bought. Written in the ledger's own signs — output credit,
    /// input debit — so the fold is what flips them.
    fn quarter() -> VatReturn {
        let output = side(
            &read(&[(Some(2100), -31_500), (Some(900), -2_250)]),
            &read(&[(Some(2100), -150_000), (Some(900), -25_000)]),
            true,
        )
        .unwrap_or_else(|e| panic!("{e:?}"));
        let input = side(
            &read(&[(Some(2100), 8_400)]),
            &read(&[(Some(2100), 40_000)]),
            false,
        )
        .unwrap_or_else(|e| panic!("{e:?}"));
        VatReturn {
            from: Date::MIN,
            to: Date::MAX,
            currency: "EUR".to_owned(),
            net_payable_cents: output.vat_cents.saturating_sub(input.vat_cents),
            output,
            input,
        }
    }

    #[test]
    fn output_tax_reads_positive_input_tax_reads_positive_and_the_net_is_the_difference() {
        let report = quarter();
        assert_eq!(
            report.output.rates,
            vec![
                VatReturnRate {
                    rate_bp: 900,
                    base_cents: 25_000,
                    vat_cents: 2_250,
                },
                VatReturnRate {
                    rate_bp: 2100,
                    base_cents: 150_000,
                    vat_cents: 31_500,
                },
            ],
            "ascending by rate, credit balances flipped once"
        );
        assert_eq!(report.output.base_cents, 175_000);
        assert_eq!(report.output.vat_cents, 33_750);
        assert_eq!(
            report.input.base_cents, 40_000,
            "a debit is already positive"
        );
        assert_eq!(report.input.vat_cents, 8_400);
        assert_eq!(
            report.net_payable_cents, 25_350,
            "33 750 charged, 8 400 paid"
        );

        // The totals are exactly the rows, which is what makes the return
        // checkable by hand.
        assert_eq!(
            report.output.base_cents,
            report
                .output
                .rates
                .iter()
                .map(|rate| rate.base_cents)
                .sum::<i64>()
        );
        assert_eq!(
            report.output.vat_cents,
            report
                .output
                .rates
                .iter()
                .map(|rate| rate.vat_cents)
                .sum::<i64>()
        );
    }

    #[test]
    fn a_refund_is_a_negative_net() {
        let output = side(&read(&[(Some(2100), -2_100)]), &nothing(), true)
            .unwrap_or_else(|e| panic!("{e:?}"));
        let input = side(&read(&[(Some(2100), 8_400)]), &nothing(), false)
            .unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(output.vat_cents.saturating_sub(input.vat_cents), -6_300);
    }

    #[test]
    fn a_rate_that_only_one_of_the_two_reads_used_is_still_a_row() {
        // Turnover at 0 % (an intra-community supply) charges no tax and is on
        // the return; and tax at a rate whose base was booked in another period
        // is on it too, at a base of zero, rather than absent.
        let side = side(
            &read(&[(Some(2100), -2_100)]),
            &read(&[(Some(0), -50_000)]),
            true,
        )
        .unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(
            side.rates,
            vec![
                VatReturnRate {
                    rate_bp: 0,
                    base_cents: 50_000,
                    vat_cents: 0,
                },
                VatReturnRate {
                    rate_bp: 2100,
                    base_cents: 0,
                    vat_cents: 2_100,
                },
            ]
        );
        assert_eq!(side.base_cents, 50_000);
        assert_eq!(side.vat_cents, 2_100);
    }

    #[test]
    fn a_rate_that_netted_out_is_reported_rather_than_dropped() {
        // An invoice and the credit note that took it back: the rate was used,
        // and it netted to zero. A return that omitted the row would say the
        // rate was never touched.
        let side = side(&read(&[(Some(2100), 0)]), &read(&[(Some(2100), 0)]), true)
            .unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(side.rates.len(), 1);
        assert_eq!(side.rates[0].rate_bp, 2100);
        assert_eq!(side.base_cents, 0);
    }

    #[test]
    fn turnover_that_states_no_rate_is_reported_apart_never_folded_into_a_rate() {
        let side = side(
            &read(&[(Some(2100), -2_100)]),
            &read(&[(Some(2100), -10_000), (None, -75_000)]),
            true,
        )
        .unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(side.rates.len(), 1, "one rate, not a second unnamed row");
        assert_eq!(
            side.base_cents, 10_000,
            "the return's base is the rated part"
        );
        assert_eq!(
            side.unrated_base_cents, 75_000,
            "and the rest is stated, because a base far below turnover is a fact the filer needs"
        );
        assert_eq!(side.unrated_vat_cents, 0);
    }

    #[test]
    fn tax_on_a_vat_account_with_no_rate_is_a_rule_with_a_bug_and_says_so() {
        let side = side(
            &read(&[(Some(2100), -2_100), (None, -500)]),
            &nothing(),
            true,
        )
        .unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(
            side.vat_cents, 2_100,
            "the return carries what it can attribute"
        );
        assert_eq!(side.unrated_vat_cents, 500);
    }

    #[test]
    fn a_period_that_moved_nothing_is_a_return_of_zeroes_not_an_absence() {
        let side = side(&nothing(), &nothing(), true).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(side, VatReturnSide::default());
        assert!(side.rates.is_empty());
        assert_eq!(side.base_cents, 0);
        assert_eq!(side.vat_cents, 0);
    }

    #[test]
    fn a_period_whose_rates_could_not_all_be_read_is_refused_never_half_summed() {
        let capped = DimensionBalances {
            truncated: true,
            ..read(&[(Some(2100), -2_100)])
        };
        for (tax, base) in [
            (capped.clone(), nothing()),
            (nothing(), capped.clone()),
            (capped.clone(), capped),
        ] {
            match side(&tax, &base, true) {
                Err(StoreError::Validation(message)) => {
                    assert_eq!(message, TOO_MANY_RATES);
                }
                other => panic!("expected Validation, got {other:?}"),
            }
        }
    }

    #[test]
    fn the_sign_is_flipped_in_exactly_one_place_and_cannot_wrap() {
        assert_eq!(natural_cents(true, -31_500), 31_500);
        assert_eq!(natural_cents(false, 8_400), 8_400);
        // A debit on an output VAT account (a correction) stays negative on the
        // return, which is what tax charged and taken back is.
        assert_eq!(natural_cents(true, 2_100), -2_100);
        assert_eq!(natural_cents(true, i64::MIN), i64::MAX);
    }

    #[test]
    fn an_absurd_period_saturates_rather_than_wrapping() {
        // Nothing the journal accepts can reach here: it would take more
        // postings than a tenant can write. The guarantee is that the sum is
        // total for any input, including a future caller's.
        let rows: Vec<(Option<i32>, i64)> = (0..64).map(|rate| (Some(rate), i64::MIN)).collect();
        let side = side(&read(&rows), &read(&rows), true).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(side.rates.len(), 64);
        assert_eq!(side.base_cents, i64::MAX, "saturated, not wrapped negative");
        assert_eq!(side.vat_cents, i64::MAX);
    }
}
