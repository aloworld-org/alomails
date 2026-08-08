//! Minutes into the quantity a billing line carries, and the money that
//! quantity is worth (alo Projects, ADR 0035, wave B3.06).
//!
//! [`crate::time_entries`] stores minutes because minutes are what a person
//! works; [`crate::billing_line`] counts in **milli-units** because a document
//! prices a quantity of something. This module is the one place the two meet,
//! and it is pure so the conversion can be property-tested rather than argued
//! about: the unbilled view, the invoice draft and the profitability report all
//! call it, which is what makes a figure on a report and a figure on the
//! printed invoice the same figure.
//!
//! # Where the rounding happens, and where it must not
//!
//! An hour is 60 minutes and a milli-hour is a thousandth of one, so a minute is
//! 16⅔ milli-hours and **not every duration is exactly expressible**. The
//! residue is dealt with in exactly one place: [`qty_milli_hours`] rounds half
//! away from zero, the convention [`crate::billing_totals`] already uses so a
//! credit note is the exact mirror of its original.
//!
//! The rule that matters is *when* it is called. The handoff sums a group's
//! **minutes** and converts **once** — never per entry — so a month of
//! six-minute stints is priced as the hours they add up to. Converting each
//! entry and summing the quantities would round two hundred times and bill a
//! client for the error. Every caller in this crate therefore passes a total.
//!
//! # No float, and no silent overflow
//!
//! The arithmetic is `i128` internally and narrows once, exactly as the totals
//! do. A group of 5000 entries at the 1440-minute ceiling is 120 000 hours —
//! 1.2 × 10⁸ milli-units, two orders of magnitude below
//! [`crate::billing_line::QTY_MAX_MILLI`] — so a real timesheet cannot reach the
//! bound, and a corrupted one saturates instead of wrapping.

use crate::billing_totals::{LineFigures, div_round_half_away, line_net_cents, to_i64};

/// Milli-units in one whole unit — an hour, here.
const MILLI: i128 = 1_000;

/// Minutes in an hour.
const MINUTES_PER_HOUR: i128 = 60;

/// The quantity, in milli-hours, that `minutes` of work becomes on a billing
/// line: 90 minutes is `1500`, an hour is `1000`.
///
/// Rounded half away from zero, **once**, on the total the caller passes. Pass a
/// group's summed minutes, never one entry at a time (see the module docs).
///
/// Pure and total: `i128` internally, saturating on the narrow, so no timesheet
/// can panic a release build.
#[must_use]
pub fn qty_milli_hours(minutes: i64) -> i64 {
    to_i64(div_round_half_away(
        i128::from(minutes) * MILLI,
        MINUTES_PER_HOUR,
    ))
}

/// What `minutes` of work at `rate_cents` an hour is worth, in integer cents.
///
/// Deliberately expressed as a billing line rather than as arithmetic of its
/// own: the value shown in the unbilled view and in the profitability report is
/// computed by [`crate::billing_totals::line_net_cents`] over the very figures
/// the invoice line will carry, so the three can never disagree by a cent.
///
/// VAT is not here. A line's tax is rounded at the rate subtotal, not per line
/// ([`crate::billing_totals::totals`]), so a per-group VAT figure would be a
/// number that does not appear on any document.
#[must_use]
pub fn hours_net_cents(minutes: i64, rate_cents: i64) -> i64 {
    line_net_cents(&LineFigures {
        qty_milli: qty_milli_hours(minutes),
        unit_price_cents: rate_cents,
        vat_rate_bp: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_durations_a_timesheet_is_full_of_are_exact() {
        assert_eq!(qty_milli_hours(0), 0);
        assert_eq!(qty_milli_hours(60), 1_000);
        assert_eq!(qty_milli_hours(90), 1_500);
        assert_eq!(qty_milli_hours(15), 250);
        assert_eq!(qty_milli_hours(6), 100);
        assert_eq!(qty_milli_hours(1_440), 24_000);
    }

    #[test]
    fn a_minute_that_is_not_expressible_rounds_once_and_says_so() {
        // 1 min = 16.67 milli-hours; 2 min = 33.3; 5 min = 83.3.
        assert_eq!(qty_milli_hours(1), 17);
        assert_eq!(qty_milli_hours(2), 33);
        assert_eq!(qty_milli_hours(5), 83);
        // …and the error never compounds, because a group converts its sum
        // once: ten one-minute stints are ten minutes, not ten roundings.
        assert_eq!(qty_milli_hours(10), 167);
        assert!(qty_milli_hours(10) < 10 * qty_milli_hours(1));
    }

    #[test]
    fn the_conversion_is_monotonic_and_within_half_a_milli_hour() {
        let mut previous = qty_milli_hours(0);
        for minutes in 1..=100_000_i64 {
            let qty = qty_milli_hours(minutes);
            assert!(qty >= previous, "{minutes} went backwards");
            previous = qty;
            // |qty × 60 − minutes × 1000| ≤ 30: at most half a milli-hour of
            // residue, whatever the duration.
            let residue = (i128::from(qty) * MINUTES_PER_HOUR - i128::from(minutes) * MILLI).abs();
            assert!(residue <= MINUTES_PER_HOUR / 2, "{minutes} drifted");
        }
    }

    #[test]
    fn every_multiple_of_three_minutes_is_exact() {
        // 3 minutes is a twentieth of an hour, so anything on that grid has no
        // residue at all — which is what makes ordinary timesheet durations
        // (quarter hours, halves, whole hours) exact.
        for step in 0..2_000_i64 {
            let minutes = step * 3;
            assert_eq!(
                i128::from(qty_milli_hours(minutes)) * MINUTES_PER_HOUR,
                i128::from(minutes) * MILLI,
                "{minutes} was not exact"
            );
        }
    }

    #[test]
    fn a_group_at_the_ceiling_stays_far_inside_the_line_bound() {
        // 5000 entries at the 1440-minute day ceiling — the most one handoff
        // may carry.
        let qty = qty_milli_hours(5_000 * 1_440);
        assert_eq!(qty, 120_000_000);
        assert!(qty < crate::billing_line::QTY_MAX_MILLI);
    }

    #[test]
    fn a_corrupted_duration_saturates_rather_than_wrapping() {
        assert_eq!(qty_milli_hours(i64::MAX), i64::MAX);
        assert_eq!(qty_milli_hours(i64::MIN), i64::MIN);
    }

    #[test]
    fn the_value_of_an_hour_is_the_line_the_invoice_will_carry() {
        // 90 minutes at €95.00 an hour is €142.50 — the same number the line
        // (1500 milli × 9500 cents) produces, because it is that line.
        assert_eq!(hours_net_cents(90, 9_500), 14_250);
        assert_eq!(hours_net_cents(60, 9_500), 9_500);
        assert_eq!(hours_net_cents(0, 9_500), 0);
        assert_eq!(hours_net_cents(90, 0), 0);
    }

    #[test]
    fn the_value_folds_the_group_once_exactly_as_the_line_does() {
        // The property that makes the report and the document agree: for any
        // duration and rate, the money is `line_net_cents` of the very figures
        // the line carries.
        for minutes in [1_i64, 7, 59, 61, 137, 1_440, 12_345] {
            for rate in [1_i64, 99, 9_500, 250_000] {
                assert_eq!(
                    hours_net_cents(minutes, rate),
                    line_net_cents(&LineFigures {
                        qty_milli: qty_milli_hours(minutes),
                        unit_price_cents: rate,
                        vat_rate_bp: 2_100,
                    }),
                    "{minutes} min at {rate}"
                );
            }
        }
    }
}
