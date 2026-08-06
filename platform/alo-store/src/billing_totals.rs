//! The money arithmetic of a billing document (alo Billing, ADR 0035, wave
//! B1): line nets, the VAT breakdown, and the gross.
//!
//! This module is **pure** — no database, no clock, no tenant. It is the only
//! place in alo where a document's totals are computed, and it is deliberately
//! separate from the store so the rounding convention can be property-tested
//! directly and so every surface (invoices, quotes, credit notes, the PDF, the
//! e-invoice XML) gets the same answer. The web layer never computes money; it
//! renders what the API returns (`docs/design/billing.md`).
//!
//! ## The convention
//!
//! ```text
//! line_net      = round(qty_milli × unit_price_cents / 1000)
//! net           = Σ line_net
//! vat[rate]     = round(Σ line_net where rate → × rate / 10 000)
//! gross         = net + Σ vat[rate]
//! ```
//!
//! Rounding happens **at the VAT-rate subtotal, not per line** — the VAT
//! directive / EN 16931 convention (BR-CO-17: the category tax amount is the
//! category taxable amount times the rate). Rounding per line and summing
//! gives a different, wrong, answer on documents with many small lines, which
//! is precisely the case a tax audit looks at.
//!
//! Rounding is **half away from zero**, not half up. On positive amounts the
//! two agree (the everyday `0.5 → 1`); they differ on negatives, and away-from-
//! zero is what keeps a credit note the exact mirror of the invoice it credits
//! — `totals(−lines) == −totals(lines)`, asserted as a property below. Half-up
//! would leave a one-cent residue on any document whose credit rounds at a
//! half, and a ledger that does not sum to zero is a real accounting defect.
//!
//! ## Why it cannot overflow
//!
//! Every intermediate is computed in `i128` and the inputs are bounded by
//! [`crate::billing_line`] (|qty| ≤ 10^9 milli-units, price ≤ 10^9 cents,
//! ≤ 500 lines per document), so a validated document's gross stays four
//! orders of magnitude below `i64::MAX`. The conversion back to `i64`
//! saturates rather than wrapping, so even a caller that ignores those bounds
//! gets an absurd number instead of a plausible wrong one — and never a panic.

/// The three numbers a line contributes to a document's totals, taken from a
/// stored line by [`crate::billing_line::Line::figures`] or built directly in
/// a test.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineFigures {
    /// Quantity in milli-units (1.5 h = 1500). May be negative: that is how a
    /// discount line is expressed.
    pub qty_milli: i64,
    /// Price of one unit in integer cents.
    pub unit_price_cents: i64,
    /// VAT rate in basis points (2100 = 21 %).
    pub vat_rate_bp: i32,
}

/// One row of the VAT breakdown a document must print: how much of the net was
/// taxed at this rate, and the tax on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VatSubtotal {
    /// The rate, in basis points.
    pub rate_bp: i32,
    /// Sum of the line nets taxed at this rate, in cents.
    pub net_cents: i64,
    /// The VAT on that net, in cents, rounded once at this subtotal.
    pub vat_cents: i64,
}

/// The computed totals of a document. Never stored as columns the client can
/// influence — always recomputed from the lines.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Totals {
    /// Sum of all line nets, in cents.
    pub net_cents: i64,
    /// Sum of the VAT over every rate, in cents.
    pub vat_cents: i64,
    /// `net_cents + vat_cents`, in cents — what the customer pays.
    pub gross_cents: i64,
    /// The breakdown by rate, ascending by rate, one row per rate present.
    pub vat_by_rate: Vec<VatSubtotal>,
}

/// The net of a single line, in cents: quantity times unit price, rounded once
/// (half away from zero) out of milli-units.
///
/// This is the only rounding that happens per line; VAT is rounded later, at
/// the rate subtotal.
pub fn line_net_cents(line: &LineFigures) -> i64 {
    let exact = i128::from(line.qty_milli) * i128::from(line.unit_price_cents);
    to_i64(div_round_half_away(exact, 1_000))
}

/// The totals of a document, from its lines in any order.
///
/// The result is independent of line order, and an empty document totals to
/// all zeros with an empty breakdown (a document with no lines is worth
/// nothing, which is a legitimate state for a draft).
pub fn totals(lines: &[LineFigures]) -> Totals {
    // Net per rate, exact in i128, keyed by rate. A document has a handful of
    // rates at most, so a sorted Vec beats a map and gives the printed order
    // for free.
    let mut by_rate: Vec<(i32, i128)> = Vec::new();
    let mut net: i128 = 0;
    for line in lines {
        let line_net = i128::from(line_net_cents(line));
        net += line_net;
        match by_rate.binary_search_by_key(&line.vat_rate_bp, |&(rate, _)| rate) {
            Ok(at) => by_rate[at].1 += line_net,
            Err(at) => by_rate.insert(at, (line.vat_rate_bp, line_net)),
        }
    }

    let mut vat_total: i128 = 0;
    let mut vat_by_rate = Vec::with_capacity(by_rate.len());
    for (rate_bp, rate_net) in by_rate {
        // One rounding per rate — the whole point of the convention.
        let vat = div_round_half_away(rate_net * i128::from(rate_bp), 10_000);
        vat_total += vat;
        vat_by_rate.push(VatSubtotal {
            rate_bp,
            net_cents: to_i64(rate_net),
            vat_cents: to_i64(vat),
        });
    }

    Totals {
        net_cents: to_i64(net),
        vat_cents: to_i64(vat_total),
        gross_cents: to_i64(net + vat_total),
        vat_by_rate,
    }
}

/// `numer / denom` rounded half **away from zero**, for a strictly positive
/// `denom`. Integer-only: no float ever touches an amount of money.
fn div_round_half_away(numer: i128, denom: i128) -> i128 {
    debug_assert!(denom > 0, "divisor must be positive");
    let quotient = numer / denom;
    let remainder = numer % denom;
    if remainder.abs() * 2 >= denom {
        // `remainder` carries the sign of `numer`, so this steps away from
        // zero in the right direction for both signs.
        quotient + if numer < 0 { -1 } else { 1 }
    } else {
        quotient
    }
}

/// Narrows an exact `i128` amount to the `i64` cents a caller sees.
///
/// A validated document cannot come near the boundary (see the module docs),
/// so saturating here is unreachable in practice — but it is total: no
/// wrapping into a plausible wrong number, and no panic.
fn to_i64(value: i128) -> i64 {
    i64::try_from(value).unwrap_or(if value < 0 { i64::MIN } else { i64::MAX })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A line at a rate, from a quantity in whole units and a price in cents.
    fn line(units: i64, price_cents: i64, rate_bp: i32) -> LineFigures {
        LineFigures {
            qty_milli: units * 1_000,
            unit_price_cents: price_cents,
            vat_rate_bp: rate_bp,
        }
    }

    /// A tiny deterministic generator, so the properties below are exercised
    /// over thousands of documents without adding a dependency and without a
    /// failure that cannot be reproduced. xorshift64*, seeded per test.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            self.0 = x;
            x.wrapping_mul(0x2545_F491_4F6C_DD1D)
        }

        /// A value in `0..=max`.
        fn upto(&mut self, max: u64) -> u64 {
            if max == u64::MAX {
                self.next()
            } else {
                self.next() % (max + 1)
            }
        }

        /// A document of 0–12 lines, every field inside the bounds
        /// `billing_line` enforces, biased towards the small quantities and
        /// awkward prices where rounding actually bites.
        fn document(&mut self) -> Vec<LineFigures> {
            let count = self.upto(12) as usize;
            (0..count)
                .map(|_| {
                    let magnitude = self.upto(1_000_000) as i64;
                    let negative = self.upto(4) == 0;
                    LineFigures {
                        qty_milli: if negative { -magnitude } else { magnitude },
                        unit_price_cents: self.upto(100_000) as i64,
                        // A handful of realistic European rates, so lines
                        // actually collide into shared subtotals.
                        vat_rate_bp: [0, 500, 600, 900, 1900, 2100, 2500][self.upto(6) as usize],
                    }
                })
                .collect()
        }
    }

    fn negated(lines: &[LineFigures]) -> Vec<LineFigures> {
        lines
            .iter()
            .map(|l| LineFigures {
                qty_milli: -l.qty_milli,
                ..*l
            })
            .collect()
    }

    // ---- the convention, pinned by hand-computed cases --------------------

    #[test]
    fn a_plain_document_totals_as_hand_computed() {
        // 10 hours at €120.00, 19 % → net 1200.00, VAT 228.00, gross 1428.00.
        let t = totals(&[line(10, 12_000, 1900)]);
        assert_eq!(t.net_cents, 120_000);
        assert_eq!(t.vat_cents, 22_800);
        assert_eq!(t.gross_cents, 142_800);
        assert_eq!(
            t.vat_by_rate,
            vec![VatSubtotal {
                rate_bp: 1900,
                net_cents: 120_000,
                vat_cents: 22_800,
            }]
        );
    }

    #[test]
    fn an_empty_document_is_all_zeros() {
        let t = totals(&[]);
        assert_eq!(t, Totals::default());
        assert!(t.vat_by_rate.is_empty());
    }

    #[test]
    fn a_line_net_rounds_half_away_from_zero_out_of_milli_units() {
        // 1.5 × 333 = 499.5 cents.
        let half = LineFigures {
            qty_milli: 1_500,
            unit_price_cents: 333,
            vat_rate_bp: 0,
        };
        assert_eq!(line_net_cents(&half), 500);
        // ... and its mirror rounds the other way, to −500, not −499.
        assert_eq!(
            line_net_cents(&LineFigures {
                qty_milli: -1_500,
                ..half
            }),
            -500
        );
        // Just under the half stays down.
        assert_eq!(
            line_net_cents(&LineFigures {
                qty_milli: 1_499,
                unit_price_cents: 333,
                vat_rate_bp: 0,
            }),
            499
        );
        // An exact product does not move at all.
        assert_eq!(line_net_cents(&line(3, 1_000, 0)), 3_000);
    }

    #[test]
    fn vat_rounds_half_away_from_zero_at_the_subtotal() {
        // Net 50 at 1 % = 0.5 cents → 1.
        let t = totals(&[line(1, 50, 100)]);
        assert_eq!(t.vat_cents, 1);
        // The credit note of the same line rounds to −1, so the pair nets out.
        let credited = totals(&[line(-1, 50, 100)]);
        assert_eq!(credited.vat_cents, -1);
        assert_eq!(t.gross_cents + credited.gross_cents, 0);
    }

    #[test]
    fn rounding_at_the_subtotal_is_not_rounding_per_line() {
        // Three lines of 3.33 at 21 %: per line the VAT would round to 1 cent
        // each (0.6993 → 1), giving 3. Rounded once on the 9.99 subtotal it is
        // 2.0979 → 2. The subtotal answer is the legally correct one.
        let lines = [line(1, 333, 2100), line(1, 333, 2100), line(1, 333, 2100)];
        let t = totals(&lines);
        assert_eq!(t.net_cents, 999);
        assert_eq!(t.vat_cents, 210, "0.21 × 9.99 = 2.0979 → 2.10");
        assert_eq!(t.gross_cents, 1_209);
    }

    #[test]
    fn each_rate_gets_its_own_subtotal_in_ascending_order() {
        // A mixed document: standard-rated consulting, reduced-rate books,
        // and a zero-rated intra-Community line.
        let t = totals(&[
            line(2, 5_000, 2100),
            line(3, 1_000, 600),
            line(1, 25_000, 0),
            line(1, 5_000, 2100),
        ]);
        let rates: Vec<i32> = t.vat_by_rate.iter().map(|s| s.rate_bp).collect();
        assert_eq!(rates, vec![0, 600, 2100]);
        assert_eq!(t.vat_by_rate[0].vat_cents, 0, "zero-rated pays no VAT");
        assert_eq!(t.vat_by_rate[1].net_cents, 3_000);
        assert_eq!(t.vat_by_rate[1].vat_cents, 180);
        assert_eq!(
            t.vat_by_rate[2].net_cents, 15_000,
            "both 21 % lines land in one subtotal"
        );
        assert_eq!(t.vat_by_rate[2].vat_cents, 3_150);
        assert_eq!(t.net_cents, 43_000);
        assert_eq!(t.vat_cents, 3_330);
        assert_eq!(t.gross_cents, 46_330);
    }

    #[test]
    fn a_discount_line_is_a_negative_quantity() {
        // 10 % off a €500 line, expressed as a negative quantity of a €50 item
        // — both taxed at the same rate, so one subtotal.
        let t = totals(&[line(1, 50_000, 2100), line(-1, 5_000, 2100)]);
        assert_eq!(t.net_cents, 45_000);
        assert_eq!(t.vat_cents, 9_450);
        assert_eq!(t.vat_by_rate.len(), 1);
    }

    // ---- properties, over generated documents ------------------------------

    #[test]
    fn property_lines_always_reconcile_to_the_totals() {
        let mut rng = Rng(0x5EED_B106_0000);
        for _ in 0..5_000 {
            let lines = rng.document();
            let t = totals(&lines);

            // Net is the sum of the line nets, and of the subtotal nets.
            let line_sum: i64 = lines.iter().map(line_net_cents).sum();
            assert_eq!(t.net_cents, line_sum, "{lines:?}");
            let subtotal_sum: i64 = t.vat_by_rate.iter().map(|s| s.net_cents).sum();
            assert_eq!(t.net_cents, subtotal_sum, "{lines:?}");

            // VAT is the sum of the per-rate VAT, each of which is the rate
            // applied once to its own net — recomputed here independently of
            // the implementation's accumulation.
            let vat_sum: i64 = t.vat_by_rate.iter().map(|s| s.vat_cents).sum();
            assert_eq!(t.vat_cents, vat_sum, "{lines:?}");
            for s in &t.vat_by_rate {
                let expected =
                    div_round_half_away(i128::from(s.net_cents) * i128::from(s.rate_bp), 10_000);
                assert_eq!(i128::from(s.vat_cents), expected, "{lines:?}");
            }

            // And the gross is exactly the two of them.
            assert_eq!(t.gross_cents, t.net_cents + t.vat_cents, "{lines:?}");
        }
    }

    #[test]
    fn property_every_rate_appears_once_ascending() {
        let mut rng = Rng(0x00B1_0601);
        for _ in 0..5_000 {
            let lines = rng.document();
            let t = totals(&lines);
            let rates: Vec<i32> = t.vat_by_rate.iter().map(|s| s.rate_bp).collect();
            let mut sorted = rates.clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(rates, sorted, "one row per rate, ascending: {lines:?}");
            let mut present: Vec<i32> = lines.iter().map(|l| l.vat_rate_bp).collect();
            present.sort_unstable();
            present.dedup();
            assert_eq!(rates, present, "exactly the rates the document uses");
        }
    }

    #[test]
    fn property_a_credit_note_is_the_exact_mirror() {
        let mut rng = Rng(0x00B1_0602);
        for _ in 0..5_000 {
            let lines = rng.document();
            let t = totals(&lines);
            let mirror = totals(&negated(&lines));
            assert_eq!(mirror.net_cents, -t.net_cents, "{lines:?}");
            assert_eq!(mirror.vat_cents, -t.vat_cents, "{lines:?}");
            assert_eq!(mirror.gross_cents, -t.gross_cents, "{lines:?}");
            // The ledger of the two documents sums to zero, to the cent.
            assert_eq!(t.gross_cents + mirror.gross_cents, 0, "{lines:?}");
        }
    }

    #[test]
    fn property_totals_do_not_depend_on_line_order() {
        let mut rng = Rng(0x00B1_0603);
        for _ in 0..2_000 {
            let lines = rng.document();
            let mut shuffled = lines.clone();
            // Fisher-Yates with the same generator.
            for i in (1..shuffled.len()).rev() {
                let j = rng.upto(i as u64) as usize;
                shuffled.swap(i, j);
            }
            assert_eq!(totals(&lines), totals(&shuffled), "{lines:?}");
        }
    }

    #[test]
    fn property_a_zero_rate_never_produces_vat() {
        let mut rng = Rng(0x00B1_0604);
        for _ in 0..2_000 {
            let lines: Vec<LineFigures> = rng
                .document()
                .into_iter()
                .map(|l| LineFigures {
                    vat_rate_bp: 0,
                    ..l
                })
                .collect();
            let t = totals(&lines);
            assert_eq!(t.vat_cents, 0);
            assert_eq!(t.gross_cents, t.net_cents);
            assert!(t.vat_by_rate.iter().all(|s| s.vat_cents == 0));
        }
    }

    // ---- the arithmetic cannot wrap ---------------------------------------

    #[test]
    fn a_document_at_every_validated_bound_stays_far_inside_i64() {
        // The worst case the validation in `billing_line` permits: 500 lines,
        // each a million units of a ten-million-euro item at 100 % VAT.
        let worst = vec![
            LineFigures {
                qty_milli: 1_000_000_000,
                unit_price_cents: 1_000_000_000,
                vat_rate_bp: 10_000,
            };
            500
        ];
        let t = totals(&worst);
        assert_eq!(t.net_cents, 500_000_000_000_000_000);
        assert_eq!(t.vat_cents, 500_000_000_000_000_000);
        assert_eq!(t.gross_cents, 1_000_000_000_000_000_000);
        assert!(
            t.gross_cents < i64::MAX / 9,
            "an order of magnitude of headroom is left"
        );
    }

    #[test]
    fn absurd_input_saturates_rather_than_wrapping_or_panicking() {
        // Nothing the store accepts can reach here — this is the guarantee
        // that the pure function is total for any caller, including a future
        // one that forgets to validate first.
        let extreme = vec![
            LineFigures {
                qty_milli: i64::MAX,
                unit_price_cents: i64::MAX,
                vat_rate_bp: 10_000,
            };
            8
        ];
        let t = totals(&extreme);
        assert_eq!(t.net_cents, i64::MAX, "saturated, not wrapped negative");
        assert_eq!(t.gross_cents, i64::MAX);
        let mirror = totals(&negated(&extreme));
        assert_eq!(mirror.net_cents, i64::MIN);
    }

    #[test]
    fn the_rounding_helper_is_symmetric_around_zero() {
        for (numer, denom, expected) in [
            (0_i128, 1000_i128, 0_i128),
            (499, 1000, 0),
            (500, 1000, 1),
            (501, 1000, 1),
            (1500, 1000, 2),
            (-499, 1000, 0),
            (-500, 1000, -1),
            (-1500, 1000, -2),
        ] {
            assert_eq!(
                div_round_half_away(numer, denom),
                expected,
                "{numer}/{denom}"
            );
            assert_eq!(
                div_round_half_away(-numer, denom),
                -expected,
                "mirror of {numer}/{denom}"
            );
        }
    }
}
